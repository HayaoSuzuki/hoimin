use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::{Duration, Instant};

use super::model::*;
use super::store::{ReviewStore, STORE_WORK, StoreWork};
use super::tests::{draft, key};

// Track real Rust allocations on the measuring thread. SQLite's C heap is not
// included; this measures the object graph that the store materializes in Rust.
struct MeasuringAllocator;

thread_local! {
    static ALLOCATIONS: Cell<Option<(i128, i128)>> = const { Cell::new(None) };
}

fn allocation_change(bytes: i128) {
    let _ = ALLOCATIONS.try_with(|counter| {
        if let Some((live, peak)) = counter.get() {
            let live = live + bytes;
            counter.set(Some((live, peak.max(live))));
        }
    });
}

unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the allocator contract and the unchanged layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocation_change(layout.size() as i128);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        allocation_change(-(layout.size() as i128));
        // SAFETY: Forward the caller's pointer and its original layout.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: Forward the caller's valid allocation and requested size.
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            allocation_change(size as i128 - layout.size() as i128);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;

#[derive(Debug)]
struct Measurement {
    work: StoreWork,
    peak_rust_bytes: i128,
    elapsed: Duration,
}

fn measure<T>(operation: impl FnOnce() -> T) -> (T, Measurement) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|counter| counter.set(None));
        }
    }
    STORE_WORK.with_borrow_mut(|work| *work = StoreWork::default());
    ALLOCATIONS.with(|counter| counter.set(Some((0, 0))));
    let reset = Reset;
    let started = Instant::now();
    let result = operation();
    let elapsed = started.elapsed();
    let peak_rust_bytes = ALLOCATIONS.with(|counter| counter.get().unwrap().1);
    drop(reset);
    let work = STORE_WORK.with_borrow(|work| *work);
    (
        result,
        Measurement {
            work,
            peak_rust_bytes,
            elapsed,
        },
    )
}

fn distinct_key(index: usize) -> ReviewKey {
    let mut value = key();
    value.descriptor.replacement = format!("{}", index + 10);
    value.candidate_id =
        hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(&value.descriptor))
            .to_string();
    value
}

// The fixture uses one transaction so the measurement includes only the real
// operation under test, not repeated setup validation or fixture allocations.
fn populate(path: &std::path::Path, target: &ReviewKey, unrelated: usize, history: usize) {
    drop(ReviewStore::open_write(path, &target.project).unwrap());
    let mut connection = rusqlite::Connection::open(path).unwrap();
    let transaction = connection.transaction().unwrap();
    let mut annotation = draft();
    annotation.reason = "r".repeat(4096);
    let json = serde_json::to_string(&annotation).unwrap();
    for index in 0..=unrelated {
        let candidate = if index == 0 {
            target.clone()
        } else {
            let mut value = distinct_key(index);
            value.project.clone_from(&target.project);
            value
        };
        transaction
            .execute(
                "INSERT INTO candidates(id, key_json) VALUES (?1, ?2)",
                rusqlite::params![
                    candidate.candidate_id,
                    serde_json::to_string(&candidate).unwrap()
                ],
            )
            .unwrap();
        let count = if index == 0 { history } else { 1 };
        for revision in 1..=u64::try_from(count).unwrap() {
            transaction.execute(
                "INSERT INTO review_events(candidate_id, revision, draft_json) VALUES (?1, ?2, ?3)",
                rusqlite::params![candidate.candidate_id, revision.to_be_bytes().as_slice(), json],
            ).unwrap();
        }
    }
    transaction.commit().unwrap();
    assert!(std::fs::metadata(path).unwrap().len() < 4 * 1024 * 1024);
}

fn record_measurement(unrelated: usize, history: usize) -> Measurement {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reviews.sqlite3");
    let target = key();
    populate(&path, &target, unrelated, history);
    let mut store = ReviewStore::open_write(&path, &target.project).unwrap();
    let (state, measured) = measure(|| {
        store
            .append(
                &target,
                SourceState::Fresh,
                u64::try_from(history).unwrap(),
                draft(),
            )
            .unwrap()
    });
    assert_eq!(state.history.len(), history + 1);
    assert_eq!(
        state.history.last().unwrap().revision,
        u64::try_from(history + 1).unwrap()
    );
    assert_eq!(measured.work.key_decodes, unrelated + 1);
    assert_eq!(measured.work.draft_decodes, unrelated + history);
    eprintln!("record unrelated={unrelated} history={history}: {measured:?}");
    assert!(measured.elapsed < Duration::from_secs(10));
    measured
}

