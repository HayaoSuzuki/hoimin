//! Public worker protocol fixture and journal. Private machine fields are never accessed.
use hoimin_core::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

fn candidate(index: usize) -> MutationCandidate {
    MutationCandidate {
        id: format!("m{index}"),
        sequence: u64::try_from(index).unwrap(),
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
fn index(id: &str) -> usize {
    id.strip_prefix('m').unwrap().parse().unwrap()
}
fn process(id: EffectId, worker: u32, status: MutationStatus) -> ProcessFinished {
    let termination = match status {
        MutationStatus::Killed => ProcessTermination::Exit(1),
        MutationStatus::Survived => ProcessTermination::Exit(0),
        MutationStatus::Timeout => ProcessTermination::Timeout,
        other => panic!("unexpected fixture process status {other:?}"),
    };
    ProcessFinished {
        id,
        worker: Some(worker),
        termination,
        output_state: ProcessOutputState::Complete,
        output: OutputSpoolRef {
            token: "output".into(),
            retained: 0,
            observed: 0,
        },
        elapsed: Duration::from_millis(1),
        resource_mode: ResourceMode::Hard,
    }
}
fn config(session: bool, jobs: usize) -> RunConfig {
    RunConfig::try_from(RawRunConfig {
        root: ".".into(),
        sources: vec!["src".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        limits: RawRunLimits {
            jobs,
            ..RawRunLimits::default()
        },
        session: session.then(|| SessionConfig {
            path: "session.sqlite3".into(),
        }),
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(EnvironmentSnapshot::Untracked)
    .unwrap()
}
fn insert(
    pending: &mut BTreeMap<EffectId, RunEffect>,
    effects: Vec<RunEffect>,
    last_sequence: &mut u64,
) {
    for effect in effects {
        if let RunEffect::EmitOutput(output) = &effect {
            *last_sequence = output.event.sequence();
        }
        assert!(pending.insert(effect.id(), effect).is_none());
    }
}
/// Complete only setup work; leave one `ApplyMutation` pending per worker.
fn bootstrap(session: bool, jobs: usize) -> (RunState, BTreeMap<EffectId, RunEffect>, u64) {
    let state = RunState::new(
        "worker-run",
        config(session, jobs),
        ResourceControl {
            mode: ResourceMode::Hard,
            mechanism: "fixture".into(),
        },
    );
    let (mut state, effects) = transition(state, RunEvent::StartRequested(StartRequested)).unwrap();
    let mut pending = BTreeMap::new();
    let mut sequence = 0;
    let mut loaded = 0;
    insert(&mut pending, effects, &mut sequence);
    for _ in 0..128 {
        if pending.len() == jobs
            && pending
                .values()
                .all(|e| matches!(e, RunEffect::ApplyMutation(_)))
        {
            return (state, pending, sequence);
        }
        let id = *pending
            .iter()
            .find(|(_, e)| !matches!(e, RunEffect::ApplyMutation(_)))
            .expect("bootstrap progress")
            .0;
        let effect = pending.remove(&id).unwrap();
        let event = match effect {
            RunEffect::ResolveTargets(r) => RunEvent::TargetsResolved(TargetsResolved {
                id: r.id,
                targets: vec![TargetSlice {
                    path: "src/calc.py".into(),
                    lines: vec![],
                    symbols: vec![],
                }],
            }),
            RunEffect::Preflight(r) => RunEvent::PreflightCompleted(PreflightCompleted {
                id: r.id,
                per_worker_logical_bytes: 10,
                requested_workers: u32::try_from(jobs).unwrap(),
                aggregate_logical_bytes: 10 * u64::try_from(jobs).unwrap(),
                fingerprint: session.then_some(RunFingerprint::from_bytes([7; 32])),
            }),
            RunEffect::BeginSession(r) => RunEvent::SessionStarted(SessionStarted {
                id: r.id,
                run_id: r.run_id,
            }),
            RunEffect::EmitOutput(r) => RunEvent::OutputEmitted(OutputEmitted { id: r.id }),
            RunEffect::CreateWorker(r) => RunEvent::WorkerCreated(WorkerCreated {
                id: r.id(),
                worker: r.worker(),
                reservation_id: r.reservation_id(),
            }),
            RunEffect::VerifyOriginals(r) => RunEvent::OriginalsVerified(OriginalsVerified {
                id: r.id,
                checkpoint: r.checkpoint,
            }),
            RunEffect::RunBaseline(r) => RunEvent::BaselineFinished(process(
                r.id,
                r.context.worker,
                MutationStatus::Survived,
            )),
            RunEffect::ResetWorker(r) => RunEvent::WorkerReset(WorkerReset {
                id: r.id,
                worker: r.worker,
            }),
            RunEffect::AnalyzeFile(r) => RunEvent::AnalysisFinished(AnalysisFinished {
                id: r.id,
                spool: Some(CandidateSpoolRef {
                    token: "candidates".into(),
                    records: u64::try_from(jobs).unwrap(),
                }),
                truncated: false,
                diagnostics: vec![],
            }),
            RunEffect::ReadCandidate(r) => {
                let body = (loaded < jobs).then(|| candidate(loaded));
                loaded += 1;
                RunEvent::CandidateLoaded(CandidateLoaded {
                    id: r.id,
                    worker: r.worker,
                    candidate: body,
                })
            }
            RunEffect::LookupStoredResult(r) => RunEvent::StoredResultLoaded(StoredResultLoaded {
                id: r.id,
                worker: r.worker,
                result: None,
            }),
            effect => panic!("unexpected bootstrap effect {effect:?}"),
        };
        let (next, effects) = transition(state, event).unwrap();
        state = next;
        insert(&mut pending, effects, &mut sequence);
    }
    panic!("bootstrap did not reach parallel apply requests")
}
/// A candidate's stage is observed from its pending public request, not private worker state.
fn slot(effect: &RunEffect) -> Option<(usize, u8)> {
    match effect {
        RunEffect::ApplyMutation(r) => Some((index(&r.candidate.id), 0)),
        RunEffect::EmitOutput(r) => match &r.event {
            OutputEvent::MutantStarted(v) => Some((index(&v.mutant_id), 1)),
            OutputEvent::MutantFinished(v) => Some((index(&v.candidate.id), 4)),
            _ => None,
        },
        RunEffect::RunMutant(r) => Some((index(&r.context.mutant_id), 2)),
        RunEffect::PersistResult(r) => Some((index(&r.result.candidate.id), 3)),
        RunEffect::ResetWorker(r) => Some((usize::try_from(r.worker).unwrap(), 5)),
        RunEffect::ReadCandidate(r) => Some((usize::try_from(r.worker).unwrap(), 6)),
        _ => None,
    }
}
fn success(effect: &RunEffect, status: MutationStatus) -> RunEvent {
    match effect {
        RunEffect::ApplyMutation(r) => RunEvent::MutationApplied(MutationApplied {
            id: r.id,
            worker: r.worker,
        }),
        RunEffect::EmitOutput(r) => RunEvent::OutputEmitted(OutputEmitted { id: r.id }),
        RunEffect::RunMutant(r) => {
            RunEvent::MutantFinished(process(r.id, r.context.worker, status))
        }
        RunEffect::PersistResult(r) => RunEvent::ResultPersisted(ResultPersisted {
            id: r.id,
            worker: r.worker,
            run_id: r.result.run_id.clone(),
            mutant_id: r.result.candidate.id.clone(),
        }),
        RunEffect::ResetWorker(r) => RunEvent::WorkerReset(WorkerReset {
            id: r.id,
            worker: r.worker,
        }),
        RunEffect::ReadCandidate(r) => RunEvent::CandidateLoaded(CandidateLoaded {
            id: r.id,
            worker: r.worker,
            candidate: None,
        }),
        RunEffect::VerifyOriginals(r) => RunEvent::OriginalsVerified(OriginalsVerified {
            id: r.id,
            checkpoint: r.checkpoint,
        }),
        RunEffect::Cleanup(r) => RunEvent::CleanupFinished(CleanupFinished {
            id: r.id,
            released_reservations: r.reservations.clone(),
        }),
        RunEffect::FinishSession(r) => RunEvent::SessionFinished(SessionFinished {
            id: r.id,
            run_id: r.run_id.clone(),
            complete: r.complete,
        }),
        effect => panic!("unexpected active effect {effect:?}"),
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Ack(u8, bool),
    Timeout(u8),
    Global,
    Stop(u8, u64),
    Wrong(u8),
    Worker(u8),
    Identity(u8, bool),
    Retired,
    Duplicate,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Accepted,
    Wrong,
    Worker,
    Identity,
    Retired,
    Duplicate,
    Unknown,
    Skipped,
}
#[derive(Default)]
struct Causes {
    interrupted: bool,
    incomplete: bool,
    fatal: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReportStage {
    NotEmitted,
    Pending,
    Acknowledged,
}
/// The journal owns observed results, acknowledged output and effect retirement.
/// It derives counts from complete histories rather than incrementing production counters.
pub struct Driver {
    state: RunState,
    pending: BTreeMap<EffectId, RunEffect>,
    issued: BTreeSet<EffectId>,
    completed: BTreeMap<EffectId, RunEffect>,
    retired: BTreeMap<EffectId, RunEffect>,
    session: bool,
    causes: Causes,
    stopped: bool,
    drained: bool,
    results: Vec<Option<MutationStatus>>,
    persisted: Vec<bool>,
    started: Vec<bool>,
    finished: Vec<Option<MutationStatus>>,
    sequence: u64,
    final_report: ReportStage,
    first_disk: Option<u64>,
}
impl Driver {
    pub fn new(session: bool, jobs: usize) -> Self {
        assert!((1..=4).contains(&jobs));
        let (state, pending, sequence) = bootstrap(session, jobs);
        let issued = pending.keys().copied().collect();
        Self {
            state,
            pending,
            issued,
            completed: BTreeMap::new(),
            retired: BTreeMap::new(),
            session,
            causes: Causes::default(),
            stopped: false,
            drained: false,
            results: vec![None; jobs],
            persisted: vec![false; jobs],
            started: vec![false; jobs],
            finished: vec![None; jobs],
            sequence,
            final_report: ReportStage::NotEmitted,
            first_disk: None,
        }
    }
    fn mode(&self) -> u8 {
        for effect in self.pending.values() {
            let mode = match effect {
                RunEffect::VerifyOriginals(_) => Some(2),
                RunEffect::Cleanup(_) => Some(3),
                RunEffect::FinishSession(_) => Some(5),
                RunEffect::EmitOutput(r) => match &r.event {
                    OutputEvent::RunFinished(_) => Some(4),
                    OutputEvent::Diagnostic(_) => Some(7),
                    _ => None,
                },
                _ => None,
            };
            if let Some(mode) = mode {
                return mode;
            }
        }
        if self.pending.is_empty() {
            6
        } else if self.stopped
            && self.finished.iter().all(Option::is_some)
            && self
                .pending
                .values()
                .any(|e| matches!(e, RunEffect::ReadCandidate(_)))
        {
            8
        } else {
            u8::from(self.stopped)
        }
    }
    fn effect(&self, worker: u8) -> Option<&RunEffect> {
        self.pending
            .values()
            .find(|e| slot(e).is_some_and(|(i, _)| i == usize::from(worker)))
    }
    fn code(&self) -> i32 {
        if self.causes.interrupted {
            130
        } else if self.causes.fatal {
            2
        } else if self.causes.incomplete || self.expected().contains(&Some(MutationStatus::Timeout))
        {
            4
        } else {
            0
        }
    }
    fn expected(&self) -> Vec<Option<MutationStatus>> {
        (0..self.results.len())
            .map(|i| {
                self.finished[i].or_else(|| {
                    self.results[i].filter(|_| !self.session || self.persisted[i] || self.drained)
                })
            })
            .collect()
    }
    fn check(&self) {
        assert_eq!(self.state.pending_count(), self.pending.len());
        for id in &self.issued {
            assert_eq!(
                self.state.is_effect_pending(*id),
                self.pending.contains_key(id)
            );
            assert_eq!(
                self.state.is_effect_retired(*id),
                self.retired.contains_key(id)
            );
        }
        let expected = self.expected();
        let counts = |status| {
            u64::try_from(expected.iter().filter(|s| **s == Some(status)).count()).unwrap()
        };
        let actual = self.state.summary();
        assert_eq!(
            self.counts(),
            [
                MutationStatus::Killed,
                MutationStatus::Survived,
                MutationStatus::NotRun,
                MutationStatus::Timeout
            ]
            .map(counts),
            "summary from result/ack journal"
        );
        assert_eq!(
            (actual.out_of_memory, actual.process_limit, actual.error),
            (0, 0, 0)
        );
        assert_eq!(
            actual.inconclusive,
            counts(MutationStatus::NotRun) + counts(MutationStatus::Timeout)
        );
        assert_eq!(self.state.exit_code(), self.code());
        assert_eq!(
            self.state
                .disk_summary()
                .stop
                .as_ref()
                .and_then(|s| s.available_bytes),
            self.first_disk
        );
        if self.pending.is_empty() {
            assert_eq!(self.state.phase(), RunPhase::Finished);
        }
    }
    fn before_completion(&mut self, effect: &RunEffect, status: MutationStatus) {
        match effect {
            RunEffect::RunMutant(r) => {
                let i = index(&r.context.mutant_id);
                assert!(self.results[i].is_none());
                self.results[i] = Some(status);
            }
            RunEffect::PersistResult(r) => self.persisted[index(&r.result.candidate.id)] = true,
            RunEffect::EmitOutput(r) => match &r.event {
                OutputEvent::MutantStarted(v) => {
                    let i = index(&v.mutant_id);
                    assert!(!self.started[i], "start acknowledged once");
                    self.started[i] = true;
                }
                OutputEvent::MutantFinished(v) => {
                    let i = index(&v.candidate.id);
                    assert!(self.started[i]);
                    assert!(self.finished[i].is_none(), "finish acknowledged once");
                    if self.stopped {
                        assert_eq!(
                            self.finished.iter().position(Option::is_none),
                            Some(i),
                            "stopped drain follows candidate sequence"
                        );
                    }
                    self.finished[i] = Some(v.outcome.status());
                }
                OutputEvent::RunFinished(_) => {
                    assert_eq!(self.final_report, ReportStage::Pending);
                    self.final_report = ReportStage::Acknowledged;
                }
                _ => {}
            },
            _ => {}
        }
    }
    fn observe(&mut self, effect: &RunEffect) {
        assert!(effect.id().0 > self.issued.last().unwrap().0);
        assert!(self.issued.insert(effect.id()));
        if self.stopped {
            assert!(
                !matches!(
                    effect,
                    RunEffect::ApplyMutation(_)
                        | RunEffect::RunMutant(_)
                        | RunEffect::LookupStoredResult(_)
                ),
                "stop cannot launch real work"
            );
        }
        if self.stopped
            && (slot(effect).is_some() || matches!(effect, RunEffect::VerifyOriginals(_)))
        {
            self.drained = true;
        }
        if let RunEffect::EmitOutput(r) = effect {
            assert!(
                self.final_report == ReportStage::NotEmitted,
                "no output after final report"
            );
            assert_eq!(r.event.run_id(), self.state.run_id());
            assert_eq!(r.event.sequence(), self.sequence + 1);
            self.sequence = r.event.sequence();
            if let OutputEvent::MutantFinished(v) = &r.event {
                let i = index(&v.candidate.id);
                let expected = self.results[i].unwrap_or(MutationStatus::NotRun);
                assert_eq!(v.outcome.status(), expected, "result survives stop");
            }
            if let OutputEvent::RunFinished(v) = &r.event {
                assert!(
                    self.finished.iter().all(Option::is_some),
                    "all candidates acknowledged before final report"
                );
                assert_eq!(&v.counts, self.state.summary());
                assert_eq!(v.exit_code, self.code());
                assert_eq!(v.complete, self.code() == 0);
                self.final_report = ReportStage::Pending;
            }
        }
        if let RunEffect::PersistResult(r) = effect {
            let i = index(&r.result.candidate.id);
            assert_eq!(r.result.run_id, self.state.run_id());
            assert_eq!(r.worker, u32::try_from(i).unwrap());
            assert_eq!(
                Some(r.result.outcome.status()),
                self.results[i],
                "persist payload matches the completed process"
            );
        }
        if let RunEffect::FinishSession(r) = effect {
            assert!(
                self.session && self.final_report == ReportStage::Acknowledged,
                "session closes after final report ack"
            );
            assert_eq!(r.complete, self.code() == 0);
        }
    }
    fn commit(
        &mut self,
        event: RunEvent,
        completion: Option<(RunEffect, MutationStatus)>,
        is_stop: bool,
    ) {
        if let Some((effect, status)) = &completion {
            self.before_completion(effect, *status);
        }
        let (state, effects) = transition(self.state.clone(), event).unwrap();
        self.state = state;
        if is_stop && !effects.is_empty() {
            self.retired.extend(self.pending.clone());
            self.pending.clear();
        }
        if let Some((effect, _)) = completion {
            self.pending.remove(&effect.id());
            self.completed.insert(effect.id(), effect);
        }
        for effect in &effects {
            self.observe(effect);
            assert!(self.pending.insert(effect.id(), effect.clone()).is_none());
        }
        self.check();
    }
    fn probe(&self, event: RunEvent, expected: Verdict) -> Verdict {
        let error =
            transition(self.state.clone(), event).expect_err("invalid completion must be rejected");
        let actual = match error {
            MachineError::WrongCompletion { .. } => Verdict::Wrong,
            MachineError::UnknownWorker(_) | MachineError::ProcessWorkerMismatch { .. } => {
                Verdict::Worker
            }
            MachineError::SessionIdentityMismatch { .. } => Verdict::Identity,
            MachineError::RetiredEffect(_) => Verdict::Retired,
            MachineError::DuplicateEffect(_) => Verdict::Duplicate,
            MachineError::UnknownEffect(_) => Verdict::Unknown,
            error => panic!("unexpected {error:?}"),
        };
        assert_eq!(actual, expected);
        actual
    }
    fn invalid(&self, command: Command) -> Verdict {
        let (event, expected) = match command {
            Command::Wrong(w) => {
                let Some(e) = self.effect(w) else {
                    return Verdict::Skipped;
                };
                (
                    RunEvent::RemainingBudgetObserved(RemainingBudgetObserved {
                        id: e.id(),
                        remaining: Duration::ZERO,
                    }),
                    Verdict::Wrong,
                )
            }
            Command::Worker(w) => {
                let Some(e) = self.effect(w) else {
                    return Verdict::Skipped;
                };
                let mut event = success(e, MutationStatus::Killed);
                let wrong = if self.results.len() > 1 {
                    u32::try_from((usize::from(w) + 1) % self.results.len()).unwrap()
                } else {
                    u32::MAX
                };
                match &mut event {
                    RunEvent::MutationApplied(v) => v.worker = wrong,
                    RunEvent::MutantFinished(v) => v.worker = Some(wrong),
                    RunEvent::ResultPersisted(v) => v.worker = wrong,
                    RunEvent::WorkerReset(v) => v.worker = wrong,
                    RunEvent::CandidateLoaded(v) => v.worker = wrong,
                    _ => return Verdict::Skipped,
                }
                (event, Verdict::Worker)
            }
            Command::Identity(w, run) => {
                let Some(RunEffect::PersistResult(r)) = self.effect(w) else {
                    return Verdict::Skipped;
                };
                (
                    RunEvent::ResultPersisted(ResultPersisted {
                        id: r.id,
                        worker: r.worker,
                        run_id: if run {
                            "wrong".into()
                        } else {
                            r.result.run_id.clone()
                        },
                        mutant_id: if run {
                            r.result.candidate.id.clone()
                        } else {
                            "wrong".into()
                        },
                    }),
                    Verdict::Identity,
                )
            }
            Command::Retired => {
                let request = self
                    .retired
                    .iter()
                    .find(|(_, e)| {
                        matches!(e, RunEffect::RunMutant(_) | RunEffect::PersistResult(_))
                    })
                    .or_else(|| self.retired.first_key_value());
                let Some((_, effect)) = request else {
                    return Verdict::Skipped;
                };
                (success(effect, MutationStatus::Killed), Verdict::Retired)
            }
            Command::Duplicate => {
                let Some((_, effect)) = self.completed.first_key_value() else {
                    return Verdict::Skipped;
                };
                (success(effect, MutationStatus::Killed), Verdict::Duplicate)
            }
            Command::Unknown => (
                RunEvent::OutputEmitted(OutputEmitted {
                    id: EffectId(u64::MAX),
                }),
                Verdict::Unknown,
            ),
            _ => return Verdict::Skipped,
        };
        self.probe(event, expected)
    }
    fn commit_success(&mut self, effect: RunEffect, status: MutationStatus) {
        self.commit(success(&effect, status), Some((effect, status)), false);
    }

    fn ack(&mut self, worker: u8, status: MutationStatus) -> Verdict {
        let Some(effect) = self.effect(worker).cloned() else {
            return Verdict::Skipped;
        };
        self.commit_success(effect, status);
        Verdict::Accepted
    }

    pub fn apply(&mut self, command: Command) -> Verdict {
        match command {
            Command::Ack(w, survived) => {
                let status = if survived {
                    MutationStatus::Survived
                } else {
                    MutationStatus::Killed
                };
                return self.ack(w, status);
            }
            Command::Timeout(w) => return self.ack(w, MutationStatus::Timeout),
            Command::Global => {
                let Some(effect) = self
                    .pending
                    .values()
                    .find(|e| slot(e).is_none() || self.mode() == 8)
                    .cloned()
                else {
                    return Verdict::Skipped;
                };
                self.commit_success(effect, MutationStatus::Killed);
            }
            Command::Stop(c, available) => {
                let frozen = matches!(self.mode(), 3..=6);
                if !frozen {
                    self.stopped = true;
                    match c % 3 {
                        0 => self.causes.interrupted = true,
                        1 => self.causes.incomplete = true,
                        _ => {
                            self.causes.fatal = true;
                            self.causes.incomplete = true;
                            self.first_disk.get_or_insert(available);
                        }
                    }
                }
                let event = match c % 3 {
                    0 => RunEvent::CancellationRequested,
                    1 => RunEvent::DeadlineReached,
                    _ => RunEvent::DiskStopRequested(DiskStopRequested {
                        failure: DiskFailure {
                            reason: DiskFailureReason::FilesystemReserveReached(DiskObservation {
                                owned_bytes: 4096,
                                available_bytes: available,
                                measured_in: Duration::from_millis(1),
                            }),
                            secondary: vec![],
                        },
                    }),
                };
                self.commit(event, None, true);
            }
            other => return self.invalid(other),
        }
        Verdict::Accepted
    }
    pub fn drain(&mut self) {
        for _ in 0..(8 * self.results.len() + 16) {
            let Some(effect) = self.pending.values().next().cloned() else {
                break;
            };
            self.commit_success(effect, MutationStatus::Killed);
        }
        assert_eq!(
            self.state.phase(),
            RunPhase::Finished,
            "successful replies eventually finish"
        );
    }
    pub fn snapshot(&self) -> Snapshot {
        let mode = self.mode();
        Snapshot {
            mode,
            stages: if mode == 0 {
                (0..self.results.len())
                    .map(|i| {
                        self.effect(u8::try_from(i).unwrap())
                            .and_then(slot)
                            .map_or(7, |(_, k)| k)
                    })
                    .collect()
            } else {
                vec![]
            },
            drain: if mode == 1 {
                self.pending.values().next().and_then(slot)
            } else {
                None
            },
            counts: self.counts(),
            started: self.started.clone(),
            finished: self.finished.iter().map(Option::is_some).collect(),
        }
    }
    pub fn assert_model(&self, row: &Value) {
        assert_eq!(self.snapshot(), Snapshot::from_spec_record(&row["s"]));
    }
    pub fn counts(&self) -> [u64; 4] {
        let s = self.state.summary();
        [s.killed, s.survived, s.not_run, s.timeout]
    }
}
/// Observable correspondence shared by direct ITF replay and Quint Connect's state checker.
#[derive(Debug, PartialEq, serde::Deserialize)]
pub struct Snapshot {
    pub mode: u8,
    pub stages: Vec<u8>,
    pub drain: Option<(usize, u8)>,
    pub counts: [u64; 4],
    pub started: Vec<bool>,
    pub finished: Vec<bool>,
}
impl Snapshot {
    pub fn from_spec_record(s: &Value) -> Self {
        let number = |v: &Value| {
            v.as_u64().unwrap_or_else(|| {
                v.as_str()
                    .or_else(|| v["#bigint"].as_str())
                    .expect("model integer")
                    .parse()
                    .unwrap()
            })
        };
        let mode = u8::try_from(number(&s["mode"])).unwrap();
        let statuses = s["status"].as_array().unwrap();
        let counted = s["counted"].as_array().unwrap();
        let count = |wanted| {
            (0..statuses.len())
                .filter(|&i| number(&statuses[i]) == wanted)
                .map(|i| number(&counted[i]))
                .sum()
        };
        Self {
            mode,
            stages: if mode == 0 {
                s["stages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| u8::try_from(number(v)).unwrap())
                    .collect()
            } else {
                vec![]
            },
            drain: if mode == 1 {
                Some((
                    usize::try_from(number(&s["queue"][0])).unwrap(),
                    if s["drainStart"].as_bool().unwrap() {
                        1
                    } else {
                        4
                    },
                ))
            } else {
                None
            },
            counts: [1, 2, 3, 4].map(count),
            started: s["started"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_bool().unwrap())
                .collect(),
            finished: s["finished"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_bool().unwrap())
                .collect(),
        }
    }
}
pub fn check_history(session: bool, jobs: usize, commands: &[Command]) -> [u64; 4] {
    let mut driver = Driver::new(session, jobs);
    for &c in commands {
        driver.apply(c);
    }
    driver.drain();
    driver.counts()
}
/// Two header bytes, then 11-byte commands: tag, worker, outcome, u64 observation.
pub fn decode(data: &[u8]) -> (bool, usize, Vec<Command>) {
    let session = data.first().copied().unwrap_or(0) & 1 != 0;
    let jobs = usize::from(data.get(1).copied().unwrap_or(0) % 4) + 1;
    let commands = data
        .get(2..)
        .unwrap_or_default()
        .as_chunks::<11>()
        .0
        .iter()
        .take(128)
        .map(|b| {
            let w = b[1] % u8::try_from(jobs).unwrap();
            let survived = b[2] & 1 != 0;
            let value = u64::from_le_bytes(b[3..].try_into().unwrap());
            match b[0] % 11 {
                0 if b[2] & 3 == 2 => Command::Timeout(w),
                0 => Command::Ack(w, survived),
                1 => Command::Global,
                2..=4 => Command::Stop(b[0] - 2, value),
                5 => Command::Wrong(w),
                6 => Command::Worker(w),
                7 => Command::Identity(w, survived),
                8 => Command::Retired,
                9 => Command::Duplicate,
                _ => Command::Unknown,
            }
        })
        .collect();
    (session, jobs, commands)
}
