use hoimin_core::{
    ByteSpan, DiskFilesystemReport, ExitPolicy, MutantFinished, MutantStarted, MutationCandidate,
    MutationOutcome, MutationStatus, OutputEvent, ProcessOutputState, ProcessTermination,
    ReportSequence, ReportVersions, ResourceMode, RunStarted, VerificationSelectionPolicy,
    exit_code, exit_code_for, summarize,
};

#[test]
fn disk_filesystem_report_rejects_missing_and_unknown_wire_fields() {
    let missing = r#"{
        "key":"workspace",
        "minimum_available_bytes":10
    }"#;
    assert!(serde_json::from_str::<DiskFilesystemReport>(missing).is_err());

    let unknown = r#"{
        "key":"workspace",
        "start_available_bytes":null,
        "minimum_available_bytes":10,
        "end_available_bytes":null,
        "available_bytes_change":null,
        "unexpected":true
    }"#;
    assert!(serde_json::from_str::<DiskFilesystemReport>(unknown).is_err());
}
use proptest::prelude::*;
use serde_json::json;

#[test]
fn disk_filesystem_report_round_trips_through_json() {
    let filesystem = DiskFilesystemReport {
        key: "workspace".into(),
        start_available_bytes: Some(1_024),
        minimum_available_bytes: Some(512),
        end_available_bytes: Some(768),
        available_bytes_change: Some(-256),
    };
    let mut disk = hoimin_core::DiskRunSummary::unmeasured(2_048, 256);
    disk.filesystems.push(filesystem);
    let event = OutputEvent::RunFinished(hoimin_core::RunSummary {
        resolved_mutant_timeout: None,
        schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
        sequence: 1,
        run_id: "run-1".into(),
        counts: summarize(&[]),
        complete: true,
        exit_code: 0,
        disk,
        verification_selection: None,
    });

    let encoded = serde_json::to_vec(&event).unwrap();
    let decoded: OutputEvent = serde_json::from_slice(&encoded).unwrap();

    assert_eq!(decoded, event);
}

#[test]
fn disk_filesystem_delta_preserves_the_full_u64_difference() {
    let positive = DiskFilesystemReport {
        key: "positive".into(),
        start_available_bytes: Some(0),
        minimum_available_bytes: Some(0),
        end_available_bytes: Some(u64::MAX),
        available_bytes_change: Some(i128::from(u64::MAX)),
    };
    let negative = DiskFilesystemReport {
        key: "negative".into(),
        start_available_bytes: Some(u64::MAX),
        minimum_available_bytes: Some(0),
        end_available_bytes: Some(0),
        available_bytes_change: Some(-i128::from(u64::MAX)),
    };

    let mut disk = hoimin_core::DiskRunSummary::unmeasured(u64::MAX, 1);
    disk.filesystems = vec![positive, negative];
    let event = OutputEvent::RunFinished(hoimin_core::RunSummary {
        resolved_mutant_timeout: None,
        schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
        sequence: 1,
        run_id: "wide-delta".into(),
        counts: summarize(&[]),
        complete: true,
        exit_code: 0,
        disk,
        verification_selection: None,
    });

    let encoded = serde_json::to_vec(&event).unwrap();
    let decoded: OutputEvent = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, event);
}

#[test]
fn disk_filesystem_report_rejects_a_non_numeric_delta() {
    let malformed = json!({
        "key": "workspace",
        "start_available_bytes": 1024,
        "minimum_available_bytes": 512,
        "end_available_bytes": 768,
        "available_bytes_change": "-256"
    });

    let decoded = serde_json::from_value::<DiskFilesystemReport>(malformed);

    assert!(decoded.is_err());
}

#[test]
fn disk_filesystem_report_rejects_a_delta_that_disagrees_with_its_endpoints() {
    let inconsistent = json!({
        "key": "workspace",
        "start_available_bytes": 1024,
        "minimum_available_bytes": 512,
        "end_available_bytes": 768,
        "available_bytes_change": -255
    });

    let decoded = serde_json::from_value::<DiskFilesystemReport>(inconsistent);

    assert!(decoded.is_err());
}

#[test]
fn disk_filesystem_report_requires_a_delta_when_both_endpoints_exist() {
    let incomplete = json!({
        "key": "workspace",
        "start_available_bytes": 1024,
        "minimum_available_bytes": 512,
        "end_available_bytes": 768,
        "available_bytes_change": null
    });

    let decoded = serde_json::from_value::<DiskFilesystemReport>(incomplete);

    assert!(decoded.is_err());
}

