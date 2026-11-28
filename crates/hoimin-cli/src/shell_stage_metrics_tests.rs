use super::*;
use crate::metrics::Clock;
use hoimin_core::StageMetric;
use std::cell::Cell;

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Validate,
    Preflight,
    Copy,
    MaterializationVerification,
    Baseline,
    BaselineRestoration,
    BudgetCheck,
    Analyze,
    Mutants,
    Finalize,
    Cleaning,
    Finished,
}

impl From<Phase> for RunPhase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Validate => Self::Validate,
            Phase::Preflight => Self::Preflight,
            Phase::Copy => Self::Copy,
            Phase::MaterializationVerification => Self::MaterializationVerification,
            Phase::Baseline => Self::Baseline,
            Phase::BaselineRestoration => Self::BaselineRestoration,
            Phase::BudgetCheck => Self::BudgetCheck,
            Phase::Analyze => Self::Analyze,
            Phase::Mutants => Self::Mutants,
            Phase::Finalize => Self::Finalize,
            Phase::Cleaning => Self::Cleaning,
            Phase::Finished => Self::Finished,
        }
    }
}

#[derive(PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Signal {
    Ordinary,
    TargetsResolved,
    PreflightCompleted,
    CleanupFinished,
}

#[derive(serde::Deserialize)]
struct Transition {
    at_ns: u64,
    previous: Phase,
    next: Phase,
    signal: Signal,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Warning {
    code: String,
    message: String,
}

#[derive(serde::Deserialize)]
struct Observation {
    stages: Vec<StageMetric>,
    warnings: Vec<Warning>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    id: String,
    mode: String,
    initial_stages: Vec<String>,
    transitions: Vec<Transition>,
    expected: Vec<Observation>,
}

struct FixtureClock<'a>(&'a Cell<Duration>);

impl Clock for FixtureClock<'_> {
    fn elapsed(&self) -> Duration {
        self.0.get()
    }
}

fn replay(fixture: &Fixture, prefix: usize, expected: &Observation) {
    let elapsed = Cell::new(Duration::ZERO);
    let mut collector = MetricsCollector::with_clock("stage-fixture", FixtureClock(&elapsed));
    for stage in &fixture.initial_stages {
        collector.begin_stage(stage).unwrap();
    }
    let mut collector = Some(collector);
    let mut warnings = Vec::new();
    for transition in &fixture.transitions[..prefix] {
        elapsed.set(Duration::from_nanos(transition.at_ns));
        observe_accepted_transition(
            &mut collector,
            &mut warnings,
            transition.previous.into(),
            transition.next.into(),
            transition.signal == Signal::TargetsResolved,
            transition.signal == Signal::PreflightCompleted,
            transition.signal == Signal::CleanupFinished,
        );
    }
    let metrics = collector.unwrap().finish(0, 0).unwrap();
    assert_eq!(
        metrics.stages, expected.stages,
        "{} at prefix {prefix}",
        fixture.id
    );
    let warnings: Vec<_> = warnings
        .into_iter()
        .map(|(code, message)| Warning {
            code: code.to_owned(),
            message,
        })
        .collect();
    assert_eq!(
        warnings, expected.warnings,
        "{} at prefix {prefix}",
        fixture.id
    );
}

#[test]
fn stage_metrics_follow_accepted_cli_transitions() {
    let mut ids = BTreeSet::new();
    for line in include_str!("../../../formal/HoiminOracle/corpus/stage-metrics.jsonl").lines() {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        assert_eq!(fixture.expected.len(), fixture.transitions.len() + 1);
        for (prefix, expected) in fixture.expected.iter().enumerate() {
            replay(&fixture, prefix, expected);
        }
    }
    assert_eq!(ids.len(), 13);
}
