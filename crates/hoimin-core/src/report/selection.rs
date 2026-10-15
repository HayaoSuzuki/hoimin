//! Selection provenance owns the relationships between mode, policy and evidence.
//! Only the wire boundary carries redundant discriminants; callers cannot change
//! counts or evidence after construction. Completion outcomes do not change the
//! selection, including the order of sample IDs that never reach execution.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSelectionMode {
    CandidateIds,
    Top,
    Sample,
}

impl VerificationSelectionMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CandidateIds => "candidate_ids",
            Self::Top => "top",
            Self::Sample => "sample",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSelectionPolicy {
    ExplicitCandidates,
    Strict,
    FileRoundRobinV1,
    LineRoundRobinV1,
    Splitmix64FisherYatesV1,
}

impl VerificationSelectionPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitCandidates => "explicit_candidates",
            Self::Strict => "strict",
            Self::FileRoundRobinV1 => "file_round_robin_v1",
            Self::LineRoundRobinV1 => "line_round_robin_v1",
            Self::Splitmix64FisherYatesV1 => "splitmix64_fisher_yates_v1",
        }
    }
}

/// Only policies that can select a ranked prefix or range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationTopPolicy {
    Strict,
    FileRoundRobinV1,
    LineRoundRobinV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSelectionScope {
    ExplicitCandidates,
    RetainedCandidates,
    SampledCandidates,
}

impl VerificationSelectionScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitCandidates => "explicit_candidates",
            Self::RetainedCandidates => "retained_candidates",
            Self::SampledCandidates => "sampled_candidates",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid verification selection: {0}")]
