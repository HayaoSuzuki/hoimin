//! Single-process requested-byte meter for the two standalone release probes.
//! Realloc contributes its full successful new request to cumulative bytes.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct Meter;

#[global_allocator]
static ALLOCATOR: Meter = Meter;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

fn requested(bytes: usize, live: usize) {
    if ACTIVE.load(Ordering::Relaxed) {
        REQUESTED.fetch_add(bytes, Ordering::Relaxed);
        CALLS.fetch_add(1, Ordering::Relaxed);
        PEAK.fetch_max(live, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            requested(layout.size(), live);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            requested(layout.size(), live);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, old, new_size) };
        if !replacement.is_null() {
            let live = if new_size >= old.size() {
                LIVE.fetch_add(new_size - old.size(), Ordering::Relaxed) + new_size - old.size()
            } else {
                LIVE.fetch_sub(old.size() - new_size, Ordering::Relaxed) - old.size() + new_size
            };
            requested(new_size, live);
        }
        replacement
    }
}

pub struct Baseline(usize);

#[derive(Debug)]
pub struct Measurement {
    pub cumulative_requested_bytes: usize,
    pub allocation_calls: usize,
    pub live_before: usize,
    pub live_after: usize,
    pub retained_delta: i128,
    pub peak_delta: usize,
}

pub fn begin() -> Baseline {
    assert!(!ACTIVE.load(Ordering::Relaxed));
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    REQUESTED.store(0, Ordering::Relaxed);
    CALLS.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    Baseline(baseline)
}

pub fn finish(baseline: Baseline) -> Measurement {
    ACTIVE.store(false, Ordering::Relaxed);
    let live_after = LIVE.load(Ordering::Relaxed);
    Measurement {
        cumulative_requested_bytes: REQUESTED.load(Ordering::Relaxed),
        allocation_calls: CALLS.load(Ordering::Relaxed),
        live_before: baseline.0,
        live_after,
        retained_delta: live_after as i128 - baseline.0 as i128,
        peak_delta: PEAK.load(Ordering::Relaxed).saturating_sub(baseline.0),
    }
}

/// Known raw requests exercise allocation, growth, shrinkage and release.
pub fn self_check() {
    let before = begin();
    unsafe {
        let small = Layout::from_size_align(16, 8).unwrap();
        let large = Layout::from_size_align(32, 8).unwrap();
        let tiny = Layout::from_size_align(8, 8).unwrap();
        let pointer = Meter.alloc(small);
        assert!(!pointer.is_null());
        let pointer = Meter.realloc(pointer, small, large.size());
        assert!(!pointer.is_null());
        let pointer = Meter.realloc(pointer, large, tiny.size());
        assert!(!pointer.is_null());
        Meter.dealloc(pointer, tiny);
    }
    let measured = finish(before);
    assert_eq!(measured.cumulative_requested_bytes, 56);
    assert_eq!(measured.allocation_calls, 3);
    assert_eq!(measured.retained_delta, 0);
    assert_eq!(measured.peak_delta, 32);
}
