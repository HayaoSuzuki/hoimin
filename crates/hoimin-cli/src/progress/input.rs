use std::path::{Path, PathBuf};

use super::duplicate::DuplicateInput;

use hoimin_core::ProcessTermination;

pub use crate::artifacts::run_report::ReportReadError;
use crate::artifacts::run_report::{self, DecodedReport, ReportConfigProjection};

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
    pub mutants: Vec<hoimin_core::MutantFinished>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnusableReason {
    MissingBaseline,
    BaselineFailed,
    Incomplete,
}

/// Reads a current-schema JSON/JSONL report and classifies its usability for progress.
///
/// # Errors
/// Returns an error when reading, decoding, or structural validation fails.
pub fn read_report(path: &Path) -> Result<InputReport, ReportReadError> {
    run_report::read_report(path).map(|report| classify(path, report))
}

pub(super) fn read_report_with_fingerprint(
    path: &Path,
) -> Result<(InputReport, blake3::Hash), ReportReadError> {
    let (report, fingerprint) = run_report::read_report_with_fingerprint(path)?;
    Ok((classify(path, report), fingerprint))
}

fn classify(path: &Path, report: DecodedReport<ReportConfigProjection>) -> InputReport {
    let source = path.to_path_buf();
    let reason = match report.baseline {
        None => Some(UnusableReason::MissingBaseline),
        Some(baseline) if baseline.termination != ProcessTermination::Exit(0) => {
            Some(UnusableReason::BaselineFailed)
        }
        Some(_) if !report.summary.complete => Some(UnusableReason::Incomplete),
        Some(_) => None,
    };
    match reason {
        Some(reason) => InputReport::Unusable { source, reason },
        None => InputReport::Usable(UsableReport {
            source,
            mutants: report.mutants,
        }),
    }
}

#[derive(Debug)]
pub(super) struct InputDisposition {
    pub(super) source: PathBuf,
    pub(super) reason: Option<UnusableReason>,
    pub(super) duplicate: Option<DuplicateInput>,
}

impl From<&InputReport> for InputDisposition {
    fn from(input: &InputReport) -> Self {
        match input {
            InputReport::Usable(report) => Self {
                source: report.source.clone(),
                reason: None,
                duplicate: None,
            },
            InputReport::Unusable { source, reason } => Self {
                source: source.clone(),
                reason: Some(*reason),
                duplicate: None,
            },
        }
    }
}

impl InputDisposition {
    pub(super) fn is_usable(&self) -> bool {
        self.reason.is_none()
    }
}
