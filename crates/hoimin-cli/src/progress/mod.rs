mod compare;
mod details;
mod duplicate;
mod input;
mod render;

use std::io::Write;

use crate::cli::ProgressArgs;
use compare::ProgressAccumulator;
pub use compare::{Comparison, ProgressResult, ProgressState, compare_reports};
use duplicate::DuplicateInputs;
use input::{InputDisposition, read_report_with_fingerprint};
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
        fail_on_regression,
        reports,
        patience,
        format,
        details,
        details_limit,
    } = args;
    let mut inputs = Vec::with_capacity(reports.len());
    let mut eligibilities = Vec::with_capacity(reports.len().saturating_sub(1));
    let mut accumulator = ProgressAccumulator::new(patience);
    let mut previous = None;
    let mut duplicates = DuplicateInputs::default();
    let final_index = reports.len().saturating_sub(1);
    let mut final_details = None;
    for (index, path) in reports.into_iter().enumerate() {
        let (current, fingerprint) = read_report_with_fingerprint(&path)?;
        let mut disposition = InputDisposition::from(&current);
        disposition.duplicate = duplicates.observe(&path, fingerprint, &inputs);
        inputs.push(disposition);
        if let Some(previous) = previous.as_ref() {
            let limit = (details && index == final_index).then_some(details_limit);
            let (eligibility, changes) =
                accumulator.advance_with_details(previous, &current, limit);
            if let Some(eligibility) = eligibility {
                eligibilities.push(eligibility);
            }
            if limit.is_some() {
                final_details = Some(details::Details::new(
                    index,
                    details_limit,
                    eligibility,
                    changes,
                ));
            }
        }
        previous = Some(current);
    }
    drop(previous);
    let result = accumulator.into_result();
    render::render(
        format,
        &inputs,
        &eligibilities,
        &result,
        final_details.as_ref(),
        stdout,
        stderr,
    )?;
    Ok(i32::from(
        fail_on_regression && result.latest == ProgressState::Regressing,
    ))
}
