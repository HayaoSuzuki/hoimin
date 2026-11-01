#[path = "support/candidate_identity.rs"]
mod candidate_identity;
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
#[path = "support/mutation_outcome.rs"]
mod mutation_outcome;
use collecting_writer::CollectingWriter;

use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::Output,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use camino::Utf8PathBuf;
use hoimin_cli::progress::{
    Comparison, InputReport, ProgressState, ReportReadError, UnusableReason, UsableReport,
    compare_reports, read_report,
};
use hoimin_core::{
    ByteSpan, MutantFinished, MutationCandidate, MutationStatus, REPORT_SCHEMA_VERSION,
    ResourceMode,
};
use proptest::prelude::*;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

const TEST_MIN_FREE_SPACE: &str = "1B";

#[test]
fn progress_rejects_removed_ci_gate() {
    let args = [
        "hoimin",
        "progress",
        "--fail-on-regression",
        "before.json",
        "after.json",
    ];

    let result = hoimin_cli::cli::parse_from(args);

    assert!(result.is_err());
}

#[rstest::rstest]
#[case::schema_2_original(2, "original")]
#[case::schema_2_current(2, "current")]
#[case::schema_3_original(3, "original")]
#[case::schema_3_current(3, "current")]
#[case::schema_4_original(4, "original")]
#[case::schema_4_current(4, "current")]
#[case::schema_5_original(5, "original")]
#[case::schema_5_current(5, "current")]
#[case::schema_6_original(6, "original")]
#[case::schema_6_current(6, "current")]
#[case::schema_7_original(7, "original")]
#[case::schema_7_current(7, "current")]
#[case::schema_8_original(8, "original")]
#[case::schema_8_current(8, "current")]
#[case::schema_9_original(9, "original")]
#[case::schema_9_current(9, "current")]
fn input_rejects_historical_report_goldens(#[case] version: u32, #[case] era: &str) {
    let path = repo_root()
        .join("crates/hoimin-cli/tests/golden/events")
        .join(format!("schema-v{version}-{era}.jsonl"));

    let result = read_report(&path);

    assert!(
        result.is_err(),
        "historical input must be rejected: {path:?}: {result:?}"
    );
}

#[rstest::rstest]
#[case::schema_0_document(0, write_json)]
#[case::schema_0_events(0, write_explicit_jsonl)]
#[case::schema_1_document(1, write_json)]
#[case::schema_1_events(1, write_explicit_jsonl)]
#[case::schema_2_document(2, write_json)]
#[case::schema_2_events(2, write_explicit_jsonl)]
#[case::schema_3_document(3, write_json)]
#[case::schema_3_events(3, write_explicit_jsonl)]
#[case::schema_4_document(4, write_json)]
#[case::schema_4_events(4, write_explicit_jsonl)]
#[case::schema_5_document(5, write_json)]
#[case::schema_5_events(5, write_explicit_jsonl)]
#[case::schema_6_document(6, write_json)]
#[case::schema_6_events(6, write_explicit_jsonl)]
#[case::schema_7_document(7, write_json)]
#[case::schema_7_events(7, write_explicit_jsonl)]
#[case::schema_8_document(8, write_json)]
#[case::schema_8_events(8, write_explicit_jsonl)]
#[case::schema_9_document(9, write_json)]
#[case::schema_9_events(9, write_explicit_jsonl)]
#[case::schema_future_document(REPORT_SCHEMA_VERSION + 1, write_json)]
#[case::schema_future_events(REPORT_SCHEMA_VERSION + 1, write_explicit_jsonl)]
fn input_rejects_every_noncurrent_schema_in_jsonl(
    #[case] version: u32,
    #[case] writer: ReportWriter,
) {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["schema_version"] = json!(version);
    for key in ["run", "baseline", "summary"] {
        document[key]["schema_version"] = json!(version);
    }
    for mutant in document["mutants"].as_array_mut().unwrap() {
        mutant["schema_version"] = json!(version);
    }
    let path = writer(&fixture, "old.json", &document);

    let result = read_report(&path);

    assert!(
        matches!(result, Err(ReportReadError::UnsupportedSchema { found, .. }) if found == version),
        "schema {version}: {result:?}"
    );
}

#[rstest::rstest]
#[case::rotation_0(0)]
#[case::rotation_1(1)]
#[case::rotation_2(2)]
#[case::rotation_3(3)]
#[case::rotation_4(4)]
#[case::rotation_5(5)]
#[case::rotation_6(6)]
fn input_dispatch_preserves_reordered_jsonl_headers(#[case] rotation: usize) {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]["normalized_config"] = json!({"schema_version": 2, "kind": "not-an-event"});
    let events = jsonl_events(&document);
    let mut keys = events[0].as_object().unwrap().keys().collect::<Vec<_>>();
    keys.rotate_left(rotation);
    let header = keys
        .iter()
        .map(|key| format!("\"{key}\":{}", events[0][*key]))
        .collect::<Vec<_>>()
        .join(",");
    let rest = events
        .iter()
        .skip(1)
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let path = fixture.path().join("reordered.jsonl");
    std::fs::write(&path, format!("{{{header}}}\n{rest}\n")).unwrap();
    let InputReport::Usable(actual) = read_report(&path).unwrap() else {
        panic!("usable JSONL")
    };
    assert_eq!(actual.mutants.len(), 1);
    assert_eq!(
        actual.mutants[0].candidate.id,
        valid_report()["mutants"][0]["candidate"]["id"]
            .as_str()
            .unwrap()
    );
}

#[rstest::rstest]
#[case::duplicate_document_schema("document", "schema")]
#[case::duplicate_document_run("document", "run")]
#[case::unknown_document_field("document", "unknown")]
#[case::document_event_kind("document", "kind")]
#[case::document_trailing_object("document", "trailing")]
#[case::truncated_document("document", "truncated")]
#[case::duplicate_header_schema("header", "schema")]
#[case::duplicate_header_kind("header", "kind")]
#[case::duplicate_header_run("header", "run")]
#[case::unknown_header_field("header", "unknown")]
#[case::header_trailing_object("header", "trailing")]
#[case::truncated_header("header", "truncated")]
fn input_dispatch_never_accepts_a_valid_prefix_with_an_invalid_suffix(
    #[case] format: &str,
    #[case] defect: &str,
) {
    let fixture = tempfile::tempdir().unwrap();
    let bytes = invalid_report_prefix(format, defect);
    let path = fixture.path().join("invalid.report");
    std::fs::write(&path, bytes).unwrap();

    let result = read_report(&path);

    assert!(result.is_err());
}

fn invalid_report_prefix(format: &str, defect: &str) -> String {
    let document = valid_report();
    match format {
        "document" => {
            let fields = ["schema_version", "run", "baseline", "mutants", "summary"]
                .iter()
                .map(|key| format!("\"{key}\":{}", document[key]))
                .collect::<Vec<_>>()
                .join(",");
            match defect {
                "schema" => format!("{{{fields},\"schema_version\":3}}"),
                "run" => format!("{{{fields},\"run\":{}}}", document["run"]),
                "unknown" => format!("{{{fields},\"unexpected\":true}}"),
                "kind" => format!("{{{fields},\"kind\":\"run_started\"}}"),
                "trailing" => format!("{{{fields}}} {{}}"),
                "truncated" => format!("{{{fields}"),
                _ => unreachable!(),
            }
        }
        "header" => {
            let events = jsonl_events(&document);
            let header = events[0].to_string();
            let prefix = header.strip_suffix('}').unwrap();
            let rest = events
                .iter()
                .skip(1)
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            let invalid_header = match defect {
                "schema" => format!("{prefix},\"schema_version\":3}}"),
                "kind" => format!("{prefix},\"kind\":\"run_started\"}}"),
                "run" => format!("{prefix},\"run_id\":\"run-1\"}}"),
                "unknown" => format!("{prefix},\"unexpected\":true}}"),
                "trailing" => format!("{header} {{}}"),
                "truncated" => prefix.to_owned(),
                _ => unreachable!(),
            };
            format!("{invalid_header}\n{rest}\n")
        }
        _ => unreachable!(),
    }
}

#[test]
fn current_report_requires_disk_summary() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("missing-disk.json");
    let mut report = valid_report();
    report["summary"].as_object_mut().unwrap().remove("disk");
    std::fs::write(&path, jsonl_report::to_vec(&report)).unwrap();

    let error = read_report(&path).unwrap_err();
    assert!(matches!(error, ReportReadError::Parse { .. }));
}

#[tokio::test]
async fn real_run_reports_expose_exact_regression_through_progress() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(
        project.path().join("pyproject.toml"),
        "[project]\nname = \"hoimin-progress-fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
    )
    .unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def add(left: int, right: int) -> int:\n    return left + right\n",
    )
    .unwrap();

    let before_output = run_real_binary(
        project.path(),
        "from src.calc import add; assert add(2, 3) == 5",
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(
        before_output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&before_output.stdout),
        String::from_utf8_lossy(&before_output.stderr)
    );
    let before_report: Value = jsonl_report::from_slice(&before_output.stdout).unwrap();
    let before_mutants = before_report["mutants"].as_array().unwrap();
    assert_eq!(before_mutants.len(), 1);
    assert_eq!(mutation_outcome::status(&before_mutants[0]), "killed");

    let after_output = run_real_binary(
        project.path(),
        "from src.calc import add; add(2, 3)",
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(
        after_output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&after_output.stdout),
        String::from_utf8_lossy(&after_output.stderr)
    );
    let after_report: Value = jsonl_report::from_slice(&after_output.stdout).unwrap();
    let after_mutants = after_report["mutants"].as_array().unwrap();
    assert_eq!(after_mutants.len(), 1);
    assert_eq!(mutation_outcome::status(&after_mutants[0]), "survived");
    assert_eq!(
        before_mutants[0]["candidate"]["id"],
        after_mutants[0]["candidate"]["id"]
    );

    let before = project.path().join("before.json");
    let after = project.path().join("after.json");
    std::fs::write(&before, &before_output.stdout).unwrap();
    std::fs::write(&after, &after_output.stdout).unwrap();

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command.arg("progress").arg(&before).arg(&after);
    let output = bounded_output(&mut command, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let progress: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(progress["latest"]["state"], "regressing");
    assert_eq!(progress["comparisons"][0]["common"], 1);
    assert_eq!(progress["comparisons"][0]["added"], 0);
    assert_eq!(progress["comparisons"][0]["removed"], 0);
    assert_eq!(progress["comparisons"][0]["improvements"], 0);
    assert_eq!(progress["comparisons"][0]["regressions"], 1);
    assert_eq!(progress["comparisons"][0]["previous_score"], 1.0);
    assert_eq!(progress["comparisons"][0]["current_score"], 0.0);
    assert_eq!(progress["comparisons"][0]["score_delta"], -1.0);
    let detailed = progress_details(&[before.clone(), after.clone()], 100).await;
    let entry = &detailed["details"]["transitions"][0];
    assert_eq!(entry["id"], before_mutants[0]["candidate"]["id"]);
    assert_eq!(entry["path"], "src/calc.py");
    assert_eq!(entry["line"], 2);
    assert_eq!(entry["column"], 16);
    assert_eq!(entry["previous_status"], "killed");
    assert_eq!(entry["current_status"], "survived");
    let improvement = progress_details(&[after, before], 100).await;
    assert_eq!(
        improvement["details"]["transitions"][0]["previous_status"],
        "survived"
    );
    assert_eq!(
        improvement["details"]["transitions"][0]["current_status"],
        "killed"
    );
}

#[tokio::test]
async fn real_run_shifted_ids_preserve_content_fallback_in_jsonl() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    let code = "def add(left: int, right: int) -> int:\n    return left + right\n";
    {
        let mut reports = Vec::new();
        for (phase, prefix) in [("before", ""), ("after", "# moved source\n")] {
            std::fs::write(source.join("calc.py"), format!("{prefix}{code}")).unwrap();
            let mut command = real_binary_run_command(project.path(), "pass");
            let output = bounded_output(&mut command, Duration::from_secs(30))
                .await
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let path = project.path().join(format!("{phase}.jsonl"));
            std::fs::write(&path, output.stdout).unwrap();
            reports.push(path);
        }
        let [InputReport::Usable(before), InputReport::Usable(after)] = [
            read_report(&reports[0]).unwrap(),
            read_report(&reports[1]).unwrap(),
        ] else {
            panic!("public run reports must be usable")
        };
        assert_eq!(before.mutants.len(), 1);
        assert_eq!(after.mutants.len(), 1);
        assert_ne!(
            before.mutants[0].candidate.id,
            after.mutants[0].candidate.id
        );
        let (exit, stdout, stderr) = run_progress(&reports).await;
        assert_eq!(exit, 0);
        let output: Value = serde_json::from_slice(&stdout).unwrap();
        let comparison = &output["comparisons"][0];
        assert_eq!(comparison["common"], 1);
        assert_eq!(comparison["carried_survivors"], 1);
        for field in [
            "added",
            "removed",
            "ambiguous",
            "inconclusive",
            "improvements",
            "regressions",
        ] {
            assert_eq!(comparison[field], 0, "jsonl: {field}");
        }
        assert_eq!(comparison["previous_score"], 0.0);
        assert_eq!(comparison["current_score"], 0.0);
        assert_eq!(comparison["score_delta"], 0.0);
        assert_eq!(output["latest"]["state"], "indeterminate");
        assert_eq!(output["consecutive_stalls"], 0);
        assert!(String::from_utf8_lossy(&stderr).contains("different candidate ID sets"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn real_run_timeout_reaps_the_supervised_process_tree() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(
        project.path().join("pyproject.toml"),
        "[project]\nname = \"hoimin-progress-timeout-fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
    )
    .unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def add(left: int, right: int) -> int:\n    return left + right\n",
    )
    .unwrap();
    let marker = project.path().join("process-tree");
    let mutation_command = format!(
        "import os,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(10)']); Path({:?}).write_text(f'{{os.getpid()}} {{child.pid}}'); time.sleep(10)",
        marker.to_string_lossy()
    );
    let test_command = format!(
        "from pathlib import Path; source=Path('src/calc.py').read_text(); exec('from src.calc import add; assert add(2, 3) == 5') if 'return left + right' in source else exec({mutation_command:?})"
    );
    let mut command = real_binary_command(project.path(), &test_command);

    let result = bounded_output(&mut command, Duration::from_secs(3)).await;
    assert!(result.is_err(), "fixture must exercise timeout cleanup");
    let (root_pid, descendant_pid) = wait_for_process_tree_marker(&marker).await;
    let root_alive = unix_process_is_alive(root_pid);
    let descendant_alive = unix_process_is_alive(descendant_pid);

    force_kill_unix_process(root_pid);
    force_kill_unix_process(descendant_pid);
    wait_for_unix_process_to_stop(root_pid).await;
    wait_for_unix_process_to_stop(descendant_pid).await;

    assert!(
        !root_alive,
        "timed-out verifier process {root_pid} survived"
    );
    assert!(
        !descendant_alive,
        "timed-out verifier descendant {descendant_pid} survived"
    );
}

#[tokio::test]
async fn timed_out_pipe_reader_is_aborted_and_joined() {
    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Arc::new(AtomicBool::new(false));
    let task_flag = Arc::clone(&dropped);
    let reader = tokio::spawn(async move {
        let _drop_flag = DropFlag(task_flag);
        std::future::pending::<()>().await;
        Ok(Vec::new())
    });

    let error = join_reader_with_timeout(reader, "fixture", Duration::from_millis(10))
        .await
        .unwrap_err();

    assert!(error.contains("timed out draining real CLI fixture"));
    assert!(
        dropped.load(Ordering::SeqCst),
        "reader task must be joined after abort"
    );
}

#[rstest::rstest]
#[case::fixture_timed_out_1("fixture timed out")]
#[case::cleanup_status_unreaped_2("cleanup status=unreaped")]
#[case::signal_failed_3("signal failed")]
#[case::reap_failed_4("reap failed")]
#[case::stdout_join_failed_5("stdout join failed")]
#[case::stderr_join_failed_6("stderr join failed")]
fn timeout_diagnostic_retains_cleanup_and_both_reader_failures(#[case] expected: &str) {
    let cleanup = CleanupOutcome {
        status: None,
        diagnostics: vec!["signal failed".to_owned(), "reap failed".to_owned()],
    };

    let diagnostic = cleanup_diagnostic(
        "fixture timed out",
        cleanup,
        Err("stdout join failed".to_owned()),
        Err("stderr join failed".to_owned()),
    );

    assert!(diagnostic.contains(expected), "{diagnostic}");
}

#[test]
fn input_accepts_an_additive_future_normalized_config_object() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]["normalized_config"] = json!({
        "future_field": { "nested": [true, 7, null] }
    });
    let report = write_json(&fixture, "future-config.json", &document);

    assert!(matches!(read_report(&report), Ok(InputReport::Usable(_))));
}

#[test]
fn input_accepts_an_additive_future_verification_selection_field() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]["verification_selection"] = json!({
        "mode": "top",
        "policy": "strict",
        "requested": 3,
        "selected": 2,
        "scope": "retained_candidates",
        "plan_truncated": false,
        "version": 2,
        "enabled": true
    });
    let report = write_json(&fixture, "future-verification-selection.json", &document);

    let result = read_report(&report);
    assert!(matches!(result, Ok(InputReport::Usable(_))), "{result:?}");
}

#[rstest::rstest]
#[case::array_document(json!([]), write_json)]
#[case::array_events(json!([]), write_explicit_jsonl)]
#[case::string_document(json!("config"), write_json)]
#[case::string_events(json!("config"), write_explicit_jsonl)]
#[case::number_document(json!(2), write_json)]
#[case::number_events(json!(2), write_explicit_jsonl)]
#[case::boolean_document(json!(true), write_json)]
#[case::boolean_events(json!(true), write_explicit_jsonl)]
fn input_rejects_non_object_non_null_normalized_configs(
    #[case] value: Value,
    #[case] writer: ReportWriter,
) {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]["normalized_config"] = value;
    let report = writer(&fixture, "invalid-config.json", &document);

    let result = read_report(&report);

    assert!(
        result.is_err(),
        "accepted invalid config in {}",
        report.display()
    );
}

#[rstest::rstest]
#[case::document(write_json)]
#[case::events(write_explicit_jsonl)]
fn input_rejects_a_missing_normalized_config(#[case] writer: ReportWriter) {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]
        .as_object_mut()
        .unwrap()
        .remove("normalized_config");
    let report = writer(&fixture, "missing-config.json", &document);

    let result = read_report(&report);

    assert!(
        result.is_err(),
        "accepted missing config in {}",
        report.display()
    );
}

#[test]
fn input_accepts_a_complete_baseline_success_report() {
    let fixture = tempfile::tempdir().unwrap();
    let report = write_json(&fixture, "complete.json", &valid_report());

    let InputReport::Usable(usable) = read_report(&report).unwrap() else {
        panic!("complete report should be usable");
    };
    assert_eq!(usable.source, report);
    assert_eq!(usable.mutants.len(), 1);
}

