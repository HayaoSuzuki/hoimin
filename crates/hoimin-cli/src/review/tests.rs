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
    plan_fixture_with_source("value = 1\n").await
}

async fn plan_fixture_with_source(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.py"), source).unwrap();
    let args = crate::cli::parse_from([
        "hoimin",
        "plan",
        "--root",
        dir.path().to_str().unwrap(),
        "--file",
        "sample.py",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
        "--",
        "must-not-execute",
    ])
    .unwrap();
    let crate::cli::ParsedCommand::Plan(args) = args else {
        panic!("plan")
    };
    let (config, discovery) = args.into_configs().unwrap();
    let output = crate::plan::create(config, discovery).await.unwrap();
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
    let rows = super::stale_rows(&super::input::index_plan(&plan), &[(key, state)], None).unwrap();
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
async fn plan_readers_reject_old_schemas_with_regeneration_guidance() {
    for version in 1..=8 {
        for review in [false, true] {
            // Arrange: old fields must not obscure schema regeneration guidance.
            let (_dir, path) = plan_fixture().await;
            let mut value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            value["schema_version"] = version.into();
            value["normalized_config"]["root"] = serde_json::json!("old-project");
            value["fingerprint_inputs"] = serde_json::json!([]);
            value["normalized_config"]
                .as_object_mut()
                .unwrap()
                .remove("fingerprint_inputs");
            let bytes = serde_json::to_vec(&value).unwrap();
            std::fs::write(&path, &bytes).unwrap();

            // Act.
            let result = if review {
                super::input::load_plan(&path)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            } else {
                crate::plan::prepare_verify(&path, &[], crate::cli::OutputFormat::Json)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            };

            // Assert.
            let error = result.unwrap_err();
            assert!(
                error.contains(&format!(
                    "unsupported schema version {version}; regenerate the plan"
                )),
                "review={review}: {error}"
            );
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }
}

#[tokio::test]
async fn plan_readers_reject_obsolete_top_level_fingerprint_inputs() {
    for review in [false, true] {
        // Arrange.
        let (_dir, path) = plan_fixture().await;
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["fingerprint_inputs"] = serde_json::json!([]);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        // Act.
        let result = if review {
            super::input::load_plan(&path)
                .map(|_| ())
                .map_err(|error| error.to_string())
        } else {
            crate::plan::prepare_verify(&path, &[], crate::cli::OutputFormat::Json)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        };

        // Assert.
        let error = result.unwrap_err();
        assert!(
            error.contains("unknown field `fingerprint_inputs`"),
            "review={review}: {error}"
        );
    }
}

#[tokio::test]
async fn plan_readers_reject_duplicate_known_fields() {
    for (field, original, duplicate) in [
        (
            "kind",
            "\"kind\":\"plan\"",
            "\"kind\":\"plan\",\"kind\":\"plan\"",
        ),
        (
            "fingerprint_inputs",
            "\"fingerprint_inputs\":[]",
            "\"fingerprint_inputs\":[],\"fingerprint_inputs\":[]",
        ),
    ] {
        for review in [true, false] {
            // Arrange: retain duplicate keys in raw JSON, not a Value map.
            let (_dir, path) = plan_fixture().await;
            let saved = std::fs::read_to_string(&path).unwrap();
            let duplicated = saved.replacen(original, duplicate, 1);
            assert_ne!(duplicated, saved);
            std::fs::write(&path, &duplicated).unwrap();

            // Act.
            let result = if review {
                super::input::load_plan(&path)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            } else {
                crate::plan::prepare_verify(&path, &[], crate::cli::OutputFormat::Json)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            };

            // Assert.
            let error = result.unwrap_err();
            assert!(
                error.contains(&format!("duplicate field `{field}`")),
                "review={review}: {error}"
            );
        }
    }
}

#[tokio::test]
async fn plan_readers_reject_malformed_nested_fingerprint_records() {
    let record = serde_json::json!({"path": "config.toml", "hash": "a".repeat(64)});
    for (name, replacement, fragment) in [
        ("missing", None, "missing field `fingerprint_inputs`"),
        ("null", Some(serde_json::Value::Null), "invalid type: null"),
        (
            "duplicate",
            Some(serde_json::json!([record, record])),
            "duplicate",
        ),
        (
            "hash",
            Some(serde_json::json!([{"path": "config.toml", "hash": "invalid"}])),
            "hash",
        ),
        (
            "path",
            Some(serde_json::json!([{"path": "../outside", "hash": "a".repeat(64)}])),
            "path",
        ),
    ] {
        for review in [false, true] {
            // Arrange.
            let (_dir, path) = plan_fixture().await;
            let mut value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            value["normalized_config"]
                .as_object_mut()
                .unwrap()
                .remove("fingerprint_inputs");
            if let Some(replacement) = &replacement {
                value["normalized_config"]["fingerprint_inputs"] = replacement.clone();
            }
            std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

            // Act.
            let result = if review {
                super::input::load_plan(&path)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            } else {
                crate::plan::prepare_verify(&path, &[], crate::cli::OutputFormat::Json)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            };

            // Assert.
            let error = result.unwrap_err();
            assert!(error.contains(fragment), "{name}, review={review}: {error}");
        }
    }
}

#[tokio::test]
async fn store_destination_protects_fingerprint_only_inputs_without_modification() {
    for hard_link in [false, true] {
        // Arrange: the protected input is not a mutation source.
        let (dir, path) = plan_fixture().await;
        let input = dir.path().join("config.toml");
        let original = b"setting = true\n";
        std::fs::write(&input, original).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["normalized_config"]["fingerprint_inputs"] = serde_json::json!([{
            "path": "config.toml", "hash": blake3::hash(original).to_hex().to_string()
        }]);
        let saved = serde_json::to_vec(&value).unwrap();
        std::fs::write(&path, &saved).unwrap();
        let plan = super::input::load_plan(&path).unwrap();
        let destination = if hard_link {
            let alias = dir.path().join("reviews.sqlite3");
            std::fs::hard_link(&input, &alias).unwrap();
            alias
        } else {
            input.clone()
        };

        // Act.
        let result = super::input::validate_store_destination(&destination, &plan, None);

        // Assert.
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("store aliases a protected input")
        );
        assert_eq!(std::fs::read(&input).unwrap(), original);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
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
        "../../tests/golden/reports/schema-v9-current.json"
    ))
    .unwrap();
    let config = plan
        .manifest
        .normalized_config
        .clone()
        .into_run_config(hoimin_core::OutputConfig::default())
        .unwrap();
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
    let (report, _) =
        super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan)).unwrap();
    let key =
        super::input::review_key(&plan.project, &plan.manifest.candidates.remove(0).candidate);
    let state = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();

    let rows = super::stale_rows(
        &super::input::index_plan(&plan),
        &[(key, state)],
        Some(&super::index_report(&report)),
    )
    .unwrap();

    assert!(matches!(
        rows[0].execution,
        super::render::ReviewExecution::Result(hoimin_core::MutationStatus::Killed)
    ));
}

