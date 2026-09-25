use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::time::Instant;

use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

const BLOCK: usize = 64 * 1024;
const PEAK_LIMIT: usize = 512 * 1024;

fn write_fixture(path: &std::path::Path, bytes: usize) {
    let mut file = File::create(path).unwrap();
    let block = vec![0xa5; BLOCK];
    for start in (0..bytes).step_by(BLOCK) {
        file.write_all(&block[..(bytes - start).min(BLOCK)])
            .unwrap();
    }
}

fn verify(path: &std::path::Path, bytes: usize) {
    let mut file = File::open(path).unwrap();
    let mut block = vec![0; BLOCK];
    let mut count = 0;
    loop {
        let read = file.read(&mut block).unwrap();
        if read == 0 {
            break;
        }
        assert!(block[..read].iter().all(|byte| *byte == 0xa5));
        count += read;
    }
    assert_eq!(count, bytes);
}

fn measure<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    heap_tracking::begin();
    let value = operation();
    (value, heap_tracking::finish())
}

fn observe(bytes: usize) -> [usize; 4] {
    let project = tempfile::tempdir().unwrap();
    write_fixture(&project.path().join("data.bin"), bytes);
    let root = Utf8Path::from_path(project.path()).unwrap();
    let (plan, preflight) =
        measure(|| WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap());
    assert_eq!(plan.aggregate_bytes(), bytes as u64);
    assert_eq!(plan.completed().per_worker_logical_bytes, bytes as u64);
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    let request = grant.create_worker(EffectId(2), 0).unwrap();
    let (mut worker, create) = measure(|| plan.create_worker(&request).unwrap());
    assert_eq!(plan.observed_copy_bytes(), bytes as u64);
    let path = worker.root().join("data.bin");
    verify(path.as_std_path(), bytes);
    let ((), unchanged) = measure(|| worker.reset().unwrap());
    verify(path.as_std_path(), bytes);
    let mut file = File::options().write(true).open(&path).unwrap();
    file.seek(SeekFrom::End(-1)).unwrap();
    file.write_all(&[0x5a]).unwrap();
    drop(file);
    let ((), changed) = measure(|| worker.reset().unwrap());
    verify(path.as_std_path(), bytes);
    verify(&project.path().join("data.bin"), bytes);
    [preflight, create, unchanged, changed]
}

#[test]
fn workspace_content_buffers_do_not_scale_with_file_bytes() {
    observe(1);
    let mut observations = Vec::new();
    for mib in [1, 8, 32] {
        for repeat in 0..3 {
            let started = Instant::now();
            let peaks = observe(mib * 1024 * 1024);
            eprintln!(
                "mib={mib} repeat={repeat} stage_peaks={peaks:?} elapsed_ms={}",
                started.elapsed().as_millis()
            );
            observations.push(peaks);
        }
    }
    let control = tempfile::tempdir().unwrap();
    let path = control.path().join("control.bin");
    write_fixture(&path, 1024 * 1024);
    let (eager, eager_peak) = measure(|| fs::read(path).unwrap());
    assert_eq!(eager.len(), 1024 * 1024);
    assert!(
        eager_peak > PEAK_LIMIT,
        "the eager-read control must fail the bound"
    );
    for peaks in observations {
        for (stage, peak) in peaks.into_iter().enumerate() {
            assert!(
                peak <= PEAK_LIMIT,
                "stage {stage} used {peak} bytes; limit {PEAK_LIMIT}"
            );
        }
    }
}
