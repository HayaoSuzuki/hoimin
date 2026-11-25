//! Correspondence for real finalization admission and evidence projection.
use super::{
    CleanupDisposition, CleanupEvidence, CleanupRecord, CleanupThreadFailure, DrainEvidence,
    EndAvailable, ExecutionCleanup, ExecutionFinalization, FilesystemKey, FinalMeasurement,
};
use hoimin_core::{DiskRunSummary, MutationSummary, REPORT_SCHEMA_VERSION, RunSummary};
use proptest::prelude::*;
use rstest::rstest;
use std::{cell::Cell, collections::BTreeMap, io};
#[path = "../../../../hoimin-core/tests/support/disk_model.rs"]
#[allow(dead_code)] // Reuse snapshot encoding, not disk policy decisions.
mod disk;
use crate::quint_itf as quint;
#[derive(Clone, Copy, Debug)]
enum Command {
    Inner(u8),
    Outer(u8),
    Measure(u8, u64),
    Request,
    Defer,
    Complete(u8),
    Available(bool, u64),
}
struct Harness {
    state: ExecutionFinalization,
    queries: Cell<u64>,
}
fn summary() -> RunSummary {
    RunSummary {
        resolved_mutant_timeout: None,
        schema_version: REPORT_SCHEMA_VERSION,
        sequence: 0,
        run_id: "model".into(),
        counts: MutationSummary::default(),
        complete: true,
        exit_code: 0,
        disk: DiskRunSummary::unmeasured(1, 1),
        verification_selection: None,
    }
}
fn record(kind: u8) -> CleanupRecord {
    CleanupRecord {
        disposition: match kind {
            2 => CleanupDisposition::Clean,
            3 => CleanupDisposition::Failed,
            4 => CleanupDisposition::Deferred(None),
            _ => panic!("unknown cleanup"),
        },
        evidence: CleanupEvidence::default(),
    }
}
fn evidence(mask: u8) -> DrainEvidence {
    DrainEvidence {
        process_reaped: mask & 1 != 0,
        output_drained: mask & 2 != 0,
        monitor_joined: mask & 4 != 0,
    }
}
fn decision(result: bool) -> &'static str {
    if result { "yes" } else { "no" }
}
impl Harness {
    fn new() -> Self {
        Self {
            state: ExecutionFinalization::new().unwrap(),
            queries: Cell::new(0),
        }
    }
    fn apply(&mut self, command: Command) -> &'static str {
        match command {
            Command::Inner(mask) => match self.state.observe_inner_drains(evidence(mask)) {
                Ok(value) => decision(value),
                Err(_) => "error",
            },
            Command::Outer(mask) => {
                let mut errors = Vec::new();
                let result = self.state.observe_outer_drains(evidence(mask), &mut errors);
                assert_eq!(errors, Vec::<String>::new());
                decision(result)
            }
            Command::Measure(kind, bytes) => {
                let input = match kind {
                    0 => Ok(Ok(bytes)),
                    1 => Ok(Err(io::Error::other("model"))),
                    2 => Err(CleanupThreadFailure::TimedOut),
                    _ => panic!("unknown measurement"),
                };
                if self.state.observe_measurement(input).is_ok() {
                    "ok"
                } else {
                    "error"
                }
            }
            Command::Request => decision(self.state.request_cleanup()),
            Command::Defer => decision(self.state.defer_cleanup("model-root".into())),
            Command::Complete(kind) => decision(self.state.complete_cleanup(record(kind), "model")),
            Command::Available(success, bytes) => {
                let queries = &self.queries;
                let result = self.state.observe_end_available(|| {
                    queries.set(queries.get() + 1);
                    if success {
                        Ok(BTreeMap::from([(FilesystemKey(0), bytes)]))
                    } else {
                        Err(io::Error::other("model"))
                    }
                });
                if result.is_ok() { "ok" } else { "error" }
            }
        }
    }
    fn view(&self) -> ([u64; 8], Option<u64>, Option<u64>) {
        let state = &self.state;
        let snapshot = state.snapshot();
        let measurement = match state.measurement {
            FinalMeasurement::Quiescent(None) => 0,
            FinalMeasurement::Quiescent(Some(0)) => 1,
            FinalMeasurement::Quiescent(Some(_)) => 2,
            FinalMeasurement::Unjoined => 3,
        };
        let cleanup = match &state.cleanup {
            ExecutionCleanup::NotAttempted(_) => 0,
            ExecutionCleanup::Attempted(_) => 1,
            ExecutionCleanup::Completed(record) => match record.disposition {
                CleanupDisposition::Clean => 2,
                CleanupDisposition::Failed => 3,
                CleanupDisposition::Deferred(_) => 4,
            },
        };
        let availability = match &state.end_available {
            EndAvailable::NotObserved => 0,
            EndAvailable::Unavailable => 1,
            EndAvailable::Available(values) => {
                if values.get(&FilesystemKey(0)) == Some(&0) {
                    2
                } else {
                    3
                }
            }
        };
        let mut report = summary();
        state.project_summary(&mut report);
        (
            [
                disk::component(snapshot.process_drain),
                disk::component(snapshot.output_drain),
                disk::component(snapshot.monitor_join),
                measurement,
                cleanup,
                disk::cleanup(&snapshot, 0),
                availability,
                self.queries.get(),
            ],
            report.disk.removed_logical_bytes,
            state.end_available_bytes(FilesystemKey(0)),
        )
    }
}
fn history(commands: &[Command]) -> Vec<&'static str> {
    let mut actual = Harness::new();
    let mut outcomes = Vec::new();
    for command in commands {
        let before = actual.view();
        let before_safe = before.0[..3].iter().all(|state| *state == 1) && before.0[3] != 3;
        assert_eq!(actual.state.safety_proven(), before_safe);
        let attempted = before.0[4] != 0;
        assert_eq!(actual.state.cleanup_attempted(), attempted);
        assert_eq!(actual.state.cleaned(), before.0[4] == 2);
        let result = actual.apply(*command);
        let after = actual.view();
        if matches!(command, Command::Measure(1, _)) {
            assert_eq!(after, before, "joined IO error preserves prior evidence");
        }
        if matches!(command, Command::Measure(2, _)) {
            assert_eq!(after.0[3], 3);
            assert_eq!(after.1, None);
        }
        if matches!(command, Command::Available(false, _)) && before.0[6] == 0 {
            assert_eq!(after.2, None, "failed free-space reading is not zero");
        }
        if before.0[3] == 3 {
            assert_eq!(after.0[3], 3, "unjoined scan must not be repaired");
        }
        for i in 0..3 {
            if before.0[i] != 0 {
                assert_eq!(after.0[i], before.0[i]);
            }
        }
        assert!(after.0[7] <= 1, "free-space observation may not retry");
        if before.0[6] != 0 {
            assert_eq!(after.2, before.2);
        }
        if let Command::Request = command {
            if result == "yes" {
                assert!(before_safe && !attempted);
            }
            if attempted {
                assert_eq!(result, "no");
            }
        }
        if after.1.is_some() {
            assert_eq!(after.0[4], 2);
            assert_ne!(after.0[3], 3);
        }
        if let Command::Measure(0, bytes) = command
            && before.0[3] != 3
            && actual.state.cleaned()
        {
            assert_eq!(after.1, Some(*bytes));
        }
        if let Command::Available(true, bytes) = command
            && before.0[6] == 0
        {
            assert_eq!(after.2, Some(*bytes));
        }
        outcomes.push(result);
    }
    outcomes
}
fn replay(trace: &str) -> Result<(), String> {
    let states = quint::states(trace);
    assert!(states.len() > 1);
    let mut actual = Harness::new();
    for row in &states {
        let value = u8::try_from(quint::number(&row["arg"])).unwrap();
        let got = match row["op"].as_str().unwrap() {
            "init" => "init",
            "inner" => actual.apply(Command::Inner(value)),
            "outer" => actual.apply(Command::Outer(value)),
            "measure" => actual.apply(match value {
                0 => Command::Measure(0, 0),
                1 => Command::Measure(0, 1),
                2 => Command::Measure(1, 0),
                3 => Command::Measure(2, 0),
                _ => panic!("unknown measurement"),
            }),
            "request" => actual.apply(Command::Request),
            "defer" => actual.apply(Command::Defer),
            "complete" => actual.apply(Command::Complete(value)),
            "available" => actual.apply(Command::Available(
                value != 0,
                u64::from(value.saturating_sub(1)),
            )),
            other => return Err(format!("unknown action {other}")),
        };
        let actual_view = actual.view();
        let mut expected = [0; 8];
        for (i, key) in [
            "process",
            "output",
            "monitor",
            "measurement",
            "cleanup",
            "rootCleanup",
            "availability",
        ]
        .iter()
        .enumerate()
        {
            expected[i] = quint::number(&row[*key]);
        }
        expected[7] = u64::from(expected[6] != 0);
        let removed = row["removedKnown"]
            .as_bool()
            .unwrap()
            .then(|| quint::number(&row["removedBytes"]));
        let available = row["availableKnown"]
            .as_bool()
            .unwrap()
            .then(|| quint::number(&row["availableBytes"]));
        if got != row["result"].as_str().unwrap() || actual_view != (expected, removed, available) {
            return Err(format!("{row}: {got}, {actual_view:?}"));
        }
    }
    Ok(())
}
#[rstest]
#[case::unjoined_cannot_repair(&[Command::Measure(2,0),Command::Measure(0,1),Command::Outer(7),Command::Request],&["error","ok","no","no"])]
#[case::refused_attempt_terminal(&[Command::Request,Command::Outer(7),Command::Request],&["no","yes","no"])]
#[case::free_space_no_retry(&[Command::Available(false,0),Command::Available(true,0)],&["error","ok"])]
#[case::cleanup_measured_zero(&[Command::Outer(7),Command::Measure(0,0),Command::Request,Command::Complete(2)],&["yes","ok","yes","yes"])]
#[case::partial_inner_drains(&[Command::Inner(0),Command::Inner(7),Command::Outer(7),Command::Request],&["no","error","no","no"])]
fn literal_finalization_history(#[case] commands: &[Command], #[case] expected: &[&str]) {
    // Arrange.
    let operations = commands;
    // Act.
    let outcomes = history(operations);
    // Assert.
    assert_eq!(outcomes, expected);
}
fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        (0..8u8).prop_map(Command::Inner),
        (0..8u8).prop_map(Command::Outer),
        (0..3u8, prop_oneof![Just(0), Just(u64::MAX), any::<u64>()])
            .prop_map(|(k, n)| Command::Measure(k, n)),
        Just(Command::Request),
        Just(Command::Defer),
        (2..5u8).prop_map(Command::Complete),
        (
            any::<bool>(),
            prop_oneof![Just(0), Just(u64::MAX), any::<u64>()]
        )
            .prop_map(|(s, n)| Command::Available(s, n))
    ]
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn generated_history_preserves_quiescence_and_attempt_evidence(commands in prop::collection::vec(command(),0..128)) {
        // Arrange: one history with arbitrary full-width byte readings.
        // Act.
        let results=history(&commands);
        // Assert.
        prop_assert_eq!(results.len(),commands.len());
    }
}
#[rstest]
#[case::trace_0(include_str!("../../../../../formal/quint/traces/finalization_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../formal/quint/traces/finalization_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../formal/quint/traces/finalization_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../formal/quint/traces/finalization_3.itf.json"))]
#[case::trace_clean_step_0(include_str!("../../../../../formal/quint/traces/finalization_cleanStep_0.itf.json"))]
#[case::trace_refused_step_0(include_str!("../../../../../formal/quint/traces/finalization_refusedStep_0.itf.json"))]
#[case::trace_unjoined_step_0(include_str!("../../../../../formal/quint/traces/finalization_unjoinedStep_0.itf.json"))]
#[case::trace_available_step_0(include_str!("../../../../../formal/quint/traces/finalization_availableStep_0.itf.json"))]
fn model_matches_finalization_methods(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let outcome = replay(oracle);
    // Assert.
    assert_eq!(outcome, Ok(()));
}