#[tokio::test]
async fn old_candidate_report_rejects_descriptor_mismatch() {
    let (_dir, path, report_path) = report_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let (report, _) =
        super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan)).unwrap();
    let mut key =
        super::input::review_key(&plan.project, &plan.manifest.candidates.remove(0).candidate);
    key.descriptor.original = "different".into();
    let state = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();

    let result = super::stale_rows(
        &super::input::index_plan(&plan),
        &[(key, state)],
        Some(&super::index_report(&report)),
    );

    assert!(result.is_err());
}

#[tokio::test]
async fn review_accepts_matching_report() {
    let (_dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();

    let (report, _) =
        super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan)).unwrap();

    assert_eq!(report.mutants.len(), 1);
}

#[tokio::test]
async fn public_review_rejects_contradictory_status_after_matching_control() {
    // Arrange: valid saved configuration, candidate identity and janitor isolate status.
    let (dir, path, report_path) = report_fixture().await;
    let store = dir.path().join("status-reviews.sqlite3");
    let plan = super::input::load_plan(&path).unwrap();
    let mut wire: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
    wire["mutants"][0]["status"] = serde_json::json!("killed");
    std::fs::write(&report_path, serde_json::to_vec(&wire).unwrap()).unwrap();
    super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan)).unwrap();
    wire["mutants"][0]["status"] = serde_json::json!("survived");
    std::fs::write(&report_path, serde_json::to_vec(&wire).unwrap()).unwrap();
    let out = crate::test_writer::CollectingWriter::default();
    let err = crate::test_writer::CollectingWriter::default();
    // Act.
    let code = crate::run_with_writers(
        [
            "hoimin",
            "review",
            "list",
            path.to_str().unwrap(),
            "--report",
            report_path.to_str().unwrap(),
            "--store",
            store.to_str().unwrap(),
        ],
        out,
        err.clone(),
    )
    .await;
    // Assert.
    assert_eq!(code, 2);
    let error = String::from_utf8(err.snapshot()).unwrap();
    assert!(
        error.contains("status survived does not match outcome status killed"),
        "{error}"
    );
}

async fn reject_semantic_report_at_command(change: fn(&mut serde_json::Value), expected: &str) {
    // Arrange: a usable matching report and valid plan isolate config acceptance.
    let (dir, plan, report_path) = report_fixture().await;
    let store = dir.path().join("missing-reviews.sqlite3");
    let mut report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
    change(&mut report["run"]["normalized_config"]);
    std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let argv = vec![
        "hoimin".to_owned(),
        "review".into(),
        "list".into(),
        plan.to_str().unwrap().into(),
        "--report".into(),
        report_path.to_str().unwrap().into(),
        "--store".into(),
        store.to_str().unwrap().into(),
        "--format".into(),
        "json".into(),
    ];
    let output = crate::test_writer::CollectingWriter::default();
    let errors = crate::test_writer::CollectingWriter::default();

    // Act
    let code = crate::run_with_writers(argv, output.clone(), errors.clone()).await;

    // Assert
    assert_eq!(code, 2);
    assert_eq!(output.snapshot(), Vec::<u8>::new());
    assert!(String::from_utf8_lossy(&errors.snapshot()).contains(expected));
    assert!(!store.exists());
}

