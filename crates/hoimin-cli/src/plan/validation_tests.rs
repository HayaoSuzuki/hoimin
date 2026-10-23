use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use camino::Utf8PathBuf;
use hoimin_core::{
    ByteSpan, CandidateDescriptor, MutationCandidate, TargetSlice, validate_candidate,
};

use super::{PlanError, ValidationStats, validate_requested_descriptors};

fn fingerprint_records(entries: &[(&str, &str)]) -> Vec<hoimin_core::FingerprintInputFile> {
    entries
        .iter()
        .map(|(path, hash)| hoimin_core::FingerprintInputFile {
            path: (*path).into(),
            hash: (*hash).into(),
        })
        .collect()
}

#[test]
fn stale_record_details_classify_sort_and_escape_both_record_kinds() {
    let expected = fingerprint_records(&[
        ("z.py", "old"),
        ("same.py", "same"),
        ("a.py", "old"),
        ("controls\n\r\t\"\\\u{1b}.py", "old"),
    ]);
    let current = fingerprint_records(&[("m.py", "new"), ("same.py", "same"), ("a.py", "new")]);
    for (kind, prefix) in [
        (
            super::RecordMismatch::Source,
            "plan.source.changed: planned target source records",
        ),
        (
            super::RecordMismatch::FingerprintInput,
            "plan.fingerprint_input.changed: planned fingerprint input records",
        ),
    ] {
        let error = super::ensure_exact_records(&expected, &current, kind)
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            format!(
                "{prefix} do not match the current workspace: modified \"a.py\"; removed \"controls\\n\\r\\t\\\"\\\\\\u{{1b}}.py\"; added \"m.py\"; removed \"z.py\""
            )
        );
        assert_eq!(error.lines().count(), 1);
        let mut reordered = expected.clone();
        reordered.reverse();
        assert_eq!(
            super::ensure_exact_records(&reordered, &current, kind)
                .unwrap_err()
                .to_string(),
            error
        );
        assert!(super::ensure_exact_records(&expected, &reordered, kind).is_ok());
        assert!(super::ensure_exact_records(&[], &[], kind).is_ok());
    }
}

#[test]
fn stale_record_details_bound_display_and_count_omitted_paths() {
    for (count, omitted) in [
        (1, ""),
        (10, ""),
        (11, "; 1 additional paths omitted"),
        (13, "; 3 additional paths omitted"),
    ] {
        let current: Vec<_> = (0..count)
            .rev()
            .map(|index| hoimin_core::FingerprintInputFile {
                path: format!("src/{index:02}.py").into(),
                hash: "current".into(),
            })
            .collect();
        for (saved, live, label) in [
            (&[][..], current.as_slice(), "added"),
            (current.as_slice(), &[][..], "removed"),
        ] {
            let error = super::ensure_exact_records(saved, live, super::RecordMismatch::Source)
                .unwrap_err()
                .to_string();
            let displayed = (0..count.min(10))
                .map(|index| format!("{label} \"src/{index:02}.py\""))
                .collect::<Vec<_>>()
                .join("; ");
            assert_eq!(
                error,
                format!(
                    "plan.source.changed: planned target source records do not match the current workspace: {displayed}{omitted}"
                )
            );
        }
    }
}

fn candidates(path: &str, count: usize, bytes: usize) -> (Vec<u8>, Vec<MutationCandidate>) {
    let mut source = "x = 1 + 2\n".repeat(count);
    if source.len() < bytes {
        source.push('#');
        source.extend(std::iter::repeat_n(' ', bytes - source.len()));
    }
    let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
    let candidates = (0..count)
        .map(|i| {
            let mut candidate = MutationCandidate {
                id: String::new(),
                sequence: i as u64 + 1,
                path: path.into(),
                span: ByteSpan {
                    start: (i * 10 + 6) as u64,
                    length: 1,
                },
                original: "+".into(),
                replacement: "-".into(),
                operator: "binary_add_sub".into(),
                line: u32::try_from(i).unwrap() + 1,
                column: 6,
                symbol: None,
                file_hash: hash.clone(),
            };
            candidate.id =
                validate_candidate(source.as_bytes(), &CandidateDescriptor::from(&candidate))
                    .unwrap()
                    .to_string();
            candidate
        })
        .collect();
    (source.into_bytes(), candidates)
}

