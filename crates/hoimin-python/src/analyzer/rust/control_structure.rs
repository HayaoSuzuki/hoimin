//! Structural changes to protected suites and pattern dispatch. Header edits keep the
//! original suites in their lexical scope, including unreachable compiler bindings.
use super::{
    AstCandidateCollector, HashSet, MutationOperator, Pattern, Ranged, TextRange, TokenKind,
    byte_range, selected_source_start,
};
use ruff_python_ast::{MatchCase, PatternMatchOr, StmtMatch, StmtTry};
use ruff_text_size::TextSize;

/// Lexical suite depth and whether the enclosing match admits dispatch edits.
/// Both are restored by the existing suite and statement walks on return.
#[derive(Default)]
pub(super) struct MatchTraversal {
    pub(super) suite_depth: usize,
    pub(super) eligible: bool,
}

impl<F: Fn() -> bool> AstCandidateCollector<'_, F> {
    pub(super) fn collect_unwrap(&mut self, node: &StmtTry) {
        let op = MutationOperator::FinallyUnwrap;
        if !self.request.operators.contains(op)
            || node.finalbody.is_empty()
            || self.check_cancelled()
        {
            return;
        }
        let previous = node
            .orelse
            .last()
            .map(Ranged::end)
            .or_else(|| node.handlers.last().map(Ranged::end))
            .or_else(|| node.body.last().map(Ranged::end))
            .expect("try suite");
        let tokens = self.facts.tokens.expect("checked parse");
        let gap = TextRange::new(previous, node.finalbody[0].start());
        for token in tokens.in_range(gap) {
            if self.check_cancelled() {
                return;
            }
            if token.kind() != TokenKind::Finally {
                continue;
            }
            let bare = node.handlers.is_empty();
            let start = if bare { node.start() } else { token.start() };
            let range = TextRange::new(start, token.end());
            if selected_source_start(range, self.line_index, self.facts, self.selection) {
                let replacement = if bare {
                    let middle = TextRange::new(start + TextSize::new(3), token.start());
                    format!("if True{}if True", &self.source[byte_range(middle)])
                } else {
                    "if True".into()
                };
                self.add_candidate(range, replacement, op);
            }
            break;
        }
    }

    pub(super) fn match_enabled(&self) -> bool {
        [
            MutationOperator::MatchCaseDelete,
            MutationOperator::MatchGuardConstant,
            MutationOperator::MatchOrAlternativeDelete,
        ]
        .into_iter()
        .any(|op| self.request.operators.contains(op))
    }

    pub(super) fn valid_cases(&self, node: &StmtMatch) -> bool {
        node.cases.iter().enumerate().all(|(i, case)| {
            (i + 1 == node.cases.len() || case.guard.is_some() || !case.pattern.is_irrefutable())
                && valid_bindings(&case.pattern, &mut HashSet::new(), self.cancelled)
        })
    }

    pub(super) fn collect_case(&mut self, case: &MatchCase) {
        if self.check_cancelled() {
            return;
        }
        if self
            .request
            .operators
            .contains(MutationOperator::MatchCaseDelete)
            && self.match_state.suite_depth + 2 <= 99
            && selected_source_start(case.range(), self.line_index, self.facts, self.selection)
            && let Some((range, replacement)) = self.dead_case_header(case)
        {
            self.add_candidate(range, replacement, MutationOperator::MatchCaseDelete);
        }
        if self.check_cancelled() {
            return;
        }
        if self
            .request
            .operators
            .contains(MutationOperator::MatchGuardConstant)
            && let Some(guard) = &case.guard
            && !guard.is_boolean_literal_expr()
            && selected_source_start(guard.range(), self.line_index, self.facts, self.selection)
        {
            let range = guard.range();
            let original = &self.source[byte_range(range)];
            for wrapper in ["True or", "False and"] {
                if self.check_cancelled() {
                    return;
                }
                self.add_candidate(
                    range,
                    format!("({wrapper} ({original}))"),
                    MutationOperator::MatchGuardConstant,
                );
            }
        }
    }

    fn dead_case_header(&mut self, case: &MatchCase) -> Option<(TextRange, String)> {
        let tokens = self.facts.tokens.expect("checked parse");
        let header_end = case
            .guard
            .as_ref()
            .map_or_else(|| case.pattern.end(), |guard| guard.end());
        let gap = TextRange::new(header_end, case.body.first()?.start());
        let mut colon = None;
        let mut indent = None;
        let mut newline = None;
        for token in tokens.in_range(gap) {
            if self.check_cancelled() {
                return None;
            }
            match token.kind() {
                TokenKind::Colon => colon = Some(token.end()),
                TokenKind::Newline => newline = Some(&self.source[byte_range(token.range())]),
                TokenKind::Indent => {
                    indent = Some(&self.source[byte_range(token.range())]);
                }
                _ => {}
            }
        }
        let range = TextRange::new(case.start(), colon?);
        // A backslash can separate indentation from the case keyword itself.
        // Both indents come from the beginning of the logical line, even when
        // the following physical line starts in column zero.
        let previous = tokens.before(case.start()).iter().rev().find(|token| {
            matches!(
                token.kind(),
                TokenKind::Newline | TokenKind::NonLogicalNewline
            )
        })?;
        let prefix = &self.source[byte_range(TextRange::new(previous.end(), case.start()))];
        let eol = newline.unwrap_or(&self.source[byte_range(previous.range())]);
        let original = &self.source[byte_range(range)];
        let header = if let Some(suite) = indent {
            let nested = deeper_indent(suite, "    ");
            format!("case _ if False:{eol}{suite}match None:{eol}{nested}{original} pass")
        } else {
            let suite = deeper_indent(prefix, "    ");
            let nested = deeper_indent(prefix, "        ");
            format!("case _ if False:{eol}{suite}match None:{eol}{nested}{original}")
        };
        Some((range, header))
    }

    pub(super) fn collect_or(&mut self, node: &PatternMatchOr) {
        if self.check_cancelled() {
            return;
        }
        let tokens = self
            .facts
            .tokens
            .expect("checked parse")
            .in_range(node.range());
        let mut depth = 0usize;
        let mut previous_end = None;
        let mut current_start = None;
        let mut current_end = node.start();
        let mut left_of_bar = None;
        for token in tokens {
            if self.check_cancelled() {
                return;
            }
            let kind = token.kind();
            if kind.is_trivia() {
                continue;
            }
            if kind == TokenKind::Vbar && depth == 0 {
                left_of_bar = current_start.take();
                previous_end = Some(current_end);
                continue;
            }
            if current_start.is_none() {
                current_start = Some(token.start());
                if let Some(left) = left_of_bar.take() {
                    self.remove_alternative(TextRange::new(left, token.start()));
                }
            }
            match kind {
                TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => depth += 1,
                TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => depth -= 1,
                _ => {}
            }
            current_end = token.end();
        }
        if let Some(left) = previous_end
            && !self.check_cancelled()
        {
            self.remove_alternative(TextRange::new(left, current_end));
        }
    }

    fn remove_alternative(&mut self, range: TextRange) {
        if selected_source_start(range, self.line_index, self.facts, self.selection) {
            // Removing grouping can put two identifier/number tokens next to each other,
            // for example the keyword and literal in "case(1)|2".
            let word_byte = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_' || !b.is_ascii();
            let bytes = self.source.as_bytes();
            let joined_words = bytes[..usize::from(range.start())]
                .last()
                .is_some_and(word_byte)
                && bytes.get(usize::from(range.end())).is_some_and(word_byte);
            let separator = if joined_words { " " } else { "" };
            self.add_candidate(
                range,
                separator.into(),
                MutationOperator::MatchOrAlternativeDelete,
            );
        }
    }
}

