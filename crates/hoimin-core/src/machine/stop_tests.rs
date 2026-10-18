use std::cell::Cell;
use std::time::Duration;

use super::{
    OrderedCandidatePhase, RunPhase, RunSelection, RunState, StoppedCandidate, WorkerState,
    transition,
};
use crate::{
    ByteSpan, CommandArg, MutationCandidate, OutputEvent, RawRunConfig, ResourceControl,
    ResourceMode, RunConfig, RunEffect, RunEvent, VerificationSelection,
};

#[derive(Clone, Copy, Debug, Default)]
struct RankWork {
    // Request-order iterator visits, excluding hashing/comparisons inside HashMap.
    requested_id_visits: usize,
    // Stable-sort key invocations, each performing at most one rank lookup.
    rank_queries: usize,
}

thread_local! {
    static RANK_WORK: Cell<RankWork> = Cell::default();
}

pub(super) fn record_requested_id_visit() {
    RANK_WORK.update(|work| RankWork {
        requested_id_visits: work.requested_id_visits + 1,
        ..work
    });
}

pub(super) fn record_rank_query() {
    RANK_WORK.update(|work| RankWork {
        rank_queries: work.rank_queries + 1,
        ..work
    });
}

fn candidate(id: String, sequence: u64) -> MutationCandidate {
    MutationCandidate {
        id,
        sequence,
        path: "src/calc.py".into(),
        span: ByteSpan {
            start: 1,
            length: 1,
        },
        original: "+".into(),
        replacement: "-".into(),
        operator: "binary_add_sub".into(),
        line: 1,
        column: 1,
        symbol: None,
        file_hash: "hash".into(),
    }
}

