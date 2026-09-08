mod compare;
mod input;
mod render;

use std::io::Write;

use crate::cli::ProgressArgs;
use compare::ProgressAccumulator;
pub use compare::{Comparison, ProgressResult, ProgressState, compare_reports};
use input::InputDisposition;
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
    let mut inputs = Vec::with_capacity(reports.len());
    let mut eligibilities = Vec::with_capacity(reports.len().saturating_sub(1));
    let mut accumulator = ProgressAccumulator::new(patience);
    let mut previous = None;
    for path in reports {
        let current = read_report(&path)?;
        inputs.push(InputDisposition::from(&current));
        if let Some(previous) = previous.as_ref()
            && let Some(eligibility) = accumulator.advance(previous, &current)
        {
            eligibilities.push(eligibility);
        }
        previous = Some(current);
    }
    drop(previous);
    let result = accumulator.into_result();
    render::render(format, &inputs, &eligibilities, &result, stdout, stderr)?;
    Ok(0)
}
