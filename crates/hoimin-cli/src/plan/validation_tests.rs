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
    project: &super::tests::Project,
    id: &str,
    path: &str,
    failure: MembershipFailure,
) -> String {
    match failure.name {
        "unknown" => format!("plan.candidate.invalid: candidate id is not in the plan: {id}"),
        "unselected" => format!("plan.candidate.invalid: candidate target is not selected: {path}"),
        "read" => format!(
            "plan.candidate.invalid: {path}: {}",
            std::fs::read(project.root.join(path)).unwrap_err()
        ),
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
