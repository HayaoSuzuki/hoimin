//! The same real core/reader workload on both revisions. Only protocol adapters
//! differ under --cfg baseline. No process, source analysis or report retention.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use hoimin_core::{
    AnalysisFinished, ByteSpan, CandidateLoaded, CandidateSpoolRef, CleanupFinished, CommandArg,
    EffectId, EnvironmentSnapshot, MutationApplied, MutationCandidate, MutationStatus,
    OriginalsVerified, OutputEmitted, OutputEvent, OutputSpoolRef, PreflightCompleted,
    ProcessFinished, ProcessOutputState, ProcessTermination, RawRunConfig, RawRunLimits,
    ReadCandidate, RemainingBudgetObserved, ResourceControl, ResourceMode, RunConfig, RunEffect,
    RunEvent, RunPhase, RunSelection, RunState, StartRequested, TargetSlice, TargetsResolved,
    VerificationSampling, VerificationSelection, VerificationTopPolicy, WorkerCreated, WorkerReset,
    transition,
};
use serde_json::{Value, json};

mod requested_heap;
// Build script records hashes of these exact production sources, inserting one
// record-read hook into store.rs. Sampling remains byte-for-byte unmodified.
include!(env!("HOIMIN_PROBE_MODULES"));

mod io_counts {
    use std::sync::atomic::{AtomicU64, Ordering};

    static RECORDS: AtomicU64 = AtomicU64::new(0);
    static BYTES: AtomicU64 = AtomicU64::new(0);

    pub fn record_read(bytes: usize) {
        if bytes != 0 {
            RECORDS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
        }
    }

    pub fn snapshot() -> (u64, u64) {
        (
            RECORDS.load(Ordering::Relaxed),
            BYTES.load(Ordering::Relaxed),
        )
    }
}

#[derive(Debug)]
struct Options {
    population: usize,
    selected: usize,
    jobs: u32,
    field_bytes: usize,
    stop: bool,
    sample: bool,
}

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

struct Fixture {
    _directory: tempfile::TempDir,
    spool: CandidateSpoolRef,
    ids: Vec<String>,
    expected_rows_digest: [u8; 32],
    spool_bytes: u64,
    selected_record_bytes: u64,
}

impl Fixture {
    fn new(options: &Options) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut store =
            store::CandidateStore::new_in(options.population as u64, directory.path()).unwrap();
        let sequences: Vec<_> = if options.sample {
            sampling::sample_indices(options.population, options.selected, 42)
                .into_iter()
                .map(|index| index as u64 + 1)
                .collect()
        } else {
            (1..=options.population as u64)
                .rev()
                .take(options.selected)
                .collect()
        };
        let membership: std::collections::BTreeSet<_> = sequences.iter().copied().collect();
        let mut expected_rows_digest = [0; 32];
        let mut selected_record_bytes = 0_u64;
        for sequence in 1..=options.population as u64 {
            // Only this one fixture body exists at a time; it is gone before
            // measuring any candidate replay or core ownership.
            let row = candidate(sequence, options.field_bytes);
            if membership.contains(&sequence) {
                selected_record_bytes += serde_json::to_vec(&row).unwrap().len() as u64 + 1;
                xor_digest(&mut expected_rows_digest, &row_digest(&row));
            }
            store.push(&row).unwrap();
        }
        let spool = store.finish().unwrap();
        let spool_bytes = std::fs::metadata(&spool.token).unwrap().len();
        Self {
            _directory: directory,
            spool,
            ids: sequences
                .iter()
                .map(|sequence| format!("{sequence:064x}"))
                .collect(),
            expected_rows_digest,
            spool_bytes,
            selected_record_bytes,
        }
    }
}

fn row_digest(row: &MutationCandidate) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    // Serialize every candidate field directly into the digest, never a Vec.
    struct HashWriter<'a>(&'a mut blake3::Hasher);
    impl std::io::Write for HashWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(HashWriter(&mut hash), row).unwrap();
    *hash.finalize().as_bytes()
}

fn xor_digest(total: &mut [u8; 32], row: &[u8; 32]) {
    for (total, row) in total.iter_mut().zip(row) {
        *total ^= row;
    }
}

struct ReplayAdapter {
    #[cfg(not(baseline))]
    replay: store::CandidateReplay,
    sequential_records: u64,
    indexed_records: u64,
    eof_reads: u64,
}

