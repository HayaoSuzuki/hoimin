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
fn identity_and_update_contract() {
    let original = key();
    assert_eq!(
        applicability(&original, &original, SourceState::Fresh),
        Applicability::Current
    );
    assert_eq!(
        applicability(&original, &original, SourceState::Stale),
        Applicability::Stale
    );
    assert_eq!(
        applicability(&original, &original, SourceState::Unavailable),
        Applicability::SourceUnavailable
    );
    let mut other = original.clone();
    other.project = "/another".into();
    assert_eq!(
        applicability(&original, &other, SourceState::Fresh),
        Applicability::IdentityConflict
    );
    other = original.clone();
    other.descriptor.original = "9".into();
    assert_eq!(
        applicability(&original, &other, SourceState::Fresh),
        Applicability::IdentityConflict
    );
    let empty = ReviewState::default();
    let first = apply_update(&empty, Applicability::Current, 0, draft()).unwrap();
    assert_eq!(first.history[0].revision, 1);
    assert_eq!(empty.history, Vec::new());
    assert_eq!(
        apply_update(&first, Applicability::Current, 0, draft()),
        Err(UpdateRejection::RevisionConflict)
    );
    assert_eq!(
        apply_update(&first, Applicability::Stale, 1, draft()),
        Err(UpdateRejection::NotCurrent)
    );
    let second = apply_update(&first, Applicability::Current, 1, draft()).unwrap();
    assert_eq!(&second.history[..1], first.history.as_slice());
    assert_eq!(second.history[1].revision, 2);
    let overflow = ReviewState {
        history: vec![ReviewEvent {
            revision: u64::MAX,
            draft: draft(),
        }],
    };
    assert_eq!(
        apply_update(&overflow, Applicability::Current, u64::MAX, draft()),
        Err(UpdateRejection::RevisionOverflow)
    );
}

#[test]
fn draft_limits() {
    let mut value = draft();
    value.reason = "x".repeat(4096);
    value.evidence = vec!["x".repeat(2048); 8];
    assert!(validate_draft(&value).is_ok());
    value.reason.push('x');
    assert!(validate_draft(&value).is_err());
    for invalid in [
        " \n\t".to_string(),
        "\x1b[31m".to_string(),
        "\0".to_string(),
        "あ".repeat(1366),
    ] {
        value.reason = invalid;
        assert!(validate_draft(&value).is_err());
    }
    value.reason = "reason\n\tmore".into();
    value.evidence.push("extra".into());
    assert!(validate_draft(&value).is_err());
    value.evidence = vec!["x".repeat(2049)];
    assert!(validate_draft(&value).is_err());
    value.evidence.clear();
    assert!(validate_draft(&value).is_ok());
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
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["candidates"][0]["id"] = serde_json::Value::String("0".repeat(64));
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(super::input::load_plan(&path).is_err());
}

#[tokio::test]
async fn source_freshness_and_plan_reordering() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    let candidate = &loaded.manifest.candidates[0].candidate;
    assert_eq!(
        super::input::source_state(&loaded.project, candidate).unwrap(),
        SourceState::Fresh
    );
    std::fs::write(dir.path().join("sample.py"), "value = 9\n").unwrap();
    assert_eq!(
        super::input::source_state(&loaded.project, candidate).unwrap(),
        SourceState::Stale
    );
    std::fs::remove_file(dir.path().join("sample.py")).unwrap();
    assert_eq!(
        super::input::source_state(&loaded.project, candidate).unwrap(),
        SourceState::Unavailable
    );
}

#[tokio::test]
async fn bounded_artifact_and_alias() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    assert!(super::input::validate_store_destination(&path, &loaded, None).is_err());
    assert!(
        super::input::validate_store_destination(&dir.path().join("sample.py"), &loaded, None)
            .is_err()
    );
    let alias = dir.path().join("alias.sqlite3");
    std::fs::hard_link(&path, &alias).unwrap();
    assert!(super::input::validate_store_destination(&alias, &loaded, None).is_err());
    let oversized = dir.path().join("oversized.json");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    assert!(super::input::load_plan(&oversized).is_err());
}