async fn check(
    project: &super::tests::Project,
    candidates: &[MutationCandidate],
    stats: &mut ValidationStats,
) -> Result<BTreeSet<Utf8PathBuf>, PlanError> {
    let ids = candidates.iter().map(|c| c.id.clone()).collect();
    let map = candidates.iter().map(|c| (c.id.as_str(), c)).collect();
    let targets: Vec<_> = candidates
        .iter()
        .map(|c| TargetSlice {
            path: c.path.clone(),
            lines: vec![],
            symbols: vec![],
        })
        .collect();
    validate_requested_descriptors(
        &map,
        &ids,
        &project
            .config()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap(),
        &targets,
        Some(stats),
    )
    .await
}

#[tokio::test]
async fn descriptor_validation_cost_tracks_files_and_bytes_not_candidates() {
    for file_count in [1, 2, 4] {
        for count in [1, 2, 4] {
            let project = super::tests::Project::new();
            let mut all = Vec::new();
            for f in 0..file_count {
                let path = format!("src/file{f}.py");
                let (source, mut records) = candidates(&path, count, 4096);
                std::fs::write(project.root.join(&path), source).unwrap();
                all.append(&mut records);
            }
            let mut stats = ValidationStats::default();
            let paths = check(&project, &all, &mut stats).await.unwrap();
            assert_eq!(paths.len(), file_count);
            assert_eq!(stats.contexts, file_count);
            assert_eq!(stats.source_bytes, file_count * 4096);
        }
    }
}

#[tokio::test]
async fn descriptor_validation_preserves_each_validation_and_stable_identity() {
    let project = super::tests::Project::new();
    let (source, original) = candidates("src/calc.py", 1, 0);
    std::fs::write(project.root.join("src/calc.py"), &source).unwrap();
    let original_candidate = original[0].clone();
    for (candidate, message) in [
        (
            MutationCandidate {
                file_hash: "0".repeat(64),
                ..original_candidate.clone()
            },
            "candidate file hash does not match the source",
        ),
        (
            MutationCandidate {
                span: ByteSpan {
                    start: 100,
                    ..original_candidate.span
                },
                ..original_candidate.clone()
            },
            "candidate span is outside the source",
        ),
        (
            MutationCandidate {
                original: "?".into(),
                ..original_candidate.clone()
            },
            "candidate original text does not match the source span",
        ),
        (
            MutationCandidate {
                line: 2,
                ..original_candidate.clone()
            },
            "candidate line or column does not match its byte span",
        ),
        (
            MutationCandidate {
                column: 7,
                ..original_candidate.clone()
            },
            "candidate line or column does not match its byte span",
        ),
        (
            MutationCandidate {
                operator: String::new(),
                ..original_candidate
            },
            "candidate operator is empty or replacement is unchanged",
        ),
    ] {
        assert_eq!(
            validate_candidate(&source, &CandidateDescriptor::from(&candidate))
                .unwrap_err()
                .to_string(),
            message
        );
        let error = check(&project, &[candidate], &mut ValidationStats::default())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("plan.candidate.invalid: {message}")
        );
    }
    let mut candidate = original[0].clone();
    candidate.id = format!("m1_{}", "0".repeat(64));
    let error = check(
        &project,
        &[candidate.clone()],
        &mut ValidationStats::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        format!(
            "plan.candidate.invalid: candidate stable id differs for {}",
            candidate.id
        )
    );
    let mut invalid = source;
    invalid.push(255);
    candidate.file_hash = blake3::hash(&invalid).to_hex().to_string();
    std::fs::write(project.root.join("src/calc.py"), invalid).unwrap();
    let error = check(&project, &[candidate], &mut ValidationStats::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate source is not valid UTF-8"
    );
}

#[tokio::test]
async fn descriptor_validation_reports_errors_in_requested_id_order() {
    let project = super::tests::Project::new();
    let mut all = Vec::new();
    for path in ["src/a.py", "src/b.py"] {
        let (source, mut records) = candidates(path, 16, 0);
        std::fs::write(project.root.join(path), source).unwrap();
        all.append(&mut records);
    }
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let first_path = all[0].path.clone();
    let other = all.iter().position(|c| c.path != first_path).unwrap();
    let later = all
        .iter()
        .enumerate()
        .skip(other + 1)
        .find(|(_, c)| c.path == first_path)
        .map(|(i, _)| i)
        .unwrap();
    all[other].original = "?".into();
    all[later].file_hash = "0".repeat(64);
    let error = check(&project, &all, &mut ValidationStats::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate original text does not match the source span"
    );
}

