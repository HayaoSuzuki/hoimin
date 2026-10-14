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
