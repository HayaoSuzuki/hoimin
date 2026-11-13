#![cfg_attr(coverage_nightly, feature(coverage_attribute, stmt_expr_attributes))]
//! Identity-only transitions must not allocate copies of candidate source text.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use hoimin_core::{
    AnalysisFinished, ByteSpan, CandidateLoaded, CandidateSpoolRef, CommandArg,
    EnvironmentSnapshot, MutationCandidate, OriginalsVerified, OutputEmitted, OutputSpoolRef,
    PreflightCompleted, ProcessFinished, ProcessOutputState, ProcessTermination, RawRunConfig,
    RawRunLimits, ReadCandidate, ResourceControl, ResourceMode, RunConfig, RunEffect, RunEvent,
    RunState, StartRequested, TargetSlice, TargetsResolved, WorkerCreated, WorkerReset, transition,
};

#[path = "support/worker_heap.rs"]
mod worker_heap;

#[global_allocator]
static ALLOCATOR: worker_heap::TrackingAllocator = worker_heap::TrackingAllocator;

static OBSERVATION: Mutex<()> = Mutex::new(());
const SELECTED: usize = 64;

fn candidate(sequence: u64, field_bytes: usize) -> MutationCandidate {
    MutationCandidate {
        id: format!("{sequence:064x}"),
        sequence,
        path: "src/calc.py".into(),
        span: ByteSpan {
            start: 0,
            length: 1,
        },
        original: "x".repeat(field_bytes),
        replacement: "y".repeat(field_bytes),
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
fn waiting_for_first_read(jobs: u32, session: bool) -> (RunState, ReadCandidate) {
    let config = RunConfig::try_from(RawRunConfig {
        root: ".".into(),
        session: session.then(|| hoimin_core::SessionConfig {
            path: "session.sqlite3".into(),
        }),
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
    let state = RunState::new(
        "worker-heap",
        config,
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
                fingerprint: session.then_some(hoimin_core::RunFingerprint::from_bytes([7; 32])),
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
            RunEffect::BeginSession(request) => {
                RunEvent::SessionStarted(hoimin_core::SessionStarted {
                    id: request.id,
                    run_id: request.run_id,
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
            unexpected => {
                // Test failure reporting only; a passing scenario cannot execute this expression.
                #[cfg(coverage_nightly)]
                {
                    (#[coverage(off)]
                    || -> ! { panic!("unexpected setup effect: {unexpected:?}") })(
                    )
                }
                #[cfg(not(coverage_nightly))]
                {
                    panic!("unexpected setup effect: {unexpected:?}")
                }
            }
        };
        let effects;
        (state, effects) = transition(state, event).unwrap();
        pending.extend(effects);
    }
    {
        // Test failure reporting only; a passing scenario cannot execute this expression.
        #[cfg(coverage_nightly)]
        {
            (#[coverage(off)]
            || -> ! { panic!("setup did not request its first candidate") })()
        }
        #[cfg(not(coverage_nightly))]
        {
            panic!("setup did not request its first candidate")
        }
    }
}

fn observed(
    state: RunState,
    event: RunEvent,
) -> (RunState, Vec<RunEffect>, worker_heap::Measurement) {
    let baseline = worker_heap::begin();
    let (state, effects) = transition(state, event).unwrap();
    let measured = worker_heap::finish(baseline);
    std::hint::black_box((&state, &effects));
    (state, effects, measured)
}

// The body exists before the observation. Bounds concern extra requested heap
// during each transition, not allocator bookkeeping, process RSS, or setup.
fn check_window(label: &str, measured: &worker_heap::Measurement) -> bool {
    check_payload_window(label, measured, 0)
}

fn check_payload_window(
    label: &str,
    measured: &worker_heap::Measurement,
    body_bytes: usize,
) -> bool {
    // A finished output and the unacknowledged worker each need an owner.
    let bound = body_bytes + 32 * 1024;
    let within_bound = measured.allocated < bound && measured.peak_delta < bound;
    if !within_bound {
        eprintln!("over allocation bound: {label}: {measured:?}");
    }
    within_bound
}

#[rstest::rstest]
#[case::large_bodies(256 * 1024, 2)]
#[case::many_small_bodies(16, 64)]
#[allow(
    clippy::too_many_lines,
    reason = "keep the measured transition chain together"
)]
fn worker_identity_cached_transitions_do_not_copy_candidate_bodies(
    #[case] field_bytes: usize,
    #[case] count: u64,
) {
    let _guard = OBSERVATION.lock().unwrap();
    worker_heap::self_check();
    let mut within_bound = true;
    let session = true;
    let started = std::time::Instant::now();
    let mut allocated = 0;
    let mut peak = 0;
    for sequence in 1..=count {
        let (state, read) = waiting_for_first_read(1, session);
        let loaded = RunEvent::CandidateLoaded(CandidateLoaded {
            id: read.id,
            worker: read.worker,
            candidate: Some(candidate(sequence, field_bytes)),
        });
        let (mut state, mut effects) = {
            let (state, effects, measured) = observed(state, loaded);
            within_bound &= check_window("lookup dispatch", &measured);
            allocated += measured.allocated;
            peak = peak.max(measured.peak_delta);
            (state, effects)
        };
        {
            let RunEffect::LookupStoredResult(request) = effects.remove(0) else {
                panic!("lookup");
            };
            let event = RunEvent::StoredResultLoaded(hoimin_core::StoredResultLoaded {
                id: request.id,
                worker: request.worker,
                result: Some(hoimin_core::StoredResult {
                    mutant_id: request.mutant_id,
                    status: hoimin_core::MutationStatus::Killed,
                }),
            });
            let measured;
            (state, effects, measured) = observed(state, event);
            within_bound &= check_window("reuse identity validation and started output", &measured);
            allocated += measured.allocated;
            peak = peak.max(measured.peak_delta);
            let RunEffect::EmitOutput(output) = effects.remove(0) else {
                panic!("synthetic started");
            };
            let event = RunEvent::OutputEmitted(OutputEmitted { id: output.id });
            drop(output);
            let measured;
            (state, effects, measured) = observed(state, event);
            within_bound &=
                check_payload_window("synthetic finished output", &measured, 2 * field_bytes);
            allocated += measured.allocated;
            peak = peak.max(measured.peak_delta);
        }
        std::hint::black_box((&state, &effects));
    }
    eprintln!(
        "worker-heap field_bytes={field_bytes} candidates={count} session={session} allocated={allocated} peak={peak} elapsed={:?}",
        started.elapsed()
    );
    assert!(
        within_bound,
        "identity transitions allocated candidate-sized temporary bodies"
    );
}

#[rstest::rstest]
#[case::large_bodies(256 * 1024, 2)]
#[case::many_small_bodies(16, 64)]
#[allow(
    clippy::too_many_lines,
    reason = "keep the measured transition chain together"
)]
fn worker_identity_executed_transitions_do_not_copy_candidate_bodies(
    #[case] field_bytes: usize,
    #[case] count: u64,
) {
    let _guard = OBSERVATION.lock().unwrap();
    worker_heap::self_check();
    let mut within_bound = true;
    let session = false;
    let started = std::time::Instant::now();
    let mut allocated = 0;
    let mut peak = 0;
    for sequence in 1..=count {
        let (state, read) = waiting_for_first_read(1, session);
        let loaded = RunEvent::CandidateLoaded(CandidateLoaded {
            id: read.id,
            worker: read.worker,
            candidate: Some(candidate(sequence, field_bytes)),
        });
        let (mut state, mut effects) = { transition(state, loaded).unwrap() };
        {
            let RunEffect::ApplyMutation(request) = effects.remove(0) else {
                panic!("apply");
            };
            let event = RunEvent::MutationApplied(hoimin_core::MutationApplied {
                id: request.id,
                worker: request.worker,
            });
            drop(request);
            let measured;
            (state, effects, measured) = observed(state, event);
            within_bound &= check_window("started output", &measured);
            allocated += measured.allocated;
            peak = peak.max(measured.peak_delta);
            let RunEffect::EmitOutput(output) = effects.remove(0) else {
                panic!("started");
            };

            assert!(matches!(
                output.event,
                hoimin_core::OutputEvent::MutantStarted(_)
            ));
            let event = RunEvent::OutputEmitted(OutputEmitted { id: output.id });
            drop(output);
            let measured;
            (state, effects, measured) = observed(state, event);
            within_bound &= check_window("process dispatch", &measured);
            allocated += measured.allocated;
            peak = peak.max(measured.peak_delta);
            assert!(
                matches!(effects.as_slice(), [RunEffect::RunMutant(process)] if process.context.mutant_id == format!("{sequence:064x}"))
            );
            let RunEffect::RunMutant(process) = effects.remove(0) else {
                panic!("process");
            };
            let event = RunEvent::MutantFinished(ProcessFinished {
                id: process.id,
                worker: Some(process.context.worker),
                termination: ProcessTermination::Exit(1),
                output_state: ProcessOutputState::Complete,
                output: OutputSpoolRef {
                    token: "mutant".into(),
                    retained: 0,
                    observed: 0,
                },
                elapsed: Duration::from_millis(1),
                resource_mode: ResourceMode::Hard,
            });
            drop(process);
            let measured;
            (state, effects, measured) = observed(state, event);
            within_bound &= check_payload_window("finished result", &measured, 2 * field_bytes);
            allocated += measured.allocated;
            peak = peak.max(measured.peak_delta);
        }
        std::hint::black_box((&state, &effects));
    }
    eprintln!(
        "worker-heap field_bytes={field_bytes} candidates={count} session={session} allocated={allocated} peak={peak} elapsed={:?}",
        started.elapsed()
    );
    assert!(
        within_bound,
        "identity transitions allocated candidate-sized temporary bodies"
    );
}

#[rstest::rstest]
#[case(32 * 1024, 0, false)]
#[case(0, 32 * 1024, false)]
#[case(32 * 1024 - 1, 32 * 1024 - 1, true)]
fn heap_window_enforces_exclusive_allocation_and_peak_bounds(
    #[case] allocated: usize,
    #[case] peak_delta: usize,
    #[case] expected: bool,
) {
    let measured = worker_heap::Measurement {
        retained_delta: 0,
        peak_delta,
        allocated,
    };
    assert_eq!(check_window("boundary-probe", &measured), expected);
}
