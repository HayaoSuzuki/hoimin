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
    let Some(first) = generator.generators.first() else {
        return false;
    };
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
                if let Some(first) = generator.generators.first() {
                    self.visit_expr(&first.iter);
                }
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
