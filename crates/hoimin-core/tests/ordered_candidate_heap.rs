//! Ordered waiting queues must not retain selected candidate bodies.
//! Exercise public transitions with candidate allocation inside the meter window;
//! only external setup completions are supplied by this driver.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use hoimin_core::{
    AnalysisFinished, ByteSpan, CandidateLoaded, CandidateReadTarget, CandidateSpoolRef,
    CommandArg, EnvironmentSnapshot, MutationCandidate, OriginalsVerified, OutputEmitted,
    OutputSpoolRef, PreflightCompleted, ProcessFinished, ProcessOutputState, ProcessTermination,
    RawRunConfig, RawRunLimits, ReadCandidate, RemainingBudgetObserved, ResourceControl,
    ResourceMode, RunConfig, RunEffect, RunEvent, RunSelection, RunState, StartRequested,
    TargetSlice, TargetsResolved, VerificationSelection, VerificationTopPolicy, WorkerCreated,
    WorkerReset, transition,
};

#[path = "support/requested_heap.rs"]
mod requested_heap;

#[global_allocator]
static ALLOCATOR: requested_heap::TrackingAllocator = requested_heap::TrackingAllocator;

static OBSERVATION: Mutex<()> = Mutex::new(());
const SELECTED: usize = 256;
const FIELD_BYTES: usize = 32 * 1024;

