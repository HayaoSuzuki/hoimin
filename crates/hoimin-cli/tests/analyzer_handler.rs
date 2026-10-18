use camino::Utf8PathBuf;
use hoimin_cli::analyzer::{AnalyzerHandler, CandidateStore, StoreError, discover_targets};
use hoimin_core::{
    AnalyzeFile, ByteSpan, CandidateCursor, EffectId, MutationCandidate, MutationOperatorSelection,
    MutationProfile, TargetSlice,
};
use hoimin_python::{AnalyzerDiagnostic, AnalyzerDiagnosticCode};
use std::{fs, num::NonZeroUsize};

use proptest::prelude::*;

fn candidate(sequence: u64) -> MutationCandidate {
    MutationCandidate {
        id: format!("m1_{sequence}"),
        sequence,
        path: Utf8PathBuf::from("pkg/calc.py"),
        span: ByteSpan {
            start: sequence * 3,
            length: 1,
        },
        original: "+".into(),
        replacement: "-".into(),
        operator: "binary_add_sub".into(),
        line: 1,
        column: u32::try_from(sequence).unwrap(),
        symbol: None,
        file_hash: "00".repeat(32),
    }
}

fn analysis_request(id: u64, path: &str, final_target: bool) -> AnalyzeFile {
    AnalyzeFile {
        id: EffectId(id),
        target: TargetSlice {
            path: path.into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
        final_target,
    }
}

fn handler(root: Utf8PathBuf, max_candidates: usize) -> AnalyzerHandler {
    AnalyzerHandler::new(
        root,
        MutationOperatorSelection::default(),
        MutationProfile::Full,
        NonZeroUsize::new(max_candidates).unwrap(),
    )
}

#[cfg(unix)]
fn create_dir_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_dir_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    match std::os::windows::fs::symlink_dir(target, link) {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.kind() == std::io::ErrorKind::Unsupported
                || error.raw_os_error() == Some(1314) =>
        {
            let output = std::process::Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()?;
            if output.status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(format!(
                    "failed to create Windows test junction: {}",
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        }
        Err(error) => Err(error),
    }
}

fn two_target_fixture() -> (
    tempfile::TempDir,
    Utf8PathBuf,
    Vec<TargetSlice>,
    MutationOperatorSelection,
) {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/first.py"),
        "def first():\n    return 1 + 2\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/second.py"),
        "def second():\n    return 3 + 4\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let targets = ["src/first.py", "src/second.py"]
        .into_iter()
        .map(|path| TargetSlice {
            path: path.into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        })
        .collect();
    (
        directory,
        root,
        targets,
        MutationOperatorSelection::default(),
    )
}

async fn replay_runtime_candidates(
    root: Utf8PathBuf,
    targets: Vec<TargetSlice>,
    operators: MutationOperatorSelection,
    profile: MutationProfile,
) -> Vec<MutationCandidate> {
    let mut handler =
        AnalyzerHandler::new(root, operators, profile, NonZeroUsize::new(100).unwrap());
    let target_count = targets.len();
    let mut spool = None;
    for (index, target) in targets.into_iter().enumerate() {
        let finished = handler
            .handle(AnalyzeFile {
                id: EffectId(u64::try_from(index).unwrap() + 100),
                target,
                final_target: index + 1 == target_count,
            })
            .await
            .unwrap();
        if finished.spool.is_some() {
            spool = finished.spool;
        }
    }
    let spool = spool.expect("the final target returns the candidate spool");
    let mut candidates = Vec::new();
    let mut cursor = CandidateCursor::START;
    while let Some((candidate, next_cursor)) = CandidateStore::replay_one(&spool, cursor).unwrap() {
        candidates.push(candidate);
        cursor = next_cursor;
    }
    candidates
}

#[tokio::test]
async fn in_memory_discovery_matches_runtime_candidate_descriptors() {
    let (_directory, root, targets, operators) = two_target_fixture();

    let planned = discover_targets(&root, &targets, &operators, MutationProfile::Focused, 100)
        .await
        .unwrap();
    let runtime =
        replay_runtime_candidates(root, targets, operators, MutationProfile::Focused).await;

    assert_eq!(planned.candidates, runtime);
    assert!(!planned.truncated);
}

#[tokio::test]
async fn analyzer_rejects_replaced_source_parent() {
    let project = tempfile::tempdir().unwrap();
    let source_parent = project.path().join("src");
    fs::create_dir(&source_parent).unwrap();
    fs::write(
        source_parent.join("calc.py"),
        "def selected():\n    return 1 + 2\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap();
    let mut analyzer = handler(root, 10);
    let original_parent = project.path().join("original-src");
    fs::rename(&source_parent, &original_parent).unwrap();

    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    fs::write(&sentinel, b"outside sentinel").unwrap();
    fs::write(
        outside.path().join("calc.py"),
        "def outside_secret():\n    return left == right\n",
    )
    .unwrap();
    create_dir_link(outside.path(), &source_parent).unwrap();

    let result = analyzer
        .handle(analysis_request(82, "src/calc.py", true))
        .await;

    assert!(matches!(
        result,
        Err(error)
            if error.id == EffectId(82) && error.failure.code() == "analyzer.source.read"
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"outside sentinel");
}

#[tokio::test]
async fn discover_targets_rejects_linked_source_parent() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    fs::write(&sentinel, b"outside sentinel").unwrap();
    fs::write(
        outside.path().join("calc.py"),
        "def outside_secret():\n    return left == right\n",
    )
    .unwrap();
    let link = project.path().join("src");
    create_dir_link(outside.path(), &link).unwrap();
    let root = Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap();
    let targets = vec![TargetSlice {
        path: "src/calc.py".into(),
        lines: Vec::new(),
        symbols: Vec::new(),
    }];

    let result = discover_targets(
        &root,
        &targets,
        &MutationOperatorSelection::default(),
        MutationProfile::Full,
        10,
    )
    .await;

    assert!(matches!(
        result,
        Err(error)
            if error.id == EffectId(0) && error.failure.code() == "analyzer.source.read"
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"outside sentinel");
}

#[tokio::test]
async fn in_memory_discovery_stops_when_a_later_target_exceeds_the_global_limit() {
    let (_directory, root, targets, _) = two_target_fixture();
    // One arithmetic candidate per target reaches the limit on the second file.
    let operators = serde_json::from_value(serde_json::json!(["binary_add_sub"])).unwrap();

    let discovery = discover_targets(&root, &targets, &operators, MutationProfile::Focused, 1)
        .await
        .unwrap();

    assert_eq!(
        discovery
            .candidates
            .iter()
            .map(|candidate| candidate.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/first.py"]
    );
    assert!(discovery.truncated);
    assert_eq!(
        discovery.diagnostics,
        vec![AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::CandidateLimitExceeded,
            path: Some("src/second.py".into()),
            line: None,
            column: None,
            message: None,
        }]
    );
}

#[test]
fn candidate_store_enforces_limit_without_retaining_records() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    assert!(matches!(
        store.push(&candidate(3)),
        Err(StoreError::LimitExceeded { limit: 2 })
    ));
    assert_eq!(store.count(), 2);
}

#[test]
fn candidate_store_spool_follows_the_caller_owned_directory_lifetime() {
    let owner = tempfile::tempdir().unwrap();
    let mut store = CandidateStore::new_in(1, owner.path()).unwrap();
    store.push(&candidate(1)).unwrap();
    let reference = store.finish().unwrap();
    let spool_path = std::path::PathBuf::from(&reference.token);

    assert!(spool_path.starts_with(owner.path()));
    assert!(spool_path.is_file());

    drop(owner);
    assert!(!spool_path.exists());
}

#[test]
fn replays_in_stable_offset_order() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let (first, second_cursor) = CandidateStore::replay_one(&reference, CandidateCursor::START)
        .unwrap()
        .unwrap();
    let (second, end_cursor) = CandidateStore::replay_one(&reference, second_cursor)
        .unwrap()
        .unwrap();
    assert_eq!((first, second), (candidate(1), candidate(2)));
    assert_eq!(
        CandidateStore::replay_one(&reference, end_cursor).unwrap(),
        None
    );
}

fn spool_lines(reference: &hoimin_core::CandidateSpoolRef) -> Vec<String> {
    fs::read_to_string(&reference.token)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn replay_rejects_duplicate_or_reversed_sequence() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let lines = spool_lines(&reference);

    fs::write(&reference.token, format!("{}\n{}\n", lines[0], lines[0])).unwrap();
    let (_, next) = CandidateStore::replay_one(&reference, CandidateCursor::START)
        .unwrap()
        .unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, next),
        Err(StoreError::InvalidSequence {
            expected: 2,
            actual: 1
        })
    ));

    fs::write(&reference.token, format!("{}\n{}\n", lines[1], lines[0])).unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, CandidateCursor::START),
        Err(StoreError::InvalidSequence {
            expected: 1,
            actual: 2
        })
    ));
}