macro_rules! invalid_report_command_cases {
    ($($name:ident: $change:expr => $expected:literal;)*) => {
        $(#[tokio::test]
        async fn $name() {
            reject_semantic_report_at_command($change, $expected).await;
        })*
    };
}

invalid_report_command_cases! {
    command_rejects_report_jobs_above_absolute_cap: |config| {
        config["limits"]["jobs"] = serde_json::json!(257);
        config["limits"]["max_processes"] = serde_json::json!(300);
    } => "--jobs 257 exceeds the supported maximum 256";
    command_rejects_report_jobs_above_process_cap: |config| {
        config["limits"]["jobs"] = serde_json::json!(65);
    } => "--jobs 65 exceeds --max-processes 64";
    command_rejects_report_empty_selector: |config| {
        config["selection"]["sources"] = serde_json::json!([]);
        config["selection"]["files"] = serde_json::json!([]);
        config["selection"]["lines"] = serde_json::json!([]);
        config["selection"]["symbols"] = serde_json::json!([]);
        config["selection"]["changed"] = serde_json::Value::Null;
    } => "at least one target selector is required";
    command_rejects_report_empty_operators: |config| config["operators"] = serde_json::json!([])
        => "mutation operator selection is empty";
    command_rejects_report_empty_argv: |config| config["test_argv"] = serde_json::json!([])
        => "at least one test argv element is required";
    command_rejects_report_escaping_import_root: |config| config["import_roots"] = serde_json::json!(["../escape"])
        => "invalid --import-root ../escape";
}

#[tokio::test]
async fn review_rejects_absent_report_import_roots() {
    for replacement in [None, Some(serde_json::Value::Null)] {
        // Arrange
        let (_dir, path, report_path) = report_fixture().await;
        let plan = super::input::load_plan(&path).unwrap();
        let mut report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        report["run"]["normalized_config"]
            .as_object_mut()
            .unwrap()
            .remove("import_roots");
        if let Some(replacement) = replacement {
            report["run"]["normalized_config"]["import_roots"] = replacement;
        }
        std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();

        // Act
        let result =
            super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan));

        // Assert
        assert!(
            result.is_err(),
            "review accepted absent report import_roots"
        );
    }
}

#[tokio::test]
async fn progress_preserves_opaque_report_import_roots() {
    for replacement in [None, Some(serde_json::Value::Null)] {
        // Arrange
        let (_dir, _path, report_path) = report_fixture().await;
        let mut report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        report["run"]["normalized_config"]
            .as_object_mut()
            .unwrap()
            .remove("import_roots");
        if let Some(replacement) = replacement {
            report["run"]["normalized_config"]["import_roots"] = replacement;
        }
        std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();

        // Act
        let result = crate::progress::read_report(&report_path);

        // Assert
        assert!(matches!(
            result.unwrap(),
            crate::progress::InputReport::Usable(_)
        ));
    }
}

#[tokio::test]
async fn review_rejects_report_with_changed_identity_or_execution() {
    let changes: [(&str, &str, serde_json::Value); 6] = [
        ("incomplete", "/summary/complete", false.into()),
        ("baseline", "/baseline/termination/Exit", 1.into()),
        (
            "root",
            "/run/normalized_config/selection/root",
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

        let result =
            super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan));

        assert!(result.is_err(), "{change}");
    }
}

#[tokio::test]
async fn review_rejects_trailing_report_document() {
    let (_dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let original = std::fs::read(&report_path).unwrap();
    std::fs::write(&report_path, [original.as_slice(), b"\n{}\n"].concat()).unwrap();

    let result = super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan));

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
    let output = crate::test_writer::CollectingWriter::default();
    let errors = crate::test_writer::CollectingWriter::default();

    let code = crate::run_with_writers(argv, output.clone(), errors.clone()).await;
    let value: serde_json::Value = serde_json::from_slice(&output.snapshot()).unwrap();

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
    let output = crate::test_writer::CollectingWriter::default();
    let errors = crate::test_writer::CollectingWriter::default();

    let code = crate::run_with_writers(argv, output.clone(), errors.clone()).await;

    assert_eq!(code, 2);
    assert!(!dir.path().join("reviews.sqlite3").exists());
}

