use std::time::Duration;

use hoimin_core::{
    ByteSpan, MutantFinished, MutationCandidate, MutationOutcome, OutputEvent, OutputSpoolRef,
    ProcessOutputState, ProcessTermination, REPORT_SCHEMA_VERSION, ResourceMode,
    ReusedMutationStatus,
};
use proptest::prelude::*;
use serde_json::json;

fn executed(termination: ProcessTermination, output_state: ProcessOutputState) -> MutationOutcome {
    MutationOutcome::Executed {
        termination,
        output_state,
        output: OutputSpoolRef {
            token: "output".into(),
            retained: 0,
            observed: 0,
        },
        elapsed: Duration::from_millis(1),
    }
}

fn finished(outcome: MutationOutcome) -> MutantFinished {
    MutantFinished {
        schema_version: REPORT_SCHEMA_VERSION,
        sequence: 3,
        run_id: "run".into(),
        candidate: MutationCandidate {
            id: "mutant".into(),
            sequence: 0,
            path: "calc.py".into(),
            span: ByteSpan {
                start: 0,
                length: 1,
            },
            original: "1".into(),
            replacement: "0".into(),
            operator: "integer_literal".into(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: "hash".into(),
        },
        outcome,
        resource_mode: ResourceMode::Hard,
        diagnostics: Vec::new(),
    }
}

// Decoder fixtures set the expected projection independently of the serializer.
fn wire_for(result: &MutantFinished, status: &str) -> serde_json::Value {
    let mut wire = serde_json::to_value(result).unwrap();
    wire["status"] = json!(status);
    wire
}

macro_rules! projection_case {
    ($name:ident, $outcome:expr, $expected:literal) => {
        mod $name {
            use super::*;

            #[test]
            fn direct_projection() {
                // Arrange: expected classification is literal, independent of status().
                let result = finished($outcome);
                // Act.
                let wire = serde_json::to_value(result).unwrap();
                // Assert.
                assert_eq!(wire["status"], $expected);
            }

            #[test]
            fn tagged_projection() {
                // Arrange.
                let event = OutputEvent::MutantFinished(finished($outcome));
                // Act.
                let wire = serde_json::to_value(event).unwrap();
                // Assert.
                assert_eq!(wire["status"], $expected);
            }

            #[test]
            fn matching_direct_decode() {
                // Arrange: overwrite projected status with the independent expected literal.
                let expected = finished($outcome);
                let wire = wire_for(&expected, $expected);
                // Act.
                let decoded = serde_json::from_value::<MutantFinished>(wire).unwrap();
                // Assert.
                assert_eq!(decoded, expected);
            }

            #[test]
            fn matching_tagged_decode() {
                // Arrange.
                let expected = finished($outcome);
                let mut wire = wire_for(&expected, $expected);
                wire["kind"] = json!("mutant_finished");
                // Act.
                let decoded = serde_json::from_value::<OutputEvent>(wire).unwrap();
                // Assert.
                assert_eq!(decoded, OutputEvent::MutantFinished(expected));
            }

            #[test]
            fn contradictory_direct_rejection() {
                // Arrange: change only the projection to a known incorrect status.
                let result = finished($outcome);
                let wrong = if $expected == "killed" {
                    "survived"
                } else {
                    "killed"
                };
                let wire = wire_for(&result, wrong);
                // Act.
                let error = serde_json::from_value::<MutantFinished>(wire).unwrap_err();
                // Assert.
                assert!(
                    error.to_string().contains("does not match outcome status"),
                    "{error}"
                );
            }

            #[test]
            fn contradictory_tagged_rejection() {
                // Arrange.
                let result = finished($outcome);
                let wrong = if $expected == "killed" {
                    "survived"
                } else {
                    "killed"
                };
                let mut wire = wire_for(&result, wrong);
                wire["kind"] = json!("mutant_finished");
                // Act.
                let error = serde_json::from_value::<OutputEvent>(wire).unwrap_err();
                // Assert.
                assert!(
                    error.to_string().contains("does not match outcome status"),
                    "{error}"
                );
            }
        }
    };
}

macro_rules! execution_cases {
    ($complete:ident, $partial:ident, $termination:expr, $expected:literal) => {
        projection_case!(
            $complete,
            executed($termination, ProcessOutputState::Complete),
            $expected
        );
        projection_case!(
            $partial,
            executed($termination, ProcessOutputState::CloseTimedOut),
            "error"
        );
    };
}
execution_cases!(
    exit_zero,
    exit_zero_close_timeout,
    ProcessTermination::Exit(0),
    "survived"
);
execution_cases!(
    exit_one,
    exit_one_close_timeout,
    ProcessTermination::Exit(1),
    "killed"
);
execution_cases!(
    negative_exit,
    negative_exit_close_timeout,
    ProcessTermination::Exit(-9),
    "killed"
);
execution_cases!(
    timeout,
    timeout_close_timeout,
    ProcessTermination::Timeout,
    "timeout"
);
execution_cases!(
    oom,
    oom_close_timeout,
    ProcessTermination::OutOfMemory,
    "out_of_memory"
);
execution_cases!(
    process_limit,
    process_limit_close_timeout,
    ProcessTermination::ProcessLimit,
    "process_limit"
);
execution_cases!(
    cancelled,
    cancelled_close_timeout,
    ProcessTermination::Cancelled,
    "not_run"
);
projection_case!(
    reused_killed,
    MutationOutcome::Reused {
        status: ReusedMutationStatus::Killed
    },
    "killed"
);
projection_case!(
    reused_survived,
    MutationOutcome::Reused {
        status: ReusedMutationStatus::Survived
    },
    "survived"
);
projection_case!(not_run, MutationOutcome::NotRun, "not_run");

fn matching_wire() -> serde_json::Value {
    let mut wire = serde_json::to_value(finished(executed(
        ProcessTermination::Exit(1),
        ProcessOutputState::Complete,
    )))
    .unwrap();
    wire["status"] = json!("killed");
    wire
}

#[test]
fn missing_status_is_rejected() {
    // Arrange.
    let mut wire = matching_wire();
    wire.as_object_mut().unwrap().remove("status");
    // Act.
    let result = serde_json::from_value::<MutantFinished>(wire);
    // Assert.
    assert!(result.is_err());
}

macro_rules! invalid_status_case {
    ($name:ident, $value:expr, $cause:literal) => {
        #[test]
        fn $name() {
            // Arrange: prove a complete matching input passes before tampering one field.
            let mut wire = matching_wire();
            assert!(serde_json::from_value::<MutantFinished>(wire.clone()).is_ok());
            wire["status"] = $value;
            // Act.
            let error = serde_json::from_value::<MutantFinished>(wire).unwrap_err();
            // Assert.
            assert!(error.to_string().contains($cause), "{error}");
        }
    };
}
invalid_status_case!(
    reject_survived,
    json!("survived"),
    "does not match outcome status killed"
);
invalid_status_case!(
    reject_timeout,
    json!("timeout"),
    "does not match outcome status killed"
);
invalid_status_case!(
    reject_oom,
    json!("out_of_memory"),
    "does not match outcome status killed"
);
invalid_status_case!(
    reject_process_limit,
    json!("process_limit"),
    "does not match outcome status killed"
);
invalid_status_case!(
    reject_error,
    json!("error"),
    "does not match outcome status killed"
);
invalid_status_case!(
    reject_not_run,
    json!("not_run"),
    "does not match outcome status killed"
);
invalid_status_case!(reject_unknown_status, json!("cancelled"), "unknown variant");
invalid_status_case!(reject_null_status, json!(null), "invalid type");
invalid_status_case!(reject_numeric_status, json!(1), "invalid type");

fn duplicate_wires(second: &str) -> (String, String) {
    let wire = serde_json::to_string(&matching_wire()).unwrap();
    let replacement = format!("\"status\":\"killed\",\"status\":\"{second}\"");
    let direct = wire.replacen("\"status\":\"killed\"", &replacement, 1);
    let tagged = format!("{{\"kind\":\"mutant_finished\",{}", &direct[1..]);
    (direct, tagged)
}

macro_rules! duplicate_case {
    ($name:ident, $second:literal) => {
        mod $name {
            use super::*;

            #[test]
            fn direct_rejection() {
                // Arrange: raw keys preserve duplicate evidence before typed decoding.
                let (wire, _) = duplicate_wires($second);
                // Act.
                let error = serde_json::from_str::<MutantFinished>(&wire).unwrap_err();
                // Assert.
                assert!(
                    error.to_string().contains("duplicate field `status`"),
                    "{error}"
                );
            }

            #[test]
            fn tagged_rejection() {
                // Arrange.
                let (_, wire) = duplicate_wires($second);
                // Act.
                let error = serde_json::from_str::<OutputEvent>(&wire).unwrap_err();
                // Assert.
                assert!(
                    error.to_string().contains("duplicate field `status`"),
                    "{error}"
                );
            }
        }
    };
}

duplicate_case!(duplicate_equal_status, "killed");
duplicate_case!(duplicate_conflicting_status, "survived");

#[test]
fn matching_status_does_not_open_unknown_fields() {
    // Arrange.
    let mut wire = matching_wire();
    assert!(serde_json::from_value::<MutantFinished>(wire.clone()).is_ok());
    wire["unexpected"] = json!(true);
    // Act.
    let error = serde_json::from_value::<MutantFinished>(wire).unwrap_err();
    // Assert.
    assert!(
        error.to_string().contains("unknown field `unexpected`"),
        "{error}"
    );
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn arbitrary_exit_projection_respects_output_collection(code in any::<i32>(), partial in any::<bool>()) {
        // Arrange: this oracle uses only the externally specified exit/output rules.
        let expected = if partial { "error" } else if code == 0 { "survived" } else { "killed" };
        let output_state = if partial { ProcessOutputState::CloseTimedOut } else { ProcessOutputState::Complete };
        let original = finished(executed(ProcessTermination::Exit(code), output_state));
        // Act.
        let wire = serde_json::to_value(&original).unwrap();
        // Assert.
        prop_assert_eq!(&wire["status"], &json!(expected));
    }

    #[test]
    fn arbitrary_exit_matching_status_decodes(code in any::<i32>(), partial in any::<bool>()) {
        // Arrange: input status comes from an independent exit/output oracle.
        let status = if partial { "error" } else if code == 0 { "survived" } else { "killed" };
        let output_state = if partial { ProcessOutputState::CloseTimedOut } else { ProcessOutputState::Complete };
        let expected = finished(executed(ProcessTermination::Exit(code), output_state));
        let wire = wire_for(&expected, status);
        // Act.
        let decoded = serde_json::from_value::<MutantFinished>(wire).unwrap();
        // Assert.
        prop_assert_eq!(decoded, expected);
    }

}
