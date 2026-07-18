mod human;
mod json;
mod jsonl;

use std::io::{self, Read, Seek, Write};
use std::path::Path;

use hoimin_core::{
    EffectFailed, EffectFailure, EmitOutput, OutputEmitted, OutputEvent, OutputFormat,
};

use self::json::{JsonError, JsonReport};

pub struct ReportHandler<Stdout, Stderr> {
    format: OutputFormat,
    stdout: Stdout,
    stderr: Stderr,
    json: Option<JsonReport>,
}

impl<Stdout, Stderr> ReportHandler<Stdout, Stderr>
where
    Stdout: Write,
    Stderr: Write,
{
    pub fn new(
        format: OutputFormat,
        stdout: Stdout,
        stderr: Stderr,
        spool_dir: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let json = match format {
            OutputFormat::Json => Some(JsonReport::new(spool_dir.as_ref())?),
            OutputFormat::Jsonl | OutputFormat::Human => None,
        };
        Ok(Self {
            format,
            stdout,
            stderr,
            json,
        })
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
        let json = match format {
            OutputFormat::Json => Some(JsonReport::with_spool(spool)),
            OutputFormat::Jsonl | OutputFormat::Human => None,
        };
        Self {
            format,
            stdout,
            stderr,
            json,
        }
    }

    pub fn handle(&mut self, request: EmitOutput) -> Result<OutputEmitted, EffectFailed> {
        let id = request.id;
        if matches!(request.event, OutputEvent::Diagnostic(_)) {
            match self.format {
                OutputFormat::Human => human::write_event(&mut self.stderr, &request.event)
                    .map_err(|error| human_failed(id, error))?,
                OutputFormat::Json | OutputFormat::Jsonl => {
                    jsonl::write_event(&mut self.stderr, &request.event)
                        .map_err(|error| serialization_failed(id, error))?;
                }
            }
            return Ok(OutputEmitted { id });
        }

        match self.format {
            OutputFormat::Jsonl => jsonl::write_event(&mut self.stdout, &request.event)
                .map_err(|error| serialization_failed(id, error))?,
            OutputFormat::Json => self
                .json
                .as_mut()
                .expect("JSON state is initialized by the constructor")
                .record(&request.event, &mut self.stdout)
                .map_err(|error| json_failed(id, error))?,
            OutputFormat::Human => human::write_event(&mut self.stdout, &request.event)
                .map_err(|error| human_failed(id, error))?,
        }
        Ok(OutputEmitted { id })
    }
}

fn serialization_failed(id: hoimin_core::EffectId, error: serde_json::Error) -> EffectFailed {
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

fn human_failed(id: hoimin_core::EffectId, error: io::Error) -> EffectFailed {
    EffectFailed {
        id,
        failure: EffectFailure::ReportIo {
            operation: "write human report event".to_owned(),
            message: error.to_string(),
        },
    }
}