#[test]
fn replay_rejects_zero_expected_sequence() {
    let mut store = CandidateStore::new(1).unwrap();
    store.push(&candidate(1)).unwrap();
    let reference = store.finish().unwrap();

    assert!(matches!(
        CandidateStore::replay_one(
            &reference,
            CandidateCursor {
                offset: 0,
                expected_sequence: 0,
            },
        ),
        Err(StoreError::InvalidSequence {
            expected: 1,
            actual: 0,
        })
    ));
}

#[test]
fn replay_rejects_offsets_outside_record_boundaries() {
    let mut store = CandidateStore::new(1).unwrap();
    store.push(&candidate(1)).unwrap();
    let reference = store.finish().unwrap();
    let length = fs::metadata(&reference.token).unwrap().len();

    for offset in [1, length + 1] {
        assert!(matches!(
            CandidateStore::replay_one(
                &reference,
                CandidateCursor {
                    offset,
                    expected_sequence: 1,
                },
            ),
            Err(StoreError::InvalidOffset { offset: actual }) if actual == offset
        ));
    }
}

#[test]
fn replay_rejects_a_cursor_sequence_mismatched_at_a_valid_boundary() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let (_, second_cursor) = CandidateStore::replay_one(&reference, CandidateCursor::START)
        .unwrap()
        .unwrap();

    assert!(matches!(
        CandidateStore::replay_one(
            &reference,
            CandidateCursor {
                expected_sequence: 1,
                ..second_cursor
            },
        ),
        Err(StoreError::InvalidSequence {
            expected: 1,
            actual: 2,
        })
    ));
}

