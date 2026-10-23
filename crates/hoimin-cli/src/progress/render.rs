use std::io::Write;

use serde::Serialize;

use crate::human_text::HumanText;

use super::compare::CandidateSetEligibility;
use super::details::Details;
use super::input::InputDisposition;
use super::{Comparison, ProgressError, ProgressResult, ProgressState, UnusableReason};

const PROGRESS_SCHEMA_VERSION: u32 = 3;

pub(super) fn render<Stdout, Stderr>(
    inputs: &[InputDisposition],
    eligibilities: &[CandidateSetEligibility],
    result: &ProgressResult,
    details: Option<&Details>,
    stdout: &mut Stdout,
    stderr: &mut Stderr,
) -> Result<(), ProgressError>
where
    Stdout: Write,
    Stderr: Write,
{
    write_diagnostics(inputs, eligibilities, result, stderr)?;
    render_json(inputs, result, details, stdout)
}

fn render_json<Stdout>(
    inputs: &[InputDisposition],
    result: &ProgressResult,
    details: Option<&Details>,
    stdout: &mut Stdout,
) -> Result<(), ProgressError>
where
    Stdout: Write,
{
    let inputs = inputs.iter().map(InputDocument::from).collect::<Vec<_>>();
    let comparisons = result
        .comparisons
        .iter()
        .map(ComparisonDocument::from)
        .collect::<Vec<_>>();
    let document = ProgressDocument {
        schema_version: PROGRESS_SCHEMA_VERSION,
        details,
        patience: result.patience.get(),
        consecutive_stalls: result.consecutive_stalls,
        latest: LatestDecision::from(result),
        inputs: &inputs,
        comparisons: &comparisons,
    };
    serde_json::to_writer(&mut *stdout, &document)
        .map_err(|source| ProgressError::Serialize { source })?;
    writeln!(stdout).map_err(write_error)
}

fn write_diagnostics<Stderr>(
    inputs: &[InputDisposition],
    eligibilities: &[CandidateSetEligibility],
    result: &ProgressResult,
    stderr: &mut Stderr,
) -> Result<(), ProgressError>
where
    Stderr: Write,
{
    for input in inputs {
        if let Some(reason) = input.reason {
            writeln!(
                stderr,
                "warning: unusable progress report {}: {}",
                HumanText(&input.source.to_string_lossy()),
                unusable_reason_label(reason)
            )
            .map_err(write_error)?;
        }
    }
    for (comparison_index, eligibility) in eligibilities.iter().enumerate() {
        match eligibility {
            CandidateSetEligibility::Matching => {}
            CandidateSetEligibility::Different => writeln!(
                stderr,
                "warning: comparison {} has different candidate ID sets; progress is indeterminate",
                comparison_index + 1,
            )
            .map_err(write_error)?,
            CandidateSetEligibility::Duplicate => writeln!(
                stderr,
                "warning: comparison {} has duplicate candidate IDs; progress is indeterminate",
                comparison_index + 1,
            )
            .map_err(write_error)?,
        }
    }
    for (index, comparison) in result.comparisons.iter().enumerate() {
        if comparison.ambiguous > 0 {
            writeln!(
                stderr,
                "warning: comparison {} has {} ambiguous mutant key(s); excluded from comparison",
                index + 1,
                comparison.ambiguous
            )
            .map_err(write_error)?;
        }
    }
    for (index, input) in inputs.iter().enumerate() {
        if let Some(duplicate) = &input.duplicate {
            writeln!(
                stderr,
                "warning: progress input {} ({}) repeats input {} ({}): {}; comparisons are unchanged",
                index + 1,
                HumanText(&input.source.to_string_lossy()),
                duplicate.previous + 1,
                HumanText(&inputs[duplicate.previous].source.to_string_lossy()),
                duplicate.reason.label(),
            )
            .map_err(write_error)?;
        }
    }
    Ok(())
}

fn state_name(state: ProgressState) -> &'static str {
    match state {
        ProgressState::Improving => "improving",
        ProgressState::Regressing => "regressing",
        ProgressState::Stalled => "stalled",
        ProgressState::Saturated => "saturated",
        ProgressState::Indeterminate => "indeterminate",
    }
}

fn unusable_reason_label(reason: UnusableReason) -> &'static str {
    match reason {
        UnusableReason::MissingBaseline => "missing baseline",
        UnusableReason::BaselineFailed => "baseline failed",
        UnusableReason::Incomplete => "incomplete run",
    }
}

fn unusable_reason_name(reason: UnusableReason) -> &'static str {
    match reason {
        UnusableReason::MissingBaseline => "missing_baseline",
        UnusableReason::BaselineFailed => "baseline_failed",
        UnusableReason::Incomplete => "incomplete",
    }
}

fn write_error(source: std::io::Error) -> ProgressError {
    ProgressError::Write { source }
}

#[derive(Serialize)]
struct ProgressDocument<'a> {
    details: Option<&'a Details>,
    schema_version: u32,
    patience: usize,
    consecutive_stalls: usize,
    latest: LatestDecision,
    inputs: &'a [InputDocument],
    comparisons: &'a [ComparisonDocument],
}

#[derive(Serialize)]
struct LatestDecision {
    state: &'static str,
    consecutive_stalls: usize,
    patience: usize,
    saturated: bool,
}

impl From<&ProgressResult> for LatestDecision {
    fn from(result: &ProgressResult) -> Self {
        Self {
            state: state_name(result.latest),
            consecutive_stalls: result.consecutive_stalls,
            patience: result.patience.get(),
            saturated: matches!(result.latest, ProgressState::Saturated),
        }
    }
}

#[derive(Serialize)]
struct InputDocument {
    source: String,
    usable: bool,
    reason: Option<&'static str>,
}

impl From<&InputDisposition> for InputDocument {
    fn from(input: &InputDisposition) -> Self {
        Self {
            source: input.source.display().to_string(),
            usable: input.is_usable(),
            reason: input.reason.map(unusable_reason_name),
        }
    }
}

#[derive(Serialize)]
struct ComparisonDocument {
    common: usize,
    added: usize,
    removed: usize,
    ambiguous: usize,
    inconclusive: usize,
    improvements: usize,
    regressions: usize,
    carried_survivors: usize,
    previous_score: Option<f64>,
    current_score: Option<f64>,
    score_delta: Option<f64>,
    state: &'static str,
}

impl From<&Comparison> for ComparisonDocument {
    fn from(comparison: &Comparison) -> Self {
        Self {
            common: comparison.common,
            added: comparison.added,
            removed: comparison.removed,
            ambiguous: comparison.ambiguous,
            inconclusive: comparison.inconclusive,
            improvements: comparison.improvements,
            regressions: comparison.regressions,
            carried_survivors: comparison.carried_survivors,
            previous_score: comparison.previous_score,
            current_score: comparison.current_score,
            score_delta: comparison.score_delta,
            state: state_name(comparison.state),
        }
    }
}