#[test]
fn unmeasured_disk_summary_makes_no_enforcement_claim() {
    let summary =
        hoimin_core::DiskRunSummary::unmeasured(8 * 1024 * 1024 * 1024, 10 * 1024 * 1024 * 1024);

    assert_eq!(summary.sample_count, 0);
    assert_eq!(summary.filesystems, Vec::new());
    assert_eq!(summary.enforcement, Vec::new());
}

#[test]
fn disk_summary_rejects_unverified_or_unregistered_aggregate_claims() {
    use hoimin_core::{DiskCapabilityProbe, DiskEnforcementReport};

    assert!(
        DiskEnforcementReport::verified_aggregate(
            "unknown_backend".into(),
            DiskCapabilityProbe {
                capability: "unknown".into(),
                verified: true,
                observation: "probe passed".into(),
            },
        )
        .is_err()
    );
    assert!(
        DiskEnforcementReport::verified_aggregate(
            "linux_project_quota".into(),
            DiskCapabilityProbe {
                capability: "project_quota".into(),
                verified: false,
                observation: "probe failed".into(),
            },
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<DiskEnforcementReport>(json!({
            "kind": "verified_aggregate",
            "backend": "linux_project_quota",
            "probe": {
                "capability": "project_quota",
                "verified": false,
                "observation": "probe failed"
            }
        }))
        .is_err()
    );
}

#[test]
fn disk_summary_serializes_primary_stop_and_cleanup_independently() {
    use hoimin_core::{DiskCleanupReport, DiskCleanupStatus, DiskRunSummary, DiskStopReport};

    let summary = DiskRunSummary {
        configured_max_owned_bytes: 8 * 1024 * 1024 * 1024,
        configured_min_free_bytes: 10 * 1024 * 1024 * 1024,
        peak_owned_bytes: 17,
        minimum_available_bytes: Some(23),
        filesystems: Vec::new(),
        sample_count: 2,
        maximum_measurement_ms: 7,
        enforcement: Vec::new(),
        stop: Some(DiskStopReport {
            code: "workspace.size.exceeded".into(),
            owned_bytes: Some(17),
            available_bytes: Some(23),
            message: None,
            secondary: Vec::new(),
        }),
        cleanup: vec![DiskCleanupReport {
            root_id: "execution".into(),
            owner: "hoimin".into(),
            status: DiskCleanupStatus::Failed,
            examined_entries: 3,
            removed_entries: 2,
            details: vec!["one path remained".into()],
            omitted_detail_count: 0,
            remaining_root: Some("lease:execution".into()),
        }],
        removed_logical_bytes: None,
        janitor: None,
    };

    let value = serde_json::to_value(summary).unwrap();
    assert_eq!(value["stop"]["code"], "workspace.size.exceeded");
    assert_eq!(value["cleanup"][0]["status"], "failed");
    assert_eq!(value["removed_logical_bytes"], serde_json::Value::Null);
}

#[test]
fn run_started_versions_serialize_only_os_and_hoimin() {
    let versions = ReportVersions {
        os: "windows".into(),
        hoimin: "0.1.0".into(),
    };
    assert_eq!(
        serde_json::to_value(versions).unwrap(),
        json!({"os":"windows","hoimin":"0.1.0"})
    );
}

#[test]
fn verification_selection_policy_uses_stable_snake_case_serialization() {
    assert_eq!(
        serde_json::to_value(VerificationSelectionPolicy::FileRoundRobinV1).unwrap(),
        json!("file_round_robin_v1"),
    );
}
#[test]
fn process_terminations_are_classified_without_test_runner_assumptions() {
    use MutationStatus::{Killed, NotRun, OutOfMemory, ProcessLimit, Survived, Timeout};

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
        ProcessLimit
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
        MutationStatus::ProcessLimit,
    ]);

    assert_eq!(summary.score, Some(0.5));
    assert_eq!(summary.killed, 1);
    assert_eq!(summary.survived, 1);
    assert_eq!(summary.inconclusive, 5);
    assert_eq!(summary.process_limit, 1);
}

#[test]
fn process_limit_and_survivor_produce_an_incomplete_exit() {
    let summary = summarize(&[MutationStatus::Survived, MutationStatus::ProcessLimit]);
    let policy = ExitPolicy::from_summary(&summary);
    assert!(policy.survivors);
    assert!(policy.incomplete);
    assert_eq!(exit_code_for(policy), 4);
}