#[tokio::test]
async fn descriptor_validation_empty_unknown_and_unselected_do_not_read_sources() {
    let project = super::tests::Project::new();
    let config = project
        .config()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
    let mut stats = ValidationStats::default();
    assert!(
        validate_requested_descriptors(
            &BTreeMap::new(),
            &BTreeSet::new(),
            &config,
            &[],
            Some(&mut stats)
        )
        .await
        .unwrap()
        .is_empty()
    );
    let error = validate_requested_descriptors(
        &BTreeMap::new(),
        &BTreeSet::from(["missing".into()]),
        &config,
        &[],
        Some(&mut stats),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate id is not in the plan: missing"
    );
    let (_, records) = candidates("src/absent.py", 1, 0);
    let map = records.iter().map(|c| (c.id.as_str(), c)).collect();
    let ids = records.iter().map(|c| c.id.clone()).collect();
    let error = validate_requested_descriptors(&map, &ids, &config, &[], Some(&mut stats))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate target is not selected: src/absent.py"
    );
    assert_eq!(stats.contexts, 0);
    assert_eq!(stats.source_bytes, 0);
}

#[tokio::test]
#[ignore = "release validation benchmark; run explicitly with --release --ignored --nocapture"]
async fn benchmark_requested_descriptor_preprocessing() {
    for file_count in [1, 2, 4] {
        for count in [1, 100, 500] {
            let project = super::tests::Project::new();
            let mut all = Vec::new();
            let mut sources = BTreeMap::new();
            for f in 0..file_count {
                let path = format!("src/file{f}.py");
                let (source, mut records) = candidates(&path, count, 2 * 1024 * 1024);
                std::fs::write(project.root.join(&path), &source).unwrap();
                sources.insert(Utf8PathBuf::from(path), source);
                all.append(&mut records);
            }
            for round in 0..3 {
                let start = Instant::now();
                for candidate in &all {
                    let id = validate_candidate(
                        std::hint::black_box(&sources[&candidate.path]),
                        &CandidateDescriptor::from(candidate),
                    )
                    .unwrap();
                    assert_eq!(id.as_str(), candidate.id);
                }
                let legacy_ms = start.elapsed().as_secs_f64() * 1000.0;
                let start = Instant::now();
                let mut stats = ValidationStats::default();
                check(&project, &all, &mut stats).await.unwrap();
                let shared_ms = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(stats.contexts, file_count);
                println!(
                    "files={file_count} candidates_per_file={count} bytes_per_file=2097152 round={round} legacy_ms={legacy_ms:.3} shared_including_read_ms={shared_ms:.3} contexts={} bytes={}",
                    stats.contexts, stats.source_bytes
                );
            }
        }
    }
}

#[tokio::test]
async fn target_membership_cost_does_not_multiply_candidates_and_targets() {
    for count in [1, 4, 16] {
        for target_count in [1, 8, 64] {
            let project = super::tests::Project::new();
            let (source, records) = candidates("src/z.py", count, 0);
            std::fs::write(project.root.join("src/z.py"), source).unwrap();
            let map = records.iter().map(|c| (c.id.as_str(), c)).collect();
            let ids = records.iter().map(|c| c.id.clone()).collect();
            let mut targets: Vec<_> = (1..target_count)
                .map(|i| TargetSlice {
                    path: format!("src/m{i}.py").into(),
                    lines: vec![],
                    symbols: vec![],
                })
                .collect();
            targets.push(TargetSlice {
                path: "src/z.py".into(),
                lines: vec![],
                symbols: vec![],
            });
            let mut stats = ValidationStats::default();
            let paths = validate_requested_descriptors(
                &map,
                &ids,
                &project
                    .config()
                    .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
                    .unwrap(),
                &targets,
                Some(&mut stats),
            )
            .await
            .unwrap();
            assert_eq!(paths, BTreeSet::from([Utf8PathBuf::from("src/z.py")]));
            // pins: issue #600 — the former scan visited C × F targets.
            assert_eq!(
                stats.target_path_visits, target_count,
                "C={count}, F={target_count}"
            );
            assert_eq!(stats.membership_queries, count);
            assert_eq!(stats.contexts, 1);
        }
    }
}

