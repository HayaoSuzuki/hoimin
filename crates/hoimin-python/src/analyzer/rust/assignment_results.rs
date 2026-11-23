//! Source edits for assignment and result semantics. Only the affected
//! operation changes dispatch or control flow; its operands keep their scope.

use super::{AstCandidateCollector, EditSink, MutationOperator, byte_range, source_text};
use ruff_python_ast::token::{TokenKind, Tokens, parenthesized_range};
use ruff_python_ast::{CmpOp, Expr, ExprCompare, Stmt, StmtAugAssign};
use ruff_text_size::{Ranged, TextRange, TextSize};

impl<'source, F: Fn() -> bool> AstCandidateCollector<'source, F> {
    /// Collect from a statement's own syntax. The caller still walks its children.
    pub(super) fn collect_assignment_result_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::AugAssign(assign) => self.collect_binary_assignment(assign),
            Stmt::Assign(assign)
                if self
                    .request
                    .operators
                    .contains(MutationOperator::AssignmentTargetSwap) =>
            {
                for target in &assign.targets {
                    if !self.collect_target_swaps(target) {
                        return;
                    }
                }
            }
            Stmt::Return(result)
                if result
                    .value
                    .as_ref()
                    .is_some_and(|value| !value.is_none_literal_expr()) =>
            {
                let range = result.range();
                let operator = MutationOperator::ReturnValueNone;
                if self.request.operators.contains(operator)
                    && self.needs_validation(range.start(), operator)
                    && self.expression_selected(result)
                {
                    let original = self.text(range);
                    // Keeping the entire operand preserves its grouping, walruses, awaits
                    // and yields. The initial pass also prevents an accidental docstring.
                    self.emit(range, operator, || {
                        format!("pass; {}; return None", &original["return".len()..])
                    });
                }
            }
            Stmt::For(node) => {
                self.collect_disabled_else(node.body.last().map(Ranged::end), &node.orelse, false);
            }
            Stmt::While(node) => {
                self.collect_disabled_else(node.body.last().map(Ranged::end), &node.orelse, false);
            }
            Stmt::Try(node) if let Some(previous) = node.handlers.last() => {
                self.collect_disabled_else(Some(previous.end()), &node.orelse, true);
            }
            _ => {}
        }
    }

    /// Change one operator token in a comparison chain, leaving the original
    /// operand nodes and Python's shared-operand/short-circuit behavior intact.
    pub(super) fn collect_identity_equality_swap(&mut self, comparison: &ExprCompare) {
        let operator = MutationOperator::IdentityEqualitySwap;
        if !self.request.operators.contains(operator) {
            return;
        }
        let source = self.source;
        let tokens = self.facts.tokens.expect("checked parse");
        for (left, current, right) in comparison.iter() {
            let gap = TextRange::new(left.end(), right.start());
            if self.check_cancelled() {
                return;
            }
            if !self.needs_validation(gap.start(), operator) {
                continue;
            }
            if let Some((range, spelling, retained)) =
                comparison_edit(tokens, gap, *current, self.cancelled)
                && !self.emit(range, operator, || {
                    let trivia = retained.map_or("", |range| &source[byte_range(range)]);
                    format!("{spelling}{trivia}")
                })
            {
                return;
            }
        }
    }

    fn collect_binary_assignment(&mut self, assign: &StmtAugAssign) {
        let operator = MutationOperator::AugmentedToBinary;
        let range = assign.range();
        if !self.request.operators.contains(operator)
            || !self.needs_validation(range.start(), operator)
            || !self.expression_selected(assign)
        {
            return;
        }
        let tokens = self.facts.tokens.expect("checked parse");
        let source = self.source;
        let delimiters = self.processing_site.delimiters;
        let gap = TextRange::new(assign.target.end(), assign.value.start());
        let Some(equal) = tokens
            .in_range(gap)
            .iter()
            .take_while(|_| !(self.cancelled)())
            .find(|token| token.kind().as_augmented_assign_operator() == Some(assign.op))
        else {
            return;
        };
        let target = assign.target.as_ref();
        let rhs = TextRange::new(equal.end(), assign.end());
        let added_rhs = if target.is_name_expr() { 1 } else { 3 };
        if !delimiters.can_enclose(rhs, added_rhs) || self.check_cancelled() {
            return;
        }
        let rhs = &source[byte_range(rhs)];
        let sign = assign.op.as_str();
        let lhs = TextRange::new(range.start(), equal.start());
        if target.is_name_expr() {
            let name = self.text(target.range());
            let lhs = self.text(lhs);
            self.emit(range, operator, || format!("{lhs}= {name} {sign} ({rhs})"));
            return;
        }
        let Some(target) = BinaryTarget::new(target, lhs, tokens, delimiters, self.cancelled)
        else {
            return;
        };
        self.emit(range, operator, || target.render(source, sign, rhs));
    }

    fn collect_target_swaps(&mut self, target: &Expr) -> bool {
        let operator = MutationOperator::AssignmentTargetSwap;
        if self.check_cancelled() {
            return false;
        }
        if !self.needs_validation(target.start(), operator) {
            return true;
        }
        let items = match target {
            Expr::Tuple(tuple) => &tuple.elts,
            Expr::List(list) => &list.elts,
            Expr::Starred(starred) => return self.collect_target_swaps(&starred.value),
            _ => return true,
        };
        let source = self.source;
        let tokens = self.facts.tokens.expect("checked parse");
        // Visit an outer edit before descending into its left target, so
        // admission sees candidates in nondecreasing source order.
        let mut previous = None;
        for (index, item) in items.iter().enumerate() {
            if self.check_cancelled() {
                return false;
            }
            if index + 1 < items.len() {
                let left = previous.unwrap_or_else(|| grouped_target(item, target, tokens));
                let right = grouped_target(&items[index + 1], target, tokens);
                previous = Some(right);
                let range = TextRange::new(left.start(), right.end());
                let first = &source[byte_range(left)];
                let second = &source[byte_range(right)];
                let same_name = matches!(
                    (item, &items[index + 1]),
                    (Expr::Name(left), Expr::Name(right)) if left.id == right.id
                );
                if first != second
                    && !same_name
                    && self.needs_validation(range.start(), operator)
                    && !self.emit(range, operator, || {
                        let separator =
                            &source[byte_range(TextRange::new(left.end(), right.start()))];
                        format!("{second}{separator}{first}")
                    })
                {
                    return false;
                }
            }
            if !self.collect_target_swaps(item) {
                return false;
            }
        }
        true
    }

    fn collect_disabled_else(
        &mut self,
        previous_end: Option<TextSize>,
        suite: &[Stmt],
        try_else: bool,
    ) {
        let operator = MutationOperator::ElseSuiteRemove;
        let (Some(previous_end), Some(first)) = (previous_end, suite.first()) else {
            return;
        };
        if !self.request.operators.contains(operator)
            || !self.needs_validation(previous_end, operator)
        {
            return;
        }
        let tokens = self.facts.tokens.expect("checked parse");
        let source = self.source;
        let gap = TextRange::new(previous_end, first.start());
        let Some(keyword) = tokens
            .in_range(gap)
            .iter()
            .take_while(|_| !(self.cancelled)())
            .find(|token| token.kind() == TokenKind::Else)
        else {
            return;
        };
        if !self.needs_validation(keyword.start(), operator) || !self.expression_selected(keyword) {
            return;
        }
        // A loop's else suite transfers break/continue to an enclosing loop,
        // just as a following if at the same indentation does.
        if !try_else {
            self.emit(keyword.range(), operator, || "if False".into());
            return;
        }
        let Some(last) = suite.last() else {
            return;
        };
        let Some(edit) = DisabledTryElse::new(
            keyword.range(),
            last.end(),
            source,
            tokens,
            self.lexical_suite_depth(),
            self.cancelled,
        ) else {
            return;
        };
        self.emit(edit.range, operator, || edit.render(source));
    }

    fn text(&self, range: TextRange) -> &'source str {
        source_text(self.source, range).expect("parsed source span")
    }
}

