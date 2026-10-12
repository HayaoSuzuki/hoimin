use std::io::BufRead;
use std::path::Path;

use hoimin_core::{OutputEvent, REPORT_SCHEMA_VERSION, ReportSequence, RunStarted};

use super::{ProgressError, ProgressRunEvent, RunReportDocument, invalid_structure};

/// Reuses storage for one nonblank physical line, including a final line
/// without a newline. No event-history buffer is retained.
pub(super) fn read_line(
    path: &Path,
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
) -> Result<bool, ProgressError> {
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', line)
            .map_err(|source| ProgressError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            return Ok(false);
        }
        if !line
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            return Ok(true);
        }
    }
}

pub(super) fn read_document(
    path: &Path,
    mut reader: impl BufRead,
    mut line: Vec<u8>,
) -> Result<RunReportDocument, ProgressError> {
    let parse_error = |source| ProgressError::Parse {
        path: path.to_path_buf(),
        source,
    };
    let run: ProgressRunEvent = serde_json::from_slice(&line).map_err(parse_error)?;
    check_schema(path, run.schema_version())?;
    let started = run.value();
    // Progress deliberately treats configuration as opaque. ReportSequence
    // consumes only header identity; don't deserialize config as today's RunConfig.
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
    while read_line(path, &mut reader, &mut line)? {
        let event: OutputEvent = serde_json::from_slice(&line).map_err(parse_error)?;
        check_schema(path, event.schema_version())?;
        if event.schema_version() != run.schema_version() {
            return Err(invalid_structure(path, "report schema versions must match"));
        }
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

fn check_schema(path: &Path, found: u32) -> Result<(), ProgressError> {
    if matches!(found, 3 | REPORT_SCHEMA_VERSION) {
        Ok(())
    } else {
        Err(ProgressError::UnsupportedSchema {
            path: path.to_path_buf(),
            found,
        })
    }
}