#[tokio::test]
async fn target_membership_preserves_path_component_equality() {
    let project = super::tests::Project::new();
    let (source, records) = candidates("src/calc.py", 1, 0);
    std::fs::write(project.root.join("src/calc.py"), source).unwrap();
    let map = records.iter().map(|c| (c.id.as_str(), c)).collect();
    let ids = records.iter().map(|c| c.id.clone()).collect();
    let selected = Ok(BTreeSet::from([Utf8PathBuf::from("src/calc.py")]));
    let unselected =
        Err("plan.candidate.invalid: candidate target is not selected: src/calc.py".to_owned());
    for (spelling, expected, expected_contexts) in [
        ("src/calc.py", selected.clone(), 1),
        ("src//calc.py", selected.clone(), 1),
        ("src/./calc.py", selected.clone(), 1),
        ("src/calc.py/", selected, 1),
        ("src/Calc.py", unselected.clone(), 0),
        ("./src/calc.py", unselected.clone(), 0),
        ("src/other/../calc.py", unselected, 0),
    ] {
        let targets = [TargetSlice {
            path: spelling.into(),
            lines: vec![],
            symbols: vec![],
        }];
        let mut stats = ValidationStats::default();
        let result = validate_requested_descriptors(
            &map,
            &ids,
            &project
                .config()
                .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
                .unwrap(),
            &targets,
            Some(&mut stats),
        )
        .await;
        assert_eq!(
            result.map_err(|error| error.to_string()),
            expected,
            "{spelling}"
        );
        assert_eq!(stats.contexts, expected_contexts, "{spelling}");
    }
    let targets = ["src/calc.py", "src/./calc.py", "src//calc.py"].map(|path| TargetSlice {
        path: path.into(),
        lines: vec![],
        symbols: vec![],
    });
    let mut stats = ValidationStats::default();
    assert_eq!(
        validate_requested_descriptors(
            &map,
            &ids,
            &project
                .config()
                .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
                .unwrap(),
            &targets,
            Some(&mut stats),
        )
        .await
        .unwrap(),
        BTreeSet::from([Utf8PathBuf::from("src/calc.py")])
    );
    assert_eq!(stats.contexts, 1);
}

#[derive(Clone, Copy, Debug)]
struct MembershipFailure {
    name: &'static str,
    present: bool,
    selected: bool,
    readable: bool,
    original: &'static str,
    contexts: usize,
}

const MEMBERSHIP_FAILURES: [MembershipFailure; 5] = [
    MembershipFailure {
        name: "unknown",
        present: false,
        selected: false,
        readable: false,
        original: "+",
        contexts: 0,
    },
    MembershipFailure {
        name: "unselected",
        present: true,
        selected: false,
        readable: false,
        original: "+",
        contexts: 0,
    },
    MembershipFailure {
        name: "read",
        present: true,
        selected: true,
        readable: false,
        original: "+",
        contexts: 0,
    },
    MembershipFailure {
        name: "descriptor",
        present: true,
        selected: true,
        readable: true,
        original: "?",
        contexts: 1,
    },
    MembershipFailure {
        name: "stable",
        present: true,
        selected: true,
        readable: true,
        original: "+",
        contexts: 1,
    },
];

fn membership_failure_records(
    project: &super::tests::Project,
    id: &str,
    path: &str,
    failure: MembershipFailure,
    records: &mut Vec<MutationCandidate>,
    targets: &mut Vec<TargetSlice>,
) {
    let (source, mut candidates) = candidates(path, 1, 0);
    let mut candidate = candidates.remove(0);
    candidate.id = id.into();
    candidate.original = failure.original.into();
    if failure.readable {
        std::fs::write(project.root.join(path), source).unwrap();
    }
    if failure.selected {
        targets.push(TargetSlice {
            path: path.into(),
            lines: vec![],
            symbols: vec![],
        });
    }
    if failure.present {
        records.push(candidate);
    }
}

fn membership_failure_message(
    _project: &super::tests::Project,
    id: &str,
    path: &str,
    failure: MembershipFailure,
) -> String {
    match failure.name {
        "unknown" => format!("plan.candidate.invalid: candidate id is not in the plan: {id}"),
        "unselected" => format!("plan.candidate.invalid: candidate target is not selected: {path}"),
        "read" => format!("plan.candidate.invalid: {path}: root-relative file was not found"),
        "descriptor" => {
            "plan.candidate.invalid: candidate original text does not match the source span".into()
        }
        "stable" => format!("plan.candidate.invalid: candidate stable id differs for {id}"),
        _ => unreachable!("closed membership failure fixtures"),
    }
}

