use std::io::{self, Seek, SeekFrom, Write};
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

pub(super) struct JsonReport {
    mutants: NamedTempFile,
    has_mutants: bool,
    run: Option<Vec<u8>>,
    baseline: Option<Vec<u8>>,
    finished: bool,
}

impl JsonReport {
    pub(super) fn new(spool_dir: &Path) -> io::Result<Self> {
        Ok(Self {
            mutants: NamedTempFile::new_in(spool_dir)?,
            has_mutants: false,
            run: None,
            baseline: None,
            finished: false,
        })
    }

    pub(super) fn resident_buffer_bytes(&self) -> usize {
        self.run.as_ref().map_or(0, Vec::capacity) + self.baseline.as_ref().map_or(0, Vec::capacity)
    }

    pub(super) fn record(
        &mut self,
        event: &OutputEvent,
        stdout: &mut impl Write,
    ) -> Result<(), JsonError> {
        if self.finished {
            return Err(JsonError::State("JSON report is already finished"));
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
            OutputEvent::MutantFinished(_) => self.write_mutant(event)?,
            OutputEvent::RunFinished(_) => {
                self.write_final(event, stdout)?;
                self.finished = true;
            }
            OutputEvent::MutantStarted(_) | OutputEvent::Diagnostic(_) => {}
        }
        Ok(())
    }

    fn write_mutant(&mut self, event: &OutputEvent) -> Result<(), JsonError> {
        if self.has_mutants {
            self.mutants
                .write_all(b",")
                .map_err(|source| JsonError::Io {
                    operation: "write mutant separator",
                    source,
                })?;
        }
        serde_json::to_writer(&mut self.mutants, event).map_err(JsonError::Serialization)?;
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
            .as_file_mut()
            .flush()
            .and_then(|()| {
                self.mutants
                    .as_file_mut()
                    .seek(SeekFrom::Start(0))
                    .map(drop)
            })
            .map_err(|source| JsonError::Io {
                operation: "rewind mutant spool",
                source,
            })?;
        io::copy(self.mutants.as_file_mut(), stdout).map_err(|source| JsonError::Io {
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