#[test]
fn replay_rejects_an_eof_cursor_past_the_declared_record_count() {
    let mut store = CandidateStore::new(1).unwrap();
    store.push(&candidate(1)).unwrap();
    let reference = store.finish().unwrap();
    let length = fs::metadata(&reference.token).unwrap().len();

    assert!(matches!(
        CandidateStore::replay_one(
            &reference,
            CandidateCursor {
                offset: length,
                expected_sequence: 3,
            },
        ),
        Err(StoreError::InvalidSequence {
            expected: 1,
            actual: 2,
        })
    ));
}

#[test]
fn replay_rejects_sequence_gap() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let first = spool_lines(&reference).remove(0);
    let third = serde_json::to_string(&candidate(3)).unwrap();
    fs::write(&reference.token, format!("{first}\n{third}\n")).unwrap();

    let (_, next) = CandidateStore::replay_one(&reference, CandidateCursor::START)
        .unwrap()
        .unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, next),
        Err(StoreError::InvalidSequence {
            expected: 2,
            actual: 3
        })
    ));
}

#[test]
fn replay_rejects_early_eof_before_reference_record_count() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let first = spool_lines(&reference).remove(0);
    fs::write(&reference.token, format!("{first}\n")).unwrap();

    let (_, end) = CandidateStore::replay_one(&reference, CandidateCursor::START)
        .unwrap()
        .unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, end),
        Err(StoreError::UnexpectedEof {
            expected_records: 2,
            actual_records: 1
        })
    ));
}

#[test]
fn replay_rejects_truncated_record() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let lines = spool_lines(&reference);
    fs::write(&reference.token, format!("{}\n{{", lines[0])).unwrap();

    let (_, next) = CandidateStore::replay_one(&reference, CandidateCursor::START)
        .unwrap()
        .unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, next),
        Err(StoreError::UnexpectedEof {
            expected_records: 2,
            actual_records: 1
        })
    ));
}

