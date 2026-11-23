//! Dispatch order and constraints, with edits confined to the affected clauses.
use super::super::{
    AstCandidateCollector, EditSink, Expr, HashSet, MutationOperator, Pattern, Ranged, Stmt,
    TextRange, TokenKind, Visitor, base_exception_boundary_replacement, byte_range,
    exception_hierarchy, exception_pair_replacements, is_termination_exception, mangled_name,
    selected_source_start, tuple_add_replacement, tuple_remove_replacement, visitor,
};
use ruff_python_ast::token::Tokens;
use ruff_python_ast::{ExceptHandler, ExceptHandlerExceptHandler, StmtMatch, StmtRaise, StmtTry};
use ruff_text_size::TextSize;

impl<F: Fn() -> bool> AstCandidateCollector<'_, F> {
    pub(in crate::rust) fn collect_handler_swaps(&mut self, node: &StmtTry) {
        let operator = MutationOperator::ExceptionHandlerSwap;
        if !self.request.operators.contains(operator) {
            return;
        }
        for pair in node.handlers.windows(2) {
            if self.check_cancelled() {
                return;
            }
            let [
                ExceptHandler::ExceptHandler(first),
                ExceptHandler::ExceptHandler(second),
            ] = pair
            else {
                unreachable!("windows of two handlers")
            };
            if first.type_.is_some() && second.type_.is_some() {
                self.swap_clauses(first.range(), second.range(), &first.body, operator);
            }
        }
    }

    pub(in crate::rust) fn collect_case_swaps(&mut self, node: &StmtMatch) {
        let operator = MutationOperator::MatchCaseSwap;
        if !self.request.operators.contains(operator) {
            return;
        }
        for pair in node.cases.windows(2) {
            if self.check_cancelled() {
                return;
            }
            let [first, second] = pair else {
                unreachable!("windows of two cases")
            };
            if second.guard.is_some() || !second.pattern.is_irrefutable() {
                self.swap_clauses(first.range(), second.range(), &first.body, operator);
            }
        }
    }

    // A directive in the first suite can become later than a use or binding in
    // the second clause. A directive moving earlier preserves that compiler
    // ordering. A nested function or class has its own directive scope.
    fn swap_clauses(
        &mut self,
        first: TextRange,
        second: TextRange,
        first_body: &[Stmt],
        operator: MutationOperator,
    ) {
        let range = TextRange::new(first.start(), second.end());
        if !self.needs_validation(range.start(), operator)
            || !selected_source_start(range, self.line_index, self.facts, self.selection)
        {
            return;
        }
        let mut directives = ScopeDirectives {
            cancelled: self.cancelled,
            names: HashSet::new(),
        };
        directives.visit_body(first_body);
        if self.check_cancelled() {
            return;
        }
        let tokens = self.facts.tokens.expect("checked parse");
        if !directives.names.is_empty() {
            // Non-ASCII spellings can normalize to an ASCII identifier.
            if directives.names.iter().any(|name| !name.is_ascii()) {
                return;
            }
            let names: HashSet<_> = directives
                .names
                .iter()
                .map(|&name| {
                    mangled_name(self.private_class, name).unwrap_or_else(|| name.to_owned())
                })
                .collect();
            for token in tokens.in_range(second) {
                if self.check_cancelled() {
                    return;
                }
                if token.kind() == TokenKind::Identifier || token.kind().is_soft_keyword() {
                    let name = &self.source[byte_range(token.range())];
                    if !name.is_ascii()
                        || names.contains(
                            mangled_name(self.private_class, name)
                                .as_deref()
                                .unwrap_or(name),
                        )
                    {
                        return;
                    }
                }
            }
        }
        // Keep the trailing semicolon and inline comment with their clause.
        let extend = |range: TextRange| {
            let mut end = range.end();
            for token in tokens.after(end) {
                if (self.cancelled)() {
                    break;
                }
                if matches!(token.kind(), TokenKind::Semi | TokenKind::Comment) {
                    end = token.end();
                } else {
                    break;
                }
            }
            TextRange::new(range.start(), end)
        };
        self.swap_source(extend(first), extend(second), operator);
    }

    fn swap_source(&mut self, first: TextRange, second: TextRange, operator: MutationOperator) {
        if self.check_cancelled() {
            return;
        }
        let left = &self.source[byte_range(first)];
        let right = &self.source[byte_range(second)];
        if left == right {
            return;
        }
        let between = &self.source[byte_range(TextRange::new(first.end(), second.start()))];
        let range = TextRange::new(first.start(), second.end());
        self.add_deferred_candidate(range, operator, range, || format!("{right}{between}{left}"));
    }

    pub(in crate::rust) fn collect_exception_cause(&mut self, node: &StmtRaise) {
        let operator = MutationOperator::ExceptionCauseOmit;
        if !self.request.operators.contains(operator) || self.check_cancelled() {
            return;
        }
        let (Some(primary), Some(cause)) = (&node.exc, &node.cause) else {
            return;
        };
        let tokens = self.facts.tokens.expect("checked parse");
        let mut primary_end = primary.end();
        for token in tokens.in_range(TextRange::new(primary_end, cause.start())) {
            if self.check_cancelled() {
                return;
            }
            if token.kind() == TokenKind::From {
                if matches!(cause.as_ref(), Expr::NoneLiteral(_)) {
                    self.add_candidate(
                        TextRange::new(primary_end, node.end()),
                        String::new(),
                        operator,
                    );
                } else {
                    // A second simple statement in the same suite is unreachable
                    // after a raise, but it keeps walrus targets, yields and free
                    // names visible to the compiler. Keep the cause's own grouping.
                    self.add_candidate(
                        TextRange::new(primary_end, token.end()),
                        ";".to_owned(),
                        operator,
                    );
                }
                return;
            }
            if !token.kind().is_trivia() {
                primary_end = token.end();
            }
        }
    }

    pub(in crate::rust) fn collect_star_exception_handler(
        &mut self,
        handler: &ExceptHandlerExceptHandler,
    ) {
        let boundary = MutationOperator::ExceptionBaseBoundary;
        let add = MutationOperator::ExceptionTupleAddPair;
        let remove = MutationOperator::ExceptionTupleRemoveMember;
        if !(self.request.operators.contains(boundary)
            || self.request.operators.contains(add)
            || self.request.operators.contains(remove))
            || self.check_cancelled()
        {
            return;
        }
        let Some(type_) = handler.type_.as_deref() else {
            return;
        };
        if let Expr::Name(name) = type_ {
            self.star_boundary(name);
            return;
        }
        let Expr::Tuple(tuple) = type_ else {
            return;
        };
        let mut names = HashSet::new();
        for element in &tuple.elts {
            if self.check_cancelled() {
                return;
            }
            let Expr::Name(name) = element else {
                return;
            };
            let id = name.id.as_str();
            if !(exception_hierarchy::ordinary_builtin(id)
                || id == "BaseException"
                || is_termination_exception(id))
                || !self.facts.resolves_builtin(name.range(), id)
            {
                return;
            }
            names.insert(id);
        }
        if names.is_empty() {
            return;
        }
        let range = tuple.range();
        let source = self.source;
        let facts = self.facts;
        let mut added = HashSet::new();
        for (index, element) in tuple.elts.iter().enumerate() {
            if self.check_cancelled() {
                return;
            }
            let name = element.as_name_expr().expect("checked tuple members");
            self.star_boundary(name);
            if self.request.operators.contains(add) && self.needs_validation(range.start(), add) {
                for &replacement in exception_pair_replacements(name.id.as_str()) {
                    if self.check_cancelled() {
                        return;
                    }
                    if !names.contains(replacement)
                        && self.facts.resolves_builtin(name.range(), replacement)
                        && added.insert(replacement)
                    {
                        let text = if tuple.parenthesized {
                            tuple_add_replacement(source, tuple, facts, replacement)
                        } else {
                            let original = &source[byte_range(range)];
                            let sep = if original.ends_with(',') { " " } else { ", " };
                            Some(format!("{original}{sep}{replacement}"))
                        };
                        if let Some(text) = text {
                            self.add_candidate(range, text, add);
                        }
                    }
                }
            }
            if self.request.operators.contains(remove)
                && self.needs_validation(range.start(), remove)
                && !self.check_cancelled()
            {
                let text = if tuple.elts.len() == 1 && !tuple.parenthesized {
                    Some("()".to_owned())
                } else {
                    tuple_remove_replacement(source, tuple, facts, index)
                };
                if let Some(text) = text {
                    self.add_candidate(range, text, remove);
                }
            }
        }
    }

    fn star_boundary(&mut self, name: &ruff_python_ast::ExprName) {
        let operator = MutationOperator::ExceptionBaseBoundary;
        if self.request.operators.contains(operator)
            && let Some(replacement) = base_exception_boundary_replacement(name.id.as_str())
            && self
                .facts
                .resolves_builtin_pair(name.range(), name.id.as_str(), replacement)
        {
            self.add_candidate(name.range(), replacement.to_owned(), operator);
        }
    }

    pub(in crate::rust) fn collect_pattern_constraints(&mut self, pattern: &Pattern) {
        let operator = MutationOperator::MatchClassPositionSwap;
        let Pattern::MatchClass(class) = pattern else {
            return;
        };
        if !self.request.operators.contains(operator) {
            return;
        }
        let tokens = self.facts.tokens.expect("checked parse");
        let positions = &class.arguments.patterns;
        for (index, first) in positions.iter().enumerate() {
            let first_range = grouped_position(first.range(), class.end(), tokens);
            if self.check_cancelled() || !self.needs_validation(first_range.start(), operator) {
                return;
            }
            if !selected_source_start(first_range, self.line_index, self.facts, self.selection) {
                continue;
            }
            for second in &positions[index + 1..] {
                if self.check_cancelled() || !self.needs_validation(first_range.start(), operator) {
                    break;
                }
                let second_range = grouped_position(second.range(), class.end(), tokens);
                self.swap_source(first_range, second_range, operator);
            }
        }
    }
}

