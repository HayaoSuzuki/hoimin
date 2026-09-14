use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;

use camino::Utf8Path;

use super::RankedPlanCandidate;
use crate::cli::TopSelectionPolicy;

pub(crate) fn select_top_candidate_ids(
    candidates: &[RankedPlanCandidate],
    count: NonZeroUsize,
    policy: TopSelectionPolicy,
) -> Vec<String> {
    let limit = count.get().min(candidates.len());
    match policy {
        TopSelectionPolicy::Strict => candidates
            .iter()
            .take(limit)
            .map(|candidate| candidate.id.clone())
            .collect(),
        TopSelectionPolicy::Diverse => select_diverse_candidate_ids(candidates, limit),
    }
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
    let prefix = offset.saturating_add(count.get()).min(candidates.len());
    select_top_candidate_ids(candidates, NonZeroUsize::new(prefix).unwrap(), policy)
        .into_iter()
        .skip(offset)
        .collect()
}

fn select_diverse_candidate_ids(candidates: &[RankedPlanCandidate], limit: usize) -> Vec<String> {
    let mut selected = Vec::with_capacity(limit);
    let mut tier_start = 0;

    while selected.len() < limit && tier_start < candidates.len() {
        let tier_score = candidates[tier_start].score;
        let tier_end = candidates[tier_start..]
            .iter()
            .position(|candidate| candidate.score != tier_score)
            .map_or(candidates.len(), |offset| tier_start + offset);
        select_from_tier(&candidates[tier_start..tier_end], limit, &mut selected);
        tier_start = tier_end;
    }

    selected
}

fn select_from_tier(candidates: &[RankedPlanCandidate], limit: usize, selected: &mut Vec<String>) {
    let mut group_index = HashMap::<&Utf8Path, usize>::new();
    let mut groups = Vec::<VecDeque<&RankedPlanCandidate>>::new();

    for candidate in candidates {
        let index = if let Some(index) = group_index.get(candidate.path.as_path()) {
            *index
        } else {
            let index = groups.len();
            groups.push(VecDeque::new());
            group_index.insert(candidate.path.as_path(), index);
            index
        };
        groups[index].push_back(candidate);
    }

    let mut active_groups = groups
        .into_iter()
        .filter(|group| !group.is_empty())
        .collect::<VecDeque<_>>();

    while selected.len() < limit {
        let Some(mut group) = active_groups.pop_front() else {
            return;
        };
        let candidate = group.pop_front().expect("active groups contain candidates");
        selected.push(candidate.id.clone());
        if !group.is_empty() {
            active_groups.push_back(group);
        }
    }
}