fn ready_state(ids: Vec<String>, remaining: Vec<MutationCandidate>, jobs: usize) -> RunState {
    let selected = ids.len();
    let selection = RunSelection::ordered(
        ids,
        VerificationSelection::top(
            selected,
            selected,
            crate::VerificationTopPolicy::Strict,
            false,
        )
        .unwrap(),
    )
    .unwrap();
    let config = RunConfig::try_from(RawRunConfig {
        root: ".".into(),
        sources: vec!["src".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        limits: crate::RawRunLimits {
            jobs,
            max_mutants: selected.max(1),
            ..crate::RawRunLimits::default()
        },
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(crate::EnvironmentSnapshot::Untracked)
    .unwrap();
    let mut state = RunState::with_selection(
        "stop-rank",
        config,
        selection,
        ResourceControl {
            mode: ResourceMode::Hard,
            mechanism: "test".into(),
        },
    );
    state.phase = RunPhase::Mutants;
    state.flags.report.report_started = true;
    state.copy_grant = Some(
        crate::reserve_workspace_copy(
            &mut state.budgets,
            &crate::PreflightCompleted {
                id: crate::EffectId(0),
                per_worker_logical_bytes: 1,
                requested_workers: u32::try_from(jobs).unwrap(),
                aggregate_logical_bytes: u64::try_from(jobs).unwrap(),
                fingerprint: None,
            },
        )
        .unwrap(),
    );
    state.selection.ordered_state_mut().unwrap().phase = OrderedCandidatePhase::Ready {
        remaining: remaining.into(),
    };
    state
}

fn assert_rank_work(candidate_count: usize, repeated: bool) {
    // Arrange: discovery sequence opposes request rank; one worker owns rank zero.
    let candidates: Vec<_> = (0..candidate_count)
        .map(|rank| {
            candidate(
                format!("{rank:064x}"),
                u64::try_from(candidate_count - rank).unwrap(),
            )
        })
        .collect();
    let mut state = ready_state(
        candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect(),
        candidates[1..].to_vec(),
        1,
    );
    state
        .workers
        .insert(0, WorkerState::Ready(candidates[0].clone()));
    if repeated {
        (state, _) = transition(state, RunEvent::CancellationRequested).unwrap();
        assert_eq!(state.stopped_candidates.len(), candidate_count - 1);
        assert!(matches!(
            &state.selection.ordered_state().unwrap().phase,
            OrderedCandidatePhase::Ready { remaining } if remaining.is_empty()
        ));
    } else {
        assert!(state.stopped_candidates.is_empty());
    }
    RANK_WORK.set(RankWork::default());

    // Act: execute the real production stop transition, including preparation/sort.
    let (state, effects) = transition(state, RunEvent::CancellationRequested).unwrap();
    let work = RANK_WORK.get();

    // Assert: sorted input must not scan the request prefix on every sort query.
    eprintln!("N={candidate_count}, repeated={repeated}: {work:?}");
    assert_eq!(work.requested_id_visits, candidate_count);
    assert!(work.rank_queries <= 4 * candidate_count);
    let RunEffect::EmitOutput(output) = &effects[0] else {
        panic!("expected started output")
    };
    assert!(
        matches!(&output.event, OutputEvent::MutantStarted(started) if started.mutant_id == "0".repeat(64))
    );
    assert_eq!(state.stopped_candidates.len(), candidate_count - 1);
    for (rank, stopped) in state.stopped_candidates.iter().enumerate() {
        let StoppedCandidate::NotStarted(candidate) = stopped else {
            panic!("expected queued candidate")
        };
        assert_eq!(candidate.id, format!("{:064x}", rank + 1));
    }
}

macro_rules! rank_work_case {
    ($name:ident, $size:literal, $repeated:literal) => {
        #[test]
        fn $name() {
            assert_rank_work($size, $repeated);
        }
    };
}

rank_work_case!(ordered_stop_rank_work_initial_2000, 2000, false);
rank_work_case!(ordered_stop_rank_work_initial_4000, 4000, false);
rank_work_case!(ordered_stop_rank_work_repeated_2000, 2000, true);
rank_work_case!(ordered_stop_rank_work_repeated_4000, 4000, true);

fn disk_stop() -> RunEvent {
    RunEvent::DiskStopRequested(crate::DiskStopRequested {
        failure: crate::DiskFailure {
            reason: crate::DiskFailureReason::FilesystemReserveReached(crate::DiskObservation {
                owned_bytes: 4096,
                available_bytes: 1024,
                measured_in: Duration::ZERO,
            }),
            secondary: Vec::new(),
        },
    })
}

fn drain_outputs(mut state: RunState, mut effects: Vec<RunEffect>) -> Vec<OutputEvent> {
    let mut outputs = Vec::new();
    for _ in 0..64 {
        if state.phase == RunPhase::Finished {
            return outputs;
        }
        assert_eq!(effects.len(), 1);
        let event = match effects.pop().unwrap() {
            RunEffect::EmitOutput(output) => {
                outputs.push(output.event);
                RunEvent::OutputEmitted(crate::OutputEmitted { id: output.id })
            }
            RunEffect::VerifyOriginals(verify) => {
                RunEvent::OriginalsVerified(crate::OriginalsVerified {
                    id: verify.id,
                    checkpoint: verify.checkpoint,
                })
            }
            RunEffect::Cleanup(cleanup) => RunEvent::CleanupFinished(crate::CleanupFinished {
                id: cleanup.id,
                released_reservations: cleanup.reservations,
            }),
            effect => panic!("unexpected stopped effect: {effect:?}"),
        };
        (state, effects) = transition(state, event).unwrap();
    }
    panic!("stopped drain did not finish");
}

fn assert_disordered_stop(event: RunEvent, exit: i32, repeated: bool) {
    // Arrange: the worker map visits b then d; discovery is a,b,c,d; request is d,b,a,c.
    let mut state = ready_state(
        ["d", "b", "a", "c"].map(String::from).into(),
        vec![candidate("a".into(), 1), candidate("c".into(), 3)],
        2,
    );
    state
        .workers
        .insert(0, WorkerState::Ready(candidate("b".into(), 2)));
    state
        .workers
        .insert(1, WorkerState::Ready(candidate("d".into(), 4)));
    let retired = if repeated {
        let (next, effects) = transition(state, event.clone()).unwrap();
        state = next;
        assert_eq!(state.stopped_candidates.len(), 3);
        assert!(matches!(
            &state.selection.ordered_state().unwrap().phase,
            OrderedCandidatePhase::Ready { remaining } if remaining.is_empty()
        ));
        Some(effects[0].id())
    } else {
        assert!(state.stopped_candidates.is_empty());
        None
    };

    // Act: initial worker+queued merge or repeated worker+stopped merge, never all three.
    let (state, effects) = transition(state, event).unwrap();
    if let Some(id) = retired {
        assert!(matches!(
            transition(state.clone(), RunEvent::OutputEmitted(crate::OutputEmitted { id })),
            Err(super::MachineError::RetiredEffect(retired)) if retired == id
        ));
    }
    let outputs = drain_outputs(state, effects);

    // Assert: literal requested order, unchanged NotRun outcomes and exactly one summary.
    let started: Vec<_> = outputs
        .iter()
        .filter_map(|event| match event {
            OutputEvent::MutantStarted(started) => Some(started.mutant_id.as_str()),
            _ => None,
        })
        .collect();
    let finished: Vec<_> = outputs
        .iter()
        .filter_map(|event| match event {
            OutputEvent::MutantFinished(finished) => {
                Some((finished.candidate.id.as_str(), finished.outcome.status()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(started, ["d", "b", "a", "c"]);
    assert_eq!(
        finished,
        [
            ("d", crate::MutationStatus::NotRun),
            ("b", crate::MutationStatus::NotRun),
            ("a", crate::MutationStatus::NotRun),
            ("c", crate::MutationStatus::NotRun),
        ]
    );
    let summaries: Vec<_> = outputs
        .iter()
        .filter_map(|event| match event {
            OutputEvent::RunFinished(summary) => Some(summary),
            _ => None,
        })
        .collect();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].counts.not_run, 4);
    assert_eq!(summaries[0].counts.killed, 0);
    assert!(!summaries[0].complete);
    assert_eq!(summaries[0].exit_code, exit);
}

macro_rules! disordered_stop_case {
    ($name:ident, $event:expr, $exit:literal, $repeated:literal) => {
        #[test]
        fn $name() {
            assert_disordered_stop($event, $exit, $repeated);
        }
    };
}

disordered_stop_case!(
    ordered_initial_cancel_merges_workers_and_queue,
    RunEvent::CancellationRequested,
    130,
    false
);
disordered_stop_case!(
    ordered_initial_deadline_merges_workers_and_queue,
    RunEvent::DeadlineReached,
    4,
    false
);
disordered_stop_case!(
    ordered_initial_disk_stop_merges_workers_and_queue,
    disk_stop(),
    2,
    false
);
disordered_stop_case!(
    ordered_repeated_cancel_merges_worker_and_stopped,
    RunEvent::CancellationRequested,
    130,
    true
);
disordered_stop_case!(
    ordered_repeated_deadline_merges_worker_and_stopped,
    RunEvent::DeadlineReached,
    4,
    true
);
disordered_stop_case!(
    ordered_repeated_disk_stop_merges_worker_and_stopped,
    disk_stop(),
    2,
    true
);

#[test]
fn unordered_stop_uses_sequence_and_preserves_equal_key_order() {
    // Arrange: existing stopped c and worker b share sequence two, after worker a.
    let mut state = ready_state(vec!["unused".into()], Vec::new(), 2);
    state.selection = RunSelection::all();
    state.flags.scheduling.candidate_exhausted = true;
    state
        .stopped_candidates
        .push_back(StoppedCandidate::NotStarted(candidate("c".into(), 2)));
    state
        .workers
        .insert(0, WorkerState::Ready(candidate("b".into(), 2)));
    state
        .workers
        .insert(1, WorkerState::Ready(candidate("a".into(), 1)));

    // Act.
    let (state, effects) = transition(state, RunEvent::CancellationRequested).unwrap();
    let outputs = drain_outputs(state, effects);

    // Assert: stable sequence fallback keeps c ahead of b at equal keys.
    let started: Vec<_> = outputs
        .iter()
        .filter_map(|event| match event {
            OutputEvent::MutantStarted(started) => Some(started.mutant_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(started, ["a", "c", "b"]);
}
