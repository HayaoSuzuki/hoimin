//! Narrow sorting-key mutations.
//!
//! Reads in a sorting key can run user code, so their evaluation order intentionally moves.
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprCall, Number, token::Tokens};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashSet;

#[expect(
    clippy::too_many_arguments,
    reason = "the shared call emitter receives source, tokens, selection, identity and cancellation evidence"
)]
pub(super) fn emit(
    call: &ExprCall,
    source: &str,
    tokens: &Tokens,
    operators: &MutationOperatorSelection,
    sorted_is_builtin: bool,
    cancelled: &impl Fn() -> bool,
    mut selected: impl FnMut(TextRange) -> bool,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) -> bool {
    if cancelled() {
        return false;
    }
    let sorting = operators.contains(MutationOperator::SortKeySwap)
        && sorted_is_builtin
        && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "sorted");
    if !sorting {
        return true;
    }
    // Dynamic expansions can supply duplicate keywords or change the call
    // contract. Reject them before the producer allocates a replacement.
    let mut names = HashSet::new();
    for keyword in &call.arguments.keywords {
        if cancelled() {
            return false;
        }
        let Some(name) = keyword.arg.as_ref() else {
            return true;
        };
        if !names.insert(name.as_str()) {
            return true;
        }
    }
    for argument in &call.arguments.args {
        if cancelled() {
            return false;
        }
        if matches!(argument, Expr::Starred(_)) {
            return true;
        }
    }
    if let Some((range, replacement)) = sort_swap(call, source, tokens, cancelled, &mut selected)
        && (cancelled() || !emit(range, replacement, MutationOperator::SortKeySwap))
    {
        return false;
    }
    !cancelled()
}

fn sort_swap(
    call: &ExprCall,
    source: &str,
    tokens: &Tokens,
    cancelled: &impl Fn() -> bool,
    selected: &mut impl FnMut(TextRange) -> bool,
) -> Option<(TextRange, String)> {
    if call.arguments.args.len() != 1 {
        return None;
    }
    let mut key = None;
    for keyword in &call.arguments.keywords {
        if cancelled() {
            return None;
        }
        match keyword.arg.as_ref()?.as_str() {
            "key" => key = Some(&keyword.value),
            "reverse" => {}
            _ => return None,
        }
    }
    let Expr::Lambda(lambda) = key? else {
        return None;
    };
    let Expr::Tuple(tuple) = lambda.body.as_ref() else {
        return None;
    };
    let [first, second] = tuple.elts.as_slice() else {
        return None;
    };
    if !simple_read(first, cancelled)
        || !simple_read(second, cancelled)
        || same_read(first, second, cancelled)
        || cancelled()
    {
        return None;
    }
    let first_range =
        ruff_python_ast::token::parenthesized_range(first.into(), tuple.into(), tokens)
            .unwrap_or_else(|| first.range());
    let second_range =
        ruff_python_ast::token::parenthesized_range(second.into(), tuple.into(), tokens)
            .unwrap_or_else(|| second.range());
    let range = TextRange::new(first_range.start(), second_range.end());
    if !selected(range) || cancelled() {
        return None;
    }
    let first_text = super::source_text(source, first_range)?;
    let second_text = super::source_text(source, second_range)?;
    // Both neighboring slices validated these UTF-8 endpoints; parser tuple
    // ordering places the separator between them within the same source.
    let separator = &source[usize::from(first_range.end())..usize::from(second_range.start())];
    Some((range, format!("{second_text}{separator}{first_text}")))
}

fn simple_read(expression: &Expr, cancelled: &impl Fn() -> bool) -> bool {
    if cancelled() {
        return false;
    }
    match expression {
        Expr::Name(_) => true,
        Expr::Attribute(attribute) => simple_read(&attribute.value, cancelled),
        Expr::Subscript(subscript) => {
            simple_read(&subscript.value, cancelled) && literal_index(&subscript.slice)
        }
        _ => false,
    }
}

fn literal_index(expression: &Expr) -> bool {
    matches!(expression, Expr::StringLiteral(value) if !value.value.is_implicit_concatenated())
        || matches!(expression, Expr::NumberLiteral(value) if matches!(value.value, Number::Int(_)))
}