fn write_matching_report(
    path: &std::path::Path,
    plan: &crate::plan::ReadOnlyPlan,
    candidates: &[&hoimin_core::MutationCandidate],
) {
    let mut report: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/golden/reports/schema-v9-current.json"
    ))
    .unwrap();
    report["run"]["normalized_config"] = serde_json::to_value(
        plan.manifest
            .normalized_config
            .clone()
            .into_run_config(hoimin_core::OutputConfig::default())
            .unwrap(),
    )
    .unwrap();
    let template = report["mutants"][0].clone();
    report["mutants"] = serde_json::Value::Array(
        candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                let mut mutant = template.clone();
                mutant["candidate"] = serde_json::to_value(candidate).unwrap();
                mutant["sequence"] = (index + 4).into();
                mutant
            })
            .collect(),
    );
    report["summary"]["sequence"] = (candidates.len() + 4).into();
    report["summary"]["counts"]["killed"] = candidates.len().into();
    std::fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
}

fn populate_matching_snapshot(store_path: &std::path::Path, plan: &crate::plan::ReadOnlyPlan) {
    // Initialize through the store, then populate the fixture in one transaction
    // so append's own validation cost is outside this join-scaling regression.
    drop(super::store::ReviewStore::open_write(store_path, &plan.project).unwrap());
    let mut connection = rusqlite::Connection::open(store_path).unwrap();
    let transaction = connection.transaction().unwrap();
    for ranked in &plan.manifest.candidates {
        let key = super::input::review_key(&plan.project, &ranked.candidate);
        let mut event = draft();
        event.reason.clone_from(&ranked.id);
        transaction
            .execute(
                "INSERT INTO candidates(id, key_json) VALUES (?1, ?2)",
                rusqlite::params![ranked.id, serde_json::to_string(&key).unwrap()],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO review_events(candidate_id, revision, draft_json) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    ranked.id,
                    1_u64.to_be_bytes().as_slice(),
                    serde_json::to_string(&event).unwrap()
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}

#[tokio::test]
async fn matching_review_command_joins_once_and_keeps_plan_display_order() {
    let (dir, path) = plan_fixture_with_source(&"value = 1\n".repeat(300)).await;
    let plan = super::input::load_plan(&path).unwrap();
    let count = plan.manifest.candidates.len();
    assert!(count >= 600, "fixture must exercise repeated joins");
    let report_path = dir.path().join("report.json");
    let candidates: Vec<_> = plan
        .manifest
        .candidates
        .iter()
        .rev()
        .map(|ranked| &ranked.candidate)
        .collect();
    write_matching_report(&report_path, &plan, &candidates);
    let store_path = dir.path().join("reviews.sqlite3");
    populate_matching_snapshot(&store_path, &plan);
    let args = crate::cli::parse_from([
        "hoimin",
        "review",
        "list",
        path.to_str().unwrap(),
        "--store",
        store_path.to_str().unwrap(),
        "--report",
        report_path.to_str().unwrap(),
        "--stale",
        "--format",
        "json",
    ])
    .unwrap();
    let crate::cli::ParsedCommand::Review(args) = args else {
        panic!("review")
    };
    super::JOIN_WORK.with_borrow_mut(|work| *work = super::JoinWork::default());
    super::input::SOURCE_WORK.with(|count| count.set((0, 0)));
    let mut output = Vec::new();
    super::run(&args, &mut output).unwrap();
    let work = super::JOIN_WORK.with_borrow(|work| *work);
    assert_eq!(super::input::SOURCE_WORK.with(std::cell::Cell::get), (1, 1));
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let rows = value["candidates"].as_array().unwrap();
    assert_eq!(rows.len(), count);
    for (row, ranked) in rows.iter().zip(&plan.manifest.candidates) {
        assert_eq!(row["candidate"]["id"], ranked.id);
        assert_eq!(row["execution"]["status"], "killed");
        assert_eq!(row["review"]["draft"]["reason"], ranked.id);
        assert_eq!(row["history"].as_array().unwrap().len(), 1);
    }
    assert_eq!(value["stale_candidates"], serde_json::json!([]));
    assert_eq!(
        work.plan_probes,
        2 * count,
        "report validation and stale membership each probe once"
    );
    assert_eq!(work.report_probes, count);
    assert_eq!(work.snapshot_probes, count);
    assert_eq!(
        (work.plan_builds, work.report_builds, work.snapshot_builds),
        (1, 1, 1)
    );
    assert_eq!(
        (
            work.plan_entries,
            work.report_entries,
            work.snapshot_entries
        ),
        (count, count, count)
    );
}

#[tokio::test]
async fn common_reader_rejects_report_identity_before_review_project_checks() {
    // Arrange
    let (_dir, path, report_path) = report_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
    value["mutants"][0]["candidate"]["replacement"] = "forged".into();
    std::fs::write(&report_path, serde_json::to_vec(&value).unwrap()).unwrap();
    plan.project = camino::Utf8PathBuf::from("another-project");
    // Act
    let error = super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan))
        .err()
        .unwrap();
    // Assert
    assert!(error.to_string().contains("candidate identity conflict"));
    assert!(error.to_string().contains(report_path.to_str().unwrap()));
}

