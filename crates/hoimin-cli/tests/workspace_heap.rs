use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

struct TrackingAllocator;

static LIVE: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            if new_size >= layout.size() {
                LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new_pointer
    }
}

#[test]
fn retained_heap_does_not_scale_with_workspace_bytes_times_workers() {
    const FILES: usize = 8;
    const FILE_BYTES: usize = 512 * 1024;
    const WORKERS: u32 = 4;
    const RETAINED_LIMIT: usize = 2 * 1024 * 1024;

    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    fs::create_dir(&source).unwrap();
    let contents = vec![b'x'; FILE_BYTES];
    for index in 0..FILES {
        fs::write(source.join(format!("large-{index}.py")), &contents).unwrap();
    }
    drop(contents);

    let root = Utf8Path::from_path(project.path()).unwrap();
    let plan =
        WorkspacePlan::preflight(root, EffectId(1), WORKERS, CopyOptions::default()).unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    let baseline = LIVE.load(Ordering::Relaxed);

    let workers = (0..WORKERS)
        .map(|worker| {
            let request = grant.create_worker(EffectId(2), worker).unwrap();
            plan.create_worker(&request).unwrap()
        })
        .collect::<Vec<_>>();

    let retained = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
    assert_eq!(workers.len(), WORKERS as usize);
    assert!(
        retained <= RETAINED_LIMIT,
        "workers retained {retained} heap bytes for a {} byte fixture",
        FILES * FILE_BYTES
    );
}
