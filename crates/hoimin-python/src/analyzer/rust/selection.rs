//! Per-file candidate admission, independent of source-position indexing.
use super::{AnalysisCancelled, AnalyzeRequest, LineRange, module_name};
use std::collections::HashSet;

/// Raw empty ranges mean all lines; nonempty reversed ranges alone mean none.
/// Construction costs O(R log R), stores O(R), and membership costs O(log R).
/// Symbol preparation visits S selectors and retains O(S) borrowed qualifier slices.
/// Membership probes only the symbol and its dot-boundary ancestors, without
/// allocating prefixes or scanning selectors.
pub(super) struct CandidateSelection<'a> {
    lines: Lines,
    symbols: Symbols<'a>,
}

enum Lines {
    All,
    Restricted(Vec<LineRange>),
}

enum Symbols<'a> {
    All,
    Restricted(HashSet<&'a str>),
}

impl<'a> CandidateSelection<'a> {
    pub(super) fn new(
        request: &AnalyzeRequest<'a>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let lines = if request.lines.is_empty() {
            Lines::All
        } else {
            let mut ranges = Vec::with_capacity(request.lines.len());
            for range in request.lines {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                #[cfg(test)]
                super::selection_tests::record_preparation();
                if range.start <= range.end {
                    ranges.push(LineRange {
                        start: range.start,
                        end: range.end,
                    });
                }
            }
            ranges.sort_unstable_by(|left, right| {
                #[cfg(test)]
                super::selection_tests::record_preparation();
                (left.start, left.end).cmp(&(right.start, right.end))
            });
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let mut maximum = 0;
            for range in &mut ranges {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                #[cfg(test)]
                super::selection_tests::record_preparation();
                maximum = maximum.max(range.end);
                range.end = maximum;
            }
            Lines::Restricted(ranges)
        };
        let symbols = if request.symbols.is_empty() {
            Symbols::All
        } else {
            let mut selected = HashSet::new();
            let mut current_module = None;
            for selector in request.symbols {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                #[cfg(test)]
                super::selection_tests::record_symbol_preparation();
                let qualname = if let Some((module, qualname)) = selector.rsplit_once(':') {
                    let current = current_module.get_or_insert_with(|| module_name(request.path));
                    if !current
                        .strip_suffix(module)
                        .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with('.'))
                    {
                        continue;
                    }
                    qualname
                } else {
                    selector.as_str()
                };
                selected.insert(qualname);
            }
            Symbols::Restricted(selected)
        };
        Ok(Self { lines, symbols })
    }

    pub(super) fn is_unrestricted(&self) -> bool {
        matches!(self.lines, Lines::All) && matches!(self.symbols, Symbols::All)
    }

    pub(super) fn includes(&self, line: u32, symbol: Option<&str>) -> bool {
        #[cfg(test)]
        super::selection_tests::record_query();
        let line_selected = match &self.lines {
            Lines::All => true,
            Lines::Restricted(ranges) => {
                let end = ranges.partition_point(|range| {
                    #[cfg(test)]
                    super::selection_tests::record_membership();
                    range.start <= line
                });
                // All starts to the left qualify. Their maximum end therefore
                // answers membership even when the latest interval is nested.
                end.checked_sub(1).is_some_and(|index| {
                    #[cfg(test)]
                    super::selection_tests::record_membership();
                    line <= ranges[index].end
                })
            }
        };
        if !line_selected {
            return false;
        }
        match &self.symbols {
            Symbols::All => true,
            Symbols::Restricted(selected) => {
                let Some(symbol) = symbol else {
                    return false;
                };
                if selected.is_empty() {
                    return false;
                }
                #[cfg(test)]
                super::selection_tests::record_symbol_lookup();
                selected.contains(symbol)
                    || symbol.match_indices('.').any(|(end, _)| {
                        #[cfg(test)]
                        super::selection_tests::record_symbol_lookup();
                        selected.contains(&symbol[..end])
                    })
            }
        }
    }
}
