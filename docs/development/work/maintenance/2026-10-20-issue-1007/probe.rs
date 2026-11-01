//! Fixed-workload requested-heap probe. Setup and output are outside observation windows.
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

struct Meter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static OBSERVING: AtomicBool = AtomicBool::new(false);
#[global_allocator]
static ALLOCATOR: Meter = Meter;

fn allocated(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    if OBSERVING.load(Ordering::Relaxed) {
        PEAK.fetch_max(live, Ordering::Relaxed);
    }
}
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() {
            if size >= layout.size() {
                allocated(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        replacement
    }
}
fn begin() -> usize {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    OBSERVING.store(true, Ordering::Relaxed);
    baseline
}
fn finish(baseline: usize) -> (usize, usize) {
    OBSERVING.store(false, Ordering::Relaxed);
    (
        LIVE.load(Ordering::Relaxed).saturating_sub(baseline),
        PEAK.load(Ordering::Relaxed).saturating_sub(baseline),
    )
}
fn check_meter() {
    let baseline = begin();
    let mut bytes = Vec::<u8>::with_capacity(4096);
    std::hint::black_box(&mut bytes);
    assert_eq!(LIVE.load(Ordering::Relaxed) - baseline, 4096);
    bytes.reserve_exact(8192);
    std::hint::black_box(&mut bytes);
    assert_eq!(LIVE.load(Ordering::Relaxed) - baseline, 8192);
    bytes.shrink_to(4096);
    std::hint::black_box(&mut bytes);
    assert_eq!(LIVE.load(Ordering::Relaxed) - baseline, 4096);
    drop(bytes);
    let (live, peak) = finish(baseline);
    assert_eq!(live, 0);
    assert_eq!(peak, 8192);
}

pub fn measure(mode: &str, files: usize, handles: u32, payload_bytes: usize) -> (usize, usize) {
    check_meter();
    assert!(matches!(mode, "clone" | "worker"));
    let project = tempfile::tempdir().unwrap();
    let payload = vec![b'x'; payload_bytes];
    for index in 0..files {
        let directory = project.path().join(format!("package{:04}", index % 40));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("source_{index:04}_with_long_descriptive_name.py")),
            &payload,
        )
        .unwrap();
    }
    let root = Utf8Path::from_path(project.path()).unwrap();
    let plan =
        WorkspacePlan::preflight(root, EffectId(1), handles, CopyOptions::default()).unwrap();
    assert_eq!(plan.manifest().entries().len(), files);
    assert_eq!(plan.manifest().directories().len(), 40.min(files));
    assert_eq!(
        plan.manifest().logical_bytes(),
        (files * payload_bytes) as u64
    );
    let mut digest = blake3::Hasher::new();
    for entry in plan.manifest().entries() {
        digest.update(entry.path.as_str().as_bytes());
        digest.update(&entry.size.to_le_bytes());
        digest.update(entry.blake3.as_bytes());
    }
    let digest = digest.finalize();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    let requests = (0..handles)
        .map(|worker| grant.create_worker(EffectId(2), worker).unwrap())
        .collect::<Vec<_>>();
    let mut clones = Vec::with_capacity(handles as usize);
    let mut workers = Vec::with_capacity(handles as usize);
    let baseline = begin();
    let started = Instant::now();
    if mode == "clone" {
        for _ in 0..handles {
            clones.push(plan.manifest().clone());
        }
    } else {
        for request in &requests {
            workers.push(plan.create_worker(request).unwrap());
        }
    }
    let elapsed = started.elapsed();
    // Handles are still live here. This is retained allocation, not just a peak.
    let (retained, peak) = finish(baseline);
    let expected_charge = if mode == "worker" {
        (files * payload_bytes) as u64 * u64::from(handles)
    } else {
        0
    };
    assert_eq!(plan.observed_copy_bytes(), expected_charge);
    for manifest in &clones {
        assert_eq!(manifest, plan.manifest());
    }
    for worker in &workers {
        assert_eq!(worker.manifest(), plan.manifest());
        for entry in worker.manifest().entries() {
            assert_eq!(worker.read(entry.path.as_str()).unwrap(), payload);
        }
    }
    // Each worker releases its independent filesystem/slot/charge sequentially.
    while workers.pop().is_some() {}
    drop(clones);
    assert_eq!(plan.observed_copy_bytes(), 0);
    assert_eq!(plan.materialized_workers(), 0);
    println!(
        "{{\"mode\":\"{mode}\",\"files\":{files},\"handles\":{handles},\"payload_bytes\":{payload_bytes},\"retained\":{retained},\"peak\":{peak},\"elapsed_ns\":{},\"digest\":\"{digest}\",\"logical_bytes\":{},\"worker_charge\":{expected_charge},\"snapshot_plus_worker_payload_bytes\":{}}}",
        elapsed.as_nanos(),
        files * payload_bytes,
        files
            * payload_bytes
            * (1 + if mode == "worker" {
                handles as usize
            } else {
                0
            })
    );
    (retained, peak)
}

#[cfg(not(test))]
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 5, "probe MODE FILES HANDLES PAYLOAD_BYTES");
    measure(
        &args[1],
        args[2].parse().unwrap(),
        args[3].parse().unwrap(),
        args[4].parse().unwrap(),
    );
}