#[tokio::test]
async fn target_membership_preserves_mixed_error_precedence() {
    for first in MEMBERSHIP_FAILURES {
        for second in MEMBERSHIP_FAILURES {
            let project = super::tests::Project::new();
            let mut records = Vec::new();
            let mut targets = Vec::new();
            membership_failure_records(
                &project,
                "a",
                "src/a.py",
                first,
                &mut records,
                &mut targets,
            );
            membership_failure_records(
                &project,
                "b",
                "src/b.py",
                second,
                &mut records,
                &mut targets,
            );
            let map = records.iter().map(|c| (c.id.as_str(), c)).collect();
            let ids = BTreeSet::from(["a".into(), "b".into()]);
            let expected = membership_failure_message(&project, "a", "src/a.py", first);
            let mut stats = ValidationStats::default();

            let error = validate_requested_descriptors(
                &map,
                &ids,
                &project
                    .config()
                    .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
                    .unwrap(),
                &targets,
                Some(&mut stats),
            )
            .await
            .unwrap_err()
            .to_string();

            assert_eq!(error, expected, "{first:?}/{second:?}");
            assert_eq!(stats.contexts, first.contexts, "{first:?}/{second:?}");
        }
    }
}

#[tokio::test]
async fn target_membership_preserves_errors_between_cached_file_results() {
    for later_original in ["?", "+"] {
        for middle_failure in MEMBERSHIP_FAILURES {
            let project = super::tests::Project::new();
            let (source, mut records) = candidates("src/a.py", 2, 0);
            std::fs::write(project.root.join("src/a.py"), source).unwrap();
            let first_id = records[0].id.clone();
            let middle_id = format!("{first_id}a");
            let later_id = format!("{first_id}z");
            records[1].id = later_id.clone();
            records[1].original = later_original.into();
            let mut targets = vec![TargetSlice {
                path: "src/a.py".into(),
                lines: vec![],
                symbols: vec![],
            }];
            membership_failure_records(
                &project,
                &middle_id,
                "src/b.py",
                middle_failure,
                &mut records,
                &mut targets,
            );
            for candidate in records
                .iter_mut()
                .filter(|candidate| candidate.id == middle_id && middle_failure.original == "?")
            {
                candidate.original = "+".into();
                candidate.span.start = 100;
            }
            let map = records.iter().map(|c| (c.id.as_str(), c)).collect();
            let ids = BTreeSet::from([first_id, middle_id.clone(), later_id]);
            let expected =
                membership_failure_message(&project, &middle_id, "src/b.py", middle_failure)
                    .replace(
                        "candidate original text does not match the source span",
                        "candidate span is outside the source",
                    );
            let mut stats = ValidationStats::default();

            let error = validate_requested_descriptors(
                &map,
                &ids,
                &project
                    .config()
                    .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
                    .unwrap(),
                &targets,
                Some(&mut stats),
            )
            .await
            .unwrap_err()
            .to_string();

            assert_eq!(error, expected, "{middle_failure:?}/{later_original}");
            assert_eq!(stats.contexts, 1 + middle_failure.contexts);
        }
    }
}