fn same_read(first: &Expr, second: &Expr, cancelled: &impl Fn() -> bool) -> bool {
    if cancelled() {
        return false;
    }
    match (first, second) {
        (Expr::Name(a), Expr::Name(b)) => a.id == b.id,
        (Expr::Attribute(a), Expr::Attribute(b)) => {
            a.attr.as_str() == b.attr.as_str() && same_read(&a.value, &b.value, cancelled)
        }
        (Expr::Subscript(a), Expr::Subscript(b)) => {
            same_read(&a.value, &b.value, cancelled)
                && match (a.slice.as_ref(), b.slice.as_ref()) {
                    (Expr::StringLiteral(a), Expr::StringLiteral(b)) => {
                        a.value.to_str() == b.value.to_str()
                    }
                    (Expr::NumberLiteral(a), Expr::NumberLiteral(b)) => a.value == b.value,
                    _ => false,
                }
        }
        _ => false,
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn run_same(first: &Expr, second: &Expr, cancelled: &dyn Fn() -> bool) -> bool {
        same_read(first, second, &cancelled)
    }

    #[rstest::rstest]
    #[case("first", "first", false, true)]
    #[case("first", "second", false, false)]
    #[case("item.key", "item.key", false, true)]
    #[case("item.key", "item.other", false, false)]
    #[case("item['key']", "item['key']", false, true)]
    #[case("item[1]", "item[1]", false, true)]
    #[case("item[1]", "item['1']", false, false)]
    #[case("item", "item.key", false, false)]
    #[case("item", "item", true, false)]
    fn sorting_read_identity_preserves_access_shape_and_literal_type(
        #[case] first: &str,
        #[case] second: &str,
        #[case] cancelled: bool,
        #[case] expected: bool,
    ) {
        let first = ruff_python_parser::parse_expression(first).unwrap();
        let second = ruff_python_parser::parse_expression(second).unwrap();

        let actual = run_same(&first.syntax().body, &second.syntax().body, &|| cancelled);

        assert_eq!(actual, expected);
    }

    fn run_sort(
        call: &ExprCall,
        source: &str,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
        mut selected: &mut dyn FnMut(TextRange) -> bool,
    ) -> Option<(TextRange, String)> {
        sort_swap(call, source, tokens, &cancelled, &mut selected)
    }

    #[rstest::rstest]
    #[case("", true, false)]
    #[case("sorted(items, key=lambda item: (item.a,", true, false)]
    #[case("sorted(items, key=lambda item: (item.a, item.b))", false, false)]
    #[case("sorted(items, key=lambda item: (item.a, item.b))", true, true)]
    fn sorting_key_edits_require_available_source_and_selected_span(
        #[case] source: &str,
        #[case] selected: bool,
        #[case] expected: bool,
    ) {
        let parsed =
            ruff_python_parser::parse_module("sorted(items, key=lambda item: (item.a, item.b))")
                .unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();

        let actual =
            run_sort(call, source, parsed.tokens(), &|| false, &mut |_| selected).is_some();

        assert_eq!(actual, expected);
    }
    #[rstest::rstest]
    #[case("sorted(items, **options)", false)]
    #[case("sorted(first, second, key=lambda item: (item.a, item.b))", false)]
    #[case("sorted(items, key=lambda item: item.a)", false)]
    #[case("sorted(items, key=lambda item: (item.a, item.b, item.c))", false)]
    #[case("sorted(items, key=lambda item: (item[dynamic], item.b))", false)]
    #[case("sorted(items, key=lambda item: (item[1], item['a']))", true)]
    fn sorting_keys_reject_dynamic_contracts_but_allow_distinct_literal_reads(
        #[case] source: &str,
        #[case] expected: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let result = run_sort(call, source, parsed.tokens(), &|| false, &mut |_| true);

        let actual = result.is_some();

        assert_eq!(actual, expected);
    }

    #[rstest::rstest]
    fn sorting_key_stops_at_each_receiver_and_identity_phase(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12)] checkpoint: usize,
    ) {
        let source = "sorted(items, key=lambda item: (item.a, item[1]))";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let total = std::cell::Cell::new(0);
        let baseline_emitted = run_sort(
            call,
            source,
            parsed.tokens(),
            &|| {
                total.set(total.get() + 1);
                false
            },
            &mut |_| true,
        )
        .is_some();
        let polls = std::cell::Cell::new(0);
        let result = run_sort(
            call,
            source,
            parsed.tokens(),
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
            &mut |_| true,
        );
        let cancelled_emitted = result.is_some();

        assert!(baseline_emitted);
        assert_eq!(cancelled_emitted, checkpoint >= total.get());
    }
    fn run_call(
        call: &ExprCall,
        source: &str,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
        selected: &mut dyn FnMut(TextRange) -> bool,
        sink: &mut dyn FnMut(TextRange, String, MutationOperator) -> bool,
    ) -> bool {
        let operators = serde_json::from_value(serde_json::json!([
            "sort_key_swap",
            "encoding_errors_ignore"
        ]))
        .unwrap();
        emit(
            call, source, tokens, &operators, true, &cancelled, selected, sink,
        )
    }
    fn collecting(
        replacements: &mut Vec<String>,
        accepted: bool,
    ) -> impl FnMut(TextRange, String, MutationOperator) -> bool + '_ {
        move |_, replacement, _| {
            replacements.push(replacement);
            accepted
        }
    }

    #[rstest::rstest]
    #[case("sorted(items, key=lambda item: (item.a, item.b))", true, true, vec!["item.b, item.a"], true)]
    #[case("sorted(items, key=lambda item: (item.a, item.b), key=other)", true, true, vec![], true)]
    #[case("other(items)", true, true, vec![], true)]
    fn followup_call_contracts_reject_duplicate_or_expanded_arguments_and_stop_on_sink_rejection(
        #[case] source: &str,
        #[case] selected: bool,
        #[case] accepted: bool,
        #[case]
        #[by_ref]
        expected: &[&str],
        #[case] completed: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let mut replacements = Vec::new();

        let actual = run_call(
            call,
            source,
            parsed.tokens(),
            &|| false,
            &mut |_| selected,
            &mut collecting(&mut replacements, accepted),
        );

        assert_eq!(actual, completed);
        assert_eq!(replacements, expected);
    }
}
