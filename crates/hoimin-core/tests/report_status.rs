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

macro_rules! projection_case {
    ($name:ident, $outcome:expr, $expected:literal) => {
        #[test]
        fn $name() {
            // Arrange: expectations are literal classifications, independent of status().
            let result = finished($outcome);
            // Act.
            let direct = serde_json::to_value(&result).unwrap();
            let event = serde_json::to_value(OutputEvent::MutantFinished(result.clone())).unwrap();
            // Assert: direct and event boundaries agree and preserve the owned outcome.
            assert_eq!(direct["status"], $expected);
            assert_eq!(event["status"], $expected);
            assert_eq!(
                serde_json::from_value::<MutantFinished>(direct).unwrap(),
                result
            );
            assert_eq!(
                serde_json::from_value::<OutputEvent>(event.clone()).unwrap(),
                OutputEvent::MutantFinished(result)
            );
            let mut tampered = event;
            tampered["status"] = json!(if $expected == "killed" {
                "survived"
            } else {
                "killed"
            });
            assert!(
                serde_json::from_value::<OutputEvent>(tampered)
                    .unwrap_err()
                    .to_string()
                    .contains("does not match outcome status")
            );
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

macro_rules! duplicate_case {
    ($name:ident, $second:literal) => {
        #[test]
        fn $name() {
            // Arrange: inject raw keys, preserving duplicate evidence before typed decoding.
            let wire = serde_json::to_string(&matching_wire()).unwrap();
            let duplicate = wire.replacen(
                "\"status\":\"killed\"",
                concat!("\"status\":\"killed\",\"status\":\"", $second, "\""),
                1,
            );
            let event = format!("{{\"kind\":\"mutant_finished\",{}", &duplicate[1..]);
            // Act.
            let direct = serde_json::from_str::<MutantFinished>(&duplicate).unwrap_err();
            let tagged = serde_json::from_str::<OutputEvent>(&event).unwrap_err();
            // Assert.
            assert!(
                direct.to_string().contains("duplicate field `status`"),
                "{direct}"
            );
            assert!(
                tagged.to_string().contains("duplicate field `status`"),
                "{tagged}"
            );
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
        let round_trip = serde_json::from_value::<MutantFinished>(wire.clone()).unwrap();
        // Assert.
        prop_assert_eq!(&wire["status"], &json!(expected));
        prop_assert_eq!(round_trip, original);
    }
}
