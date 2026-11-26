//! Internal-fixture correspondence for the actual compressed completion ledger.
//! This does not claim to execute the surrounding public transition protocol.
use super::{CompletionKind, MachineError, RunState};
use crate::{
    Cleanup, CleanupFinished, CommandArg, EffectFailed, EffectId, ObserveRemainingBudget,
    OutputConfig, RawRunConfig, RawRunLimits, RemainingBudgetObserved, ReservationId, ResetWorker,
    ResourceControl, ResourceMode, RunConfig, RunEffect, RunEvent, WorkerReset,
};
use proptest::prelude::*;
use rstest::rstest;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
#[path = "../../tests/support/quint.rs"]
mod quint;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Work {
    Budget,
    Reset,
    Cleanup(Vec<ReservationId>),
}

#[derive(Clone, Debug)]
enum Reply {
    Success(Work),
    Failure,
}

#[derive(Clone, Debug)]
enum Command {
    Issue(Work),
    Complete(u64, Reply),
    Retire,
}
#[derive(Debug)]
enum Slot {
    Pending(Work),
    Done,
    Retired,
}
#[derive(Debug, Eq, PartialEq)]
struct Snapshot {
    next: u64,
    floor: u64,
    gaps: BTreeSet<u64>,
    retired: BTreeSet<u64>,
    pending: BTreeMap<u64, Work>,
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
fn apply(state: &mut RunState, command: &Command) -> &'static str {
    match command {
        Command::Issue(work) => {
            let id = state.allocate_id().unwrap();
            let request = match work {
                Work::Budget => RunEffect::ObserveRemainingBudget(ObserveRemainingBudget { id }),
                Work::Reset => RunEffect::ResetWorker(ResetWorker { id, worker: 0 }),
                Work::Cleanup(reservations) => RunEffect::Cleanup(Cleanup {
                    id,
                    reservations: reservations.clone(),
                }),
            };
            state.register(&[request]).unwrap();
            "issued"
        }
        Command::Complete(id, reply) => {
            let id = EffectId(*id);
            let event = match reply {
                Reply::Success(Work::Budget) => {
                    RunEvent::RemainingBudgetObserved(RemainingBudgetObserved {
                        id,
                        remaining: Duration::ZERO,
                    })
                }
                Reply::Success(Work::Reset) => RunEvent::WorkerReset(WorkerReset { id, worker: 0 }),
                Reply::Success(Work::Cleanup(reservations)) => {
                    RunEvent::CleanupFinished(CleanupFinished {
                        id,
                        released_reservations: reservations.clone(),
                    })
                }
                Reply::Failure => {
                    RunEvent::EffectFailed(EffectFailed::other(id, "fixture", "fixture"))
                }
            };
            match state.accept_completion(&event) {
                Ok(Some(_)) => "ok",
                Err(MachineError::RetiredEffect(_)) => "retired",
                Err(MachineError::DuplicateEffect(_)) => "duplicate",
                Err(MachineError::UnknownEffect(_)) => "unknown",
                Err(MachineError::EffectNotPending(_)) => "not_pending",
                Err(MachineError::WrongCompletion { .. }) => "wrong",
                Err(MachineError::CleanupReservationMismatch {
                    id: rejected,
                    expected,
                    received,
                }) => {
                    let Reply::Success(Work::Cleanup(reservations)) = reply else {
                        panic!("unexpected reservation mismatch for {reply:?}");
                    };
                    assert_eq!(rejected, id);
                    assert_eq!(&received, reservations);
                    assert_eq!(
                        state.pending[&id].cleanup_reservations.as_ref(),
                        Some(&expected)
                    );
                    "reservation_mismatch"
                }
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
                        CompletionKind::RemainingBudgetObserved => Work::Budget,
                        CompletionKind::WorkerReset => Work::Reset,
                        CompletionKind::CleanupFinished => {
                            Work::Cleanup(pending.cleanup_reservations.as_ref().unwrap().clone())
                        }
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
                Slot::Pending(work) => Some((id, work.clone())),
                _ => None,
            })
            .collect(),
    }
}
fn journal(slots: &mut Vec<Slot>, command: &Command) -> &'static str {
    match command {
        Command::Issue(work) => {
            slots.push(Slot::Pending(work.clone()));
            "issued"
        }
        Command::Complete(id, reply) => {
            if *id == 0 {
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
                    let Slot::Pending(wanted) = slot else {
                        unreachable!()
                    };
                    let result = match (&*wanted, reply) {
                        (_, Reply::Failure)
                        | (Work::Budget, Reply::Success(Work::Budget))
                        | (Work::Reset, Reply::Success(Work::Reset)) => "ok",
                        (Work::Cleanup(expected), Reply::Success(Work::Cleanup(received))) => {
                            // Require one copy of each expected item without canonicalizing sets.
                            if expected.len() == received.len()
                                && expected.iter().enumerate().all(|(index, id)| {
                                    !expected[..index].contains(id)
                                        && received.iter().filter(|item| *item == id).count() == 1
                                })
                            {
                                "ok"
                            } else {
                                "reservation_mismatch"
                            }
                        }
                        _ => "wrong",
                    };
                    if result == "ok" {
                        *slot = Slot::Done;
                    }
                    result
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
        let want = journal(&mut slots, command);
        let got = apply(&mut state, command);
        assert_eq!(got, want, "{command:?}");
        assert_eq!(actual(&state), expected(&slots), "{command:?}");
        results.push(got);
    }
    results
}
fn reservations_from_model(row: &serde_json::Value) -> Vec<ReservationId> {
    row["reservations"]
        .as_array()
        .expect("ITF reservation list")
        .iter()
        .map(|id| ReservationId(quint::number(id)))
        .collect()
}

fn work_from_model(kind: u64, reservations: Vec<ReservationId>) -> Work {
    match kind {
        0 => Work::Budget,
        1 => Work::Reset,
        3 => Work::Cleanup(reservations),
        _ => panic!("unknown model request kind {kind}"),
    }
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
                    work_from_model(quint::number(&row["kind"]), reservations_from_model(row)),
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
        let kind = quint::number(&row["kind"]);
        let reservations = reservations_from_model(row);
        let got = match row["op"].as_str().unwrap() {
            "init" => "init",
            "issue" => apply(
                &mut state,
                &Command::Issue(work_from_model(kind, reservations)),
            ),
            "complete" => {
                let reply = if kind == 2 {
                    Reply::Failure
                } else {
                    Reply::Success(work_from_model(kind, reservations))
                };
                apply(&mut state, &Command::Complete(id, reply))
            }
            "retire" => apply(&mut state, &Command::Retire),
            other => return Err(format!("unknown action {other}")),
        };
        if got != row["result"].as_str().unwrap() || actual(&state) != model_snapshot(row) {
            return Err(format!("{row}: got {got}, {:?}", actual(&state)));
        }
    }
    Ok(())
}

