use super::*;
use std::collections::BTreeSet;

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Queue,
    Start,
    Cancel,
    FinishCounted,
    FinishUncounted,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Rejection {
    AlreadyQueued,
    AlreadyStarted,
    NotQueued,
    NotStarted,
}

#[derive(serde::Deserialize)]
struct Event {
    at_ns: String,
    worker: u32,
    action: Action,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Finish {
    Outstanding {
        worker: u32,
    },
    Validation,
    Saved {
        discovered: u64,
        workers: Vec<WorkerMetric>,
    },
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    id: String,
    mode: String,
    events: Vec<Event>,
    decisions: Vec<Option<Rejection>>,
    elapsed_ns: String,
    elapsed_ms: u64,
    discovered: u64,
    executed: u64,
    finish: Finish,
}

fn duration_from_nanos(value: &str) -> Duration {
    let nanos = value.parse::<u128>().unwrap();
    Duration::new(
        u64::try_from(nanos / 1_000_000_000).unwrap(),
        u32::try_from(nanos % 1_000_000_000).unwrap(),
    )
}

fn record(
    collector: &mut MetricsCollector<FakeClock>,
    event: Event,
    id: &str,
) -> Option<Rejection> {
    let Event {
        at_ns,
        worker,
        action,
    } = event;
    collector.clock.set(duration_from_nanos(&at_ns));
    let result = match action {
        Action::Queue => collector.queued(worker),
        Action::Start => collector.process_started(worker),
        Action::Cancel => collector.cancel_queued(worker),
        Action::FinishCounted => collector.process_finished(worker, true),
        Action::FinishUncounted => collector.process_finished(worker, false),
    };
    let (actual_worker, reason) = match result {
        Ok(()) => return None,
        Err(MetricsError::ProcessAlreadyQueued { worker }) => (worker, Rejection::AlreadyQueued),
        Err(MetricsError::ProcessAlreadyStarted { worker }) => (worker, Rejection::AlreadyStarted),
        Err(MetricsError::ProcessNotQueued { worker }) => (worker, Rejection::NotQueued),
        Err(MetricsError::ProcessNotStarted { worker }) => (worker, Rejection::NotStarted),
        Err(error) => panic!("{id}: unexpected worker error: {error}"),
    };
    assert_eq!(actual_worker, worker, "{id}: rejected worker");
    Some(reason)
}

fn replay(fixture: Fixture) {
    let id = &fixture.id;
    let mut collector = MetricsCollector::with_clock("fixture-run", FakeClock::default());
    let decisions: Vec<_> = fixture
        .events
        .into_iter()
        .map(|event| record(&mut collector, event, id))
        .collect();
    assert_eq!(decisions, fixture.decisions, "{id}: worker transitions");
    collector
        .clock
        .set(duration_from_nanos(&fixture.elapsed_ns));
    let finished = match collector.finish(fixture.discovered, fixture.executed) {
        Ok(metrics) => {
            metrics.validate().unwrap();
            assert_eq!(metrics.elapsed_ms, fixture.elapsed_ms, "{id}");
            assert_eq!(metrics.executed, fixture.executed, "{id}");
            Finish::Saved {
                discovered: metrics.discovered,
                workers: metrics.workers,
            }
        }
        Err(MetricsError::ProcessOutstanding { worker }) => Finish::Outstanding { worker },
        Err(MetricsError::Validation(_)) => Finish::Validation,
        Err(error) => panic!("{id}: unexpected finalization error: {error}"),
    };
    assert_eq!(finished, fixture.finish, "{id}: metrics projection");
}

#[test]
fn worker_metrics_accounting_matches_lean() {
    let mut ids = BTreeSet::new();
    for line in include_str!("../../../formal/HoiminOracle/corpus/worker-metrics.jsonl").lines() {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 2, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        replay(fixture);
    }
    assert_eq!(ids.len(), 139);
}