impl ReplayAdapter {
    fn new(_ids: &[String]) -> Self {
        Self {
            #[cfg(not(baseline))]
            replay: store::CandidateReplay::new(_ids.iter().cloned()),
            sequential_records: 0,
            indexed_records: 0,
            eof_reads: 0,
        }
    }

    fn read(&mut self, request: ReadCandidate) -> RunEvent {
        #[cfg(baseline)]
        let (candidate, next_cursor, sequential) = {
            let record = store::CandidateStore::replay_one(&request.spool, request.cursor).unwrap();
            let (row, cursor) = match record {
                Some((row, cursor)) => (Some(row), cursor),
                None => (None, request.cursor),
            };
            (row, cursor, true)
        };
        #[cfg(not(baseline))]
        let (candidate, sequential) = (
            self.replay.read(&request.spool, &request.target).unwrap(),
            matches!(request.target, hoimin_core::CandidateReadTarget::Next),
        );
        if candidate.is_some() {
            if sequential {
                self.sequential_records += 1;
            } else {
                self.indexed_records += 1;
            }
        } else {
            self.eof_reads += 1;
        }
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: request.id,
            worker: request.worker,
            candidate,
            #[cfg(baseline)]
            next_cursor,
        })
    }
}

struct Driver<'a> {
    state: Option<RunState>,
    pending: VecDeque<RunEffect>,
    replay: ReplayAdapter,
    fixture: &'a Fixture,
    options: &'a Options,
    started: usize,
    finished: usize,
    killed: usize,
    not_run: usize,
    run_finished: bool,
    rows_digest: [u8; 32],
    started_digest: blake3::Hasher,
}

impl<'a> Driver<'a> {
    fn new(options: &'a Options, fixture: &'a Fixture) -> Self {
        let config = RunConfig::try_from(RawRunConfig {
            root: ".".into(),
            sources: vec!["src".into()],
            test_argv: vec![CommandArg::Unix(b"never-executed".to_vec())],
            limits: RawRunLimits {
                jobs: options.jobs as usize,
                max_candidates: options.population,
                max_mutants: options.selected,
                ..RawRunLimits::default()
            },
            ..RawRunConfig::default()
        })
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap();
        let metadata = if options.sample {
            VerificationSelection::sample(
                options.selected,
                VerificationSampling::new(options.population, 42, fixture.ids.clone()).unwrap(),
            )
            .unwrap()
        } else {
            VerificationSelection::top(
                options.selected,
                options.selected,
                VerificationTopPolicy::Strict,
                false,
            )
            .unwrap()
        };
        let selection = RunSelection::ordered(fixture.ids.clone(), metadata).unwrap();
        let state = RunState::with_selection(
            "ordered-probe",
            config,
            selection,
            ResourceControl {
                mode: ResourceMode::Hard,
                mechanism: "fixture".into(),
            },
        );
        let mut driver = Self {
            state: Some(state),
            pending: VecDeque::new(),
            replay: ReplayAdapter::new(&fixture.ids),
            fixture,
            options,
            started: 0,
            finished: 0,
            killed: 0,
            not_run: 0,
            run_finished: false,
            rows_digest: [0; 32],
            started_digest: blake3::Hasher::new(),
        };
        driver.accept(RunEvent::StartRequested(StartRequested));
        while !matches!(driver.pending.front(), Some(RunEffect::ReadCandidate(_))) {
            driver.step();
        }
        assert_eq!(driver.pending.len(), 1);
        driver
    }

    fn accept(&mut self, event: RunEvent) {
        let (state, effects) = transition(self.state.take().unwrap(), event).unwrap();
        self.state = Some(state);
        self.pending.extend(effects);
    }

    fn step(&mut self) {
        let effect = self.pending.pop_front().expect("run still has work");
        self.complete(effect);
    }

