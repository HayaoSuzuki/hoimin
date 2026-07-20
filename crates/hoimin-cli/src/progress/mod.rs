mod compare;
mod input;

pub use compare::{Comparison, ProgressResult, ProgressState, compare_reports};
pub use input::{InputReport, ProgressError, UnusableReason, UsableReport, read_report};