#[test]
fn error_results_take_infrastructure_exit_precedence() {
    let error_only = summarize(&[MutationStatus::Error]);
    assert_eq!(exit_code_for(ExitPolicy::from_summary(&error_only)), 2);

    let error_and_survivor = summarize(&[MutationStatus::Error, MutationStatus::Survived]);
    assert_eq!(
        exit_code_for(ExitPolicy::from_summary(&error_and_survivor)),
        2
    );

    let error_survivor_and_incomplete = summarize(&[
        MutationStatus::Error,
        MutationStatus::Survived,
        MutationStatus::Timeout,
    ]);
    let policy = ExitPolicy::from_summary(&error_survivor_and_incomplete);
    assert!(policy.infrastructure_error);
    assert!(policy.incomplete);
    assert!(policy.survivors);
    assert_eq!(exit_code_for(policy), 2);
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
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            10,
            test_resource_control(),
        )))
        .unwrap();
    assert_eq!(
        sequence.observe(&OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
            "run-1", 10, "warning", "x", "y",
        ))),
        Err(hoimin_core::ReportSequenceError::NotMonotonic {
            previous: 10,
            received: 10,
        })
    );
}

#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_requires_run_start_and_rejects_cross_run_events() {
    let mut sequence = ReportSequence::new();
    assert_eq!(
        sequence.observe(&OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
            "run-1", 1, "warning", "x", "y",
        ))),
        Err(hoimin_core::ReportSequenceError::RunNotStarted)
    );
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    assert_eq!(
        sequence.observe(&OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
            "run-2", 2, "warning", "x", "y",
        ))),
        Err(hoimin_core::ReportSequenceError::RunIdMismatch {
            expected: "run-1".to_owned(),
            received: "run-2".to_owned(),
        })
    );
    assert!(matches!(
        sequence.observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            2,
            test_resource_control()
        ))),
        Err(hoimin_core::ReportSequenceError::RunAlreadyStarted { .. })
    ));
}

#[cfg(not(feature = "contracts"))]
#[test]
fn a_mutant_finish_must_follow_its_matching_start() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    let finished = finished_event(2, candidate("m1", 7));
    assert_eq!(
        sequence.observe(&finished),
        Err(hoimin_core::ReportSequenceError::MutantNotStarted {
            mutant_id: "m1".to_owned(),
            mutant_sequence: 7,
        })
    );
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 3, "m1", 7,
        )))
        .unwrap();
    sequence
        .observe(&finished_event(4, candidate("m1", 7)))
        .unwrap();
}

#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_rejects_sequential_stable_identity_reuse_with_another_sequence() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();
    sequence
        .observe(&finished_event(3, candidate("m1", 7)))
        .unwrap();

    assert_eq!(
        sequence.observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 4, "m1", 8,
        ))),
        Err(
            hoimin_core::ReportSequenceError::MutantIdentitySequenceMismatch {
                mutant_id: "m1".to_owned(),
                expected_sequence: 7,
                received_sequence: 8,
            }
        )
    );
}

#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_rejects_concurrent_stable_identity_reuse_with_another_sequence() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();

    assert_eq!(
        sequence.observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 3, "m1", 8,
        ))),
        Err(
            hoimin_core::ReportSequenceError::MutantIdentitySequenceMismatch {
                mutant_id: "m1".to_owned(),
                expected_sequence: 7,
                received_sequence: 8,
            }
        )
    );
}

#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_rejects_duplicate_stable_identity_with_the_same_sequence() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();
    sequence
        .observe(&finished_event(3, candidate("m1", 7)))
        .unwrap();

    assert_eq!(
        sequence.observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 4, "m1", 7,
        ))),
        Err(hoimin_core::ReportSequenceError::DuplicateMutantIdentity {
            mutant_id: "m1".to_owned(),
            mutant_sequence: 7,
        })
    );
}

#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_rejects_finish_with_a_different_identity_sequence() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();

    assert_eq!(
        sequence.observe(&finished_event(3, candidate("m1", 8))),
        Err(
            hoimin_core::ReportSequenceError::MutantIdentitySequenceMismatch {
                mutant_id: "m1".to_owned(),
                expected_sequence: 7,
                received_sequence: 8,
            }
        )
    );
}

