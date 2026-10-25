//! Local mutations of missing values, state ownership, scope and delegation.
use super::{function_entry, is_simple_receiver};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{CmpOp, Expr, ExprCompare, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

/// A single identity comparison with exactly one None operand.
fn none_operand(compare: &ExprCompare) -> Option<(&Expr, &Expr, bool)> {
    if compare.ops.len() != 1 || compare.operands.len() != 2 {
        return None;
    }
    let negated = match compare.ops[0] {
        CmpOp::Is => false,
        CmpOp::IsNot => true,
        _ => return None,
    };
    let left = &compare.operands[0];
    let right = &compare.operands[1];
    match (
        matches!(left, Expr::NoneLiteral(_)),
        matches!(right, Expr::NoneLiteral(_)),
    ) {
        (true, false) => Some((right, left, negated)),
        (false, true) => Some((left, right, negated)),
        _ => None,
    }
}

pub(super) fn expression<F: Fn() -> bool>(
    expression: &Expr,
    source: &str,
    tokens: &Tokens,
    operators: &MutationOperatorSelection,
    cancelled: &F,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    if cancelled() {
        return;
    }
    match expression {
        Expr::Compare(compare) if operators.contains(MutationOperator::NoneTruthiness) => {
            if let Some(replacement) = none_replacement(compare, source, tokens, cancelled) {
                emit(
                    compare.range(),
                    replacement,
                    MutationOperator::NoneTruthiness,
                );
            }
        }
        Expr::YieldFrom(value) if operators.contains(MutationOperator::YieldFromToYield) => {
            for token in tokens.in_range(TextRange::new(value.start(), value.value.start())) {
                if cancelled() {
                    return;
                }
                if token.kind() == TokenKind::From {
                    emit(
                        token.range(),
                        String::new(),
                        MutationOperator::YieldFromToYield,
                    );
                    return;
                }
            }
        }
        Expr::Call(call) if operators.contains(MutationOperator::MappingSetdefaultToGet) => {
            let Expr::Attribute(attribute) = call.func.as_ref() else {
                return;
            };
            if attribute.attr.as_str() == "setdefault"
                && is_simple_receiver(&attribute.value)
                && (1..=2).contains(&call.arguments.args.len())
                && call.arguments.keywords.is_empty()
                && !call.arguments.args.iter().any(Expr::is_starred_expr)
                && !cancelled()
            {
                emit(
                    attribute.attr.range(),
                    "get".to_owned(),
                    MutationOperator::MappingSetdefaultToGet,
                );
            }
        }
        _ => {}
    }
}

fn none_replacement<F: Fn() -> bool>(
    compare: &ExprCompare,
    source: &str,
    tokens: &Tokens,
    cancelled: &F,
) -> Option<String> {
    let (_, none, negated) = none_operand(compare)?;
    // Keep trivia and the surviving operand's grouping. None's own grouping
    // must disappear too: retaining it would introduce an empty tuple/call.
    let mut edits = vec![none.range()];
    let operator_range = TextRange::new(compare.operands[0].end(), compare.operands[1].start());
    let mut operator_start = None;
    let mut operator_end = None;
    for token in tokens.in_range(operator_range) {
        if cancelled() {
            return None;
        }
        if token.kind() == TokenKind::Is {
            operator_start = Some(token.start());
        }
        if matches!(token.kind(), TokenKind::Is | TokenKind::Not) {
            operator_end = Some(token.end());
            edits.push(token.range());
        }
    }
    let none_range = if matches!(&compare.operands[0], Expr::NoneLiteral(_)) {
        TextRange::new(compare.start(), operator_start?)
    } else {
        TextRange::new(operator_end?, compare.end())
    };
    for token in tokens.in_range(none_range) {
        if cancelled() {
            return None;
        }
        if matches!(token.kind(), TokenKind::Lpar | TokenKind::Rpar) {
            edits.push(token.range());
        }
    }
    if cancelled() {
        return None;
    }
    let start = usize::from(compare.start());
    let mut original = source[start..usize::from(compare.end())].to_owned();
    edits.sort_unstable_by_key(|range| std::cmp::Reverse(range.start()));
    for edit in edits {
        if cancelled() {
            return None;
        }
        original.replace_range(
            usize::from(edit.start()) - start..usize::from(edit.end()) - start,
            "",
        );
    }
    let prefix = if negated { "not not" } else { "not" };
    Some(format!("({prefix} ({original}))"))
}

pub(super) fn statement<F: Fn() -> bool>(
    statement: &Stmt,
    operators: &MutationOperatorSelection,
    cancelled: &F,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    if cancelled() {
        return;
    }
    match statement {
        Stmt::Nonlocal(value) if operators.contains(MutationOperator::NonlocalRemove) => {
            emit(
                value.range(),
                "pass".to_owned(),
                MutationOperator::NonlocalRemove,
            );
        }
        Stmt::FunctionDef(function)
            if operators.contains(MutationOperator::MutableDefaultShare) =>
        {
            if let Some((range, replacement)) = shared_default(function, cancelled) {
                emit(
                    range,
                    replacement.to_owned(),
                    MutationOperator::MutableDefaultShare,
                );
            }
        }
        _ => {}
    }
}

fn shared_default<F: Fn() -> bool>(
    function: &StmtFunctionDef,
    cancelled: &F,
) -> Option<(TextRange, &'static str)> {
    if !function.decorator_list.is_empty() || function.type_params.is_some() {
        return None;
    }
    let Stmt::If(initializer) = function_entry::first_statement(function)? else {
        return None;
    };
    if !initializer.elif_else_clauses.is_empty() || initializer.body.len() != 1 {
        return None;
    }
    let Expr::Compare(compare) = initializer.test.as_ref() else {
        return None;
    };
    let (operand, _, negated) = none_operand(compare)?;
    if negated {
        return None;
    }
    let Expr::Name(name) = operand else {
        return None;
    };
    let Stmt::Assign(assignment) = &initializer.body[0] else {
        return None;
    };
    let [Expr::Name(target)] = assignment.targets.as_slice() else {
        return None;
    };
    if name.id != target.id {
        return None;
    }
    let replacement = match assignment.value.as_ref() {
        Expr::List(value) if value.elts.is_empty() => "[]",
        Expr::Dict(value) if value.items.is_empty() => "{}",
        _ => return None,
    };
    for parameter in function.parameters.iter_non_variadic_params() {
        if cancelled() {
            return None;
        }
        if parameter.parameter.name.as_str() == name.id.as_str() {
            let default = parameter.default.as_deref()?;
            return matches!(default, Expr::NoneLiteral(_))
                .then_some((default.range(), replacement));
        }
    }
    None
}
