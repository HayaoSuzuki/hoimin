use hoimin_core::{
    ByteSpan, ExitPolicy, MutantFinished, MutantStarted, MutationCandidate, MutationStatus,
    OutputEvent, ProcessTermination, ReportSequence, ResourceMode, RunStarted, exit_code,
    exit_code_for, summarize,
};

#[test]
fn process_terminations_are_classified_without_test_runner_assumptions() {
    use MutationStatus::{Error, Killed, NotRun, OutOfMemory, Survived, Timeout};

    assert_eq!(
        hoimin_core::classify_mutant(ProcessTermination::Exit(0)),
        Survived
    );
    assert_eq!(
        hoimin_core::classify_mutant(ProcessTermination::Exit(7)),
        Killed
    );
    assert_eq!(
        hoimin_core::classify_mutant(ProcessTermination::Timeout),
        Timeout
    );
    assert_eq!(
        hoimin_core::classify_mutant(ProcessTermination::OutOfMemory),
        OutOfMemory
    );
    assert_eq!(
        hoimin_core::classify_mutant(ProcessTermination::ProcessLimit),
        Error
    );
    assert_eq!(
        hoimin_core::classify_mutant(ProcessTermination::Cancelled),
        NotRun
    );
}

#[test]
fn score_uses_only_killed_and_survived() {
    let summary = summarize(&[
        MutationStatus::Killed,
        MutationStatus::Survived,
        MutationStatus::Timeout,
        MutationStatus::OutOfMemory,
        MutationStatus::Error,
        MutationStatus::NotRun,
    ]);

    assert_eq!(summary.score, Some(0.5));
    assert_eq!(summary.killed, 1);
    assert_eq!(summary.survived, 1);
    assert_eq!(summary.inconclusive, 4);
}

#[test]
fn no_candidates_has_a_null_score_and_success_exit() {
    assert_eq!(summarize(&[]).score, None);
    assert_eq!(exit_code(false, false, false), 0);
}

#[test]
fn incomplete_takes_precedence_over_survivors() {
    assert_eq!(exit_code(true, true, false), 4);
    assert_eq!(exit_code(false, true, false), 1);
    assert_eq!(exit_code(false, false, false), 0);
    assert_eq!(exit_code(true, true, true), 130);
}

#[test]
fn complete_exit_policy_has_stable_precedence() {
    let mut policy = ExitPolicy::default();
    assert_eq!(exit_code_for(policy), 0);
    policy.survivors = true;
    assert_eq!(exit_code_for(policy), 1);
    policy.incomplete = true;
    assert_eq!(exit_code_for(policy), 4);
    policy.baseline_failed = true;
    assert_eq!(exit_code_for(policy), 3);
    policy.infrastructure_error = true;
    assert_eq!(exit_code_for(policy), 2);
    policy.interrupted = true;
    assert_eq!(exit_code_for(policy), 130);
}

#[cfg(not(feature = "contracts"))]
#[test]
fn invalid_sequence_is_a_typed_runtime_error() {
    let mut sequence = ReportSequence::new();
    sequence.observe_sequence(10).unwrap();
    assert_eq!(
        sequence.observe_sequence(10),
        Err(hoimin_core::ReportSequenceError::NotMonotonic {
            previous: 10,
            received: 10,
        })
    );
}

#[cfg(not(feature = "contracts"))]
#[test]
fn a_mutant_finish_must_follow_its_matching_start() {
    let mut sequence = ReportSequence::new();
    let finished = finished_event(2, candidate("m1", 7));
    assert_eq!(
        sequence.observe(&finished),
        Err(hoimin_core::ReportSequenceError::MutantNotStarted {
            mutant_id: "m1".to_owned(),
            mutant_sequence: 7,
        })
    );

    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();
    sequence
        .observe(&finished_event(3, candidate("m1", 7)))
        .unwrap();
}

#[test]
fn all_event_variants_have_the_exact_public_kind() {
    let events = [
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
        serde_json::from_value(serde_json::json!({
            "kind": "baseline_finished",
            "schema_version": 1,
            "sequence": 2,
            "run_id": "run-1",
            "termination": { "Exit": 0 },
            "elapsed_ms": 1,
            "resource_mode": "hard",
            "output": { "token": "o", "retained": 0, "observed": 0 }
        }))
        .unwrap(),
        OutputEvent::MutantStarted(MutantStarted::new("run-1", 3, "m1", 7)),
        finished_event(4, candidate("m1", 7)),
        serde_json::from_value(serde_json::json!({
            "kind": "diagnostic", "schema_version": 1, "sequence": 5,
            "run_id": "run-1", "level": "warning", "code": "x", "message": "y"
        }))
        .unwrap(),
        serde_json::from_value(serde_json::json!({
            "kind": "run_finished", "schema_version": 1, "sequence": 6,
            "run_id": "run-1",
            "counts": { "killed": 0, "survived": 0, "timeout": 0,
                "out_of_memory": 0, "error": 0, "not_run": 0,
                "inconclusive": 0, "score": null },
            "complete": true, "exit_code": 0
        }))
        .unwrap(),
    ];
    let kinds = events
        .iter()
        .map(|event| {
            serde_json::to_value(event).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "run_started",
            "baseline_finished",
            "mutant_started",
            "mutant_finished",
            "diagnostic",
            "run_finished",
        ]
    );
    let mut sequence = ReportSequence::new();
    for event in &events {
        sequence.observe(event).unwrap();
    }
}

#[cfg(feature = "contracts")]
#[test]
#[should_panic(expected = "report.sequence.invariant")]
fn invalid_sequence_trips_the_ci_contract() {
    let mut sequence = ReportSequence::new();
    sequence.observe_sequence(10).unwrap();
    let _ = sequence.observe_sequence(9);
}

fn candidate(id: &str, sequence: u64) -> MutationCandidate {
    MutationCandidate {
        id: id.to_owned(),
        sequence,
        path: "src/example.py".into(),
        span: ByteSpan {
            start: 0,
            length: 4,
        },
        original: "True".to_owned(),
        replacement: "False".to_owned(),
        operator: "boolean".to_owned(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: "hash".to_owned(),
    }
}

fn finished_event(sequence: u64, candidate: MutationCandidate) -> OutputEvent {
    OutputEvent::MutantFinished(MutantFinished {
        schema_version: 1,
        sequence,
        run_id: "run-1".to_owned(),
        candidate,
        status: MutationStatus::Killed,
        termination: Some(ProcessTermination::Exit(1)),
        elapsed_ms: 2,
        resource_mode: ResourceMode::Hard,
        output: None,
    })
}
