use std::io::{self, BufRead, Read};
use std::path::Path;

use hoimin_core::{OutputEvent, ReportSequence, RunStarted};

use super::{
    ProgressConfig, ProgressError, ReportRunEvent, RunReportDocument, check_schema,
    invalid_structure,
};

/// Expose one physical line as an independent JSON input without retaining its
/// bytes. Very long lines have no additional raw-line allocation; event payload
/// decoding retains at most the current event in addition to finished mutants.
struct LineReader<'a, R> {
    reader: &'a mut R,
    ended: bool,
}

impl<R: BufRead> Read for LineReader<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.ended || output.is_empty() {
            return Ok(0);
        }
        let available = self.reader.fill_buf()?;
        let available = &available[..available.len().min(output.len())];
        let line_end = available.iter().position(|byte| *byte == b'\n');
        let count = line_end.map_or(available.len(), |end| end + 1);
        output[..count].copy_from_slice(&available[..count]);
        self.ended = line_end.is_some_and(|end| count == end + 1);
        self.reader.consume(count);
        Ok(count)
    }
}

// Skip blank lines and indentation without accumulating whitespace. The event
// parser must still reach the end of its physical line, rejecting trailing JSON.
fn read_event<T: serde::de::DeserializeOwned>(
    path: &Path,
    reader: &mut impl BufRead,
) -> Result<Option<T>, ProgressError> {
    loop {
        let available = reader.fill_buf().map_err(|source| ProgressError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        if available.is_empty() {
            return Ok(None);
        }
        let whitespace = available
            .iter()
            .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            .count();
        if whitespace == 0 {
            break;
        }
        reader.consume(whitespace);
    }
    serde_json::from_reader(LineReader {
        reader,
        ended: false,
    })
    .map(Some)
    .map_err(|source| ProgressError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn read_document(
    path: &Path,
    mut reader: impl BufRead,
) -> Result<RunReportDocument<ProgressConfig>, ProgressError> {
    let run: ReportRunEvent<ProgressConfig> = read_event(path, &mut reader)?
        .ok_or_else(|| invalid_structure(path, "JSONL report requires run_started"))?;
    check_schema(path, run.schema_version())?;
    let started = run.value();
    // Progress validates the closed config descendants without requiring a full
    // RunConfig. ReportSequence consumes only header identity.
    let header = RunStarted::minimal(
        started.run_id.clone(),
        started.sequence,
        started.resource_control.clone(),
    );
    let mut sequence = ReportSequence::new();
    sequence
        .observe(&OutputEvent::RunStarted(header))
        .map_err(|source| ProgressError::InvalidResult {
            path: path.to_path_buf(),
            source,
        })?;
    let mut baseline = None;
    let mut mutants = Vec::new();
    let mut summary = None;
    let mut saw_mutant = false;
    while let Some(event) = read_event::<OutputEvent>(path, &mut reader)? {
        check_schema(path, event.schema_version())?;
        sequence
            .validate(&event)
            .and_then(|()| sequence.observe(&event))
            .map_err(|source| ProgressError::InvalidResult {
                path: path.to_path_buf(),
                source,
            })?;
        match event {
            OutputEvent::BaselineFinished(_) => {
                if baseline.is_some() || saw_mutant {
                    return Err(invalid_structure(
                        path,
                        "baseline must occur once before mutant events",
                    ));
                }
                baseline = Some(event);
            }
            OutputEvent::MutantStarted(_) => {
                if baseline.is_none() {
                    return Err(invalid_structure(
                        path,
                        "mutant events require a preceding baseline",
                    ));
                }
                saw_mutant = true;
            }
            OutputEvent::MutantFinished(_) => mutants.push(event),
            OutputEvent::RunFinished(_) => summary = Some(event),
            OutputEvent::Diagnostic(_) => {}
            OutputEvent::RunStarted(_) => {
                unreachable!("ReportSequence rejects duplicate run starts")
            }
        }
    }
    let summary =
        summary.ok_or_else(|| invalid_structure(path, "JSONL report requires run_finished"))?;
    Ok(RunReportDocument {
        schema_version: run.schema_version(),
        run,
        baseline,
        mutants,
        summary,
    })
}