fn grouped_target(target: &Expr, parent: &Expr, tokens: &Tokens) -> TextRange {
    parenthesized_range(target.into(), parent.into(), tokens).unwrap_or_else(|| target.range())
}

fn comparison_edit(
    tokens: &Tokens,
    gap: TextRange,
    current: CmpOp,
    cancelled: &impl Fn() -> bool,
) -> Option<(TextRange, &'static str, Option<TextRange>)> {
    let (kind, spelling) = match current {
        CmpOp::Eq => (TokenKind::EqEqual, " is "),
        CmpOp::NotEq => (TokenKind::NotEqual, " is not "),
        CmpOp::Is => (TokenKind::Is, "=="),
        CmpOp::IsNot => (TokenKind::Is, "!="),
        _ => return None,
    };
    let mut gap = tokens.in_range(gap).iter().take_while(|_| !cancelled());
    let first = gap.find(|token| token.kind() == kind)?;
    if current == CmpOp::IsNot {
        let second = gap.find(|token| token.kind() == TokenKind::Not)?;
        return Some((
            TextRange::new(first.start(), second.end()),
            spelling,
            Some(TextRange::new(first.end(), second.start())),
        ));
    }
    Some((first.range(), spelling, None))
}

enum Access {
    Attribute(TextRange),
    Subscript {
        brackets: TextRange,
        syntax: KeySyntax,
    },
}

