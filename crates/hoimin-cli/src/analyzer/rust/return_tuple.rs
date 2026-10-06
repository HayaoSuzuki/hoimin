//! Swap two atomic return-tuple elements while retaining source separators.
use ruff_python_ast::{Expr, ExprTuple, UnaryOp};
use ruff_text_size::Ranged;

pub(super) fn replacement(tuple: &ExprTuple, source: &str) -> Option<String> {
    let [first, second] = tuple.elts.as_slice() else {
        return None;
    };
    if !atomic(first) || !atomic(second) {
        return None;
    }
    if let (Expr::Name(a), Expr::Name(b)) = (first, second)
        && a.id == b.id
    {
        return None;
    }
    let first_text = super::source_text(source, first.range())?;
    let second_text = super::source_text(source, second.range())?;
    if first_text == second_text {
        return None;
    }
    let start = usize::from(tuple.start());
    let end = usize::from(tuple.end());
    let a = usize::from(first.start());
    let b = usize::from(first.end());
    let c = usize::from(second.start());
    let d = usize::from(second.end());
    let replacement = format!(
        "{}{second_text}{}{first_text}{}",
        &source[start..a],
        &source[b..c],
        &source[d..end]
    );
    // `return'x', name` must not become the identifier `returnname`.
    if !tuple.parenthesized && start > 0 && source.as_bytes()[start - 1].is_ascii_alphabetic() {
        Some(format!("({replacement})"))
    } else {
        Some(replacement)
    }
}

fn atomic(expression: &Expr) -> bool {
    match expression {
        Expr::Name(_) | Expr::NumberLiteral(_) | Expr::BooleanLiteral(_) | Expr::NoneLiteral(_) => {
            true
        }
        Expr::StringLiteral(s) => !s.value.is_implicit_concatenated(),
        Expr::UnaryOp(u) => {
            matches!(u.op, UnaryOp::UAdd | UnaryOp::USub)
                && matches!(u.operand.as_ref(), Expr::NumberLiteral(_))
        }
        _ => false,
    }
}