#[rstest::rstest]
#[case::duplicate("duplicate", 1, |mutant_id| hoimin_core::ReportSequenceError::DuplicateMutantIdentity { mutant_id, mutant_sequence: 1 })]
#[case::mismatched_sequence("mismatch", 2, |mutant_id| hoimin_core::ReportSequenceError::MutantIdentitySequenceMismatch { mutant_id, expected_sequence: 1, received_sequence: 2 })]
fn input_rejects_repeated_stable_mutant_identities(
    #[case] name: &str,
    #[case] candidate_sequence: usize,
    #[case] expected_error: fn(String) -> hoimin_core::ReportSequenceError,
) {
    let fixture = tempfile::tempdir().unwrap();

    let mut document = valid_report();
    let mut duplicate = document["mutants"][0].clone();
    duplicate["sequence"] = json!(4);
    duplicate["candidate"]["sequence"] = json!(candidate_sequence);
    document["mutants"].as_array_mut().unwrap().push(duplicate);
    document["summary"]["sequence"] = json!(5);
    document["summary"]["counts"]["killed"] = json!(2);

    let report = write_json(&fixture, &format!("{name}.json"), &document);
    let mutant_id = document["mutants"][0]["candidate"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let expected = expected_error(mutant_id);

    let error = read_report(&report).unwrap_err();
    let ReportReadError::InvalidResult { source, .. } = error else {
        panic!("expected lifecycle error: {error}")
    };

    assert_eq!(source, expected);
}

#[rstest::rstest]
#[case::status("status")]
#[case::missing("missing")]
#[case::extra("extra")]
#[case::inconclusive("inconclusive")]
#[case::score("score")]
fn input_rejects_every_summary_mutant_consistency_mismatch(#[case] name: &str) {
    let fixture = tempfile::tempdir().unwrap();
    let document = inconsistent_summary(name);
    let report = write_json(&fixture, &format!("{name}.json"), &document);

    let error = read_report(&report).unwrap_err();

    assert!(matches!(
        error,
        ReportReadError::InvalidStructure {
            message: "summary counts must match mutant events",
            ..
        }
    ));
}

fn inconsistent_summary(name: &str) -> Value {
    let mut cases = Vec::new();

    let mut status = valid_report();
    status["mutants"][0]["outcome"]["executed"]["termination"] = json!({"Exit":0});
    status["mutants"][0]["status"] = json!("survived");
    cases.push(("status", status));

    let mut missing = valid_report();
    missing["mutants"] = json!([]);
    cases.push(("missing", missing));

    let mut extra = valid_report();
    let mut duplicate = extra["mutants"][0].clone();
    duplicate["sequence"] = json!(4);
    duplicate["candidate"]["span"]["start"] = json!(2);
    candidate_identity::assign_id(&mut duplicate["candidate"]);
    duplicate["candidate"]["sequence"] = json!(2);
    extra["mutants"].as_array_mut().unwrap().push(duplicate);
    extra["summary"]["sequence"] = json!(5);
    cases.push(("extra", extra));

    let mut inconclusive = valid_report();
    inconclusive["summary"]["counts"]["inconclusive"] = json!(1);
    cases.push(("inconclusive", inconclusive));

    let mut score = valid_report();
    score["summary"]["counts"]["score"] = json!(0.5);
    cases.push(("score", score));

    cases
        .into_iter()
        .find(|(selected, _)| *selected == name)
        .unwrap()
        .1
}

#[test]
fn input_accepts_a_canonical_null_score_summary() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["mutants"][0]["outcome"]["executed"]["termination"] = json!("Timeout");
    document["mutants"][0]["status"] = json!("timeout");
    document["summary"]["complete"] = json!(false);
    document["summary"]["exit_code"] = json!(4);
    document["summary"]["counts"] = json!({
        "killed": 0,
        "survived": 0,
        "timeout": 1,
        "out_of_memory": 0,
        "process_limit": 0,
        "error": 0,
        "not_run": 0,
        "inconclusive": 1,
        "score": null
    });
    let report = write_json(&fixture, "null-score.json", &document);

    assert!(matches!(
        read_report(&report),
        Ok(InputReport::Unusable {
            reason: UnusableReason::Incomplete,
            ..
        })
    ));
}

#[test]
fn input_rejects_a_non_null_score_without_decidable_mutants() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["mutants"][0]["outcome"]["executed"]["termination"] = json!("Timeout");
    document["mutants"][0]["status"] = json!("timeout");
    document["summary"]["counts"] = json!({
        "killed": 0,
        "survived": 0,
        "timeout": 1,
        "out_of_memory": 0,
        "process_limit": 0,
        "error": 0,
        "not_run": 0,
        "inconclusive": 1,
        "score": 0.0
    });
    let report = write_json(&fixture, "non-null-score.json", &document);

    assert!(matches!(
        read_report(&report),
        Err(ReportReadError::InvalidStructure {
            message: "summary counts must match mutant events",
            ..
        })
    ));
}

#[test]
fn input_marks_missing_baseline_reports_unusable() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["baseline"] = Value::Null;
    document["mutants"] = json!([]);
    document["summary"]["counts"] = serde_json::to_value(hoimin_core::summarize(&[])).unwrap();
    let report = write_json(&fixture, "missing-baseline.json", &document);

    assert!(matches!(
        read_report(&report),
        Ok(InputReport::Unusable {
            reason: UnusableReason::MissingBaseline,
            ..
        })
    ));
}

#[test]
fn input_marks_failed_baseline_reports_unusable() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["baseline"]["termination"] = json!({ "Exit": 1 });
    let report = write_json(&fixture, "failed-baseline.json", &document);

    assert!(matches!(
        read_report(&report),
        Ok(InputReport::Unusable {
            reason: UnusableReason::BaselineFailed,
            ..
        })
    ));
}

#[test]
fn input_marks_incomplete_reports_unusable() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["summary"]["complete"] = json!(false);
    document["summary"]["exit_code"] = json!(4);
    let report = write_json(&fixture, "incomplete.json", &document);

    assert!(matches!(
        read_report(&report),
        Ok(InputReport::Unusable {
            reason: UnusableReason::Incomplete,
            ..
        })
    ));
}

#[test]
fn input_rejects_unsupported_report_schema() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["schema_version"] = json!(REPORT_SCHEMA_VERSION + 1);
    let report = write_json(&fixture, "unsupported-schema.json", &document);

    assert!(read_report(&report).is_err());
}

#[test]
fn input_rejects_unsupported_nested_event_schema() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["baseline"]["schema_version"] = json!(REPORT_SCHEMA_VERSION + 1);
    let report = write_json(&fixture, "unsupported-event-schema.json", &document);

    assert!(read_report(&report).is_err());
}

#[test]
fn input_rejects_malformed_json() {
    let fixture = tempfile::tempdir().unwrap();
    let report = fixture.path().join("malformed.json");
    std::fs::write(&report, b"{ not json").unwrap();

    assert!(read_report(&report).is_err());
}

#[test]
fn input_rejects_invalid_document_structure() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["mutants"] = json!([document["run"].clone()]);
    let report = write_json(&fixture, "invalid-structure.json", &document);

    assert!(read_report(&report).is_err());
}

#[test]
fn input_rejects_events_from_a_different_run() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["summary"]["run_id"] = json!("different-run");
    let report = write_json(&fixture, "different-run.json", &document);

    let error = read_report(&report).unwrap_err();
    assert!(matches!(
        error,
        ReportReadError::InvalidResult {
            source: hoimin_core::ReportSequenceError::RunIdMismatch { .. },
            ..
        }
    ));
}

#[test]
fn input_rejects_nonmonotonic_event_sequences() {
    let fixture = tempfile::tempdir().unwrap();
    let document = valid_report();
    let mut events = jsonl_events(&document);
    events.last_mut().unwrap()["sequence"] = json!(3);
    let report = write_jsonl(&fixture, &events);

    let error = read_report(&report).unwrap_err();
    assert!(matches!(
        error,
        ReportReadError::InvalidResult {
            source: hoimin_core::ReportSequenceError::NotMonotonic { .. },
            ..
        }
    ));
}

#[test]
fn input_rejects_unreadable_paths() {
    let fixture = tempfile::tempdir().unwrap();
    let missing = fixture.path().join("missing.json");

    assert!(read_report(&missing).is_err());
}

#[test]
fn compare_three_adjacent_stalls_are_saturated() {
    let result = compare_reports(&[killed(), killed(), killed(), killed()], nz(3));

    assert_eq!(result.consecutive_stalls, 3);
    assert_eq!(result.latest, ProgressState::Saturated);
}

#[rstest::rstest]
#[case::improvement(vec![survived(), killed()], ProgressState::Improving)]
#[case::regression(vec![killed(), killed(), survived()], ProgressState::Regressing)]
fn compare_improvement_and_regression_break_the_stall_chain(
    #[case] reports: Vec<InputReport>,
    #[case] expected: ProgressState,
) {
    let result = compare_reports(&reports, nz(3));

    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, expected);
}

#[test]
fn compare_regression_between_stalls_breaks_adjacency() {
    let result = compare_reports(&[killed(), killed(), survived(), survived()], nz(2));

    assert_eq!(
        result
            .comparisons
            .iter()
            .map(|comparison| comparison.state)
            .collect::<Vec<_>>(),
        vec![
            ProgressState::Stalled,
            ProgressState::Regressing,
            ProgressState::Stalled,
        ]
    );
    assert_eq!(result.consecutive_stalls, 1);
    assert_eq!(result.latest, ProgressState::Stalled);
}

#[test]
fn compare_an_improvement_resets_prior_stalls() {
    let result = compare_reports(&[killed(), killed(), survived(), killed()], nz(3));

    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Improving);
}

#[test]
fn compare_regression_takes_precedence_over_improvement_in_a_mixed_transition() {
    let result = compare_reports(
        &[
            usable(vec![
                mutant("improvement", MutationStatus::Survived),
                mutant("regression", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant("improvement", MutationStatus::Survived),
                mutant("regression", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant("improvement", MutationStatus::Killed),
                mutant("regression", MutationStatus::Survived),
            ]),
        ],
        nz(3),
    );

    let comparison = &result.comparisons[1];
    assert_eq!(comparison.improvements, 1);
    assert_eq!(comparison.regressions, 1);
    assert_eq!(comparison.state, ProgressState::Regressing);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Regressing);
}

#[test]
fn matching_candidate_ids_pair_mutants_with_the_same_content_key() {
    let same_status = || {
        usable(vec![
            mutant_with_id("stable-a", "shared", MutationStatus::Killed),
            mutant_with_id("stable-b", "shared", MutationStatus::Killed),
        ])
    };
    let result = compare_reports(
        &[
            same_status(),
            same_status(),
            usable(vec![
                mutant_with_id("stable-a", "shared", MutationStatus::Killed),
                mutant_with_id("stable-b", "shared", MutationStatus::Survived),
            ]),
        ],
        nz(3),
    );

    let comparison = &result.comparisons[1];
    assert_eq!(comparison.common, 2);
    assert_eq!(comparison.ambiguous, 0);
    assert_eq!(comparison.regressions, 1);
    assert_eq!(comparison.state, ProgressState::Regressing);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Regressing);
}

#[rstest::rstest]
#[case::path(|_: &mut MutantFinished, after: &mut MutantFinished| after.candidate.path = "other.py".into())]
#[case::original(|_: &mut MutantFinished, after: &mut MutantFinished| after.candidate.original.push('x'))]
#[case::replacement(|_: &mut MutantFinished, after: &mut MutantFinished| after.candidate.replacement.push('x'))]
#[case::operator(|_: &mut MutantFinished, after: &mut MutantFinished| after.candidate.operator.push('x'))]
#[case::symbol(|_: &mut MutantFinished, after: &mut MutantFinished| after.candidate.symbol = Some("other".to_owned()))]
#[case::empty_symbol(|before: &mut MutantFinished, after: &mut MutantFinished| { before.candidate.symbol = None; after.candidate.symbol = Some(String::new()); })]
fn content_fallback_compares_every_identity_field_by_value(
    #[case] change: fn(&mut MutantFinished, &mut MutantFinished),
) {
    let mut before = mutant_with_id("before", "same", MutationStatus::Survived);
    let mut after = mutant_with_id("after", "same", MutationStatus::Killed);
    change(&mut before, &mut after);

    let comparison = compare_pair(&[before], &[after]);
    assert_eq!(comparison.common, 0);
    assert_eq!(comparison.added, 1);
    assert_eq!(comparison.removed, 1);
    assert_eq!(comparison.improvements, 0);
    assert_eq!(comparison.previous_score, None);
    assert_eq!(comparison.current_score, None);
    assert_eq!(comparison.state, ProgressState::Indeterminate);
}

#[test]
fn content_fallback_ignores_non_identity_metadata() {
    // Independent allocations with equal values still match. These metadata
    // fields do not belong to the fallback identity, even if all change.
    let before = mutant_with_id("before", "same", MutationStatus::Survived);
    let mut after = mutant_with_id("after", "same", MutationStatus::Killed);
    after.candidate.sequence = 42;
    after.candidate.span.start = 99;
    after.candidate.span.length = 12;
    after.candidate.line = 100;
    after.candidate.column = 7;
    after.candidate.file_hash = "changed".to_owned();
    let comparison = compare_pair(&[before], &[after]);
    assert_eq!(comparison.common, 1);
    assert_eq!(comparison.added, 0);
    assert_eq!(comparison.removed, 0);
    assert_eq!(comparison.improvements, 1);
    assert_eq!(comparison.previous_score, Some(0.0));
    assert_eq!(comparison.current_score, Some(1.0));
    assert_eq!(comparison.score_delta, Some(1.0));
    assert_eq!(comparison.state, ProgressState::Indeterminate);
}

#[test]
fn content_fallback_unions_ambiguous_and_inconclusive_keys() {
    let before = vec![
        mutant_with_id("a0", "both-duplicate", MutationStatus::Timeout),
        mutant_with_id("a1", "both-duplicate", MutationStatus::Killed),
        mutant_with_id("a2", "after-duplicate", MutationStatus::Timeout),
        mutant_with_id("a3", "uncertain", MutationStatus::Timeout),
        mutant_with_id("a4", "removed", MutationStatus::Error),
        mutant_with_id("a5", "improvement", MutationStatus::Survived),
        mutant_with_id("a6", "survivor", MutationStatus::Survived),
    ];
    let after = vec![
        mutant_with_id("b0", "both-duplicate", MutationStatus::Error),
        mutant_with_id("b1", "both-duplicate", MutationStatus::Survived),
        mutant_with_id("b2", "after-duplicate", MutationStatus::Timeout),
        mutant_with_id("b3", "after-duplicate", MutationStatus::Killed),
        mutant_with_id("b4", "uncertain", MutationStatus::Error),
        mutant_with_id("b5", "added", MutationStatus::NotRun),
        mutant_with_id("b6", "improvement", MutationStatus::Killed),
        mutant_with_id("b7", "survivor", MutationStatus::Survived),
    ];
    assert_eq!(
        compare_pair(&before, &after),
        Comparison {
            common: 3,
            added: 1,
            removed: 1,
            ambiguous: 2,
            inconclusive: 3,
            improvements: 1,
            regressions: 0,
            carried_survivors: 1,
            previous_score: Some(0.0),
            current_score: Some(0.5),
            score_delta: Some(0.5),
            state: ProgressState::Indeterminate,
        }
    );
}

#[test]
fn compare_scores_include_survivors_from_both_reports() {
    let result = compare_reports(
        &[
            usable(vec![
                mutant("killed", MutationStatus::Killed),
                mutant("survived", MutationStatus::Survived),
            ]),
            usable(vec![
                mutant("killed", MutationStatus::Killed),
                mutant("survived", MutationStatus::Survived),
            ]),
        ],
        nz(3),
    );

    let comparison = &result.comparisons[0];
    assert_eq!(comparison.previous_score, Some(0.5));
    assert_eq!(comparison.current_score, Some(0.5));
    assert_eq!(comparison.score_delta, Some(0.0));
}

#[test]
fn compare_counts_added_and_removed_mutants() {
    let result = compare_reports(
        &[
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("removed", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("added", MutationStatus::Survived),
            ]),
        ],
        nz(3),
    );

    let comparison = &result.comparisons[0];
    assert_eq!(comparison.common, 1);
    assert_eq!(comparison.added, 1);
    assert_eq!(comparison.removed, 1);
    assert_eq!(comparison.ambiguous, 0);
    assert_eq!(comparison.state, ProgressState::Indeterminate);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

#[test]
fn changing_candidate_id_sets_break_the_stall_chain() {
    let result = compare_reports(
        &[
            usable(vec![mutant("common", MutationStatus::Killed)]),
            usable(vec![mutant("common", MutationStatus::Killed)]),
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("rotated-a", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant("common", MutationStatus::Killed),
                mutant("rotated-b", MutationStatus::Killed),
            ]),
            usable(vec![mutant("common", MutationStatus::Killed)]),
        ],
        nz(2),
    );

    assert!(
        result
            .comparisons
            .iter()
            .skip(1)
            .all(|comparison| comparison.state == ProgressState::Indeterminate)
    );
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

