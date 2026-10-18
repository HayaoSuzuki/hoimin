#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, hash_map::Entry};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct IndexLookupStats {
    pub(crate) facts: usize,
    pub(crate) queries: usize,
    pub(crate) comparisons: usize,
}

#[derive(Clone)]
pub(super) struct ScopeInterval {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) symbol: String,
}

impl ScopeInterval {
    pub(super) fn new(start: usize, end: usize, symbol: impl Into<String>) -> Self {
        Self {
            start,
            end,
            symbol: symbol.into(),
        }
    }
}

#[derive(Default)]
pub(super) struct ContainmentIndex {
    starts: Vec<usize>,
    prefix_max_ends: Vec<usize>,
    #[cfg(test)]
    queries: Cell<usize>,
    #[cfg(test)]
    comparisons: Cell<usize>,
}

impl ContainmentIndex {
    pub(super) fn new(mut ranges: Vec<(usize, usize)>) -> Self {
        ranges.sort_unstable_by_key(|&(start, _)| start);
        let mut starts = Vec::with_capacity(ranges.len());
        let mut prefix_max_ends = Vec::with_capacity(ranges.len());
        let mut maximum_end = 0;
        for (start, end) in ranges {
            starts.push(start);
            maximum_end = maximum_end.max(end);
            prefix_max_ends.push(maximum_end);
        }
        Self {
            starts,
            prefix_max_ends,
            #[cfg(test)]
            queries: Cell::new(0),
            #[cfg(test)]
            comparisons: Cell::new(0),
        }
    }