fn candidate(sequence: u64) -> MutationCandidate {
    MutationCandidate {
        id: format!("{sequence:064x}"),
        sequence,
        path: "src/calc.py".into(),
        span: ByteSpan {
            start: 0,
            length: 1,
        },
        original: "x".repeat(FIELD_BYTES),
        replacement: "y".repeat(FIELD_BYTES),
        operator: "binary_add_sub".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: "0".repeat(64),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "keep the public setup transition sequence together for the isolated heap fixture"
)]
fn waiting_for_first_read(jobs: u32) -> (RunState, ReadCandidate) {
    let config = RunConfig::try_from(RawRunConfig {
        root: ".".into(),
        sources: vec!["src".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        limits: RawRunLimits {
            jobs: usize::try_from(jobs).unwrap(),
            max_mutants: SELECTED,
            ..RawRunLimits::default()
        },
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(EnvironmentSnapshot::Untracked)
    .unwrap();
    let ids = (1..=SELECTED)
        .rev()
        .map(|sequence| format!("{sequence:064x}"))
        .collect();
    let selection = RunSelection::ordered(
        ids,
        VerificationSelection::top(SELECTED, SELECTED, VerificationTopPolicy::Strict, false)
            .unwrap(),
    )
    .unwrap();
    let state = RunState::with_selection(
        "ordered-heap",
        config,
        selection,
        ResourceControl {
            mode: ResourceMode::Hard,
            mechanism: "test".into(),
        },
    );
    let (mut state, effects) = transition(state, RunEvent::StartRequested(StartRequested)).unwrap();
    let mut pending: VecDeque<_> = effects.into();
    while let Some(effect) = pending.pop_front() {
        let event = match effect {
            RunEffect::ReadCandidate(request) => {
                assert!(pending.is_empty());
                return (state, request);
            }
            RunEffect::ResolveTargets(request) => RunEvent::TargetsResolved(TargetsResolved {
                id: request.id,
                targets: vec![TargetSlice {
                    path: "src/calc.py".into(),
                    lines: Vec::new(),
                    symbols: Vec::new(),
                }],
            }),
            RunEffect::Preflight(request) => RunEvent::PreflightCompleted(PreflightCompleted {
                id: request.id,
                per_worker_logical_bytes: 10,
                requested_workers: jobs,
                aggregate_logical_bytes: 10 * u64::from(jobs),
                fingerprint: None,
            }),
            RunEffect::EmitOutput(request) => {
                RunEvent::OutputEmitted(OutputEmitted { id: request.id })
            }
            RunEffect::CreateWorker(request) => RunEvent::WorkerCreated(WorkerCreated {
                id: request.id(),
                worker: request.worker(),
                reservation_id: request.reservation_id(),
            }),
            RunEffect::VerifyOriginals(request) => RunEvent::OriginalsVerified(OriginalsVerified {
                id: request.id,
                checkpoint: request.checkpoint,
            }),
            RunEffect::RunBaseline(request) => RunEvent::BaselineFinished(ProcessFinished {
                id: request.id,
                worker: Some(request.context.worker),
                termination: ProcessTermination::Exit(0),
                output_state: ProcessOutputState::Complete,
                output: OutputSpoolRef {
                    token: "baseline".into(),
                    retained: 0,
                    observed: 0,
                },
                elapsed: Duration::from_millis(1),
                resource_mode: ResourceMode::Hard,
            }),
            RunEffect::ResetWorker(request) => RunEvent::WorkerReset(WorkerReset {
                id: request.id,
                worker: request.worker,
            }),
            RunEffect::ObserveRemainingBudget(request) => {
                RunEvent::RemainingBudgetObserved(RemainingBudgetObserved {
                    id: request.id,
                    remaining: Duration::from_hours(24),
                })
            }
            RunEffect::AnalyzeFile(request) => RunEvent::AnalysisFinished(AnalysisFinished {
                id: request.id,
                spool: Some(CandidateSpoolRef {
                    token: "synthetic-ordered-spool".into(),
                    records: SELECTED as u64,
                }),
                truncated: false,
                diagnostics: Vec::new(),
            }),
            unexpected => panic!("unexpected setup effect: {unexpected:?}"),
        };
        let effects;
        (state, effects) = transition(state, event).unwrap();
        pending.extend(effects);
    }
    panic!("setup did not request its first candidate");
}

fn measure_waiting_state(jobs: u32, complete_scan: bool) -> requested_heap::Measurement {
    let _observation = OBSERVATION.lock().unwrap();
    requested_heap::self_check();
    let (mut state, mut read) = waiting_for_first_read(jobs);

    // Bodies must be allocated here: moving prebuilt candidates into state would
    // hide their retained ownership in the allocator's baseline.
    let baseline = requested_heap::begin();
    for sequence in 1..=SELECTED as u64 {
        assert_eq!(read.target, CandidateReadTarget::Next);
        let effects;
        (state, effects) = transition(
            state,
            RunEvent::CandidateLoaded(CandidateLoaded {
                id: read.id,
                worker: read.worker,
                candidate: Some(candidate(sequence)),
            }),
        )
        .unwrap();
        let [RunEffect::ReadCandidate(next)] = effects.as_slice() else {
            panic!("ordered discovery must finish before dispatch");
        };
        read = next.clone();
    }
    let effects = if complete_scan {
        let effects;
        (state, effects) = transition(
            state,
            RunEvent::CandidateLoaded(CandidateLoaded {
                id: read.id,
                worker: read.worker,
                candidate: None,
            }),
        )
        .unwrap();
        // Accept indexed reads until every worker owns a body. Apply effects
        // remain live and unacknowledged, so no worker can admit another body.
        let mut pending: VecDeque<_> = effects.into();
        let mut applying = Vec::new();
        let mut loaded = 0_u32;
        while let Some(effect) = pending.pop_front() {
            match effect {
                RunEffect::ReadCandidate(request) => {
                    let CandidateReadTarget::Id(id) = request.target else {
                        panic!("completed ordered discovery must use indexed reads");
                    };
                    let sequence = SELECTED as u64 - u64::from(loaded);
                    assert_eq!(id, format!("{sequence:064x}"));
                    loaded += 1;
                    let effects;
                    (state, effects) = transition(
                        state,
                        RunEvent::CandidateLoaded(CandidateLoaded {
                            id: request.id,
                            worker: request.worker,
                            candidate: Some(candidate(sequence)),
                        }),
                    )
                    .unwrap();
                    pending.extend(effects);
                }
                effect @ RunEffect::ApplyMutation(_) => applying.push(effect),
                unexpected => panic!("unexpected dispatch effect: {unexpected:?}"),
            }
        }
        assert_eq!(loaded, jobs);
        assert_eq!(applying.len(), usize::try_from(jobs).unwrap());
        applying
    } else {
        vec![RunEffect::ReadCandidate(read)]
    };
    // Returned effects own worker body clones too; keep both owners alive.
    std::hint::black_box((&state, &effects));
    let measured = requested_heap::finish(baseline);
    drop(effects);
    drop(state);
    measured
}

fn assert_jobs_bounded_waiting_heap(jobs: u32, complete_scan: bool) {
    // Arrange/Act: the only variable-sized payload is 2 * FIELD_BYTES per
    // candidate. Allow four copies per worker plus generous metadata headroom.
    let measured = measure_waiting_state(jobs, complete_scan);
    let bound = 256 * 1024 + usize::try_from(jobs).unwrap() * 4 * 2 * FIELD_BYTES;

    // Assert: changing waiting ownership back to full candidates exceeds this
    // ceiling by several MiB, whether collecting or waiting after EOF.
    eprintln!("ordered-heap jobs={jobs} eof={complete_scan} bound={bound} {measured:?}");
    assert!(
        measured.retained_delta <= bound as i128,
        "ordered queue retained selected bodies: {measured:?}, bound={bound}"
    );
    assert!(
        measured.peak_delta <= bound,
        "ordered queue materialized selected bodies: {measured:?}, bound={bound}"
    );
}

macro_rules! heap_case {
    ($name:ident, $jobs:expr, $eof:expr) => {
        #[test]
        fn $name() {
            assert_jobs_bounded_waiting_heap($jobs, $eof);
        }
    };
}

heap_case!(collecting_two_jobs_drops_waiting_bodies, 2, false);
heap_case!(collected_two_jobs_keeps_only_worker_bodies, 2, true);
heap_case!(collected_eight_jobs_keeps_only_worker_bodies, 8, true);