#[test]
fn duplicate_candidate_ids_are_ineligible() {
    let result = compare_reports(
        &[
            usable(vec![
                mutant_with_id("duplicate-id", "first", MutationStatus::Killed),
                mutant_with_id("duplicate-id", "second", MutationStatus::Killed),
            ]),
            usable(vec![
                mutant_with_id("duplicate-id", "first", MutationStatus::Killed),
                mutant_with_id("duplicate-id", "second", MutationStatus::Killed),
            ]),
        ],
        nz(1),
    );

    assert_eq!(result.comparisons[0].state, ProgressState::Indeterminate);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

#[test]
fn compare_excludes_duplicate_keys_from_the_common_set() {
    let result = compare_reports(
        &[
            usable(vec![
                mutant("duplicate", MutationStatus::Killed),
                duplicate("duplicate", MutationStatus::Survived),
            ]),
            usable(vec![mutant("duplicate", MutationStatus::Survived)]),
        ],
        nz(3),
    );

    let comparison = &result.comparisons[0];
    assert_eq!(comparison.ambiguous, 1);
    assert_eq!(comparison.common, 0);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

#[rstest::rstest]
#[case::mutationstatus_timeout_1(MutationStatus::Timeout)]
#[case::mutationstatus_outofmemory_2(MutationStatus::OutOfMemory)]
#[case::mutationstatus_processlimit_3(MutationStatus::ProcessLimit)]
#[case::mutationstatus_error_4(MutationStatus::Error)]
#[case::mutationstatus_notrun_5(MutationStatus::NotRun)]
fn compare_excludes_each_inconclusive_status_from_judgments_and_scores(
    #[case] status: MutationStatus,
) {
    let result = compare_reports(
        &[
            usable(vec![mutant("common", MutationStatus::Survived)]),
            usable(vec![mutant("common", status)]),
        ],
        nz(3),
    );

    let comparison = &result.comparisons[0];

    assert_eq!(comparison.common, 1, "{status:?}");
    assert_eq!(comparison.inconclusive, 1, "{status:?}");
    assert_eq!(comparison.previous_score, None, "{status:?}");
    assert_eq!(comparison.current_score, None, "{status:?}");
    assert_eq!(result.consecutive_stalls, 0, "{status:?}");
    assert_eq!(result.latest, ProgressState::Indeterminate, "{status:?}");
}

#[test]
fn compare_inconclusive_status_between_stalls_breaks_adjacency() {
    let result = compare_reports(
        &[
            killed(),
            killed(),
            usable(vec![mutant("common", MutationStatus::Timeout)]),
            killed(),
            killed(),
        ],
        nz(2),
    );

    assert_eq!(
        result
            .comparisons
            .iter()
            .map(|comparison| comparison.state)
            .collect::<Vec<_>>(),
        vec![
            ProgressState::Stalled,
            ProgressState::Indeterminate,
            ProgressState::Indeterminate,
            ProgressState::Stalled,
        ]
    );
    assert_eq!(result.consecutive_stalls, 1);
    assert_eq!(result.latest, ProgressState::Stalled);
}

#[test]
fn compare_an_empty_common_set_breaks_the_stall_chain() {
    let result = compare_reports(
        &[
            killed(),
            killed(),
            usable(vec![mutant_with_id(
                "different-id",
                "different",
                MutationStatus::Killed,
            )]),
        ],
        nz(3),
    );

    assert_eq!(result.comparisons.len(), 2);
    assert_eq!(result.comparisons[1].common, 0);
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

#[test]
fn compare_patience_one_saturates_on_the_first_stall() {
    let result = compare_reports(&[killed(), killed()], nz(1));

    assert_eq!(result.consecutive_stalls, 1);
    assert_eq!(result.latest, ProgressState::Saturated);
}

#[test]
fn compare_a_usable_unusable_usable_history_has_no_cross_gap_comparison() {
    let result = compare_reports(&[killed(), unusable(), killed()], nz(3));

    assert_eq!(result.comparisons, Vec::new());
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

proptest! {
    #[test]
    fn compare_property_self_comparison_is_stable(generated in generated_comparison_reports()) {
        let expectations = [
            (Some(0.0), ProgressState::Stalled),
            (None, ProgressState::Indeterminate),
        ];
        let (score_delta, state) = expectations[usize::from(generated.before.is_empty())];
        let expected = comparison_oracle(&generated.before, &generated.before);

        let comparison = compare_pair(&generated.before, &generated.before);

        prop_assert_eq!(&comparison, &expected);
        prop_assert_eq!(comparison.added, 0);
        prop_assert_eq!(comparison.removed, 0);
        prop_assert_eq!(comparison.improvements, 0);
        prop_assert_eq!(comparison.regressions, 0);
        prop_assert_eq!(comparison.score_delta, score_delta);
        prop_assert_eq!(comparison.state, state);
    }

    #[test]
    fn compare_property_reversal_is_antisymmetric_and_order_independent(
        generated in generated_comparison_reports(),
    ) {
        let forward = compare_pair(&generated.before, &generated.after);
        let reverse = compare_pair(&generated.after, &generated.before);
        let expected = comparison_oracle(&generated.before, &generated.after);
        let permuted = compare_pair(&generated.permuted_before, &generated.permuted_after);

        prop_assert_eq!(&forward, &expected);
        prop_assert_eq!(forward.improvements, reverse.regressions);
        prop_assert_eq!(forward.regressions, reverse.improvements);
        prop_assert_eq!(forward.added, reverse.removed);
        prop_assert_eq!(forward.removed, reverse.added);
        prop_assert_eq!(forward.score_delta, reverse.score_delta.map(|delta| -delta));

        prop_assert_eq!(permuted, forward);
    }

    #[test]
    fn compare_property_killed_to_survived_uses_id_despite_duplicate_content(
        (before, after) in duplicate_content_regression_reports(),
    ) {
        let comparison = compare_pair(&before, &after);
        let expected = comparison_oracle(&before, &after);

        // pins: issue #102
        prop_assert_eq!(&comparison, &expected);
        prop_assert_eq!(comparison.common, before.len());
        prop_assert_eq!(comparison.ambiguous, 0);
        prop_assert_eq!(comparison.regressions, 1);
        prop_assert_eq!(comparison.state, ProgressState::Regressing);
    }
}

fn duplicate_input_paths(fixture: &tempfile::TempDir, copied: bool) -> (PathBuf, Vec<PathBuf>) {
    let original = write_jsonl(fixture, &jsonl_events(&valid_report()));

    let mut reports = vec![original.clone()];
    for index in 1..4 {
        let path = if copied {
            let copy = fixture.path().join(format!("copy-{index}"));
            std::fs::copy(&original, &copy).unwrap();
            copy
        } else {
            original.clone()
        };
        reports.push(path);
    }
    (original, reports)
}

fn assert_duplicate_warnings(stderr: Vec<u8>, original: &Path, reports: &[PathBuf], reason: &str) {
    let warnings = String::from_utf8(stderr).unwrap();
    assert_eq!(warnings.lines().count(), 3, "{warnings}");
    for index in 2..=4 {
        assert!(
            warnings.contains(&format!(
                "progress input {index} ({})",
                reports[index - 1].display()
            )),
            "{warnings}"
        );
    }
    assert_eq!(
        warnings
            .matches(&format!("repeats input 1 ({})", original.display()))
            .count(),
        3
    );
    assert!(warnings.contains(reason));
}

#[rstest::rstest]
#[case::same_path(false, "same path")]
#[case::identical_bytes(true, "identical bytes")]
#[tokio::test]
async fn duplicate_input_paths_and_copies_preserve_json_saturation(
    #[case] copied: bool,
    #[case] reason: &str,
) {
    let fixture = tempfile::tempdir().unwrap();
    let (original, reports) = duplicate_input_paths(&fixture, copied);

    let (exit, stdout, stderr) = run_progress(&reports).await;

    assert_eq!(exit, 0);
    assert_duplicate_warnings(stderr, &original, &reports, reason);
    let output: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(output["latest"]["state"], "saturated");
    assert_eq!(output["consecutive_stalls"], 3);
    assert_eq!(output["inputs"].as_array().unwrap().len(), 4);
    assert_eq!(output["comparisons"].as_array().unwrap().len(), 3);
}

#[rstest::rstest]
#[case::independent_run("independent", "stalled")]
#[case::resumed_execution("resumed", "stalled")]
#[case::incomplete_execution("incomplete", "indeterminate")]
#[case::changed_result("changed-result", "regressing")]
#[tokio::test]
async fn duplicate_input_detection_distinguishes_independent_and_resumed_reports(
    #[case] name: &str,
    #[case] expected_state: &str,
) {
    let fixture = tempfile::tempdir().unwrap();
    let original = valid_report();
    let first = write_json(&fixture, "first.json", &original);
    let report = independent_history_report(name, &original);
    let next = write_json(&fixture, &format!("{name}.json"), &report);

    let (exit, stdout, stderr) = run_progress(&[first, next]).await;

    let output: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&stderr));
    assert!(
        !String::from_utf8_lossy(&stderr).contains("repeats input"),
        "{name}"
    );
    assert_eq!(output["latest"]["state"], expected_state);
}

fn independent_history_report(name: &str, original: &Value) -> Value {
    let mut independent = original.clone();
    for field in ["run", "baseline", "summary"] {
        independent[field]["run_id"] = json!("independent-run");
    }
    for mutant in independent["mutants"].as_array_mut().unwrap() {
        mutant["run_id"] = json!("independent-run");
    }
    let mut resumed = original.clone();
    resumed["mutants"][0]["outcome"]["executed"]["elapsed"] = json!({"secs":0,"nanos":42_000_000});
    let mut incomplete = original.clone();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let mut changed_result = original.clone();
    changed_result["mutants"][0]["outcome"]["executed"]["termination"] = json!({"Exit": 0});
    changed_result["mutants"][0]["status"] = json!("survived");
    changed_result["summary"]["counts"]["killed"] = json!(0);
    changed_result["summary"]["counts"]["survived"] = json!(1);
    changed_result["summary"]["counts"]["score"] = json!(0.0);
    changed_result["summary"]["exit_code"] = json!(0);
    [
        ("independent", independent),
        ("resumed", resumed),
        ("incomplete", incomplete),
        ("changed-result", changed_result),
    ]
    .into_iter()
    .find(|(selected, _)| *selected == name)
    .unwrap()
    .1
}

#[tokio::test]
async fn duplicate_detection_ignores_equivalent_line_ending_encodings() {
    let fixture = tempfile::tempdir().unwrap();
    let original = valid_report();
    let first = write_json(&fixture, "first.json", &original);
    let jsonl = write_jsonl(&fixture, &jsonl_events(&original));
    let wire = std::fs::read_to_string(&jsonl)
        .unwrap()
        .replace('\n', "\r\n");
    std::fs::write(&jsonl, wire).unwrap();

    let (exit, _, stderr) = run_progress(&[first, jsonl]).await;

    assert_eq!(exit, 0);
    assert_eq!(stderr, Vec::<u8>::new());
}

#[tokio::test]
async fn duplicate_input_warnings_preserve_unusable_barriers() {
    let fixture = tempfile::tempdir().unwrap();
    let first = write_json(&fixture, "first.json", &valid_report());
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let barrier = write_json(&fixture, "incomplete.json", &incomplete);
    let reports = vec![
        first.clone(),
        first.clone(),
        barrier.clone(),
        barrier,
        first.clone(),
        first,
    ];
    let (exit, stdout, stderr) = run_progress(&reports).await;
    assert_eq!(exit, 0);
    let output: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(output["latest"]["state"], "stalled");
    assert_eq!(output["consecutive_stalls"], 1);
    assert_eq!(output["comparisons"].as_array().unwrap().len(), 2);
    let warnings = String::from_utf8(stderr).unwrap();
    assert!(warnings.lines().next().unwrap().contains("incomplete run"));
    assert_eq!(warnings.matches("repeats input").count(), 4);
}

#[tokio::test]
async fn duplicate_input_warnings_wait_for_all_input_validation() {
    let fixture = tempfile::tempdir().unwrap();
    let first = write_json(&fixture, "first.json", &valid_report());
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let barrier = write_json(&fixture, "incomplete.json", &incomplete);
    let reports = vec![
        first.clone(),
        first.clone(),
        barrier.clone(),
        barrier,
        first.clone(),
        first,
    ];
    let invalid = fixture.path().join("invalid.json");
    std::fs::write(&invalid, "{").unwrap();
    let mut reports = reports;
    reports.push(invalid);
    let (exit, stdout, stderr) = run_progress(&reports).await;
    assert_eq!(exit, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(!String::from_utf8_lossy(&stderr).contains("warning:"));
}

#[tokio::test]
async fn agent_output_progress_defaults_to_json() {
    let fixture = tempfile::tempdir().unwrap();
    let before = write_json(&fixture, "before.json", &valid_report());
    let after = write_json(&fixture, "after.json", &valid_report());
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(
        vec![
            "hoimin".to_owned(),
            "progress".to_owned(),
            before.display().to_string(),
            after.display().to_string(),
        ],
        stdout.clone(),
        stderr.clone(),
    )
    .await;

    assert_eq!(code, 0);
    assert!(String::from_utf8_lossy(&stderr.snapshot()).contains("identical bytes"));
    let output: Value = serde_json::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(output["schema_version"], 3);
    assert_eq!(output["latest"]["state"], "stalled");
    assert_eq!(output["latest"]["patience"], 3);
    assert_eq!(output["latest"]["saturated"], false);
    assert_eq!(output["inputs"].as_array().unwrap().len(), 2);
    assert_eq!(output["inputs"][0]["usable"], true);
    assert_eq!(output["comparisons"].as_array().unwrap().len(), 1);
    assert_eq!(output["comparisons"][0]["current_score"], 1.0);
}

#[tokio::test]
async fn output_json_exposes_agent_decision_fields() {
    let fixture = tempfile::tempdir().unwrap();
    let reports = (0..4)
        .map(|index| {
            let mut report = valid_report();
            for field in ["run", "baseline", "summary"] {
                report[field]["run_id"] = json!(format!("independent-{index}"));
            }
            for mutant in report["mutants"].as_array_mut().unwrap() {
                mutant["run_id"] = json!(format!("independent-{index}"));
            }
            write_json(&fixture, &format!("stalled-{index}.json"), &report)
        })
        .collect::<Vec<_>>();

    let (code, stdout, stderr) = run_progress(&reports).await;
    let value: Value = serde_json::from_slice(&stdout).unwrap();

    assert_eq!(code, 0);
    assert_eq!(stderr, Vec::<u8>::new());
    assert_eq!(value["schema_version"], 3);
    assert_eq!(value["latest"]["state"], "saturated");
    assert_eq!(value["latest"]["consecutive_stalls"], 3);
    assert_eq!(value["latest"]["patience"], 3);
    assert_eq!(value["latest"]["saturated"], true);
    assert_eq!(value["inputs"][0]["usable"], true);
    assert_eq!(value["comparisons"].as_array().unwrap().len(), 3);
    assert_eq!(value["comparisons"][2]["state"], "stalled");
    assert_eq!(value["comparisons"][2]["current_score"], 1.0);
    assert_eq!(value["comparisons"][2]["score_delta"], 0.0);
    for field in [
        "improvements",
        "regressions",
        "carried_survivors",
        "added",
        "removed",
        "ambiguous",
        "inconclusive",
    ] {
        assert_eq!(value["comparisons"][2][field], 0, "{field}");
    }
}

#[tokio::test]
async fn output_preserves_intersection_score_when_candidate_sets_differ() {
    let fixture = tempfile::tempdir().unwrap();
    let mut before = valid_report();
    before["mutants"][0]["outcome"]["executed"]["termination"] = json!({ "Exit": 0 });
    before["mutants"][0]["status"] = json!("survived");
    let mut added = before["mutants"][0].clone();
    added["sequence"] = json!(4);

    added["candidate"]["sequence"] = json!(2);
    added["candidate"]["span"]["start"] = json!(2);
    added["candidate"]["original"] = json!("2");
    candidate_identity::assign_id(&mut added["candidate"]);
    added["outcome"]["executed"]["termination"] = json!({ "Exit": 1 });
    added["status"] = json!("killed");
    before["summary"]["exit_code"] = json!(0);
    before["summary"]["counts"] = json!({
        "killed": 0,
        "survived": 1,
        "timeout": 0,
        "out_of_memory": 0,
        "process_limit": 0,
        "error": 0,
        "not_run": 0,
        "inconclusive": 0,
        "score": 0.0
    });

    let mut after = before.clone();
    after["mutants"].as_array_mut().unwrap().push(added);
    after["summary"]["sequence"] = json!(5);
    after["summary"]["counts"] = json!({
        "killed": 1,
        "survived": 1,
        "timeout": 0,
        "out_of_memory": 0,
        "process_limit": 0,
        "error": 0,
        "not_run": 0,
        "inconclusive": 0,
        "score": 0.5
    });
    let reports = vec![
        write_json(&fixture, "before.json", &before),
        write_json(&fixture, "after.json", &after),
    ];

    let (json_code, stdout, json_stderr) = run_progress(&reports).await;
    let document: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(json_code, 0);
    assert!(String::from_utf8_lossy(&json_stderr).contains("candidate"));
    assert_eq!(document["comparisons"][0]["current_score"], 0.0);
    assert_eq!(document["comparisons"][0]["score_delta"], 0.0);
}

#[tokio::test]
async fn output_latest_state_is_indeterminate_after_an_unusable_report() {
    let fixture = tempfile::tempdir().unwrap();
    let mut before = valid_report();
    before["mutants"][0]["outcome"]["executed"]["termination"] = json!({ "Exit": 0 });
    before["mutants"][0]["status"] = json!("survived");
    before["summary"]["counts"]["killed"] = json!(0);
    before["summary"]["counts"]["survived"] = json!(1);
    before["summary"]["counts"]["score"] = json!(0.0);
    before["summary"]["exit_code"] = json!(0);
    let after = valid_report();
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let reports = vec![
        write_json(&fixture, "before.json", &before),
        write_json(&fixture, "after.json", &after),
        write_json(&fixture, "incomplete.json", &incomplete),
    ];

    let (code, stdout, stderr) = run_progress(&reports).await;
    let output: Value = serde_json::from_slice(&stdout).unwrap();

    assert_eq!(code, 0);
    assert_eq!(output["latest"]["state"], "indeterminate");
    assert_eq!(output["consecutive_stalls"], 0);
    assert_eq!(output["comparisons"].as_array().unwrap().len(), 1);
    assert_eq!(output["comparisons"][0]["state"], "improving");
    assert_eq!(output["inputs"][2]["usable"], false);
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("incomplete run")
    );
}

#[tokio::test]
async fn output_defers_mixed_history_rendering_until_all_reports_are_valid() {
    let fixture = tempfile::tempdir().unwrap();
    let first = write_json(&fixture, "first.json", &valid_report());
    let mut missing_baseline = valid_report();
    missing_baseline["baseline"] = Value::Null;
    missing_baseline["mutants"] = json!([]);
    missing_baseline["summary"]["counts"] =
        serde_json::to_value(hoimin_core::summarize(&[])).unwrap();
    let missing_baseline = write_json(&fixture, "missing-baseline.json", &missing_baseline);
    let before = write_json(&fixture, "before.json", &valid_report());
    let mut different = valid_report();
    different["mutants"][0]["candidate"]["file_hash"] = json!("different-id");
    candidate_identity::assign_id(&mut different["mutants"][0]["candidate"]);
    let different = write_json(&fixture, "different.json", &different);
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let incomplete = write_json(&fixture, "incomplete.json", &incomplete);
    let reports = vec![
        first.clone(),
        missing_baseline.clone(),
        before,
        different,
        incomplete.clone(),
    ];
    let expected_diagnostics = format!(
        "warning: unusable progress report {}: missing baseline\nwarning: unusable progress report {}: incomplete run\nwarning: comparison 1 has different candidate ID sets; progress is indeterminate\nwarning: progress input 3 ({}) repeats input 1 ({}): identical bytes; comparisons are unchanged\n",
        missing_baseline.display(),
        incomplete.display(),
        reports[2].display(),
        first.display(),
    );

    let (code, stdout, stderr) = run_progress(&reports).await;
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8(stderr).unwrap(), expected_diagnostics);
    let document: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(
        document["inputs"],
        json!([
            { "source": first.display().to_string(), "usable": true, "reason": null },
            { "source": missing_baseline.display().to_string(), "usable": false, "reason": "missing_baseline" },
            { "source": reports[2].display().to_string(), "usable": true, "reason": null },
            { "source": reports[3].display().to_string(), "usable": true, "reason": null },
            { "source": incomplete.display().to_string(), "usable": false, "reason": "incomplete" },
        ])
    );
    assert_eq!(document["comparisons"].as_array().unwrap().len(), 1);
    assert_eq!(document["comparisons"][0]["state"], "indeterminate");
}

#[tokio::test]
async fn documented_progress_invocation_accepts_ordered_reports() {
    let fixture = tempfile::tempdir().unwrap();
    let first = write_json(&fixture, "before.json", &valid_report());
    let second = write_json(&fixture, "after.json", &valid_report());
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();

    let code = hoimin_cli::run_with_writers(
        [
            "hoimin",
            "progress",
            "--patience",
            "3",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
        ],
        stdout.clone(),
        stderr.clone(),
    )
    .await;

    assert_eq!(code, 0);
}

#[tokio::test]
async fn output_malformed_json_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let malformed = fixture.path().join("malformed.json");
    std::fs::write(&malformed, b"{ not json").unwrap();
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[malformed, valid]).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("could not parse report")
    );
}