#[tokio::test]
async fn concrete_handler_does_not_spawn_python_for_analysis() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/calc.py"),
        "def equal(left, right):\n    return left == right\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let mut handler = handler(root, 10);
    let finished = handler
        .handle(AnalyzeFile {
            id: EffectId(77),
            target: TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            },
            final_target: true,
        })
        .await
        .unwrap();
    let spool = finished.spool.unwrap();
    let (first_candidate, _) = CandidateStore::replay_one(&spool, CandidateCursor::START)
        .unwrap()
        .unwrap();

    assert_eq!(spool.records, 1);
    assert_eq!(first_candidate.original, "==");
    assert_eq!(first_candidate.replacement, "!=");
}

#[tokio::test]
async fn concrete_handler_reports_invalid_syntax_with_the_source_path() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(directory.path().join("src/broken.py"), "def broken(:\n").unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let finished = handler(root, 10)
        .handle(analysis_request(78, "src/broken.py", true))
        .await
        .unwrap();

    assert_eq!(finished.diagnostics.len(), 1);
    assert_eq!(finished.diagnostics[0].code, "analyzer.invalid_syntax");
    assert!(
        finished.diagnostics[0].message.contains("src/broken.py"),
        "{}",
        finished.diagnostics[0].message
    );
    assert!(
        finished.diagnostics[0]
            .message
            .contains("source could not be parsed"),
        "{}",
        finished.diagnostics[0].message
    );
}

#[tokio::test]
async fn concrete_handler_reports_source_read_failure() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let result = handler(root, 10)
        .handle(analysis_request(78, "src/missing.py", true))
        .await;

    assert!(matches!(
        result,
        Err(error) if error.id == EffectId(78) && error.failure.code() == "analyzer.source.read"
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn concrete_handler_rejects_colon_named_source_before_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let path = "target:sample.py";
    std::fs::write(root.join(path), "enabled = True\n").unwrap();
    let result = handler(root, 10)
        .handle(analysis_request(78, path, true))
        .await;

    assert!(matches!(
        result,
        Err(error) if error.id == EffectId(78) && error.failure.code() == "analyzer.source.read"
    ));
}

#[tokio::test]
async fn concrete_handler_truncates_at_candidate_limit() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/calc.py"),
        "first = left == right\nsecond = top == bottom\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let finished = handler(root, 1)
        .handle(analysis_request(79, "src/calc.py", true))
        .await
        .unwrap();
    assert_eq!(finished.diagnostics.len(), 1);
    assert_eq!(finished.diagnostics[0].code, "analyzer.candidate_limit");
    assert!(
        finished.diagnostics[0].message.contains("src/calc.py"),
        "{}",
        finished.diagnostics[0].message
    );
    let spool = finished.spool.unwrap();
    let (candidate, cursor) = CandidateStore::replay_one(&spool, CandidateCursor::START)
        .unwrap()
        .unwrap();

    assert!(finished.truncated);
    assert_eq!(spool.records, 1);
    assert_eq!(candidate.sequence, 1);
    assert_eq!(CandidateStore::replay_one(&spool, cursor).unwrap(), None);
}

#[tokio::test]
async fn concrete_handler_finishes_spool_when_a_non_final_target_is_truncated() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/calc.py"),
        "first = left == right\nsecond = top == bottom\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let finished = handler(root, 1)
        .handle(analysis_request(80, "src/calc.py", false))
        .await
        .unwrap();
    let spool = finished.spool.unwrap();

    assert!(finished.truncated);
    assert_eq!(spool.records, 1);
}

#[tokio::test]
async fn concrete_handler_spools_multiple_requests_on_final_target() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/first.py"),
        "first = left == right\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/second.py"),
        "second = top == bottom\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let mut handler = handler(root, 10);

    let first = handler
        .handle(analysis_request(80, "src/first.py", false))
        .await
        .unwrap();
    let second = handler
        .handle(analysis_request(81, "src/second.py", true))
        .await
        .unwrap();
    let spool = second.spool.unwrap();
    let (first_candidate, second_cursor) =
        CandidateStore::replay_one(&spool, CandidateCursor::START)
            .unwrap()
            .unwrap();
    let (second_candidate, end_cursor) = CandidateStore::replay_one(&spool, second_cursor)
        .unwrap()
        .unwrap();

    assert_eq!(first.spool, None);
    assert_eq!(spool.records, 2);
    assert_eq!(
        (first_candidate.sequence, first_candidate.path.as_str()),
        (1, "src/first.py")
    );
    assert_eq!(
        (second_candidate.sequence, second_candidate.path.as_str()),
        (2, "src/second.py")
    );
    assert_eq!(
        CandidateStore::replay_one(&spool, end_cursor).unwrap(),
        None
    );
}

