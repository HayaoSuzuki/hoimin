use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Write;

use hoimin_core::{MutationCandidate, MutationStatus};
use serde::Serialize;

use super::ReviewError;
use super::model::{Applicability, Assessment, ReviewEvent};
use crate::cli::ReviewOutputFormat;
use crate::human_text::HumanText;

#[derive(Serialize)]
pub(super) struct ReviewDocument<'a> {
    pub schema_version: u32,
    pub command: &'static str,
    pub project: camino::Utf8PathBuf,
    pub candidates: Vec<ReviewRow<'a>>,
    pub stale_candidates: Vec<ReviewRow<'a>>,
    pub operators: Vec<OperatorReviewCounts>,
}

#[derive(Serialize)]
pub(super) struct ReviewRow<'a> {
    pub candidate: Cow<'a, MutationCandidate>,
    pub execution: ReviewExecution,
    pub review: Option<&'a ReviewEvent>,
    pub applicability: Applicability,
    pub history: &'a [ReviewEvent],
}

#[derive(Serialize)]
#[serde(tag = "state", content = "status", rename_all = "snake_case")]
pub(super) enum ReviewExecution {
    NotLoaded,
    NotInReport,
    Result(MutationStatus),
}

#[derive(Default, Serialize)]
pub(super) struct OperatorReviewCounts {
    pub operator: String,
    pub total: u64,
    pub reviewed: u64,
    pub unreviewed: u64,
    pub stale: u64,
    pub source_unavailable: u64,
    pub assessments: BTreeMap<Assessment, u64>,
}

pub(super) fn counts(rows: &[ReviewRow<'_>]) -> Vec<OperatorReviewCounts> {
    let mut operators = BTreeMap::<String, OperatorReviewCounts>::new();
    for row in rows {
        let entry = operators.entry(row.candidate.operator.clone()).or_default();
        entry.operator.clone_from(&row.candidate.operator);
        entry.total += 1;
        match row.applicability {
            Applicability::Stale => entry.stale += 1,
            Applicability::SourceUnavailable => entry.source_unavailable += 1,
            Applicability::Current | Applicability::IdentityConflict => {
                if let Some(review) = &row.review {
                    entry.reviewed += 1;
                    *entry
                        .assessments
                        .entry(review.draft.assessment)
                        .or_default() += 1;
                } else {
                    entry.unreviewed += 1;
                }
            }
        }
    }
    operators.into_values().collect()
}

pub(super) fn write(
    document: &ReviewDocument<'_>,
    format: ReviewOutputFormat,
    writer: &mut dyn Write,
) -> Result<(), ReviewError> {
    if format == ReviewOutputFormat::Json {
        serde_json::to_writer(&mut *writer, document)?;
        writeln!(writer)?;
        return Ok(());
    }
    writeln!(
        writer,
        "Human assessments (execution outcomes and mutation scores are unchanged)"
    )?;
    for (label, rows) in [
        ("candidate", &document.candidates),
        ("old candidate", &document.stale_candidates),
    ] {
        for row in rows {
            let assessment = row.review.as_ref().map_or_else(
                || "unreviewed".to_owned(),
                |event| serde_json::to_string(&event.draft.assessment).unwrap_or_default(),
            );
            writeln!(
                writer,
                "{label} {:?} {:?}:{} {} {:?}",
                HumanText(&row.candidate.id),
                HumanText(row.candidate.path.as_str()),
                row.candidate.line,
                assessment,
                row.applicability
            )?;
            for event in row.history {
                // Quote each external field through the shared human projection.
                writeln!(
                    writer,
                    "  revision {} {:?}: {:?}; evidence {:?}",
                    event.revision,
                    event.draft.assessment,
                    HumanText(&event.draft.reason),
                    event
                        .draft
                        .evidence
                        .iter()
                        .map(|value| HumanText(value))
                        .collect::<Vec<_>>()
                )?;
            }
            match row.execution {
                ReviewExecution::Result(status) => writeln!(writer, "  execution: {status:?}")?,
                ReviewExecution::NotLoaded => writeln!(writer, "  execution: not_loaded")?,
                ReviewExecution::NotInReport => writeln!(writer, "  execution: not_in_report")?,
            }
        }
    }
    for count in &document.operators {
        writeln!(
            writer,
            "operator {:?}: total {}, reviewed {}, unreviewed {}, stale {}, source_unavailable {}, assessments {:?}",
            HumanText(&count.operator),
            count.total,
            count.reviewed,
            count.unreviewed,
            count.stale,
            count.source_unavailable,
            count.assessments
        )?;
    }
    Ok(())
}
