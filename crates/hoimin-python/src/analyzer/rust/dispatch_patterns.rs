//! Wildcard weakening keeps captures and compiler-visible reads.
use super::super::{
    AstCandidateCollector, EditSink, Expr, HashSet, MutationOperator, Pattern, Ranged, TextRange,
    TokenKind, Visitor, byte_range, selected_source_start, visitor,
};
use super::valid_bindings;
use ruff_python_ast::MatchCase;

impl<F: Fn() -> bool> AstCandidateCollector<'_, F> {
    pub(super) fn collect_case_wildcards(&mut self, case: &MatchCase) {
        if self
            .request
            .operators
            .contains(MutationOperator::MatchPatternWildcard)
        {
            WildcardSites {
                collector: self,
                case,
            }
            .visit_pattern(&case.pattern);
        }
    }

    fn weaken_pattern(&mut self, mut pattern: &Pattern, case: &MatchCase) {
        // An AS wrapper can keep its capture when its constraint weakens.
        while let Pattern::MatchAs(binding) = pattern
            && let Some(inner) = binding.pattern.as_deref()
        {
            if self.check_cancelled() {
                return;
            }
            pattern = inner;
        }
        let operator = MutationOperator::MatchPatternWildcard;
        let range = pattern.range();
        if pattern.is_irrefutable()
            || pattern.is_match_star()
            || !self.needs_validation(range.start(), operator)
            || !selected_source_start(range, self.line_index, self.facts, self.selection)
        {
            return;
        }
        let mut bindings = HashSet::new();
        if !valid_bindings(pattern, &mut bindings, self.cancelled) || !bindings.is_empty() {
            return;
        }
        let mut reads = PatternReads {
            cancelled: self.cancelled,
            names: Vec::new(),
        };
        reads.visit_pattern(pattern);
        if self.check_cancelled() {
            return;
        }
        // Delimited patterns can abut the AS keyword with no spaces.
        let wildcard = if self.source[usize::from(range.end())..].starts_with("as") {
            "_ "
        } else {
            "_"
        };
        if reads.names.is_empty() {
            self.add_candidate(range, wildcard.to_owned(), operator);
            return;
        }
        self.protect_pattern_reads(case, range, wildcard, &reads.names, operator);
    }

    fn protect_pattern_reads(
        &mut self,
        case: &MatchCase,
        range: TextRange,
        wildcard: &str,
        names: &[TextRange],
        operator: MutationOperator,
    ) {
        let tokens = self.facts.tokens.expect("checked parse");
        let header_end = case
            .guard
            .as_ref()
            .map_or_else(|| case.pattern.end(), |guard| guard.end());
        let Some(body) = case.body.first() else {
            return;
        };
        let Some(colon) = tokens
            .in_range(TextRange::new(header_end, body.start()))
            .iter()
            .find(|token| token.kind() == TokenKind::Colon)
        else {
            return;
        };
        let edit = TextRange::new(range.start(), colon.start());
        let guard_keyword = if let Some(guard) = &case.guard {
            if !self.guard_wrapper_fits(case.pattern.end(), guard.end(), 1) {
                return;
            }
            let Some(keyword) = tokens
                .in_range(TextRange::new(case.pattern.end(), guard.start()))
                .iter()
                .find(|token| token.kind() == TokenKind::If)
            else {
                return;
            };
            Some(keyword.end())
        } else {
            None
        };
        let source = self.source;
        // Preserve the original guard object so CPython applies its original
        // truth test once. The unselected expression keeps free and class-cell
        // reads in the symbol table without evaluating the removed lookup.
        self.add_deferred_candidate(edit, operator, edit, || {
            let names = names
                .iter()
                .map(|&name| &source[byte_range(name)])
                .collect::<Vec<_>>()
                .join(" or ");
            if let Some(keyword_end) = guard_keyword {
                let before = &source[byte_range(TextRange::new(range.end(), keyword_end))];
                let guard = source[byte_range(TextRange::new(keyword_end, colon.start()))]
                    .trim_start_matches([' ', '\t']);
                format!("{wildcard}{before} ({guard}) if True else {names}")
            } else {
                let before = &source[byte_range(TextRange::new(range.end(), colon.start()))];
                format!("{wildcard}{before} if True or {names}")
            }
        });
    }
}

struct WildcardSites<'c, 'case, 'source, F> {
    collector: &'c mut AstCandidateCollector<'source, F>,
    case: &'case MatchCase,
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for WildcardSites<'_, '_, '_, F> {
    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        let children = match pattern {
            Pattern::MatchSequence(node) => node.patterns.as_slice(),
            Pattern::MatchMapping(node) => node.patterns.as_slice(),
            Pattern::MatchClass(node) => {
                for keyword in &node.arguments.keywords {
                    if self.collector.check_cancelled() {
                        return;
                    }
                    self.collector.weaken_pattern(&keyword.pattern, self.case);
                }
                node.arguments.patterns.as_slice()
            }
            _ => &[],
        };
        for child in children {
            if self.collector.check_cancelled() {
                return;
            }
            self.collector.weaken_pattern(child, self.case);
        }
        if !self.collector.check_cancelled() {
            visitor::walk_pattern(self, pattern);
        }
    }

    fn visit_expr(&mut self, _: &'ast Expr) {}
}

struct PatternReads<'c, F> {
    cancelled: &'c F,
    names: Vec<TextRange>,
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for PatternReads<'_, F> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if !(self.cancelled)() {
            if let Expr::Name(name) = expression {
                self.names.push(name.range());
            }
            visitor::walk_expr(self, expression);
        }
    }
}
