//! Domain projections for integration assertions over serialized outcomes.
#![allow(dead_code)]

use hoimin_core::{MutationOutcome, MutationStatus, ProcessTermination};
use serde_json::Value;

pub fn outcome(event: &Value) -> MutationOutcome {
    serde_json::from_value(event["outcome"].clone()).expect("typed mutation outcome")
}

pub fn status(event: &Value) -> Value {
    serde_json::to_value(outcome(event).status()).unwrap()
}

pub fn termination(event: &Value) -> Value {
    match outcome(event) {
        MutationOutcome::Executed { termination, .. } => serde_json::to_value(termination).unwrap(),
        MutationOutcome::Reused { .. } | MutationOutcome::NotRun => Value::Null,
    }
}

pub fn output(event: &Value) -> Value {
    serde_json::to_value(outcome(event).output()).unwrap()
}

pub fn elapsed_ms(event: &Value) -> Value {
    serde_json::to_value(u64::try_from(outcome(event).elapsed().as_millis()).unwrap()).unwrap()
}

/// Constructs matching execution evidence for a test requiring a particular status.
pub fn executed(status: MutationStatus) -> MutationOutcome {
    MutationOutcome::Executed {
        termination: match status {
            MutationStatus::Killed | MutationStatus::Error => ProcessTermination::Exit(1),
            MutationStatus::Survived => ProcessTermination::Exit(0),
            MutationStatus::Timeout => ProcessTermination::Timeout,
            MutationStatus::OutOfMemory => ProcessTermination::OutOfMemory,
            MutationStatus::ProcessLimit => ProcessTermination::ProcessLimit,
            MutationStatus::NotRun => ProcessTermination::Cancelled,
        },
        output_state: if status == MutationStatus::Error {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest::rstest]
    #[case::killed(
        MutationStatus::Killed,
        ProcessTermination::Exit(1),
        hoimin_core::ProcessOutputState::Complete
    )]
    #[case::survived(
        MutationStatus::Survived,
        ProcessTermination::Exit(0),
        hoimin_core::ProcessOutputState::Complete
    )]
    #[case::error(
        MutationStatus::Error,
        ProcessTermination::Exit(1),
        hoimin_core::ProcessOutputState::CloseTimedOut
    )]
    #[case::timeout(
        MutationStatus::Timeout,
        ProcessTermination::Timeout,
        hoimin_core::ProcessOutputState::Complete
    )]
    #[case::memory(
        MutationStatus::OutOfMemory,
        ProcessTermination::OutOfMemory,
        hoimin_core::ProcessOutputState::Complete
    )]
    #[case::process_limit(
        MutationStatus::ProcessLimit,
        ProcessTermination::ProcessLimit,
        hoimin_core::ProcessOutputState::Complete
    )]
    #[case::cancelled(
        MutationStatus::NotRun,
        ProcessTermination::Cancelled,
        hoimin_core::ProcessOutputState::Complete
    )]
    fn execution_evidence_preserves_termination_and_output_state(
        #[case] status: MutationStatus,
        #[case] termination: ProcessTermination,
        #[case] output_state: hoimin_core::ProcessOutputState,
    ) {
        let evidence = serde_json::to_value(executed(status)).unwrap();
        assert_eq!(
            evidence["executed"]["termination"],
            serde_json::to_value(termination).unwrap()
        );
        assert_eq!(
            evidence["executed"]["output_state"],
            serde_json::to_value(output_state).unwrap()
        );
    }

    #[rstest::rstest]
    #[case::reused(&MutationOutcome::Reused { status: hoimin_core::ReusedMutationStatus::Killed })]
    #[case::not_run(&MutationOutcome::NotRun)]
    fn nonexecuted_evidence_has_no_termination(#[case] value: &MutationOutcome) {
        let event = serde_json::json!({"outcome": value});
        assert_eq!(termination(&event), Value::Null);
    }

    #[test]
    fn serialized_execution_keeps_output_and_elapsed_projection() {
        let event = serde_json::json!({"outcome": executed(MutationStatus::Killed)});
        assert_eq!(status(&event), "killed");
        assert_eq!(elapsed_ms(&event), 1);
        assert_eq!(output(&event)["token"], "fixture-output");
        assert_eq!(termination(&event), serde_json::json!({"Exit": 1}));
    }

    #[test]
    #[should_panic(expected = "typed mutation outcome")]
    fn outcome_projection_rejects_invalid_evidence() {
        let _ = outcome(&serde_json::json!({"outcome": 42}));
    }
}