#[tokio::test]
async fn concrete_handler_finalizes_accumulated_candidates_on_an_empty_final_target() {
    // Arrange.
    let (_directory, root, _, _) = two_target_fixture();
    fs::write(root.join("src/empty.py"), "").unwrap();
    let mut handler = AnalyzerHandler::new(
        root,
        serde_json::from_value(serde_json::json!(["binary_add_sub"])).unwrap(),
        MutationProfile::Full,
        NonZeroUsize::new(10).unwrap(),
    );
    handler
        .handle(analysis_request(90, "src/first.py", false))
        .await
        .unwrap();

    // Act.
    let finished = handler
        .handle(analysis_request(91, "src/empty.py", true))
        .await
        .unwrap();
    let spool = finished.spool.unwrap();

    // Assert.
    assert_eq!(spool.records, 1);
    assert!(!finished.truncated);
    let (candidate, _) = CandidateStore::replay_one(&spool, CandidateCursor::START)
        .unwrap()
        .unwrap();
    assert_eq!(candidate.path, "src/first.py");
}

async fn assert_fresh_session(source: &str, expected_truncated: bool) {
    // Arrange: only comparison candidates are selected and each session has a one-record cap.
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("calc.py"), source).unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let operators = serde_json::from_value(serde_json::json!(["compare_eq_ne"])).unwrap();
    let mut handler = AnalyzerHandler::new(
        root,
        operators,
        MutationProfile::Full,
        NonZeroUsize::new(1).unwrap(),
    );
    let first = handler
        .handle(analysis_request(92, "calc.py", true))
        .await
        .unwrap()
        .spool
        .unwrap();

    // Act.
    let second = handler
        .handle(analysis_request(93, "calc.py", true))
        .await
        .unwrap();
    let second_spool = second.spool.unwrap();
    let (candidate, cursor) = CandidateStore::replay_one(&second_spool, CandidateCursor::START)
        .unwrap()
        .unwrap();

    // Assert: the old spool is separate and the new session starts at sequence one.
    assert_ne!(first.token, second_spool.token);
    assert_eq!(first.records, 1);
    assert_eq!(second_spool.records, 1);
    assert_eq!(second.truncated, expected_truncated);
    assert_eq!(candidate.sequence, 1);
    assert_eq!(candidate.operator, "compare_eq_ne");
    assert_eq!(
        CandidateStore::replay_one(&second_spool, cursor).unwrap(),
        None
    );
}

#[tokio::test]
async fn concrete_handler_restarts_after_a_complete_final_target() {
    assert_fresh_session("a = 1 + 2\nfirst = left == right\n", false).await;
}

#[tokio::test]
async fn concrete_handler_restarts_after_truncation() {
    assert_fresh_session(
        "a = 1 + 2\nfirst = left == right\nsecond = top == bottom\n",
        true,
    )
    .await;
}

#[tokio::test]
async fn concrete_handler_retains_accumulated_candidates_after_source_read_failure() {
    // Arrange.
    let (_directory, root, _, _) = two_target_fixture();
    let mut handler = AnalyzerHandler::new(
        root,
        serde_json::from_value(serde_json::json!(["binary_add_sub"])).unwrap(),
        MutationProfile::Full,
        NonZeroUsize::new(10).unwrap(),
    );
    handler
        .handle(analysis_request(94, "src/first.py", false))
        .await
        .unwrap();

    // Act: a read failure occurs before store ownership moves to blocking work.
    let error = handler
        .handle(analysis_request(95, "src/missing.py", false))
        .await
        .unwrap_err();
    let spool = handler
        .handle(analysis_request(96, "src/second.py", true))
        .await
        .unwrap()
        .spool
        .unwrap();

    // Assert.
    assert_eq!(error.failure.code(), "analyzer.source.read");
    assert_eq!(spool.records, 2);
    let (first, cursor) = CandidateStore::replay_one(&spool, CandidateCursor::START)
        .unwrap()
        .unwrap();
    let (second, _) = CandidateStore::replay_one(&spool, cursor).unwrap().unwrap();
    assert_eq!((first.sequence, first.path.as_str()), (1, "src/first.py"));
    assert_eq!(
        (second.sequence, second.path.as_str()),
        (2, "src/second.py")
    );
}

