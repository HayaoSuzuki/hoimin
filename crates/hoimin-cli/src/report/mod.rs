pub(crate) mod delivery;
mod jsonl;

use std::io::{self, Write};

use hoimin_core::{EffectFailed, EffectFailure, EmitOutput, OutputEmitted, OutputEvent};

/// Runtime presentation only: never part of saved plans, sessions or fingerprints.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReportPresentation {
    pub verbose: bool,
}

/// Streams execution events as JSON Lines, with diagnostics on stderr.
pub struct ReportHandler<Stdout, Stderr> {
    presentation: ReportPresentation,
    stdout: Stdout,
    stderr: Stderr,
    lock_stdio_stderr: bool,
}

impl<Stdout: Write, Stderr: Write> ReportHandler<Stdout, Stderr> {
    pub fn new(stdout: Stdout, stderr: Stderr) -> Self {
        Self {
            presentation: ReportPresentation::default(),
            stdout,
            stderr,
            lock_stdio_stderr: false,
        }
    }

    /// Emits and flushes one event before acknowledging delivery.
    ///
    /// # Errors
    ///
    /// Returns an error when the event cannot be serialized or written.
    pub fn handle(&mut self, request: EmitOutput) -> Result<OutputEmitted, EffectFailed> {
        let EmitOutput { id, mut event } = request;
        if let OutputEvent::RunFinished(summary) = &mut event
            && let Some(janitor) = &mut summary.disk.janitor
        {
            janitor.project_details(self.presentation.verbose);
        }
        if matches!(event, OutputEvent::Diagnostic(_)) {
            // Keep streaming serialization atomic with respect to the tracing
            // output thread. Only owned CLI stdio uses this reentrant lock;
            // arbitrary library writers retain their own synchronization policy.
            let stderr = io::stderr();
            let _record = self.lock_stdio_stderr.then(|| stderr.lock());
            jsonl::write_buffered_event(&mut self.stderr, &event)
                .map_err(|error| serialization_failed(id, &error))?;
            return Ok(OutputEmitted { id });
        }

        jsonl::write_event(&mut self.stdout, &event)
            .map_err(|error| serialization_failed(id, &error))?;
        Ok(OutputEmitted { id })
    }

    #[must_use]
    pub fn with_presentation(mut self, presentation: ReportPresentation) -> Self {
        self.presentation = presentation;
        self
    }

    pub(crate) fn flush(&mut self) -> io::Result<()> {
        let stdout = self.stdout.flush();
        let stderr = self.stderr.flush();
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
