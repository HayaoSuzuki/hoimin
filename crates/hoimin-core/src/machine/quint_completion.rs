//! Internal-fixture correspondence for the actual compressed completion ledger.
//! This does not claim to execute the surrounding public transition protocol.
use super::{CompletionKind, MachineError, RunState};
use crate::{
    CommandArg, EffectFailed, EffectId, ObserveRemainingBudget, OutputConfig, RawRunConfig,
    RawRunLimits, RemainingBudgetObserved, ResetWorker, ResourceControl, ResourceMode, RunConfig,
    RunEffect, RunEvent, WorkerReset,
};
use proptest::prelude::*;
use rstest::rstest;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
#[path = "../../tests/support/quint.rs"]
mod quint;

#[derive(Clone, Copy, Debug)]
enum Command {
    Issue(u8),
    Complete(u64, u8),
    Retire,
}
#[derive(Clone, Copy, Debug)]
enum Slot {
    Pending(u8),
    Done,
    Retired,
}
#[derive(Debug, Eq, PartialEq)]
struct Snapshot {
    next: u64,
    floor: u64,
    gaps: BTreeSet<u64>,
    retired: BTreeSet<u64>,
    pending: BTreeMap<u64, u8>,
}
fn fixture() -> RunState {
    let config = RunConfig::try_from(RawRunConfig {
        root: ".".into(),
        sources: vec!["src".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        limits: RawRunLimits {
            baseline_timeout: Duration::from_secs(1),
            ..RawRunLimits::default()
        },
        output: OutputConfig::default(),
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(crate::EnvironmentSnapshot::Untracked)
    .unwrap();
    RunState::new(
        "quint",
        config,
        ResourceControl {
            mode: ResourceMode::BestEffort,
            mechanism: "fixture".into(),
        },
    )
}
fn apply(state: &mut RunState, command: Command) -> &'static str {
    match command {
        Command::Issue(kind) => {
            let id = state.allocate_id().unwrap();
            let request = match kind {
                0 => RunEffect::ObserveRemainingBudget(ObserveRemainingBudget { id }),
                1 => RunEffect::ResetWorker(ResetWorker { id, worker: 0 }),
                _ => panic!("unsupported request kind"),
            };
            state.register(&[request]).unwrap();
            "issued"
        }
        Command::Complete(id, kind) => {
            let id = EffectId(id);
            let event = match kind {
                0 => RunEvent::RemainingBudgetObserved(RemainingBudgetObserved {
                    id,
                    remaining: Duration::ZERO,
                }),
                1 => RunEvent::WorkerReset(WorkerReset { id, worker: 0 }),
                2 => RunEvent::EffectFailed(EffectFailed::other(id, "fixture", "fixture")),
                _ => panic!("unsupported completion kind"),
            };
            match state.accept_completion(&event) {
                Ok(Some(_)) => "ok",
                Err(MachineError::RetiredEffect(_)) => "retired",
                Err(MachineError::DuplicateEffect(_)) => "duplicate",
                Err(MachineError::UnknownEffect(_)) => "unknown",
                Err(MachineError::EffectNotPending(_)) => "not_pending",
                Err(MachineError::WrongCompletion { .. }) => "wrong",
                other => panic!("unexpected fixture completion: {other:?}"),
            }
        }
        Command::Retire => {
            state.retire_pending();
            "retired_all"
        }
    }
}
fn actual(state: &RunState) -> Snapshot {
    Snapshot {
        next: state.next_effect_id,
        floor: state.completed_floor,
        gaps: state.completed_gaps.iter().map(|id| id.0).collect(),
        retired: state.retired.iter().map(|id| id.0).collect(),
        pending: state
            .pending
            .iter()
            .map(|(id, pending)| {
                (
                    id.0,
                    match pending.kind() {
                        CompletionKind::RemainingBudgetObserved => 0,
                        CompletionKind::WorkerReset => 1,
                        other => panic!("unexpected fixture pending {other:?}"),
                    },
                )
            })
            .collect(),
    }
}
fn expected(slots: &[Slot]) -> Snapshot {
    let floor = u64::try_from(
        slots
            .iter()
            .take_while(|slot| !matches!(slot, Slot::Pending(_)))
            .count(),
    )
    .unwrap();
    let numbered = || {
        slots
            .iter()
            .enumerate()
            .map(|(i, slot)| (u64::try_from(i).unwrap() + 1, slot))
    };
    Snapshot {
        next: u64::try_from(slots.len()).unwrap() + 1,
        floor,
        gaps: numbered()
            .filter_map(|(id, slot)| (id > floor && matches!(slot, Slot::Done)).then_some(id))
            .collect(),
        retired: numbered()
            .filter_map(|(id, slot)| matches!(slot, Slot::Retired).then_some(id))
            .collect(),
        pending: numbered()
            .filter_map(|(id, slot)| match slot {
                Slot::Pending(kind) => Some((id, *kind)),
                _ => None,
            })
            .collect(),
    }
}
fn journal(slots: &mut Vec<Slot>, command: Command) -> &'static str {
    match command {
        Command::Issue(kind) => {
            slots.push(Slot::Pending(kind));
            "issued"
        }
        Command::Complete(id, kind) => {
            if id == 0 {
                return "duplicate";
            }
            match usize::try_from(id - 1)
                .ok()
                .and_then(|index| slots.get_mut(index))
            {
                None => "unknown",
                Some(Slot::Retired) => "retired",
                Some(Slot::Done) => "duplicate",
                Some(slot @ Slot::Pending(_)) => {
                    let Slot::Pending(wanted) = *slot else {
                        unreachable!()
                    };
                    if kind != 2 && kind != wanted {
                        "wrong"
                    } else {
                        *slot = Slot::Done;
                        "ok"
                    }
                }
            }
        }
        Command::Retire => {
            for slot in slots {
                if matches!(slot, Slot::Pending(_)) {
                    *slot = Slot::Retired;
                }
            }
            "retired_all"
        }
    }
}
fn check_history(commands: &[Command]) -> Vec<&'static str> {
    let mut state = fixture();
    let mut slots = Vec::new();
    let mut results = Vec::new();
    for command in commands {
        let want = journal(&mut slots, *command);
        let got = apply(&mut state, *command);
        assert_eq!(got, want, "{command:?}");
        assert_eq!(actual(&state), expected(&slots), "{command:?}");
        results.push(got);
    }
    results
}
fn model_snapshot(state: &serde_json::Value) -> Snapshot {
    Snapshot {
        next: quint::number(&state["nextId"]),
        floor: quint::number(&state["floor"]),
        gaps: quint::set(&state["gaps"])
            .iter()
            .map(quint::number)
            .collect(),
        retired: quint::set(&state["retired"])
            .iter()
            .map(quint::number)
            .collect(),
        pending: quint::set(&state["pending"])
            .iter()
            .map(|row| {
                (
                    quint::number(&row["id"]),
                    u8::try_from(quint::number(&row["kind"])).unwrap(),
                )
            })
            .collect(),
    }
}
fn replay(trace: &str) -> Result<(), String> {
    let states = quint::states(trace);
    assert!(states.len() > 1, "nonempty oracle");
    let mut state = fixture();
    for row in &states {
        let id = quint::number(&row["id"]);
        let kind = u8::try_from(quint::number(&row["kind"])).unwrap();
        let got = match row["op"].as_str().unwrap() {
            "init" => "init",
            "issue" => apply(&mut state, Command::Issue(kind)),
            "complete" => apply(&mut state, Command::Complete(id, kind)),
            "retire" => apply(&mut state, Command::Retire),
            other => return Err(format!("unknown action {other}")),
        };
        if got != row["result"].as_str().unwrap() || actual(&state) != model_snapshot(row) {
            return Err(format!("{row}: got {got}, {:?}", actual(&state)));
        }
    }
    Ok(())
}

