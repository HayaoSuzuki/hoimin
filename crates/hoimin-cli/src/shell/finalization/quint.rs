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
    Available(Option<[Option<u64>; 2]>),
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
            Command::Available(readings) => {
                let queries = &self.queries;
                let result = self.state.observe_end_available(|| {
                    queries.set(queries.get() + 1);
                    let [first, second] = readings.ok_or_else(|| io::Error::other("model"))?;
                    Ok(BTreeMap::from_iter(
                        [(FilesystemKey(0), first), (FilesystemKey(1), second)]
                            .into_iter()
                            .filter_map(|(key, bytes)| bytes.map(|bytes| (key, bytes))),
                    ))
                });
                if result.is_ok() { "ok" } else { "error" }
            }
        }
    }
    fn view(&self) -> ([u64; 8], Option<u64>, [Option<u64>; 3]) {
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
            EndAvailable::Available(_) => 2,
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
            [0, 1, 2].map(|key| state.end_available_bytes(FilesystemKey(key))),
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
        if let Command::Available(readings) = command
            && before.0[6] == 0
        {
            let [first, second] = readings.unwrap_or([None, None]);
            assert_eq!(after.2, [first, second, None]);
            assert_eq!(after.0[7], 1, "first free-space query must execute");
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
            "available" => actual.apply(Command::Available(match value {
                0 => None,
                1 => Some([Some(0), None]),
                2 => Some([Some(1), None]),
                3 => Some([None, Some(0)]),
                4 => Some([Some(0), Some(1)]),
                5 => Some([None, None]),
                _ => panic!("unknown free-space observation"),
            })),
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
        let mut available = [None; 3];
        for entry in row["availableReadings"]["#map"].as_array().unwrap() {
            let index = usize::try_from(quint::number(&entry[0])).unwrap();
            assert!(index < 2, "unexpected model filesystem key");
            assert_eq!(available[index].replace(quint::number(&entry[1])), None);
        }
        if got != row["result"].as_str().unwrap() || actual_view != (expected, removed, available) {
            return Err(format!("{row}: {got}, {actual_view:?}"));
        }
    }
    Ok(())
}
#[rstest]
#[case::unjoined_cannot_repair(&[Command::Measure(2,0),Command::Measure(0,1),Command::Outer(7),Command::Request],&["error","ok","no","no"])]
#[case::refused_attempt_terminal(&[Command::Request,Command::Outer(7),Command::Request],&["no","yes","no"])]
#[case::free_space_no_retry(&[Command::Available(None),Command::Available(Some([Some(0),None]))],&["error","ok"])]
#[case::empty_free_space_no_retry(&[Command::Available(Some([None,None])),Command::Available(Some([Some(0),None]))],&["ok","ok"])]
#[case::free_space_is_per_filesystem(&[Command::Available(Some([Some(0),Some(u64::MAX)])),Command::Available(Some([Some(1),None]))],&["ok","ok"])]
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
        prop::option::of(prop::array::uniform2(prop::option::of(prop_oneof![
            Just(0),
            Just(u64::MAX),
            any::<u64>()
        ])))
        .prop_map(Command::Available)
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
#[case::paired_available(include_str!("../../../../../formal/quint/traces/finalization_pairedAvailableStep_0.itf.json"))]
#[case::missing_available(include_str!("../../../../../formal/quint/traces/finalization_missingAvailableStep_0.itf.json"))]
fn model_matches_finalization_methods(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let outcome = replay(oracle);
    // Assert.
    assert_eq!(outcome, Ok(()));
}