#[tokio::test]
async fn output_late_malformed_report_emits_no_earlier_warnings_or_output() {
    let fixture = tempfile::tempdir().unwrap();
    let valid = write_json(&fixture, "valid.json", &valid_report());
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let incomplete = write_json(&fixture, "incomplete.json", &incomplete);
    let malformed = fixture.path().join("malformed.json");
    std::fs::write(&malformed, b"{ not json").unwrap();

    let (code, stdout, stderr) =
        run_progress(&[valid.clone(), incomplete.clone(), malformed.clone()]).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    let diagnostic = String::from_utf8(stderr).unwrap();
    assert!(
        diagnostic.contains("could not parse report"),
        "{diagnostic}"
    );
    assert!(!diagnostic.contains("warning:"), "{diagnostic}");
}

#[tokio::test]
async fn output_unreadable_path_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let missing = fixture.path().join("missing.json");
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[missing, valid]).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("could not read report")
    );
}

#[tokio::test]
async fn output_unsupported_schema_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let mut unsupported = valid_report();
    unsupported["schema_version"] = json!(REPORT_SCHEMA_VERSION + 1);
    let unsupported = write_json(&fixture, "unsupported.json", &unsupported);
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[unsupported, valid]).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("unsupported schema version")
    );
}

#[tokio::test]
async fn output_invalid_structure_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let mut events = jsonl_events(&valid_report());
    events.swap(1, 2);
    for (index, event) in events.iter_mut().enumerate() {
        event["sequence"] = json!(index + 1);
    }
    let invalid = write_jsonl(&fixture, &events);
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[invalid, valid]).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("invalid structure in report")
    );
}

#[tokio::test]
async fn output_repeated_stable_identity_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let mut invalid = valid_report();
    let mut duplicate = invalid["mutants"][0].clone();
    duplicate["sequence"] = json!(4);
    duplicate["candidate"]["sequence"] = json!(2);
    invalid["mutants"].as_array_mut().unwrap().push(duplicate);
    invalid["summary"]["sequence"] = json!(5);
    invalid["summary"]["counts"]["killed"] = json!(2);
    let invalid = write_json(&fixture, "repeated-identity.json", &invalid);
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[invalid, valid]).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    let diagnostic = String::from_utf8(stderr).unwrap();
    assert!(diagnostic.contains("invalid structure in report"));
    assert!(diagnostic.contains("identity maps to sequence 1, received 2"));
}

#[tokio::test]
async fn output_inconsistent_summary_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let mut invalid = valid_report();
    invalid["summary"]["counts"]["killed"] = json!(0);
    let invalid = write_json(&fixture, "inconsistent-summary.json", &invalid);
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[invalid, valid]).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    let diagnostic = String::from_utf8(stderr).unwrap();
    assert!(diagnostic.contains("invalid structure in report"));
    assert!(diagnostic.contains("summary counts must match mutant events"));
}

#[tokio::test]
async fn output_unusable_reports_are_indeterminate_and_exit_zero() {
    let fixture = tempfile::tempdir().unwrap();
    let mut missing_baseline = valid_report();
    missing_baseline["baseline"] = Value::Null;
    missing_baseline["mutants"] = json!([]);
    missing_baseline["summary"]["counts"] =
        serde_json::to_value(hoimin_core::summarize(&[])).unwrap();
    let missing_baseline = write_json(&fixture, "missing-baseline.json", &missing_baseline);
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let incomplete = write_json(&fixture, "incomplete.json", &incomplete);
    let mut baseline_failed = valid_report();
    baseline_failed["baseline"]["termination"] = json!({ "Exit": 1 });
    let baseline_failed = write_json(&fixture, "baseline-failed.json", &baseline_failed);

    let (code, stdout, stderr) =
        run_progress(&[missing_baseline, incomplete, baseline_failed]).await;
    let value: Value = serde_json::from_slice(&stdout).unwrap();

    assert_eq!(code, 0);
    assert_eq!(value["latest"]["state"], "indeterminate");
    assert_eq!(value["latest"]["consecutive_stalls"], 0);
    assert_eq!(value["inputs"][0]["usable"], false);
    assert_eq!(value["inputs"][0]["reason"], "missing_baseline");
    assert_eq!(value["inputs"][1]["reason"], "incomplete");
    assert_eq!(value["inputs"][2]["reason"], "baseline_failed");
    let diagnostics = String::from_utf8(stderr).unwrap();
    assert!(diagnostics.contains("missing baseline"));
    assert!(diagnostics.contains("incomplete run"));
    assert!(diagnostics.contains("baseline failed"));
}

#[tokio::test]
async fn output_warns_when_candidate_id_sets_differ() {
    let fixture = tempfile::tempdir().unwrap();
    let before = valid_report();
    let mut after = valid_report();
    after["mutants"][0]["candidate"]["file_hash"] = json!("different-id");
    candidate_identity::assign_id(&mut after["mutants"][0]["candidate"]);
    let reports = vec![
        write_json(&fixture, "before.json", &before),
        write_json(&fixture, "after.json", &after),
    ];

    let (code, stdout, stderr) = run_progress(&reports).await;
    let value: Value = serde_json::from_slice(&stdout).unwrap();
    let diagnostics = String::from_utf8(stderr).unwrap();

    assert_eq!(code, 0);
    assert_eq!(value["latest"]["state"], "indeterminate");
    assert_eq!(value["latest"]["consecutive_stalls"], 0);
    assert!(
        diagnostics
            .contains("comparison 1 has different candidate ID sets; progress is indeterminate")
    );
    assert!(value["comparisons"][0].get("candidate_set_match").is_none());
}

#[tokio::test]
async fn output_ambiguity_is_structured_and_warned_on_stderr() {
    let fixture = tempfile::tempdir().unwrap();
    let mut ambiguous = valid_report();
    let first_mutant = ambiguous["mutants"][0].clone();
    let mut duplicate_mutant = first_mutant.clone();
    duplicate_mutant["sequence"] = json!(4);
    duplicate_mutant["candidate"]["span"]["start"] = json!(2);
    candidate_identity::assign_id(&mut duplicate_mutant["candidate"]);
    duplicate_mutant["candidate"]["sequence"] = json!(2);
    ambiguous["mutants"] = json!([first_mutant, duplicate_mutant]);
    ambiguous["summary"]["sequence"] = json!(5);
    ambiguous["summary"]["counts"]["killed"] = json!(2);
    let mut different_ids = ambiguous.clone();
    different_ids["mutants"][1]["candidate"]["span"]["start"] = json!(3);
    candidate_identity::assign_id(&mut different_ids["mutants"][1]["candidate"]);
    let reports = vec![
        write_json(&fixture, "before.json", &ambiguous),
        write_json(&fixture, "after.json", &different_ids),
    ];

    let (code, stdout, stderr) = run_progress(&reports).await;
    let value: Value = serde_json::from_slice(&stdout).unwrap();

    assert_eq!(code, 0);
    assert_eq!(value["comparisons"][0]["ambiguous"], 1);
    assert_eq!(value["latest"]["state"], "indeterminate");
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("comparison 1 has 1 ambiguous mutant key(s)")
    );
}

#[tokio::test]
async fn progress_json_document_matches_its_schema() {
    let fixture = tempfile::tempdir().unwrap();
    let reports = (0..4)
        .map(|index| {
            let mut report = valid_report();
            for field in ["run", "baseline", "summary"] {
                report[field]["run_id"] = json!(format!("independent-{index}"));
            }
            for mutant in report["mutants"].as_array_mut().unwrap() {
                mutant["run_id"] = json!(format!("independent-{index}"));
            }
            write_json(&fixture, &format!("stalled-{index}.json"), &report)
        })
        .collect::<Vec<_>>();
    let (code, stdout, _) = run_progress(&reports).await;
    assert_eq!(code, 0);

    let schema = read_schema(
        &repo_root().join("docs/user/reference/json-schema/progress-result.schema.json"),
    );
    let document: Value = serde_json::from_slice(&stdout).unwrap();
    assert_schema_valid(&schema, &document);

    let mut invalid = document.clone();
    invalid["unexpected"] = json!(true);
    assert_schema_invalid(&schema, &invalid);

    let mut invalid = document.clone();
    invalid["comparisons"][0]["unexpected"] = json!(true);
    assert_schema_invalid(&schema, &invalid);

    let mut invalid = document.clone();
    invalid["comparisons"][0]["state"] = json!("unknown");
    assert_schema_invalid(&schema, &invalid);

    let mut invalid = document.clone();
    invalid["comparisons"][0]["added"] = json!(-1);
    assert_schema_invalid(&schema, &invalid);

    let mut invalid = document.clone();
    invalid["inputs"][0]["reason"] = json!("missing_baseline");
    assert_schema_invalid(&schema, &invalid);

    let mut invalid = document;
    invalid["inputs"][0]["usable"] = json!(false);
    assert_schema_invalid(&schema, &invalid);
}

#[derive(Debug)]
struct GeneratedComparisonReports {
    before: Vec<MutantFinished>,
    after: Vec<MutantFinished>,
    permuted_before: Vec<MutantFinished>,
    permuted_after: Vec<MutantFinished>,
}

fn mutation_status() -> impl Strategy<Value = MutationStatus> {
    prop_oneof![
        Just(MutationStatus::Killed),
        Just(MutationStatus::Survived),
        Just(MutationStatus::Timeout),
        Just(MutationStatus::OutOfMemory),
        Just(MutationStatus::ProcessLimit),
        Just(MutationStatus::Error),
        Just(MutationStatus::NotRun),
    ]
}

fn conclusive_status() -> impl Strategy<Value = MutationStatus> {
    prop_oneof![Just(MutationStatus::Killed), Just(MutationStatus::Survived),]
}

fn generated_comparison_reports() -> impl Strategy<Value = GeneratedComparisonReports> {
    (0usize..=12)
        .prop_flat_map(|len| {
            (
                proptest::collection::vec(
                    (
                        mutation_status(),
                        mutation_status(),
                        0u8..4,
                        any::<u64>(),
                        any::<u64>(),
                    ),
                    len,
                ),
                conclusive_status(),
                conclusive_status(),
                any::<bool>(),
            )
        })
        .prop_map(
            |(mut rows, forced_before, forced_after, different_id_sets)| {
                if let Some(first) = rows.first_mut() {
                    first.0 = forced_before;
                    first.1 = forced_after;
                }
                if rows.len() >= 2 && !different_id_sets {
                    rows[1].2 = rows[0].2;
                }

                let before = generated_mutants(&rows, different_id_sets, |row| row.0);
                let mut after = generated_mutants(&rows, different_id_sets, |row| row.1);
                if different_id_sets && !after.is_empty() {
                    let added = after.last_mut().unwrap();
                    "generated-added".clone_into(&mut added.candidate.id);
                    added.candidate.original = format!("shared-content-{}", rows.len());
                }
                let permuted_before = permute_generated_mutants(&before, &rows, |row| row.3);
                let permuted_after = permute_generated_mutants(&after, &rows, |row| row.4);
                GeneratedComparisonReports {
                    before,
                    after,
                    permuted_before,
                    permuted_after,
                }
            },
        )
}

fn duplicate_content_regression_reports()
-> impl Strategy<Value = (Vec<MutantFinished>, Vec<MutantFinished>)> {
    proptest::collection::vec((mutation_status(), 0u8..4), 1..=11).prop_map(|remaining| {
        let mut before = vec![generated_mutant(0, 0, MutationStatus::Killed)];
        let mut after = vec![generated_mutant(0, 0, MutationStatus::Survived)];
        for (offset, (status, content)) in remaining.into_iter().enumerate() {
            let index = offset + 1;
            let content = if index == 1 { 0 } else { content };
            before.push(generated_mutant(index, usize::from(content), status));
            after.push(generated_mutant(index, usize::from(content), status));
        }
        (before, after)
    })
}

fn generated_mutants<F>(
    rows: &[(MutationStatus, MutationStatus, u8, u64, u64)],
    unique_content: bool,
    status: F,
) -> Vec<MutantFinished>
where
    F: Fn(&(MutationStatus, MutationStatus, u8, u64, u64)) -> MutationStatus,
{
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let content = if unique_content {
                index
            } else {
                usize::from(row.2)
            };
            generated_mutant(index, content, status(row))
        })
        .collect()
}

fn generated_mutant(index: usize, content: usize, status: MutationStatus) -> MutantFinished {
    mutant_with_id(
        &format!("generated-{index}"),
        &format!("shared-content-{content}"),
        status,
    )
}

fn permute_generated_mutants<F>(
    mutants: &[MutantFinished],
    rows: &[(MutationStatus, MutationStatus, u8, u64, u64)],
    rank: F,
) -> Vec<MutantFinished>
where
    F: Fn(&(MutationStatus, MutationStatus, u8, u64, u64)) -> u64,
{
    let mut ranked = mutants
        .iter()
        .cloned()
        .zip(rows.iter())
        .enumerate()
        .map(|(index, (mutant, row))| (rank(row), index, mutant))
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(rank, index, _)| (*rank, *index));
    ranked.into_iter().map(|(_, _, mutant)| mutant).collect()
}

fn compare_pair(before: &[MutantFinished], after: &[MutantFinished]) -> Comparison {
    compare_reports(
        &[usable(before.to_vec()), usable(after.to_vec())],
        nz(usize::MAX),
    )
    .comparisons
    .into_iter()
    .next()
    .expect("two usable reports always produce one comparison")
}

fn comparison_oracle(before: &[MutantFinished], after: &[MutantFinished]) -> Comparison {
    let before_by_id = before
        .iter()
        .map(|mutant| (mutant.candidate.id.as_str(), mutant.outcome.status()))
        .collect::<BTreeMap<_, _>>();
    let after_by_id = after
        .iter()
        .map(|mutant| (mutant.candidate.id.as_str(), mutant.outcome.status()))
        .collect::<BTreeMap<_, _>>();
    let unique_ids = before_by_id.len() == before.len() && after_by_id.len() == after.len();
    let matching_ids = unique_ids && before_by_id.keys().eq(after_by_id.keys());

    let mut common = 0;
    let inconclusive = before_by_id
        .iter()
        .chain(&after_by_id)
        .filter_map(|(id, status)| (!oracle_is_conclusive(*status)).then_some(*id))
        .collect::<BTreeSet<_>>()
        .len();
    let mut improvements = 0;
    let mut regressions = 0;
    let mut carried_survivors = 0;
    let mut previous_killed = 0;
    let mut previous_survived = 0;
    let mut current_killed = 0;
    let mut current_survived = 0;
    for (id, previous) in &before_by_id {
        let Some(current) = after_by_id.get(id) else {
            continue;
        };
        common += 1;
        if !oracle_is_conclusive(*previous) || !oracle_is_conclusive(*current) {
            continue;
        }
        match previous {
            MutationStatus::Killed => previous_killed += 1,
            MutationStatus::Survived => previous_survived += 1,
            _ => unreachable!(),
        }
        match current {
            MutationStatus::Killed => current_killed += 1,
            MutationStatus::Survived => current_survived += 1,
            _ => unreachable!(),
        }
        match (previous, current) {
            (MutationStatus::Survived, MutationStatus::Killed) => improvements += 1,
            (MutationStatus::Killed, MutationStatus::Survived) => regressions += 1,
            (MutationStatus::Survived, MutationStatus::Survived) => carried_survivors += 1,
            (MutationStatus::Killed, MutationStatus::Killed) => {}
            _ => unreachable!(),
        }
    }

    let comparable = previous_killed + previous_survived;
    let previous_score = oracle_score(previous_killed, previous_survived);
    let current_score = oracle_score(current_killed, current_survived);
    let state = if !matching_ids || comparable == 0 {
        ProgressState::Indeterminate
    } else if regressions > 0 {
        ProgressState::Regressing
    } else if improvements > 0 {
        ProgressState::Improving
    } else {
        ProgressState::Stalled
    };

    Comparison {
        common,
        added: after_by_id
            .keys()
            .filter(|id| !before_by_id.contains_key(*id))
            .count(),
        removed: before_by_id
            .keys()
            .filter(|id| !after_by_id.contains_key(*id))
            .count(),
        ambiguous: 0,
        inconclusive,
        improvements,
        regressions,
        carried_survivors,
        previous_score,
        current_score,
        score_delta: previous_score
            .zip(current_score)
            .map(|(previous, current)| current - previous),
        state,
    }
}

fn oracle_is_conclusive(status: MutationStatus) -> bool {
    matches!(status, MutationStatus::Killed | MutationStatus::Survived)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "the independent oracle must model the public f64 score"
)]
fn oracle_score(killed: usize, survived: usize) -> Option<f64> {
    let total = killed + survived;
    (total > 0).then(|| killed as f64 / total as f64)
}

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn killed() -> InputReport {
    usable(vec![mutant("common", MutationStatus::Killed)])
}

fn survived() -> InputReport {
    usable(vec![mutant("common", MutationStatus::Survived)])
}

fn usable(mutants: Vec<MutantFinished>) -> InputReport {
    InputReport::Usable(UsableReport {
        source: PathBuf::from("report.json"),
        mutants,
    })
}

fn unusable() -> InputReport {
    InputReport::Unusable {
        source: PathBuf::from("unusable.json"),
        reason: UnusableReason::Incomplete,
    }
}

fn duplicate(key: &str, status: MutationStatus) -> MutantFinished {
    let mut mutant = mutant(key, status);
    mutant.candidate.id = format!("duplicate-{key}");
    mutant
}

fn mutant(key: &str, status: MutationStatus) -> MutantFinished {
    MutantFinished {
        schema_version: REPORT_SCHEMA_VERSION,
        sequence: 1,
        run_id: "run".to_owned(),
        candidate: MutationCandidate {
            id: key.to_owned(),
            sequence: 1,
            path: Utf8PathBuf::from("src/example.py"),
            span: ByteSpan {
                start: 0,
                length: 1,
            },
            original: key.to_owned(),
            replacement: "replacement".to_owned(),
            operator: "operator".to_owned(),
            line: 1,
            column: 0,
            symbol: Some("symbol".to_owned()),
            file_hash: "hash".to_owned(),
        },
        outcome: match status {
            hoimin_core::MutationStatus::Killed => hoimin_core::MutationOutcome::Reused {
                status: hoimin_core::ReusedMutationStatus::Killed,
            },
            hoimin_core::MutationStatus::Survived => hoimin_core::MutationOutcome::Reused {
                status: hoimin_core::ReusedMutationStatus::Survived,
            },
            hoimin_core::MutationStatus::NotRun => hoimin_core::MutationOutcome::NotRun,
            status => hoimin_core::MutationOutcome::Executed {
                termination: match status {
                    hoimin_core::MutationStatus::Timeout => {
                        hoimin_core::ProcessTermination::Timeout
                    }
                    hoimin_core::MutationStatus::OutOfMemory => {
                        hoimin_core::ProcessTermination::OutOfMemory
                    }
                    hoimin_core::MutationStatus::ProcessLimit => {
                        hoimin_core::ProcessTermination::ProcessLimit
                    }
                    hoimin_core::MutationStatus::Error => hoimin_core::ProcessTermination::Exit(1),
                    _ => unreachable!(),
                },
                output_state: if status == hoimin_core::MutationStatus::Error {
                    hoimin_core::ProcessOutputState::CloseTimedOut
                } else {
                    hoimin_core::ProcessOutputState::Complete
                },
                output: hoimin_core::OutputSpoolRef {
                    token: "fixture-output".into(),
                    retained: 0,
                    observed: 0,
                },
                elapsed: std::time::Duration::from_millis(1),
            },
        },
        resource_mode: ResourceMode::Hard,
        diagnostics: Vec::new(),
    }
}

