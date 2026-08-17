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
    Comparison, InputReport, ProgressError, ProgressState, UnusableReason, UsableReport,
    compare_reports, read_report,
};
use hoimin_core::{
    ByteSpan, MutantFinished, MutationCandidate, MutationStatus, REPORT_SCHEMA_VERSION,
    ResourceMode,
};
use proptest::prelude::*;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

fn original_schema_v2_report() -> PathBuf {
    repo_root().join("crates/hoimin-cli/tests/golden/reports/schema-v2-original.json")
}

#[test]
fn input_accepts_the_oldest_schema_v2_normalized_config() {
    let report = original_schema_v2_report();

    let result = read_report(&report);
    assert!(matches!(result, Ok(InputReport::Usable(_))), "{result:?}");
}

#[test]
fn golden_schema_v2_report_eras_are_usable() {
    let root = repo_root().join("crates/hoimin-cli/tests/golden/reports");

    for name in ["schema-v2-original.json", "schema-v2-current.json"] {
        assert!(
            matches!(read_report(&root.join(name)), Ok(InputReport::Usable(_))),
            "golden report must remain a usable schema-v2 input: {name}"
        );
    }
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
    let before_report: Value = serde_json::from_slice(&before_output.stdout).unwrap();
    let before_mutants = before_report["mutants"].as_array().unwrap();
    assert_eq!(before_mutants.len(), 1);
    assert_eq!(before_mutants[0]["status"], "killed");

    let after_output = run_real_binary(
        project.path(),
        "from src.calc import add; add(2, 3)",
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(
        after_output.status.code(),
        Some(1),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&after_output.stdout),
        String::from_utf8_lossy(&after_output.stderr)
    );
    let after_report: Value = serde_json::from_slice(&after_output.stdout).unwrap();
    let after_mutants = after_report["mutants"].as_array().unwrap();
    assert_eq!(after_mutants.len(), 1);
    assert_eq!(after_mutants[0]["status"], "survived");
    assert_eq!(
        before_mutants[0]["candidate"]["id"],
        after_mutants[0]["candidate"]["id"]
    );

    let before = project.path().join("before.json");
    let after = project.path().join("after.json");
    std::fs::write(&before, &before_output.stdout).unwrap();
    std::fs::write(&after, &after_output.stdout).unwrap();

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["progress", "--format", "json"])
        .arg(&before)
        .arg(&after);
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

#[test]
fn timeout_diagnostic_retains_cleanup_and_both_reader_failures() {
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

    for expected in [
        "fixture timed out",
        "cleanup status=unreaped",
        "signal failed",
        "reap failed",
        "stdout join failed",
        "stderr join failed",
    ] {
        assert!(diagnostic.contains(expected), "{diagnostic}");
    }
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

#[test]
fn input_rejects_non_object_non_null_normalized_configs() {
    let fixture = tempfile::tempdir().unwrap();
    for (name, value) in [
        ("array", json!([])),
        ("string", json!("config")),
        ("number", json!(2)),
        ("boolean", json!(true)),
    ] {
        let mut document = valid_report();
        document["run"]["normalized_config"] = value;
        let report = write_json(&fixture, &format!("{name}.json"), &document);

        assert!(read_report(&report).is_err());
    }
}

#[test]
fn input_rejects_a_missing_normalized_config() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["run"]
        .as_object_mut()
        .unwrap()
        .remove("normalized_config");
    let report = write_json(&fixture, "missing-config.json", &document);

    assert!(read_report(&report).is_err());
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

#[test]
fn input_rejects_repeated_stable_mutant_identities() {
    let fixture = tempfile::tempdir().unwrap();
    for (name, candidate_sequence, expected_message) in [
        ("duplicate", 1, "mutant stable IDs must appear at most once"),
        (
            "mismatch",
            2,
            "mutant stable IDs must map to one candidate sequence",
        ),
    ] {
        let mut document = valid_report();
        let mut duplicate = document["mutants"][0].clone();
        duplicate["sequence"] = json!(4);
        duplicate["candidate"]["sequence"] = json!(candidate_sequence);
        document["mutants"].as_array_mut().unwrap().push(duplicate);
        document["summary"]["sequence"] = json!(5);
        document["summary"]["counts"]["killed"] = json!(2);

        let report = write_json(&fixture, &format!("{name}.json"), &document);
        assert!(matches!(
            read_report(&report),
            Err(ProgressError::InvalidStructure {
                message,
                ..
            }) if message == expected_message
        ));
    }
}

#[test]
fn input_rejects_every_summary_mutant_consistency_mismatch() {
    let fixture = tempfile::tempdir().unwrap();
    let mut cases = Vec::new();

    let mut status = valid_report();
    status["mutants"][0]["status"] = json!("survived");
    cases.push(("status", status));

    let mut missing = valid_report();
    missing["mutants"] = json!([]);
    cases.push(("missing", missing));

    let mut extra = valid_report();
    let mut duplicate = extra["mutants"][0].clone();
    duplicate["sequence"] = json!(4);
    duplicate["candidate"]["id"] = json!("mutant-2");
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

    for (name, document) in cases {
        let report = write_json(&fixture, &format!("{name}.json"), &document);
        let error = read_report(&report).unwrap_err();
        assert!(matches!(
            error,
            ProgressError::InvalidStructure {
                message: "summary counts must match mutant events",
                ..
            }
        ));
    }
}

#[test]
fn input_accepts_a_canonical_null_score_summary() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["mutants"][0]["status"] = json!("timeout");
    document["mutants"][0]["termination"] = json!("Timeout");
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

    assert!(matches!(read_report(&report), Ok(InputReport::Usable(_))));
}