#[rstest]
#[case::out_of_order(&[Command::Issue(0),Command::Issue(1),Command::Complete(2,1),Command::Complete(1,0)], &["issued","issued","ok","ok"])]
#[case::wrong_kind_and_failure(&[Command::Issue(0),Command::Complete(1,1),Command::Complete(1,2),Command::Complete(1,0)], &["issued","wrong","ok","duplicate"])]
#[case::late_retired_and_new_issue(&[Command::Issue(0),Command::Retire,Command::Complete(1,0),Command::Issue(1),Command::Complete(2,1),Command::Complete(1,2)], &["issued","retired_all","retired","issued","ok","retired"])]
#[case::unknown(&[Command::Complete(u64::MAX,0),Command::Complete(0,0)], &["unknown","duplicate"])]
fn literal_completion_history(#[case] commands: &[Command], #[case] expected: &[&str]) {
    // Arrange.
    let history = commands;
    // Act.
    let result = check_history(history);
    // Assert.
    assert_eq!(result, expected);
}
fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        5 => (0..2u8).prop_map(Command::Issue),
        8 => (prop_oneof![0..20u64,Just(u64::MAX)],0..3u8).prop_map(|(id,kind)|Command::Complete(id,kind)),
        1 => Just(Command::Retire),
    ]
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn generated_completion_history_matches_journal(commands in prop::collection::vec(command(),0..128)) {
        // Arrange: one generated sequence of issued requests and asynchronous arrivals.
        // Act.
        let outcomes=check_history(&commands);
        // Assert.
        prop_assert_eq!(outcomes.len(),commands.len());
    }
}
#[rstest]
#[case::trace_0(include_str!("../../../../formal/quint/traces/completion_0.itf.json"))]
#[case::trace_1(include_str!("../../../../formal/quint/traces/completion_1.itf.json"))]
#[case::trace_2(include_str!("../../../../formal/quint/traces/completion_2.itf.json"))]
#[case::trace_3(include_str!("../../../../formal/quint/traces/completion_3.itf.json"))]
#[case::gap_compaction(include_str!("../../../../formal/quint/traces/completion_gapStep_0.itf.json"))]
#[case::wrong_kind_and_failure(include_str!("../../../../formal/quint/traces/completion_wrongKindStep_0.itf.json"))]
fn generated_model_matches_internal_completion_ledger(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let outcome = replay(oracle);
    // Assert.
    assert_eq!(outcome, Ok(()));
}