#[tokio::test]
async fn concrete_handler_defers_project_access_until_the_first_request() {
    // Arrange: the project does not yet exist at construction.
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().join("later")).unwrap();
    let mut handler = handler(root.clone(), 10);
    fs::create_dir(&root).unwrap();
    fs::write(root.join("calc.py"), "result = left == right\n").unwrap();

    // Act.
    let finished = handler
        .handle(analysis_request(97, "calc.py", true))
        .await
        .unwrap();

    // Assert.
    assert_eq!(finished.spool.unwrap().records, 1);
}

async fn candidates_for_settings(
    operators: &[&str],
    profile: MutationProfile,
) -> Vec<MutationCandidate> {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("calc.py"),
        "if __name__ == '__main__':\n    run()\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let mut handler = AnalyzerHandler::new(
        root,
        serde_json::from_value(serde_json::json!(operators)).unwrap(),
        profile,
        NonZeroUsize::new(10).unwrap(),
    );
    let spool = handler
        .handle(analysis_request(98, "calc.py", true))
        .await
        .unwrap()
        .spool
        .unwrap();
    let mut candidates = Vec::new();
    let mut cursor = CandidateCursor::START;
    while let Some((candidate, next)) = CandidateStore::replay_one(&spool, cursor).unwrap() {
        candidates.push(candidate);
        cursor = next;
    }
    candidates
}

#[tokio::test]
async fn a_new_handler_uses_its_selected_operators() {
    // Arrange/act: two handlers own different selections for identical source text.
    let selected = candidates_for_settings(&["condition_constant"], MutationProfile::Full).await;
    let unselected = candidates_for_settings(&["binary_add_sub"], MutationProfile::Full).await;

    // Assert.
    assert_eq!(selected.len(), 2);
    assert!(
        selected
            .iter()
            .all(|candidate| candidate.operator == "condition_constant")
    );
    assert_eq!(unselected, Vec::<MutationCandidate>::new());
}

#[tokio::test]
async fn a_new_handler_uses_its_selected_profile() {
    // Arrange/act: focused analysis omits the module entry-point guard.
    let full = candidates_for_settings(&["condition_constant"], MutationProfile::Full).await;
    let focused = candidates_for_settings(&["condition_constant"], MutationProfile::Focused).await;

    // Assert.
    assert_eq!(full.len(), 2);
    assert_eq!(focused, Vec::<MutationCandidate>::new());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn fixed_limit_bounds_cross_file_sessions(
        first_count in 0_usize..5,
        second_count in 0_usize..5,
        limit in 1_usize..7,
    ) {
        // Arrange: each equality contributes exactly one selected candidate.
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
        for (path, count) in [("first.py", first_count), ("second.py", second_count)] {
            fs::write(root.join(path), "value = left == right\n".repeat(count)).unwrap();
        }
        let operators = serde_json::from_value(serde_json::json!(["compare_eq_ne"])).unwrap();
        let mut handler = AnalyzerHandler::new(
            root,
            operators,
            MutationProfile::Full,
            NonZeroUsize::new(limit).unwrap(),
        );
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();

        // Act.
        let finished = runtime.block_on(async {
            let first = handler.handle(analysis_request(100, "first.py", false)).await.unwrap();
            if first.spool.is_some() {
                first
            } else {
                handler.handle(analysis_request(101, "second.py", true)).await.unwrap()
            }
        });
        let spool = finished.spool.unwrap();
        let mut sequences = Vec::new();
        let mut cursor = CandidateCursor::START;
        while let Some((candidate, next)) = CandidateStore::replay_one(&spool, cursor).unwrap() {
            sequences.push(candidate.sequence);
            cursor = next;
        }

        // Assert: one constructor limit applies to the complete ordered target set.
        prop_assert_eq!(usize::try_from(spool.records).unwrap(), (first_count + second_count).min(limit));
        prop_assert_eq!(finished.truncated, first_count + second_count > limit);
        prop_assert_eq!(sequences, (1..=spool.records).collect::<Vec<_>>());
    }
}