pub struct VerificationSelectionError(&'static str);

/// Validated selection metadata with read-only mode, counts and provenance.
/// Reports may select zero candidates; sample evidence must be nonempty.
/// Unknown additive selection fields are accepted on input, but contradictory
/// known fields are rejected rather than repaired.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(try_from = "SelectionWire")]
pub struct VerificationSelection {
    requested: NonZeroUsize,
    selected: usize,
    kind: SelectionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SelectionKind {
    CandidateIds {
        plan_truncated: bool,
    },
    Top {
        policy: VerificationTopPolicy,
        plan_truncated: bool,
    },
    Sample(VerificationSampling),
}

impl VerificationSelection {
    /// Records explicit membership, including an empty selected set in a report.
    ///
    /// # Errors
    /// Rejects zero requested counts or selected counts above the request.
    pub fn candidate_ids(
        requested: usize,
        selected: usize,
        plan_truncated: bool,
    ) -> Result<Self, VerificationSelectionError> {
        Self::new(
            requested,
            selected,
            SelectionKind::CandidateIds { plan_truncated },
        )
    }

    /// Records a ranked selection with its ordering policy.
    ///
    /// # Errors
    /// Rejects zero requested counts or selected counts above the request.
    pub fn top(
        requested: usize,
        selected: usize,
        policy: VerificationTopPolicy,
        plan_truncated: bool,
    ) -> Result<Self, VerificationSelectionError> {
        Self::new(
            requested,
            selected,
            SelectionKind::Top {
                policy,
                plan_truncated,
            },
        )
    }

    /// Records sampling from a complete plan, deriving the count from its IDs.
    ///
    /// # Errors
    /// Rejects a zero request or a sample larger than the request.
    pub fn sample(
        requested: usize,
        sampling: VerificationSampling,
    ) -> Result<Self, VerificationSelectionError> {
        Self::new(
            requested,
            sampling.selected_ids.len(),
            SelectionKind::Sample(sampling),
        )
    }

    fn new(
        requested: usize,
        selected: usize,
        kind: SelectionKind,
    ) -> Result<Self, VerificationSelectionError> {
        let requested = NonZeroUsize::new(requested)
            .ok_or(VerificationSelectionError("requested must be positive"))?;
        if selected > requested.get() {
            return Err(VerificationSelectionError("selected exceeds requested"));
        }
        Ok(Self {
            requested,
            selected,
            kind,
        })
    }

    #[must_use]
    pub const fn requested(&self) -> usize {
        self.requested.get()
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub const fn mode(&self) -> VerificationSelectionMode {
        match self.kind {
            SelectionKind::CandidateIds { .. } => VerificationSelectionMode::CandidateIds,
            SelectionKind::Top { .. } => VerificationSelectionMode::Top,
            SelectionKind::Sample(_) => VerificationSelectionMode::Sample,
        }
    }

    #[must_use]
    pub const fn policy(&self) -> VerificationSelectionPolicy {
        match self.kind {
            SelectionKind::CandidateIds { .. } => VerificationSelectionPolicy::ExplicitCandidates,
            SelectionKind::Top { policy, .. } => match policy {
                VerificationTopPolicy::Strict => VerificationSelectionPolicy::Strict,
                VerificationTopPolicy::FileRoundRobinV1 => {
                    VerificationSelectionPolicy::FileRoundRobinV1
                }
                VerificationTopPolicy::LineRoundRobinV1 => {
                    VerificationSelectionPolicy::LineRoundRobinV1
                }
            },
            SelectionKind::Sample(_) => VerificationSelectionPolicy::Splitmix64FisherYatesV1,
        }
    }

    #[must_use]
    pub const fn scope(&self) -> VerificationSelectionScope {
        match self.kind {
            SelectionKind::CandidateIds { .. } => VerificationSelectionScope::ExplicitCandidates,
            SelectionKind::Top { .. } => VerificationSelectionScope::RetainedCandidates,
            SelectionKind::Sample(_) => VerificationSelectionScope::SampledCandidates,
        }
    }

    #[must_use]
    pub const fn plan_truncated(&self) -> bool {
        match self.kind {
            SelectionKind::CandidateIds { plan_truncated }
            | SelectionKind::Top { plan_truncated, .. } => plan_truncated,
            SelectionKind::Sample(_) => false,
        }
    }

    #[must_use]
    pub const fn sampling(&self) -> Option<&VerificationSampling> {
        match &self.kind {
            SelectionKind::Sample(sampling) => Some(sampling),
            SelectionKind::CandidateIds { .. } | SelectionKind::Top { .. } => None,
        }
    }
}

impl Serialize for VerificationSelection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut wire = serializer.serialize_struct(
            "VerificationSelection",
            if self.sampling().is_some() { 7 } else { 6 },
        )?;
        wire.serialize_field("mode", &self.mode())?;
        wire.serialize_field("policy", &self.policy())?;
        wire.serialize_field("requested", &self.requested())?;
        wire.serialize_field("selected", &self.selected())?;
        wire.serialize_field("scope", &self.scope())?;
        wire.serialize_field("plan_truncated", &self.plan_truncated())?;
        if let Some(sampling) = self.sampling() {
            wire.serialize_field("sampling", sampling)?;
        }
        wire.end()
    }
}

#[derive(Deserialize)]
struct SelectionWire {
    mode: VerificationSelectionMode,
    policy: VerificationSelectionPolicy,
    requested: usize,
    selected: usize,
    scope: VerificationSelectionScope,
    plan_truncated: bool,
    #[serde(default, deserialize_with = "present_sampling")]
    sampling: Option<VerificationSampling>,
}

fn present_sampling<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<VerificationSampling>, D::Error> {
    VerificationSampling::deserialize(deserializer).map(Some)
}

impl TryFrom<SelectionWire> for VerificationSelection {
    type Error = VerificationSelectionError;

    fn try_from(wire: SelectionWire) -> Result<Self, Self::Error> {
        if wire.mode != VerificationSelectionMode::Sample && wire.sampling.is_some() {
            return Err(VerificationSelectionError(
                "sampling is only permitted in sample mode",
            ));
        }
        let selection = match wire.mode {
            VerificationSelectionMode::CandidateIds => {
                Self::candidate_ids(wire.requested, wire.selected, wire.plan_truncated)?
            }
            VerificationSelectionMode::Top => {
                let policy = match wire.policy {
                    VerificationSelectionPolicy::Strict => VerificationTopPolicy::Strict,
                    VerificationSelectionPolicy::FileRoundRobinV1 => {
                        VerificationTopPolicy::FileRoundRobinV1
                    }
                    VerificationSelectionPolicy::LineRoundRobinV1 => {
                        VerificationTopPolicy::LineRoundRobinV1
                    }
                    _ => return Err(VerificationSelectionError("policy does not match top mode")),
                };
                Self::top(wire.requested, wire.selected, policy, wire.plan_truncated)?
            }
            VerificationSelectionMode::Sample => {
                if wire.plan_truncated {
                    return Err(VerificationSelectionError(
                        "sample requires a complete plan",
                    ));
                }
                let sampling = wire.sampling.ok_or(VerificationSelectionError(
                    "sample requires sampling evidence",
                ))?;
                let selection = Self::sample(wire.requested, sampling)?;
                if selection.selected != wire.selected {
                    return Err(VerificationSelectionError(
                        "selected count differs from sample IDs",
                    ));
                }
                selection
            }
        };
        if wire.policy != selection.policy() || wire.scope != selection.scope() {
            return Err(VerificationSelectionError(
                "policy or scope does not match mode",
            ));
        }
        Ok(selection)
    }
}

/// Nonempty, unique ordered dispatch IDs bounded by a positive population.
/// IDs may never reach execution; completion order is not their dispatch order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SamplingWire")]
pub struct VerificationSampling {
    population: usize,
    seed: u64,
    selected_ids: Vec<String>,
}

impl VerificationSampling {
    /// Validates sample provenance without changing the supplied ID order.
    ///
    /// # Errors
    /// Rejects zero populations, empty/oversized samples and duplicate IDs.
    pub fn new(
        population: usize,
        seed: u64,
        selected_ids: Vec<String>,
    ) -> Result<Self, VerificationSelectionError> {
        if population == 0 || selected_ids.is_empty() || selected_ids.len() > population {
            return Err(VerificationSelectionError(
                "sample size must be positive and bounded by population",
            ));
        }
        if selected_ids.iter().collect::<BTreeSet<_>>().len() != selected_ids.len() {
            return Err(VerificationSelectionError("sample IDs must be unique"));
        }
        Ok(Self {
            population,
            seed,
            selected_ids,
        })
    }

    #[must_use]
    pub const fn population(&self) -> usize {
        self.population
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub fn selected_ids(&self) -> &[String] {
        &self.selected_ids
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SamplingWire {
    population: usize,
    seed: u64,
    selected_ids: Vec<String>,
}

impl TryFrom<SamplingWire> for VerificationSampling {
    type Error = VerificationSelectionError;
    fn try_from(wire: SamplingWire) -> Result<Self, Self::Error> {
        Self::new(wire.population, wire.seed, wire.selected_ids)
    }
}
