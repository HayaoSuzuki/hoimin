use hoimin_core::MutantFinished;

#[test]
fn finished_mutant_rejects_flat_legacy_wire() {
    // Arrange
    let document: serde_json::Value = serde_json::from_str(include_str!(
        "../../hoimin-cli/tests/golden/reports/schema-v5-current.json"
    ))
    .unwrap();
    let mut legacy = document["mutants"][0].clone();
    legacy.as_object_mut().unwrap().remove("kind");

    // Act
    let result = serde_json::from_value::<MutantFinished>(legacy);

    // Assert
    assert!(result.is_err());
}

use hoimin_core::{
    MutationOutcome, MutationStatus, OutputSpoolRef, ProcessOutputState, ProcessTermination,
    ReusedMutationStatus,
};
use proptest::prelude::*;
use std::time::Duration;

proptest! {
    #[test]
    fn completed_exit_status_derives_from_exit_code(code in any::<i32>()) {
        // Arrange
        let outcome = MutationOutcome::Executed {
            termination: ProcessTermination::Exit(code),
            output_state: ProcessOutputState::Complete,
            output: OutputSpoolRef { token: "output".into(), retained: 0, observed: 0 },
            elapsed: Duration::ZERO,
        };
        // Act
        let status = outcome.status();
        // Assert
        prop_assert_eq!(status, if code == 0 { MutationStatus::Survived } else { MutationStatus::Killed });
    }

    #[test]
    fn executed_wire_round_trip_preserves_evidence(code in any::<i32>(), seconds in any::<u64>(), nanos in 0u32..1_000_000_000, retained in any::<u64>(), observed in any::<u64>(), close_timeout in any::<bool>()) {
        // Arrange
        let outcome = MutationOutcome::Executed {
            termination: ProcessTermination::Exit(code),
            output_state: if close_timeout { ProcessOutputState::CloseTimedOut } else { ProcessOutputState::Complete },
            output: OutputSpoolRef { token: "output".into(), retained, observed },
            elapsed: Duration::new(seconds, nanos),
        };
        // Act
        let encoded = serde_json::to_value(&outcome).unwrap();
        let decoded: MutationOutcome = serde_json::from_value(encoded).unwrap();
        // Assert
        prop_assert_eq!(decoded, outcome);
    }
}

#[test]
fn reused_outcome_has_no_execution_output() {
    // Arrange
    let outcome = MutationOutcome::Reused {
        status: ReusedMutationStatus::Killed,
    };
    // Act
    let output = outcome.output();
    // Assert
    assert_eq!(output, None);
}

#[test]
fn finished_event_decodes_current_typed_wire() {
    // Arrange
    let document: serde_json::Value = serde_json::from_str(include_str!(
        "../../hoimin-cli/tests/golden/reports/schema-v6-current.json"
    ))
    .unwrap();
    // Act
    let result = serde_json::from_value::<hoimin_core::OutputEvent>(document["mutants"][0].clone());
    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn not_run_projects_zero_elapsed() {
    // Arrange
    let outcome = MutationOutcome::NotRun;
    // Act
    let elapsed = outcome.elapsed();
    // Assert
    assert_eq!(elapsed, Duration::ZERO);
}

#[test]
fn not_run_has_no_output() {
    // Arrange
    let outcome = MutationOutcome::NotRun;
    // Act
    let output = outcome.output();
    // Assert
    assert_eq!(output, None);
}

#[test]
fn outcome_rejects_independent_status() {
    // Arrange
    let mut encoded = serde_json::to_value(MutationOutcome::Executed {
        termination: ProcessTermination::Exit(0),
        output_state: ProcessOutputState::Complete,
        output: OutputSpoolRef {
            token: "output".into(),
            retained: 0,
            observed: 0,
        },
        elapsed: Duration::ZERO,
    })
    .unwrap();
    encoded["executed"]["status"] = serde_json::json!("killed");
    // Act
    let result = serde_json::from_value::<MutationOutcome>(encoded);
    // Assert
    assert!(result.is_err());
}

proptest! {
    #[test]
    fn diagnostic_codes_do_not_change_outcome_status(codes in proptest::collection::vec(prop_oneof![Just(hoimin_core::PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE.to_owned()), any::<String>()], 0..8)) {
        // Arrange
        let mut document: serde_json::Value = serde_json::from_str(include_str!(
            "../../hoimin-cli/tests/golden/reports/schema-v6-current.json"
        )).unwrap();
        let event = &mut document["mutants"][0];
        event["diagnostics"] = serde_json::to_value(codes.into_iter().map(|code| hoimin_core::SessionDiagnostic {
            mutant_id: "golden-mutant".into(), level: "error".into(), code, message: "evidence".into(),
        }).collect::<Vec<_>>()).unwrap();
        // Act
        let decoded: hoimin_core::OutputEvent = serde_json::from_value(event.clone()).unwrap();
        let hoimin_core::OutputEvent::MutantFinished(result) = decoded else { unreachable!() };
        // Assert
        prop_assert_eq!(result.outcome.status(), MutationStatus::Killed);
    }
}

fn termination_strategy() -> impl Strategy<Value = ProcessTermination> {
    prop_oneof![
        any::<i32>().prop_map(ProcessTermination::Exit),
        Just(ProcessTermination::Timeout),
        Just(ProcessTermination::OutOfMemory),
        Just(ProcessTermination::ProcessLimit),
        Just(ProcessTermination::Cancelled)
    ]
}

proptest! {
    #[test]
    fn reused_outcomes_project_zero_elapsed(status in prop_oneof![Just(ReusedMutationStatus::Killed), Just(ReusedMutationStatus::Survived)]) {
        // Arrange
        let outcome = MutationOutcome::Reused { status };
        // Act
        let elapsed = outcome.elapsed();
        // Assert
        prop_assert_eq!(elapsed, Duration::ZERO);
    }

    #[test]
    fn output_close_timeout_overrides_any_termination(termination in termination_strategy()) {
        // Arrange
        let outcome = MutationOutcome::Executed {
            termination, output_state: ProcessOutputState::CloseTimedOut,
            output: OutputSpoolRef { token: "partial".into(), retained: 0, observed: 0 },
            elapsed: Duration::ZERO,
        };
        // Act
        let status = outcome.status();
        // Assert
        prop_assert_eq!(status, MutationStatus::Error);
    }

    #[test]
    fn reused_rejects_every_other_status_name(status in any::<String>().prop_filter("not a reusable status", |name| name != "killed" && name != "survived")) {
        // Arrange
        let encoded = serde_json::json!({"reused":{"status":status}});
        // Act
        let result = serde_json::from_value::<MutationOutcome>(encoded);
        // Assert
        prop_assert!(result.is_err());
    }
}
