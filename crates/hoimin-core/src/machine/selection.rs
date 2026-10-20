use std::collections::{BTreeSet, VecDeque};

use crate::{VerificationSelection, VerificationSelectionMode};

/// Execution membership, dispatch order and immutable verification provenance.
/// Consume this value when constructing a run; provenance cannot be attached or
/// replaced after initialization. All-candidate runs carry no verification metadata.
///
/// ```compile_fail
/// use hoimin_core::{RunSelection, VerificationSelection};
/// let metadata = VerificationSelection::candidate_ids(1, 1, false).unwrap();
/// let selection = RunSelection::all(metadata);
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunSelection {
    state: SelectionState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SelectionState {
    All,
    Explicit {
        ids: BTreeSet<String>,
        matched: BTreeSet<String>,
        metadata: VerificationSelection,
    },
    Ordered {
        ids: BTreeSet<String>,
        candidates: Box<OrderedCandidateState>,
        metadata: VerificationSelection,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RunSelectionError {
    #[error("verification mode does not match execution selection")]
    ModeMismatch,
    #[error("verification selected count differs from unique execution IDs")]
    SelectedCountMismatch,
    #[error("sample evidence differs from execution IDs or their order")]
    SampleIdsMismatch,
}

impl RunSelection {
    /// Executes every discovered candidate without verification provenance.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            state: SelectionState::All,
        }
    }

    /// Executes selected IDs as they are discovered, ignoring duplicate IDs.
    ///
    /// # Errors
    /// Requires `CandidateIds` provenance whose selected count equals the number
    /// of unique IDs. Empty selections are valid when selected is zero.
    pub fn explicit(
        ids: impl IntoIterator<Item = String>,
        metadata: VerificationSelection,
    ) -> Result<Self, RunSelectionError> {
        match metadata.mode() {
            VerificationSelectionMode::CandidateIds => {}
            VerificationSelectionMode::Top | VerificationSelectionMode::Sample => {
                return Err(RunSelectionError::ModeMismatch);
            }
        }
        let ids: BTreeSet<_> = ids.into_iter().collect();
        if ids.len() != metadata.selected() {
            return Err(RunSelectionError::SelectedCountMismatch);
        }
        Ok(Self {
            state: SelectionState::Explicit {
                ids,
                matched: BTreeSet::new(),
                metadata,
            },
        })
    }

    /// Validates selected IDs through EOF before dispatching them in request order.
    /// Top IDs are deduplicated at their first occurrence. Sample IDs must
    /// exactly match the unique ordered IDs in their provenance.
    ///
    /// # Errors
    /// Requires Top or Sample provenance, matching unique selected counts and,
    /// for Sample, identical IDs and order. Empty Top selections are valid.
    pub fn ordered(
        ordered_ids: Vec<String>,
        metadata: VerificationSelection,
    ) -> Result<Self, RunSelectionError> {
        match metadata.mode() {
            VerificationSelectionMode::CandidateIds => return Err(RunSelectionError::ModeMismatch),
            VerificationSelectionMode::Top => {}
            VerificationSelectionMode::Sample => {
                if metadata
                    .sampling()
                    .is_none_or(|sampling| sampling.selected_ids() != ordered_ids)
                {
                    return Err(RunSelectionError::SampleIdsMismatch);
                }
            }
        }
        let mut ids = BTreeSet::new();
        let ordered_ids: Vec<_> = ordered_ids
            .into_iter()
            .filter(|id| ids.insert(id.clone()))
            .collect();
        if ids.len() != metadata.selected() {
            return Err(RunSelectionError::SelectedCountMismatch);
        }
        Ok(Self {
            state: SelectionState::Ordered {
                ids,
                candidates: Box::new(OrderedCandidateState {
                    ids: ordered_ids,
                    phase: OrderedCandidatePhase::Collecting {
                        discovered: BTreeSet::new(),
                    },
                }),
                metadata,
            },
        })
    }

    /// Selected membership, or None for an unrestricted run.
    #[must_use]
    pub const fn candidate_ids(&self) -> Option<&BTreeSet<String>> {
        match &self.state {
            SelectionState::All => None,
            SelectionState::Explicit { ids, .. } | SelectionState::Ordered { ids, .. } => Some(ids),
        }
    }

    /// Requested dispatch order; explicit membership streams in discovery order.
    #[must_use]
    pub fn ordered_ids(&self) -> Option<&[String]> {
        self.ordered_state().map(|ordered| ordered.ids.as_slice())
    }

    /// The provenance shared by `run_started`, budget projection and run summary.
    #[must_use]
    pub const fn verification_selection(&self) -> Option<&VerificationSelection> {
        match &self.state {
            SelectionState::All => None,
            SelectionState::Explicit { metadata, .. }
            | SelectionState::Ordered { metadata, .. } => Some(metadata),
        }
    }

    pub(super) fn ordered_state(&self) -> Option<&OrderedCandidateState> {
        match &self.state {
            SelectionState::Ordered { candidates, .. } => Some(candidates),
            SelectionState::All | SelectionState::Explicit { .. } => None,
        }
    }

    pub(super) fn ordered_state_mut(&mut self) -> Option<&mut OrderedCandidateState> {
        match &mut self.state {
            SelectionState::Ordered { candidates, .. } => Some(candidates),
            SelectionState::All | SelectionState::Explicit { .. } => None,
        }
    }

    pub(super) fn select_streamed(&mut self, id: &str) -> bool {
        match &mut self.state {
            SelectionState::All => true,
            SelectionState::Explicit { ids, matched, .. } => {
                ids.contains(id) && matched.insert(id.to_owned())
            }
            SelectionState::Ordered { .. } => {
                unreachable!("ordered selection collects before dispatch")
            }
        }
    }

    pub(super) fn missing_id(&self) -> Option<&String> {
        match &self.state {
            SelectionState::All => None,
            SelectionState::Explicit { ids, matched, .. } => {
                ids.iter().find(|id| !matched.contains(*id))
            }
            SelectionState::Ordered { candidates, .. } => match &candidates.phase {
                OrderedCandidatePhase::Collecting { discovered } => {
                    candidates.ids.iter().find(|id| !discovered.contains(*id))
                }
                OrderedCandidatePhase::Ready { .. } => None,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OrderedCandidateState {
    pub(super) ids: Vec<String>,
    pub(super) phase: OrderedCandidatePhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum OrderedCandidatePhase {
    Collecting { discovered: BTreeSet<String> },
    Ready { remaining: VecDeque<String> },
}

impl OrderedCandidateState {
    /// Transfers only discovered IDs in request order, ending collection even
    /// when stopped before EOF. Undiscovered requested IDs have no result to emit.
    pub(super) fn take_remaining(&mut self) -> VecDeque<String> {
        match std::mem::replace(
            &mut self.phase,
            OrderedCandidatePhase::Ready {
                remaining: VecDeque::new(),
            },
        ) {
            OrderedCandidatePhase::Collecting { discovered } => self
                .ids
                .iter()
                .filter(|id| discovered.contains(*id))
                .cloned()
                .collect(),
            OrderedCandidatePhase::Ready { remaining } => remaining,
        }
    }
}