#[test]
fn record_streams_unrelated_history_with_bounded_queries_and_peak_rust_heap() {
    let small = record_measurement(0, 3);
    let large = record_measurement(192, 3);
    assert!(large.work.data_queries <= 3, "{large:?}");
    assert!(
        large.peak_rust_bytes <= small.peak_rust_bytes + 64 * 1024,
        "small={small:?}; large={large:?}"
    );
}

#[test]
fn record_retains_target_history_once_without_cloning_existing_events() {
    let short = record_measurement(0, 1);
    let long = record_measurement(0, 128);
    assert_eq!(long.work.cloned_events, 0, "{long:?}");
    assert!(long.work.data_queries <= 3, "{long:?}");
    assert!(long.peak_rust_bytes > short.peak_rust_bytes);
    assert!(long.peak_rust_bytes < 1024 * 1024, "{long:?}");
}

#[test]
fn full_snapshot_uses_bounded_queries_without_decoding_history_twice() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reviews.sqlite3");
    let target = key();
    populate(&path, &target, 192, 3);
    let mut store = ReviewStore::open_read(&path, &target.project)
        .unwrap()
        .unwrap();
    let (snapshot, measured) = measure(|| store.snapshot().unwrap());
    assert_eq!(snapshot.len(), 193);
    assert_eq!(measured.work.key_decodes, 193);
    assert_eq!(measured.work.draft_decodes, 195);
    assert!(measured.work.data_queries <= 3, "{measured:?}");
}

#[tokio::test]
async fn show_streams_unrelated_history_and_keeps_the_selected_history() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("sample.py"), "value = 1\n").unwrap();
    let crate::cli::ParsedCommand::Plan(args) = crate::cli::parse_from([
        "hoimin",
        "plan",
        "--root",
        directory.path().to_str().unwrap(),
        "--file",
        "sample.py",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
        "--",
        "must-not-execute",
    ])
    .unwrap() else {
        panic!("plan")
    };
    let (config, discovery) = args.into_configs().unwrap();
    let output = crate::plan::create(config, discovery).await.unwrap();
    let plan_path = directory.path().join("plan.json");
    std::fs::write(&plan_path, serde_json::to_vec(&output.manifest).unwrap()).unwrap();
    let plan = super::input::load_plan(&plan_path).unwrap();
    let target = super::input::review_key(&plan.project, &plan.manifest.candidates[0].candidate);
    let mut measurements = Vec::new();
    for unrelated in [0, 192] {
        let path = directory
            .path()
            .join(format!("reviews-{unrelated}.sqlite3"));
        populate(&path, &target, unrelated, 3);
        let crate::cli::ParsedCommand::Review(args) = crate::cli::parse_from([
            "hoimin",
            "review",
            "show",
            plan_path.to_str().unwrap(),
            "--store",
            path.to_str().unwrap(),
            "--candidate",
            &target.candidate_id,
            "--format",
            "json",
        ])
        .unwrap() else {
            panic!("review")
        };
        let ((code, bytes), measured) = measure(|| {
            let mut bytes = Vec::new();
            let code = super::run(&args, &mut bytes).unwrap();
            (code, bytes)
        });
        assert_eq!(code, 0);
        let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(document["candidates"].as_array().unwrap().len(), 1);
        assert_eq!(
            document["candidates"][0]["history"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(measured.work.key_decodes, unrelated + 1);
        assert_eq!(measured.work.draft_decodes, unrelated + 3);
        eprintln!("show unrelated={unrelated}: {measured:?}");
        measurements.push(measured);
    }
    let (small, large) = (&measurements[0], &measurements[1]);
    assert!(large.work.data_queries <= 3, "{large:?}");
    assert!(
        large.peak_rust_bytes <= small.peak_rust_bytes + 64 * 1024,
        "small={small:?}; large={large:?}"
    );
}

#[test]
fn unrelated_corruption_is_rejected_and_append_rolls_back() {
    for corruption in [
        "UPDATE review_events SET draft_json='{}' WHERE candidate_id=?1",
        "UPDATE review_events SET revision=x'0000000000000002' WHERE candidate_id=?1",
        "DELETE FROM review_events WHERE candidate_id=?1",
        "UPDATE candidates SET key_json='{}' WHERE id=?1",
        "DELETE FROM candidates WHERE id=?1",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reviews.sqlite3");
        let target = key();
        populate(&path, &target, 1, 3);
        let mut store = ReviewStore::open_write(&path, &target.project).unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();
        connection
            .execute(corruption, [distinct_key(1).candidate_id])
            .unwrap();
        drop(connection);
        let before = std::fs::read(&path).unwrap();
        assert!(
            store
                .append(&target, SourceState::Fresh, 3, draft())
                .is_err(),
            "{corruption}"
        );
        assert!(store.snapshot().is_err(), "{corruption}");
        assert!(
            store.candidate(&target.candidate_id).is_err(),
            "{corruption}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "{corruption}");
    }
}
