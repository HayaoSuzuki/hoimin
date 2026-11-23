//! Expression evaluation and deletion changes that preserve surrounding syntax and scope.
use super::source_delimiters::DelimiterDepth;
use super::{MutationOperator, MutationOperatorSelection, source_text};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{
    BoolOp, CmpOp, Expr, ExprBoolOp, ExprCompare, ExprDict, ExprIf, Stmt, StmtDelete,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

#[derive(Clone, Copy)]
pub(super) struct Options<'a> {
    pub(super) operators: &'a MutationOperatorSelection,
    pub(super) max_candidates: usize,
    /// The statement collector retains the original direct if/elif behavior.
    pub(super) skip_clause_delete: bool,
}

/// Emit the changes to this expression only, stopping when the emitting collector asks.
pub(super) fn expression(
    expression: &Expr,
    source: &str,
    tokens: &Tokens,
    delimiters: &DelimiterDepth,
    options: Options<'_>,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    if cancelled() {
        return;
    }
    let range = expression.range();
    let operators = options.operators;
    match expression {
        Expr::If(conditional) => {
            conditional_edits(
                conditional,
                source,
                delimiters,
                options,
                cancelled,
                &mut emit,
            );
        }
        Expr::BoolOp(boolean)
            if operators.contains(MutationOperator::ConditionClauseDelete)
                && !options.skip_clause_delete =>
        {
            boolean_edits(
                boolean,
                source,
                delimiters,
                options.max_candidates,
                cancelled,
                &mut emit,
            );
        }
        Expr::Compare(comparison) if operators.contains(MutationOperator::CompareDirection) => {
            comparison_edits(comparison, tokens, cancelled, &mut emit);
        }
        Expr::Await(awaited)
            if operators.contains(MutationOperator::AwaitRemove)
                && delimiters.can_enclose(range, 1) =>
        {
            if let Some(operand) = source_text(source, awaited.value.range()) {
                // A standalone string expression can become the suite's docstring.
                emit(range, MutationOperator::AwaitRemove, &|| {
                    if awaited.value.is_string_literal_expr() {
                        format!("({operand} if True else None)")
                    } else {
                        format!("({operand})")
                    }
                });
            }
        }
        Expr::Dict(dictionary)
            if operators.contains(MutationOperator::DictUnpackDelete)
                || operators.contains(MutationOperator::DictUnpackOrderSwap) =>
        {
            dictionary_edits(
                dictionary, source, tokens, delimiters, operators, cancelled, emit,
            );
        }
        Expr::DictComp(comprehension)
            if comprehension.key.is_none()
                && operators.contains(MutationOperator::DictUnpackDelete) =>
        {
            emit_unpack_delete(comprehension.value.range(), source, delimiters, &mut emit);
        }
        _ => {}
    }
}

fn conditional_edits(
    conditional: &ExprIf,
    source: &str,
    delimiters: &DelimiterDepth,
    options: Options<'_>,
    cancelled: &impl Fn() -> bool,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let operators = options.operators;
    let range = conditional.range();
    let test_range = conditional.test.range();
    let Some(original_test) = source_text(source, test_range) else {
        return;
    };
    if operators.contains(MutationOperator::ConditionalBranchSwap)
        && delimiters.can_enclose(range, 2)
        && let (Some(body), Some(alternative)) = (
            source_text(source, conditional.body.range()),
            source_text(source, conditional.orelse.range()),
        )
        && body != alternative
        && !emit(range, MutationOperator::ConditionalBranchSwap, &|| {
            format!("(({alternative}) if ({original_test}) else ({body}))")
        })
    {
        return;
    }
    if !delimiters.can_enclose(test_range, 2) {
        return;
    }
    // Preserve bindings and captures in the original test even if evaluation is suppressed.
    if operators.contains(MutationOperator::ConditionConstant)
        && !conditional.test.is_boolean_literal_expr()
    {
        for prefix in ["True or", "False and"] {
            if cancelled()
                || !emit(test_range, MutationOperator::ConditionConstant, &|| {
                    format!("({prefix} ({original_test}))")
                })
            {
                return;
            }
        }
    }
}

