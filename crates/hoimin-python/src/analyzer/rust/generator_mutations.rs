//! Locally proven generator transformations; no iterator alias inference or draining.
use crate::visitor::{self, Visitor};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprGenerator};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn emit<F: Fn() -> bool>(
    expression: &Expr,
    source: &str,
    operators: &MutationOperatorSelection,
    next_is_builtin: bool,
    cancelled: &F,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    if cancelled() {
        return;
    }
    let range = expression.range();
    match expression {
        Expr::ListComp(_) if operators.contains(MutationOperator::ListComprehensionToGenerator) => {
            let original = &source[usize::from(range.start())..usize::from(range.end())];
            if let Some(interior) = original.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                emit(
                    range,
                    format!("({interior})"),
                    MutationOperator::ListComprehensionToGenerator,
                );
            }
        }
        Expr::Generator(generator)
            if (operators.contains(MutationOperator::GeneratorSkipFirst)
                || operators.contains(MutationOperator::GeneratorEmpty))
                && supported_generator(generator, cancelled) =>
        {
            let original = &source[usize::from(range.start())..usize::from(range.end())];
            if next_is_builtin && operators.contains(MutationOperator::GeneratorSkipFirst) {
                if cancelled() {
                    return;
                }
                let replacement = format!(
                    "(lambda __hoimin_iterator, __hoimin_next: (__hoimin_next(__hoimin_iterator, None), __hoimin_iterator)[1])(({original}), next)"
                );
                if !emit(range, replacement, MutationOperator::GeneratorSkipFirst) {
                    return;
                }
            }
            if operators.contains(MutationOperator::GeneratorEmpty) && !cancelled() {
                emit(
                    range,
                    format!("(({original}), (_ for _ in ()))[1]"),
                    MutationOperator::GeneratorEmpty,
                );
            }
        }
        _ => {}
    }
}

fn supported_generator<F: Fn() -> bool>(generator: &ExprGenerator, cancelled: &F) -> bool {
    // The source analyzer admits only syntax-checked parser trees. A generator
    // expression's grammar requires at least one for-clause.
    let first = generator
        .generators
        .first()
        .expect("checked generators have a for-clause");
    if matches!(&first.iter, Expr::Tuple(value) if value.elts.is_empty())
        || matches!(&first.iter, Expr::List(value) if value.elts.is_empty())
        || generator.generators.iter().any(|item| item.is_async)
    {
        return false;
    }
    let mut frame = AsyncFrame {
        asynchronous: false,
        cancelled_observed: false,
        cancelled,
    };
    frame.visit_expr(&generator.elt);
    for (index, item) in generator.generators.iter().enumerate() {
        if frame.asynchronous || frame.cancelled_observed {
            return false;
        }
        frame.visit_expr(&item.target);
        // The leftmost iterator is evaluated in the enclosing frame during
        // construction; awaits there do not make this generator asynchronous.
        if index > 0 {
            frame.visit_expr(&item.iter);
        }
        for condition in &item.ifs {
            frame.visit_expr(condition);
        }
    }
    !frame.asynchronous && !frame.cancelled_observed && !cancelled()
}