#[test]
fn input_rejects_a_non_null_score_without_decidable_mutants() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["mutants"][0]["status"] = json!("timeout");
    document["mutants"][0]["termination"] = json!("Timeout");
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
        Err(ProgressError::InvalidStructure {
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
    assert!(matches!(error, ProgressError::InvalidStructure { .. }));
}

#[test]
fn input_rejects_nonmonotonic_event_sequences() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["summary"]["sequence"] = json!(3);
    let report = write_json(&fixture, "nonmonotonic-sequence.json", &document);

    let error = read_report(&report).unwrap_err();
    assert!(matches!(error, ProgressError::InvalidStructure { .. }));
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

#[test]
fn compare_improvement_and_regression_break_the_stall_chain() {
    let improvement = compare_reports(&[survived(), killed()], nz(3));
    assert_eq!(improvement.consecutive_stalls, 0);
    assert_eq!(improvement.latest, ProgressState::Improving);

    let regression = compare_reports(&[killed(), killed(), survived()], nz(3));
    assert_eq!(regression.consecutive_stalls, 0);
    assert_eq!(regression.latest, ProgressState::Regressing);
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

#[test]
fn compare_excludes_each_inconclusive_status_from_judgments_and_scores() {
    for status in [
        MutationStatus::Timeout,
        MutationStatus::OutOfMemory,
        MutationStatus::ProcessLimit,
        MutationStatus::Error,
        MutationStatus::NotRun,
    ] {
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

    assert!(result.comparisons.is_empty());
    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Indeterminate);
}

proptest! {
    #[test]
    fn compare_property_self_comparison_is_stable(generated in generated_comparison_reports()) {
        let comparison = compare_pair(&generated.before, &generated.before);
        let expected = comparison_oracle(&generated.before, &generated.before);

        prop_assert_eq!(&comparison, &expected);
        prop_assert_eq!(comparison.added, 0);
        prop_assert_eq!(comparison.removed, 0);
        prop_assert_eq!(comparison.improvements, 0);
        prop_assert_eq!(comparison.regressions, 0);
        prop_assert_eq!(
            comparison.score_delta,
            if generated.before.is_empty() { None } else { Some(0.0) }
        );
        prop_assert_eq!(
            comparison.state,
            if generated.before.is_empty() {
                ProgressState::Indeterminate
            } else {
                ProgressState::Stalled
            }
        );
    }

    #[test]
    fn compare_property_reversal_is_antisymmetric_and_order_independent(
        generated in generated_comparison_reports(),
    ) {
        let forward = compare_pair(&generated.before, &generated.after);
        let reverse = compare_pair(&generated.after, &generated.before);
        let expected = comparison_oracle(&generated.before, &generated.after);

        prop_assert_eq!(&forward, &expected);
        prop_assert_eq!(forward.improvements, reverse.regressions);
        prop_assert_eq!(forward.regressions, reverse.improvements);
        prop_assert_eq!(forward.added, reverse.removed);
        prop_assert_eq!(forward.removed, reverse.added);
        prop_assert_eq!(forward.score_delta, reverse.score_delta.map(|delta| -delta));

        let permuted = compare_pair(&generated.permuted_before, &generated.permuted_after);
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

#[tokio::test]
async fn output_json_exposes_agent_decision_fields() {
    let fixture = tempfile::tempdir().unwrap();
    let reports = (0..4)
        .map(|index| write_json(&fixture, &format!("stalled-{index}.json"), &valid_report()))
        .collect::<Vec<_>>();

    let (code, stdout, stderr) = run_progress(&reports, "json").await;
    let value: Value = serde_json::from_slice(&stdout).unwrap();

    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["latest"]["state"], "saturated");
    assert_eq!(value["latest"]["consecutive_stalls"], 3);
    assert_eq!(value["latest"]["patience"], 3);
    assert_eq!(value["latest"]["saturated"], true);
    assert_eq!(value["inputs"][0]["usable"], true);
    assert_eq!(value["comparisons"].as_array().unwrap().len(), 3);
    assert_eq!(value["comparisons"][2]["state"], "stalled");
    assert_eq!(value["comparisons"][2]["current_score"], 1.0);
    assert_eq!(value["comparisons"][2]["score_delta"], 0.0);
}

#[tokio::test]
async fn output_human_includes_latest_comparison_fields() {
    let fixture = tempfile::tempdir().unwrap();
    let reports = vec![
        write_json(&fixture, "before.json", &valid_report()),
        write_json(&fixture, "after.json", &valid_report()),
    ];

    let (code, stdout, stderr) = run_progress(&reports, "human").await;
    let output = String::from_utf8(stdout).unwrap();

    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    for field in [
        "state: stalled",
        "score: 1.000000",
        "delta: +0.000000",
        "improvements: 0",
        "regressions: 0",
        "carried_survivors: 0",
        "added: 0",
        "removed: 0",
        "ambiguous: 0",
        "inconclusive: 0",
        "stalls: 1",
        "patience: 3",
        "saturated: false",
    ] {
        assert!(output.contains(field), "missing `{field}` from:\n{output}");
    }
}

#[tokio::test]
async fn output_human_omits_stale_comparison_fields_after_an_unusable_report() {
    let fixture = tempfile::tempdir().unwrap();
    let mut before = valid_report();
    before["mutants"][0]["status"] = json!("survived");
    before["mutants"][0]["termination"] = json!({ "Exit": 0 });
    before["summary"]["counts"]["killed"] = json!(0);
    before["summary"]["counts"]["survived"] = json!(1);
    before["summary"]["counts"]["score"] = json!(0.0);
    let after = valid_report();
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let reports = vec![
        write_json(&fixture, "before.json", &before),
        write_json(&fixture, "after.json", &after),
        write_json(&fixture, "incomplete.json", &incomplete),
    ];

    let (code, stdout, stderr) = run_progress(&reports, "human").await;
    let output = String::from_utf8(stdout).unwrap();

    assert_eq!(code, 0);
    assert!(output.contains("state: indeterminate"), "{output}");
    for stale_field in [
        "score:",
        "delta:",
        "improvements:",
        "regressions:",
        "carried_survivors:",
        "added:",
        "removed:",
        "ambiguous:",
        "inconclusive:",
    ] {
        assert!(
            !output.contains(stale_field),
            "unexpected stale field {stale_field:?} in:\n{output}"
        );
    }
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("incomplete run")
    );
}

#[tokio::test]
async fn documented_progress_invocation_accepts_ordered_reports() {
    let fixture = tempfile::tempdir().unwrap();
    let first = write_json(&fixture, "before.json", &valid_report());
    let second = write_json(&fixture, "after.json", &valid_report());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(
        [
            "hoimin",
            "progress",
            "--patience",
            "3",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
        ],
        &mut stdout,
        &mut stderr,
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

    let (code, stdout, stderr) = run_progress(&[malformed, valid], "json").await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("could not parse progress report")
    );
}

#[tokio::test]
async fn output_unreadable_path_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let missing = fixture.path().join("missing.json");
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[missing, valid], "json").await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("could not read progress report")
    );
}

