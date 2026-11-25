//! Accepted-event journal: derive reference decisions without copying validator state.
use hoimin_core::{
    ByteSpan, Diagnostic, DiskRunSummary, MutantFinished, MutantStarted, MutationCandidate,
    MutationOutcome, OutputEvent, REPORT_SCHEMA_VERSION, ReportSequence, ReportSequenceError,
    ResourceControl, ResourceMode, RunStarted, RunSummary, summarize,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Start,
    MutantStart,
    MutantFinish,
    Diagnostic,
    Finish,
}

#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub kind: Kind,
    pub run: u8,
    pub mutant: u8,
    pub identity: u64,
    pub sequence: u64,
}

impl Command {
    pub fn new(kind: Kind, sequence: u64, mutant: u8, identity: u64) -> Self {
        Self {
            kind,
            run: 0,
            mutant,
            identity,
            sequence,
        }
    }

    pub fn event(self) -> OutputEvent {
        let run = format!("run-{}", self.run);
        let mutant = format!("mutant-{}", self.mutant);
        match self.kind {
            Kind::Start => OutputEvent::RunStarted(RunStarted::minimal(
                run,
                self.sequence,
                ResourceControl {
                    mode: ResourceMode::BestEffort,
                    mechanism: "model".into(),
                },
            )),
            Kind::MutantStart => OutputEvent::MutantStarted(MutantStarted::new(
                run,
                self.sequence,
                mutant,
                self.identity,
            )),
            Kind::MutantFinish => OutputEvent::MutantFinished(MutantFinished {
                schema_version: REPORT_SCHEMA_VERSION,
                sequence: self.sequence,
                run_id: run,
                candidate: MutationCandidate {
                    id: mutant,
                    sequence: self.identity,
                    path: "model.py".into(),
                    span: ByteSpan {
                        start: 0,
                        length: 1,
                    },
                    original: "x".into(),
                    replacement: "y".into(),
                    operator: "model".into(),
                    line: 1,
                    column: 0,
                    symbol: None,
                    file_hash: "0".repeat(64),
                },
                outcome: MutationOutcome::NotRun,
                resource_mode: ResourceMode::BestEffort,
                diagnostics: Vec::new(),
            }),
            Kind::Diagnostic => OutputEvent::Diagnostic(Diagnostic::new(
                run,
                self.sequence,
                "info",
                "model",
                "model",
            )),
            Kind::Finish => OutputEvent::RunFinished(RunSummary {
                resolved_mutant_timeout: None,
                schema_version: REPORT_SCHEMA_VERSION,
                sequence: self.sequence,
                run_id: run,
                counts: summarize(&[]),
                complete: true,
                exit_code: 0,
                disk: DiskRunSummary::unmeasured(0, 0),
                verification_selection: None,
            }),
        }
    }
}

pub fn code(result: &Result<(), ReportSequenceError>) -> &'static str {
    use ReportSequenceError as E;
    match result {
        Ok(()) => "ok",
        Err(E::RunNotStarted) => "not_started",
        Err(E::RunAlreadyStarted { .. }) => "started",
        Err(E::RunAlreadyFinished { .. }) => "finished",
        Err(E::RunFinishedWithActiveMutants { .. }) => "unfinished",
        Err(E::RunIdMismatch { .. }) => "run_id",
        Err(E::NotMonotonic { .. }) => "sequence",
        Err(E::MutantAlreadyStarted { .. }) => "active",
        Err(E::DuplicateMutantIdentity { .. }) => "identity_duplicate",
        Err(E::MutantIdentitySequenceMismatch { .. }) => "identity_sequence",
        Err(E::MutantNotStarted { .. }) => "not_active",
    }
}

pub fn apply(actual: &mut ReportSequence, command: Command) -> Result<(), ReportSequenceError> {
    let event = command.event();
    let result = actual.validate(&event);
    // contracts builds deliberately assert on invalid producer events. Consumer
    // validation must stay available in both profiles; default builds also check
    // observe's rejected-input path rather than silently avoiding it.
    #[cfg(feature = "contracts")]
    if result.is_ok() {
        actual.observe(&event).unwrap();
    }
    #[cfg(not(feature = "contracts"))]
    assert_eq!(actual.observe(&event), result);
    result
}