// A separate process gives a regressed blocking FIFO open a bounded watchdog.
// stdin/stdout provide the synchronization; elapsed time is only a failure guard.
#[cfg(unix)]
fn assert_resolved_source_swap_rejected(consumer: &str, replacement: &str) {
    use std::io::{BufRead, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    let project = super::tests::Project::new();
    let (source, _) = candidates("src/calc.py", 1, 0);
    std::fs::write(project.root.join("src/calc.py"), source).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "plan::validation_tests::safe_source_swap_probe",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("HOIMIN_SOURCE_SWAP_ROOT", &project.root)
        .env("HOIMIN_SOURCE_SWAP_CONSUMER", consumer)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let stdout = child.stdout.take().unwrap();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let output_reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let line = line.unwrap();
            if line.contains("HOIMIN_TARGET_RESOLVED") {
                ready_tx.send(()).unwrap();
            }
        }
    });
    let mut stdin = child.stdin.take().unwrap();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let waiter =
        std::thread::spawn(move || done_tx.send(child.wait_with_output().unwrap()).unwrap());
    let ready = ready_rx.recv_timeout(Duration::from_secs(10));
    if ready.is_ok() {
        let path = project.root.join("src/calc.py");
        std::fs::remove_file(&path).unwrap();
        match replacement {
            "fifo" => assert!(
                Command::new("mkfifo")
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            ),
            "symlink_fifo" => {
                let fifo = project.root.join("fifo.py");
                assert!(
                    Command::new("mkfifo")
                        .arg(&fifo)
                        .status()
                        .unwrap()
                        .success()
                );
                std::os::unix::fs::symlink(&fifo, &path).unwrap();
            }
            "symlink" => {
                // Even a regular-file target must not be followed after resolution.
                std::fs::write(project.root.join("outside.py"), "x = 1 + 2\n").unwrap();
                std::os::unix::fs::symlink(project.root.join("outside.py"), &path).unwrap();
            }
            _ => unreachable!(),
        }
        stdin.write_all(b"resume\n").unwrap();
    }
    drop(stdin);
    let output = match done_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(output) => output,
        Err(error) => {
            // SAFETY: pid belongs to this test's live child and is not reaped yet.
            unsafe {
                libc::kill(i32::try_from(pid).unwrap(), libc::SIGKILL);
            }
            let output = done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            waiter.join().unwrap();
            output_reader.join().unwrap();
            panic!(
                "{consumer}/{replacement} did not reject the resolved swap: {error}; {:?}",
                output.status
            );
        }
    };
    waiter.join().unwrap();
    output_reader.join().unwrap();
    assert!(ready.is_ok(), "child did not resolve target: {ready:?}");
    assert!(
        output.status.success(),
        "{consumer}/{replacement}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
macro_rules! resolved_swap_cases {
    ($($name:ident: $consumer:literal, $replacement:literal;)*) => {
        $(#[test] fn $name() { assert_resolved_source_swap_rejected($consumer, $replacement); })*
    };
}

#[cfg(unix)]
resolved_swap_cases! {
    source_hash_rejects_resolved_fifo_swap: "hash", "fifo";
    source_hash_rejects_resolved_symlink_swap: "hash", "symlink";
    source_hash_rejects_resolved_symlink_fifo_swap: "hash", "symlink_fifo";
    descriptor_validation_rejects_resolved_fifo_swap: "validate", "fifo";
    descriptor_validation_rejects_resolved_symlink_swap: "validate", "symlink";
    descriptor_validation_rejects_resolved_symlink_fifo_swap: "validate", "symlink_fifo";
}

#[cfg(unix)]
#[tokio::test]
async fn safe_source_swap_probe() {
    use std::io::{BufRead, Write};
    let Ok(root) = std::env::var("HOIMIN_SOURCE_SWAP_ROOT") else {
        return;
    };
    let config = hoimin_core::RunConfig::try_from(hoimin_core::RawRunConfig {
        root: root.into(),
        files: vec!["src/calc.py".into()],
        test_argv: vec![hoimin_core::CommandArg::Unix(b"unused".to_vec())],
        ..hoimin_core::RawRunConfig::default()
    })
    .unwrap()
    .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
    .unwrap();
    let (_, records) = candidates("src/calc.py", 1, 0);
    let targets = crate::target::TargetHandler::resolve(&config.selection)
        .await
        .unwrap();
    println!("HOIMIN_TARGET_RESOLVED");
    std::io::stdout().flush().unwrap();
    let mut resume = String::new();
    std::io::stdin().lock().read_line(&mut resume).unwrap();
    assert_eq!(resume, "resume\n");

    let error = if std::env::var("HOIMIN_SOURCE_SWAP_CONSUMER").unwrap() == "hash" {
        super::source_records(&config.selection.root, &targets)
            .await
            .unwrap_err()
    } else {
        let map = records.iter().map(|c| (c.id.as_str(), c)).collect();
        let ids = records.iter().map(|c| c.id.clone()).collect();
        validate_requested_descriptors(&map, &ids, &config, &targets, None)
            .await
            .unwrap_err()
    };

    assert!(matches!(
        error,
        PlanError::SourceRead(_) | PlanError::CandidateInvalid(_)
    ));
    assert!(error.to_string().contains("src/calc.py"), "{error}");
}

#[tokio::test]
async fn source_records_preserve_sorted_hashes_and_duplicate_paths() {
    let project = super::tests::Project::new();
    std::fs::write(project.root.join("src/a.py"), b"a\n").unwrap();
    std::fs::write(project.root.join("src/z.py"), b"z\n").unwrap();
    let targets = ["src/z.py", "src/a.py", "src/z.py"].map(|path| TargetSlice {
        path: path.into(),
        lines: vec![],
        symbols: vec![],
    });

    let records = super::source_records(&project.root, &targets)
        .await
        .unwrap();

    assert_eq!(
        records,
        fingerprint_records(&[
            ("src/a.py", &blake3::hash(b"a\n").to_hex()),
            ("src/z.py", &blake3::hash(b"z\n").to_hex()),
        ])
    );
}

#[tokio::test]
async fn descriptor_validation_rejects_source_changed_after_hashing() {
    let project = super::tests::Project::new();
    let (source, records) = candidates("src/calc.py", 1, 0);
    std::fs::write(project.root.join("src/calc.py"), source).unwrap();
    let targets = [TargetSlice {
        path: "src/calc.py".into(),
        lines: vec![],
        symbols: vec![],
    }];
    super::source_records(&project.root, &targets)
        .await
        .unwrap();
    std::fs::write(project.root.join("src/calc.py"), b"x = 1 + 3\n").unwrap();

    let error = check(&project, &records, &mut ValidationStats::default())
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "plan.candidate.invalid: candidate file hash does not match the source"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn source_reader_reuses_its_root_capability_for_hash_and_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().join("root")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("first.py"), b"first\n").unwrap();
    std::fs::write(root.join("second.py"), b"original\n").unwrap();
    let mut reader = super::source::SourceReader::new(&root);
    assert_eq!(
        reader
            .hash(camino::Utf8Path::new("first.py"))
            .await
            .unwrap(),
        blake3::hash(b"first\n")
    );
    std::fs::rename(&root, directory.path().join("held")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("second.py"), b"replacement\n").unwrap();

    let source = reader
        .read(camino::Utf8Path::new("second.py"))
        .await
        .unwrap();

    assert_eq!(source, b"original\n");
}

async fn issue_1081_asset_plan(
    fingerprint_count: usize,
    asset_count: usize,
    asset_bytes: usize,
) -> (
    super::tests::Project,
    super::PlanManifest,
    tempfile::NamedTempFile,
) {
    let project = super::tests::Project::new();
    for index in 0..asset_count {
        std::fs::write(
            project.root.join(format!("asset-{index}.bin")),
            vec![42; asset_bytes],
        )
        .unwrap();
    }
    std::fs::write(project.root.join("input.txt"), "fingerprint input").unwrap();
    let mut data = project.config().into_data();
    if fingerprint_count == 1 {
        data.fingerprint_files = vec!["input.txt".into()];
    }
    let output = super::create(
        hoimin_core::RunConfig::try_from(data).unwrap(),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    let artifact = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        artifact.path(),
        serde_json::to_vec(&output.manifest).unwrap(),
    )
    .unwrap();
    (project, output.manifest, artifact)
}

fn issue_1081_selection(
    manifest: &super::PlanManifest,
    top_one: bool,
) -> (Vec<String>, crate::cli::VerifySelection) {
    if top_one {
        let ids = vec![manifest.candidates[0].id.clone()];
        (
            ids,
            crate::cli::VerifySelection::Top {
                count: std::num::NonZeroUsize::new(1).unwrap(),
                policy: crate::cli::TopSelectionPolicy::Strict,
                offset: 0,
            },
        )
    } else {
        let ids: Vec<_> = manifest
            .candidates
            .iter()
            .take(2)
            .rev()
            .map(|candidate| candidate.id.clone())
            .collect();
        (ids.clone(), crate::cli::VerifySelection::CandidateIds(ids))
    }
}

#[tokio::test]
async fn issue_1081_prepare_and_preview_do_not_read_or_hash_unrelated_assets() {
    for fingerprint_count in [0, 1] {
        for (asset_count, asset_bytes) in [(1, 4096), (4, 16384), (16, 65536)] {
            let (_project, manifest, artifact) =
                issue_1081_asset_plan(fingerprint_count, asset_count, asset_bytes).await;
            let (ids, selection) = issue_1081_selection(&manifest, asset_count == 4);
            crate::workspace::reset_source_content_metrics();
            let prepared = super::prepare_verify_selection(artifact.path(), &selection)
                .await
                .unwrap();
            let metrics = crate::workspace::source_content_metrics();
            assert_eq!(
                (metrics.read_bytes, metrics.hash_bytes),
                (0, 0),
                "prepare F={fingerprint_count}, assets={asset_count}x{asset_bytes}: {metrics:?}"
            );
            assert_eq!(
                prepared.selection.candidate_ids().unwrap(),
                &ids.iter().cloned().collect::<BTreeSet<_>>()
            );
            let expected_preview_ids = if asset_count == 4 {
                assert_eq!(prepared.selection.ordered_ids().unwrap(), ids);
                ids.clone()
            } else {
                assert!(prepared.selection.ordered_ids().is_none());
                let mut selected: Vec<_> = manifest
                    .candidates
                    .iter()
                    .filter(|candidate| ids.contains(&candidate.id))
                    .collect();
                selected.sort_by_key(|candidate| candidate.sequence);
                selected
                    .into_iter()
                    .map(|candidate| candidate.id.clone())
                    .collect()
            };
            assert_eq!(
                prepared.fingerprint_copy_inputs,
                if fingerprint_count == 0 {
                    BTreeSet::new()
                } else {
                    BTreeSet::from(["input.txt".into()])
                }
            );
            crate::workspace::reset_source_content_metrics();
            let preview = super::prepare_verify_preview_selection(artifact.path(), &selection)
                .await
                .unwrap();
            let metrics = crate::workspace::source_content_metrics();
            assert_eq!(
                (metrics.read_bytes, metrics.hash_bytes),
                (0, 0),
                "preview F={fingerprint_count}, assets={asset_count}x{asset_bytes}: {metrics:?}"
            );
            let mut rendered = Vec::new();
            preview.write(&mut rendered).unwrap();
            let rendered: serde_json::Value = serde_json::from_slice(&rendered).unwrap();
            assert_eq!(
                rendered["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|candidate| candidate["id"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                expected_preview_ids
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[tokio::test]
async fn issue_1081_membership_matches_content_manifest_selection() {
    for (includes, excludes) in [
        (vec![], vec![]),
        (vec!["ignored/**"], vec![]),
        (vec!["ignored/**"], vec!["**/excluded.txt"]),
    ] {
        let project = super::tests::Project::new();
        std::fs::create_dir_all(project.root.join("ignored/nested")).unwrap();
        std::fs::write(project.root.join(".gitignore"), "ignored/\n").unwrap();
        for path in [
            "plain.txt",
            "ignored/nested/kept.txt",
            "ignored/nested/excluded.txt",
        ] {
            std::fs::write(project.root.join(path), path).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("plain.txt", project.root.join("link.txt")).unwrap();
            // Non-regular entries must not be admitted or opened as files.
            let listener =
                std::os::unix::net::UnixListener::bind(project.root.join("socket")).unwrap();
            drop(listener);
        }
        let mut data = project.config().into_data();
        data.selection.includes = includes.iter().map(|value| (*value).into()).collect();
        data.selection.excludes = excludes.iter().map(|value| (*value).into()).collect();
        data.fingerprint_files = vec![
            "plain.txt".into(),
            "ignored/nested/kept.txt".into(),
            "ignored/nested/excluded.txt".into(),
        ];
        let options = crate::workspace::CopyOptions {
            includes: data.selection.includes.clone(),
            excludes: data.selection.excludes.clone(),
            literal_exclusions: vec![],
        };
        let legacy = crate::workspace::build_validation_manifest(&project.root, &options).unwrap();
        let output = super::create(
            hoimin_core::RunConfig::try_from(data).unwrap(),
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
        )
        .await
        .unwrap();
        let expected: BTreeSet<_> = output
            .manifest
            .normalized_config
            .fingerprint_inputs
            .iter()
            .filter(|record| legacy.entry(&record.path).is_some())
            .map(|record| record.path.clone())
            .collect();
        let artifact = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            artifact.path(),
            serde_json::to_vec(&output.manifest).unwrap(),
        )
        .unwrap();
        let prepared =
            super::prepare_verify(artifact.path(), &[output.manifest.candidates[0].id.clone()])
                .await
                .unwrap();
        assert_eq!(prepared.fingerprint_copy_inputs, expected);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn issue_1081_empty_fingerprints_skip_copy_selection_walk() {
    let project = super::tests::Project::new();
    let output = super::create(
        project.config(),
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(output.manifest.normalized_config.fingerprint_inputs, []);
    let artifact = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        artifact.path(),
        serde_json::to_vec(&output.manifest).unwrap(),
    )
    .unwrap();
    // A copy walk must reject this selected path; no fingerprint membership is needed.
    std::fs::write(project.root.join("native\\name.txt"), b"asset").unwrap();
    assert!(matches!(
        crate::workspace::build_validation_manifest(
            &project.root,
            &crate::workspace::CopyOptions::default()
        ),
        Err(crate::workspace::WorkspaceError::InvalidPath { .. })
    ));
    let selection =
        crate::cli::VerifySelection::CandidateIds(vec![output.manifest.candidates[0].id.clone()]);
    assert!(
        super::prepare_verify_selection(artifact.path(), &selection)
            .await
            .unwrap()
            .fingerprint_copy_inputs
            .is_empty()
    );
    super::prepare_verify_preview_selection(artifact.path(), &selection)
        .await
        .unwrap();
}
