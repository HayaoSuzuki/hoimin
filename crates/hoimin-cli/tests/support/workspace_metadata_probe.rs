//! Fixed-workload requested-heap probe. Setup and output are outside observation windows.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use std::time::Instant;

use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

struct Meter;
// The measured operations run synchronously on the calling thread. Rust's test
// harness may allocate concurrently; those allocations do not belong to the probe.
// A signed balance also permits allocations transferred between threads outside
// the observation window without underflowing the per-thread counter.
thread_local! {
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}
#[global_allocator]
static ALLOCATOR: Meter = Meter;

fn allocated(bytes: usize) {
    let live = LIVE.get() + isize::try_from(bytes).expect("allocation size fits isize");
    LIVE.set(live);
    if OBSERVING.get() {
        PEAK.set(PEAK.get().max(live));
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
        LIVE.set(LIVE.get() - isize::try_from(layout.size()).expect("allocation size fits isize"));
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() {
            if size >= layout.size() {
                allocated(size - layout.size());
            } else {
                LIVE.set(
                    LIVE.get()
                        - isize::try_from(layout.size() - size)
                            .expect("allocation size fits isize"),
                );
            }
        }
        replacement
    }
}
fn begin() -> isize {
    let baseline = LIVE.get();
    PEAK.set(baseline);
    OBSERVING.set(true);
    baseline
}
fn finish(baseline: isize) -> (usize, usize) {
    OBSERVING.set(false);
    (
        usize::try_from(LIVE.get() - baseline).unwrap_or(0),
        usize::try_from(PEAK.get() - baseline).unwrap_or(0),
    )
}
fn check_meter() {
    let baseline = begin();
    let mut bytes = Vec::<u8>::with_capacity(4096);
    std::hint::black_box(&mut bytes);
    assert_eq!(LIVE.get() - baseline, 4096);
    bytes.reserve_exact(8192);
    std::hint::black_box(&mut bytes);
    assert_eq!(LIVE.get() - baseline, 8192);
    bytes.shrink_to(4096);
    std::hint::black_box(&mut bytes);
    assert_eq!(LIVE.get() - baseline, 4096);
    drop(bytes);
    let (live, peak) = finish(baseline);
    assert_eq!(live, 0);
    assert_eq!(peak, 8192);
}

#[test]
fn meter_excludes_allocations_from_other_threads() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let phase = Arc::new(AtomicUsize::new(0));
    let other_phase = Arc::clone(&phase);
    let other = std::thread::spawn(move || {
        while other_phase.load(Ordering::Acquire) != 1 {
            std::hint::spin_loop();
        }
        let bytes = vec![0_u8; 8192];
        std::hint::black_box(&bytes);
        other_phase.store(2, Ordering::Release);
        while other_phase.load(Ordering::Acquire) != 3 {
            std::hint::spin_loop();
        }
        drop(bytes);
    });
    let baseline = begin();
    let bytes = vec![0_u8; 4096];
    std::hint::black_box(&bytes);
    phase.store(1, Ordering::Release);
    while phase.load(Ordering::Acquire) != 2 {
        std::hint::spin_loop();
    }
    let measured = finish(baseline);
    phase.store(3, Ordering::Release);
    other.join().unwrap();
    drop(bytes);
    assert_eq!(measured, (4096, 4096));
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
