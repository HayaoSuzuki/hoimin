//! Requested Rust heap bytes during the public pure exception-index build.
use std::{alloc::{GlobalAlloc, Layout, System}, sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed}, time::Instant};
use camino::Utf8PathBuf;
struct Meter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
fn allocated(old: usize, new: usize) {
    let live = if new >= old { LIVE.fetch_add(new-old, Relaxed) + new-old }
               else { LIVE.fetch_sub(old-new, Relaxed) - (old-new) };
    PEAK.fetch_max(live, Relaxed);
    if ACTIVE.load(Relaxed) { CALLS.fetch_add(1, Relaxed); BYTES.fetch_add(new, Relaxed); }
}
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() { allocated(0, layout.size()); } p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() { allocated(0, layout.size()); } p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, layout, size) };
        if !p.is_null() { allocated(layout.size(), size); } p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }; LIVE.fetch_sub(layout.size(), Relaxed);
    }
}
#[global_allocator] static ALLOC: Meter = Meter;
fn sanity() {
    let before = LIVE.load(Relaxed);
    unsafe {
        let layout = Layout::from_size_align(64, 8).unwrap();
        let p = ALLOC.alloc_zeroed(layout);
        assert!(!p.is_null()); assert_eq!(LIVE.load(Relaxed), before+64);
        let p = ALLOC.realloc(p, layout, 128);
        assert!(!p.is_null()); assert_eq!(LIVE.load(Relaxed), before+128);
        ALLOC.dealloc(p, Layout::from_size_align(128, 8).unwrap());
    }
    assert_eq!(LIVE.load(Relaxed), before);
}
fn main() {
    sanity();
    let root = std::env::args().nth(1).expect("fixture directory");
    let mut paths = std::fs::read_dir(&root).unwrap().map(|p|p.unwrap().path())
        .filter(|p|p.extension().is_some_and(|e|e=="py")).collect::<Vec<_>>();
    paths.sort();
    let sources = paths.iter().map(|p|(Utf8PathBuf::from(p.file_name().unwrap().to_str().unwrap()), std::fs::read_to_string(p).unwrap())).collect::<Vec<_>>();
    let roots = [Utf8PathBuf::from(".")];
    let initial = LIVE.load(Relaxed); PEAK.store(initial, Relaxed);
    CALLS.store(0, Relaxed); BYTES.store(0, Relaxed); ACTIVE.store(true, Relaxed);
    let start = Instant::now();
    let index = hoimin_python::ExceptionIndex::from_sources_with_inputs(&sources, &roots, &[], &||false).unwrap();
    let elapsed_ns = start.elapsed().as_nanos();
    ACTIVE.store(false, Relaxed);
    let peak = PEAK.load(Relaxed); let retained = LIVE.load(Relaxed);
    let calls = CALLS.load(Relaxed); let bytes = BYTES.load(Relaxed);
    std::hint::black_box(&index); drop(index);
    let released = LIVE.load(Relaxed);
    println!("{{\"elapsed_ns\":{elapsed_ns},\"initial_live\":{initial},\"peak_live\":{peak},\"peak_above_input\":{},\"retained_live\":{retained},\"after_drop_live\":{released},\"allocation_calls\":{calls},\"cumulative_requested_bytes\":{bytes}}}", peak-initial);
}