// Parentheses around an operand must move with it. Leaving them in place
// can exceed CPython's delimiter limit when a deeper operand moves inside.
fn grouped_position(range: TextRange, class_end: TextSize, tokens: &Tokens) -> TextRange {
    let left = tokens
        .before(range.start())
        .iter()
        .rev()
        .filter(|token| !token.kind().is_trivia())
        .take_while(|token| token.kind() == TokenKind::Lpar);
    let right = tokens
        .after(range.end())
        .iter()
        .filter(|token| !token.kind().is_trivia())
        .take_while(|token| token.kind() == TokenKind::Rpar && token.end() < class_end);
    left.zip(right).last().map_or(range, |(open, close)| {
        TextRange::new(open.start(), close.end())
    })
}

struct ScopeDirectives<'c, 'ast, F> {
    cancelled: &'c F,
    names: HashSet<&'ast str>,
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for ScopeDirectives<'_, 'ast, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if (self.cancelled)() {
            return;
        }
        let names = match statement {
            Stmt::Global(node) => &node.names,
            Stmt::Nonlocal(node) => &node.names,
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            _ => return visitor::walk_stmt(self, statement),
        };
        self.names
            .extend(names.iter().map(ruff_python_ast::Identifier::as_str));
    }

    fn visit_expr(&mut self, _: &'ast Expr) {}

    fn visit_pattern(&mut self, _: &'ast Pattern) {}
}