#[cfg(not(feature = "contracts"))]
#[test]
fn sequence_accepts_distinct_stable_identities_executing_concurrently() {
    let mut sequence = ReportSequence::new();
    for event in [
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
        OutputEvent::MutantStarted(MutantStarted::new("run-1", 2, "m1", 7)),
        OutputEvent::MutantStarted(MutantStarted::new("run-1", 3, "m2", 8)),
        finished_event(4, candidate("m2", 8)),
        finished_event(5, candidate("m1", 7)),
        run_finished_event(6),
    ] {
        sequence.observe(&event).unwrap();
    }
}

#[cfg(not(feature = "contracts"))]
#[test]
fn run_finished_rejects_every_later_event() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    sequence.observe(&run_finished_event(2)).unwrap();

    assert_eq!(
        sequence.observe(&OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
            "run-1",
            3,
            "warning",
            "late",
            "late diagnostic",
        ))),
        Err(hoimin_core::ReportSequenceError::RunAlreadyFinished {
            run_id: "run-1".to_owned(),
        })
    );
    assert_eq!(
        sequence.observe(&run_finished_event(4)),
        Err(hoimin_core::ReportSequenceError::RunAlreadyFinished {
            run_id: "run-1".to_owned(),
        })
    );
}

#[cfg(not(feature = "contracts"))]
#[test]
fn run_finished_rejects_active_mutants() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();

    assert_eq!(
        sequence.observe(&run_finished_event(3)),
        Err(hoimin_core::ReportSequenceError::RunFinishedWithActiveMutants { count: 1 },)
    );

    sequence
        .observe(&finished_event(3, candidate("m1", 7)))
        .unwrap();
    sequence.observe(&run_finished_event(4)).unwrap();
}

fn typed_outcomes() -> impl Strategy<Value = MutationOutcome> {
    prop_oneof![
        any::<i32>().prop_map(|code| executed_outcome(
            ProcessTermination::Exit(code),
            ProcessOutputState::Complete
        )),
        Just(executed_outcome(
            ProcessTermination::Timeout,
            ProcessOutputState::Complete
        )),
        Just(executed_outcome(
            ProcessTermination::OutOfMemory,
            ProcessOutputState::Complete
        )),
        Just(executed_outcome(
            ProcessTermination::ProcessLimit,
            ProcessOutputState::Complete
        )),
        Just(executed_outcome(
            ProcessTermination::Cancelled,
            ProcessOutputState::Complete
        )),
        Just(executed_outcome(
            ProcessTermination::Timeout,
            ProcessOutputState::CloseTimedOut
        )),
        Just(MutationOutcome::Reused {
            status: hoimin_core::ReusedMutationStatus::Killed
        }),
        Just(MutationOutcome::Reused {
            status: hoimin_core::ReusedMutationStatus::Survived
        }),
        Just(MutationOutcome::NotRun),
    ]
}

proptest! {
    #[test]
    fn sequence_accepts_classified_typed_outcome(outcome in typed_outcomes()) {
        // Arrange
        let mut sequence = sequence_with_started_mutant();
        let event = finished_event_with(3, candidate("m1", 7), outcome);

        // Act
        let result = sequence.observe(&event);

        // Assert
        prop_assert!(result.is_ok());
    }
}

fn sequence_with_started_mutant() -> ReportSequence {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    sequence
        .observe(&OutputEvent::MutantStarted(MutantStarted::new(
            "run-1", 2, "m1", 7,
        )))
        .unwrap();
    sequence
}

fn close_timeout_diagnostic() -> hoimin_core::SessionDiagnostic {
    hoimin_core::SessionDiagnostic {
        mutant_id: "m1".to_owned(),
        level: "error".to_owned(),
        code: "process.output.close.timeout".to_owned(),
        message: "captured output may be incomplete".to_owned(),
    }
}

fn output_close_timeout_event(diagnostics: Vec<hoimin_core::SessionDiagnostic>) -> OutputEvent {
    let mut event = finished_event_with(
        3,
        candidate("m1", 7),
        executed_outcome(
            ProcessTermination::Timeout,
            ProcessOutputState::CloseTimedOut,
        ),
    );
    let OutputEvent::MutantFinished(finished) = &mut event else {
        unreachable!()
    };
    finished.diagnostics = diagnostics;
    event
}

#[test]
fn sequence_accepts_output_close_timeout_error_with_known_termination() {
    let mut sequence = sequence_with_started_mutant();
    let event = output_close_timeout_event(vec![close_timeout_diagnostic()]);

    sequence.observe(&event).unwrap();
}