#[tokio::test]
async fn output_unsupported_schema_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let mut unsupported = valid_report();
    unsupported["schema_version"] = json!(REPORT_SCHEMA_VERSION + 1);
    let unsupported = write_json(&fixture, "unsupported.json", &unsupported);
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[unsupported, valid], "json").await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("unsupported schema version")
    );
}

#[tokio::test]
async fn output_invalid_structure_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let mut invalid = valid_report();
    invalid["mutants"] = json!([invalid["run"].clone()]);
    let invalid = write_json(&fixture, "invalid.json", &invalid);
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[invalid, valid], "json").await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("invalid structure in progress report")
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

    let (code, stdout, stderr) = run_progress(&[invalid, valid], "json").await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let diagnostic = String::from_utf8(stderr).unwrap();
    assert!(diagnostic.contains("invalid structure in progress report"));
    assert!(diagnostic.contains("mutant stable IDs must map to one candidate sequence"));
}

#[tokio::test]
async fn output_inconsistent_summary_returns_exit_two() {
    let fixture = tempfile::tempdir().unwrap();
    let mut invalid = valid_report();
    invalid["summary"]["counts"]["killed"] = json!(0);
    let invalid = write_json(&fixture, "inconsistent-summary.json", &invalid);
    let valid = write_json(&fixture, "valid.json", &valid_report());

    let (code, stdout, stderr) = run_progress(&[invalid, valid], "json").await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let diagnostic = String::from_utf8(stderr).unwrap();
    assert!(diagnostic.contains("invalid structure in progress report"));
    assert!(diagnostic.contains("summary counts must match mutant events"));
}