enum KeySyntax {
    Expression,
    Slice,
    ExtendedSlice,
}

struct BinaryTarget {
    receiver: TextRange,
    access: Access,
    displaced_comments: Vec<TextRange>,
}

impl BinaryTarget {
    fn new(
        target: &Expr,
        lhs: TextRange,
        tokens: &Tokens,
        delimiters: &super::source_delimiters::DelimiterDepth,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Self> {
        let (receiver_end, access) = match target {
            Expr::Attribute(attribute) => {
                let gap = TextRange::new(attribute.value.end(), attribute.attr.start());
                let dot = tokens
                    .in_range(gap)
                    .iter()
                    .take_while(|_| !cancelled())
                    .find(|token| token.kind() == TokenKind::Dot)?;
                (dot.start(), Access::Attribute(attribute.attr.range()))
            }
            Expr::Subscript(subscript) => {
                let gap = TextRange::new(subscript.value.end(), subscript.slice.start());
                let open = tokens
                    .in_range(gap)
                    .iter()
                    .take_while(|_| !cancelled())
                    .find(|token| token.kind() == TokenKind::Lsqb)?;
                let brackets = TextRange::new(open.start(), subscript.end());
                if !delimiters.can_enclose(brackets, 2) {
                    return None;
                }
                let syntax = match subscript.slice.as_ref() {
                    Expr::Slice(_) => KeySyntax::Slice,
                    Expr::Tuple(tuple)
                        if !tuple.parenthesized
                            && tuple
                                .elts
                                .iter()
                                .take_while(|_| !cancelled())
                                .any(|item| item.is_slice_expr() || item.is_starred_expr()) =>
                    {
                        KeySyntax::ExtendedSlice
                    }
                    _ => KeySyntax::Expression,
                };
                (open.start(), Access::Subscript { brackets, syntax })
            }
            _ => return None,
        };
        let receiver = TextRange::new(target.start(), receiver_end);
        if cancelled() || !delimiters.can_enclose(receiver, 3) {
            return None;
        }
        let mut displaced_comments = Vec::new();
        for token in tokens.in_range(lhs) {
            if cancelled() {
                return None;
            }
            let range = token.range();
            if token.kind() == TokenKind::Comment
                && !receiver.contains_range(range)
                && !matches!(access, Access::Subscript { brackets, .. } if brackets.contains_range(range))
            {
                displaced_comments.push(range);
            }
        }
        Some(Self {
            receiver,
            access,
            displaced_comments,
        })
    }

    fn render(&self, source: &str, sign: &str, rhs: &str) -> String {
        let receiver = &source[byte_range(self.receiver)];
        let mut arguments = format!("({receiver}");
        if let Some(first) = self.displaced_comments.first() {
            let tail = &source[usize::from(first.end())..];
            let newline = if tail.starts_with("\r\n") {
                "\r\n"
            } else if tail.starts_with('\r') {
                "\r"
            } else {
                "\n"
            };
            // Comments from discarded outer grouping stay inside a delimiter.
            for comment in &self.displaced_comments {
                arguments.push_str(newline);
                arguments.push_str(&source[byte_range(*comment)]);
            }
            arguments.push_str(newline);
        }
        arguments.push(')');
        let (read, retain_destination, write) = match self.access {
            Access::Attribute(attr) => {
                let attr = &source[byte_range(attr)];
                (
                    format!("target[0], target.pop(0).{attr}"),
                    "target.pop(0)",
                    format!("target.pop(0).{attr}"),
                )
            }
            Access::Subscript {
                brackets,
                ref syntax,
            } => {
                let brackets = &source[byte_range(brackets)];
                // A generic alias on the built-in tuple type exposes the
                // native subscription key as its arguments. Only a single
                // slice needs unpacking from that argument tuple.
                let key = match syntax {
                    KeySyntax::Expression => format!("({})", &brackets[1..brackets.len() - 1]),
                    KeySyntax::Slice => format!("().__class__{brackets}.__args__[0]"),
                    KeySyntax::ExtendedSlice => format!("().__class__{brackets}.__args__"),
                };
                arguments.push_str(", ");
                arguments.push_str(&key);
                (
                    "target[0], target[1], target.pop(0)[target.pop()]".into(),
                    "target.pop(0), target.pop(0)",
                    "target.pop(0)[target.pop()]".into(),
                )
            }
        };
        // Only generated names live in the lambdas. User expressions run as
        // arguments in their original frame, with the getter before the RHS.
        // Stage the receiver and key on the expression stack before each
        // dispatch, then restore them only on success. An exception unwinds
        // the stack even when its traceback retains the helper frame.
        format!(
            "(lambda target: (target.extend(({read})), lambda right: \
             (target.extend(({retain_destination}, target.pop() {sign} right.pop())), \
             [None for {write} in (target.pop(),)])[1])[1])\
             ([{arguments}])([({rhs})])"
        )
    }
}

enum GuardPlacement {
    Inline {
        indent: String,
        newline: &'static str,
    },
    Block {
        indentation_ends: Vec<TextSize>,
        newline: &'static str,
    },
}

struct DisabledTryElse {
    range: TextRange,
    colon_end: TextSize,
    guard: GuardPlacement,
}

impl DisabledTryElse {
    fn new(
        keyword: TextRange,
        end: TextSize,
        source: &str,
        tokens: &Tokens,
        suite_depth: usize,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Self> {
        if suite_depth >= 99 {
            return None;
        }
        let colon = tokens
            .in_range(TextRange::new(keyword.end(), end))
            .iter()
            .take_while(|_| !cancelled())
            .find(|token| token.kind() == TokenKind::Colon)?;
        let colon_end = colon.end();
        let range = TextRange::new(keyword.start(), end);
        let previous_line = tokens
            .before(keyword.start())
            .iter()
            .rev()
            .take_while(|_| !cancelled())
            .find(|token| {
                matches!(
                    token.kind(),
                    TokenKind::Newline | TokenKind::NonLogicalNewline
                )
            })?;
        let previous_newline = newline_at(source, previous_line.range());
        let body = tokens.in_range(TextRange::new(colon_end, end));
        let linebreak = body
            .iter()
            .take_while(|_| !cancelled())
            .find(|token| token.kind() != TokenKind::Comment)
            .filter(|token| token.kind() == TokenKind::Newline);
        let guard = if let Some(linebreak) = linebreak {
            GuardPlacement::Block {
                indentation_ends: logical_indentation_ends(source, body, suite_depth, cancelled)?,
                newline: newline_at(source, linebreak.range()),
            }
        } else {
            let prefix = &source[byte_range(TextRange::new(previous_line.end(), keyword.start()))];
            GuardPlacement::Inline {
                indent: prefix[super::effective_indentation(prefix)].to_owned(),
                newline: previous_newline,
            }
        };
        (!cancelled()).then_some(Self {
            range,
            colon_end,
            guard,
        })
    }

    fn render(&self, source: &str) -> String {
        let text = |start, end| &source[byte_range(TextRange::new(start, end))];
        let mut rendered = text(self.range.start(), self.colon_end).to_owned();
        match &self.guard {
            GuardPlacement::Inline { indent, newline } => {
                rendered.push_str(newline);
                rendered.push_str(indent);
                rendered.push_str(" if False:");
                rendered.push_str(text(self.colon_end, self.range.end()));
            }
            GuardPlacement::Block {
                indentation_ends,
                newline,
            } => {
                let mut cursor = self.colon_end;
                for (index, &split) in indentation_ends.iter().enumerate() {
                    rendered.push_str(text(cursor, split));
                    if index == 0 {
                        rendered.push_str("if False:");
                        rendered.push_str(newline);
                        let prefix = &source[..usize::from(split)];
                        let line = prefix.rsplit(['\n', '\r']).next().expect("line prefix");
                        rendered.push_str(line);
                    }
                    rendered.push(' ');
                    cursor = split;
                }
                rendered.push_str(text(cursor, self.range.end()));
            }
        }
        rendered
    }
}

fn newline_at(source: &str, range: TextRange) -> &'static str {
    match &source[byte_range(range)] {
        "\r" => "\r",
        "\r\n" => "\r\n",
        _ => "\n",
    }
}

fn logical_indentation_ends(
    source: &str,
    tokens: &[ruff_python_ast::token::Token],
    suite_depth: usize,
    cancelled: &impl Fn() -> bool,
) -> Option<Vec<TextSize>> {
    let mut ends = Vec::new();
    let mut line_start = None;
    let mut depth = suite_depth;
    for token in tokens {
        if cancelled() {
            return None;
        }
        match token.kind() {
            TokenKind::Newline => line_start = Some(token.end()),
            TokenKind::NonLogicalNewline if line_start.is_some() => line_start = Some(token.end()),
            TokenKind::Indent => {
                depth += 1;
                // The inserted guard adds one level to each existing block.
                if depth >= 99 {
                    return None;
                }
            }
            TokenKind::Dedent => depth -= 1,
            TokenKind::Comment | TokenKind::NonLogicalNewline => {}
            _ if let Some(start) = line_start.take() => {
                // One extra column after the original prefix changes both
                // tab-expanded and alternate indent widths by exactly one.
                let prefix = &source[byte_range(TextRange::new(start, token.start()))];
                let bytes = u32::try_from(super::effective_indentation(prefix).end).ok()?;
                ends.push(start + TextSize::new(bytes));
            }
            _ => {}
        }
    }
    (!ends.is_empty()).then_some(ends)
}
