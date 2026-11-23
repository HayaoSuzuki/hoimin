//! Width constraints for count edits, built while the regex grammar is scanned.
//!
//! A valid lookbehind has fixed width. Its contributing concatenations, branches,
//! consuming repeats, and referenced captures therefore also have fixed width.
//! Changing one such repeat must preserve its fixed width and every dependent
//! branch equality. The model tracks those dependencies through shared captures.
#[path = "width/limits.rs"]
mod limits;

use limits::{Error, WidthLimits};
use std::collections::{BTreeSet, HashMap};

const MAX_WIDTH: u64 = u32::MAX as u64;
const SPARSE_ALLOCATIONS_PER_NODE: usize = 4;
type NodeId = usize;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(super) struct Interval {
    min: u64,
    max: u64,
}
impl Interval {
    const ONE: Self = Self { min: 1, max: 1 };

    fn add(self, other: Self) -> Self {
        Self {
            min: self.min.saturating_add(other.min),
            max: self.max.saturating_add(other.max),
        }
    }

    fn branch(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub(super) fn repeat(self, lo: u32, hi: u32) -> Self {
        Self {
            min: self.min.saturating_mul(lo.into()),
            max: if hi == u32::MAX && self.max != 0 {
                u64::MAX
            } else {
                self.max.saturating_mul(hi.into())
            },
        }
    }

    fn fixed(self) -> bool {
        self.min == self.max && self.max <= MAX_WIDTH
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(super) struct Width {
    pub interval: Interval,
    node: Option<NodeId>,
}
impl Width {
    pub(super) const ONE: Self = Self {
        interval: Interval::ONE,
        node: None,
    };
}

#[derive(Clone, Copy)]
enum Rule {
    Sum(Width, Width),
    Branch(Width, Width),
    Repeat { atom: Width, lo: u32, hi: u32 },
}
impl Rule {
    fn inputs(self) -> [Option<NodeId>; 2] {
        match self {
            Self::Sum(a, b) | Self::Branch(a, b) => {
                [a.node, b.node.filter(|id| Some(*id) != a.node)]
            }
            Self::Repeat { atom, hi, .. } => [atom.node.filter(|_| hi != 0), None],
        }
    }

    /// Compute the change per unit change in a single count's width. An
    /// unequal branch or a variable repeat of nonempty text invalidates the
    /// fixed-width constraint, independent of the sign of that change.
    fn slope(self, get: impl Fn(Option<NodeId>) -> u64) -> Option<u64> {
        match self {
            Self::Sum(a, b) => Some(get(a.node).saturating_add(get(b.node))),
            Self::Branch(a, b) => {
                let a = get(a.node);
                (a == get(b.node)).then_some(a)
            }
            Self::Repeat { atom, lo, hi } => {
                let change = get(atom.node);
                (hi == 0 || lo == hi || change == 0).then(|| change.saturating_mul(lo.into()))
            }
        }
    }

    /// Express this output as a linear combination, assuming the mutant
    /// satisfies the branch equalities and zero-width requirements.
    fn linear_terms(self) -> [(Option<NodeId>, u32); 2] {
        let empty = (None, 0);
        match self {
            Self::Sum(a, b) => [(a.node, 1), (b.node, 1)],
            Self::Branch(a, _) => [(a.node, 1), empty],
            Self::Repeat { atom, lo, hi } if lo == hi => [(atom.node, lo), empty],
            Self::Repeat { .. } => [empty; 2],
        }
    }

    fn guards_inputs(self) -> bool {
        match self {
            Self::Sum(..) => false,
            Self::Branch(..) => true,
            Self::Repeat { lo, hi, .. } => lo != hi,
        }
    }

    // Project every branch equality and every variable repeat's zero-width
    // requirement into a single modular sum. A nonzero coefficient proves
    // there is a violated requirement. Zero still needs an exact query.
    fn accumulate_guards(self, output: u64, guard: u64, guards: &mut [u64]) {
        let mut add = |input: Option<NodeId>, value: u64| {
            if let Some(id) = input {
                guards[id] = guards[id].wrapping_add(value);
            }
        };
        for (input, multiplier) in self.linear_terms() {
            add(input, output.wrapping_mul(multiplier.into()));
        }
        match self {
            Self::Branch(a, b) => {
                add(a.node, guard);
                add(b.node, guard.wrapping_neg());
            }
            Self::Repeat { atom, lo, hi } if lo != hi => add(atom.node, guard),
            _ => {}
        }
    }
}

// Mix node IDs to keep independently generated branch requirements from
// routinely cancelling. Collisions only cause extra exact work.
fn guard_weight(id: NodeId) -> u64 {
    let mut n = u64::try_from(id)
        .expect("width node ID")
        .wrapping_add(0x9e37_79b9_7f4a_7c15);
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (n ^ (n >> 31)) | 1
}

struct Node {
    rule: Rule,
    interval: Interval,
}

#[derive(Default)]
pub(super) struct Model {
    nodes: Vec<Node>,
    assertions: Vec<NodeId>,
}
impl Model {
    fn record(&mut self, interval: Interval, rule: Rule, track: bool) -> Width {
        let node = track.then(|| {
            let id = self.nodes.len();
            self.nodes.push(Node { rule, interval });
            id
        });
        Width { interval, node }
    }

    pub(super) fn add(&mut self, a: Width, b: Width) -> Width {
        if a == Width::default() {
            return b;
        }
        if b == Width::default() {
            return a;
        }
        self.record(
            a.interval.add(b.interval),
            Rule::Sum(a, b),
            a.node.is_some() || b.node.is_some(),
        )
    }

    pub(super) fn branch(&mut self, a: Width, b: Width) -> Width {
        if a == b {
            return a;
        }
        self.record(
            a.interval.branch(b.interval),
            Rule::Branch(a, b),
            a.node.is_some() || b.node.is_some(),
        )
    }

    pub(super) fn repeat(&mut self, atom: Width, lo: u32, hi: u32, track: bool) -> Width {
        self.record(
            atom.interval.repeat(lo, hi),
            Rule::Repeat { atom, lo, hi },
            track,
        )
    }

    pub(super) fn lookbehind(&mut self, width: Width) -> Option<()> {
        if let Some(node) = width.node {
            self.assertions.push(node);
        }
        width.interval.fixed().then_some(())
    }

    /// Compile growth limits across all assertion paths before querying edits.
    /// Only unresolved equality guards on shared paths need an exact graph
    /// query, cached at the point where paths split.
    pub(super) fn finish(self, cancelled: &dyn Fn() -> bool) -> Option<Constraints> {
        if self.assertions.is_empty() {
            return Some(Constraints::default());
        }
        let len = self.nodes.len();
        let growth =
            self.growth_limits(len.saturating_mul(SPARSE_ALLOCATIONS_PER_NODE), cancelled)?;
        let mut constraints = Constraints {
            nodes: self.nodes,
            ..Constraints::default()
        };
        let mut active = vec![false; len];
        let mut guarded = vec![false; len];
        constraints.plans.resize(len, Plan::Unused);
        constraints.parents.resize_with(len, Vec::new);
        constraints.guards.resize(len, 0);
        for node in self.assertions {
            active[node] = true;
        }
        for id in (0..len).rev() {
            if cancelled() {
                return None;
            }
            if !active[id] {
                continue;
            }
            let node = &constraints.nodes[id];
            let slack = (MAX_WIDTH - node.interval.max).min(growth[id]);
            constraints.plans[id] = match constraints.parents[id].as_slice() {
                [] => Plan::new(slack, None),
                [parent] => {
                    let rule = constraints.nodes[*parent].rule;
                    match rule.slope(|child| u64::from(child == Some(id))) {
                        Some(scale) => constraints.plans[*parent].upstream(slack, scale),
                        None => Plan::Fixed,
                    }
                }
                _ => Plan::new(slack, guarded[id].then_some((id, 1))),
            };
            let guards_inputs = guarded[id] || node.rule.guards_inputs();
            for child in node.rule.inputs().into_iter().flatten() {
                active[child] = true;
                guarded[child] |= guards_inputs;
                constraints.parents[child].push(id);
            }
            let output = constraints.guards[id];
            node.rule
                .accumulate_guards(output, guard_weight(id), &mut constraints.guards);
        }
        Some(constraints)
    }

    /// The sparse pass combines assertion paths with shared storage. Dense
    /// overlap switches to a streamed pass to keep scratch memory linear.
    /// Both compute the same bound across all distinct assertions.
    fn growth_limits(&self, allocations: usize, cancelled: &dyn Fn() -> bool) -> Option<Vec<u64>> {
        let mut seen = vec![false; self.nodes.len()];
        let mut assertions = Vec::new();
        for &node in &self.assertions {
            if cancelled() {
                return None;
            }
            if !std::mem::replace(&mut seen[node], true) {
                assertions.push(node);
            }
        }
        match self.sparse_growth_limits(&assertions, allocations, cancelled) {
            Ok(limits) => Some(limits),
            Err(Error::AllocationLimit) => self.stream_growth_limits(&assertions, cancelled),
            Err(Error::Cancelled) => None,
        }
    }

    fn sparse_growth_limits(
        &self,
        assertions: &[NodeId],
        mut allocations: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<u64>, Error> {
        let len = self.nodes.len();
        let mut limits = vec![WidthLimits::default(); len];
        let mut growth = vec![u64::MAX; len];
        let bits = assertions.len().saturating_sub(1).bit_width();
        for (key, &node) in assertions.iter().enumerate() {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let remaining = MAX_WIDTH - self.nodes[node].interval.max;
            limits[node] = WidthLimits::assertion(key, bits, remaining, &mut allocations)?;
        }
        for id in (0..len).rev() {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let current = std::mem::take(&mut limits[id]);
            if current.is_empty() {
                continue;
            }
            growth[id] = current.allowance();
            for (child, multiplier) in self.nodes[id].rule.linear_terms() {
                if let Some(child) = child
                    && multiplier != 0
                {
                    let incoming = current.scaled(multiplier.into());
                    let existing = std::mem::take(&mut limits[child]);
                    limits[child] = existing.merge(incoming, &mut allocations, cancelled)?;
                }
            }
        }
        Ok(growth)
    }

    fn stream_growth_limits(
        &self,
        assertions: &[NodeId],
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<u64>> {
        let len = self.nodes.len();
        let mut growth = vec![u64::MAX; len];
        let mut incoming = vec![0u64; len];
        let mut pending = BTreeSet::new();
        for &assertion in assertions {
            let remaining = MAX_WIDTH - self.nodes[assertion].interval.max;
            incoming[assertion] = 1;
            pending.insert(assertion);
            while let Some(id) = pending.pop_last() {
                if cancelled() {
                    return None;
                }
                let paths = std::mem::take(&mut incoming[id]);
                growth[id] = growth[id].min(remaining / paths);
                for (child, multiplier) in self.nodes[id].rule.linear_terms() {
                    if let Some(child) = child
                        && multiplier != 0
                    {
                        let paths = paths.saturating_mul(multiplier.into());
                        incoming[child] = incoming[child].saturating_add(paths);
                        pending.insert(child);
                    }
                }
            }
        }
        Some(growth)
    }
}

#[derive(Clone, Copy)]
enum Plan {
    Unused,
    Fixed,
    Bounded {
        increase: u64,
        shared: Option<(NodeId, u64)>,
    },
}
impl Plan {
    fn new(increase: u64, shared: Option<(NodeId, u64)>) -> Self {
        Self::Bounded { increase, shared }
    }

    fn upstream(self, slack: u64, scale: u64) -> Self {
        if scale == 0 {
            return Self::new(slack, None);
        }
        match self {
            Self::Bounded { increase, shared } => Self::new(
                slack.min(increase / scale),
                shared.map(|(entry, factor)| (entry, factor.saturating_mul(scale))),
            ),
            other => other,
        }
    }
}

#[derive(Default)]
pub(super) struct Constraints {
    nodes: Vec<Node>,
    parents: Vec<Vec<NodeId>>,
    plans: Vec<Plan>,
    guards: Vec<u64>,
    shared: HashMap<NodeId, Option<u64>>,
}
impl Constraints {
    pub(super) fn permits(
        &mut self,
        before: Width,
        after: Interval,
        cancelled: &dyn Fn() -> bool,
    ) -> bool {
        if before.interval == after {
            return true;
        }
        let plan = before.node.and_then(|id| {
            self.plans.get(id).copied().map(|plan| {
                if self.guards[id] == 0 {
                    plan
                } else {
                    Plan::Fixed
                }
            })
        });
        match plan {
            Some(Plan::Fixed) => false,
            Some(Plan::Bounded { increase, shared }) => {
                let gain = after.max.saturating_sub(before.interval.max);
                if !after.fixed() || gain > increase {
                    return false;
                }
                if let Some((entry, factor)) = shared {
                    let limit = if let Some(limit) = self.shared.get(&entry) {
                        *limit
                    } else {
                        let limit = self.shared_limit(entry, cancelled);
                        self.shared.insert(entry, limit);
                        limit
                    };
                    return limit.is_some_and(|limit| gain <= limit / factor);
                }
                true
            }
            Some(Plan::Unused) | None => true,
        }
    }

    fn shared_limit(&self, seed: NodeId, cancelled: &dyn Fn() -> bool) -> Option<u64> {
        let mut changes = HashMap::from([(seed, 1)]);
        let mut pending: BTreeSet<_> = self.parents[seed].iter().copied().collect();
        let mut increase = MAX_WIDTH - self.nodes[seed].interval.max;
        while let Some(id) = pending.pop_first() {
            if cancelled() {
                return None;
            }
            let node = &self.nodes[id];
            let slope = node
                .rule
                .slope(|child| child.and_then(|id| changes.get(&id)).copied().unwrap_or(0))?;
            if slope == 0 {
                continue;
            }
            increase = increase.min((MAX_WIDTH - node.interval.max) / slope);
            changes.insert(id, slope);
            pending.extend(&self.parents[id]);
        }
        Some(increase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest::rstest]
    #[case::streamed(0)]
    #[case::sparse(100)]
    fn competing_assertions_keep_tightest_growth_bound(#[case] allocations: usize) {
        // Arrange
        let mut model = Model::default();
        let first = model.repeat(Width::ONE, 0, 0, true);
        let second = model.repeat(Width::ONE, 0, 0, true);
        let fixed = Width {
            interval: Interval {
                min: MAX_WIDTH - 1,
                max: MAX_WIDTH - 1,
            },
            node: None,
        };
        let twice = model.add(first, first);
        let earlier = model.add(twice, fixed);
        model.lookbehind(earlier).unwrap();
        let both = model.add(first, second);
        let later = model.add(both, fixed);
        model.lookbehind(later).unwrap();

        // Act
        let growth = model.growth_limits(allocations, &|| false).unwrap();

        // Assert
        assert_eq!(
            [growth[first.node.unwrap()], growth[second.node.unwrap()]],
            [0, 1]
        );
    }
}