use Command::{Complete, Issue, Retire};
use Reply::{Failure, Success};
use Work::{Budget, Reset};

fn cleanup(ids: &[u64]) -> Work {
    Work::Cleanup(ids.iter().copied().map(ReservationId).collect())
}

#[rstest]
#[case::out_of_order(&[Issue(Budget),Issue(Reset),Complete(2,Success(Reset)),Complete(1,Success(Budget))], &["issued","issued","ok","ok"])]
#[case::wrong_kind_and_failure(&[Issue(Budget),Complete(1,Success(Reset)),Complete(1,Failure),Complete(1,Success(Budget))], &["issued","wrong","ok","duplicate"])]
#[case::late_retired_and_new_issue(&[Issue(Budget),Retire,Complete(1,Success(Budget)),Issue(Reset),Complete(2,Success(Reset)),Complete(1,Failure)], &["issued","retired_all","retired","issued","ok","retired"])]
#[case::unknown(&[Complete(u64::MAX,Success(Budget)),Complete(0,Success(Budget))], &["unknown","duplicate"])]
#[case::cleanup_retry(
    &[Issue(cleanup(&[1,2])),Issue(Budget),Complete(2,Success(Budget)),
      Complete(1,Success(cleanup(&[1,1,2]))),
      Complete(1,Success(cleanup(&[1]))),Complete(1,Success(cleanup(&[1,3]))),
      Complete(1,Success(cleanup(&[2,1]))),Complete(1,Success(cleanup(&[2,1])))],
    &["issued","issued","ok","reservation_mismatch","reservation_mismatch","reservation_mismatch","ok","duplicate"]
)]
#[case::cleanup_failure(
    &[Issue(cleanup(&[1,2])),Complete(1,Failure),Complete(1,Success(cleanup(&[2,1])))],
    &["issued","ok","duplicate"]
)]
fn literal_completion_history(#[case] commands: &[Command], #[case] expected: &[&str]) {
    // Arrange.
    let history = commands;
    // Act.
    let result = check_history(history);
    // Assert.
    assert_eq!(result, expected);
}
fn work() -> impl Strategy<Value = Work> {
    prop_oneof![
        Just(Budget),
        Just(Reset),
        prop::collection::vec(prop_oneof![0..6u64, Just(u64::MAX)], 0..5)
            .prop_map(|ids| cleanup(&ids)),
    ]
}
fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        5 => work().prop_map(Issue),
        8 => (prop_oneof![0..20u64,Just(u64::MAX)], prop_oneof![Just(Failure), work().prop_map(Success)])
            .prop_map(|(id,reply)| Complete(id,reply)),
        1 => Just(Retire),
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
#[case::cleanup_retry(include_str!("../../../../formal/quint/traces/completion_cleanupStep_0.itf.json"))]
fn generated_model_matches_internal_completion_ledger(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let outcome = replay(oracle);
    // Assert.
    assert_eq!(outcome, Ok(()));
}
