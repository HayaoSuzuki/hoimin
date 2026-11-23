use super::*;
use crate::visitor::{self, Visitor};
use proptest::prelude::*;
use rstest::rstest;
use ruff_python_ast::{Expr, Stmt};

type Edit = (TextRange, String, MutationOperator);

fn edits(source: &str, max_candidates: usize, stop_after_first: bool) -> Vec<Edit> {
    struct Walk<'a, F> {
        source: &'a str,
        tokens: &'a Tokens,
        delimiters: &'a DelimiterDepth,
        operators: &'a MutationOperatorSelection,
        max_candidates: usize,
        emit: F,
    }
    impl<'ast, F: FnMut(TextRange, String, MutationOperator) -> bool> Visitor<'ast> for Walk<'_, F> {
        fn visit_expr(&mut self, node: &'ast Expr) {
            expression(
                node,
                self.source,
                self.tokens,
                self.delimiters,
                Options {
                    operators: self.operators,
                    max_candidates: self.max_candidates,
                },
                &|| false,
                |range, operator, replacement| (self.emit)(range, replacement(), operator),
            );
            visitor::walk_expr(self, node);
        }
        fn visit_stmt(&mut self, node: &'ast Stmt) {
            statement(
                node,
                self.source,
                self.delimiters,
                self.operators,
                &|| false,
                &mut self.emit,
            );
            visitor::walk_stmt(self, node);
        }
    }
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let tokens = parsed.tokens();
    let delimiters = DelimiterDepth::build(tokens, &|| false).unwrap();
    let mut operators = MutationOperatorSelection::default();
    operators.include(MutationOperator::ConditionConstant);
    operators.include(MutationOperator::ConditionClauseDelete);
    let mut edits = Vec::new();
    Walk {
        source,
        tokens,
        delimiters: &delimiters,
        operators: &operators,
        max_candidates,
        emit: |span, replacement, operator| {
            edits.push((span, replacement, operator));
            !stop_after_first
        },
    }
    .visit_body(&parsed.syntax().body);
    edits
}

#[rstest]
#[case(
    "out = a() if test() else b()",
    MutationOperator::ConditionalBranchSwap,
    "out = ((b()) if (test()) else (a()))"
)]
#[case(
    "async def f(): return (await (a or b))()",
    MutationOperator::AwaitRemove,
    "async def f(): return ((a or b))()"
)]
#[case(
    "async def f(): await 'text'",
    MutationOperator::AwaitRemove,
    "async def f(): ('text' if True else None)"
)]
#[case(
    "out = {**(a), **((b)), 'tail': 1}",
    MutationOperator::DictUnpackOrderSwap,
    "out = {**(lambda left, right: {**right, **left})({**(a)}, {**((b))}), 'tail': 1}"
)]
#[case(
    "out = {**(source := read()), 'tail': 1}",
    MutationOperator::DictUnpackDelete,
    "out = {**(({} if True else (source := read()))), 'tail': 1}"
)]
#[case(
    "out = {**get(row) for row in rows}",
    MutationOperator::DictUnpackDelete,
    "out = {**({} if True else (get(row))) for row in rows}"
)]
#[case(
    "def f(): del name, obj[name]; return name",
    MutationOperator::DelRemove,
    "def f(): assert True, ( (name := None), obj[name]); return name"
)]
fn emits_one_independently_parseable_edit(
    #[case] source: &str,
    #[case] operator: MutationOperator,
    #[case] expected: &str,
) {
    let edits = edits(source, 100, false);

    let mut selected = edits.into_iter().filter(|(_, _, op)| *op == operator);
    let (range, replacement, _) = selected.next().expect("operator generates an edit");
    let mut changed = source.to_owned();
    changed.replace_range(super::super::byte_range(range), &replacement);

    assert_eq!(changed, expected);
    assert!(selected.next().is_none());
    ruff_python_parser::parse_module(&changed).unwrap();
}

#[test]
fn dictionary_stops_when_emitter_declines() {
    let source = "out = {**a, **b, **c}";

    let emitted = edits(source, 100, true);

    assert_eq!(emitted.len(), 1);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn adjacent_unpack_swap_spans_exactly_two_items(
        before in 0..5_usize,
        after in 0..5_usize,
        depth in 0..6_usize,
    ) {
        let items: Vec<_> = (0..before + after + 2)
            .map(|index| format!("**{}source_{index}{}", "(".repeat(depth), ")".repeat(depth)))
            .collect();
        let source = format!("out = {{{}}}", items.join(", # next\n  "));

        let swaps: Vec<_> = edits(&source, 100, false)
            .into_iter()
            .filter(|(_, _, op)| *op == MutationOperator::DictUnpackOrderSwap)
            .collect();
        let (span, replacement, _) = &swaps[before];
        let range = super::super::byte_range(*span);
        let mut changed = source.clone();
        changed.replace_range(range.clone(), replacement);

        prop_assert_eq!(swaps.len(), items.len() - 1);
        prop_assert_eq!(
            &source[range],
            format!("{}, # next\n  {}", items[before], items[before + 1]),
        );
        prop_assert!(ruff_python_parser::parse_module(&changed).is_ok(), "{changed}");
    }

    #[test]
    fn clause_deletions_keep_original_test_and_bound_equal_rank_edits(
        before in 0..5_usize,
        after in 1..6_usize,
        additional_budget in 0..4_usize,
    ) {
        let clauses: Vec<_> = (0..=before + after)
            .map(|index| format!("clause_{index}()"))
            .collect();
        let test = clauses.join(" and ");
        let source = format!("out = yes if {test} else no");
        let budget = before + additional_budget;

        let deletions: Vec<_> = edits(&source, budget, false)
            .into_iter()
            .filter(|(_, _, op)| *op == MutationOperator::ConditionClauseDelete)
            .collect();
        let (span, replacement, _) = &deletions[before];
        let (live, unreachable) = replacement.split_once(" if True else ").unwrap();
        let mut changed = source.clone();
        changed.replace_range(super::super::byte_range(*span), replacement);

        prop_assert_eq!(deletions.len(), clauses.len().min(budget + 1));
        prop_assert!(!live.contains(&clauses[before]));
        prop_assert_eq!(unreachable, format!("({test}))"));
        prop_assert!(ruff_python_parser::parse_module(&changed).is_ok(), "{changed}");
    }
}
