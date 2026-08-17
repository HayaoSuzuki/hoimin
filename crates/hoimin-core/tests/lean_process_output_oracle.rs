use std::collections::BTreeSet;
use std::time::Duration;

use hoimin_core::{
    EffectId, MutationStatus, OutputSpoolRef, ProcessFinished, ProcessOutputState,
    ProcessTermination, ResourceMode, classify_mutant_result,
};
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/process-output-outcome.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
struct CorpusCase {
    schema: u64,
    id: String,
    mode: String,
    execution: String,
    process: String,
    output: String,
    expected_fatal: bool,
    expected_status: Option<String>,
    expected_termination: Option<String>,
    expected_output_incomplete: bool,
    expected_diagnostic: bool,
    expected_continue: bool,
    input_termination: Option<String>,
}

fn parse_cases() -> Vec<CorpusCase> {
    let cases = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<CorpusCase>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 42);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        42
    );
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "strict");
        assert!(matches!(case.execution.as_str(), "baseline" | "mutant"));
        assert!(matches!(
            case.process.as_str(),
            "known_exit_success"
                | "known_exit_failure"
                | "known_timeout"
                | "known_out_of_memory"
                | "known_process_limit"
                | "known_cancelled"
                | "failed"
        ));
        assert!(matches!(
            case.output.as_str(),
            "complete" | "close_timed_out" | "failed"
        ));
        assert_eq!(case.expected_continue, !case.expected_fatal);
        assert_eq!(
            case.expected_output_incomplete,
            case.execution == "mutant"
                && case.input_termination.is_some()
                && case.output == "close_timed_out"
        );
        assert_eq!(case.expected_diagnostic, case.expected_output_incomplete);
        assert_eq!(case.expected_termination.is_some(), !case.expected_fatal);
    }
    cases
}

fn termination(name: &str) -> ProcessTermination {
    match name {
        "exit_success" => ProcessTermination::Exit(0),
        "exit_failure" => ProcessTermination::Exit(7),
        "timeout" => ProcessTermination::Timeout,
        "out_of_memory" => ProcessTermination::OutOfMemory,
        "process_limit" => ProcessTermination::ProcessLimit,
        "cancelled" => ProcessTermination::Cancelled,
        other => panic!("unknown termination {other}"),
    }
}

fn status(name: &str) -> MutationStatus {
    match name {
        "killed" => MutationStatus::Killed,
        "survived" => MutationStatus::Survived,
        "timeout" => MutationStatus::Timeout,
        "out_of_memory" => MutationStatus::OutOfMemory,
        "process_limit" => MutationStatus::ProcessLimit,
        "not_run" => MutationStatus::NotRun,
        "error" => MutationStatus::Error,
        other => panic!("unknown status {other}"),
    }
}

#[test]
fn lean_process_output_oracle_matches_public_mutant_classification() {
    for case in parse_cases()
        .into_iter()
        .filter(|case| case.execution == "mutant" && !case.expected_fatal)
    {
        let output_state = match case.output.as_str() {
            "complete" => ProcessOutputState::Complete,
            "close_timed_out" => ProcessOutputState::CloseTimedOut,
            other => panic!("non-fatal row has unsupported output {other}"),
        };
        let actual = classify_mutant_result(
            termination(case.input_termination.as_deref().unwrap()),
            output_state,
        );
        assert_eq!(
            actual,
            status(case.expected_status.as_deref().unwrap()),
            "{}",
            case.id
        );
    }
}

#[test]
fn legacy_process_completion_defaults_to_complete_output() {
    let completion = ProcessFinished {
        id: EffectId(7),
        worker: Some(0),
        termination: ProcessTermination::Exit(0),
        output_state: ProcessOutputState::Complete,
        output: OutputSpoolRef {
            token: "output".to_owned(),
            retained: 0,
            observed: 0,
        },
        elapsed: Duration::from_millis(5),
        resource_mode: ResourceMode::Hard,
    };
    let mut legacy = serde_json::to_value(&completion).unwrap();
    legacy.as_object_mut().unwrap().remove("output_state");

    let restored: ProcessFinished = serde_json::from_value(legacy).unwrap();

    assert_eq!(restored, completion);
}
