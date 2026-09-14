use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::EffectId;

struct TrackingAllocator;

static PEAK: AtomicUsize = AtomicUsize::new(0);

static LIVE: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            if new_size >= layout.size() {
                let live = LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size
                    - layout.size();
                PEAK.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new_pointer
    }
}

// A separate integration binary keeps unrelated parallel tests out of global
// allocator observations. Only preflight lies inside the measured interval.
fn observe(files: usize, file_bytes: usize, workers: u32) -> usize {
    observe_with_eager_read(files, file_bytes, workers, false)
}

fn observe_with_eager_read(files: usize, file_bytes: usize, workers: u32, broken: bool) -> usize {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    fs::create_dir(&source).unwrap();
    let contents = vec![b'x'; file_bytes];
    for index in 0..files {
        fs::write(source.join(format!("file-{index:04}.py")), &contents).unwrap();
    }
    drop(contents);
    let root = Utf8Path::from_path(project.path()).unwrap();
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    // A real eager-read variant proves this allocator observation catches a
    // preflight that temporarily buffers complete worker copies.
    let eager = broken.then(|| {
        (0..workers)
            .flat_map(|_| 0..files)
            .map(|index| fs::read(source.join(format!("file-{index:04}.py"))).unwrap())
            .collect::<Vec<_>>()
    });
    let result = WorkspacePlan::preflight(root, EffectId(1), workers, CopyOptions::default());
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    drop(eager);
    if workers == 0 {
        assert!(result.is_err(), "zero workers must remain invalid");
    } else {
        let plan = result.unwrap();
        let per_worker = u64::try_from(files * file_bytes).unwrap();
        assert_eq!(plan.completed().per_worker_logical_bytes, per_worker);
        assert_eq!(plan.aggregate_bytes(), per_worker * u64::from(workers));
        assert_eq!(
            plan.observed_copy_bytes(),
            0,
            "preflight does not copy workers"
        );
    }
    peak
}

#[test]
fn preflight_peak_tracks_entries_and_largest_file_not_worker_copies() {
    // Warm allocator/runtime and filesystem setup before comparisons.
    observe(1, 1, 1);
    observe(0, 0, 0);
    observe(0, 0, 1);
    observe(1, 0, 1);
    let small = observe(8, 64 * 1024, 1);
    let broken = observe_with_eager_read(8, 1024 * 1024, 4, true);
    assert!(
        broken > small + 1024 * 1024 + 256 * 1024,
        "eager-read regression must be detected"
    );
    for scale in [1, 2, 4] {
        let files = 8 * scale;
        let file_peak = observe(files, 64 * 1024, 1);
        assert!(
            file_peak <= small * scale + 256 * 1024,
            "manifest entry growth: {files} files peak {file_peak}, baseline {small}"
        );
        let byte_peak = observe(8, scale * 1024 * 1024, 1);
        assert!(
            byte_peak <= small + scale * 1024 * 1024 + 256 * 1024,
            "file bytes growth: scale {scale} peak {byte_peak}, baseline {small}"
        );
        let worker_peak = observe(8, 64 * 1024, u32::try_from(scale).unwrap());
        eprintln!(
            "scale={scale} preflight_peak_bytes files={file_peak} bytes={byte_peak} workers={worker_peak} baseline={small}"
        );
        assert!(
            worker_peak <= small + 256 * 1024,
            "worker growth: scale {scale} peak {worker_peak}, baseline {small}"
        );
    }
}