fn mutant_with_id(id: &str, semantic_key: &str, status: MutationStatus) -> MutantFinished {
    let mut value = mutant(semantic_key, status);
    id.clone_into(&mut value.candidate.id);
    value
}

fn write_json(fixture: &tempfile::TempDir, name: &str, document: &Value) -> PathBuf {
    let report = fixture.path().join(name);
    let mut events = jsonl_events(document);
    if document["schema_version"] != REPORT_SCHEMA_VERSION {
        events[0]["schema_version"] = document["schema_version"].clone();
    }
    std::fs::write(
        &report,
        events
            .iter()
            .map(|event| event.to_string() + "\n")
            .collect::<String>(),
    )
    .unwrap();
    report
}

async fn run_progress(reports: &[PathBuf]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut argv = vec!["hoimin".to_owned(), "progress".to_owned()];
    argv.extend(
        reports
            .iter()
            .map(|report| report.to_str().unwrap().to_owned()),
    );
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(argv, stdout.clone(), stderr.clone()).await;
    (code, stdout.snapshot(), stderr.snapshot())
}

async fn run_real_binary(project: &Path, test_command: &str, timeout: Duration) -> Output {
    let mut command = real_binary_command(project, test_command);
    bounded_output(&mut command, timeout).await.unwrap()
}

fn real_binary_command(project: &Path, test_command: &str) -> tokio::process::Command {
    real_binary_run_command(project, test_command)
}

fn real_binary_run_command(project: &Path, test_command: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .arg("run")
        .arg("--min-free-space")
        .arg(TEST_MIN_FREE_SPACE)
        .arg("--root")
        .arg(project)
        .arg("--source")
        .arg("src")
        .arg("--file")
        .arg("src/calc.py")
        .arg("--operators")
        .arg("binary_add_sub")
        .arg("--max-mutants")
        .arg("1")
        .arg("--allow-best-effort-memory")
        .arg("--")
        .arg(python_executable())
        .arg("-c")
        .arg(test_command);
    command
}

async fn bounded_output(
    command: &mut tokio::process::Command,
    timeout: Duration,
) -> Result<Output, String> {
    command
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "real CLI stdout was not piped".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "real CLI stderr was not piped".to_owned())?;
    let stdout = tokio::spawn(read_all(stdout));
    let stderr = tokio::spawn(read_all(stderr));

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            let cleanup = cleanup_timed_out_child(&mut child).await;
            let stdout = join_reader(stdout, "stdout").await;
            let stderr = join_reader(stderr, "stderr").await;
            return Err(cleanup_diagnostic(
                &format!("wait for real CLI child failed: {error}"),
                cleanup,
                stdout,
                stderr,
            ));
        }
        Err(_) => {
            let cleanup = cleanup_timed_out_child(&mut child).await;
            let stdout = join_reader(stdout, "stdout").await;
            let stderr = join_reader(stderr, "stderr").await;
            return Err(cleanup_diagnostic(
                "real CLI command timed out",
                cleanup,
                stdout,
                stderr,
            ));
        }
    };
    let stdout = join_reader(stdout, "stdout").await;
    let stderr = join_reader(stderr, "stderr").await;
    match (stdout, stderr) {
        (Ok(stdout), Ok(stderr)) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        (stdout, stderr) => Err(reader_diagnostic(stdout, stderr)),
    }
}

async fn read_all<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    Ok(bytes)
}

async fn join_reader(
    reader: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, String> {
    join_reader_with_timeout(reader, name, Duration::from_secs(5)).await
}

async fn join_reader_with_timeout(
    mut reader: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
    name: &str,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    if let Ok(result) = tokio::time::timeout(timeout, &mut reader).await {
        result
            .map_err(|error| format!("join real CLI {name} reader: {error}"))?
            .map_err(|error| format!("read real CLI {name}: {error}"))
    } else {
        reader.abort();
        let join_error = match tokio::time::timeout(Duration::from_secs(1), &mut reader).await {
            Ok(Err(error)) if error.is_cancelled() => None,
            Ok(Ok(Ok(_))) => Some("reader completed after its abort request".to_owned()),
            Ok(Ok(Err(error))) => Some(format!("reader failed after abort: {error}")),
            Ok(Err(error)) => Some(format!("join reader after abort: {error}")),
            Err(_) => Some("timed out joining reader after abort".to_owned()),
        };
        let mut message = format!(
            "timed out draining real CLI {name}; reader aborted and bounded join attempted"
        );
        if let Some(error) = join_error {
            message.push_str("; ");
            message.push_str(&error);
        }
        Err(message)
    }
}

#[derive(Default)]
struct CleanupOutcome {
    status: Option<std::process::ExitStatus>,
    diagnostics: Vec<String>,
}

impl CleanupOutcome {
    fn record(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.diagnostics.push(error);
        }
    }
}

#[cfg(unix)]
async fn cleanup_timed_out_child(child: &mut tokio::process::Child) -> CleanupOutcome {
    let mut outcome = CleanupOutcome::default();
    if observe_child_exit(child, &mut outcome, "before first SIGINT") {
        return outcome;
    }
    let signal = send_sigint_to_test_child(child);
    outcome.record(signal);
    if wait_for_child_exit(
        child,
        Duration::from_secs(5),
        "after first SIGINT",
        &mut outcome,
    )
    .await
    {
        return outcome;
    }

    if !observe_child_exit(child, &mut outcome, "before second SIGINT") {
        let signal = send_sigint_to_test_child(child);
        outcome.record(signal);
    }
    if wait_for_child_exit(
        child,
        Duration::from_secs(1),
        "after second SIGINT",
        &mut outcome,
    )
    .await
    {
        return outcome;
    }

    force_kill_and_reap(child, &mut outcome).await;
    outcome
}

