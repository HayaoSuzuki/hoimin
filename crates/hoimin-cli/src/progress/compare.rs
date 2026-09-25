use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
};

use camino::Utf8Path;
use hoimin_core::{MutantFinished, MutationCandidate, MutationStatus};

use super::{InputReport, UsableReport};

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
        let (InputReport::Usable(previous), InputReport::Usable(current)) = (previous, current)
        else {
            self.result.consecutive_stalls = 0;
            self.result.latest = ProgressState::Indeterminate;
            return None;
        };

        let eligibility = candidate_set_eligibility(previous, current);
        let comparison = compare_usable_reports(previous, current, eligibility);
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
        Some(eligibility)
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
) -> Comparison {
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
        match mutant.status {
            MutationStatus::Killed => previous_killed += 1,
            MutationStatus::Survived => previous_survived += 1,
            _ => unreachable!("inconclusive mutants are excluded above"),
        }
        match next.status {
            MutationStatus::Killed => current_killed += 1,
            MutationStatus::Survived => current_survived += 1,
            _ => unreachable!("inconclusive mutants are excluded above"),
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
    }
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