struct AsyncFrame<'a, F> {
    asynchronous: bool,
    cancelled_observed: bool,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for AsyncFrame<'_, F> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.cancelled_observed || (self.cancelled)() {
            self.cancelled_observed = true;
            return;
        }
        if self.asynchronous {
            return;
        }
        match expression {
            Expr::Await(_) => self.asynchronous = true,
            Expr::Generator(generator) => {
                let first = generator
                    .generators
                    .first()
                    .expect("checked generators have a for-clause");
                self.visit_expr(&first.iter);
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
            }
            Expr::ListComp(value) if value.generators.iter().any(|item| item.is_async) => {
                self.asynchronous = true;
            }
            Expr::SetComp(value) if value.generators.iter().any(|item| item.is_async) => {
                self.asynchronous = true;
            }
            Expr::DictComp(value) if value.generators.iter().any(|item| item.is_async) => {
                self.asynchronous = true;
            }
            _ => visitor::walk_expr(self, expression),
        }
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_emit(
        expression: &Expr,
        source: &str,
        operators: &MutationOperatorSelection,
        next_is_builtin: bool,
        cancelled: &dyn Fn() -> bool,
        sink: &mut dyn FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        emit(
            expression,
            source,
            operators,
            next_is_builtin,
            &cancelled,
            sink,
        );
    }

    #[test]
    fn rejected_first_emission_does_not_reach_later_generator_edits() {
        let source = "(item for item in values)";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let operators =
            serde_json::from_str("[\"generator_skip_first\",\"generator_empty\"]").unwrap();
        let mut mutations = Vec::new();
        run_emit(
            &parsed.syntax().body,
            source,
            &operators,
            true,
            &|| false,
            &mut |_, replacement, operator| {
                mutations.push((replacement, operator));
                false
            },
        );

        let actual = mutations.len();
        let actual_2 = mutations[0]
            .0
            .contains("__hoimin_next(__hoimin_iterator, None)");

        assert_eq!(actual, 1);
        assert_eq!(mutations[0].1, MutationOperator::GeneratorSkipFirst);
        assert!(actual_2);
    }

    #[rstest::rstest]
    #[case(0, 0)]
    #[case(1, 0)]
    #[case(2, 0)]
    #[case(3, 0)]
    #[case(4, 0)]
    #[case(5, 1)]
    #[case(6, 2)]
    #[case(64, 2)]
    fn cancellation_preserves_generator_mutation_prefix(
        #[case] checkpoint: usize,
        #[case] expected_count: usize,
    ) {
        let source = "(item for item in values)";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let operators =
            serde_json::from_str("[\"generator_skip_first\",\"generator_empty\"]").unwrap();
        let polls = Cell::new(0);
        let mut emitted = Vec::new();
        run_emit(
            &parsed.syntax().body,
            source,
            &operators,
            true,
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
            &mut |_, _, operator| {
                emitted.push(operator);
                true
            },
        );

        let actual = emitted.len();

        assert_eq!(actual, expected_count);
        assert_eq!(
            emitted,
            [
                MutationOperator::GeneratorSkipFirst,
                MutationOperator::GeneratorEmpty
            ][..emitted.len()]
        );
    }

    #[rstest::rstest]
    #[case("(await first, await second)", true)]
    #[case("[item async for item in values]", true)]
    #[case("{item async for item in values}", true)]
    #[case("{item: item async for item in values}", true)]
    #[case("(item for item in (await values))", true)]
    #[case("(await item for item in values)", false)]
    #[case("lambda arg=(await value): arg", true)]
    #[case("lambda: (await value)", false)]
    fn async_frame_distinguishes_deferred_bodies_from_eager_comprehensions(
        #[case] source: &str,
        #[case] asynchronous: bool,
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let cancelled: &dyn Fn() -> bool = &|| false;
        let mut frame = AsyncFrame {
            asynchronous: false,
            cancelled_observed: false,
            cancelled: &cancelled,
        };
        frame.visit_expr(&parsed.syntax().body);

        assert_eq!(frame.asynchronous, asynchronous);
        assert!(!frame.cancelled_observed);
    }

    #[test]
    fn async_frame_remembers_cancellation_on_later_visits() {
        let parsed = ruff_python_parser::parse_expression("value").unwrap();
        let cancelled: &dyn Fn() -> bool = &|| true;
        let mut frame = AsyncFrame {
            asynchronous: false,
            cancelled_observed: false,
            cancelled: &cancelled,
        };
        frame.visit_expr(&parsed.syntax().body);
        let cancelled_after_first_visit = frame.cancelled_observed;
        frame.visit_expr(&parsed.syntax().body);
        let synchronous_after_second_visit = !frame.asynchronous;

        assert!(cancelled_after_first_visit);
        assert!(synchronous_after_second_visit);
    }
    #[rstest::rstest]
    #[case("(await item for item in values)")]
    #[case("(item async for item in values)")]
    fn supported_generator_rejects_asynchronous_execution(#[case] source: &str) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let generator = parsed.syntax().body.as_generator_expr().unwrap();
        let cancelled: &dyn Fn() -> bool = &|| false;

        let actual = !supported_generator(generator, &cancelled);

        assert!(actual);
    }
    #[rstest::rstest]
    #[case("[item for item in values]", "[item for item in values]", vec!["(item for item in values)"])]
    #[case("[item for item in values]", "(item for item in values)", vec![])]
    #[case("value", "value", vec![])]
    #[case("(item for item in ())", "(item for item in ())", vec![])]
    #[case("(item for item in [])", "(item for item in [])", vec![])]
    fn generator_edits_require_supported_expression_and_source_shapes(
        #[case] parsed_source: &str,
        #[case] source: &str,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(parsed_source).unwrap();
        let operators = serde_json::from_str(
            "[\"list_comprehension_to_generator\",\"generator_skip_first\",\"generator_empty\"]",
        )
        .unwrap();
        let mut replacements = Vec::new();
        run_emit(
            &parsed.syntax().body,
            source,
            &operators,
            true,
            &|| false,
            &mut |_, replacement, _| {
                replacements.push(replacement);
                true
            },
        );

        assert_eq!(replacements, expected);
    }
    #[rstest::rstest]
    #[case("(item for item in values)", "[\"generator_empty\"]", true, vec![MutationOperator::GeneratorEmpty])]
    #[case("(item for item in values)", "[\"generator_skip_first\"]", true, vec![MutationOperator::GeneratorSkipFirst])]
    #[case("(item for item in values)", "[\"generator_skip_first\",\"generator_empty\"]", false, vec![MutationOperator::GeneratorEmpty])]
    #[case("((outer, inner) for outer in values if predicate(outer) for inner in other)", "[\"generator_skip_first\",\"generator_empty\"]", true, vec![MutationOperator::GeneratorSkipFirst, MutationOperator::GeneratorEmpty])]
    fn generator_selection_and_next_identity_control_the_emitted_edits(
        #[case] source: &str,
        #[case] selectors: &str,
        #[case] next_is_builtin: bool,
        #[case]
        #[by_ref]
        expected: &[MutationOperator],
    ) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let operators = serde_json::from_str(selectors).unwrap();
        let mut edits = Vec::new();
        run_emit(
            &parsed.syntax().body,
            source,
            &operators,
            next_is_builtin,
            &|| false,
            &mut |_, _, operator| {
                edits.push(operator);
                true
            },
        );

        assert_eq!(edits, expected);
    }
}
