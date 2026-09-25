use std::collections::BTreeMap;
use std::io::Write;

use camino::Utf8PathBuf;
use hoimin_core::VerificationSelection;
use serde::Serialize;

use super::{PlanManifest, ResolvedVerifySelection};
use crate::cli::{OutputFormat, VerifySelection};

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
        resolved: &ResolvedVerifySelection,
        discovered_ids: &[String],
        verification_selection: VerificationSelection,
    ) -> Self {
        let ids = match resolved {
            ResolvedVerifySelection::ExplicitCandidates(_) => discovered_ids,
            ResolvedVerifySelection::RankedCandidates(ids) => ids,
        };
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
                VerifySelection::CandidateIds(_) => None,
                VerifySelection::Top { .. } => Some(0),
                VerifySelection::TopRange { offset, .. } => Some(*offset),
            },
            retained_candidates: manifest.candidates.len(),
            candidates,
        }
    }

    /// Writes one preview document, or a human-readable header and candidate rows.
    ///
    /// # Errors
    /// Returns a destination write or serialization error.
    pub fn write(&self, format: OutputFormat, writer: &mut impl Write) -> Result<(), String> {
        match format {
            OutputFormat::Json | OutputFormat::Jsonl => {
                serde_json::to_writer(&mut *writer, self).map_err(|error| error.to_string())?;
                writeln!(writer).map_err(|error| error.to_string())
            }
            OutputFormat::Human => self.write_human(writer).map_err(|error| error.to_string()),
        }
    }

    fn write_human(&self, writer: &mut impl Write) -> std::io::Result<()> {
        let selection = &self.verification_selection;
        // Serialize enum names to keep the human and machine policy vocabulary identical.
        let metadata = serde_json::to_value(selection)?;
        writeln!(
            writer,
            "verify preview: mode={} policy={} requested={} selected={} scope={} plan_truncated={} offset={} retained_candidates={}",
            metadata["mode"].as_str().unwrap_or_default(),
            metadata["policy"].as_str().unwrap_or_default(),
            selection.requested,
            selection.selected,
            metadata["scope"].as_str().unwrap_or_default(),
            selection.plan_truncated,
            self.offset
                .map_or_else(|| "null".to_owned(), |offset| offset.to_string()),
            self.retained_candidates,
        )?;
        for candidate in &self.candidates {
            writeln!(
                writer,
                "{}: {} rank={} {}:{}:{} operator={} {:?} -> {:?}",
                candidate.selection_order,
                candidate.id,
                candidate.rank,
                candidate.path,
                candidate.line,
                candidate.column,
                candidate.operator,
                candidate.original,
                candidate.replacement
            )?;
        }
        Ok(())
    }
}
