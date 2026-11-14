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

#[cfg(test)]
mod coverage_tests {
    use super::super::duplicate::{DuplicateInput, DuplicateReason};
    use super::*;
    use std::io;

    #[derive(Clone, Copy)]
    enum Warning {
        Unusable,
        Different,
        Duplicate,
        Ambiguous,
        Repeated(DuplicateReason),
        Quiet,
    }

    fn warning_fixture(
        warning: Warning,
    ) -> (
        Vec<InputDisposition>,
        Vec<CandidateSetEligibility>,
        ProgressResult,
    ) {
        let mut inputs = vec![InputDisposition {
            source: "first.json".into(),
            reason: None,
            duplicate: None,
        }];
        let mut eligibilities = vec![CandidateSetEligibility::Matching];
        let mut result =
            super::super::compare_reports(&[], std::num::NonZeroUsize::new(2).unwrap());
        result.comparisons.push(Comparison {
            common: 0,
            added: 0,
            removed: 0,
            ambiguous: 0,
            inconclusive: 0,
            improvements: 0,
            regressions: 0,
            carried_survivors: 0,
            previous_score: None,
            current_score: None,
            score_delta: None,
            state: ProgressState::Indeterminate,
        });
        match warning {
            Warning::Unusable => inputs[0].reason = Some(UnusableReason::MissingBaseline),
            Warning::Different => eligibilities[0] = CandidateSetEligibility::Different,
            Warning::Duplicate => eligibilities[0] = CandidateSetEligibility::Duplicate,
            Warning::Ambiguous => result.comparisons[0].ambiguous = 1,
            Warning::Repeated(reason) => inputs.push(InputDisposition {
                source: "repeated.json".into(),
                reason: None,
                duplicate: Some(DuplicateInput {
                    previous: 0,
                    reason,
                }),
            }),
            Warning::Quiet => {}
        }
        (inputs, eligibilities, result)
    }

    #[derive(Default)]
    struct DiagnosticWriter {
        fail: bool,
        fail_newline: bool,
        bytes: Vec<u8>,
    }

    impl Write for DiagnosticWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail || (self.fail_newline && bytes == b"\n") {
                return Err(io::Error::other("progress diagnostic failed"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[rstest::rstest]
    #[case::unusable(
        Warning::Unusable,
        "unusable progress report first.json: missing baseline"
    )]
    #[case::different(Warning::Different, "different candidate ID sets")]
    #[case::duplicate(Warning::Duplicate, "duplicate candidate IDs")]
    #[case::ambiguous(Warning::Ambiguous, "1 ambiguous mutant key(s)")]
    #[case::same_path(
        Warning::Repeated(DuplicateReason::SamePath),
        "same path; comparisons are unchanged"
    )]
    #[case::identical(
        Warning::Repeated(DuplicateReason::IdenticalBytes),
        "identical bytes; comparisons are unchanged"
    )]
    fn diagnostic_output_explains_the_observed_warning(
        #[case] warning: Warning,
        #[case] expected: &str,
    ) {
        let (inputs, eligibilities, result) = warning_fixture(warning);
        let mut output = DiagnosticWriter::default();
        let mut stdout = DiagnosticWriter::default();
        render(
            &inputs,
            &eligibilities,
            &result,
            None,
            &mut stdout,
            &mut output,
        )
        .unwrap();
        output.flush().unwrap();
        let actual = String::from_utf8(output.bytes).unwrap();
        let document: serde_json::Value = serde_json::from_slice(&stdout.bytes).unwrap();

        let actual_1 = actual.lines().count();
        let actual_3 = document["comparisons"].as_array();

        assert!(actual.contains(expected), "{actual}");
        assert_eq!(actual_1, 1);
        assert_eq!(document["schema_version"], 3);
        assert_eq!(actual_3.unwrap().len(), 1);
    }

    #[rstest::rstest]
    #[case::unusable(Warning::Unusable)]
    #[case::different(Warning::Different)]
    #[case::duplicate(Warning::Duplicate)]
    #[case::ambiguous(Warning::Ambiguous)]
    #[case::same_path(Warning::Repeated(DuplicateReason::SamePath))]
    #[case::identical(Warning::Repeated(DuplicateReason::IdenticalBytes))]
    fn diagnostic_write_failure_preserves_the_io_cause(#[case] warning: Warning) {
        let (inputs, eligibilities, result) = warning_fixture(warning);
        let mut output = DiagnosticWriter {
            fail: true,
            ..DiagnosticWriter::default()
        };
        let mut stdout = DiagnosticWriter::default();

        let error = render(
            &inputs,
            &eligibilities,
            &result,
            None,
            &mut stdout,
            &mut output,
        )
        .unwrap_err();

        assert!(
            matches!(error, ProgressError::Write { source } if source.kind() == io::ErrorKind::Other && source.to_string() == "progress diagnostic failed")
        );
        assert_eq!(output.bytes, Vec::<u8>::new());
        assert_eq!(stdout.bytes, Vec::<u8>::new());
    }

    #[test]
    fn matching_usable_inputs_without_ambiguity_produce_no_warning() {
        let (inputs, eligibilities, result) = warning_fixture(Warning::Quiet);
        let mut output = DiagnosticWriter::default();

        write_diagnostics(&inputs, &eligibilities, &result, &mut output).unwrap();

        assert_eq!(output.bytes, Vec::<u8>::new());
    }

    #[test]
    fn json_output_failure_preserves_the_serializers_io_cause() {
        let (inputs, eligibilities, result) = warning_fixture(Warning::Quiet);
        let mut stdout = DiagnosticWriter {
            fail: true,
            ..DiagnosticWriter::default()
        };
        let mut stderr = DiagnosticWriter::default();

        let error = render(
            &inputs,
            &eligibilities,
            &result,
            None,
            &mut stdout,
            &mut stderr,
        )
        .unwrap_err();

        assert!(
            matches!(error, ProgressError::Serialize { source } if source.io_error_kind() == Some(io::ErrorKind::Other) && source.to_string().contains("progress diagnostic failed"))
        );
        assert_eq!(stdout.bytes, Vec::<u8>::new());
        assert_eq!(stderr.bytes, Vec::<u8>::new());
    }

    #[test]
    fn trailing_newline_failure_preserves_the_completed_json_document() {
        let (inputs, eligibilities, result) = warning_fixture(Warning::Quiet);
        let mut stdout = DiagnosticWriter {
            fail_newline: true,
            ..DiagnosticWriter::default()
        };
        let mut stderr = DiagnosticWriter::default();
        let error = render(
            &inputs,
            &eligibilities,
            &result,
            None,
            &mut stdout,
            &mut stderr,
        )
        .unwrap_err();
        let document: serde_json::Value = serde_json::from_slice(&stdout.bytes).unwrap();

        let actual_2 = stdout.bytes.last();

        assert!(
            matches!(error, ProgressError::Write { source } if source.kind() == io::ErrorKind::Other)
        );
        assert_eq!(document["schema_version"], 3);
        assert_ne!(actual_2, Some(&b'\n'));
        assert_eq!(stderr.bytes, Vec::<u8>::new());
    }
}