#[tokio::test]
async fn review_rejects_valid_identity_from_another_project() {
    // Arrange
    let (_dir, path, report_path) = report_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    plan.project = camino::Utf8PathBuf::from("another-project");
    // Act
    let error = super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan))
        .err()
        .unwrap();
    // Assert
    assert_eq!(
        error.to_string(),
        "invalid review input: report belongs to another project"
    );
}

#[tokio::test]
async fn review_retains_plan_descriptor_check_for_display_only_difference() {
    // Arrange: original text does not participate in the stable ID.
    let (_dir, path, report_path) = report_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    plan.manifest.candidates[0].candidate.original = "different".into();
    // Act
    let error = super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan))
        .err()
        .unwrap();
    // Assert
    assert_eq!(
        error.to_string(),
        "invalid review input: report candidate descriptor differs from plan"
    );
}

#[tokio::test]
async fn stale_rows_keep_snapshot_order() {
    let (_dir, path) = plan_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let candidates = std::mem::take(&mut plan.manifest.candidates);
    let state = apply_update(&ReviewState::default(), Applicability::Current, 0, draft()).unwrap();
    let first = super::input::review_key(&plan.project, &candidates[0].candidate);
    let second = super::input::review_key(&plan.project, &candidates[1].candidate);
    let snapshot = [(second.clone(), state.clone()), (first.clone(), state)];
    let rows = super::stale_rows(&super::input::index_plan(&plan), &snapshot, None).unwrap();
    let ids: Vec<_> = rows.iter().map(|row| row.candidate.id.as_str()).collect();
    assert_eq!(
        ids,
        [second.candidate_id.as_str(), first.candidate_id.as_str()]
    );
}

#[tokio::test]
async fn checked_report_duplicate_error_precedes_identity_conflict() {
    let (_dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
    value["mutants"][0]["candidate"]["id"] = "forged".into();
    let mut duplicate = value["mutants"][0].clone();
    duplicate["sequence"] = 5.into();
    value["mutants"].as_array_mut().unwrap().push(duplicate);
    value["summary"]["counts"]["killed"] = 2.into();
    std::fs::write(&report_path, serde_json::to_vec(&value).unwrap()).unwrap();
    let error = super::input::load_report(&report_path, &plan, &super::input::index_plan(&plan))
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("mutant stable IDs must appear at most once"),
        "{error}"
    );
}

#[tokio::test]
async fn checked_plan_keeps_duplicate_id_diagnostic() {
    let (_dir, path) = plan_fixture().await;
    let mut plan = super::input::load_plan(&path).unwrap();
    let mut first = plan.manifest.candidates[0].clone();
    first.rank = 1;
    first.candidate.sequence = 1;
    let mut second = first.clone();
    second.rank = 2;
    second.candidate.sequence = 2;
    let id = first.id.clone();
    plan.manifest.candidates = vec![first, second];
    std::fs::write(&path, serde_json::to_vec(&plan.manifest).unwrap()).unwrap();
    let error = super::input::load_plan(&path).err().unwrap();
    assert_eq!(
        error.to_string(),
        format!("invalid review input: plan.candidate.invalid: duplicate candidate id {id}")
    );
}

