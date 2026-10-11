use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, CandidateDescriptor, CandidateIdentity, stable_mutant_id};

use super::model::*;

pub(super) fn key() -> ReviewKey {
    let descriptor = CandidateDescriptor {
        schema_version: 1,
        path: Utf8PathBuf::from("sample.py"),
        span: ByteSpan {
            start: 0,
            length: 1,
        },
        original: "1".into(),
        replacement: "2".into(),
        operator: "integer_literal".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: blake3::hash(b"1\n").to_hex().to_string(),
    };
    ReviewKey {
        project: Utf8PathBuf::from("/project"),
        candidate_id: stable_mutant_id(&CandidateIdentity::from(&descriptor)).to_string(),
        descriptor,
    }
}

pub(super) fn draft() -> ReviewDraft {
    ReviewDraft {
        assessment: Assessment::Actionable,
        reason: "required field missing".into(),
        evidence: vec![],
        timestamp_ms: 0,
        plan: ArtifactReference {
            path: "plan.json".into(),
            content_hash: "a".repeat(64),
            run_id: None,
        },
        report: None,
    }
}

#[tokio::test]
async fn post_commit_validation_failure_reports_saved_revision() {
    let (dir, path) = plan_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let mut candidate = plan.manifest.candidates[0].candidate.clone();
    let key = super::input::review_key(&plan.project, &candidate);
    let mut store =
        super::store::ReviewStore::open_write(&dir.path().join("reviews.sqlite3"), &plan.project)
            .unwrap();
    let state = store.append(&key, SourceState::Fresh, 0, draft()).unwrap();
    let revision = state.history.last().unwrap().revision;
    candidate.path = "../escaped.py".into();
    let result = super::saved_output(Some(revision), || {
        super::input::source_state(&plan.project, &candidate)
    });
    assert!(matches!(
        result,
        Err(super::ReviewError::SavedOutput { revision: 1, .. })
    ));
    assert_eq!(store.snapshot().unwrap()[0].1.history.len(), 1);
}

#[test]
fn applicability_reflects_source_state_for_matching_identity() {
    let original = key();

    for (source, expected) in [
        (SourceState::Fresh, Applicability::Current),
        (SourceState::Stale, Applicability::Stale),
        (SourceState::Unavailable, Applicability::SourceUnavailable),
    ] {
        let actual = applicability(&original, &original, source);

        assert_eq!(actual, expected, "{source:?}");
    }
}

#[test]
fn applicability_rejects_different_project_or_descriptor() {
    let original = key();
    let mut different_project = original.clone();
    different_project.project = "/another".into();
    let mut different_descriptor = original.clone();
    different_descriptor.descriptor.original = "9".into();

    for other in [different_project, different_descriptor] {
        let actual = applicability(&original, &other, SourceState::Fresh);

        assert_eq!(actual, Applicability::IdentityConflict, "{other:?}");
    }
}

#[test]
fn first_update_preserves_empty_input_and_starts_revision_one() {
    let empty = ReviewState::default();

    let first = apply_update(&empty, Applicability::Current, 0, draft()).unwrap();

    assert_eq!(first.history[0].revision, 1);
    assert_eq!(empty.history, Vec::new());
}

#[test]
fn update_rejects_conflicting_revision_and_stale_source() {
    let first = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();

    for (applicability, revision, expected) in [
        (Applicability::Current, 0, UpdateRejection::RevisionConflict),
        (Applicability::Stale, 1, UpdateRejection::NotCurrent),
    ] {
        let result = apply_update(&first, applicability, revision, draft());

        assert_eq!(result, Err(expected), "{applicability:?}");
    }
}

#[test]
fn second_update_preserves_history_and_increments_revision() {
    let first = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();

    let second = apply_update(&first, Applicability::Current, 1, draft()).unwrap();

    assert_eq!(&second.history[..1], first.history.as_slice());
    assert_eq!(second.history[1].revision, 2);
}

