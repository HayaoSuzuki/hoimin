//! Requested-byte changes on the measured thread, excluding libtest activity.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

pub struct TrackingAllocator;

#[derive(Clone, Copy)]
struct Window {
    live: i128,
    peak: usize,
}

thread_local! {
    static WINDOW: Cell<Option<Window>> = const { Cell::new(None) };
}

fn changed(bytes: i128) {
    // The meter is synchronous. Other test threads and libtest may allocate or
    // free their own objects while this thread observes the core transitions.
    // A const Cell requires no allocator work and try_with tolerates TLS teardown.
    let _ = WINDOW.try_with(|slot| {
        if let Some(mut window) = slot.get() {
            window.live += bytes;
            window.peak = window
                .peak
                .max(usize::try_from(window.live).unwrap_or_default());
            slot.set(Some(window));
        }
    });
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            changed(layout.size() as i128);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            changed(layout.size() as i128);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        changed(-(layout.size() as i128));
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, old, new_size) };
        if !replacement.is_null() {
            changed(new_size as i128 - old.size() as i128);
        }
        replacement
    }
}

#[derive(Debug)]
pub struct Measurement {
    pub retained_delta: i128,
    pub peak_delta: usize,
}

pub struct Baseline;

pub fn begin() -> Baseline {
    WINDOW.with(|slot| {
        assert!(slot.get().is_none());
        slot.set(Some(Window { live: 0, peak: 0 }));
    });
    Baseline
}

pub fn finish(_baseline: Baseline) -> Measurement {
    let window = WINDOW.with(|slot| slot.take().expect("measurement was started"));
    Measurement {
        retained_delta: window.live,
        peak_delta: window.peak,
    }
}

pub fn self_check() {
    let baseline = begin();
    unsafe {
        let small = Layout::from_size_align(16, 8).unwrap();
        let large = Layout::from_size_align(32, 8).unwrap();
        let tiny = Layout::from_size_align(8, 8).unwrap();
        let pointer = TrackingAllocator.alloc(small);
        assert!(!pointer.is_null());
        let pointer = TrackingAllocator.realloc(pointer, small, large.size());
        assert!(!pointer.is_null());
        let pointer = TrackingAllocator.realloc(pointer, large, tiny.size());
        assert!(!pointer.is_null());
        TrackingAllocator.dealloc(pointer, tiny);
    }
    let measured = finish(baseline);
    assert_eq!(measured.retained_delta, 0);
    assert_eq!(measured.peak_delta, 32);
}
