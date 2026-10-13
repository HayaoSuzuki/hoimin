pub(crate) mod delivery;
mod human;
mod json;
mod jsonl;

use std::io::{self, Read, Seek, Write};
use std::path::Path;

use hoimin_core::{
    EffectFailed, EffectFailure, EmitOutput, OutputEmitted, OutputEvent, OutputFormat,
};

use self::json::{JsonError, JsonReport};

pub(crate) struct PreparedReport {
    encoding: ReportEncoding,
}

/// A JSON spool belongs only to JSON output. Releasing it preserves the format
/// for stderr diagnostics, but further non-diagnostic events must be rejected.
enum ReportEncoding {
    Human,
    Jsonl,
    Json(JsonReport),
    JsonReleased,
}

impl PreparedReport {
    pub(crate) fn new(format: OutputFormat, spool_dir: impl AsRef<Path>) -> io::Result<Self> {
        let encoding = match format {
            OutputFormat::Json => ReportEncoding::Json(JsonReport::new(spool_dir.as_ref())?),
            OutputFormat::Jsonl => ReportEncoding::Jsonl,
            OutputFormat::Human => ReportEncoding::Human,
        };
        Ok(Self { encoding })
    }

    pub(crate) fn attach<Stdout, Stderr>(
        self,
        stdout: Stdout,
        stderr: Stderr,
    ) -> ReportHandler<Stdout, Stderr>
    where
        Stdout: Write,
        Stderr: Write,
    {
        ReportHandler {
            encoding: self.encoding,
            stdout,
            stderr,
            lock_stdio_stderr: false,
        }
    }
}

pub struct ReportHandler<Stdout, Stderr> {
    encoding: ReportEncoding,
    stdout: Stdout,
    stderr: Stderr,
    lock_stdio_stderr: bool,
}

impl<Stdout, Stderr> ReportHandler<Stdout, Stderr>
where
    Stdout: Write,
    Stderr: Write,
{
    /// Creates a report handler backed by a temporary JSON spool when required.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON spool cannot be created.
    pub fn new(
        format: OutputFormat,
        stdout: Stdout,
        stderr: Stderr,
        spool_dir: impl AsRef<Path>,
    ) -> io::Result<Self> {
        Ok(PreparedReport::new(format, spool_dir)?.attach(stdout, stderr))
    }

    pub fn with_mutant_spool<Spool>(
        format: OutputFormat,
        stdout: Stdout,
        stderr: Stderr,
        spool: Spool,
    ) -> Self
    where
        Spool: Read + Write + Seek + Send + 'static,
    {
        let encoding = match format {
            OutputFormat::Json => ReportEncoding::Json(JsonReport::with_spool(spool)),
            OutputFormat::Jsonl => ReportEncoding::Jsonl,
            OutputFormat::Human => ReportEncoding::Human,
        };
        Self {
            encoding,
            stdout,
            stderr,
            lock_stdio_stderr: false,
        }
    }

    /// Emits one output event.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured report cannot serialize or write the event.
    pub fn handle(&mut self, request: EmitOutput) -> Result<OutputEmitted, EffectFailed> {
        let EmitOutput { id, event } = request;
        if matches!(event, OutputEvent::Diagnostic(_)) {
            // Keep streaming serialization atomic with respect to the tracing
            // output thread. Only owned CLI stdio uses this reentrant lock;
            // arbitrary library writers retain their own synchronization policy.
            let stderr = io::stderr();
            let _record = self.lock_stdio_stderr.then(|| stderr.lock());
            match &self.encoding {
                ReportEncoding::Human => human::write_event(&mut self.stderr, &event)
                    .map_err(|error| human_failed(id, &error))?,
                ReportEncoding::Json(_) | ReportEncoding::Jsonl | ReportEncoding::JsonReleased => {
                    jsonl::write_buffered_event(&mut self.stderr, &event)
                        .map_err(|error| serialization_failed(id, &error))?;
                }
            }
            return Ok(OutputEmitted { id });
        }

        match &mut self.encoding {
            ReportEncoding::Jsonl => jsonl::write_event(&mut self.stdout, &event)
                .map_err(|error| serialization_failed(id, &error))?,
            ReportEncoding::Json(report) => {
                report
                    .record(&event, &mut self.stdout)
                    .map_err(|error| json_failed(id, error))?;
            }
            ReportEncoding::JsonReleased => {
                return Err(EffectFailed {
                    id,
                    failure: EffectFailure::ReportState {
                        message: "JSON report state was not initialized".to_owned(),
                    },
                });
            }
            ReportEncoding::Human => human::write_event(&mut self.stdout, &event)
                .map_err(|error| human_failed(id, &error))?,
        }
        if matches!(self.encoding, ReportEncoding::Human)
            && let OutputEvent::MutantFinished(value) = &event
            && !value.diagnostics.is_empty()
        {
            let stderr = io::stderr();
            let _record = self.lock_stdio_stderr.then(|| stderr.lock());
            human::write_mutant_diagnostics(&mut self.stderr, &value.diagnostics)
                .map_err(|error| human_failed(id, &error))?;
        }
        Ok(OutputEmitted { id })
    }

    pub(crate) fn flush_and_release_spool(&mut self) -> io::Result<()> {
        let stdout = self.stdout.flush();
        let stderr = self.stderr.flush();
        // Release the descendant spool handle even when either external writer fails. Windows
        // cannot claim the delivery directory while any descendant file remains open.
        if matches!(self.encoding, ReportEncoding::Json(_)) {
            self.encoding = ReportEncoding::JsonReleased;
        }
        stdout.and(stderr)
    }
}