#[tokio::test]
async fn review_list_reads_and_indexes_each_source_once_per_command() {
    let (dir, path) = plan_fixture().await;
    let loaded = super::input::load_plan(&path).unwrap();
    assert!(loaded.manifest.candidates.len() > 1);
    let command_line = vec![
        "hoimin".to_owned(),
        "review".into(),
        "list".into(),
        path.to_str().unwrap().into(),
        "--store".into(),
        dir.path().join("reviews.sqlite3").to_str().unwrap().into(),
        "--format".into(),
        "json".into(),
    ];
    let crate::cli::ParsedCommand::Review(args) = crate::cli::parse_from(command_line).unwrap()
    else {
        panic!("review");
    };
    super::input::SOURCE_WORK.with(|count| count.set((0, 0)));
    let mut output = Vec::new();
    super::run(&args, &mut output).unwrap();
    assert_eq!(super::input::SOURCE_WORK.with(std::cell::Cell::get), (1, 1));
    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let ids: Vec<_> = result["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["candidate"]["id"].as_str().unwrap())
        .collect();
    let expected: Vec<_> = loaded
        .manifest
        .candidates
        .iter()
        .map(|ranked| ranked.candidate.id.as_str())
        .collect();
    assert_eq!(ids, expected);

    std::fs::write(dir.path().join("sample.py"), "value = 9\n").unwrap();
    output.clear();
    super::run(&args, &mut output).unwrap();
    assert_eq!(super::input::SOURCE_WORK.with(std::cell::Cell::get), (2, 2));
    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(
        result["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["applicability"] == "stale")
    );
}

fn source_fixture() -> (
    tempfile::TempDir,
    Utf8PathBuf,
    hoimin_core::MutationCandidate,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.py"), b"1\n").unwrap();
    let root = Utf8PathBuf::from_path_buf(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let descriptor = key().descriptor;
    let candidate = hoimin_core::MutationCandidate {
        id: stable_mutant_id(&CandidateIdentity::from(&descriptor)).to_string(),
        sequence: 1,
        path: descriptor.path,
        span: descriptor.span,
        original: descriptor.original,
        replacement: descriptor.replacement,
        operator: descriptor.operator,
        line: descriptor.line,
        column: descriptor.column,
        symbol: descriptor.symbol,
        file_hash: descriptor.file_hash,
    };
    (dir, root, candidate)
}

#[test]
fn source_batch_preserves_hash_descriptor_and_identity_precedence() {
    let (_dir, root, candidate) = source_fixture();
    for field in ["span", "original", "location", "mutation", "identity"] {
        let mut invalid = candidate.clone();
        match field {
            "span" => invalid.span.start = 100,
            "original" => invalid.original = "9".into(),
            "location" => invalid.line = 9,
            "mutation" => invalid.replacement = invalid.original.clone(),
            "identity" => invalid.id = "0".repeat(64),
            _ => unreachable!(),
        }
        assert!(
            super::input::source_states(&root, &[&candidate, &invalid])
                .pop()
                .unwrap()
                .is_err(),
            "{field}"
        );
        invalid.file_hash = "0".repeat(64);
        assert_eq!(
            super::input::source_state(&root, &invalid).unwrap(),
            SourceState::Stale,
            "{field}"
        );
    }
}

#[test]
fn source_state_preserves_encoding_checks_and_stale_precedence() {
    let (dir, root, mut candidate) = source_fixture();
    std::fs::write(dir.path().join("sample.py"), b"1\n\xff").unwrap();
    assert_eq!(
        super::input::source_state(&root, &candidate).unwrap(),
        SourceState::Stale
    );
    candidate.file_hash = blake3::hash(b"1\n\xff").to_hex().to_string();
    let error = super::input::source_state(&root, &candidate).unwrap_err();
    assert!(error.to_string().contains("not valid UTF-8"));

    let latin1 = b"# coding: latin-1\n1\n";
    std::fs::write(dir.path().join("sample.py"), latin1).unwrap();
    candidate.file_hash = blake3::hash(latin1).to_hex().to_string();
    candidate.span.start = 18;
    candidate.line = 2;
    candidate.replacement = "\u{100}".into();
    let error = super::input::source_state(&root, &candidate).unwrap_err();
    assert!(error.to_string().contains("encoding"));
}

#[test]
fn source_state_rejects_path_escape_and_bounds_source_reads() {
    let (dir, root, mut candidate) = source_fixture();
    candidate.path = "../sample.py".into();
    assert!(super::input::source_state(&root, &candidate).is_err());
    candidate.path = "sample.py".into();
    std::fs::File::create(dir.path().join("sample.py"))
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(
        super::input::source_state(&root, &candidate).unwrap(),
        SourceState::Unavailable
    );
}

#[test]
fn source_batch_reuses_one_context_for_many_candidates_and_record_reads_again() {
    let (dir, root, candidate) = source_fixture();
    super::input::SOURCE_WORK.with(|count| count.set((0, 0)));
    let candidates = vec![&candidate; 128];
    let states = super::input::source_states(&root, &candidates);
    assert_eq!(states.len(), 128);
    assert!(
        states
            .into_iter()
            .all(|state| matches!(state, Ok(SourceState::Fresh)))
    );
    assert_eq!(super::input::SOURCE_WORK.with(std::cell::Cell::get), (1, 1));

    std::fs::write(dir.path().join("sample.py"), b"9\n").unwrap();
    assert_eq!(
        super::input::source_state(&root, &candidate).unwrap(),
        SourceState::Stale
    );
    assert_eq!(super::input::SOURCE_WORK.with(std::cell::Cell::get), (2, 2));
}

#[test]
fn source_batch_preserves_interleaved_states_and_first_error_order() {
    let (dir, root, candidate) = source_fixture();
    std::fs::write(dir.path().join("another.py"), b"1\n").unwrap();
    let mut other = candidate.clone();
    other.path = "another.py".into();
    other.id =
        stable_mutant_id(&CandidateIdentity::from(&CandidateDescriptor::from(&other))).to_string();
    let mut stale = candidate.clone();
    stale.file_hash = "0".repeat(64);
    let mut missing = candidate.clone();
    missing.path = "missing.py".into();
    super::input::SOURCE_WORK.with(|count| count.set((0, 0)));

    let states =
        super::input::source_states(&root, &[&candidate, &other, &stale, &missing, &candidate]);
    let actual: Vec<_> = states.into_iter().map(Result::unwrap).collect();
    assert_eq!(
        actual,
        [
            SourceState::Fresh,
            SourceState::Fresh,
            SourceState::Stale,
            SourceState::Unavailable,
            SourceState::Fresh
        ]
    );
    assert_eq!(super::input::SOURCE_WORK.with(std::cell::Cell::get), (2, 2));

    other.original = "9".into();
    let mut invalid_span = candidate.clone();
    invalid_span.span.start = 100;
    let states = super::input::source_states(&root, &[&candidate, &other, &invalid_span]);
    // The first file's later span error is discovered before another.py's earlier error.
    let error = states
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_err();
    assert!(error.to_string().contains("original text"));
}

#[cfg(unix)]
#[test]
fn source_batch_rejects_canonical_root_escape() {
    let (dir, root, candidate) = source_fixture();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("outside.py"), b"1\n").unwrap();
    std::fs::remove_file(dir.path().join("sample.py")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("outside.py"),
        dir.path().join("sample.py"),
    )
    .unwrap();
    let states = super::input::source_states(&root, &[&candidate, &candidate]);
    assert!(states.into_iter().all(|state| {
        state
            .unwrap_err()
            .to_string()
            .contains("escapes project root")
    }));
}

#[test]
fn persisted_plan_rejects_legacy_run_id() {
    for run_id in [serde_json::Value::Null, "run-1".into()] {
        // Arrange
        let mut json = serde_json::to_value(draft()).unwrap();
        json["plan"]["run_id"] = run_id;

        // Act
        let result = serde_json::from_value::<ReviewDraft>(json);

        // Assert
        assert!(result.is_err());
    }
}

proptest::proptest! {
    #[test]
    fn report_reference_roundtrips_required_run_identity(
        run_id in "[a-zA-Z0-9_-]{1,80}", hash in "[0-9a-f]{64}"
    ) {
        // Arrange
        let mut json = serde_json::to_value(draft()).unwrap();
        json["report"] = serde_json::json!({
            "artifact": {"path": "report.json", "content_hash": hash},
            "run_id": run_id,
        });

        // Act
        let decoded = serde_json::from_value::<ReviewDraft>(json.clone());

        // Assert
        proptest::prop_assert!(decoded.is_ok());
        let decoded = decoded.unwrap();
        proptest::prop_assert_eq!(serde_json::to_value(decoded).unwrap(), json);
    }
}

#[test]
fn persisted_report_rejects_missing_or_null_run_identity() {
    for run_id in [None, Some(serde_json::Value::Null)] {
        // Arrange
        let mut json = serde_json::to_value(draft()).unwrap();
        let mut report = serde_json::json!({"artifact": json["plan"].clone()});
        if let Some(run_id) = run_id {
            report["run_id"] = run_id;
        }
        json["report"] = report;

        // Act
        let result = serde_json::from_value::<ReviewDraft>(json);

        // Assert
        assert!(result.is_err());
    }
}

#[test]
fn report_identifier_rejects_blank_text() {
    for id in ["", " ", "\u{00a0}"] {
        // Arrange
        let mut value = draft();
        value.report = Some(ReportReference {
            artifact: value.plan.clone(),
            run_id: id.into(),
        });

        // Act
        let result = validate_draft(&value);

        // Assert
        assert!(result.is_err());
    }
}

#[test]
fn report_identifier_rejects_excessive_utf8_bytes() {
    // Arrange
    let mut value = draft();
    value.report = Some(ReportReference {
        artifact: value.plan.clone(),
        run_id: "\u{03b1}".repeat(2049),
    });

    // Act
    let result = validate_draft(&value);

    // Assert
    assert!(result.is_err());
}

proptest::proptest! {
    #[test]
    fn valid_report_reference_passes_content_validation(
        run_id in "[a-zA-Z0-9_-]{1,80}", hash in "[0-9a-f]{64}"
    ) {
        // Arrange
        let mut value = draft();
        value.report = Some(ReportReference {
            artifact: ArtifactReference {path: "report.json".into(), content_hash: hash},
            run_id,
        });

        // Act
        let result = validate_draft(&value);

        // Assert
        proptest::prop_assert!(result.is_ok());
    }
}

#[tokio::test]
async fn recording_report_preserves_reference_when_showing_history() {
    // Arrange
    let (dir, path, report_path) = report_fixture().await;
    let plan = super::input::load_plan(&path).unwrap();
    let id = &plan.manifest.candidates[0].id;
    let store = dir.path().join("reviews.sqlite3");
    let common = vec![
        path.to_str().unwrap().to_owned(),
        "--store".into(),
        store.to_str().unwrap().to_owned(),
        "--report".into(),
        report_path.to_str().unwrap().to_owned(),
        "--format".into(),
        "json".into(),
    ];
    let mut record = vec!["hoimin".into(), "review".into(), "record".into()];
    record.extend(common.clone());
    record.extend([
        "--candidate".into(),
        id.clone(),
        "--assessment".into(),
        "actionable".into(),
        "--reason".into(),
        "missing check".into(),
        "--expected-revision".into(),
        "0".into(),
    ]);
    let (code, _, err) = run_review_cli(record).await;
    assert_eq!(code, 0, "{err}");
    let report_bytes = std::fs::read(&report_path).unwrap();
    let report_input: serde_json::Value = serde_json::from_slice(&report_bytes).unwrap();
    let expected = ReportReference {
        artifact: ArtifactReference {
            path: std::fs::canonicalize(&report_path).unwrap(),
            content_hash: blake3::hash(&report_bytes).to_hex().to_string(),
        },
        run_id: report_input["run"]["run_id"].as_str().unwrap().into(),
    };
    let mut show = vec!["hoimin".into(), "review".into(), "show".into()];
    show.extend(common);
    show.extend(["--candidate".into(), id.clone()]);

    // Act
    let (code, out, err) = run_review_cli(show).await;
    let shown: serde_json::Value = serde_json::from_str(&out).unwrap();

    // Assert
    assert_eq!(code, 0, "{err}");
    let report: ReportReference =
        serde_json::from_value(shown["candidates"][0]["history"][0]["draft"]["report"].clone())
            .unwrap();
    assert_eq!(report, expected);
}

async fn run_review_cli(args: Vec<String>) -> (i32, String, String) {
    let out = crate::test_writer::CollectingWriter::default();
    let err = crate::test_writer::CollectingWriter::default();
    let code = crate::run_with_writers(args, out.clone(), err.clone()).await;
    (
        code,
        String::from_utf8(out.snapshot()).unwrap(),
        String::from_utf8(err.snapshot()).unwrap(),
    )
}

proptest::proptest! {
    #[test]
    fn report_identifier_rejects_every_control_character(code in 0u8..=159) {
        // Arrange
        let control = char::from(code);
        proptest::prop_assume!(control.is_control());
        let mut value = draft();
        value.report = Some(ReportReference {
            artifact: value.plan.clone(),
            run_id: format!("run{control}1"),
        });

        // Act
        let result = validate_draft(&value);

        // Assert
        proptest::prop_assert!(result.is_err());
    }
}

#[test]
fn report_identifier_accepts_maximum_utf8_byte_length() {
    // Arrange
    let mut value = draft();
    value.report = Some(ReportReference {
        artifact: value.plan.clone(),
        run_id: "\u{03b1}".repeat(2048),
    });

    // Act
    let result = validate_draft(&value);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn reference_content_rejects_empty_path() {
    for report_reference in [false, true] {
        // Arrange
        let mut value = draft();
        let reference = ArtifactReference {
            path: "".into(),
            content_hash: "a".repeat(64),
        };
        if report_reference {
            value.report = Some(ReportReference {
                artifact: reference,
                run_id: "run-1".into(),
            });
        } else {
            value.plan = reference;
        }

        // Act
        let result = validate_draft(&value);

        // Assert
        assert!(result.is_err());
    }
}

#[test]
fn reference_content_rejects_malformed_hash() {
    for hash in ["A".repeat(64), "g".repeat(64), "a".repeat(63)] {
        for report_reference in [false, true] {
            // Arrange
            let mut value = draft();
            let reference = ArtifactReference {
                path: "plan.json".into(),
                content_hash: hash.clone(),
            };
            if report_reference {
                value.report = Some(ReportReference {
                    artifact: reference,
                    run_id: "run-1".into(),
                });
            } else {
                value.plan = reference;
            }

            // Act
            let result = validate_draft(&value);

            // Assert
            assert!(result.is_err());
        }
    }
}

#[test]
fn persisted_report_rejects_legacy_flat_artifact() {
    // Arrange
    let mut json = serde_json::to_value(draft()).unwrap();
    json["report"] = serde_json::json!({"path": "report.json", "content_hash": "a".repeat(64), "run_id": "run-1"});

    // Act
    let result = serde_json::from_value::<ReviewDraft>(json);

    // Assert
    assert!(result.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn store_destination_protects_a_symlinked_fingerprint_input() {
    // Arrange
    let (dir, path) = plan_fixture().await;
    let destination = dir.path().join("reviews.sqlite3");
    let original = b"setting = true\n";
    std::fs::write(&destination, original).unwrap();
    std::os::unix::fs::symlink(&destination, dir.path().join("config.toml")).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["normalized_config"]["fingerprint_inputs"] = serde_json::json!([{
        "path": "config.toml", "hash": blake3::hash(original).to_hex().to_string()
    }]);
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let plan = super::input::load_plan(&path).unwrap();

    // Act
    let result = super::input::validate_store_destination(&destination, &plan, None);

    // Assert
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("store aliases a protected input")
    );
    assert_eq!(std::fs::read(&destination).unwrap(), original);
}