// CPython treats a continued tab indent differently from a physical-line tab
// during its mixed-tab check. Retain the continuation, and increase indentation
// before its backslash, where the tokenizer measures the effective column.
fn deeper_indent(prefix: &str, extra: &str) -> String {
    let end = prefix
        .find(|ch| !matches!(ch, ' ' | '\t' | '\x0c'))
        .unwrap_or(prefix.len());
    format!("{}{extra}{}", &prefix[..end], &prefix[end..])
}

// Ruff's syntax parser accepts several pattern errors that CPython diagnoses during
// compilation. Check reachability and capture sets before admitting dispatch changes.
fn valid_bindings<'a>(
    pattern: &'a Pattern,
    names: &mut HashSet<&'a str>,
    cancelled: &impl Fn() -> bool,
) -> bool {
    if cancelled() {
        return false;
    }
    let bind = |names: &mut HashSet<&'a str>, name: &'a ruff_python_ast::Identifier| {
        name.as_str() != "__debug__" && names.insert(name.as_str())
    };
    match pattern {
        Pattern::MatchAs(p) => {
            p.pattern
                .as_deref()
                .is_none_or(|p| valid_bindings(p, names, cancelled))
                && p.name.as_ref().is_none_or(|name| bind(names, name))
        }
        Pattern::MatchStar(p) => p.name.as_ref().is_none_or(|name| bind(names, name)),
        Pattern::MatchSequence(p) => {
            p.patterns
                .iter()
                .filter(|p| p.is_match_star())
                .take(2)
                .count()
                < 2
                && p.patterns
                    .iter()
                    .all(|p| valid_bindings(p, names, cancelled))
        }
        Pattern::MatchMapping(p) => {
            #[expect(
                clippy::mutable_key_type,
                reason = "Ruff's Python-equality hash ignores AST node indices and the checked tree is immutable during discovery"
            )]
            let mut literals = HashSet::new();
            p.keys.iter().all(|key| {
                !cancelled()
                    && (key.is_attribute_expr()
                        || literals.insert(ruff_python_ast::comparable::HashableExpr::from(key)))
            }) && p.rest.as_ref().is_none_or(|name| bind(names, name))
                && p.patterns
                    .iter()
                    .all(|p| valid_bindings(p, names, cancelled))
        }
        Pattern::MatchClass(p) => {
            let mut keywords = HashSet::new();
            p.arguments
                .patterns
                .iter()
                .all(|p| valid_bindings(p, names, cancelled))
                && p.arguments.keywords.iter().all(|kw| {
                    keywords.insert(kw.attr.as_str())
                        && valid_bindings(&kw.pattern, names, cancelled)
                })
        }
        Pattern::MatchOr(p) => {
            let mut expected = None;
            for (i, alternative) in p.patterns.iter().enumerate() {
                let mut bound = HashSet::new();
                if (i + 1 != p.patterns.len() && alternative.is_irrefutable())
                    || !valid_bindings(alternative, &mut bound, cancelled)
                    || expected.as_ref().is_some_and(|set| *set != bound)
                {
                    return false;
                }
                expected = Some(bound);
            }
            expected.is_some_and(|set| set.into_iter().all(|name| names.insert(name)))
        }
        Pattern::MatchValue(_) | Pattern::MatchSingleton(_) => true,
    }
}