fn expected(journal: &[Command], command: Command) -> Result<(), ReportSequenceError> {
    use ReportSequenceError as E;
    let Some(first) = journal.first() else {
        return if command.kind == Kind::Start {
            Ok(())
        } else {
            Err(E::RunNotStarted)
        };
    };
    let run_id = format!("run-{}", first.run);
    if journal.last().unwrap().kind == Kind::Finish {
        return Err(E::RunAlreadyFinished { run_id });
    }
    if command.kind == Kind::Start {
        return Err(E::RunAlreadyStarted {
            run_id: format!("run-{}", command.run),
        });
    }
    if first.run != command.run {
        return Err(E::RunIdMismatch {
            expected: run_id,
            received: format!("run-{}", command.run),
        });
    }
    let started = journal
        .iter()
        .find(|event| event.kind == Kind::MutantStart && event.mutant == command.mutant);
    let finished = journal
        .iter()
        .any(|event| event.kind == Kind::MutantFinish && event.mutant == command.mutant);
    let mutant_id = format!("mutant-{}", command.mutant);
    match command.kind {
        Kind::MutantStart | Kind::MutantFinish => {
            if let Some(start) = started {
                if start.identity != command.identity {
                    return Err(E::MutantIdentitySequenceMismatch {
                        mutant_id,
                        expected_sequence: start.identity,
                        received_sequence: command.identity,
                    });
                }
                match (command.kind, finished) {
                    (Kind::MutantStart, false) => {
                        return Err(E::MutantAlreadyStarted {
                            mutant_id,
                            mutant_sequence: command.identity,
                        });
                    }
                    (Kind::MutantStart, true) => {
                        return Err(E::DuplicateMutantIdentity {
                            mutant_id,
                            mutant_sequence: command.identity,
                        });
                    }
                    (Kind::MutantFinish, true) => {
                        return Err(E::MutantNotStarted {
                            mutant_id,
                            mutant_sequence: command.identity,
                        });
                    }
                    _ => {}
                }
            } else if command.kind == Kind::MutantFinish {
                return Err(E::MutantNotStarted {
                    mutant_id,
                    mutant_sequence: command.identity,
                });
            }
        }
        Kind::Finish => {
            let count = journal
                .iter()
                .filter(|start| {
                    start.kind == Kind::MutantStart
                        && !journal
                            .iter()
                            .any(|end| end.kind == Kind::MutantFinish && end.mutant == start.mutant)
                })
                .count();
            if count != 0 {
                return Err(E::RunFinishedWithActiveMutants { count });
            }
        }
        _ => {}
    }
    let previous = journal.last().unwrap().sequence;
    if command.sequence <= previous {
        return Err(E::NotMonotonic {
            previous,
            received: command.sequence,
        });
    }
    Ok(())
}

/// Check every input and a final observation basis against the accepted journal.
/// The probes expose latent changes made by a rejected observe operation.
pub fn check_history(commands: &[Command]) -> Vec<&'static str> {
    let mut actual = ReportSequence::new();
    let mut journal = Vec::new();
    let mut outcomes = Vec::new();
    for command in commands {
        let want = expected(&journal, *command);
        let got = apply(&mut actual, *command);
        assert_eq!(got, want, "history: {journal:?}, input: {command:?}");
        outcomes.push(code(&got));
        if got.is_ok() {
            journal.push(*command);
        }
    }
    for run in 0..2 {
        for mutant in 0..2 {
            for identity in [0, 1, u64::MAX] {
                for kind in [
                    Kind::Start,
                    Kind::MutantStart,
                    Kind::MutantFinish,
                    Kind::Diagnostic,
                    Kind::Finish,
                ] {
                    for sequence in [0, u64::MAX] {
                        let probe = Command {
                            kind,
                            run,
                            mutant,
                            identity,
                            sequence,
                        };
                        assert_eq!(
                            actual.validate(&probe.event()),
                            expected(&journal, probe),
                            "probe: {probe:?}"
                        );
                    }
                }
            }
        }
    }
    outcomes
}

/// 19-byte commands keep full event and identity counters; at most128 commands.
pub fn decode(data: &[u8]) -> Vec<Command> {
    data.as_chunks::<19>()
        .0
        .iter()
        .take(128)
        .map(|chunk| Command {
            run: chunk[1] % 2,
            ..Command::new(
                [
                    Kind::Start,
                    Kind::MutantStart,
                    Kind::MutantFinish,
                    Kind::Diagnostic,
                    Kind::Finish,
                ][usize::from(chunk[0] % 5)],
                u64::from_le_bytes(chunk[3..11].try_into().unwrap()),
                chunk[2] % 2,
                u64::from_le_bytes(chunk[11..19].try_into().unwrap()),
            )
        })
        .collect()
}
