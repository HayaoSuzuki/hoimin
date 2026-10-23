//! Bounded observations from startup cleanup, independent of run cleanup policy.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A typed cause, never inferred from diagnostic wording.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JanitorReason {
    Young,
    ActiveLease,
    RetainMarker,
    CoordinatorBusy,
    MutationBarrierActive,
    InvalidCandidate,
    CandidateInspectionFailed,
    IdentityInspectionFailed,
    LeaseInvalid,
    LeaseLockFailed,
    MarkerInvalid,
    HeartbeatInvalid,
    AgeUnverifiable,
    UnsafeStaging,
    CoordinatorFailed,
    MutationBarrierFailed,
    ClaimFailed,
    ClaimedLeaseUnavailable,
    DeleteFailed,
    CleanupIncomplete,
    SelectionFailed,
    SelectionDeadline,
    SelectionChildLimit,
    CursorPersistenceFailed,
}

impl JanitorReason {
    /// Classifies ordinary holds so bounded detail retention can prioritize failures.
    #[must_use]
    pub const fn is_normal_hold(self) -> bool {
        matches!(
            self,
            Self::Young
                | Self::ActiveLease
                | Self::RetainMarker
                | Self::CoordinatorBusy
                | Self::MutationBarrierActive
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JanitorScan {
    /// Enumeration completed; this does not mean every candidate was inspected.
    pub directory_scan_complete: bool,
    /// Directory-entry observations, not a stable distinct-root census.
    pub observed_matching_entries: u64,
    pub selected_roots: u64,
    pub uninspected_selected_roots: u64,
    pub selection_omitted_observed_entries: u64,
    pub selection_limited: bool,
    pub cursor_deferred: bool,
    pub stop_reason: Option<JanitorReason>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JanitorDetail {
    pub reason: JanitorReason,
    /// Observed target and cause; not a certified remaining-root path.
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JanitorDetailOmissions {
    pub normal_details: u64,
    pub issue_details: u64,
    pub truncated_details: u64,
}

/// Janitor-owned facts. Root outcomes partition `inspected_roots`; deleted items
/// are acknowledged unlink/delete operations, never freed bytes or a remaining census.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JanitorSummary {
    pub inspected_roots: u64,
    pub reclaimed_roots: u64,
    pub absent_roots: u64,
    pub preserved_roots: u64,
    pub preserved_by_reason: BTreeMap<JanitorReason, u64>,
    pub unresolved_roots: u64,
    pub unresolved_by_reason: BTreeMap<JanitorReason, u64>,
    /// Traversal observations may revisit an entry.
    pub examined_entries: u64,
    /// Includes control markers and root entries; Windows acknowledgement can be delete-pending.
    pub removed_entries: u64,
    pub scan: JanitorScan,
    pub details: Vec<JanitorDetail>,
    pub detail_omissions: JanitorDetailOmissions,
}
