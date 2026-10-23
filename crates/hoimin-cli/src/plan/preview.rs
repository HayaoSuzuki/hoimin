use std::collections::BTreeMap;
use std::io::Write;

use camino::Utf8PathBuf;
use hoimin_core::{RunSelection, VerificationSelection};
use serde::Serialize;

use super::PlanManifest;
use crate::cli::VerifySelection;

/// Validated selection metadata, independent of execution reports and outcomes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VerifyPreview {
    schema_version: u32,
    kind: &'static str,
    plan_schema_version: u32,
    ranking_rule_version: u32,
    verification_selection: VerificationSelection,
    offset: Option<usize>,
    retained_candidates: usize,
    candidates: Vec<PreviewCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PreviewCandidate {
    id: String,
    rank: usize,
    selection_order: usize,
    path: Utf8PathBuf,
    line: u32,
    column: u32,
    operator: String,
    original: String,
    replacement: String,
}

impl VerifyPreview {
    pub(super) fn new(
        manifest: &PlanManifest,
        requested: &VerifySelection,
        resolved: &RunSelection,
        discovered_ids: &[String],
    ) -> Self {
        let ids = resolved.ordered_ids().unwrap_or(discovered_ids);
        let verification_selection = resolved
            .verification_selection()
            .expect("verified preview requires selection provenance")
            .clone();
        let by_id = manifest
            .candidates
            .iter()
            .map(|candidate| (candidate.id.as_str(), candidate))
            .collect::<BTreeMap<_, _>>();
        let candidates = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let candidate = by_id[id.as_str()];
                PreviewCandidate {
                    id: id.clone(),
                    rank: candidate.rank,
                    selection_order: index + 1,
                    path: candidate.path.clone(),
                    line: candidate.line,
                    column: candidate.column,
                    operator: candidate.operator.clone(),
                    original: candidate.original.clone(),
                    replacement: candidate.replacement.clone(),
                }
            })
            .collect();
        Self {
            schema_version: 2,
            kind: "verify_preview",
            plan_schema_version: manifest.schema_version,
            ranking_rule_version: manifest.ranking_rule_version,
            verification_selection,
            offset: match requested {
                VerifySelection::CandidateIds(_) | VerifySelection::Sample { .. } => None,
                VerifySelection::Top { offset, .. } => Some(*offset),
            },
            retained_candidates: manifest.candidates.len(),
            candidates,
        }
    }

    /// Writes one JSON preview document followed by a newline.
    ///
    /// # Errors
    /// Returns a destination write or serialization error.
    pub fn write(&self, writer: &mut impl Write) -> Result<(), String> {
        serde_json::to_writer(&mut *writer, self).map_err(|error| error.to_string())?;
        writeln!(writer).map_err(|error| error.to_string())
    }
}
