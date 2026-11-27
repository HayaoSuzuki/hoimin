use super::*;
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Valid,
    Invalid {
        message: String,
    },
    ExecutedExceedsDiscovered {
        discovered: u64,
        executed: u64,
    },
    WorkerProcessOverflow,
    WorkerProcessMismatch {
        worker_processes: u64,
        executed: u64,
    },
}

impl From<Result<(), MetricsValidationError>> for Observation {
    fn from(result: Result<(), MetricsValidationError>) -> Self {
        match result {
            Ok(()) => Self::Valid,
            Err(MetricsValidationError::Invalid(message)) => Self::Invalid { message },
            Err(MetricsValidationError::ExecutedExceedsDiscovered {
                discovered,
                executed,
            }) => Self::ExecutedExceedsDiscovered {
                discovered,
                executed,
            },
            Err(MetricsValidationError::WorkerProcessOverflow) => Self::WorkerProcessOverflow,
            Err(MetricsValidationError::WorkerProcessMismatch {
                worker_processes,
                executed,
            }) => Self::WorkerProcessMismatch {
                worker_processes,
                executed,
            },
        }
    }
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    metrics: RunMetrics,
    expected: Observation,
}

#[test]
fn metrics_validation_precedence_matches_lean() {
    let mut ids = BTreeSet::new();
    for line in
        include_str!("../../../../formal/HoiminOracle/corpus/metrics-validation.jsonl").lines()
    {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        let actual = Observation::from(fixture.metrics.validate());
        assert_eq!(actual, fixture.expected, "{}", fixture.id);
    }
    assert_eq!(ids.len(), 430);
}
