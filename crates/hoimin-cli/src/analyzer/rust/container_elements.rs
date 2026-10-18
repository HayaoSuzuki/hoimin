//! Stream whole-literal edits without interpreting commas inside element expressions.
use super::{deletion, source_text};
use ruff_python_ast::{Expr, ExprContext};
use ruff_text_size::Ranged;
use std::fmt::Write as _;

pub(super) fn emit(
    expression: &Expr,
    source: &str,
    max_candidates: usize,
    cancelled: &impl Fn() -> bool,
    mut candidate: impl FnMut(String) -> bool,
) {
    let (open, close, entries): (_, _, Vec<_>) = match expression {
        Expr::List(list) if list.ctx == ExprContext::Load => (
            '[',
            ']',
            list.elts.iter().map(|value| (None, value)).collect(),
        ),
        Expr::Tuple(tuple) if tuple.ctx == ExprContext::Load && tuple.parenthesized => (
            '(',
            ')',
            tuple.elts.iter().map(|value| (None, value)).collect(),
        ),
        Expr::Dict(dict) if dict.items.iter().all(|item| item.key.is_some()) => (
            '{',
            '}',
            dict.items
                .iter()
                .map(|item| (item.key.as_ref(), &item.value))
                .collect(),
        ),
        _ => return,
    };
    if entries.is_empty()
        || entries
            .iter()
            .any(|(_, value)| matches!(value, Expr::Starred(_)))
        || !deletion::can_remove(expression, cancelled)
    {
        return;
    }
    let mut fragments = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        if cancelled() {
            return;
        }
        let Some(value) = source_text(source, value.range()) else {
            return;
        };
        let key = match key {
            Some(key) => {
                let Some(key) = source_text(source, key.range()) else {
                    return;
                };
                Some(key)
            }
            None => None,
        };
        fragments.push((key, value));
    }
    let mut emitted = 0usize;
    for removed in 0..fragments.len() {
        if cancelled() {
            return;
        }
        if removed > 0 && fragments[removed] == fragments[removed - 1] {
            continue;
        }
        // Shared prefix ordering at this fixed span/operator is emission order.
        if emitted > max_candidates {
            break;
        }
        emitted += 1;
        let mut replacement = String::from(open);
        for (index, fragment) in fragments.iter().enumerate() {
            if cancelled() {
                return;
            }
            if index != removed {
                if let Some(key) = fragment.0 {
                    write!(replacement, "({key}): ").unwrap();
                }
                write!(replacement, "({})", fragment.1).unwrap();
                replacement.push(',');
            }
        }
        replacement.push(close);
        #[cfg(test)]
        super::whole_expression_selection_tests::record_replacement(
            super::MutationOperator::ContainerElementDelete,
            &replacement,
        );
        if !candidate(replacement) {
            return;
        }
    }
}
