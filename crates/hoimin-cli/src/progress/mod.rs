mod compare;
mod input;
mod render;

use std::io::Write;

use crate::cli::ProgressArgs;
pub use compare::{Comparison, ProgressResult, ProgressState, compare_reports};
pub use input::{InputReport, ProgressError, UnusableReason, UsableReport, read_report};

/// Reads, compares, and renders ordered mutation run reports.
///
/// # Errors
///
/// Returns an error when an input report cannot be read or validated, or output cannot be written.
pub fn run<Stdout, Stderr>(
    args: ProgressArgs,
    stdout: &mut Stdout,
    stderr: &mut Stderr,
) -> Result<i32, ProgressError>
where
    Stdout: Write,
    Stderr: Write,
{
    let ProgressArgs {
        reports,
        patience,
        format,
    } = args;
    let reports = reports
        .iter()
        .map(|path| read_report(path))
        .collect::<Result<Vec<_>, _>>()?;
    let result = compare_reports(&reports, patience);
    render::render(format, &reports, &result, stdout, stderr)?;
    Ok(0)
}
