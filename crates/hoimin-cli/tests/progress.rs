use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use camino::Utf8PathBuf;
use hoimin_cli::progress::{
    InputReport, ProgressError, ProgressState, UnusableReason, UsableReport, compare_reports,
    read_report,
};
use hoimin_core::{
    ByteSpan, MutantFinished, MutationCandidate, MutationStatus, REPORT_SCHEMA_VERSION,
    ResourceMode,
};
use serde_json::{Value, json};

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
                "common",
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
    let reports = vec![
        write_json(&fixture, "before.json", &ambiguous),
        write_json(&fixture, "after.json", &ambiguous),
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
        elapsed_ms: 1,
        resource_mode: ResourceMode::Hard,
        output: None,
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