    pub(super) fn contains(&self, start: usize, end: usize) -> bool {
        #[cfg(test)]
        self.queries.set(self.queries.get() + 1);
        let mut left = 0;
        let mut right = self.starts.len();
        while left < right {
            #[cfg(test)]
            self.comparisons.set(self.comparisons.get() + 1);
            let middle = left + (right - left) / 2;
            if self.starts[middle] <= start {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        let eligible = left;
        eligible > 0 && end <= self.prefix_max_ends[eligible - 1]
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> IndexLookupStats {
        IndexLookupStats {
            facts: self.starts.len(),
            queries: self.queries.get(),
            comparisons: self.comparisons.get(),
        }
    }
}

#[derive(Default)]
pub(super) struct NotOperandIndex {
    operands: HashMap<usize, (usize, usize)>,
    #[cfg(test)]
    queries: Cell<usize>,
}

impl NotOperandIndex {
    pub(super) fn new(operands: Vec<(usize, usize, usize)>) -> Self {
        let mut by_start = HashMap::with_capacity(operands.len());
        for (start, operand_start, operand_end) in operands {
            match by_start.entry(start) {
                Entry::Vacant(entry) => {
                    entry.insert((operand_start, operand_end));
                }
                Entry::Occupied(_) => {
                    debug_assert!(false, "duplicate unary-not operator start");
                }
            }
        }
        Self {
            operands: by_start,
            #[cfg(test)]
            queries: Cell::new(0),
        }
    }

    pub(super) fn operand_at(&self, start: usize) -> Option<(usize, usize)> {
        #[cfg(test)]
        self.queries.set(self.queries.get() + 1);
        self.operands.get(&start).copied()
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> IndexLookupStats {
        IndexLookupStats {
            facts: self.operands.len(),
            queries: self.queries.get(),
            comparisons: 0,
        }
    }
}

struct ScopeSegment {
    start: usize,
    end: usize,
    scope: usize,
}

#[derive(Default)]
pub(super) struct ScopeIndex {
    scopes: Vec<ScopeInterval>,
    segments: Vec<ScopeSegment>,
    #[cfg(test)]
    queries: Cell<usize>,
    #[cfg(test)]
    comparisons: Cell<usize>,
}

impl ScopeIndex {
    pub(super) fn new(scopes: Vec<ScopeInterval>) -> Self {
        let mut events = BTreeMap::<usize, (Vec<usize>, Vec<usize>)>::new();
        for (ordinal, scope) in scopes.iter().enumerate() {
            if scope.start >= scope.end {
                continue;
            }
            events.entry(scope.start).or_default().1.push(ordinal);
            events.entry(scope.end).or_default().0.push(ordinal);
        }

        let mut active = BTreeSet::<(usize, usize)>::new();
        let mut segments: Vec<ScopeSegment> = Vec::new();
        let mut previous = events.first_key_value().map(|(&offset, _)| offset);
        for (offset, (ending, starting)) in events {
            if let Some(segment_start) = previous
                && segment_start < offset
                && let Some(&(_, scope)) = active.last()
            {
                if let Some(last) = segments.last_mut()
                    && last.scope == scope
                    && last.end == segment_start
                {
                    last.end = offset;
                } else {
                    segments.push(ScopeSegment {
                        start: segment_start,
                        end: offset,
                        scope,
                    });
                }
            }
            for ordinal in ending {
                active.remove(&(scopes[ordinal].start, ordinal));
            }
            for ordinal in starting {
                active.insert((scopes[ordinal].start, ordinal));
            }
            previous = Some(offset);
        }

        Self {
            scopes,
            segments,
            #[cfg(test)]
            queries: Cell::new(0),
            #[cfg(test)]
            comparisons: Cell::new(0),
        }
    }

    pub(super) fn symbol_at(&self, offset: usize) -> Option<&str> {
        #[cfg(test)]
        self.queries.set(self.queries.get() + 1);
        let mut left = 0;
        let mut right = self.segments.len();
        while left < right {
            #[cfg(test)]
            self.comparisons.set(self.comparisons.get() + 1);
            let middle = left + (right - left) / 2;
            if self.segments[middle].start <= offset {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        let eligible = left;
        let segment = eligible.checked_sub(1).map(|index| &self.segments[index])?;
        (offset < segment.end).then(|| self.scopes[segment.scope].symbol.as_str())
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> IndexLookupStats {
        IndexLookupStats {
            facts: self.segments.len(),
            queries: self.queries.get(),
            comparisons: self.comparisons.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContainmentIndex, NotOperandIndex, ScopeIndex, ScopeInterval};
    use proptest::prelude::*;

    fn linear_contains(ranges: &[(usize, usize)], start: usize, end: usize) -> bool {
        ranges
            .iter()
            .any(|&(outer_start, outer_end)| outer_start <= start && end <= outer_end)
    }

    #[test]
    fn containment_matches_linear_semantics_at_boundaries_and_overlaps() {
        let cases = [
            vec![],
            vec![(0, 10)],
            vec![(0, 10), (5, 6)],
            vec![(5, 6), (0, 10), (5, 12), (12, 15)],
            vec![(3, 3), (3, 8), (3, 5)],
        ];

        for ranges in cases {
            let index = ContainmentIndex::new(ranges.clone());
            for start in 0..=16 {
                for end in start..=16 {
                    assert_eq!(
                        index.contains(start, end),
                        linear_contains(&ranges, start, end),
                        "ranges={ranges:?} query={start}..{end}",
                    );
                }
            }
        }
    }

    fn linear_scope(scopes: &[ScopeInterval], offset: usize) -> Option<&str> {
        scopes
            .iter()
            .filter(|scope| scope.start <= offset && offset < scope.end)
            .max_by_key(|scope| scope.start)
            .map(|scope| scope.symbol.as_str())
    }

    #[test]
    fn scope_segments_match_linear_innermost_selection() {
        let scopes = vec![
            ScopeInterval::new(0, 20, "outer"),
            ScopeInterval::new(5, 10, "outer.inner"),
            ScopeInterval::new(12, 18, "outer.sibling"),
            ScopeInterval::new(12, 15, "outer.sibling.last_equal_start"),
        ];
        let index = ScopeIndex::new(scopes.clone());

        for offset in 0..=21 {
            assert_eq!(
                index.symbol_at(offset),
                linear_scope(&scopes, offset),
                "offset={offset}",
            );
        }
    }

    #[test]
    fn scope_segments_cover_disjoint_and_empty_inputs() {
        for scopes in [
            vec![],
            vec![ScopeInterval::new(3, 5, "first")],
            vec![
                ScopeInterval::new(0, 2, "first"),
                ScopeInterval::new(4, 6, "second"),
            ],
        ] {
            let index = ScopeIndex::new(scopes.clone());
            for offset in 0..=7 {
                assert_eq!(index.symbol_at(offset), linear_scope(&scopes, offset));
            }
        }
    }

    #[test]
    fn not_operands_are_looked_up_only_by_exact_operator_start() {
        let index = NotOperandIndex::new(vec![(4, 8, 13), (20, 24, 30)]);

        assert_eq!(index.operand_at(3), None);
        assert_eq!(index.operand_at(4), Some((8, 13)));
        assert_eq!(index.operand_at(5), None);
        assert_eq!(index.operand_at(20), Some((24, 30)));
    }

    proptest! {
        #[test]
        fn generated_containment_queries_match_linear_scans(
            ranges in proptest::collection::vec((0usize..128, 0usize..32), 0..96),
            query_start in 0usize..160,
            query_length in 0usize..32,
        ) {
            let ranges = ranges
                .into_iter()
                .map(|(start, length)| (start, start + length))
                .collect::<Vec<_>>();
            let query_end = query_start + query_length;
            let index = ContainmentIndex::new(ranges.clone());

            prop_assert_eq!(
                index.contains(query_start, query_end),
                linear_contains(&ranges, query_start, query_end),
            );
        }

        #[test]
        fn generated_scope_segments_match_linear_scans(
            ranges in proptest::collection::vec((0usize..128, 1usize..32), 0..96),
            offset in 0usize..160,
        ) {
            let scopes = ranges
                .into_iter()
                .enumerate()
                .map(|(ordinal, (start, length))| {
                    ScopeInterval::new(start, start + length, ordinal.to_string())
                })
                .collect::<Vec<_>>();
            let index = ScopeIndex::new(scopes.clone());

            prop_assert_eq!(index.symbol_at(offset), linear_scope(&scopes, offset));
        }
    }
}