#[cfg(unix)]
fn send_sigint_to_test_child(child: &tokio::process::Child) -> Result<(), String> {
    let pid = child
        .id()
        .ok_or_else(|| "real CLI child exited before SIGINT".to_owned())?;
    let pid = i32::try_from(pid).map_err(|error| error.to_string())?;
    // SAFETY: the positive PID belongs to the retained child spawned by this test.
    if unsafe { libc::kill(pid, libc::SIGINT) } != 0 {
        return Err(format!(
            "SIGINT real CLI child: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(windows)]
async fn cleanup_timed_out_child(child: &mut tokio::process::Child) -> CleanupOutcome {
    let mut outcome = CleanupOutcome::default();
    if observe_child_exit(child, &mut outcome, "before taskkill") {
        return outcome;
    }
    let mut taskkill_succeeded = false;
    if let Some(pid) = child.id() {
        let pid = pid.to_string();
        let mut taskkill = tokio::process::Command::new("taskkill");
        taskkill.args(["/PID", &pid, "/T", "/F"]).kill_on_drop(true);
        match tokio::time::timeout(Duration::from_secs(5), taskkill.output()).await {
            Ok(Ok(output)) if output.status.success() => taskkill_succeeded = true,
            Ok(Ok(output)) => outcome.diagnostics.push(format!(
                "taskkill real CLI process tree failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )),
            Ok(Err(error)) => outcome
                .diagnostics
                .push(format!("start taskkill for real CLI process tree: {error}")),
            Err(_) => outcome
                .diagnostics
                .push("timed out terminating real CLI Windows process tree".to_owned()),
        }
    } else {
        outcome
            .diagnostics
            .push("real CLI child exited before process-tree cleanup".to_owned());
    }
    if taskkill_succeeded
        && wait_for_child_exit(
            child,
            Duration::from_secs(5),
            "after taskkill",
            &mut outcome,
        )
        .await
    {
        return outcome;
    }

    force_kill_and_reap(child, &mut outcome).await;
    outcome
}

fn observe_child_exit(
    child: &mut tokio::process::Child,
    outcome: &mut CleanupOutcome,
    context: &str,
) -> bool {
    match child.try_wait() {
        Ok(Some(status)) => {
            outcome.status = Some(status);
            true
        }
        Ok(None) => false,
        Err(error) => {
            outcome
                .diagnostics
                .push(format!("inspect real CLI child {context}: {error}"));
            false
        }
    }
}

async fn wait_for_child_exit(
    child: &mut tokio::process::Child,
    timeout: Duration,
    context: &str,
    outcome: &mut CleanupOutcome,
) -> bool {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => {
            outcome.status = Some(status);
            true
        }
        Ok(Err(error)) => {
            outcome
                .diagnostics
                .push(format!("wait for real CLI child {context}: {error}"));
            false
        }
        Err(_) => {
            outcome
                .diagnostics
                .push(format!("timed out waiting for real CLI child {context}"));
            false
        }
    }
}

async fn force_kill_and_reap(child: &mut tokio::process::Child, outcome: &mut CleanupOutcome) {
    if !observe_child_exit(child, outcome, "before final kill") {
        let kill = child
            .start_kill()
            .map_err(|error| format!("force-kill real CLI child: {error}"));
        outcome.record(kill);
    }
    if outcome.status.is_none() {
        wait_for_child_exit(child, Duration::from_secs(5), "after final kill", outcome).await;
    }
}

fn cleanup_diagnostic(
    reason: &str,
    cleanup: CleanupOutcome,
    stdout: Result<Vec<u8>, String>,
    stderr: Result<Vec<u8>, String>,
) -> String {
    let mut diagnostics = vec![format!(
        "{reason}; cleanup status={}",
        cleanup
            .status
            .map_or_else(|| "unreaped".to_owned(), |status| status.to_string())
    )];
    diagnostics.extend(cleanup.diagnostics);
    match stdout {
        Ok(stdout) => diagnostics.push(format!("stdout={}", String::from_utf8_lossy(&stdout))),
        Err(error) => diagnostics.push(error),
    }
    match stderr {
        Ok(stderr) => diagnostics.push(format!("stderr={}", String::from_utf8_lossy(&stderr))),
        Err(error) => diagnostics.push(error),
    }
    diagnostics.join("; ")
}

fn reader_diagnostic(stdout: Result<Vec<u8>, String>, stderr: Result<Vec<u8>, String>) -> String {
    [stdout.err(), stderr.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(unix)]
async fn wait_for_process_tree_marker(marker: &Path) -> (i32, i32) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(pids) = std::fs::read_to_string(marker) {
            let mut pids = pids
                .split_whitespace()
                .map(|pid| pid.parse::<i32>().unwrap());
            return (pids.next().unwrap(), pids.next().unwrap());
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed-out run did not publish its process tree marker"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
fn unix_process_is_alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes a positive PID written by this test fixture.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(unix)]
fn force_kill_unix_process(pid: i32) {
    if unix_process_is_alive(pid) {
        // SAFETY: the PID belongs to this test's timed-out fixture process tree.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
async fn wait_for_unix_process_to_stop(pid: i32) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while unix_process_is_alive(pid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn python_executable() -> PathBuf {
    let executable = if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    };
    assert!(
        executable.is_file(),
        "missing controlled test Python interpreter: {}",
        executable.display()
    );
    executable
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn read_schema(path: &Path) -> Value {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("read schema {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse schema {}: {error}", path.display()))
}

fn assert_schema_valid(schema: &Value, instance: &Value) {
    if let Err(error) = validate_schema(schema, instance, schema, "$") {
        panic!("schema validation failed: {error}\ninstance: {instance}");
    }
}

fn assert_schema_invalid(schema: &Value, instance: &Value) {
    assert!(
        validate_schema(schema, instance, schema, "$").is_err(),
        "invalid instance unexpectedly matched schema: {instance}"
    );
}

fn validate_schema(
    schema: &Value,
    instance: &Value,
    active_root: &Value,
    path: &str,
) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("{path}: unsupported schema reference {reference}"))?;
        let target = active_root
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: unresolved schema reference {reference}"))?;
        return validate_schema(target, instance, active_root, path);
    }

    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        let matches = branches
            .iter()
            .filter(|branch| validate_schema(branch, instance, active_root, path).is_ok())
            .count();
        if matches != 1 {
            return Err(format!(
                "{path}: expected exactly one oneOf match, got {matches}"
            ));
        }
    }

    if let Some(expected) = schema.get("const")
        && instance != expected
    {
        return Err(format!("{path}: expected const {expected}, got {instance}"));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.contains(instance)
    {
        return Err(format!("{path}: {instance} is not in enum"));
    }
    if let Some(expected) = schema.get("type") {
        let matches = match expected {
            Value::String(kind) => instance_has_type(instance, kind),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| instance_has_type(instance, kind)),
            _ => false,
        };
        if !matches {
            return Err(format!("{path}: {instance} does not have type {expected}"));
        }
    }
    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64)
        && instance.as_f64().is_some_and(|value| value < minimum)
    {
        return Err(format!("{path}: number is below {minimum}"));
    }
    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64)
        && instance.as_f64().is_some_and(|value| value > maximum)
    {
        return Err(format!("{path}: number is above {maximum}"));
    }

    if let Some(object) = instance.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(format!("{path}: missing required property {name}"));
                }
            }
        }
        if let Some(properties) = properties {
            for (name, value) in object {
                if let Some(property_schema) = properties.get(name) {
                    validate_schema(
                        property_schema,
                        value,
                        active_root,
                        &format!("{path}.{name}"),
                    )?;
                } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                    return Err(format!("{path}: unexpected property {name}"));
                }
            }
        }
    }

    if let (Some(items), Some(values)) = (schema.get("items"), instance.as_array()) {
        for (index, value) in values.iter().enumerate() {
            validate_schema(items, value, active_root, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

fn instance_has_type(instance: &Value, kind: &str) -> bool {
    match kind {
        "null" => instance.is_null(),
        "boolean" => instance.is_boolean(),
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "number" => instance.is_number(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "string" => instance.is_string(),
        _ => false,
    }
}

#[rstest::rstest]
#[case::identity_jsonl_replacement("/replacement", json!("obj.extend([(item)])"))]
#[case::identity_jsonl_operator("/operator", json!("other"))]
#[case::identity_jsonl_path("/path", json!("other.py"))]
#[case::identity_jsonl_hash("/file_hash", json!("different"))]
#[case::identity_jsonl_span_start("/span/start", json!(33))]
#[case::identity_jsonl_span_length("/span/length", json!(19))]
#[tokio::test]
async fn inconsistent_candidate_identity_is_rejected(#[case] pointer: &str, #[case] value: Value) {
    // use a published real candidate, retaining its ID after tampering.
    let fixture = tempfile::tempdir().unwrap();
    let candidates: Value =
        serde_json::from_str(include_str!("fixtures/method-replacements/candidates.json")).unwrap();
    let mut document = valid_report();
    document["mutants"][0]["candidate"] = candidates[0].clone();
    *document["mutants"][0]["candidate"]
        .pointer_mut(pointer)
        .unwrap() = value;
    let path = write_jsonl(&fixture, &jsonl_events(&document));

    let (code, stdout, stderr) = run_progress(&[path.clone(), path.clone()]).await;
    // reject before either regression or saturation can be rendered.
    assert_eq!(code, 2, "{}", String::from_utf8_lossy(&stderr));
    assert_eq!(stdout, Vec::<u8>::new());
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains(path.to_str().unwrap()));
    assert!(stderr.contains("candidate identity conflict"), "{stderr}");
}

#[rstest::rstest]
#[case::identity_jsonl_accepts_line("line", json!(99))]
#[case::identity_jsonl_accepts_column("column", json!(99))]
#[case::identity_jsonl_accepts_symbol("symbol", json!("different"))]
#[case::identity_jsonl_accepts_original("original", json!("different"))]
#[case::identity_jsonl_accepts_sequence("sequence", json!(99))]
#[tokio::test]
async fn display_metadata_preserves_candidate_identity(#[case] field: &str, #[case] value: Value) {
    // display metadata changes while the stable mutation stays fixed.
    let fixture = tempfile::tempdir().unwrap();
    let mut after = detail_report(&[("a", MutationStatus::Killed)]);
    after["mutants"][0]["candidate"][field] = value;
    let before = write_json(
        &fixture,
        "before.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let after = write_jsonl(&fixture, &jsonl_events(&after));

    let (code, stdout, stderr) = run_progress(&[before, after]).await;

    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    let value: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(value["latest"]["state"], "stalled");
}

#[tokio::test]
async fn identity_jsonl_rejects_stolen_id_before_regression() {
    // separate real mutations from the same append call, killed -> survived.
    let fixture = tempfile::tempdir().unwrap();
    let candidates: Value =
        serde_json::from_str(include_str!("fixtures/method-replacements/candidates.json")).unwrap();
    let mut before = detail_report(&[("a", MutationStatus::Killed)]);
    before["mutants"][0]["candidate"] = candidates[0].clone();
    let mut after = detail_report(&[("a", MutationStatus::Survived)]);
    after["mutants"][0]["candidate"] = candidates[1].clone();
    after["mutants"][0]["candidate"]["id"] = candidates[0]["id"].clone();
    let before = write_json(&fixture, "before.json", &before);
    let after = write_jsonl(&fixture, &jsonl_events(&after));

    let (code, stdout, stderr) = progress_output(&[before, after.clone()], true).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    let diagnostic = String::from_utf8(stderr).unwrap();
    assert!(diagnostic.contains(after.to_str().unwrap()));
    assert!(diagnostic.contains("candidate identity conflict"));
}

#[tokio::test]
async fn identity_jsonl_accepts_status_change() {
    // a legitimate same-ID status transition remains a regression.
    let fixture = tempfile::tempdir().unwrap();
    let before = write_json(
        &fixture,
        "before.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let after = detail_report(&[("a", MutationStatus::Survived)]);
    let after = write_jsonl(&fixture, &jsonl_events(&after));

    let (code, stdout, stderr) = progress_output(&[before, after], true).await;

    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    let result: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(result["latest"]["state"], "regressing");
    assert_eq!(result["comparisons"][0]["regressions"], 1);
}

#[tokio::test]
async fn identity_jsonl_accepts_independent_discovery_order() {
    // two independently discovered orders of the same genuine identities.
    let fixture = tempfile::tempdir().unwrap();
    let before = detail_report(&[("a", MutationStatus::Killed), ("b", MutationStatus::Killed)]);
    let after = detail_report(&[
        ("b", MutationStatus::Survived),
        ("a", MutationStatus::Survived),
    ]);
    let before = write_json(&fixture, "before.json", &before);
    let after = write_jsonl(&fixture, &jsonl_events(&after));

    let (code, stdout, stderr) = progress_output(&[before, after], true).await;

    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    let result: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(result["details"]["eligibility"], "matching");
    assert_eq!(result["comparisons"][0]["common"], 2);
    assert_eq!(result["comparisons"][0]["regressions"], 2);
}

fn valid_report() -> Value {
    let mut document = json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "run": {
            "kind": "run_started",
            "schema_version": REPORT_SCHEMA_VERSION,
            "sequence": 1,
            "run_id": "run-1",
            "normalized_config": null,
            "versions": { "os": "test", "hoimin": "test" },
            "resource_control": { "mode": "hard", "mechanism": "test" }
        },
        "baseline": {
            "kind": "baseline_finished",
            "schema_version": REPORT_SCHEMA_VERSION,
            "sequence": 2,
            "run_id": "run-1",
            "termination": { "Exit": 0 },
            "elapsed_ms": 1,
            "resource_mode": "hard",
            "output": { "token": "baseline", "retained": 0, "observed": 0 }
        },
        "mutants": [{
            "kind": "mutant_finished",
            "schema_version": REPORT_SCHEMA_VERSION,
            "sequence": 3,
            "run_id": "run-1",
            "candidate": {
                "id": "mutant-1",
                "sequence": 1,
                "path": "src/example.py",
                "span": { "start": 0, "length": 1 },
                "original": "1",
                "replacement": "0",
                "operator": "integer_literal",
                "line": 1,
                "column": 0,
                "symbol": null,
                "file_hash": "hash"
            },
            "outcome": { "executed": { "termination": {"Exit":1}, "output_state":"complete", "elapsed":{"secs":0,"nanos":1_000_000}, "output":{"token":"mutant-1","retained":0,"observed":0} } },
            "resource_mode": "hard",
            "status": "killed"
        }],
        "summary": {
            "kind": "run_finished",
            "resolved_mutant_timeout": {"secs": 5, "nanos": 0},
            "schema_version": REPORT_SCHEMA_VERSION,
            "sequence": 4,
            "run_id": "run-1",
            "counts": {
                "killed": 1,
                "survived": 0,
                "timeout": 0,
                "out_of_memory": 0,
                "process_limit": 0,
                "error": 0,
                "not_run": 0,
                "inconclusive": 0,
                "score": 1.0
            },
            "complete": true,
            "exit_code": 0,
            "disk": serde_json::to_value(hoimin_core::DiskRunSummary::unmeasured(8, 10)).unwrap()
        }
    });
    candidate_identity::assign_id(&mut document["mutants"][0]["candidate"]);
    document
}

fn result_report(close_timeout: bool) -> Value {
    let mut document = valid_report();
    if close_timeout {
        let mutant = &mut document["mutants"][0];
        mutant["outcome"]["executed"]["termination"] = json!({"Exit": 0});
        mutant["outcome"]["executed"]["output_state"] = json!("close_timed_out");
        mutant["status"] = json!("error");
        mutant["outcome"]["executed"]["output"] =
            json!({"token":"partial","retained":0,"observed":0});
        mutant["diagnostics"] = json!([{
            "mutant_id": mutant["candidate"]["id"], "level": "error",
            "code": hoimin_core::PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE,
            "message": "captured output may be incomplete"
        }]);
        document["summary"]["counts"] =
            serde_json::to_value(hoimin_core::summarize(&[MutationStatus::Error])).unwrap();
        document["summary"]["complete"] = json!(false);
        document["summary"]["exit_code"] = json!(2);
    }
    document
}

#[rstest::rstest]
#[case::input_requires_executed_termination("termination")]
#[case::input_requires_executed_output_state("output_state")]
#[case::input_requires_executed_output("output")]
#[case::input_requires_executed_elapsed("elapsed")]
fn input_requires_each_executed_outcome_field(#[case] field: &str) {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = result_report(true);
    document["mutants"][0]["outcome"]["executed"]
        .as_object_mut()
        .unwrap()
        .remove(field);
    let path = write_json(&fixture, "missing-field.json", &document);

    let result = read_report(&path);

    assert!(matches!(result, Err(ReportReadError::Parse { .. })));
}

#[test]
fn input_preserves_output_error_override() {
    let fixture = tempfile::tempdir().unwrap();
    let path = write_json(&fixture, "output-error.json", &result_report(true));

    let result = read_report(&path);

    assert!(matches!(
        result,
        Ok(InputReport::Unusable {
            reason: UnusableReason::Incomplete,
            ..
        })
    ));
}

#[test]
fn input_accepts_reused_conclusive_outcome() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = result_report(false);
    document["mutants"][0]["outcome"] = json!({"reused":{"status":"killed"}});
    let path = write_json(&fixture, "reused.json", &document);

    let result = read_report(&path);

    assert!(matches!(result, Ok(InputReport::Usable(_))));
}

fn jsonl_events(document: &Value) -> Vec<Value> {
    let mut events = vec![document["run"].clone()];
    if !document["baseline"].is_null() {
        events.push(document["baseline"].clone());
    }
    for mutant in document["mutants"].as_array().unwrap() {
        events.push(json!({
            "kind": "mutant_started", "schema_version": REPORT_SCHEMA_VERSION,
            "sequence": 0, "run_id": document["run"]["run_id"],
            "mutant_id": mutant["candidate"]["id"],
            "mutant_sequence": mutant["candidate"]["sequence"]
        }));
        events.push(mutant.clone());
    }
    events.push(document["summary"].clone());
    for (index, event) in events.iter_mut().enumerate() {
        event["sequence"] = json!(index + 1);
    }
    events
}

#[rstest::rstest]
#[case::retired_member("retired", "unknown field `stale_roots_reclaimed`")]
#[case::missing_janitor("missing", "missing field `janitor`")]
#[tokio::test]
async fn public_jsonl_disk_rejects_invalid_fields(
    #[case] defect: &str,
    #[case] expected_error: &str,
) {
    let fixture = tempfile::tempdir().unwrap();
    let path = disk_wire_fixture(&fixture, defect);

    let output = public_progress_output(&path).await;

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains(expected_error), "{stderr}");
    assert_eq!(output.stdout, Vec::<u8>::new());
}

#[tokio::test]
async fn public_jsonl_disk_accepts_current_null() {
    let fixture = tempfile::tempdir().unwrap();
    let path = disk_wire_fixture(&fixture, "current");

    let output = public_progress_output(&path).await;

    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        result["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|input| input["usable"] == true)
    );
}

async fn public_progress_output(path: &Path) -> Output {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .arg("progress")
        .arg(path)
        .arg(path)
        .output()
        .await
        .unwrap()
}

fn disk_wire_fixture(fixture: &tempfile::TempDir, defect: &str) -> PathBuf {
    let source = repo_root().join("crates/hoimin-cli/tests/golden/events/schema-v10-current.jsonl");
    let source = std::fs::read_to_string(source).unwrap();
    let mut values: Vec<Value> = {
        source
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    let disk = {
        &mut values
            .iter_mut()
            .find(|event| event["kind"] == "run_finished")
            .unwrap()["disk"]
    };
    assert_eq!(disk.get("janitor"), Some(&Value::Null));
    match defect {
        "retired" => disk["stale_roots_reclaimed"] = json!(999),
        "missing" => {
            disk.as_object_mut().unwrap().remove("janitor");
        }
        "current" => {}
        _ => unreachable!(),
    }
    write_jsonl(fixture, &values)
}

#[tokio::test]
async fn public_jsonl_rejects_contradictory_status() {
    // valid identities, janitor and summary isolate the status projection.
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["mutants"][0]["status"] = json!("killed");
    let valid = write_jsonl(&fixture, &jsonl_events(&document));
    assert!(matches!(read_report(&valid), Ok(InputReport::Usable(_))));
    document["mutants"][0]["status"] = json!("survived");
    let path = write_jsonl(&fixture, &jsonl_events(&document));
    // exercise the real executable, including public error mapping.
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .arg("progress")
        .arg(&path)
        .arg(&path)
        .output()
        .await
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("status survived does not match outcome status killed"),
        "{stderr}"
    );
}

#[rstest::rstest]
#[case::jsonl_requires_status("missing", "missing field `status`")]
#[case::jsonl_rejects_equal_status_keys("equal", "duplicate field `status`")]
#[case::jsonl_rejects_conflicting_status_keys("conflicting", "duplicate field `status`")]
#[case::jsonl_rejects_unknown_status("unknown", "unknown variant `future_status`")]
fn jsonl_rejects_invalid_status_fields(#[case] defect: &str, #[case] expected: &str) {
    // raw edits preserve duplicate-key evidence at the public file boundary.
    let fixture = tempfile::tempdir().unwrap();
    let document = valid_report();
    let path = write_jsonl(&fixture, &jsonl_events(&document));
    assert!(matches!(read_report(&path), Ok(InputReport::Usable(_))));
    let wire = std::fs::read_to_string(&path).unwrap();
    let changed = changed_status_wire(&document, wire, defect);
    std::fs::write(&path, changed).unwrap();

    let error = read_report(&path).unwrap_err();

    assert!(error.to_string().contains(expected), "{error}");
}

fn write_jsonl(fixture: &tempfile::TempDir, events: &[Value]) -> PathBuf {
    let path = fixture.path().join("events.report");
    let mut text = events
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    std::fs::write(&path, text).unwrap();
    path
}

#[tokio::test]
async fn jsonl_and_document_inputs_have_identical_progress() {
    let fixture = tempfile::tempdir().unwrap();
    let document = valid_report();
    let json = write_json(&fixture, "document.report", &document);
    let jsonl = write_jsonl(&fixture, &jsonl_events(&document));
    let InputReport::Usable(parsed_stream) = read_report(&jsonl).unwrap() else {
        panic!("completed JSONL must be usable");
    };
    let (expected_code, expected, _) = run_progress(&[json.clone(), json.clone()]).await;
    let (actual_code, actual, errors) = run_progress(&[json, jsonl]).await;
    assert_eq!(parsed_stream.mutants.len(), 1);
    assert_eq!(
        parsed_stream.mutants[0].candidate.id,
        valid_report()["mutants"][0]["candidate"]["id"]
            .as_str()
            .unwrap()
    );
    assert_eq!(actual_code, expected_code, "{errors:?}");
    let expected: Value = serde_json::from_slice(&expected).unwrap();
    let actual: Value = serde_json::from_slice(&actual).unwrap();
    assert_eq!(actual["latest"], expected["latest"]);
    assert_eq!(actual["comparisons"], expected["comparisons"]);
}

fn corrupt_lifecycle_events(defect: &str) -> Vec<Value> {
    let mut events = jsonl_events(&valid_report());
    match defect {
        "missing_end" => {
            events.pop();
        }
        "duplicate_run" => {
            events.insert(1, events[0].clone());
        }
        "after_end" => {
            events.push(events[1].clone());
        }
        "duplicate_baseline" => {
            events.insert(2, events[1].clone());
        }
        "late_baseline" => {
            events.swap(1, 2);
        }
        "missing_start" => {
            events.remove(2);
        }
        "active_at_end" => {
            events.remove(3);
        }
        "duplicate_mutant" => {
            events.insert(4, events[3].clone());
        }
        "wrong_run" => events[2]["run_id"] = json!("other"),
        "sequence" => events[2]["sequence"] = json!(1),
        "schema" => events[2]["schema_version"] = json!(2),
        "unknown_kind" => events[2]["kind"] = json!("future_event"),
        "counts" => events[4]["counts"]["killed"] = json!(2),
        "result" => events[3]["outcome"]["executed"]["termination"] = json!({"Exit": 0}),
        _ => unreachable!(),
    }
    // Keep sequence valid unless sequence order itself is under test.
    if defect != "sequence" {
        for (index, event) in events.iter_mut().enumerate() {
            event["sequence"] = json!(index + 1);
        }
    }
    events
}

#[rstest::rstest]
#[case::missing_end("missing_end")]
#[case::duplicate_run("duplicate_run")]
#[case::after_end("after_end")]
#[case::duplicate_baseline("duplicate_baseline")]
#[case::late_baseline("late_baseline")]
#[case::missing_start("missing_start")]
#[case::active_at_end("active_at_end")]
#[case::duplicate_mutant("duplicate_mutant")]
#[case::wrong_run("wrong_run")]
#[case::sequence("sequence")]
#[case::schema("schema")]
#[case::unknown_kind("unknown_kind")]
#[case::counts("counts")]
#[case::result("result")]
fn jsonl_rejects_corrupt_lifecycles_and_results(#[case] defect: &str) {
    let fixture = tempfile::tempdir().unwrap();
    let events = corrupt_lifecycle_events(defect);
    let path = write_jsonl(&fixture, &events);

    let result = read_report(&path);

    assert!(result.is_err(), "accepted {defect}");
}

#[test]
fn jsonl_rejects_a_truncated_event_after_a_valid_stream() {
    let fixture = tempfile::tempdir().unwrap();
    let path = write_jsonl(&fixture, &jsonl_events(&valid_report()));
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("{\"kind\":");
    std::fs::write(&path, text).unwrap();

    let result = read_report(&path);

    assert!(matches!(result, Err(ReportReadError::Parse { .. })));
}

fn opaque_config_document(reason: Option<UnusableReason>) -> Value {
    let mut document = valid_report();
    document["run"]["normalized_config"] = json!({"future_config": [1, 2]});
    match reason {
        Some(UnusableReason::MissingBaseline) => {
            document["baseline"] = Value::Null;
            document["mutants"] = json!([]);
            document["summary"]["counts"] =
                serde_json::to_value(hoimin_core::summarize(&[])).unwrap();
            document["mutants"] = json!([]);
            document["summary"]["counts"] =
                serde_json::to_value(hoimin_core::summarize(&[])).unwrap();
            document["summary"]["complete"] = json!(false);
            document["summary"]["exit_code"] = json!(4);
        }
        Some(UnusableReason::BaselineFailed) => {
            document["baseline"]["termination"] = json!({"Exit": 1});
        }
        Some(UnusableReason::Incomplete) => {
            document["summary"]["complete"] = json!(false);
            document["summary"]["exit_code"] = json!(4);
        }
        None => {}
    }
    document
}

fn report_unusable_reason(report: &InputReport) -> Option<UnusableReason> {
    match report {
        InputReport::Usable(_) => None,
        InputReport::Unusable { reason, .. } => Some(*reason),
    }
}

#[rstest::rstest]
#[case::none_1(None)]
#[case::some_unusablereason_missingbaseline_2(Some(UnusableReason::MissingBaseline))]
#[case::some_unusablereason_baselinefailed_3(Some(UnusableReason::BaselineFailed))]
#[case::some_unusablereason_incomplete_4(Some(UnusableReason::Incomplete))]
fn jsonl_preserves_opaque_config_layout_and_unusable_reasons(
    #[case] reason: Option<UnusableReason>,
) {
    let fixture = tempfile::tempdir().unwrap();

    let document = opaque_config_document(reason);
    let events = jsonl_events(&document);
    let path = write_jsonl(&fixture, &events);
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        format!("\r\n {}", text.trim_end().replace('\n', "\r\n\r\n")),
    )
    .unwrap();
    let actual = read_report(&path).unwrap();
    let json_path = write_json(&fixture, "same.json", &document);
    let expected = read_report(&json_path).unwrap();

    assert_eq!(report_unusable_reason(&actual), reason);
    assert_eq!(report_unusable_reason(&expected), reason);
}

#[rstest::rstest]
#[case::usable_null_document(None, "null", write_json)]
#[case::usable_null_events(None, "null", write_explicit_jsonl)]
#[case::usable_future_document(None, r#"{"future_config":[1,2]}"#, write_json)]
#[case::usable_future_events(None, r#"{"future_config":[1,2]}"#, write_explicit_jsonl)]
#[case::usable_duplicate_keys_document(
    None,
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_json
)]
#[case::usable_duplicate_keys_events(
    None,
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_explicit_jsonl
)]
#[case::missing_baseline_null_document(Some(UnusableReason::MissingBaseline), "null", write_json)]
#[case::missing_baseline_null_events(
    Some(UnusableReason::MissingBaseline),
    "null",
    write_explicit_jsonl
)]
#[case::missing_baseline_future_document(
    Some(UnusableReason::MissingBaseline),
    r#"{"future_config":[1,2]}"#,
    write_json
)]
#[case::missing_baseline_future_events(
    Some(UnusableReason::MissingBaseline),
    r#"{"future_config":[1,2]}"#,
    write_explicit_jsonl
)]
#[case::missing_baseline_duplicate_keys_document(
    Some(UnusableReason::MissingBaseline),
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_json
)]
#[case::missing_baseline_duplicate_keys_events(
    Some(UnusableReason::MissingBaseline),
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_explicit_jsonl
)]
#[case::failed_baseline_null_document(Some(UnusableReason::BaselineFailed), "null", write_json)]
#[case::failed_baseline_null_events(
    Some(UnusableReason::BaselineFailed),
    "null",
    write_explicit_jsonl
)]
#[case::failed_baseline_future_document(
    Some(UnusableReason::BaselineFailed),
    r#"{"future_config":[1,2]}"#,
    write_json
)]
#[case::failed_baseline_future_events(
    Some(UnusableReason::BaselineFailed),
    r#"{"future_config":[1,2]}"#,
    write_explicit_jsonl
)]
#[case::failed_baseline_duplicate_keys_document(
    Some(UnusableReason::BaselineFailed),
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_json
)]
#[case::failed_baseline_duplicate_keys_events(
    Some(UnusableReason::BaselineFailed),
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_explicit_jsonl
)]
#[case::incomplete_null_document(Some(UnusableReason::Incomplete), "null", write_json)]
#[case::incomplete_null_events(Some(UnusableReason::Incomplete), "null", write_explicit_jsonl)]
#[case::incomplete_future_document(
    Some(UnusableReason::Incomplete),
    r#"{"future_config":[1,2]}"#,
    write_json
)]
#[case::incomplete_future_events(
    Some(UnusableReason::Incomplete),
    r#"{"future_config":[1,2]}"#,
    write_explicit_jsonl
)]
#[case::incomplete_duplicate_keys_document(
    Some(UnusableReason::Incomplete),
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_json
)]
#[case::incomplete_duplicate_keys_events(
    Some(UnusableReason::Incomplete),
    r#"{"root":"/not/a/real/hoimin/project","import_roots":null,"import_roots":[]}"#,
    write_explicit_jsonl
)]
fn raw_opaque_config_preserves_jsonl_classification(
    #[case] reason: Option<UnusableReason>,
    #[case] config: &str,
    #[case] writer: ReportWriter,
) {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = opaque_config_document(reason);
    document["run"]["normalized_config"] = Value::Null;
    let report = writer(&fixture, "opaque.json", &document);
    let original = std::fs::read_to_string(&report).unwrap();
    let field = r#""normalized_config":null"#;
    assert_eq!(original.matches(field).count(), 1);
    let raw = original.replacen(field, &format!(r#""normalized_config":{config}"#), 1);
    std::fs::write(&report, raw).unwrap();

    let result = read_report(&report).unwrap();

    assert_eq!(
        report_unusable_reason(&result),
        reason,
        "{config} in {}",
        report.display()
    );
}

#[tokio::test]
async fn jsonl_real_run_output_supports_history_comparison() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/calc.py"),
        "def add(a, b):\n    return a + b\n",
    )
    .unwrap();
    std::fs::write(project.path().join("src/__init__.py"), "").unwrap();
    let mut reports = Vec::new();
    for phase in ["before", "after"] {
        let mut command = real_binary_run_command(
            project.path(),
            "from src.calc import add; assert add(2, 3) == 5",
        );
        let output = bounded_output(&mut command, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{phase}: {:?}",
            output.stderr
        );
        let path = project.path().join(format!("{phase}.report"));
        std::fs::write(&path, output.stdout).unwrap();
        assert!(matches!(
            read_report(&path).unwrap(),
            InputReport::Usable(_)
        ));
        reports.push(path);
    }
    let (code, output, errors) = run_progress(&reports).await;
    assert_eq!(code, 0, "{errors:?}");
    let output: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(output["latest"]["state"], "stalled");
    assert_eq!(output["comparisons"][0]["common"], 1);
    assert_eq!(output["comparisons"][0]["previous_score"], 1.0);
    assert_eq!(output["comparisons"][0]["current_score"], 1.0);
}

