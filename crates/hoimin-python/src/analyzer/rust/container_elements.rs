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

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn run(
        expression: &Expr,
        source: &str,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
        sink: &mut dyn FnMut(String) -> bool,
    ) {
        emit(expression, source, limit, &cancelled, sink);
    }

    fn collecting(replacements: &mut Vec<String>) -> impl FnMut(String) -> bool + '_ {
        |replacement| {
            replacements.push(replacement);
            true
        }
    }

    #[rstest::rstest]
    #[case("[value]", "")]
    #[case("{key: value}", "{key: ")]
    #[case("{key: value}", "{")]
    #[case("{ab: value}", "{aé:xxxxx}")]
    fn incomplete_source_never_emits_a_partial_literal(
        #[case] original: &str,
        #[case] source: &str,
    ) {
        let parsed = ruff_python_parser::parse_expression(original).unwrap();
        let mut replacements = Vec::new();
        run(
            &parsed.syntax().body,
            source,
            100,
            &|| false,
            &mut collecting(&mut replacements),
        );
        assert_eq!(replacements, Vec::<String>::new());
    }

    #[test]
    fn zero_candidate_limit_retains_only_the_first_overflow_witness() {
        let source = "[first, second, third]";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let mut replacements = Vec::new();
        run(
            &parsed.syntax().body,
            source,
            0,
            &|| false,
            &mut collecting(&mut replacements),
        );
        assert_eq!(replacements, ["[(second),(third),]"]);
    }

    #[test]
    fn rejected_literal_stops_before_its_next_deletion() {
        let source = "[first, second]";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let mut replacements = Vec::new();
        run(
            &parsed.syntax().body,
            source,
            100,
            &|| false,
            &mut |replacement| {
                replacements.push(replacement);
                false
            },
        );
        assert_eq!(replacements, ["[(second),]"]);
    }
    #[rstest::rstest]
    #[case("(first, second)", vec!["((second),)", "((first),)"])]
    #[case("{first: value, second: other}", vec!["{(second): (other),}", "{(first): (value),}"])]
    #[case("[same, same]", vec!["[(same),]"])]
    #[case("[]", vec![])]
    #[case("[*items]", vec![])]
    #[case("{**items}", vec![])]
    #[case("items", vec![])]
    fn literal_deletion_preserves_entry_spelling_and_omits_duplicate_edits(
        #[case] source: &str,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let mut replacements = Vec::new();
        run(
            &parsed.syntax().body,
            source,
            100,
            &|| false,
            &mut collecting(&mut replacements),
        );
        assert_eq!(replacements, expected);
    }

    #[rstest::rstest]
    fn cancellation_stops_deletion_before_any_extra_poll_or_emission(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15)] checkpoint: usize,
    ) {
        let source = "[first, second, third]";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let total = std::cell::Cell::new(0);
        let mut complete = Vec::new();
        run(
            &parsed.syntax().body,
            source,
            100,
            &|| {
                total.set(total.get() + 1);
                false
            },
            &mut collecting(&mut complete),
        );
        let polls = std::cell::Cell::new(0);
        let mut partial = Vec::new();
        run(
            &parsed.syntax().body,
            source,
            100,
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
            &mut collecting(&mut partial),
        );
        assert_eq!(polls.get(), total.get().min(checkpoint + 1));
        assert!(complete.starts_with(&partial));
    }
}
