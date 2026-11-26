//! Public bootstrap/stop driver. Protocol laws use emitted effects and an event journal;
//! no private `RunState` fields or transition helpers are used.
use hoimin_core::{
    CleanupFinished, CommandArg, DiskFailure, DiskFailureReason, DiskObservation, DiskSecondary,
    DiskStopReport, DiskStopRequested, EffectFailed, EffectFailure, EffectId, IntegrityCheckpoint,
    MachineError, OriginalsVerified, OutputEmitted, OutputEvent, PreflightCompleted, RawRunConfig,
    RemainingBudgetObserved, ReservationId, ResourceControl, ResourceMode, RunConfig, RunEffect,
    RunEvent, RunFingerprint, RunPhase, RunState, SessionConfig, SessionFinished, SessionStarted,
    StartRequested, TargetSlice, TargetsResolved, transition,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub enum Entry {
    Targets,
    Preflight,
    Report,
    Session,
}
impl Entry {
    pub fn decode(value: u8) -> Self {
        match value % 4 {
            0 => Self::Targets,
            1 => Self::Preflight,
            2 => Self::Report,
            _ => Self::Session,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Stop(u8, u64),
    DiskMeasurementFailed,
    Ack,
    Fail(bool),
    Retired,
    Duplicate,
    Unknown,
    Wrong,
    Reservation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Accepted,
    Retired,
    Duplicate,
    Unknown,
    Wrong,
    Reservation,
    Skipped,
}
fn kind(effect: &RunEffect) -> u8 {
    match effect {
        RunEffect::ResolveTargets(_) | RunEffect::Preflight(_) => 0,
        RunEffect::EmitOutput(output) => match &output.event {
            OutputEvent::RunStarted(_) => 1,
            OutputEvent::Diagnostic(d)
                if matches!(
                    d.code.as_str(),
                    "filesystem.reserve.reached" | "disk.measurement.failed"
                ) =>
            {
                2
            }
            OutputEvent::Diagnostic(_) => 5,
            OutputEvent::RunFinished(_) => 6,
            event => panic!("unexpected output {event:?}"),
        },
        RunEffect::VerifyOriginals(_) => 3,
        RunEffect::Cleanup(_) => 4,
        RunEffect::FinishSession(_) => 7,
        effect => panic!("stopped bootstrap dispatched work {effect:?}"),
    }
}
fn output_kind(value: u8) -> bool {
    matches!(value, 1 | 2 | 5 | 6)
}
fn config(session: bool) -> RunConfig {
    RunConfig::try_from(RawRunConfig {
        root: ".".into(),
        sources: vec!["src".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        session: session.then(|| SessionConfig {
            path: "session.sqlite3".into(),
        }),
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
    .unwrap()
}
fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect]: [RunEffect; 1] = effects.try_into().expect("one bootstrap effect");
    effect
}
/// Reach the entry only through the shipping constructor and public transition API.
fn bootstrap(entry: Entry) -> (RunState, RunEffect) {
    let state = RunState::new(
        "stop-run",
        config(matches!(entry, Entry::Session)),
        ResourceControl {
            mode: ResourceMode::Hard,
            mechanism: "fixture".into(),
        },
    );
    let (state, effects) = transition(state, RunEvent::StartRequested(StartRequested)).unwrap();
    let effect = one(effects);
    if matches!(entry, Entry::Targets) {
        return (state, effect);
    }
    let (state, effects) = transition(
        state,
        RunEvent::TargetsResolved(TargetsResolved {
            id: effect.id(),
            targets: vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: vec![],
                symbols: vec![],
            }],
        }),
    )
    .unwrap();
    let effect = one(effects);
    if matches!(entry, Entry::Preflight) {
        return (state, effect);
    }
    let (state, effects) = transition(
        state,
        RunEvent::PreflightCompleted(PreflightCompleted {
            id: effect.id(),
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: matches!(entry, Entry::Session)
                .then_some(RunFingerprint::from_bytes([7; 32])),
        }),
    )
    .unwrap();
    let effect = one(effects);
    if let RunEffect::BeginSession(request) = effect {
        let (state, effects) = transition(
            state,
            RunEvent::SessionStarted(SessionStarted {
                id: request.id,
                run_id: request.run_id,
            }),
        )
        .unwrap();
        (state, one(effects))
    } else {
        (state, effect)
    }
}
fn stop(cause: u8, available: u64) -> RunEvent {
    match cause % 3 {
        0 => RunEvent::CancellationRequested,
        1 => RunEvent::DeadlineReached,
        _ => RunEvent::DiskStopRequested(DiskStopRequested {
            failure: DiskFailure {
                reason: DiskFailureReason::FilesystemReserveReached(DiskObservation {
                    owned_bytes: 4096,
                    available_bytes: available,
                    measured_in: Duration::from_millis(7),
                }),
                secondary: vec![],
            },
        }),
    }
}
fn completion(effect: &RunEffect) -> RunEvent {
    match effect {
        RunEffect::EmitOutput(request) => RunEvent::OutputEmitted(OutputEmitted { id: request.id }),
        RunEffect::VerifyOriginals(request) => RunEvent::OriginalsVerified(OriginalsVerified {
            id: request.id,
            checkpoint: IntegrityCheckpoint::PreFinalReport,
        }),
        RunEffect::Cleanup(request) => RunEvent::CleanupFinished(CleanupFinished {
            id: request.id,
            released_reservations: request.reservations.clone(),
        }),
        RunEffect::FinishSession(request) => RunEvent::SessionFinished(SessionFinished {
            id: request.id,
            run_id: request.run_id.clone(),
            complete: request.complete,
        }),
        _ => panic!("cannot complete preparation after stop"),
    }
}
fn integer(value: &Value) -> u64 {
    value["#bigint"].as_str().unwrap().parse().unwrap()
}

#[derive(Default)]
struct Causes {
    interrupted: bool,
    fatal: bool,
    incomplete: bool,
}
/// Hides request construction, public observations and history laws from PBT/fuzz.
pub struct Driver {
    state: RunState,
    pending: Option<RunEffect>,
    initial: EffectId,
    issued: Vec<EffectId>,
    completed: Vec<EffectId>,
    retired: BTreeSet<EffectId>,
    session: bool,
    stopped: bool,
    causes: Causes,
    first_disk: Option<DiskStopReport>,
    outputs: u64,
    last_sequence: Option<u64>,
    finish_code: Option<i32>,
    report_ack: bool,
    session_finishes: usize,
    finished_cleanup: Option<EffectId>,
}
impl Driver {
    pub fn new(entry: Entry) -> Self {
        let (state, effect) = bootstrap(entry);
        let started = kind(&effect) == 1;
        let initial = effect.id();
        Self {
            state,
            pending: Some(effect),
            initial,
            issued: vec![initial],
            completed: vec![],
            retired: BTreeSet::new(),
            session: matches!(entry, Entry::Session),
            stopped: false,
            causes: Causes::default(),
            first_disk: None,
            outputs: u64::from(started),
            last_sequence: started.then_some(1),
            finish_code: None,
            report_ack: false,
            session_finishes: 0,
            finished_cleanup: None,
        }
    }
    fn pending_kind(&self) -> u8 {
        self.pending.as_ref().map_or(8, kind)
    }
    fn expected_code(&self) -> i32 {
        if self.causes.interrupted {
            130
        } else if self.causes.fatal {
            2
        } else if self.causes.incomplete {
            4
        } else {
            0
        }
    }
    fn observe(&mut self, effect: &RunEffect) {
        assert!(
            effect.id().0 > self.issued.last().unwrap().0,
            "effect IDs increase"
        );
        self.issued.push(effect.id());
        assert!(
            self.state.is_effect_pending(effect.id()),
            "emitted effect is registered"
        );
        let k = kind(effect);
        assert_ne!(k, 0, "stopped preparation cannot restart");
        assert!(
            k != 4 || self.finished_cleanup.is_none(),
            "cleanup cannot be reissued"
        );
        if let RunEffect::EmitOutput(request) = effect {
            assert!(
                self.finish_code.is_none(),
                "final report cannot be followed by further output"
            );
            if k == 2 {
                let OutputEvent::Diagnostic(diagnostic) = &request.event else {
                    unreachable!("disk diagnostic classified as output");
                };
                let first = self.first_disk.as_ref().unwrap();
                let message = first.message.clone().unwrap_or_else(|| {
                    format!(
                        "disk policy stopped execution; owned bytes: {}; available bytes: {}",
                        first.owned_bytes.unwrap(),
                        first.available_bytes.unwrap(),
                    )
                });
                assert_eq!(diagnostic.code, first.code);
                assert_eq!(diagnostic.message, message);
                assert_eq!(diagnostic.level, "error");
            }
            let sequence = match &request.event {
                OutputEvent::RunStarted(value) => value.sequence,
                OutputEvent::Diagnostic(value) => value.sequence,
                OutputEvent::RunFinished(value) => {
                    assert!(self.finish_code.is_none(), "at most one final report");
                    assert!(!value.complete, "stopped run is incomplete");
                    assert_eq!(value.exit_code, self.expected_code());
                    assert!(
                        !self.state.disk_summary().cleanup.is_empty(),
                        "cleanup precedes final report"
                    );
                    assert_eq!(&value.disk, self.state.disk_summary());
                    self.finish_code = Some(value.exit_code);
                    value.sequence
                }
                _ => unreachable!(),
            };
            assert_eq!(sequence, self.last_sequence.map_or(1, |s| s + 1));
            self.last_sequence = Some(sequence);
            self.outputs += 1;
        }
        if let RunEffect::FinishSession(request) = effect {
            assert!(
                self.session && self.report_ack,
                "session finish follows final-report acknowledgement"
            );
            assert!(!request.complete);
            assert_eq!(request.run_id, self.state.run_id());
            self.session_finishes += 1;
            assert_eq!(self.session_finishes, 1);
        }
    }
    fn check(&self) {
        assert_eq!(
            self.state.exit_code(),
            self.expected_code(),
            "exit policy from accepted causes"
        );
        assert_eq!(
            self.state.pending_count(),
            usize::from(self.pending.is_some())
        );
        for id in &self.issued {
            assert_eq!(
                self.state.is_effect_pending(*id),
                self.pending.as_ref().is_some_and(|p| p.id() == *id)
            );
            assert_eq!(self.state.is_effect_retired(*id), self.retired.contains(id));
        }
        if self.stopped {
            assert!(
                self.state.is_effect_retired(self.initial),
                "initial work is retired"
            );
        }
        let disk = self.state.disk_summary();
        assert_eq!(
            disk.cleanup.len(),
            usize::from(self.finished_cleanup.is_some())
        );
        assert_eq!(
            disk.stop, self.first_disk,
            "first disk reason and later diagnostics match the event journal"
        );
        if self.pending.is_none() {
            assert_eq!(self.state.phase(), RunPhase::Finished);
        }
    }
    fn stops_are_frozen(&self) -> bool {
        matches!(self.pending_kind(), 4 | 6 | 7 | 8) || self.finished_cleanup.is_some()
    }
    fn record_stop(&mut self, event: RunEvent) -> RunEvent {
        if self.stops_are_frozen() {
            return event;
        }
        self.stopped = true;
        match &event {
            RunEvent::CancellationRequested => self.causes.interrupted = true,
            RunEvent::DeadlineReached => self.causes.incomplete = true,
            RunEvent::DiskStopRequested(request) => {
                self.causes.fatal = true;
                self.causes.incomplete = true;
                self.first_disk
                    .get_or_insert_with(|| match &request.failure.reason {
                        DiskFailureReason::FilesystemReserveReached(observation) => {
                            DiskStopReport {
                                code: "filesystem.reserve.reached".into(),
                                owned_bytes: Some(observation.owned_bytes),
                                available_bytes: Some(observation.available_bytes),
                                message: None,
                                secondary: vec![],
                            }
                        }
                        DiskFailureReason::MeasurementFailed(message) => DiskStopReport {
                            code: "disk.measurement.failed".into(),
                            owned_bytes: None,
                            available_bytes: None,
                            message: Some(message.clone()),
                            secondary: vec![],
                        },
                        other => panic!("unexpected fixture disk reason {other:?}"),
                    });
            }
            other => panic!("unexpected fixture stop {other:?}"),
        }
        event
    }

    fn event(&mut self, command: Command) -> Option<(RunEvent, Option<Verdict>, bool)> {
        let k = self.pending_kind();
        let mut completing = false;
        let mut expected = None;
        let event = match command {
            Command::Stop(cause, available) => self.record_stop(stop(cause, available)),
            Command::DiskMeasurementFailed => {
                self.record_stop(RunEvent::DiskStopRequested(DiskStopRequested {
                    failure: DiskFailure {
                        reason: DiskFailureReason::MeasurementFailed("meter failed".into()),
                        secondary: vec![],
                    },
                }))
            }
            Command::Ack if k != 8 => {
                completing = true;
                if k == 6 {
                    self.report_ack = true;
                }
                completion(self.pending.as_ref().unwrap())
            }
            Command::Fail(report) if k != 8 && (!report || output_kind(k)) => {
                completing = true;
                self.causes.fatal = true;
                if let Some(stop) = &mut self.first_disk {
                    let (code, message) = if report {
                        ("report.io", "write: fixture")
                    } else {
                        ("fixture.failed", "fixture")
                    };
                    let secondary = DiskSecondary::Error {
                        code: code.into(),
                        message: message.into(),
                    };
                    if !stop.secondary.contains(&secondary) {
                        stop.secondary.push(secondary);
                    }
                }
                RunEvent::EffectFailed(EffectFailed {
                    id: self.pending.as_ref().unwrap().id(),
                    failure: if report {
                        EffectFailure::ReportIo {
                            operation: "write".into(),
                            message: "fixture".into(),
                        }
                    } else {
                        EffectFailure::Other {
                            code: "fixture.failed".into(),
                            message: "fixture".into(),
                        }
                    },
                })
            }
            Command::Retired if self.stopped => {
                expected = Some(Verdict::Retired);
                RunEvent::OutputEmitted(OutputEmitted { id: self.initial })
            }
            Command::Duplicate if !self.completed.is_empty() => {
                expected = Some(Verdict::Duplicate);
                RunEvent::OutputEmitted(OutputEmitted {
                    id: *self.completed.last().unwrap(),
                })
            }
            Command::Unknown => {
                expected = Some(Verdict::Unknown);
                RunEvent::OutputEmitted(OutputEmitted {
                    id: EffectId(u64::MAX),
                })
            }
            Command::Wrong if k != 8 => {
                expected = Some(Verdict::Wrong);
                RunEvent::RemainingBudgetObserved(RemainingBudgetObserved {
                    id: self.pending.as_ref().unwrap().id(),
                    remaining: Duration::ZERO,
                })
            }
            Command::Reservation if k == 4 => {
                expected = Some(Verdict::Reservation);
                RunEvent::CleanupFinished(CleanupFinished {
                    id: self.pending.as_ref().unwrap().id(),
                    released_reservations: vec![ReservationId(u64::MAX)],
                })
            }
            _ => return None,
        };
        Some((event, expected, completing))
    }
    pub fn apply(&mut self, command: Command) -> Verdict {
        let k = self.pending_kind();
        let frozen = self.stops_are_frozen();
        let previous = self.pending.clone();
        let Some((event, expected, completing)) = self.event(command) else {
            return Verdict::Skipped;
        };
        let before = (
            self.state.phase(),
            self.state.exit_code(),
            self.state.disk_summary().clone(),
        );
        // Errors consume the cloned input. The retained state is replay policy, not an error-state observation.
        let (state, effects) = match transition(self.state.clone(), event) {
            Ok(outcome) => {
                assert!(expected.is_none(), "expected rejected {command:?}");
                outcome
            }
            Err(error) => {
                let verdict = match error {
                    MachineError::RetiredEffect(_) => Verdict::Retired,
                    MachineError::DuplicateEffect(_) => Verdict::Duplicate,
                    MachineError::UnknownEffect(_) => Verdict::Unknown,
                    MachineError::WrongCompletion { .. } => Verdict::Wrong,
                    MachineError::CleanupReservationMismatch { .. } => Verdict::Reservation,
                    error => panic!("unexpected {error:?} for {command:?}"),
                };
                assert_eq!(Some(verdict), expected, "{command:?}");
                return verdict;
            }
        };
        assert!(effects.len() <= 1, "one bootstrap continuation");
        self.state = state;
        if completing {
            let id = previous.as_ref().unwrap().id();
            self.completed.push(id);
            if k == 4 {
                self.finished_cleanup = Some(id);
            }
        }
        self.pending = effects.first().cloned().or_else(|| {
            previous
                .as_ref()
                .filter(|p| self.state.is_effect_pending(p.id()))
                .cloned()
        });
        if let Some(old) = previous
            && !completing
            && self.pending.as_ref().is_none_or(|p| p.id() != old.id())
        {
            self.retired.insert(old.id());
        }
        if matches!(command, Command::Stop(..) | Command::DiskMeasurementFailed) && frozen {
            assert_eq!(effects, Vec::new(), "terminal stop cannot emit effects");
            assert_eq!(
                (
                    self.state.phase(),
                    self.state.exit_code(),
                    self.state.disk_summary().clone()
                ),
                before,
                "terminal stop is inert"
            );
        }
        for effect in &effects {
            self.observe(effect);
        }
        self.check();
        Verdict::Accepted
    }
    pub fn assert_model(&self, model: &Value) {
        let s = &model["s"];
        assert_eq!(s["cleanupDone"], self.finished_cleanup.is_some());
        assert_eq!(u64::from(self.pending_kind()), integer(&s["pending"]));
        let phase = match self.state.phase() {
            RunPhase::Preflight => 0,
            RunPhase::Finalize => 1,
            RunPhase::Cleaning => 2,
            RunPhase::Finished => 3,
            phase => panic!("unexpected phase {phase:?}"),
        };
        assert_eq!(phase, integer(&s["phase"]));
        let expected = if s["interrupted"].as_bool().unwrap() {
            130
        } else if s["fatal"].as_bool().unwrap() {
            2
        } else if s["incomplete"].as_bool().unwrap() {
            4
        } else {
            0
        };
        assert_eq!(self.state.exit_code(), expected);
        assert_eq!(self.outputs, integer(&s["outputs"]));
        assert_eq!(
            self.finish_code.is_some(),
            s["finishPublished"].as_bool().unwrap()
        );
        if let Some(code) = self.finish_code {
            assert_eq!(u64::try_from(code).unwrap(), integer(&s["finishCode"]));
        }
        assert_eq!(self.report_ack, s["reportAck"].as_bool().unwrap());
        assert_eq!(
            !self.completed.is_empty(),
            s["completed"].as_bool().unwrap()
        );
        assert_eq!(
            !self.state.disk_summary().cleanup.is_empty(),
            s["cleanupDone"].as_bool().unwrap()
        );
        let evidence = &s["diskEvidence"];
        let code = evidence["code"].as_str().unwrap();
        let message = evidence["message"].as_str().unwrap();
        let expected_disk = (!code.is_empty()).then(|| DiskStopReport {
            code: code.to_owned(),
            owned_bytes: (code == "filesystem.reserve.reached")
                .then(|| integer(&evidence["owned"])),
            available_bytes: (code == "filesystem.reserve.reached")
                .then(|| integer(&evidence["available"])),
            message: (!message.is_empty()).then(|| message.to_owned()),
            secondary: s["diskSecondary"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| DiskSecondary::Error {
                    code: entry["code"].as_str().unwrap().to_owned(),
                    message: entry["message"].as_str().unwrap().to_owned(),
                })
                .collect(),
        });
        assert_eq!(self.state.disk_summary().stop, expected_disk);
    }
    pub fn exit_code(&self) -> i32 {
        self.state.exit_code()
    }
    pub fn finished(&self) -> bool {
        self.state.phase() == RunPhase::Finished
    }
}
pub fn check_history(entry: Entry, cause: u8, commands: &[Command]) -> (i32, bool) {
    let mut driver = Driver::new(entry);
    driver.apply(Command::Stop(cause, 1024));
    // After the injected schedule, give the protocol eight successful acknowledgements.
    // This checks recovery progress separately from arbitrary failure/stutter histories.
    for command in commands
        .iter()
        .copied()
        .chain(std::iter::repeat_n(Command::Ack, 8))
    {
        driver.apply(command);
    }
    (driver.exit_code(), driver.finished())
}
/// One byte per entry/cause, then 9-byte commands: tag + full-width observation.
pub fn decode(data: &[u8]) -> (Entry, u8, Vec<Command>) {
    let entry = Entry::decode(data.first().copied().unwrap_or(0));
    let cause = data.get(1).copied().unwrap_or(0) % 3;
    let commands = data
        .get(2..)
        .unwrap_or_default()
        .as_chunks::<9>()
        .0
        .iter()
        .take(128)
        .map(|chunk| {
            let bytes = u64::from_le_bytes(chunk[1..].try_into().unwrap());
            match chunk[0] % 11 {
                0..=2 => Command::Stop(chunk[0] % 3, bytes),
                3 => Command::Ack,
                4 => Command::Fail(false),
                5 => Command::Fail(true),
                6 => Command::Retired,
                7 => Command::Duplicate,
                8 => Command::Unknown,
                9 => Command::Wrong,
                _ => Command::Reservation,
            }
        })
        .collect();
    (entry, cause, commands)
}