#[tokio::test]
async fn jsonl_invalid_input_has_no_partial_progress_output() {
    let fixture = tempfile::tempdir().unwrap();
    let json = write_json(&fixture, "valid.json", &valid_report());
    let mut events = jsonl_events(&valid_report());
    events.pop();
    let jsonl = write_jsonl(&fixture, &events);
    let (code, output, error) = run_progress(&[json, jsonl.clone()]).await;
    assert_eq!(code, 2);
    assert_eq!(output, Vec::<u8>::new());
    let error = String::from_utf8(error).unwrap();
    assert!(error.contains(&jsonl.display().to_string()), "{error}");
    assert!(error.contains("run_finished"), "{error}");
}

#[test]
fn jsonl_rejects_a_legacy_stream_header() {
    let fixture = tempfile::tempdir().unwrap();
    let mut events = jsonl_events(&valid_report());
    events[0]["schema_version"] = json!(2);
    let path = write_jsonl(&fixture, &events);

    let result = read_report(&path);

    assert!(matches!(
        result,
        Err(ReportReadError::UnsupportedSchema { found: 2, .. })
    ));
}

#[rstest::rstest]
#[case::valid_diagnostic("schema_version", json!(REPORT_SCHEMA_VERSION), true)]
#[case::old_schema("schema_version", json!(2), false)]
#[case::wrong_run("run_id", json!("another-run"), false)]
#[case::repeated_sequence("sequence", json!(4), false)]
fn jsonl_validates_discarded_diagnostics(
    #[case] field: &str,
    #[case] replacement: Value,
    #[case] accepted: bool,
) {
    let fixture = tempfile::tempdir().unwrap();
    let mut events = jsonl_events(&valid_report());
    let mut diagnostic = json!({"kind": "diagnostic", "schema_version": REPORT_SCHEMA_VERSION, "sequence": 5, "run_id": "run-1", "level": "info", "code": "test", "message": "test"});
    diagnostic[field] = replacement;
    events[4]["sequence"] = json!(6);
    events.insert(4, diagnostic);
    let path = write_jsonl(&fixture, &events);

    let result = read_report(&path);

    assert_eq!(result.is_ok(), accepted);
}

#[test]
fn jsonl_rejects_non_json_control_whitespace() {
    let fixture = tempfile::tempdir().unwrap();
    let path = write_jsonl(&fixture, &jsonl_events(&valid_report()));
    let text = std::fs::read(&path).unwrap();
    std::fs::write(&path, [b"\x0b\n".as_slice(), &text].concat()).unwrap();

    let result = read_report(&path);

    assert!(
        result.is_err(),
        "non-JSON control whitespace must not be skipped"
    );
}

#[tokio::test]
async fn jsonl_real_failed_baseline_is_unusable() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    std::fs::write(project.path().join("src/calc.py"), "value = 1 + 2\n").unwrap();
    {
        let mut command = real_binary_run_command(project.path(), "raise SystemExit(7)");
        let output = bounded_output(&mut command, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(output.status.code(), Some(3), "jsonl: {:?}", output.stderr);
        let path = project.path().join("jsonl.report");
        std::fs::write(&path, output.stdout).unwrap();
        assert!(matches!(
            read_report(&path).unwrap(),
            InputReport::Unusable {
                reason: UnusableReason::BaselineFailed,
                ..
            }
        ));
    }
}

#[test]
fn jsonl_accepts_not_run_with_start_and_preceding_baseline() {
    let fixture = tempfile::tempdir().unwrap();
    let path = write_jsonl(&fixture, &not_run_events());

    let result = read_report(&path).unwrap();

    assert!(matches!(
        result,
        InputReport::Unusable {
            reason: UnusableReason::Incomplete,
            ..
        }
    ));
}

#[rstest::rstest]
#[case::missing_start(false)]
#[case::before_baseline(true)]
fn jsonl_not_run_requires_start_and_preceding_baseline(#[case] reorder_baseline: bool) {
    let fixture = tempfile::tempdir().unwrap();
    let mut events = not_run_events();
    events.remove(2);
    reorder_not_run_baseline(&mut events, reorder_baseline);
    let path = write_jsonl(&fixture, &events);

    let result = read_report(&path);

    assert!(
        result.is_err(),
        "not_run does not exempt lifecycle ordering"
    );
}

fn not_run_events() -> Vec<Value> {
    let mut document = valid_report();
    document["mutants"][0]["outcome"] = json!("not_run");
    document["mutants"][0]["status"] = json!("not_run");
    document["summary"]["counts"] =
        serde_json::to_value(hoimin_core::summarize(&[MutationStatus::NotRun])).unwrap();
    document["summary"]["complete"] = json!(false);
    document["summary"]["exit_code"] = json!(4);
    jsonl_events(&document)
}

fn reorder_not_run_baseline(events: &mut [Value], reordered: bool) {
    if reordered {
        events.swap(1, 2);
        for (index, event) in events.iter_mut().enumerate() {
            event["sequence"] = json!(index + 1);
        }
    }
}

async fn progress_details(reports: &[PathBuf], limit: usize) -> Value {
    let mut args = vec![
        "hoimin".to_owned(),
        "progress".to_owned(),
        "--details".to_owned(),
        "--details-limit".to_owned(),
        limit.to_string(),
    ];
    args.extend(reports.iter().map(|path| path.to_str().unwrap().to_owned()));
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    serde_json::from_slice(&stdout.snapshot()).unwrap()
}

