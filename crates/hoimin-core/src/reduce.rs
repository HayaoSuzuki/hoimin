//! Admission and publication of evidence for one immutable reduction session.
//! Identity numbers are session-owned handles bound to immutable bytes/settings;
//! they must never be reused for different source, command, or variant contents.
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[must_use]
pub fn median(mut times: [u128; 3]) -> u128 {
    times.sort_unstable();
    times[1]
}

#[must_use]
pub const fn valid_rate(num: u128, den: u128) -> bool {
    num > 0 && num < den
}

/// Exact comparison, including full-width ticks; no rounded discount or overflow.
#[must_use]
pub fn pair_pass(num: u128, den: u128, original: u128, reduced: u128) -> bool {
    valid_rate(num, den)
        && original > 0
        && reduced > 0
        && BigUint::from(den) * BigUint::from(reduced)
            <= BigUint::from(den - num) * BigUint::from(original)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Span {
    pub id: u64,
    pub lo: u64,
    pub hi: u64,
}
impl Span {
    #[must_use]
    pub const fn overlaps(&self, other: &Self) -> bool {
        self.lo < self.hi && other.lo < other.hi && self.lo < other.hi && other.lo < self.hi
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Killed,
    Survived,
    Inconclusive,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ResultEntry {
    pub id: u64,
    pub outcome: Outcome,
}

fn result_map(results: &[ResultEntry]) -> Option<BTreeMap<u64, Outcome>> {
    let map: BTreeMap<_, _> = results.iter().map(|r| (r.id, r.outcome)).collect();
    (map.len() == results.len() && results.iter().all(|r| r.outcome != Outcome::Inconclusive))
        .then_some(map)
}
/// Requires a complete unique outcome for every fixed selected ID, in any order.
#[must_use]
pub fn exact_results(selected: &[u64], before: &[ResultEntry], after: &[ResultEntry]) -> bool {
    let ids: BTreeSet<_> = selected.iter().copied().collect();
    !ids.is_empty()
        && ids.len() == selected.len()
        && result_map(before).is_some_and(|b| {
            b.keys().copied().collect::<BTreeSet<_>>() == ids
                && result_map(after).as_ref() == Some(&b)
        })
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Identity {
    pub source: u128,
    pub plan: u128,
    pub command: u128,
    pub environment: u128,
    pub timeout: u128,
    pub variant: u64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Triple {
    pub a: u128,
    pub b: u128,
    pub c: u128,
}
impl Triple {
    #[must_use]
    pub const fn values(self) -> [u128; 3] {
        [self.a, self.b, self.c]
    }
}
impl From<[u128; 3]> for Triple {
    fn from([a, b, c]: [u128; 3]) -> Self {
        Self { a, b, c }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub original: Vec<u64>,
    pub kept_spans: Vec<Span>,
    pub selected: Vec<u64>,
    pub before: Vec<ResultEntry>,
    pub identity: Identity,
    pub baseline_pass: bool,
    pub source_disjoint: bool,
    pub selectors_unique: bool,
    pub numerator: u128,
    pub denominator: u128,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: u64,
    pub retained: Vec<u64>,
    pub removed: Vec<Span>,
    pub after: Vec<ResultEntry>,
    pub evidence: Identity,
    pub baseline_pass: bool,
    pub exploration: Triple,
    pub original_pairs: Triple,
    pub reduced_pairs: Triple,
    pub pair_count: usize,
    pub completed_at: u128,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent diagnostic observations match the Lean certification contract"
)]
pub struct Checks {
    pub certified: bool,
    pub config_valid: bool,
    pub exact_results: bool,
    pub exploration_median: u128,
    pub final_pairs_pass: bool,
    pub measurements_valid: bool,
    pub same_evidence: bool,
    pub scope_valid: bool,
}
impl Context {
    #[must_use]
    pub fn valid(&self) -> bool {
        valid_rate(self.numerator, self.denominator)
            && self.baseline_pass
            && self.source_disjoint
            && self.selectors_unique
            && unique(&self.original)
            && exact_results(&self.selected, &self.before, &self.before)
            && self.before.iter().any(|r| r.outcome == Outcome::Killed)
    }
    /// Checks evidence for exactly this source/plan/command/environment/timeout/variant.
    #[must_use]
    pub fn check(&self, p: &Proposal) -> Checks {
        let config_valid = self.valid();
        let removed: Vec<_> = p.removed.iter().map(|s| s.id).collect();
        let mut partition = p.retained.clone();
        partition.extend(&removed);
        partition.sort_unstable();
        let mut original = self.original.clone();
        original.sort_unstable();
        let scope_valid = !removed.is_empty()
            && unique(&p.retained)
            && unique(&removed)
            && partition == original
            && p.removed.iter().all(|s| s.lo < s.hi)
            && self
                .kept_spans
                .iter()
                .all(|k| p.retained.contains(&k.id) && p.removed.iter().all(|r| !r.overlaps(k)));
        let mut identity = self.identity.clone();
        identity.variant = p.id;
        let same_evidence = identity == p.evidence;
        let exact_results = exact_results(&self.selected, &self.before, &p.after);
        let measurements_valid = p.measurements_valid();
        let final_pairs_pass = p
            .original_pairs
            .values()
            .into_iter()
            .zip(p.reduced_pairs.values())
            .all(|(o, r)| pair_pass(self.numerator, self.denominator, o, r));
        Checks {
            certified: config_valid
                && scope_valid
                && same_evidence
                && p.baseline_pass
                && exact_results
                && measurements_valid
                && p.pair_count == 3
                && final_pairs_pass,
            config_valid,
            scope_valid,
            same_evidence,
            exact_results,
            measurements_valid,
            final_pairs_pass,
            exploration_median: median(p.exploration.values()),
        }
    }
}
fn unique(ids: &[u64]) -> bool {
    ids.iter().collect::<BTreeSet<_>>().len() == ids.len()
}
impl Proposal {
    #[must_use]
    pub fn measurements_valid(&self) -> bool {
        self.exploration.values().into_iter().all(|t| t > 0)
            && self
                .original_pairs
                .values()
                .into_iter()
                .take(self.pair_count)
                .all(|t| t > 0)
            && self
                .reduced_pairs
                .values()
                .into_iter()
                .take(self.pair_count)
                .all(|t| t > 0)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    pub tick: u128,
    pub proposal: Option<Proposal>,
    pub stop: bool,
}
/// Owns only certified best evidence. Stop is absorbing, including clock reversal
/// and simultaneous completion/deadline; an earlier certified best survives.
#[derive(Clone, Debug, Serialize)]
pub struct State {
    pub now: u128,
    pub deadline: u128,
    pub stopped: bool,
    pub best: Option<Proposal>,
}
impl State {
    #[must_use]
    pub const fn new(deadline: u128) -> Self {
        Self {
            now: 0,
            deadline,
            stopped: false,
            best: None,
        }
    }
    pub fn step(&mut self, context: &Context, event: Event) {
        if self.stopped {
            return;
        }
        if !context.valid() || event.tick < self.now || event.tick >= self.deadline || event.stop {
            self.now = self.now.max(event.tick);
            self.stopped = true;
            return;
        }
        self.now = event.tick;
        if let Some(p) = event.proposal {
            if !p.measurements_valid() {
                self.stopped = true;
            } else if p.completed_at == event.tick
                && context.check(&p).certified
                && self.best.as_ref().is_none_or(|old| {
                    median(p.exploration.values()) < median(old.exploration.values())
                })
            {
                self.best = Some(p);
            }
        }
    }
}
