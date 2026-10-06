//! Stream whole-literal edits without interpreting commas inside element expressions.
use super::{deletion, source_text};
use ruff_python_ast::{Expr, ExprContext};
use ruff_text_size::Ranged;

pub(super) fn emit(
    expression: &Expr,
    source: &str,
    max_candidates: usize,
    cancelled: &impl Fn() -> bool,
    mut candidate: impl FnMut(String),
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
        let fragment = if let Some(key) = key {
            let Some(key) = source_text(source, key.range()) else {
                return;
            };
            format!("({key}): ({value})")
        } else {
            format!("({value})")
        };
        fragments.push(fragment);
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
                replacement.push_str(fragment);
                replacement.push(',');
            }
        }
        replacement.push(close);
        candidate(replacement);
    }
}
