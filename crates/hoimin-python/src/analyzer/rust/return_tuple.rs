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

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[rstest::rstest]
    #[case("", None)]
    #[case("(first", None)]
    #[case("(first, second)", Some("(second, first)"))]
    fn swapping_requires_both_original_element_source_ranges(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_expression("(first, second)").unwrap();
        let tuple = parsed.syntax().body.as_tuple_expr().unwrap();

        let actual = replacement(tuple, source);

        assert_eq!(actual, expected.map(str::to_owned));
    }
    #[rstest::rstest]
    #[case("(same, same)", None)]
    #[case("(1, 1)", None)]
    #[case("(-1, second)", Some("(second, -1)"))]
    #[case("(+1, second)", Some("(second, +1)"))]
    #[case("(-value, second)", None)]
    #[case("(not value, second)", None)]
    #[case("('one' 'two', second)", None)]
    #[case("(call(), second)", None)]
    #[case("(None, True)", Some("(True, None)"))]
    #[case("('one', second)", Some("(second, 'one')"))]
    fn tuple_swapping_preserves_atomicity_and_rejects_unchanged_elements(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let tuple = parsed.syntax().body.as_tuple_expr().unwrap();

        let actual = replacement(tuple, source);

        assert_eq!(actual, expected.map(str::to_owned));
    }

    #[rstest::rstest]
    #[case("return'x', name", "(name, 'x')")]
    #[case("return 'x', name", "name, 'x'")]
    fn unparenthesized_return_swap_cannot_merge_with_the_return_keyword(
        #[case] statement: &str,
        #[case] expected: &str,
    ) {
        let source = format!("def function(): {statement}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let tuple = definition.body[0]
            .as_return_stmt()
            .unwrap()
            .value
            .as_ref()
            .unwrap()
            .as_tuple_expr()
            .unwrap();

        let actual = replacement(tuple, &source).unwrap();

        assert_eq!(actual, expected);
    }
}