    fn complete(&mut self, effect: RunEffect) {
        let event = match effect {
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
                requested_workers: self.options.jobs,
                aggregate_logical_bytes: 10 * u64::from(self.options.jobs),
                fingerprint: None,
            }),
            RunEffect::CreateWorker(request) => RunEvent::WorkerCreated(WorkerCreated {
                id: request.id(),
                worker: request.worker(),
                reservation_id: request.reservation_id(),
            }),
            RunEffect::VerifyOriginals(request) => RunEvent::OriginalsVerified(OriginalsVerified {
                id: request.id,
                checkpoint: request.checkpoint,
            }),
            RunEffect::RunBaseline(request) => {
                RunEvent::BaselineFinished(process_finished(request.id, request.context.worker, 0))
            }
            RunEffect::AnalyzeFile(request) => RunEvent::AnalysisFinished(AnalysisFinished {
                id: request.id,
                spool: Some(self.fixture.spool.clone()),
                truncated: false,
                diagnostics: Vec::new(),
            }),
            RunEffect::ReadCandidate(request) => self.replay.read(request),
            RunEffect::ApplyMutation(request) => RunEvent::MutationApplied(MutationApplied {
                id: request.id,
                worker: request.worker,
            }),
            RunEffect::RunMutant(request) => {
                RunEvent::MutantFinished(process_finished(request.id, request.context.worker, 1))
            }
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
            RunEffect::Cleanup(request) => RunEvent::CleanupFinished(CleanupFinished {
                id: request.id,
                released_reservations: request.reservations,
            }),
            RunEffect::EmitOutput(request) => {
                match &request.event {
                    OutputEvent::MutantStarted(row) => {
                        assert_eq!(row.mutant_id, self.fixture.ids[self.started]);
                        self.started_digest.update(row.mutant_id.as_bytes());
                        self.started += 1;
                    }
                    OutputEvent::MutantFinished(row) => {
                        xor_digest(&mut self.rows_digest, &row_digest(&row.candidate));
                        self.finished += 1;
                        match row.outcome.status() {
                            MutationStatus::Killed => self.killed += 1,
                            MutationStatus::NotRun => self.not_run += 1,
                            status => panic!("unexpected status: {status:?}"),
                        }
                    }
                    OutputEvent::RunFinished(_) => self.run_finished = true,
                    _ => {}
                }
                RunEvent::OutputEmitted(OutputEmitted { id: request.id })
            }
            unexpected => panic!("unexpected fixture effect: {unexpected:?}"),
        };
        self.accept(event);
    }

    fn saturate(&mut self) {
        // Only body reads complete here. Applying workers and returned effects
        // continue owning their payloads until the live saturation snapshot.
        while let Some(index) = self
            .pending
            .iter()
            .position(|effect| matches!(effect, RunEffect::ReadCandidate(_)))
        {
            let effect = self.pending.remove(index).unwrap();
            self.complete(effect);
        }
        assert_eq!(self.pending.len(), self.options.jobs as usize);
        assert!(
            self.pending
                .iter()
                .all(|effect| matches!(effect, RunEffect::ApplyMutation(_)))
        );
    }

    fn stop(&mut self) {
        // These apply effects have not been run. Cancellation retires them;
        // dropping the unexecuted requests mirrors cancellation before polling.
        let old_effects = std::mem::take(&mut self.pending);
        self.accept(RunEvent::CancellationRequested);
        for effect in &old_effects {
            assert!(self.state.as_ref().unwrap().is_effect_retired(effect.id()));
        }
        drop(old_effects);
    }
}

fn process_finished(id: EffectId, worker: u32, exit: i32) -> ProcessFinished {
    ProcessFinished {
        id,
        worker: Some(worker),
        termination: ProcessTermination::Exit(exit),
        output_state: ProcessOutputState::Complete,
        output: OutputSpoolRef {
            token: "fixture-output".into(),
            retained: 0,
            observed: 0,
        },
        elapsed: Duration::from_millis(1),
        resource_mode: ResourceMode::Hard,
    }
}

#[derive(Clone, Copy)]
struct Snapshot {
    heap: requested_heap::Measurement,
    elapsed_ns: u128,
    records: u64,
    bytes: u64,
}

fn snapshot(baseline: &requested_heap::Baseline, start: Instant) -> Snapshot {
    let heap = requested_heap::snapshot(baseline);
    let (records, bytes) = io_counts::snapshot();
    Snapshot {
        heap,
        elapsed_ns: start.elapsed().as_nanos(),
        records,
        bytes,
    }
}