#[tokio::test]
async fn store_journal_must_not_alias_a_protected_input() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    let store = dir.path().join("reviews.sqlite3");
    std::fs::hard_link(&path, dir.path().join("reviews.sqlite3-journal")).unwrap();
    assert!(super::input::validate_store_destination(&store, &loaded, None).is_err());
}

#[test]
fn malformed_artifact_reference_is_rejected() {
    let mut value = draft();
    value.plan.content_hash = "invalid".into();
    assert!(validate_draft(&value).is_err());
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
async fn old_candidate_report_requires_full_descriptor_match() {
    let (_dir, path, report_path) = report_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let (report, _) = super::input::load_report(&report_path, &plan).unwrap();
    let mut key =
        super::input::review_key(&plan.project, &plan.manifest.candidates.remove(0).candidate);
    let state = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();
    let rows = super::stale_rows(&plan, &[(key.clone(), state.clone())], Some(&report)).unwrap();
    assert!(matches!(
        rows[0].execution,
        super::render::ReviewExecution::Result(hoimin_core::MutationStatus::Killed)
    ));
    key.descriptor.original = "different".into();
    assert!(super::stale_rows(&plan, &[(key, state)], Some(&report)).is_err());
}

#[tokio::test]
async fn review_report_identity() {
    let (_dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let original = std::fs::read(&report_path).unwrap();
    let (report, _) = super::input::load_report(&report_path, &plan).unwrap();
    assert_eq!(report.mutants.len(), 1);
    for change in [
        "incomplete",
        "baseline",
        "root",
        "summary",
        "descriptor",
        "version",
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        match change {
            "incomplete" => value["summary"]["complete"] = false.into(),
            "baseline" => value["baseline"]["termination"]["Exit"] = 1.into(),
            "root" => {
                value["run"]["normalized_config"]["root"] =
                    std::env::temp_dir().to_str().unwrap().into();
            }
            "summary" => value["summary"]["counts"]["killed"] = 9.into(),
            "descriptor" => value["mutants"][0]["candidate"]["original"] = "wrong".into(),
            "version" => value["schema_version"] = 3.into(),
            _ => unreachable!(),
        }
        std::fs::write(&report_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            super::input::load_report(&report_path, &plan).is_err(),
            "{change}"
        );
    }
    std::fs::write(&report_path, [original.as_slice(), b"\n{}\n"].concat()).unwrap();
    assert!(super::input::load_report(&report_path, &plan).is_err());
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
    assert_eq!(result["candidates"][0]["execution"]["status"], "killed");
    assert_eq!(std::fs::read(&report_path).unwrap(), before);
    let reports = [
        crate::progress::read_report(&report_path).unwrap(),
        crate::progress::read_report(&report_path).unwrap(),
    ];
    assert_eq!(
        crate::progress::compare_reports(&reports, std::num::NonZeroUsize::new(2).unwrap()),
        comparison
    );
}

#[tokio::test]
async fn review_report_missing_candidate_is_not_an_execution_result() {
    let (dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let candidate = &plan.manifest.candidates[1].candidate;
    let mut argv = vec![
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
    let mut output = Vec::new();
    let mut errors = Vec::new();
    assert_eq!(
        crate::run_with_io(argv.clone(), &mut output, &mut errors).await,
        0
    );
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["candidates"][0]["execution"]["state"],
        "not_in_report"
    );
    argv[2] = "record".into();
    argv.extend([
        "--assessment".into(),
        "needs_investigation".into(),
        "--reason".into(),
        "missing".into(),
        "--expected-revision".into(),
        "0".into(),
    ]);
    assert_eq!(crate::run_with_io(argv, &mut output, &mut errors).await, 2);
    assert!(!dir.path().join("reviews.sqlite3").exists());
}
