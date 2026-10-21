use std::path::PathBuf;

use camino::Utf8PathBuf;
use hoimin_core::CandidateDescriptor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewKey {
    pub project: Utf8PathBuf,
    pub candidate_id: String,
    pub descriptor: CandidateDescriptor,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub enum Assessment {
    Actionable,
    NotUseful,
    SuspectedEquivalent,
    NeedsInvestigation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceState {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    Current,
    Stale,
    SourceUnavailable,
    IdentityConflict,
}

/// Identifies an artifact by its path and content, independently of execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub path: PathBuf,
    pub content_hash: String,
}

/// Associates a report artifact with the execution that produced it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportReference {
    pub artifact: ArtifactReference,
    pub run_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDraft {
    pub assessment: Assessment,
    pub reason: String,
    pub evidence: Vec<String>,
    pub timestamp_ms: u64,
    pub plan: ArtifactReference,
    pub report: Option<ReportReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewEvent {
    pub revision: u64,
    pub draft: ReviewDraft,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReviewState {
    pub history: Vec<ReviewEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum UpdateRejection {
    #[error("candidate source is not current")]
    NotCurrent,
    #[error("review revision conflict; read the current revision and retry")]
    RevisionConflict,
    #[error("review revision overflow")]
    RevisionOverflow,
}

#[must_use]
pub fn applicability(
    expected: &ReviewKey,
    stored: &ReviewKey,
    source: SourceState,
) -> Applicability {
    if expected != stored {
        return Applicability::IdentityConflict;
    }
    match source {
        SourceState::Fresh => Applicability::Current,
        SourceState::Stale => Applicability::Stale,
        SourceState::Unavailable => Applicability::SourceUnavailable,
    }
}

/// Appends one event without mutating the caller's history.
///
/// # Errors
/// Rejects stale candidates, outdated revisions, and revision overflow.
pub fn apply_update(
    state: &ReviewState,
    applicability: Applicability,
    expected_revision: u64,
    draft: ReviewDraft,
) -> Result<ReviewState, UpdateRejection> {
    let revision = next_revision(state, applicability, expected_revision)?;
    #[cfg(test)]
    super::store::STORE_WORK.with_borrow_mut(|work| work.cloned_events += state.history.len());
    let mut result = state.clone();
    result.history.push(ReviewEvent { revision, draft });
    Ok(result)
}

// The store owns its freshly loaded target history, so it can append without
// cloning every previously saved annotation. Both APIs share rejection rules.
pub(super) fn apply_update_owned(
    mut state: ReviewState,
    applicability: Applicability,
    expected_revision: u64,
    draft: ReviewDraft,
) -> Result<ReviewState, UpdateRejection> {
    let revision = next_revision(&state, applicability, expected_revision)?;
    state.history.push(ReviewEvent { revision, draft });
    Ok(state)
}

fn next_revision(
    state: &ReviewState,
    applicability: Applicability,
    expected_revision: u64,
) -> Result<u64, UpdateRejection> {
    if applicability != Applicability::Current {
        return Err(UpdateRejection::NotCurrent);
    }
    let revision = state.history.last().map_or(0, |event| event.revision);
    if revision != expected_revision {
        return Err(UpdateRejection::RevisionConflict);
    }
    revision
        .checked_add(1)
        .ok_or(UpdateRejection::RevisionOverflow)
}

/// Validates bounded, inert human annotations.
///
/// # Errors
/// Returns an error for empty reasons, excessive sizes, or control characters.
pub fn validate_draft(draft: &ReviewDraft) -> Result<(), super::ReviewError> {
    fn safe_text(value: &str, limit: usize) -> bool {
        value.len() <= limit
            && !value
                .chars()
                .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
    }
    fn valid_reference(reference: &ArtifactReference) -> bool {
        !reference.path.as_os_str().is_empty()
            && reference.content_hash.len() == 64
            && reference
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }
    if !valid_reference(&draft.plan)
        || draft.report.as_ref().is_some_and(|reference| {
            !valid_reference(&reference.artifact)
                || reference.run_id.trim().is_empty()
                || reference.run_id.len() > 4096
                || reference.run_id.chars().any(char::is_control)
        })
    {
        return Err(super::ReviewError::Invalid(
            "invalid review artifact reference".into(),
        ));
    }
    if draft.reason.trim().is_empty() || !safe_text(&draft.reason, 4096) {
        return Err(super::ReviewError::Invalid(
            "reason must be nonblank, safe text of at most 4096 UTF-8 bytes".into(),
        ));
    }
    if draft.evidence.len() > 8 || draft.evidence.iter().any(|value| !safe_text(value, 2048)) {
        return Err(super::ReviewError::Invalid(
            "evidence permits at most 8 safe strings of 2048 UTF-8 bytes each".into(),
        ));
    }
    Ok(())
}