fn serialization_failed(id: hoimin_core::EffectId, error: &serde_json::Error) -> EffectFailed {
    let failure = if error.is_io() {
        EffectFailure::ReportIo {
            operation: "write JSON Lines event".to_owned(),
            message: error.to_string(),
        }
    } else {
        EffectFailure::ReportSerialization {
            message: error.to_string(),
        }
    };
    EffectFailed { id, failure }
}

fn json_failed(id: hoimin_core::EffectId, error: JsonError) -> EffectFailed {
    let failure = match error {
        JsonError::Io { operation, source } => EffectFailure::ReportIo {
            operation: operation.to_owned(),
            message: source.to_string(),
        },
        JsonError::Serialization(error) if error.is_io() => EffectFailure::ReportIo {
            operation: "serialize JSON report".to_owned(),
            message: error.to_string(),
        },
        JsonError::Serialization(error) => EffectFailure::ReportSerialization {
            message: error.to_string(),
        },
        JsonError::State(message) => EffectFailure::ReportState {
            message: message.to_owned(),
        },
    };
    EffectFailed { id, failure }
}

fn human_failed(id: hoimin_core::EffectId, error: &io::Error) -> EffectFailed {
    EffectFailed {
        id,
        failure: EffectFailure::ReportIo {
            operation: "write human report event".to_owned(),
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn prepared_report_is_send() {
        assert_send::<PreparedReport>();
    }

    #[test]
    fn released_json_rejects_report_events_but_retains_stderr_diagnostics() {
        let mut handler = ReportHandler::with_mutant_spool(
            OutputFormat::Json,
            Vec::new(),
            Vec::new(),
            io::Cursor::new(Vec::new()),
        );
        handler.flush_and_release_spool().unwrap();
        let diagnostic = EmitOutput {
            id: hoimin_core::EffectId(1),
            event: OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
                "run",
                1,
                "warning",
                "late",
                "late diagnostic",
            )),
        };
        let started = EmitOutput {
            id: hoimin_core::EffectId(2),
            event: OutputEvent::MutantStarted(hoimin_core::MutantStarted::new("run", 2, "m1", 1)),
        };

        let acknowledged = handler.handle(diagnostic).unwrap();
        let rejected = handler.handle(started).unwrap_err();

        assert_eq!(acknowledged.id, hoimin_core::EffectId(1));
        assert_eq!(rejected.id, hoimin_core::EffectId(2));
        assert!(matches!(
            rejected.failure,
            EffectFailure::ReportState { .. }
        ));
        assert_eq!(handler.stdout, [] as [u8; 0]);
        let emitted: serde_json::Value = serde_json::from_slice(&handler.stderr).unwrap();
        assert_eq!(emitted["kind"], "diagnostic");
        assert_eq!(emitted["code"], "late");
    }
}