#[test]
fn typed_mutant_finish_round_trips() {
    let event = finished_event(3, candidate("m1", 7));
    let encoded = serde_json::to_value(&event).unwrap();

    let restored: OutputEvent = serde_json::from_value(encoded).unwrap();

    assert_eq!(restored, event);
}

#[test]
fn all_event_variants_have_the_exact_public_kind() {
    let events = [
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
        serde_json::from_value(json!({
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
        serde_json::from_value(json!({
            "kind": "diagnostic", "schema_version": 1, "sequence": 5,
            "run_id": "run-1", "level": "warning", "code": "x", "message": "y"
        }))
        .unwrap(),
        serde_json::from_value(json!({
            "kind": "run_finished", "schema_version": 1, "sequence": 6,
            "resolved_mutant_timeout": null,
            "run_id": "run-1",
            "counts": { "killed": 0, "survived": 0, "timeout": 0,
                "out_of_memory": 0, "process_limit": 0, "error": 0, "not_run": 0,
                "inconclusive": 0, "score": null },
            "complete": true, "exit_code": 0,
            "disk": disk_json()
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
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            10,
            test_resource_control(),
        )))
        .unwrap();
    let _ = sequence.observe(&OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
        "run-1", 9, "warning", "x", "y",
    )));
}

#[cfg(feature = "contracts")]
#[test]
#[should_panic(expected = "report.sequence.invariant")]
fn cross_run_event_trips_the_ci_contract() {
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    let _ = sequence.observe(&OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
        "run-2", 2, "warning", "x", "y",
    )));
}

#[cfg(feature = "contracts")]
#[test]
#[should_panic(expected = "report.sequence.invariant")]
fn mutant_finish_without_start_trips_the_ci_contract() {
    // Arrange: a valid typed result still requires a matching start event.
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(RunStarted::minimal(
            "run-1",
            1,
            test_resource_control(),
        )))
        .unwrap();
    let event = finished_event_with(
        2,
        candidate("m1", 7),
        executed_outcome(ProcessTermination::Exit(1), ProcessOutputState::Complete),
    );

    // Act
    let _ = sequence.observe(&event);

    // Assert: the producer contract panics for the missing lifecycle transition.
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

fn executed_outcome(
    termination: ProcessTermination,
    output_state: ProcessOutputState,
) -> MutationOutcome {
    MutationOutcome::Executed {
        termination,
        output_state,
        output: hoimin_core::OutputSpoolRef {
            token: "fixture-output".into(),
            retained: 0,
            observed: 0,
        },
        elapsed: std::time::Duration::from_millis(2),
    }
}

fn finished_event(sequence: u64, candidate: MutationCandidate) -> OutputEvent {
    finished_event_with(
        sequence,
        candidate,
        executed_outcome(ProcessTermination::Exit(1), ProcessOutputState::Complete),
    )
}

fn finished_event_with(
    sequence: u64,
    candidate: MutationCandidate,
    outcome: MutationOutcome,
) -> OutputEvent {
    OutputEvent::MutantFinished(MutantFinished {
        schema_version: 1,
        sequence,
        run_id: "run-1".to_owned(),
        candidate,
        outcome,
        resource_mode: ResourceMode::Hard,
        diagnostics: Vec::new(),
    })
}

#[cfg(not(feature = "contracts"))]
fn run_finished_event(sequence: u64) -> OutputEvent {
    serde_json::from_value(json!({
        "kind": "run_finished",
        "resolved_mutant_timeout": null,
        "schema_version": hoimin_core::REPORT_SCHEMA_VERSION,
        "sequence": sequence,
        "run_id": "run-1",
        "counts": {
            "killed": 0,
            "survived": 0,
            "timeout": 0,
            "out_of_memory": 0,
            "process_limit": 0,
            "error": 0,
            "not_run": 0,
            "inconclusive": 0,
            "score": null
        },
        "complete": true,
        "exit_code": 0,
        "disk": disk_json(),
        "verification_selection": null
    }))
    .unwrap()
}

fn disk_json() -> serde_json::Value {
    serde_json::to_value(hoimin_core::DiskRunSummary::unmeasured(8, 10)).unwrap()
}

fn test_resource_control() -> hoimin_core::ResourceControl {
    hoimin_core::ResourceControl {
        mode: hoimin_core::ResourceMode::Hard,
        mechanism: "test_supplied_hard".into(),
    }
}