#[test]
fn update_rejects_revision_overflow() {
    let overflow = ReviewState {
        history: vec![ReviewEvent {
            revision: u64::MAX,
            draft: draft(),
        }],
    };

    let result = apply_update(&overflow, Applicability::Current, u64::MAX, draft());

    assert_eq!(result, Err(UpdateRejection::RevisionOverflow));
}

#[test]
fn draft_accepts_maximum_reason_and_evidence_sizes() {
    let mut value = draft();
    value.reason = "x".repeat(4096);
    value.evidence = vec!["x".repeat(2048); 8];

    let result = validate_draft(&value);

    assert!(result.is_ok());
}

#[test]
fn draft_rejects_invalid_reason_without_relying_on_other_limits() {
    for invalid in [
        "x".repeat(4097),
        " \n\t".to_string(),
        "\x1b[31m".to_string(),
        "\0".to_string(),
        "あ".repeat(1366),
    ] {
        let mut value = draft();
        value.reason = invalid;

        let result = validate_draft(&value);

        assert!(result.is_err(), "reason bytes={}", value.reason.len());
    }
}

#[test]
fn draft_rejects_excessive_evidence_count_or_item_size() {
    for evidence in [vec!["x".repeat(2048); 9], vec!["x".repeat(2049)]] {
        let mut value = draft();
        value.reason = "reason\n\tmore".into();
        value.evidence = evidence;

        let result = validate_draft(&value);

        assert!(result.is_err());
    }
}

#[test]
fn draft_accepts_multiline_reason_without_evidence() {
    let mut value = draft();
    value.reason = "reason\n\tmore".into();

    let result = validate_draft(&value);

    assert!(result.is_ok());
}

async fn plan_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.py"), "value = 1\n").unwrap();
    let args = crate::cli::parse_from([
        "hoimin",
        "plan",
        "--root",
        dir.path().to_str().unwrap(),
        "--file",
        "sample.py",
        "--min-free-space",
        "1B",
        "--",
        "must-not-execute",
    ])
    .unwrap();
    let crate::cli::ParsedCommand::Plan(args) = args else {
        panic!("plan")
    };
    let output = crate::plan::create(args.into_run_config().unwrap())
        .await
        .unwrap();
    assert_ne!(output.manifest.candidates.len(), 0);
    let path = dir.path().join("plan.json");
    std::fs::write(&path, serde_json::to_vec(&output.manifest).unwrap()).unwrap();
    (dir, path)
}

#[tokio::test]
async fn old_candidate_without_report_is_not_loaded() {
    let (_dir, path) = plan_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let key =
        super::input::review_key(&plan.project, &plan.manifest.candidates.remove(0).candidate);
    let state = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();
    let rows = super::stale_rows(&plan, &[(key, state)], None).unwrap();
    assert!(matches!(
        rows[0].execution,
        super::render::ReviewExecution::NotLoaded
    ));
}

#[tokio::test]
async fn read_only_plan_does_not_execute() {
    let (_dir, path) = plan_fixture().await;

    let loaded = super::input::load_plan(&path).unwrap();

    assert_ne!(loaded.manifest.candidates.len(), 0);
}

#[tokio::test]
async fn plan_rejects_forged_candidate_id() {
    let (_dir, path) = plan_fixture().await;
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["candidates"][0]["id"] = serde_json::Value::String("0".repeat(64));
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

    let result = super::input::load_plan(&path);

    assert!(result.is_err());
}

#[tokio::test]
async fn source_state_is_fresh_for_unchanged_file() {
    let (_dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    let candidate = &loaded.manifest.candidates[0].candidate;

    let actual = super::input::source_state(&loaded.project, candidate).unwrap();

    assert_eq!(actual, SourceState::Fresh);
}

#[tokio::test]
async fn source_state_is_stale_for_changed_file() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    let candidate = &loaded.manifest.candidates[0].candidate;
    std::fs::write(dir.path().join("sample.py"), "value = 9\n").unwrap();

    let actual = super::input::source_state(&loaded.project, candidate).unwrap();

    assert_eq!(actual, SourceState::Stale);
}