fn snapshot_json(value: Snapshot) -> Value {
    json!({
        "retained_bytes": value.heap.retained_delta,
        "peak_bytes": value.heap.peak_delta,
        "cumulative_requested_bytes": value.heap.cumulative_requested_bytes,
        "allocation_calls": value.heap.allocation_calls,
        "live_before": value.heap.live_before,
        "live_after": value.heap.live_after,
        "elapsed_ns": value.elapsed_ns,
        "records_read": value.records,
        "record_bytes_read": value.bytes,
    })
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        7,
        "probe N S JOBS FIELD_BYTES run|stop top|sample"
    );
    assert!(matches!(args[5].as_str(), "run" | "stop"));
    assert!(matches!(args[6].as_str(), "top" | "sample"));
    let options = Options {
        population: args[1].parse().unwrap(),
        selected: args[2].parse().unwrap(),
        jobs: args[3].parse().unwrap(),
        field_bytes: args[4].parse().unwrap(),
        stop: args[5] == "stop",
        sample: args[6] == "sample",
    };
    assert!(options.jobs > 0 && options.jobs as usize <= options.selected);
    assert!(options.selected <= options.population);
    requested_heap::self_check();
    let fixture = Fixture::new(&options);

    // Includes actual core selection/worker setup and I/O index construction.
    // Excludes spool writing, fixture payloads, and expected IDs/digests.
    let baseline = requested_heap::begin();
    let start = Instant::now();
    let mut driver = Driver::new(&options, &fixture);
    let setup = snapshot(&baseline, start);
    while driver.replay.sequential_records < options.population as u64 {
        driver.step();
    }
    let scan = snapshot(&baseline, start);
    assert_eq!(scan.records, options.population as u64);
    assert_eq!(scan.bytes, fixture.spool_bytes);
    driver.saturate(); // Includes the one EOF completion and indexed reads.
    assert_eq!(driver.replay.eof_reads, 1);
    let saturation = snapshot(&baseline, start);
    let stopped = if options.stop {
        driver.stop();
        Some(snapshot(&baseline, start))
    } else {
        None
    };
    while !driver.pending.is_empty() {
        driver.step();
    }
    let drained = snapshot(&baseline, start);
    let total_heap = requested_heap::finish(baseline);

    assert!(driver.run_finished);
    assert_eq!(driver.state.as_ref().unwrap().phase(), RunPhase::Finished);
    assert_eq!(driver.started, options.selected);
    assert_eq!(driver.finished, options.selected);
    assert_eq!(driver.rows_digest, fixture.expected_rows_digest);
    assert_eq!(
        driver.killed,
        if options.stop { 0 } else { options.selected }
    );
    assert_eq!(
        driver.not_run,
        if options.stop { options.selected } else { 0 }
    );
    let expected_fetches = if cfg!(baseline) {
        0
    } else {
        options.selected as u64
    };
    assert_eq!(driver.replay.indexed_records, expected_fetches);
    assert_eq!(
        drained.records,
        options.population as u64 + expected_fetches
    );
    assert_eq!(
        drained.bytes,
        fixture.spool_bytes
            + if cfg!(baseline) {
                0
            } else {
                fixture.selected_record_bytes
            }
    );
    let result = json!({
        "revision": if cfg!(baseline) { "baseline" } else { "changed" },
        "population": options.population, "selected": options.selected, "jobs": options.jobs,
        "field_bytes": options.field_bytes, "mode": args[5], "selection": args[6],
        "spool_bytes": fixture.spool_bytes, "selected_record_bytes": fixture.selected_record_bytes,
        "selected_body_bytes": options.selected * 2 * options.field_bytes,
        "setup": snapshot_json(setup), "scan": snapshot_json(scan),
        "saturation": snapshot_json(saturation), "stopped": stopped.map(snapshot_json),
        "drained": snapshot_json(drained), "total_peak_bytes": total_heap.peak_delta,
        "sequential_records": driver.replay.sequential_records,
        "indexed_records": driver.replay.indexed_records, "eof_reads": driver.replay.eof_reads,
        "started": driver.started, "finished": driver.finished,
        "killed": driver.killed, "not_run": driver.not_run,
        "started_digest": driver.started_digest.finalize().to_hex().to_string(),
        "rows_digest": blake3::Hash::from(driver.rows_digest).to_hex().to_string(),
    });
    println!("{result}");
}