fn detail_id(key: &str) -> String {
    detail_report(&[(key, MutationStatus::Killed)])["mutants"][0]["candidate"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn detail_report(statuses: &[(&str, MutationStatus)]) -> Value {
    let mut report = valid_report();
    let template = report["mutants"][0].clone();
    report["mutants"] = Value::Array(
        statuses
            .iter()
            .enumerate()
            .map(|(index, (id, status))| {
                let mut event = template.clone();
                event["sequence"] = json!(index + 3);
                event["candidate"]["sequence"] = json!(index + 1);
                event["candidate"]["file_hash"] = json!(id);
                candidate_identity::assign_id(&mut event["candidate"]);
                event["candidate"]["original"] = json!(id);
                event["outcome"] =
                    serde_json::to_value(mutation_outcome::executed(*status)).unwrap();
                event["status"] = serde_json::to_value(status).unwrap();
                event
            })
            .collect(),
    );
    let statuses: Vec<_> = statuses.iter().map(|(_, status)| *status).collect();
    let counts = hoimin_core::summarize(&statuses);
    report["summary"]["sequence"] = json!(statuses.len() + 3);
    report["summary"]["counts"] = serde_json::to_value(counts).unwrap();
    report["summary"]["exit_code"] = json!(0);
    report
}

#[rstest::rstest]
#[case::regressing("regressing", "regressing")]
#[case::unavailable("unavailable", "indeterminate")]
#[case::different("different", "indeterminate")]
#[tokio::test]
async fn progress_json_contract_without_details(
    #[case] scenario: &str,
    #[case] expected_state: &str,
) {
    let fixture = tempfile::tempdir().unwrap();
    let reports = independent_contract_history(&fixture, scenario);
    let schema = read_schema(
        &repo_root().join("docs/user/reference/json-schema/progress-result.schema.json"),
    );

    let (code, output, stderr) = progress_output(&reports, false).await;

    let (plain_code, plain, plain_stderr) = progress_output(&reports, false).await;

    let actual: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(code, 0, "{stderr:?}");
    assert_eq!(actual["schema_version"], 3);
    assert_schema_valid(&schema, &actual);
    assert_eq!(actual["latest"]["state"], expected_state);
    assert_eq!(actual.get("details"), Some(&Value::Null));
    let plain: Value = serde_json::from_slice(&plain).unwrap();
    assert_eq!(plain_code, code);
    assert_eq!(plain_stderr, stderr);
    for field in [
        "inputs",
        "comparisons",
        "latest",
        "patience",
        "consecutive_stalls",
    ] {
        assert_eq!(actual[field], plain[field], "{field}");
    }
}

#[rstest::rstest]
#[case::regressing_limit_0("regressing", 0, "regressing", true)]
#[case::regressing_limit_1("regressing", 1, "regressing", true)]
#[case::regressing_limit_100("regressing", 100, "regressing", true)]
#[case::unavailable_limit_0("unavailable", 0, "indeterminate", false)]
#[case::unavailable_limit_100("unavailable", 100, "indeterminate", false)]
#[case::different_limit_0("different", 0, "indeterminate", true)]
#[case::different_limit_100("different", 100, "indeterminate", true)]
#[case::stalled_limit_0("stalled", 0, "stalled", true)]
#[case::saturated_limit_0("saturated", 0, "saturated", true)]
#[case::saturated_limit_100("saturated", 100, "saturated", true)]
#[tokio::test]
async fn progress_json_details_preserve_aggregate_decisions(
    #[case] scenario: &str,
    #[case] limit: usize,
    #[case] expected_state: &str,
    #[case] available: bool,
) {
    let fixture = tempfile::tempdir().unwrap();
    let reports = independent_contract_history(&fixture, scenario);
    let schema = read_schema(
        &repo_root().join("docs/user/reference/json-schema/progress-result.schema.json"),
    );

    let (code, actual, stderr) = progress_with_limit(&reports, limit).await;
    let (plain_code, plain, plain_stderr) = progress_output(&reports, false).await;

    let plain: Value = serde_json::from_slice(&plain).unwrap();
    assert_eq!(actual["schema_version"], 3);
    assert_schema_valid(&schema, &actual);
    assert_eq!(code, 0);
    assert_eq!(plain_code, code);
    assert_eq!(plain_stderr, stderr);
    assert_eq!(actual["latest"]["state"], expected_state);
    for field in [
        "inputs",
        "comparisons",
        "latest",
        "patience",
        "consecutive_stalls",
    ] {
        assert_eq!(actual[field], plain[field], "{field}");
    }
    assert!(actual["details"].is_object());
    assert_eq!(actual["details"]["limit"], limit);
    assert_eq!(actual["details"]["available"], available);
}

#[rstest::rstest]
#[case::zero(0)]
#[case::full(100)]
#[tokio::test]
async fn unavailable_details_have_no_eligibility(#[case] limit: usize) {
    let fixture = tempfile::tempdir().unwrap();
    let reports = independent_contract_history(&fixture, "unavailable");

    let (_, actual, _) = progress_with_limit(&reports, limit).await;

    assert!(actual["details"]["eligibility"].is_null());
}

#[rstest::rstest]
#[case::regressing("regressing")]
#[case::unavailable("unavailable")]
#[case::different("different")]
#[case::stalled("stalled")]
#[case::saturated("saturated")]
#[tokio::test]
async fn zero_details_limit_omits_transitions(#[case] scenario: &str) {
    let fixture = tempfile::tempdir().unwrap();
    let reports = independent_contract_history(&fixture, scenario);

    let (_, actual, _) = progress_with_limit(&reports, 0).await;

    assert_eq!(actual["details"]["transitions"], json!([]));
}

#[rstest::rstest]
#[case::zero(0, 1, 0)]
#[case::one(1, 0, 1)]
#[case::full(100, 0, 1)]
#[tokio::test]
async fn regression_details_counts_match_the_limit(
    #[case] limit: usize,
    #[case] omitted: usize,
    #[case] displayed: usize,
) {
    let fixture = tempfile::tempdir().unwrap();
    let reports = independent_contract_history(&fixture, "regressing");

    let (_, actual, _) = progress_with_limit(&reports, limit).await;

    assert_eq!(actual["comparisons"][0]["regressions"], 1);
    assert_eq!(actual["details"]["omitted"], omitted);
    assert_eq!(
        actual["details"]["transitions"].as_array().unwrap().len(),
        displayed
    );
}

#[rstest::rstest]
#[case::progress_duplicate_identity_default(None)]
#[case::progress_duplicate_identity_zero(Some(0))]
#[case::progress_duplicate_identity_full(Some(100))]
#[tokio::test]
async fn progress_rejects_duplicate_identity_for_every_details_limit(#[case] limit: Option<usize>) {
    // the report reader rejects one ID assigned to multiple sequences.
    let fixture = tempfile::tempdir().unwrap();
    let before = write_json(
        &fixture,
        "before.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let after = write_json(
        &fixture,
        "after.json",
        &detail_report(&[
            ("a", MutationStatus::Survived),
            ("a", MutationStatus::Survived),
        ]),
    );
    let args = progress_limit_args(&[before, after], limit);
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();

    let code = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;

    // presentation cannot bypass the current input contract.
    assert_eq!(code, 2);
    assert_eq!(stdout.snapshot(), Vec::<u8>::new());
    assert!(
        String::from_utf8_lossy(&stderr.snapshot())
            .contains("identity maps to sequence 1, received 2")
    );
}

#[tokio::test]
async fn progress_json_matches_current_golden() {
    let fixture = tempfile::tempdir().unwrap();
    let before = write_json(
        &fixture,
        "input-0.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let after = write_json(
        &fixture,
        "input-1.json",
        &detail_report(&[("a", MutationStatus::Survived)]),
    );
    let expected = read_schema(
        &repo_root().join("crates/hoimin-cli/tests/golden/progress/schema-v3-current.json"),
    );

    let (code, stdout, _) = run_progress(&[before, after]).await;
    let mut actual: Value = serde_json::from_slice(&stdout).unwrap();
    for input in actual["inputs"].as_array_mut().unwrap() {
        input["source"] = json!(
            Path::new(input["source"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        );
    }

    assert_eq!(code, 0);
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn details_identify_both_directions() {
    let fixture = tempfile::tempdir().unwrap();
    let before = write_json(
        &fixture,
        "before.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let after = write_json(
        &fixture,
        "after.json",
        &detail_report(&[("a", MutationStatus::Survived)]),
    );
    let reports = [before, after];
    let (code, original, stderr) = run_progress(&reports).await;
    let reverse = progress_details(&[reports[1].clone(), reports[0].clone()], 100).await;
    let original: Value = serde_json::from_slice(&original).unwrap();
    let detailed = progress_details(&reports, 100).await;

    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    for field in [
        "inputs",
        "comparisons",
        "latest",
        "patience",
        "consecutive_stalls",
    ] {
        assert_eq!(detailed[field], original[field], "{field}");
    }
    let detail = &detailed["details"];
    assert_eq!(detail["previous_input"], 0);
    assert_eq!(detail["current_input"], 1);
    assert_eq!(detail["available"], true);
    assert_eq!(detail["eligibility"], "matching");
    assert_eq!(detail["omitted"], 0);
    assert_eq!(detail["unidentified"], 0);
    assert_eq!(
        detail["transitions"][0],
        json!({"id":detail_id("a"), "path":"src/example.py", "line":1,
        "column":0, "previous_path":"src/example.py", "previous_line":1, "previous_column":0,
        "operator":"integer_literal", "classification":"regression", "previous_status":"killed", "current_status":"survived"})
    );

    assert_eq!(
        reverse["details"]["transitions"][0]["classification"],
        "improvement"
    );
    assert_eq!(
        reverse["details"]["transitions"][0]["previous_status"],
        "survived"
    );
    assert_eq!(
        reverse["details"]["transitions"][0]["current_status"],
        "killed"
    );
}

#[rstest::rstest]
#[case::input_0_1(0)]
#[case::input_1_2(1)]
#[case::input_2_3(2)]
#[case::input_100_4(100)]
#[tokio::test]
async fn details_cap_and_order_do_not_change_aggregate_counts(#[case] limit: usize) {
    let fixture = tempfile::tempdir().unwrap();
    let before = write_json(
        &fixture,
        "before.json",
        &detail_report(&[
            ("2", MutationStatus::Killed),
            ("10", MutationStatus::Survived),
            ("a", MutationStatus::Killed),
        ]),
    );
    let after = write_json(
        &fixture,
        "after.json",
        &detail_report(&[
            ("a", MutationStatus::Survived),
            ("10", MutationStatus::Killed),
            ("2", MutationStatus::Survived),
        ]),
    );

    let detailed = progress_details(&[before.clone(), after.clone()], limit).await;

    let entries = detailed["details"]["transitions"].as_array().unwrap();
    let ids: Vec<_> = entries
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect();
    let mut expected = [detail_id("10"), detail_id("2"), detail_id("a")];
    expected.sort();
    assert_eq!(detailed["comparisons"][0]["improvements"], 1);
    assert_eq!(detailed["comparisons"][0]["regressions"], 2);
    assert_eq!(ids, expected[..limit.min(3)]);
    assert_eq!(detailed["details"]["omitted"], 3 - limit.min(3));
}

#[tokio::test]
async fn details_do_not_identify_content_only_matches() {
    let fixture = tempfile::tempdir().unwrap();
    let before_doc = detail_report(&[("a", MutationStatus::Killed)]);
    let mut after_doc = detail_report(&[("a", MutationStatus::Survived)]);
    after_doc["mutants"][0]["candidate"]["file_hash"] = json!("different");
    candidate_identity::assign_id(&mut after_doc["mutants"][0]["candidate"]);
    let before = write_json(&fixture, "before.json", &before_doc);
    let after = write_json(&fixture, "after.json", &after_doc);
    let detailed = progress_details(&[before, after], 100).await;

    assert_eq!(detailed["latest"]["state"], "indeterminate");
    assert_eq!(detailed["comparisons"][0]["regressions"], 1);
    assert_eq!(detailed["details"]["eligibility"], "different");
    assert_eq!(detailed["details"]["unidentified"], 1);
    assert_eq!(detailed["details"]["transitions"], json!([]));
}

#[tokio::test]
async fn details_mark_a_trailing_unusable_pair_unavailable() {
    let fixture = tempfile::tempdir().unwrap();
    let before_doc = detail_report(&[("a", MutationStatus::Killed)]);
    let mut after_doc = detail_report(&[("a", MutationStatus::Survived)]);
    after_doc["mutants"][0]["candidate"]["file_hash"] = json!("different");
    candidate_identity::assign_id(&mut after_doc["mutants"][0]["candidate"]);
    let before = write_json(&fixture, "before.json", &before_doc);
    let after = write_json(&fixture, "after.json", &after_doc);
    let mut unusable = before_doc.clone();
    unusable["summary"]["complete"] = json!(false);
    unusable["summary"]["exit_code"] = json!(4);
    let gap = write_json(&fixture, "gap.json", &unusable);

    let trailing = progress_details(&[before, after, gap], 100).await;

    assert_eq!(trailing["details"]["previous_input"], 1);
    assert_eq!(trailing["details"]["current_input"], 2);
    assert_eq!(trailing["details"]["available"], false);
    assert!(trailing["details"]["eligibility"].is_null());
    assert_eq!(trailing["details"]["transitions"], json!([]));
}

#[tokio::test]
async fn details_select_the_latest_pair_after_an_unusable_gap() {
    let fixture = tempfile::tempdir().unwrap();
    let before_doc = detail_report(&[("a", MutationStatus::Killed)]);
    let mut after_doc = detail_report(&[("a", MutationStatus::Survived)]);
    after_doc["mutants"][0]["candidate"]["file_hash"] = json!("different");
    candidate_identity::assign_id(&mut after_doc["mutants"][0]["candidate"]);
    let before = write_json(&fixture, "before.json", &before_doc);
    let after = write_json(&fixture, "after.json", &after_doc);
    let mut unusable = before_doc.clone();
    unusable["summary"]["complete"] = json!(false);
    unusable["summary"]["exit_code"] = json!(4);
    let gap = write_json(&fixture, "gap.json", &unusable);

    let middle = progress_details(&[before.clone(), gap, before, after], 100).await;

    assert_eq!(middle["details"]["previous_input"], 2);
    assert_eq!(middle["details"]["current_input"], 3);
}

#[tokio::test]
async fn details_schema_jsonl_and_quoted_locations_are_consistent() {
    let fixture = tempfile::tempdir().unwrap();
    let (before_doc, after_doc) = quoted_location_documents();
    let before = write_json(&fixture, "before.json", &before_doc);
    let after = write_json(&fixture, "after.json", &after_doc);
    let schema = read_schema(
        &repo_root().join("docs/user/reference/json-schema/progress-result.schema.json"),
    );
    let mut paths = Vec::new();
    for (name, doc) in [("before.jsonl", before_doc), ("after.jsonl", after_doc)] {
        let path = fixture.path().join(name);
        let events = jsonl_events(&doc);
        let text = events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap() + "\n")
            .collect::<String>();
        std::fs::write(&path, text).unwrap();
        paths.push(path);
    }

    let actual = progress_details(&[before, after], 100).await;
    let jsonl = progress_details(&paths, 100).await;

    let transition = &actual["details"]["transitions"][0];
    assert_schema_valid(&schema, &actual);
    assert_eq!(transition["path"], "src/a\n\"b.py");
    assert_eq!(transition["previous_path"], "src/a\n\"b.py");
    assert_eq!(transition["previous_line"], 9);
    assert_eq!(transition["previous_column"], 0);
    assert_eq!(actual["details"], jsonl["details"]);
}

#[rstest::rstest]
#[case::unknown_details_field(|document: &mut Value| { document["details"]["unknown"] = json!(true); })]
#[case::unknown_transition_field(|document: &mut Value| { document["details"]["transitions"][0]["unknown"] = json!(true); })]
#[case::inconclusive_transition_status(|document: &mut Value| { document["details"]["transitions"][0]["current_status"] = json!("timeout"); })]
#[tokio::test]
async fn details_schema_rejects_invalid_fields(#[case] damage: fn(&mut Value)) {
    let fixture = tempfile::tempdir().unwrap();
    let (before_doc, after_doc) = quoted_location_documents();
    let before = write_json(&fixture, "before.json", &before_doc);
    let after = write_json(&fixture, "after.json", &after_doc);
    let schema = read_schema(
        &repo_root().join("docs/user/reference/json-schema/progress-result.schema.json"),
    );
    let mut invalid = progress_details(&[before, after], 100).await;
    damage(&mut invalid);

    assert_schema_invalid(&schema, &invalid);
}

fn quoted_location_documents() -> (Value, Value) {
    let mut before = detail_report(&[("a", MutationStatus::Killed)]);
    let mut after = detail_report(&[("a", MutationStatus::Survived)]);
    before["mutants"][0]["candidate"]["line"] = json!(9);
    for document in [&mut before, &mut after] {
        document["mutants"][0]["candidate"]["path"] = json!("src/a\n\"b.py");
        candidate_identity::assign_id(&mut document["mutants"][0]["candidate"]);
    }
    (before, after)
}

#[rstest::rstest]
#[case::details_limit_1(vec!["--details-limit", "1"])]
#[case::details_2(vec!["--details", "--details-limit", "-1"])]
#[case::details_3(vec!["--details", "--details-limit", "no"])]
#[tokio::test]
async fn details_limit_requires_opt_in_and_nonnegative_integer(#[case] options: Vec<&str>) {
    let mut args = vec!["hoimin", "progress"];
    args.extend(options);
    args.extend(["before.json", "after.json"]);
    let out = CollectingWriter::default();
    let err = CollectingWriter::default();

    let code = hoimin_cli::run_with_writers(args, out.clone(), err.clone()).await;

    assert_eq!(code, 2);
    assert_eq!(out.snapshot(), Vec::<u8>::new());
    assert!(!err.is_empty());
}

#[tokio::test]
async fn details_default_cap_is_explicit() {
    let fixture = tempfile::tempdir().unwrap();
    let ids: Vec<_> = (0..101).map(|i| format!("id-{i:03}")).collect();
    let before_entries: Vec<_> = ids
        .iter()
        .map(|id| (id.as_str(), MutationStatus::Killed))
        .collect();
    let after_entries: Vec<_> = ids
        .iter()
        .map(|id| (id.as_str(), MutationStatus::Survived))
        .collect();
    let before = write_json(&fixture, "before.json", &detail_report(&before_entries));
    let after = write_json(&fixture, "after.json", &detail_report(&after_entries));
    let out = CollectingWriter::default();
    let err = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(
        [
            "hoimin",
            "progress",
            "--details",
            before.to_str().unwrap(),
            after.to_str().unwrap(),
        ],
        out.clone(),
        err.clone(),
    )
    .await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err.snapshot()));
    let value: Value = serde_json::from_slice(&out.snapshot()).unwrap();
    assert_eq!(value["details"]["limit"], 100);
    assert_eq!(value["details"]["omitted"], 1);
    assert_eq!(
        value["details"]["transitions"].as_array().unwrap().len(),
        100
    );
    let mut expected_ids: Vec<_> = ids.iter().map(|id| detail_id(id)).collect();
    expected_ids.sort();
    assert_eq!(value["details"]["transitions"][99]["id"], expected_ids[99]);
}

#[tokio::test]
async fn details_partial_sets_are_explicit() {
    let fixture = tempfile::tempdir().unwrap();
    let ids: Vec<_> = (0..101).map(|i| format!("id-{i:03}")).collect();
    let before_entries: Vec<_> = ids
        .iter()
        .map(|id| (id.as_str(), MutationStatus::Killed))
        .collect();
    let after_entries: Vec<_> = ids
        .iter()
        .map(|id| (id.as_str(), MutationStatus::Survived))
        .collect();
    let before = write_json(&fixture, "before.json", &detail_report(&before_entries));
    let after = write_json(
        &fixture,
        "partial.json",
        &detail_report(&after_entries[..1]),
    );
    let value = progress_details(&[before, after], 100).await;
    assert_eq!(value["details"]["eligibility"], "different");
    assert_eq!(
        value["details"]["transitions"][0]["id"],
        detail_id("id-000")
    );
    assert_eq!(value["latest"]["state"], "indeterminate");
}

fn progress_args(reports: &[PathBuf], details: bool) -> Vec<String> {
    let mut args = vec!["hoimin".to_owned(), "progress".to_owned()];
    if details {
        args.push("--details".to_owned());
    }
    args.extend(reports.iter().map(|path| path.to_str().unwrap().to_owned()));
    args
}

async fn progress_output(reports: &[PathBuf], details: bool) -> (i32, Vec<u8>, Vec<u8>) {
    let out = CollectingWriter::default();
    let err = CollectingWriter::default();
    let status =
        hoimin_cli::run_with_writers(progress_args(reports, details), out.clone(), err.clone())
            .await;
    (status, out.snapshot(), err.snapshot())
}

#[rstest::rstest]
#[case::regressing_plain("regressing", false, "regressing")]
#[case::regressing_details("regressing", true, "regressing")]
#[case::improving_plain("improving", false, "improving")]
#[case::improving_details("improving", true, "improving")]
#[case::stalled_plain("stalled", false, "stalled")]
#[case::stalled_details("stalled", true, "stalled")]
#[case::saturated_plain("saturated", false, "saturated")]
#[case::saturated_details("saturated", true, "saturated")]
#[case::different_plain("different", false, "indeterminate")]
#[case::different_details("different", true, "indeterminate")]
#[case::recovered_improvement_plain("recovered_improvement", false, "improving")]
#[case::recovered_improvement_details("recovered_improvement", true, "improving")]
#[case::trailing_gap_plain("trailing_gap", false, "indeterminate")]
#[case::trailing_gap_details("trailing_gap", true, "indeterminate")]
#[case::leading_gap_plain("leading_gap", false, "regressing")]
#[case::leading_gap_details("leading_gap", true, "regressing")]
#[tokio::test]
async fn completed_progress_succeeds_for_every_comparison_state(
    #[case] name: &str,
    #[case] details: bool,
    #[case] state: &str,
) {
    let fixture = tempfile::tempdir().unwrap();
    let reports = completion_history(&fixture, name);

    let (status, out, err) = progress_output(&reports, details).await;

    assert_eq!(
        status,
        0,
        "{state} details={details}: {}",
        String::from_utf8_lossy(&err)
    );
    assert_json_regression_state(&out, state);
}

fn completion_history(fixture: &tempfile::TempDir, name: &str) -> Vec<PathBuf> {
    let killed = write_json(
        fixture,
        "killed.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let survived = write_json(
        fixture,
        "survived.json",
        &detail_report(&[("a", MutationStatus::Survived)]),
    );
    let mut gap_doc = detail_report(&[("a", MutationStatus::Killed)]);
    gap_doc["summary"]["complete"] = json!(false);
    gap_doc["summary"]["exit_code"] = json!(4);
    let gap = write_json(fixture, "gap.json", &gap_doc);
    let mut different_doc = detail_report(&[("a", MutationStatus::Survived)]);
    different_doc["mutants"][0]["candidate"]["file_hash"] = json!("different");
    candidate_identity::assign_id(&mut different_doc["mutants"][0]["candidate"]);
    let different = write_json(fixture, "different.json", &different_doc);
    match name {
        "regressing" => vec![killed, survived],
        "improving" => vec![survived, killed],
        "stalled" => vec![killed.clone(), killed],
        "saturated" => vec![killed; 4],
        "different" => vec![killed, different],
        "recovered_improvement" => vec![killed.clone(), survived, killed],
        "trailing_gap" => vec![killed, survived, gap],
        "leading_gap" => vec![gap, killed, survived],
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn progress_rejects_trailing_invalid_input_before_output() {
    let fixture = tempfile::tempdir().unwrap();
    let killed = write_json(
        &fixture,
        "killed.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let survived = write_json(
        &fixture,
        "survived.json",
        &detail_report(&[("a", MutationStatus::Survived)]),
    );
    let invalid = fixture.path().join("invalid.json");
    std::fs::write(&invalid, "{").unwrap();
    let (status, out, err) = progress_output(
        &[
            killed.clone(),
            killed.clone(),
            survived.clone(),
            invalid.clone(),
        ],
        true,
    )
    .await;
    assert_eq!(status, 2);
    assert_eq!(out, Vec::<u8>::new());
    let err = String::from_utf8(err).unwrap();
    assert!(err.contains("could not parse report"), "{err}");
    assert!(
        !err.contains("same path"),
        "warnings must wait for validation: {err}"
    );
}

struct ProgressFailingWriter;
impl std::io::Write for ProgressFailingWriter {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("progress fixture write failure"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("progress fixture flush failure"))
    }
}

#[tokio::test]
async fn progress_reports_stdout_failure() {
    let fixture = tempfile::tempdir().unwrap();
    let killed = write_json(
        &fixture,
        "killed.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let survived = write_json(
        &fixture,
        "survived.json",
        &detail_report(&[("a", MutationStatus::Survived)]),
    );
    let error = CollectingWriter::default();
    let status = hoimin_cli::run_with_writers(
        progress_args(&[killed.clone(), survived.clone()], true),
        ProgressFailingWriter,
        error.clone(),
    )
    .await;
    assert_eq!(status, 2);
    assert!(
        String::from_utf8(error.snapshot())
            .unwrap()
            .contains("progress fixture write failure")
    );
}

#[tokio::test]
async fn progress_reports_stderr_failure() {
    let fixture = tempfile::tempdir().unwrap();
    let killed = write_json(
        &fixture,
        "killed.json",
        &detail_report(&[("a", MutationStatus::Killed)]),
    );
    let survived = write_json(
        &fixture,
        "survived.json",
        &detail_report(&[("a", MutationStatus::Survived)]),
    );
    let out = CollectingWriter::default();
    let status = hoimin_cli::run_with_writers(
        progress_args(&[killed.clone(), killed.clone(), survived.clone()], true),
        out.clone(),
        ProgressFailingWriter,
    )
    .await;
    assert_eq!(status, 2);
    assert_eq!(out.snapshot(), Vec::<u8>::new());
}

#[test]
fn progress_rejects_aggregate_report_documents() {
    let source = repo_root().join(format!(
        "crates/hoimin-cli/tests/golden/reports/schema-v{REPORT_SCHEMA_VERSION}-current.json"
    ));

    let result = read_report(&source);

    assert!(result.is_err());
}

#[test]
fn progress_accepts_current_golden_event_stream() {
    let source = repo_root().join(format!(
        "crates/hoimin-cli/tests/golden/events/schema-v{REPORT_SCHEMA_VERSION}-current.jsonl"
    ));

    let result = read_report(&source);

    assert!(matches!(result, Ok(InputReport::Usable(_))));
}

#[test]
fn progress_rejects_mixed_baseline_schema_through_document_writer() {
    let fixture = tempfile::tempdir().unwrap();
    let source = repo_root().join(format!(
        "crates/hoimin-cli/tests/golden/reports/schema-v{REPORT_SCHEMA_VERSION}-current.json"
    ));
    let mut value: Value = serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
    value["baseline"]["schema_version"] = json!(3);
    let path = write_json(&fixture, "mixed.json", &value);

    let result = read_report(&path);

    assert!(result.is_err());
}

#[rstest::rstest]
#[case::baseline_event(1)]
#[case::summary_event(4)]
fn progress_rejects_mixed_schema_jsonl_events(#[case] index: usize) {
    let fixture = tempfile::tempdir().unwrap();
    let source = repo_root().join(format!(
        "crates/hoimin-cli/tests/golden/events/schema-v{REPORT_SCHEMA_VERSION}-current.jsonl"
    ));
    let mut events = std::fs::read_to_string(source)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    events[index]["schema_version"] = json!(3);
    let path = write_jsonl(&fixture, &events);

    let result = read_report(&path);

    assert!(result.is_err());
}

#[rstest::rstest]
#[case::document(write_json)]
#[case::events(write_explicit_jsonl)]
fn progress_resume_metadata_rejects_null_in_jsonl(#[case] writer: ReportWriter) {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]["resume"] = Value::Null;
    let report = writer(&fixture, "null.json", &document);

    let result = read_report(&report);

    assert!(result.is_err());
}

fn assert_json_regression_state(bytes: &[u8], state: &str) {
    let doc: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(doc["latest"]["state"], state);
}

#[cfg(unix)]
#[tokio::test]
async fn progress_diagnostics_escape_external_report_paths() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["baseline"] = Value::Null;
    document["mutants"] = json!([]);
    document["summary"]["counts"] = serde_json::to_value(hoimin_core::summarize(&[])).unwrap();
    let path = write_json(&fixture, "report\x1b[31m\r\n\t日本語.json", &document);

    let (code, out, err) = run_progress(&[path.clone(), path.clone()]).await;

    assert_eq!(code, 0);
    let result: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(result["inputs"][0]["source"], path.to_str().unwrap());
    let text = String::from_utf8(err).unwrap();
    assert_eq!(text.lines().count(), 3, "{text:?}");
    assert!(
        text.contains(r"report\u{1b}[31m\r\n\t日本語.json"),
        "{text:?}"
    );
    assert!(
        !text.chars().any(|c| c.is_control() && c != '\n'),
        "{text:?}"
    );
}

type ReportWriter = fn(&tempfile::TempDir, &str, &Value) -> PathBuf;

fn write_explicit_jsonl(fixture: &tempfile::TempDir, _name: &str, document: &Value) -> PathBuf {
    write_jsonl(fixture, &jsonl_events(document))
}

fn changed_status_wire(document: &Value, wire: String, defect: &str) -> String {
    let mut parsed = document.clone();
    parsed["mutants"][0]
        .as_object_mut()
        .unwrap()
        .remove("status");
    if defect == "missing" {
        {
            jsonl_events(&parsed)
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        }
    } else {
        // Both serializers use this compact field representation in event payloads.
        let compact = wire;
        let replacement = match defect {
            "equal" => "\"status\":\"killed\",\"status\":\"killed\"",
            "conflicting" => "\"status\":\"killed\",\"status\":\"survived\"",
            "unknown" => "\"status\":\"future_status\"",
            _ => unreachable!(),
        };
        compact.replacen("\"status\":\"killed\"", replacement, 1)
    }
}

fn independent_contract_history(fixture: &tempfile::TempDir, scenario: &str) -> Vec<PathBuf> {
    // literal scenarios cover the final pair and independent stalled runs.
    let before = detail_report(&[("a", MutationStatus::Killed)]);
    let mut after = detail_report(&[("a", MutationStatus::Survived)]);
    match scenario {
        "unavailable" => {
            after["summary"]["complete"] = json!(false);
            after["summary"]["exit_code"] = json!(4);
        }
        "different" => {
            after["mutants"][0]["candidate"]["file_hash"] = json!("different");
            candidate_identity::assign_id(&mut after["mutants"][0]["candidate"]);
        }
        "stalled" | "saturated" => after = before.clone(),
        "regressing" => {}
        _ => unreachable!(),
    }
    let mut documents = vec![before, after];
    if scenario == "saturated" {
        documents.extend([documents[0].clone(), documents[0].clone()]);
    }
    documents
        .into_iter()
        .enumerate()
        .map(|(index, mut document)| {
            for field in ["run", "baseline", "summary"] {
                document[field]["run_id"] = json!(format!("independent-{index}"));
            }
            for mutant in document["mutants"].as_array_mut().unwrap() {
                mutant["run_id"] = json!(format!("independent-{index}"));
            }
            write_json(fixture, &format!("input-{index}.json"), &document)
        })
        .collect::<Vec<_>>()
}

fn progress_limit_args(reports: &[PathBuf], limit: Option<usize>) -> Vec<String> {
    let mut args = progress_args(reports, limit.is_some());
    if let Some(limit) = limit {
        args.extend(["--details-limit".to_owned(), limit.to_string()]);
    }
    args
}

async fn progress_with_limit(reports: &[PathBuf], limit: usize) -> (i32, Value, Vec<u8>) {
    let out = CollectingWriter::default();
    let err = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(
        progress_limit_args(reports, Some(limit)),
        out.clone(),
        err.clone(),
    )
    .await;
    let document = serde_json::from_slice(&out.snapshot()).unwrap();
    (code, document, err.snapshot())
}

proptest! {
    #[test]
    fn input_dispatch_accepts_arbitrary_header_field_permutations(priorities in any::<[u64; 7]>()) {
        let fixture = tempfile::tempdir().unwrap();
        let mut document = valid_report();
        document["run"]["normalized_config"] = json!({"schema_version": 2, "kind": "not-an-event"});
        let events = jsonl_events(&document);
        let keys = events[0].as_object().unwrap().keys().collect::<Vec<_>>();
        prop_assert_eq!(keys.len(), priorities.len());
        let mut order = (0..keys.len()).collect::<Vec<_>>();
        order.sort_by_key(|index| priorities[*index]);
        let header = order.into_iter().map(|index| {
            let key = keys[index];
            format!("\"{key}\":{}", events[0][key])
        }).collect::<Vec<_>>().join(",");
        let rest = events.iter().skip(1).map(Value::to_string).collect::<Vec<_>>().join("\n");
        let path = fixture.path().join("permuted.jsonl");
        std::fs::write(&path, format!("{{{header}}}\n{rest}\n")).unwrap();

        let actual = read_report(&path);

        prop_assert!(matches!(actual, Ok(InputReport::Usable(_))));
    }
}
