use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
};

use camino::Utf8Path;
use hoimin_core::{MutantFinished, MutationCandidate, MutationStatus};

use super::{
    InputReport, UsableReport,
    details::{Changes, Collector},
};

#[derive(Debug, PartialEq)]
pub struct ProgressResult {
    pub comparisons: Vec<Comparison>,
    pub latest: ProgressState,
    pub consecutive_stalls: usize,
    pub patience: NonZeroUsize,
}

pub(crate) struct ProgressAccumulator {
    result: ProgressResult,
}

impl ProgressAccumulator {
    pub(crate) fn new(patience: NonZeroUsize) -> Self {
        Self {
            result: ProgressResult {
                comparisons: Vec::new(),
                latest: ProgressState::Indeterminate,
                consecutive_stalls: 0,
                patience,
            },
        }
    }

    pub(crate) fn advance(
        &mut self,
        previous: &InputReport,
        current: &InputReport,
    ) -> Option<CandidateSetEligibility> {
        self.advance_with_details(previous, current, None).0
    }

    pub(crate) fn advance_with_details(
        &mut self,
        previous: &InputReport,
        current: &InputReport,
        limit: Option<usize>,
    ) -> (Option<CandidateSetEligibility>, Changes) {
        let (InputReport::Usable(previous), InputReport::Usable(current)) = (previous, current)
        else {
            self.result.consecutive_stalls = 0;
            self.result.latest = ProgressState::Indeterminate;
            return (None, Changes::default());
        };

        let eligibility = candidate_set_eligibility(previous, current);
        let (comparison, changes) = compare_usable_reports(previous, current, eligibility, limit);
        match comparison.state {
            ProgressState::Improving => {
                self.result.consecutive_stalls = 0;
                self.result.latest = ProgressState::Improving;
            }
            ProgressState::Regressing => {
                self.result.consecutive_stalls = 0;
                self.result.latest = ProgressState::Regressing;
            }
            ProgressState::Stalled => {
                self.result.consecutive_stalls += 1;
                self.result.latest = if self.result.consecutive_stalls >= self.result.patience.get()
                {
                    ProgressState::Saturated
                } else {
                    ProgressState::Stalled
                };
            }
            ProgressState::Indeterminate => {
                self.result.consecutive_stalls = 0;
                self.result.latest = ProgressState::Indeterminate;
            }
            ProgressState::Saturated => unreachable!("individual comparisons cannot saturate"),
        }
        self.result.comparisons.push(comparison);
        (Some(eligibility), changes)
    }

