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
pub(super) struct Measurement {
    work: StoreWork,
    pub(super) peak_rust_bytes: i128,
    elapsed: Duration,
}

pub(super) fn measure<T>(operation: impl FnOnce() -> T) -> (T, Measurement) {
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

// The command writes directly into a counter during measurement. Holding its
// serialized JSON would obscure the extra in-memory history being tested.
#[derive(Default)]
struct CountingWriter(usize);

impl std::io::Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn populate_selected_history(
    path: &std::path::Path,
    target: &ReviewKey,
    history: usize,
    text_bytes: usize,
) {
    drop(ReviewStore::open_write(path, &target.project).unwrap());
    let mut connection = rusqlite::Connection::open(path).unwrap();
    let transaction = connection.transaction().unwrap();
    transaction
        .execute(
            "INSERT INTO candidates(id, key_json) VALUES (?1, ?2)",
            rusqlite::params![target.candidate_id, serde_json::to_string(target).unwrap()],
        )
        .unwrap();
    let mut annotation = draft();
    annotation.reason = "r".repeat(text_bytes);
    annotation.evidence = vec!["e".repeat(text_bytes.min(2048))];
    super::model::validate_draft(&annotation).unwrap();
    let json = serde_json::to_string(&annotation).unwrap();
    for revision in 1..=u64::try_from(history).unwrap() {
        transaction
            .execute(
                "INSERT INTO review_events(candidate_id, revision, draft_json) VALUES (?1, ?2, ?3)",
                rusqlite::params![target.candidate_id, revision.to_be_bytes().as_slice(), json],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
    assert!(std::fs::metadata(path).unwrap().len() <= 1024 * 1024);
}

#[tokio::test]
async fn selected_review_history_is_retained_once_while_rendering() {
    let (directory, plan_path) = super::tests::plan_fixture().await;
    let plan = super::input::load_plan(&plan_path).unwrap();
    let current = super::input::review_key(&plan.project, &plan.manifest.candidates[0].candidate);
    for command in ["show", "list", "list-stale"] {
        let mut target = current.clone();
        if command == "list-stale" {
            target.descriptor.replacement = "987654321".into();
            target.candidate_id = hoimin_core::stable_mutant_id(
                &hoimin_core::CandidateIdentity::from(&target.descriptor),
            )
            .to_string();
            assert!(
                plan.manifest
                    .candidates
                    .iter()
                    .all(|row| row.candidate.id != target.candidate_id)
            );
        }
        for text_bytes in [1024, 4096] {
            let mut measurements = Vec::new();
            for history in [1, 16, 64] {
                let path = directory
                    .path()
                    .join(format!("{command}-{text_bytes}-{history}.sqlite3"));
                populate_selected_history(&path, &target, history, text_bytes);
                let mut cli_arguments = vec![
                    "hoimin",
                    "review",
                    if command == "show" { "show" } else { "list" },
                    plan_path.to_str().unwrap(),
                    "--store",
                    path.to_str().unwrap(),
                ];
                if command == "show" {
                    cli_arguments.extend(["--candidate", current.candidate_id.as_str()]);
                } else if command == "list-stale" {
                    cli_arguments.push("--stale");
                }
                let crate::cli::ParsedCommand::Review(args) =
                    crate::cli::parse_from(cli_arguments).unwrap()
                else {
                    panic!("review")
                };
                let mut writer = CountingWriter::default();
                let (code, measured) = measure(|| super::run(&args, &mut writer).unwrap());
                assert_eq!(code, 0);
                assert!(writer.0 >= history * (text_bytes + text_bytes.min(2048)));

                // Validate actual output separately, after the allocator is off.
                let mut bytes = Vec::new();
                assert_eq!(super::run(&args, &mut bytes).unwrap(), 0);
                let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let field = if command == "list-stale" {
                    "stale_candidates"
                } else {
                    "candidates"
                };
                let row = document[field]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["candidate"]["id"] == target.candidate_id)
                    .unwrap();
                let events = row["history"].as_array().unwrap();
                assert_eq!(events.len(), history);
                for (index, event) in events.iter().enumerate() {
                    assert_eq!(event["revision"], index + 1);
                    assert_eq!(event["draft"]["reason"], "r".repeat(text_bytes));
                    assert_eq!(
                        event["draft"]["evidence"][0],
                        "e".repeat(text_bytes.min(2048))
                    );
                }
                if command == "list-stale" {
                    assert!(row["review"].is_null());
                    assert_eq!(row["applicability"], "stale");
                } else {
                    assert_eq!(&row["review"], events.last().unwrap());
                    assert_eq!(row["applicability"], "current");
                }
                eprintln!(
                    "selected {command} history={history} text_bytes={text_bytes}: {measured:?}"
                );
                measurements.push(measured);
            }
            // One snapshot must retain the text. Row metadata, event capacity,
            // decoder scratch and plan/source validation get 96 KiB of slack;
            // retaining a second 64-event history exceeds this even at 1 KiB.
            let payload_growth = 63 * (text_bytes + text_bytes.min(2048));
            let growth = measurements[2].peak_rust_bytes - measurements[0].peak_rust_bytes;
            assert!(
                growth <= payload_growth as i128 + 96 * 1024,
                "{command} text_bytes={text_bytes}: growth={growth}, single_history_payload_growth={payload_growth}; {measurements:?}"
            );
        }
    }
}

#[tokio::test]
async fn selected_record_history_is_retained_once_after_saving() {
    let (directory, plan_path) = super::tests::plan_fixture().await;
    let plan = super::input::load_plan(&plan_path).unwrap();
    let target = super::input::review_key(&plan.project, &plan.manifest.candidates[0].candidate);
    let reason = "r".repeat(4096);
    let evidence = "e".repeat(2048);
    let mut measurements = Vec::new();
    for history in [1, 16, 64] {
        let path = directory.path().join(format!("record-{history}.sqlite3"));
        populate_selected_history(&path, &target, history, reason.len());
        let revision = history.to_string();
        let crate::cli::ParsedCommand::Review(args) = crate::cli::parse_from([
            "hoimin",
            "review",
            "record",
            plan_path.to_str().unwrap(),
            "--store",
            path.to_str().unwrap(),
            "--candidate",
            &target.candidate_id,
            "--assessment",
            "actionable",
            "--reason",
            &reason,
            "--evidence",
            &evidence,
            "--expected-revision",
            &revision,
        ])
        .unwrap() else {
            panic!("record")
        };
        let mut writer = CountingWriter::default();
        let (code, measured) = measure(|| super::run(&args, &mut writer).unwrap());
        assert_eq!(code, 0);
        assert!(writer.0 >= (history + 1) * (reason.len() + evidence.len()));

        // Read through the command again to prove the new event is durable and
        // the latest assessment still appears both in review and in history.
        let crate::cli::ParsedCommand::Review(show) = crate::cli::parse_from([
            "hoimin",
            "review",
            "show",
            plan_path.to_str().unwrap(),
            "--store",
            path.to_str().unwrap(),
            "--candidate",
            &target.candidate_id,
        ])
        .unwrap() else {
            panic!("show")
        };
        let mut bytes = Vec::new();
        assert_eq!(super::run(&show, &mut bytes).unwrap(), 0);
        let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let row = &document["candidates"][0];
        let events = row["history"].as_array().unwrap();
        assert_eq!(events.len(), history + 1);
        assert_eq!(row["review"]["revision"], history + 1);
        assert_eq!(&row["review"], events.last().unwrap());
        assert_eq!(row["review"]["draft"]["reason"], reason);
        assert_eq!(row["review"]["draft"]["evidence"][0], evidence);
        eprintln!("selected record history={history}: {measured:?}");
        measurements.push(measured);
    }
    let payload_growth = 63 * (reason.len() + evidence.len());
    let growth = measurements[2].peak_rust_bytes - measurements[0].peak_rust_bytes;
    assert!(
        growth <= payload_growth as i128 + 96 * 1024,
        "record growth={growth}, single_history_payload_growth={payload_growth}; {measurements:?}"
    );
}

#[tokio::test]
async fn issue_1083_read_only_plan_identity_does_not_clone_payloads() {
    let (directory, plan_path) = super::tests::plan_fixture().await;
    let mut manifest = crate::plan::read_for_review(&plan_path).unwrap().manifest;
    manifest.candidates.truncate(1);
    manifest.candidates[0].candidate.sequence = 1;
    let mut measurements = Vec::new();
    for payload in [4 * 1024, 16 * 1024, 64 * 1024] {
        let candidate = &mut manifest.candidates[0].candidate;
        candidate.original = "o".repeat(payload);
        candidate.replacement = "r".repeat(payload);
        let mut source = vec![b' '; usize::try_from(candidate.span.start).unwrap()];
        source.extend_from_slice(candidate.original.as_bytes());
        source.push(b'\n');
        candidate.span.length = u64::try_from(candidate.original.len()).unwrap();
        candidate.file_hash = blake3::hash(&source).to_hex().to_string();
        candidate.id = hoimin_core::CandidateIdentityRef::from(&*candidate)
            .stable_mutant_id()
            .to_string();
        let expected_id = candidate.id.clone();
        std::fs::write(directory.path().join(&candidate.path), source).unwrap();
        for record in &mut manifest.sources {
            if record.path == candidate.path {
                record.hash.clone_from(&candidate.file_hash);
            }
        }
        std::fs::write(&plan_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let (plan, measured) = measure(|| crate::plan::read_for_review(&plan_path).unwrap());
        assert_eq!(plan.manifest.candidates[0].id, expected_id);
        assert_eq!(plan.manifest.candidates[0].original.len(), payload);
        assert_eq!(plan.manifest.candidates[0].replacement.len(), payload);
        eprintln!("identity plan payload={payload}: {measured:?}");
        measurements.push((payload, measured.peak_rust_bytes));
        // Exercise public rendering separately from the allocation window.
        let crate::cli::ParsedCommand::Review(args) = crate::cli::parse_from([
            "hoimin",
            "review",
            "list",
            plan_path.to_str().unwrap(),
            "--store",
            directory.path().join("absent.sqlite3").to_str().unwrap(),
        ])
        .unwrap() else {
            panic!("review")
        };
        assert_identity_list_output(&args, &manifest.candidates[0].candidate);
    }
    for (payload, peak) in measurements {
        // JSON input storage, decoder and owned candidate remain necessary.
        // A second descriptor and identity would retain three more payloads.
        assert!(
            peak <= (7 * payload + 32 * 1024) as i128,
            "identity plan payload={payload} peak={peak}"
        );
    }
    manifest.candidates[0].candidate.id = "0".repeat(64);
    std::fs::write(&plan_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(crate::plan::read_for_review(&plan_path).is_err());
}

#[test]
fn issue_1083_store_identity_does_not_clone_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let mut measurements = Vec::new();
    for payload in [4 * 1024, 16 * 1024, 64 * 1024] {
        let path = directory.path().join(format!("identity-{payload}.sqlite3"));
        let mut target = key();
        target.descriptor.replacement = "r".repeat(payload);
        target.candidate_id = hoimin_core::CandidateIdentityRef::from(&target.descriptor)
            .stable_mutant_id()
            .to_string();
        populate_selected_history(&path, &target, 1, 1024);
        let mut store = ReviewStore::open_read(&path, &target.project)
            .unwrap()
            .unwrap();
        let (row, measured) = measure(|| store.candidate(&target.candidate_id).unwrap().unwrap());
        assert_eq!(row.0, target);
        assert_eq!(row.1.history.len(), 1);
        eprintln!("identity store payload={payload}: {measured:?}");
        measurements.push((payload, measured.peak_rust_bytes));
        // Corrupt identity through the real database reader, after measurement.
        let mut corrupt = target.clone();
        corrupt.descriptor.replacement.push('x');
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE candidates SET key_json=?1",
                [serde_json::to_string(&corrupt).unwrap()],
            )
            .unwrap();
        drop(connection);
        assert!(store.candidate(&target.candidate_id).is_err());
    }
    for (payload, peak) in measurements {
        // Retain the serialized DB row and decoded key, without a third copy.
        assert!(
            peak <= (2 * payload + 16 * 1024) as i128,
            "identity store payload={payload} peak={peak}"
        );
    }
}

fn assert_identity_list_output(
    args: &crate::cli::ReviewArgs,
    candidate: &hoimin_core::MutationCandidate,
) {
    let mut bytes = Vec::new();
    assert_eq!(super::run(args, &mut bytes).unwrap(), 0);
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let row = &document["candidates"][0]["candidate"];
    assert_eq!(row["id"], candidate.id);
    assert_eq!(row["original"], candidate.original);
    assert_eq!(row["replacement"], candidate.replacement);
    assert_eq!(row["path"], candidate.path.as_str());
}

// Observe the actual public command's render phase. Unlike a Vec output sink,
// this writer retains no JSON; the decoder's input and source contexts have
// already been dropped before serde_json issues its first write.
#[derive(Default)]
struct Issue1087RenderWriter {
    first_write_live_bytes: Option<i128>,
    bytes: usize,
}

impl std::io::Write for Issue1087RenderWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.first_write_live_bytes.is_none() {
            self.first_write_live_bytes =
                ALLOCATIONS.with(|counter| counter.get().map(|(live, _)| live));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn issue_1087_current_candidate_payloads_are_retained_once_at_first_json_write() {
    let (directory, plan_path) = super::tests::plan_fixture().await;
    let mut manifest = crate::plan::read_for_review(&plan_path).unwrap().manifest;
    let template = manifest.candidates[0].clone();
    for count in [16, 64] {
        let mut measurements = Vec::new();
        for payload in [4 * 1024, 16 * 1024, 64 * 1024] {
            manifest.candidates = (0..count)
                .map(|index| {
                    let mut row = template.clone();
                    // A valid Python expression, with distinct equal-size payloads.
                    row.candidate.replacement = format!("'{index:04}{}'", "r".repeat(payload - 6));
                    assert_eq!(row.replacement.len(), payload);
                    row.candidate.sequence = u64::try_from(index + 1).unwrap();
                    row.candidate.id = hoimin_core::CandidateIdentityRef::from(&row.candidate)
                        .stable_mutant_id()
                        .to_string();
                    row
                })
                .collect();
            // All preceding ranking keys are identical; stable ID breaks ties.
            manifest
                .candidates
                .sort_by(|left, right| left.id.cmp(&right.id));
            for (index, row) in manifest.candidates.iter_mut().enumerate() {
                row.rank = index + 1;
            }
            let encoded = serde_json::to_vec(&manifest).unwrap();
            assert!(encoded.len() < 8 * 1024 * 1024);
            std::fs::write(&plan_path, encoded).unwrap();
            let checked = super::input::load_plan(&plan_path).unwrap();
            assert!(
                super::input::source_states(
                    &checked.project,
                    &checked
                        .manifest
                        .candidates
                        .iter()
                        .map(|row| &row.candidate)
                        .collect::<Vec<_>>()
                )
                .into_iter()
                .all(|state| state.unwrap() == SourceState::Fresh)
            );
            let project = checked.project.clone();
            drop(checked);
            let crate::cli::ParsedCommand::Review(args) = crate::cli::parse_from([
                "hoimin",
                "review",
                "list",
                plan_path.to_str().unwrap(),
                "--store",
                directory.path().join("absent.sqlite3").to_str().unwrap(),
            ])
            .unwrap() else {
                panic!("review list")
            };
            let mut writer = Issue1087RenderWriter::default();
            let (code, _) = measure(|| super::run(&args, &mut writer).unwrap());
            assert_eq!(code, 0);
            assert!(writer.bytes >= count * payload);
            let live = writer
                .first_write_live_bytes
                .expect("actual JSON write observed");
            assert!(
                live >= (count * payload) as i128,
                "the loaded plan must retain its payloads"
            );
            eprintln!("issue_1087 count={count} replacement_bytes={payload} render_live={live}");
            measurements.push((payload, live));
            // Full content and ordering are checked with allocation tracking off.
            assert_issue_1087_output(&args, &manifest, &project);
        }
        for pair in measurements.windows(2) {
            let payload_growth = count * (pair[1].0 - pair[0].0);
            let live_growth = pair[1].1 - pair[0].1;
            // The plan needs one payload population. Fixed-size row/index metadata
            // and allocator capacity receive 128 KiB; a second population fails.
            assert!(
                live_growth <= payload_growth as i128 + 128 * 1024,
                "count={count}: live_growth={live_growth}, single_population_growth={payload_growth}; {measurements:?}"
            );
        }
    }
}

fn assert_issue_1087_output(
    args: &crate::cli::ReviewArgs,
    manifest: &crate::plan::PlanManifest,
    project: &camino::Utf8Path,
) {
    let mut bytes = Vec::new();
    assert_eq!(super::run(args, &mut bytes).unwrap(), 0);
    let rows: Vec<_> = manifest
        .candidates
        .iter()
        .map(|row| {
            serde_json::json!({
                "candidate": row.candidate,
                "execution": {"state": "not_loaded"},
                "review": null,
                "applicability": "current",
                "history": [],
            })
        })
        .collect();
    let operator = &manifest.candidates[0].operator;
    let count = manifest.candidates.len();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        serde_json::json!({
            "schema_version": 2,
            "command": "list",
            "project": project,
            "candidates": rows,
            "stale_candidates": [],
            "operators": [{"operator": operator, "total": count, "reviewed": 0,
                "unreviewed": count, "stale": 0, "source_unavailable": 0, "assessments": {}}],
        })
    );
}

pub(super) fn issue_1089_candidates(
    count: usize,
    payload: usize,
) -> (Vec<u8>, Vec<hoimin_core::MutationCandidate>) {
    let original = format!("'{}'", "o".repeat(payload - 2));
    let source = format!("{original}\n").into_bytes();
    let file_hash = blake3::hash(&source).to_hex().to_string();
    let candidates = (0..count)
        .map(|index| {
            let mut candidate = hoimin_core::MutationCandidate {
                id: String::new(),
                sequence: u64::try_from(index + 1).unwrap(),
                path: "subject.py".into(),
                span: hoimin_core::ByteSpan {
                    start: 0,
                    length: u64::try_from(payload).unwrap(),
                },
                original: original.clone(),
                replacement: format!("'{index:04}{}'", "r".repeat(payload - 6)),
                operator: "string_literal_empty".into(),
                line: 1,
                column: 0,
                symbol: Some("selected".into()),
                file_hash: file_hash.clone(),
            };
            candidate.id = hoimin_core::CandidateIdentityRef::from(&candidate)
                .stable_mutant_id()
                .to_string();
            candidate
        })
        .collect();
    (source, candidates)
}

#[test]
fn issue_1089_execution_comparison_does_not_copy_descriptor_payloads() {
    let mut observations = Vec::new();
    for count in [16, 64] {
        for payload in [4 * 1024, 16 * 1024, 64 * 1024] {
            let (_, candidates) = issue_1089_candidates(count, payload);
            let document: serde_json::Value =
                crate::artifacts::run_report::test_support::current_report_view();
            let mut wire = document["mutants"][0].clone();
            wire.as_object_mut().unwrap().remove("kind");
            let template: hoimin_core::MutantFinished = serde_json::from_value(wire).unwrap();
            let reported: Vec<_> = candidates
                .iter()
                .map(|candidate| {
                    let mut result = template.clone();
                    result.candidate = candidate.clone();
                    result
                })
                .collect();
            let index = reported
                .iter()
                .map(|result| (result.candidate.id.as_str(), result))
                .collect();
            let ((), measurement) = measure(|| {
                for candidate in &candidates {
                    let result = super::execution(candidate, Some(&index)).unwrap();
                    assert!(matches!(result, super::render::ReviewExecution::Result(_)));
                }
            });
            eprintln!(
                "issue_1089 execution count={count} payload={payload} peak={}",
                measurement.peak_rust_bytes
            );
            observations.push((count, payload, measurement.peak_rust_bytes));
        }
    }
    assert!(
        observations.iter().all(|(_, _, peak)| *peak < 4096),
        "descriptor comparison copied payloads: {observations:?}"
    );
}

#[test]
fn issue_1089_execution_excludes_sequence_but_checks_symbol() {
    let (_, candidates) = issue_1089_candidates(1, 32);
    let candidate = &candidates[0];
    let mut reported = hoimin_core::MutantFinished {
        schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
        sequence: 1,
        run_id: "run".into(),
        candidate: candidate.clone(),
        outcome: hoimin_core::MutationOutcome::NotRun,
        resource_mode: hoimin_core::ResourceMode::BestEffort,
        diagnostics: Vec::new(),
    };
    reported.candidate.sequence = 0;
    {
        let index = std::collections::HashMap::from([(candidate.id.as_str(), &reported)]);
        assert!(matches!(
            super::execution(candidate, Some(&index)).unwrap(),
            super::render::ReviewExecution::Result(hoimin_core::MutationStatus::NotRun)
        ));
    }
    reported.candidate.symbol = None;
    let index = std::collections::HashMap::from([(candidate.id.as_str(), &reported)]);
    assert!(
        matches!(super::execution(candidate, Some(&index)), Err(error) if error.to_string().contains("descriptor differs"))
    );
}