#[tokio::test]
async fn output_unusable_reports_are_indeterminate_and_exit_zero() {
    let fixture = tempfile::tempdir().unwrap();
    let mut missing_baseline = valid_report();
    missing_baseline["baseline"] = Value::Null;
    let missing_baseline = write_json(&fixture, "missing-baseline.json", &missing_baseline);
    let mut incomplete = valid_report();
    incomplete["summary"]["complete"] = json!(false);
    incomplete["summary"]["exit_code"] = json!(4);
    let incomplete = write_json(&fixture, "incomplete.json", &incomplete);
    let mut baseline_failed = valid_report();
    baseline_failed["baseline"]["termination"] = json!({ "Exit": 1 });
    let baseline_failed = write_json(&fixture, "baseline-failed.json", &baseline_failed);

    let (code, stdout, stderr) =
        run_progress(&[missing_baseline, incomplete, baseline_failed], "json").await;
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
    after["mutants"][0]["candidate"]["id"] = json!("different-id");
    let reports = vec![
        write_json(&fixture, "before.json", &before),
        write_json(&fixture, "after.json", &after),
    ];

    let (code, stdout, stderr) = run_progress(&reports, "json").await;
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
    duplicate_mutant["candidate"]["id"] = json!("mutant-2");
    duplicate_mutant["candidate"]["sequence"] = json!(2);
    ambiguous["mutants"] = json!([first_mutant, duplicate_mutant]);
    ambiguous["summary"]["sequence"] = json!(5);
    ambiguous["summary"]["counts"]["killed"] = json!(2);
    let mut different_ids = ambiguous.clone();
    different_ids["mutants"][1]["candidate"]["id"] = json!("mutant-3");
    let reports = vec![
        write_json(&fixture, "before.json", &ambiguous),
        write_json(&fixture, "after.json", &different_ids),
    ];

    let (code, stdout, stderr) = run_progress(&reports, "json").await;
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
        .map(|index| write_json(&fixture, &format!("stalled-{index}.json"), &valid_report()))
        .collect::<Vec<_>>();
    let (code, stdout, _) = run_progress(&reports, "json").await;
    assert_eq!(code, 0);

    let schema = read_schema(&repo_root().join("docs/json-schema/progress-result.schema.json"));
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
        .map(|mutant| (mutant.candidate.id.as_str(), mutant.status))
        .collect::<BTreeMap<_, _>>();
    let after_by_id = after
        .iter()
        .map(|mutant| (mutant.candidate.id.as_str(), mutant.status))
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
        status,
        termination: None,
        output_state: hoimin_core::ProcessOutputState::Complete,
        elapsed_ms: 1,
        resource_mode: ResourceMode::Hard,
        output: None,
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
    std::fs::write(&report, serde_json::to_vec(document).unwrap()).unwrap();
    report
}

async fn run_progress(reports: &[PathBuf], format: &str) -> (i32, Vec<u8>, Vec<u8>) {
    let mut argv = vec![
        "hoimin".to_owned(),
        "progress".to_owned(),
        "--format".to_owned(),
        format.to_owned(),
    ];
    argv.extend(
        reports
            .iter()
            .map(|report| report.to_str().unwrap().to_owned()),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(argv, &mut stdout, &mut stderr).await;
    (code, stdout, stderr)
}

async fn run_real_binary(project: &Path, test_command: &str, timeout: Duration) -> Output {
    let mut command = real_binary_command(project, test_command);
    bounded_output(&mut command, timeout).await.unwrap()
}

fn real_binary_command(project: &Path, test_command: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .arg("run")
        .arg("--root")
        .arg(project)
        .arg("--source")
        .arg("src")
        .arg("--file")
        .arg("src/calc.py")
        .arg("--format")
        .arg("json")
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

fn valid_report() -> Value {
    json!({
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
            "status": "killed",
            "termination": { "Exit": 1 },
            "elapsed_ms": 1,
            "resource_mode": "hard",
            "output": { "token": "mutant-1", "retained": 0, "observed": 0 }
        }],
        "summary": {
            "kind": "run_finished",
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
            "exit_code": 0
        }
    })
}
