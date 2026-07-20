use std::fs;
use std::path::{Path, PathBuf};

use hoimin_core::{MutantFinished, OutputEvent, ProcessTermination, REPORT_SCHEMA_VERSION};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug)]
pub enum InputReport {
    Usable(UsableReport),
    Unusable {
        source: PathBuf,
        reason: UnusableReason,
    },
}

#[derive(Debug)]
pub struct UsableReport {
    pub source: PathBuf,
    pub mutants: Vec<MutantFinished>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnusableReason {
    MissingBaseline,
    BaselineFailed,
    Incomplete,
}

#[derive(Debug, Error)]
pub enum ProgressError {
    #[error("could not read progress report {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse progress report {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "unsupported schema version {found} in progress report {path}; expected {REPORT_SCHEMA_VERSION}"
    )]
    UnsupportedSchema { path: PathBuf, found: u32 },
    #[error("invalid structure in progress report {path}: {message}")]
    InvalidStructure {
        path: PathBuf,
        message: &'static str,
    },
    #[error("could not serialize progress output: {source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },
    #[error("could not write progress output: {source}")]
    Write {
        #[source]
        source: std::io::Error,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunReportDocument {
    schema_version: u32,
    run: OutputEvent,
    baseline: Option<OutputEvent>,
    mutants: Vec<OutputEvent>,
    summary: OutputEvent,
}

/// Reads and validates one mutation run report.
///
/// # Errors
///
/// Returns an error when the report cannot be read, parsed, or structurally validated.
pub fn read_report(path: &Path) -> Result<InputReport, ProgressError> {
    let bytes = fs::read(path).map_err(|source| ProgressError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let document = serde_json::from_slice::<RunReportDocument>(&bytes).map_err(|source| {
        ProgressError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })?;

    validate_schema_versions(path, &document)?;
    validate_structure(path, &document)?;

    let source = path.to_path_buf();
    let Some(OutputEvent::BaselineFinished(baseline)) = document.baseline else {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::MissingBaseline,
        });
    };
    if baseline.termination != ProcessTermination::Exit(0) {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::BaselineFailed,
        });
    }
    let OutputEvent::RunFinished(summary) = document.summary else {
        unreachable!("validate_structure requires a run_finished summary");
    };
    if !summary.complete {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::Incomplete,
        });
    }

    let mutants = document
        .mutants
        .into_iter()
        .map(|event| match event {
            OutputEvent::MutantFinished(mutant) => mutant,
            _ => unreachable!("validate_structure requires only mutant_finished events"),
        })
        .collect();
    Ok(InputReport::Usable(UsableReport { source, mutants }))
}

fn validate_schema_versions(
    path: &Path,
    document: &RunReportDocument,
) -> Result<(), ProgressError> {
    let versions = std::iter::once(document.schema_version)
        .chain(std::iter::once(document.run.schema_version()))
        .chain(document.baseline.iter().map(OutputEvent::schema_version))
        .chain(document.mutants.iter().map(OutputEvent::schema_version))
        .chain(std::iter::once(document.summary.schema_version()));
    for found in versions {
        if found != REPORT_SCHEMA_VERSION {
            return Err(ProgressError::UnsupportedSchema {
                path: path.to_path_buf(),
                found,
            });
        }
    }
    Ok(())
}

fn validate_structure(path: &Path, document: &RunReportDocument) -> Result<(), ProgressError> {
    if !matches!(document.run, OutputEvent::RunStarted(_)) {
        return Err(invalid_structure(path, "run must be a run_started event"));
    }
    if !matches!(
        document.baseline,
        None | Some(OutputEvent::BaselineFinished(_))
    ) {
        return Err(invalid_structure(
            path,
            "baseline must be null or a baseline_finished event",
        ));
    }
    if document
        .mutants
        .iter()
        .any(|event| !matches!(event, OutputEvent::MutantFinished(_)))
    {
        return Err(invalid_structure(
            path,
            "mutants must contain only mutant_finished events",
        ));
    }
    if !matches!(document.summary, OutputEvent::RunFinished(_)) {
        return Err(invalid_structure(
            path,
            "summary must be a run_finished event",
        ));
    }

    let run_id = document.run.run_id();
    let events = std::iter::once(&document.run)
        .chain(document.baseline.iter())
        .chain(document.mutants.iter())
        .chain(std::iter::once(&document.summary));
    let mut previous_sequence = None;
    for event in events {
        if event.run_id() != run_id {
            return Err(invalid_structure(
                path,
                "all present events must share run.run_id",
            ));
        }
        if previous_sequence.is_some_and(|previous| event.sequence() <= previous) {
            return Err(invalid_structure(
                path,
                "event sequences must be strictly increasing in document order",
            ));
        }
        previous_sequence = Some(event.sequence());
    }
    Ok(())
}

fn invalid_structure(path: &Path, message: &'static str) -> ProgressError {
    ProgressError::InvalidStructure {
        path: path.to_path_buf(),
        message,
    }
}
