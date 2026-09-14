use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use hoimin_core::{OutputEvent, REPORT_SCHEMA_VERSION};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub(super) enum JsonError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Serialization(serde_json::Error),
    State(&'static str),
}

trait MutantSpool: Read + Write + Seek + Send {}

impl<T> MutantSpool for T where T: Read + Write + Seek + Send {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Lifecycle {
    Open,
    Poisoned,
    Finished,
}

pub(super) struct JsonReport {
    mutants: Box<dyn MutantSpool>,
    has_mutants: bool,
    run: Option<Vec<u8>>,
    baseline: Option<Vec<u8>>,
    lifecycle: Lifecycle,
}

impl JsonReport {
    pub(super) fn new(spool_dir: &Path) -> io::Result<Self> {
        Ok(Self::with_spool(NamedTempFile::new_in(spool_dir)?))
    }

    pub(super) fn with_spool(spool: impl Read + Write + Seek + Send + 'static) -> Self {
        Self {
            mutants: Box::new(spool),
            has_mutants: false,
            run: None,
            baseline: None,
            lifecycle: Lifecycle::Open,
        }
    }

    pub(super) fn record(
        &mut self,
        event: &OutputEvent,
        stdout: &mut impl Write,
    ) -> Result<(), JsonError> {
        if self.lifecycle != Lifecycle::Open {
            return Err(JsonError::State(match self.lifecycle {
                Lifecycle::Poisoned => "JSON report is poisoned after a partial write",
                Lifecycle::Finished => "JSON report is already finished",
                Lifecycle::Open => unreachable!(),
            }));
        }
        match event {
            OutputEvent::RunStarted(_) => {
                if self.run.is_some() {
                    return Err(JsonError::State("run_started was emitted more than once"));
                }
                self.run = Some(serde_json::to_vec(event).map_err(JsonError::Serialization)?);
            }
            OutputEvent::BaselineFinished(_) => {
                if self.baseline.is_some() {
                    return Err(JsonError::State(
                        "baseline_finished was emitted more than once",
                    ));
                }
                self.baseline = Some(serde_json::to_vec(event).map_err(JsonError::Serialization)?);
            }
            OutputEvent::MutantFinished(_) => {
                self.lifecycle = Lifecycle::Poisoned;
                self.write_mutant(event)?;
                self.lifecycle = Lifecycle::Open;
            }
            OutputEvent::RunFinished(_) => {
                self.lifecycle = Lifecycle::Poisoned;
                self.write_final(event, stdout)?;
                self.lifecycle = Lifecycle::Finished;
            }
            OutputEvent::MutantStarted(_) | OutputEvent::Diagnostic(_) => {}
        }
        Ok(())
    }

    fn write_mutant(&mut self, event: &OutputEvent) -> Result<(), JsonError> {
        let mut record = Vec::new();
        if self.has_mutants {
            record.push(b',');
        }
        serde_json::to_writer(&mut record, event).map_err(JsonError::Serialization)?;
        self.mutants
            .write_all(&record)
            .map_err(|source| JsonError::Io {
                operation: "write mutant record",
                source,
            })?;
        self.has_mutants = true;
        Ok(())
    }

    fn write_final(
        &mut self,
        summary: &OutputEvent,
        stdout: &mut impl Write,
    ) -> Result<(), JsonError> {
        let run = self
            .run
            .as_deref()
            .ok_or(JsonError::State("run_finished preceded run_started"))?;
        write!(
            stdout,
            "{{\"schema_version\":{REPORT_SCHEMA_VERSION},\"run\":"
        )
        .and_then(|()| stdout.write_all(run))
        .and_then(|()| stdout.write_all(b",\"baseline\":"))
        .map_err(|source| JsonError::Io {
            operation: "write report header",
            source,
        })?;
        match self.baseline.as_deref() {
            Some(baseline) => stdout.write_all(baseline),
            None => stdout.write_all(b"null"),
        }
        .and_then(|()| stdout.write_all(b",\"mutants\":["))
        .map_err(|source| JsonError::Io {
            operation: "write baseline",
            source,
        })?;

        self.mutants
            .flush()
            .and_then(|()| self.mutants.seek(SeekFrom::Start(0)).map(drop))
            .map_err(|source| JsonError::Io {
                operation: "rewind mutant spool",
                source,
            })?;
        io::copy(&mut self.mutants, stdout).map_err(|source| JsonError::Io {
            operation: "copy mutant spool",
            source,
        })?;
        stdout
            .write_all(b"],\"summary\":")
            .map_err(|source| JsonError::Io {
                operation: "write summary prefix",
                source,
            })?;
        serde_json::to_writer(&mut *stdout, summary).map_err(JsonError::Serialization)?;
        stdout
            .write_all(b"}\n")
            .and_then(|()| stdout.flush())
            .map_err(|source| JsonError::Io {
                operation: "finish JSON report",
                source,
            })
    }
}