#[tokio::test]
async fn source_state_is_unavailable_for_missing_file() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    let candidate = &loaded.manifest.candidates[0].candidate;
    std::fs::remove_file(dir.path().join("sample.py")).unwrap();

    let actual = super::input::source_state(&loaded.project, candidate).unwrap();

    assert_eq!(actual, SourceState::Unavailable);
}

#[tokio::test]
async fn store_destination_rejects_plan_source_and_hard_link_alias() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    let alias = dir.path().join("alias.sqlite3");
    std::fs::hard_link(&path, &alias).unwrap();

    for destination in [path, dir.path().join("sample.py"), alias] {
        let result = super::input::validate_store_destination(&destination, &loaded, None);

        assert!(result.is_err(), "{}", destination.display());
    }
}

#[tokio::test]
async fn plan_rejects_oversized_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let oversized = dir.path().join("oversized.json");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();

    let result = super::input::load_plan(&oversized);

    assert!(result.is_err());
}

#[tokio::test]
async fn store_journal_must_not_alias_a_protected_input() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    let store = dir.path().join("reviews.sqlite3");
    std::fs::hard_link(&path, dir.path().join("reviews.sqlite3-journal")).unwrap();

    let result = super::input::validate_store_destination(&store, &loaded, None);

    assert!(result.is_err());
}

#[test]
fn malformed_artifact_reference_is_rejected() {
    let mut value = draft();
    value.plan.content_hash = "invalid".into();

    let result = validate_draft(&value);

    assert!(result.is_err());
}

async fn report_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let (dir, path) = plan_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let mut report: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/golden/reports/schema-v4-current.json"
    ))
    .unwrap();
    let config = plan
        .manifest
        .normalized_config
        .clone()
        .into_run_config(hoimin_core::OutputConfig::default());
    report["run"]["normalized_config"] = serde_json::to_value(config).unwrap();
    report["mutants"][0]["candidate"] =
        serde_json::to_value(&plan.manifest.candidates[0].candidate).unwrap();
    let report_path = dir.path().join("report.json");
    std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    (dir, path, report_path)
}

#[tokio::test]
async fn old_candidate_report_retains_matching_execution_result() {
    let (_dir, path, report_path) = report_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let (report, _) = super::input::load_report(&report_path, &plan).unwrap();
    let key =
        super::input::review_key(&plan.project, &plan.manifest.candidates.remove(0).candidate);
    let state = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();

    let rows = super::stale_rows(&plan, &[(key, state)], Some(&report)).unwrap();

    assert!(matches!(
        rows[0].execution,
        super::render::ReviewExecution::Result(hoimin_core::MutationStatus::Killed)
    ));
}

#[tokio::test]
async fn old_candidate_report_rejects_descriptor_mismatch() {
    let (_dir, path, report_path) = report_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let (report, _) = super::input::load_report(&report_path, &plan).unwrap();
    let mut key =
        super::input::review_key(&plan.project, &plan.manifest.candidates.remove(0).candidate);
    key.descriptor.original = "different".into();
    let state = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();

    let result = super::stale_rows(&plan, &[(key, state)], Some(&report));

    assert!(result.is_err());
}