fn boolean_edits(
    boolean: &ExprBoolOp,
    source: &str,
    delimiters: &DelimiterDepth,
    max_candidates: usize,
    cancelled: &impl Fn() -> bool,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let range = boolean.range();
    if boolean.values.len() < 2 || !delimiters.can_enclose(range, 2) {
        return;
    }
    let Some(original) = source_text(source, range) else {
        return;
    };
    let separator = match boolean.op {
        BoolOp::And => " and ",
        BoolOp::Or => " or ",
    };
    let mut previous = None;
    let mut emitted = 0;
    for (removed, operand) in boolean.values.iter().enumerate() {
        if cancelled() || emitted > max_candidates {
            return;
        }
        let text = source_text(source, operand.range());
        if previous == text {
            continue;
        }
        previous = text;
        emitted += 1;
        if !emit(range, MutationOperator::ConditionClauseDelete, &|| {
            let mut replacement = String::from("(");
            let mut first = true;
            for (index, operand) in boolean.values.iter().enumerate() {
                if cancelled() {
                    return String::new();
                }
                if index == removed {
                    continue;
                }
                if !first {
                    replacement.push_str(separator);
                }
                first = false;
                replacement.push('(');
                replacement.push_str(source_text(source, operand.range()).expect("parsed operand"));
                replacement.push(')');
            }
            // The original branch contributes bindings, free reads and yield
            // classification without evaluating any of the removed operand.
            replacement.push_str(" if True else (");
            replacement.push_str(original);
            replacement.push_str("))");
            replacement
        }) {
            return;
        }
    }
}

fn comparison_edits(
    comparison: &ExprCompare,
    tokens: &Tokens,
    cancelled: &impl Fn() -> bool,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let mut left = comparison.first_operand().end();
    for (operator, right) in comparison.ops.iter().zip(comparison.comparators()) {
        if cancelled() {
            return;
        }
        let gap = TextRange::new(left, right.start());
        left = right.end();
        let (kind, replacement) = match operator {
            CmpOp::Lt => (TokenKind::Less, ">"),
            CmpOp::LtE => (TokenKind::LessEqual, ">="),
            CmpOp::Gt => (TokenKind::Greater, "<"),
            CmpOp::GtE => (TokenKind::GreaterEqual, "<="),
            CmpOp::Eq | CmpOp::NotEq | CmpOp::Is | CmpOp::IsNot | CmpOp::In | CmpOp::NotIn => {
                continue;
            }
        };
        let token = tokens
            .in_range(gap)
            .iter()
            .find(|token| cancelled() || token.kind() == kind);
        if cancelled() {
            return;
        }
        if let Some(token) = token
            && !emit(token.range(), MutationOperator::CompareDirection, &|| {
                replacement.to_owned()
            })
        {
            return;
        }
    }
}

fn dictionary_edits(
    dictionary: &ExprDict,
    source: &str,
    tokens: &Tokens,
    delimiters: &DelimiterDepth,
    operators: &MutationOperatorSelection,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let delete = operators.contains(MutationOperator::DictUnpackDelete);
    let swap = operators.contains(MutationOperator::DictUnpackOrderSwap);
    let mut previous = None;
    for item in &dictionary.items {
        if cancelled() {
            return;
        }
        if item.key.is_some() {
            previous = None;
            continue;
        }
        let operand = item.value.range();
        if delete && !emit_unpack_delete(operand, source, delimiters, &mut emit) {
            return;
        }
        if swap {
            let current = unpack_range(tokens, operand, cancelled);
            if let (Some(left), Some(right)) = (previous, current)
                && !emit_unpack_swap(left, right, source, delimiters, &mut emit)
            {
                return;
            }
            previous = current;
        }
    }
}

fn emit_unpack_delete(
    operand: TextRange,
    source: &str,
    delimiters: &DelimiterDepth,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) -> bool {
    if !delimiters.can_enclose(operand, 2) {
        return true;
    }
    let Some(text) = source_text(source, operand) else {
        return true;
    };
    // The dead branch preserves bindings, free reads, and yield classification.
    emit(operand, MutationOperator::DictUnpackDelete, &|| {
        format!("({{}} if True else ({text}))")
    })
}