    pub(crate) fn into_result(self) -> ProgressResult {
        self.result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    pub common: usize,
    pub added: usize,
    pub removed: usize,
    pub ambiguous: usize,
    pub inconclusive: usize,
    pub improvements: usize,
    pub regressions: usize,
    pub carried_survivors: usize,
    pub previous_score: Option<f64>,
    pub current_score: Option<f64>,
    pub score_delta: Option<f64>,
    pub state: ProgressState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CandidateSetEligibility {
    Matching,
    Different,
    Duplicate,
}

impl CandidateSetEligibility {
    fn is_matching(self) -> bool {
        self == Self::Matching
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressState {
    Improving,
    Regressing,
    Stalled,
    Saturated,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct MutantKey<'a> {
    path: &'a Utf8Path,
    original: &'a str,
    replacement: &'a str,
    operator: &'a str,
    symbol: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ComparisonKey<'a> {
    CandidateId(&'a str),
    Content(MutantKey<'a>),
}

struct ReportMutants<'a> {
    unique: HashMap<ComparisonKey<'a>, &'a MutantFinished>,
    duplicates: HashSet<ComparisonKey<'a>>,
}

#[must_use]
pub fn compare_reports(reports: &[InputReport], patience: NonZeroUsize) -> ProgressResult {
    let mut accumulator = ProgressAccumulator::new(patience);

    for pair in reports.windows(2) {
        let [previous, current] = pair else {
            unreachable!("windows(2) always yields two elements");
        };
        accumulator.advance(previous, current);
    }

    accumulator.into_result()
}

fn compare_usable_reports(
    previous: &UsableReport,
    current: &UsableReport,
    candidate_set_eligibility: CandidateSetEligibility,
    limit: Option<usize>,
) -> (Comparison, Changes) {
    let mut details = limit.map(|limit| Collector::new(limit, previous, current));
    let previous = index_mutants(&previous.mutants, candidate_set_eligibility);
    let current = index_mutants(&current.mutants, candidate_set_eligibility);
    let ambiguous: HashSet<_> = previous
        .duplicates
        .union(&current.duplicates)
        .copied()
        .collect();

    let mut common = 0;
    let mut added = 0;
    let mut removed = 0;
    let mut inconclusive = HashSet::new();
    let mut improvements = 0;
    let mut regressions = 0;
    let mut carried_survivors = 0;
    let mut comparable_common = 0;
    let mut previous_killed = 0;
    let mut previous_survived = 0;
    let mut current_killed = 0;
    let mut current_survived = 0;

    for (key, mutant) in &previous.unique {
        if ambiguous.contains(key) {
            continue;
        }

        if is_inconclusive(mutant.status) {
            inconclusive.insert(*key);
        }

        let Some(next) = current.unique.get(key) else {
            removed += 1;
            continue;
        };
        common += 1;

        if is_inconclusive(next.status) {
            inconclusive.insert(*key);
        }
        if !is_conclusive(mutant.status) || !is_conclusive(next.status) {
            continue;
        }

        comparable_common += 1;
        previous_killed += usize::from(mutant.status == MutationStatus::Killed);
        previous_survived += usize::from(mutant.status == MutationStatus::Survived);
        current_killed += usize::from(next.status == MutationStatus::Killed);
        current_survived += usize::from(next.status == MutationStatus::Survived);
        if mutant.status != next.status
            && let Some(details) = details.as_mut()
        {
            details.observe(mutant, next);
        }
        match (mutant.status, next.status) {
            (MutationStatus::Survived, MutationStatus::Killed) => improvements += 1,
            (MutationStatus::Killed, MutationStatus::Survived) => regressions += 1,
            (MutationStatus::Survived, MutationStatus::Survived) => carried_survivors += 1,
            (MutationStatus::Killed, MutationStatus::Killed) => {}
            _ => unreachable!("inconclusive mutants are excluded above"),
        }
    }

    for (key, mutant) in &current.unique {
        if ambiguous.contains(key) {
            continue;
        }
        if is_inconclusive(mutant.status) {
            inconclusive.insert(*key);
        }
        if !previous.unique.contains_key(key) {
            added += 1;
        }
    }

    let previous_score = score(previous_killed, previous_survived);
    let current_score = score(current_killed, current_survived);
    let score_delta = previous_score
        .zip(current_score)
        .map(|(before, after)| after - before);
    let state = comparison_state(
        candidate_set_eligibility,
        comparable_common,
        regressions,
        improvements,
    );

    (
        Comparison {
            common,
            added,
            removed,
            ambiguous: ambiguous.len(),
            inconclusive: inconclusive.len(),
            improvements,
            regressions,
            carried_survivors,
            previous_score,
            current_score,
            score_delta,
            state,
        },
        details.map_or_else(Changes::default, Collector::finish),
    )
}

pub(crate) fn candidate_set_eligibility(
    previous: &UsableReport,
    current: &UsableReport,
) -> CandidateSetEligibility {
    fn ids(report: &UsableReport) -> Option<HashSet<&str>> {
        let ids = report
            .mutants
            .iter()
            .map(|mutant| mutant.candidate.id.as_str())
            .collect::<HashSet<_>>();
        (ids.len() == report.mutants.len()).then_some(ids)
    }

    let (Some(previous), Some(current)) = (ids(previous), ids(current)) else {
        return CandidateSetEligibility::Duplicate;
    };
    if previous == current {
        CandidateSetEligibility::Matching
    } else {
        CandidateSetEligibility::Different
    }
}

fn comparison_state(
    eligibility: CandidateSetEligibility,
    comparable_common: usize,
    regressions: usize,
    improvements: usize,
) -> ProgressState {
    if !eligibility.is_matching() || comparable_common == 0 {
        return ProgressState::Indeterminate;
    }
    if regressions > 0 {
        ProgressState::Regressing
    } else if improvements > 0 {
        ProgressState::Improving
    } else {
        ProgressState::Stalled
    }
}

fn index_mutants(
    mutants: &[MutantFinished],
    eligibility: CandidateSetEligibility,
) -> ReportMutants<'_> {
    let mut unique = HashMap::new();
    let mut duplicates = HashSet::new();

    for mutant in mutants {
        match eligibility {
            CandidateSetEligibility::Matching => {
                unique.insert(ComparisonKey::CandidateId(&mutant.candidate.id), mutant);
            }
            CandidateSetEligibility::Different | CandidateSetEligibility::Duplicate => {
                let key = ComparisonKey::Content(key(&mutant.candidate));
                if unique.insert(key, mutant).is_some() {
                    duplicates.insert(key);
                }
            }
        }
    }

    ReportMutants { unique, duplicates }
}

fn key(candidate: &MutationCandidate) -> MutantKey<'_> {
    MutantKey {
        path: &candidate.path,
        original: &candidate.original,
        replacement: &candidate.replacement,
        operator: &candidate.operator,
        symbol: candidate.symbol.as_deref(),
    }
}

fn is_conclusive(status: MutationStatus) -> bool {
    matches!(status, MutationStatus::Killed | MutationStatus::Survived)
}

fn is_inconclusive(status: MutationStatus) -> bool {
    !is_conclusive(status)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "The progress schema models scores as f64, so this calculation must preserve that output type."
)]
fn score(killed: usize, survived: usize) -> Option<f64> {
    let denominator = killed + survived;
    (denominator > 0).then(|| killed as f64 / denominator as f64)
}

#[cfg(test)]
mod detail_tests {
    use super::*;

    fn report(entries: &[(&str, &str, MutationStatus)]) -> UsableReport {
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/golden/reports/schema-v3-current.json"
        ))
        .unwrap();
        let template: MutantFinished = serde_json::from_value(value["mutants"][0].clone()).unwrap();
        UsableReport {
            source: "internal.json".into(),
            mutants: entries
                .iter()
                .map(|(id, body, status)| {
                    let mut event = template.clone();
                    event.candidate.id = (*id).to_owned();
                    event.candidate.original = (*body).to_owned();
                    event.status = *status;
                    event
                })
                .collect(),
        }
    }