#[tokio::test]
async fn review_accepts_matching_report() {
    let (_dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();

    let (report, _) = super::input::load_report(&report_path, &plan).unwrap();

    assert_eq!(report.mutants.len(), 1);
}

#[tokio::test]
async fn review_rejects_report_with_changed_identity_or_execution() {
    let changes: [(&str, &str, serde_json::Value); 6] = [
        ("incomplete", "/summary/complete", false.into()),
        ("baseline", "/baseline/termination/Exit", 1.into()),
        (
            "root",
            "/run/normalized_config/root",
            std::env::temp_dir().to_str().unwrap().into(),
        ),
        ("summary", "/summary/counts/killed", 9.into()),
        (
            "descriptor",
            "/mutants/0/candidate/original",
            "wrong".into(),
        ),
        ("version", "/schema_version", 3.into()),
    ];
    for (change, pointer, replacement) in changes {
        let (_dir, path, report_path) = report_fixture().await;
        let plan = super::input::load_plan(&path).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        *value.pointer_mut(pointer).unwrap() = replacement;
        std::fs::write(&report_path, serde_json::to_vec(&value).unwrap()).unwrap();

        let result = super::input::load_report(&report_path, &plan);

        assert!(result.is_err(), "{change}");
    }
}

#[tokio::test]
async fn review_rejects_trailing_report_document() {
    let (_dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let original = std::fs::read(&report_path).unwrap();
    std::fs::write(&report_path, [original.as_slice(), b"\n{}\n"].concat()).unwrap();

    let result = super::input::load_report(&report_path, &plan);

    assert!(result.is_err());
}

#[tokio::test]
async fn review_does_not_change_execution_results() {
    let (dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let before = std::fs::read(&report_path).unwrap();
    let reports = [
        crate::progress::read_report(&report_path).unwrap(),
        crate::progress::read_report(&report_path).unwrap(),
    ];
    let comparison =
        crate::progress::compare_reports(&reports, std::num::NonZeroUsize::new(2).unwrap());
    let candidate = &plan.manifest.candidates[0].candidate;
    let args = crate::cli::parse_from(vec![
        "hoimin".into(),
        "review".into(),
        "record".into(),
        path.to_str().unwrap().to_owned(),
        "--store".into(),
        dir.path().join("reviews.sqlite3").to_str().unwrap().into(),
        "--report".into(),
        report_path.to_str().unwrap().into(),
        "--candidate".into(),
        candidate.id.clone(),
        "--assessment".into(),
        "suspected_equivalent".into(),
        "--reason".into(),
        "allowed reorder".into(),
        "--expected-revision".into(),
        "0".into(),
        "--format".into(),
        "json".into(),
    ])
    .unwrap();
    let crate::cli::ParsedCommand::Review(args) = args else {
        panic!("review")
    };
    let mut output = Vec::new();

    super::run(&args, &mut output).unwrap();
    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let reports = [
        crate::progress::read_report(&report_path).unwrap(),
        crate::progress::read_report(&report_path).unwrap(),
    ];
    let after_comparison =
        crate::progress::compare_reports(&reports, std::num::NonZeroUsize::new(2).unwrap());

    assert_eq!(result["candidates"][0]["execution"]["status"], "killed");
    assert_eq!(std::fs::read(&report_path).unwrap(), before);
    assert_eq!(after_comparison, comparison);
}

async fn missing_report_candidate_fixture() -> (tempfile::TempDir, Vec<String>) {
    let (dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let candidate = &plan.manifest.candidates[1].candidate;
    let argv = vec![
        "hoimin".into(),
        "review".into(),
        "show".into(),
        path.to_str().unwrap().to_owned(),
        "--store".into(),
        dir.path().join("reviews.sqlite3").to_str().unwrap().into(),
        "--report".into(),
        report_path.to_str().unwrap().into(),
        "--candidate".into(),
        candidate.id.clone(),
        "--format".into(),
        "json".into(),
    ];
    (dir, argv)
}

#[tokio::test]
async fn review_report_missing_candidate_is_not_an_execution_result() {
    let (_dir, argv) = missing_report_candidate_fixture().await;
    let mut output = Vec::new();
    let mut errors = Vec::new();

    let code = crate::run_with_io(argv, &mut output, &mut errors).await;
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        value["candidates"][0]["execution"]["state"],
        "not_in_report"
    );
}

#[tokio::test]
async fn review_record_rejects_candidate_missing_from_report() {
    let (dir, mut argv) = missing_report_candidate_fixture().await;
    argv[2] = "record".into();
    argv.extend([
        "--assessment".into(),
        "needs_investigation".into(),
        "--reason".into(),
        "missing".into(),
        "--expected-revision".into(),
        "0".into(),
    ]);
    let mut output = Vec::new();
    let mut errors = Vec::new();

    let code = crate::run_with_io(argv, &mut output, &mut errors).await;

    assert_eq!(code, 2);
    assert!(!dir.path().join("reviews.sqlite3").exists());
}