fn unpack_range(
    tokens: &Tokens,
    operand: TextRange,
    cancelled: &impl Fn() -> bool,
) -> Option<TextRange> {
    // The AST omits the unpack marker and can omit redundant operand parentheses.
    let start = tokens.before(operand.start()).iter().rev().find(|token| {
        cancelled()
            || !matches!(
                token.kind(),
                TokenKind::Lpar | TokenKind::Comment | TokenKind::NonLogicalNewline
            )
    })?;
    if cancelled() || start.kind() != TokenKind::DoubleStar {
        return None;
    }
    let mut end = operand.end();
    for token in tokens.after(operand.end()) {
        if cancelled() {
            return None;
        }
        match token.kind() {
            TokenKind::Rpar => end = token.end(),
            TokenKind::Comment | TokenKind::NonLogicalNewline => {}
            _ => break,
        }
    }
    Some(TextRange::new(start.start(), end))
}

fn emit_unpack_swap(
    left: TextRange,
    right: TextRange,
    source: &str,
    delimiters: &DelimiterDepth,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) -> bool {
    let range = TextRange::new(left.start(), right.end());
    if !delimiters.can_enclose(range, 2) {
        return true;
    }
    let (Some(left_text), Some(gap), Some(right_text)) = (
        source_text(source, left),
        source_text(source, TextRange::new(left.end(), right.start())),
        source_text(source, right),
    ) else {
        return true;
    };
    // Each operand is evaluated and expanded in the original scope and order.
    // In particular a failure while expanding the first operand prevents evaluation of the second.
    emit(range, MutationOperator::DictUnpackOrderSwap, &|| {
        format!("**(lambda left, right: {{**right, **left}})({{{left_text}}}{gap}{{{right_text}}})")
    })
}

/// Disable a del while keeping its lexical effects in a simple statement.
pub(super) fn statement(
    statement: &Stmt,
    source: &str,
    delimiters: &DelimiterDepth,
    operators: &MutationOperatorSelection,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    if let Stmt::Delete(delete) = statement
        && operators.contains(MutationOperator::DelRemove)
        && !cancelled()
        && delimiters.can_enclose(delete.range(), 2)
        && let Some(replacement) = skip_delete(delete, source, cancelled)
    {
        emit(delete.range(), replacement, MutationOperator::DelRemove);
    }
}

fn skip_delete(delete: &StmtDelete, source: &str, cancelled: &impl Fn() -> bool) -> Option<String> {
    // An assertion's unreachable message is legal beside semicolons and in inline suites.
    // Keep attribute and subscript expressions as source, including their nested bindings.
    let mut replacement = String::from("assert True, (");
    let mut cursor = delete.start() + TextSize::new(3); // del keyword
    for target in &delete.targets {
        keep_delete_bindings(target, source, &mut cursor, &mut replacement, cancelled)?;
    }
    replacement.push_str(source_text(source, TextRange::new(cursor, delete.end()))?);
    replacement.push(')');
    Some(replacement)
}

fn keep_delete_bindings(
    target: &Expr,
    source: &str,
    cursor: &mut TextSize,
    replacement: &mut String,
    cancelled: &impl Fn() -> bool,
) -> Option<()> {
    if cancelled() {
        return None;
    }
    let targets = match target {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::Name(name) => {
            // A deleted bare name still counts as a binding for symbol-table purposes.
            replacement.push_str(source_text(source, TextRange::new(*cursor, name.start()))?);
            replacement.push('(');
            replacement.push_str(source_text(source, name.range())?);
            replacement.push_str(" := None)");
            *cursor = name.end();
            return Some(());
        }
        Expr::Attribute(_) | Expr::Subscript(_) => return Some(()),
        _ => return None,
    };
    for target in targets {
        keep_delete_bindings(target, source, cursor, replacement, cancelled)?;
    }
    Some(())
}

#[cfg(test)]
#[path = "expression_behavior/tests.rs"]
mod tests;
