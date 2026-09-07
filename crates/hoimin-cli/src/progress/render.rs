use std::io::Write;

use serde::Serialize;

use crate::cli::ProgressOutputFormat;

use super::compare::{CandidateSetEligibility, candidate_set_eligibility};
use super::{
    Comparison, InputReport, ProgressError, ProgressResult, ProgressState, UnusableReason,
};

const PROGRESS_SCHEMA_VERSION: u32 = 1;

pub(super) fn render<Stdout, Stderr>(
    format: ProgressOutputFormat,
    inputs: &[InputReport],
    result: &ProgressResult,
    stdout: &mut Stdout,
    stderr: &mut Stderr,
) -> Result<(), ProgressError>
where
    Stdout: Write,
    Stderr: Write,
{
    write_diagnostics(inputs, result, stderr)?;
    match format {
        ProgressOutputFormat::Human => render_human(inputs, result, stdout),
        ProgressOutputFormat::Json => render_json(inputs, result, stdout),
    }
}

fn render_json<Stdout>(
    inputs: &[InputReport],
    result: &ProgressResult,
    stdout: &mut Stdout,
) -> Result<(), ProgressError>
where
    Stdout: Write,
{
    let inputs = inputs
        .iter()
        .map(InputDisposition::from)
        .collect::<Vec<_>>();
    let comparisons = result
        .comparisons
        .iter()
        .map(ComparisonDocument::from)
        .collect::<Vec<_>>();
    let document = ProgressDocument {
        schema_version: PROGRESS_SCHEMA_VERSION,
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

fn render_human<Stdout>(
    inputs: &[InputReport],
    result: &ProgressResult,
    stdout: &mut Stdout,
) -> Result<(), ProgressError>
where
    Stdout: Write,
{
    writeln!(stdout, "state: {}", state_name(result.latest)).map_err(write_error)?;
    if let Some(comparison) = final_pair_is_usable(inputs)
        .then_some(result.comparisons.last())
        .flatten()
    {
        writeln!(
            stdout,
            "comparable score: {}",
            optional_score(comparison.current_score, false)
        )
        .map_err(write_error)?;
        writeln!(
            stdout,
            "delta: {}",
            optional_score(comparison.score_delta, true)
        )
        .map_err(write_error)?;
        writeln!(stdout, "improvements: {}", comparison.improvements).map_err(write_error)?;
        writeln!(stdout, "regressions: {}", comparison.regressions).map_err(write_error)?;
        writeln!(
            stdout,
            "carried_survivors: {}",
            comparison.carried_survivors
        )
        .map_err(write_error)?;
        writeln!(stdout, "added: {}", comparison.added).map_err(write_error)?;
        writeln!(stdout, "removed: {}", comparison.removed).map_err(write_error)?;
        writeln!(stdout, "ambiguous: {}", comparison.ambiguous).map_err(write_error)?;
        writeln!(stdout, "inconclusive: {}", comparison.inconclusive).map_err(write_error)?;
    }
    writeln!(stdout, "stalls: {}", result.consecutive_stalls).map_err(write_error)?;
    writeln!(stdout, "patience: {}", result.patience).map_err(write_error)?;
    writeln!(
        stdout,
        "saturated: {}",
        matches!(result.latest, ProgressState::Saturated)
    )
    .map_err(write_error)
}

fn final_pair_is_usable(inputs: &[InputReport]) -> bool {
    inputs
        .windows(2)
        .last()
        .is_some_and(|pair| matches!(pair, [InputReport::Usable(_), InputReport::Usable(_)]))
}

fn write_diagnostics<Stderr>(
    inputs: &[InputReport],
    result: &ProgressResult,
    stderr: &mut Stderr,
) -> Result<(), ProgressError>
where
    Stderr: Write,
{
    for input in inputs {
        if let InputReport::Unusable { source, reason } = input {
            writeln!(
                stderr,
                "warning: unusable progress report {}: {}",
                source.display(),
                unusable_reason_label(*reason)
            )
            .map_err(write_error)?;
        }
    }
    let mut comparison_index = 0;
    for pair in inputs.windows(2) {
        let [InputReport::Usable(previous), InputReport::Usable(current)] = pair else {
            continue;
        };
        comparison_index += 1;
        match candidate_set_eligibility(previous, current) {
            CandidateSetEligibility::Matching => {}
            CandidateSetEligibility::Different => writeln!(
                stderr,
                "warning: comparison {comparison_index} has different candidate ID sets; progress is indeterminate"
            )
            .map_err(write_error)?,
            CandidateSetEligibility::Duplicate => writeln!(
                stderr,
                "warning: comparison {comparison_index} has duplicate candidate IDs; progress is indeterminate"
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
    Ok(())
}

fn optional_score(value: Option<f64>, signed: bool) -> String {
    match value {
        Some(value) if signed => format!("{value:+.6}"),
        Some(value) => format!("{value:.6}"),
        None => "n/a".to_owned(),
    }
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
    schema_version: u32,
    patience: usize,
    consecutive_stalls: usize,
    latest: LatestDecision,
    inputs: &'a [InputDisposition],
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
struct InputDisposition {
    source: String,
    usable: bool,
    reason: Option<&'static str>,
}

impl From<&InputReport> for InputDisposition {
    fn from(input: &InputReport) -> Self {
        match input {
            InputReport::Usable(report) => Self {
                source: report.source.display().to_string(),
                usable: true,
                reason: None,
            },
            InputReport::Unusable { source, reason } => Self {
                source: source.display().to_string(),
                usable: false,
                reason: Some(unusable_reason_name(*reason)),
            },
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
