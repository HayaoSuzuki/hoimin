use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;

use camino::Utf8Path;

use super::RankedPlanCandidate;
use crate::cli::TopSelectionPolicy;

#[cfg(test)]
pub(crate) fn select_top_candidate_ids(
    candidates: &[RankedPlanCandidate],
    count: NonZeroUsize,
    policy: TopSelectionPolicy,
) -> Vec<String> {
    select_top_candidate_ids_at(candidates, count, policy, 0)
}

pub(crate) fn select_top_candidate_ids_at(
    candidates: &[RankedPlanCandidate],
    count: NonZeroUsize,
    policy: TopSelectionPolicy,
    offset: usize,
) -> Vec<String> {
    if offset >= candidates.len() {
        return Vec::new();
    }
    let limit = count.get().min(candidates.len() - offset);
    let mut selected = Vec::with_capacity(limit);
    match policy {
        TopSelectionPolicy::Strict => selected.extend(
            candidates
                .iter()
                .skip(offset)
                .take(limit)
                .map(|candidate| candidate.id.clone()),
        ),
        TopSelectionPolicy::Diverse | TopSelectionPolicy::LineDiverse => selected.extend(
            DiverseCandidates::new(candidates, policy == TopSelectionPolicy::LineDiverse)
                .skip(offset)
                .take(limit)
                .map(|candidate| candidate.id.clone()),
        ),
    }
    selected
}

/// Yield the complete policy order without owning any IDs. Paging must consume
/// this order before cloning, so an offset never restarts the group rotation.
struct DiverseCandidates<'a> {
    remaining: &'a [RankedPlanCandidate],
    by_line: bool,
    active_groups: VecDeque<VecDeque<&'a RankedPlanCandidate>>,
}

impl<'a> DiverseCandidates<'a> {
    fn new(candidates: &'a [RankedPlanCandidate], by_line: bool) -> Self {
        Self {
            remaining: candidates,
            by_line,
            active_groups: VecDeque::new(),
        }
    }
}

impl<'a> Iterator for DiverseCandidates<'a> {
    type Item = &'a RankedPlanCandidate;

    fn next(&mut self) -> Option<Self::Item> {
        if self.active_groups.is_empty() {
            let first = self.remaining.first()?;
            let tier_end = self
                .remaining
                .iter()
                .position(|candidate| candidate.score != first.score)
                .unwrap_or(self.remaining.len());
            let (tier, remaining) = self.remaining.split_at(tier_end);
            self.remaining = remaining;
            self.active_groups = group_tier(tier, self.by_line);
        }
        let mut group = self.active_groups.pop_front()?;
        let candidate = group.pop_front().expect("active groups contain candidates");
        if !group.is_empty() {
            self.active_groups.push_back(group);
        }
        Some(candidate)
    }
}

fn group_tier(
    candidates: &[RankedPlanCandidate],
    by_line: bool,
) -> VecDeque<VecDeque<&RankedPlanCandidate>> {
    let mut group_index = HashMap::<(&Utf8Path, Option<u32>), usize>::new();
    let mut groups = Vec::<VecDeque<&RankedPlanCandidate>>::new();

    for candidate in candidates {
        let key = (candidate.path.as_path(), by_line.then_some(candidate.line));
        let index = if let Some(index) = group_index.get(&key) {
            *index
        } else {
            let index = groups.len();
            groups.push(VecDeque::new());
            group_index.insert(key, index);
            index
        };
        groups[index].push_back(candidate);
    }

    groups.into_iter().collect()
}
