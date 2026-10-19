//! Per-file candidate admission, independent of source-position indexing.
use super::{AnalysisCancelled, AnalyzeRequest, LineRange, module_name};
use camino::Utf8Path;

/// Raw empty ranges mean all lines; nonempty reversed ranges alone mean none.
/// Construction costs O(R log R), stores O(R), and membership costs O(log R).
/// Path and symbol metadata remain borrowed from the unchanged public request.
pub(super) struct CandidateSelection<'a> {
    lines: Lines,
    path: &'a Utf8Path,
    symbols: &'a [String],
}

enum Lines {
    All,
    Restricted(Vec<LineRange>),
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
        Ok(Self {
            lines,
            path: request.path,
            symbols: request.symbols,
        })
    }

    pub(super) fn is_unrestricted(&self) -> bool {
        matches!(self.lines, Lines::All) && self.symbols.is_empty()
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
        let symbol_selected = self.symbols.is_empty()
            || self.symbols.iter().any(|selector| {
                let (module_selected, qualname) = selector.rsplit_once(':').map_or_else(
                    || (true, selector.as_str()),
                    |(module, qualname)| {
                        let current_module = module_name(self.path);
                        (
                            current_module == module
                                || current_module.ends_with(&format!(".{module}")),
                            qualname,
                        )
                    },
                );
                module_selected
                    && symbol.is_some_and(|symbol| {
                        symbol == qualname || symbol.starts_with(&format!("{qualname}."))
                    })
            });
        line_selected && symbol_selected
    }
}
