//! Conservative eligibility for removing evaluated expressions.
use ruff_python_ast::{
    Expr,
    visitor::{self, Visitor},
};

pub(super) fn can_remove<F: Fn() -> bool>(expression: &Expr, cancelled: &F) -> bool {
    check(expression, cancelled, false)
}

pub(super) fn conversion_argument<F: Fn() -> bool>(expression: &Expr, cancelled: &F) -> bool {
    check(expression, cancelled, true)
}

fn check<F: Fn() -> bool>(expression: &Expr, cancelled: &F, reject_generators: bool) -> bool {
    let mut checker = RemovalCheck {
        safe: true,
        cancelled,
        reject_generators,
    };
    checker.visit_expr(expression);
    checker.safe
}

struct RemovalCheck<'a, F> {
    safe: bool,
    reject_generators: bool,
    cancelled: &'a F,
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for RemovalCheck<'_, F> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if !self.safe {
            return;
        }
        if (self.cancelled)()
            || (self.reject_generators && matches!(expression, Expr::Generator(_)))
            || matches!(
                expression,
                Expr::Named(_) | Expr::Await(_) | Expr::Yield(_) | Expr::YieldFrom(_)
            )
        {
            self.safe = false;
            return;
        }
        // Walk nested lambdas too: conservative exclusion is intentional.
        visitor::walk_expr(self, expression);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_during_nested_arguments_rejects_removal() {
        let parsed = ruff_python_parser::parse_module("f(g(h(1)))").unwrap();
        let ruff_python_ast::Stmt::Expr(statement) = &parsed.syntax().body[0] else {
            panic!()
        };
        let visits = std::cell::Cell::new(0);
        assert!(!can_remove(&statement.value, &|| {
            visits.set(visits.get() + 1);
            visits.get() == 3
        }));
        assert_eq!(visits.get(), 3);
    }
}
