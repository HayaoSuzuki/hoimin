use std::path::PathBuf;

use super::duplicate::DuplicateInput;

// Keep the existing public progress reader API while format knowledge has one owner.
pub(super) use crate::artifacts::run_report::read_report_with_fingerprint;
pub use crate::artifacts::run_report::{
    InputReport, ProgressError, UnusableReason, UsableReport, read_report,
};

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
