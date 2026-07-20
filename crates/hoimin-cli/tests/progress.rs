use std::{num::NonZeroUsize, path::PathBuf};

use camino::Utf8PathBuf;
use hoimin_cli::progress::{
    InputReport, ProgressState, UnusableReason, UsableReport, compare_reports, read_report,
};
use hoimin_core::{
    ByteSpan, MutantFinished, MutationCandidate, MutationStatus, REPORT_SCHEMA_VERSION,
    ResourceMode,
};
use serde_json::{Value, json};

#[test]
fn input_accepts_a_complete_baseline_success_report() {
    let fixture = tempfile::tempdir().unwrap();
    let report = write_json(&fixture, "complete.json", valid_report());

    let InputReport::Usable(usable) = read_report(&report).unwrap() else {
        panic!("complete report should be usable");
    };
    assert_eq!(usable.source, report);
    assert_eq!(usable.mutants.len(), 1);
}

#[test]
fn input_marks_missing_baseline_reports_unusable() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["baseline"] = Value::Null;
    let report = write_json(&fixture, "missing-baseline.json", document);

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
    let report = write_json(&fixture, "failed-baseline.json", document);

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
    let report = write_json(&fixture, "incomplete.json", document);

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
    let report = write_json(&fixture, "unsupported-schema.json", document);

    assert!(read_report(&report).is_err());
}

#[test]
fn input_rejects_unsupported_nested_event_schema() {
    let fixture = tempfile::tempdir().unwrap();
    let mut document = valid_report();
    document["baseline"]["schema_version"] = json!(REPORT_SCHEMA_VERSION + 1);
    let report = write_json(&fixture, "unsupported-event-schema.json", document);

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
    let report = write_json(&fixture, "invalid-structure.json", document);

    assert!(read_report(&report).is_err());
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
fn compare_improvement_resets_and_regression_does_not_increment_stalls() {
    let improvement = compare_reports(&[survived(), killed()], nz(3));
    assert_eq!(improvement.consecutive_stalls, 0);
    assert_eq!(improvement.latest, ProgressState::Improving);

    let regression = compare_reports(&[killed(), killed(), survived()], nz(3));
    assert_eq!(regression.consecutive_stalls, 1);
    assert_eq!(regression.latest, ProgressState::Regressing);
}

#[test]
fn compare_an_improvement_resets_prior_stalls() {
    let result = compare_reports(&[killed(), killed(), survived(), killed()], nz(3));

    assert_eq!(result.consecutive_stalls, 0);
    assert_eq!(result.latest, ProgressState::Improving);
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
fn compare_an_empty_common_set_does_not_change_stalls() {
    let result = compare_reports(
        &[
            killed(),
            killed(),
            usable(vec![mutant("different", MutationStatus::Killed)]),
        ],
        nz(3),
    );

    assert_eq!(result.comparisons.len(), 2);
    assert_eq!(result.comparisons[1].common, 0);
    assert_eq!(result.consecutive_stalls, 1);
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

fn write_json(fixture: &tempfile::TempDir, name: &str, document: Value) -> PathBuf {
    let report = fixture.path().join(name);
    std::fs::write(&report, serde_json::to_vec(&document).unwrap()).unwrap();
    report
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
