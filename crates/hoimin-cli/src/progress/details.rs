use std::collections::{BTreeMap, HashSet};

use hoimin_core::{MutantFinished, MutationStatus};
use serde::Serialize;

use super::{UsableReport, compare::CandidateSetEligibility};

#[derive(Debug, Serialize)]
pub(super) struct Transition {
    id: String,
    path: String,
    line: u32,
    column: u32,
    previous_path: String,
    previous_line: u32,
    previous_column: u32,
    operator: String,
    classification: &'static str,
    previous_status: MutationStatus,
    current_status: MutationStatus,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct Changes {
    pub(super) omitted: usize,
    pub(super) unidentified: usize,
    pub(super) transitions: Vec<Transition>,
}

#[derive(Serialize)]
pub(super) struct Details {
    scope: &'static str,
    previous_input: usize,
    current_input: usize,
    available: bool,
    eligibility: Option<&'static str>,
    limit: usize,
    #[serde(flatten)]
    changes: Changes,
}

impl Details {
    pub(super) fn new(
        current_input: usize,
        limit: usize,
        eligibility: Option<CandidateSetEligibility>,
        changes: Changes,
    ) -> Self {
        Self {
            scope: "latest_pair",
            previous_input: current_input - 1,
            current_input,
            available: eligibility.is_some(),
            eligibility: eligibility.map(|value| match value {
                CandidateSetEligibility::Matching => "matching",
                CandidateSetEligibility::Different => "different",
                CandidateSetEligibility::Duplicate => "duplicate",
            }),
            limit,
            changes,
        }
    }
}

pub(super) struct Collector<'a> {
    limit: usize,
    identified: usize,
    unidentified: usize,
    duplicate_ids: HashSet<&'a str>,
    entries: BTreeMap<&'a str, (&'a MutantFinished, &'a MutantFinished)>,
}

impl<'a> Collector<'a> {
    pub(super) fn new(limit: usize, previous: &'a UsableReport, current: &'a UsableReport) -> Self {
        let mut duplicate_ids = HashSet::new();
        for report in [previous, current] {
            let mut seen = HashSet::new();
            for mutant in &report.mutants {
                if !seen.insert(mutant.candidate.id.as_str()) {
                    duplicate_ids.insert(mutant.candidate.id.as_str());
                }
            }
        }
        Self {
            limit,
            identified: 0,
            unidentified: 0,
            duplicate_ids,
            entries: BTreeMap::new(),
        }
    }

    pub(super) fn observe(&mut self, previous: &'a MutantFinished, current: &'a MutantFinished) {
        let id = previous.candidate.id.as_str();
        if id != current.candidate.id || self.duplicate_ids.contains(id) {
            self.unidentified += 1;
            return;
        }
        self.identified += 1;
        if self.limit == 0
            || (self.entries.len() == self.limit
                && self
                    .entries
                    .last_key_value()
                    .is_some_and(|(last, _)| id > *last))
        {
            return;
        }
        if self.entries.len() == self.limit {
            self.entries.pop_last();
        }
        self.entries.insert(id, (previous, current));
    }

    pub(super) fn finish(self) -> Changes {
        Changes {
            omitted: self.identified - self.entries.len(),
            unidentified: self.unidentified,
            transitions: self
                .entries
                .into_iter()
                .map(|(id, (previous, current))| Transition {
                    id: id.to_owned(),
                    path: current.candidate.path.to_string(),
                    line: current.candidate.line,
                    column: current.candidate.column,
                    previous_path: previous.candidate.path.to_string(),
                    previous_line: previous.candidate.line,
                    previous_column: previous.candidate.column,
                    operator: current.candidate.operator.clone(),
                    classification: if current.outcome.status() == MutationStatus::Killed {
                        "improvement"
                    } else {
                        "regression"
                    },
                    previous_status: previous.outcome.status(),
                    current_status: current.outcome.status(),
                })
                .collect(),
        }
    }
}