    #[test]
    fn detail_exclusions_follow_real_ambiguous_and_inconclusive_comparison_paths() {
        use MutationStatus::{Error, Killed, NotRun, OutOfMemory, ProcessLimit, Survived, Timeout};
        for status in [Timeout, Error, NotRun, OutOfMemory, ProcessLimit] {
            let before = report(&[("a", "body", Survived)]);
            let after = report(&[("a", "body", status)]);
            let (comparison, changes) = compare_usable_reports(
                &before,
                &after,
                candidate_set_eligibility(&before, &after),
                Some(100),
            );
            assert_eq!(comparison.inconclusive, 1);
            assert_eq!(changes.transitions.len(), 0);
            assert_eq!(changes.unidentified, 0);
        }
        // Content fallback is ambiguous, including the otherwise identical ID.
        let before = report(&[("a", "body", Killed), ("b", "body", Killed)]);
        let after = report(&[("a", "body", Survived)]);
        let (comparison, changes) = compare_usable_reports(
            &before,
            &after,
            candidate_set_eligibility(&before, &after),
            Some(100),
        );
        assert_eq!(comparison.ambiguous, 1);
        assert!(changes.transitions.is_empty());
        // Duplicate IDs with distinct content can count a change but cannot identify it.
        let before = report(&[("a", "body1", Killed), ("a", "body2", Killed)]);
        let after = report(&[("a", "body1", Survived)]);
        let (comparison, changes) = compare_usable_reports(
            &before,
            &after,
            candidate_set_eligibility(&before, &after),
            Some(100),
        );
        assert_eq!(comparison.regressions, 1);
        assert_eq!(changes.unidentified, 1);
        assert!(changes.transitions.is_empty());
    }

    #[test]
    fn internal_details_match_lean_generated_inconclusive_and_duplicate_cases() {
        let corpus = include_str!("../../../../formal/HoiminOracle/corpus/progress-decision.jsonl");
        for line in corpus.lines() {
            let case: serde_json::Value = serde_json::from_str(line).unwrap();
            if case["mode"] == "strict" {
                continue;
            }
            let reports = case["reports"].as_array().unwrap();
            let build = |value: &serde_json::Value| {
                let owned: Vec<_> = value["mutants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| {
                        (
                            format!("candidate-{}", entry["candidate_id"].as_u64().unwrap()),
                            format!("body-{}", entry["content_key"].as_u64().unwrap()),
                            serde_json::from_value::<MutationStatus>(entry["status"].clone())
                                .unwrap(),
                        )
                    })
                    .collect();
                let borrowed: Vec<_> = owned
                    .iter()
                    .map(|(id, body, status)| (id.as_str(), body.as_str(), *status))
                    .collect();
                report(&borrowed)
            };
            let previous = build(&reports[reports.len() - 2]);
            let current = build(&reports[reports.len() - 1]);
            let (_, changes) = compare_usable_reports(
                &previous,
                &current,
                candidate_set_eligibility(&previous, &current),
                Some(1),
            );
            let actual = serde_json::to_value(changes).unwrap();
            let expected = &case["expected"]["details"];
            assert_eq!(actual["omitted"], expected["omitted"], "{}", case["id"]);
            assert_eq!(
                actual["unidentified"], expected["unidentified"],
                "{}",
                case["id"]
            );
            assert_eq!(
                actual["transitions"].as_array().unwrap().len(),
                expected["transitions"].as_array().unwrap().len()
            );
            for (actual, expected) in actual["transitions"]
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["transitions"].as_array().unwrap())
            {
                assert_eq!(
                    actual["id"],
                    format!("candidate-{}", expected["candidate_id"].as_u64().unwrap())
                );
                for field in ["previous_status", "current_status", "classification"] {
                    assert_eq!(actual[field], expected[field]);
                }
            }
        }
    }
}
