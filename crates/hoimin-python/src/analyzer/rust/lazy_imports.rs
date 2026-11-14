//! Conservative boundary for module-wide Python 3.15 lazy import configuration.
use super::AnalysisCancelled;
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ModModule, Stmt};

/// A declaration may be dynamic or changed later. Do not prove eager import
/// identity anywhere in that module; this also works when analyzing 3.14 source.
pub(super) fn configuration_present(
    module: &ModModule,
    cancelled: &impl Fn() -> bool,
) -> Result<bool, AnalysisCancelled> {
    struct Scan<'a, F> {
        cancelled: &'a F,
        stopped: bool,
        found: bool,
    }
    impl<'ast, F: Fn() -> bool> Visitor<'ast> for Scan<'_, F> {
        fn visit_body(&mut self, body: &'ast [Stmt]) {
            // The pinned walker dispatches every statement through visit_body,
            // so this loop also guards statements in nested suites.
            for statement in body {
                if self.stopped || self.found {
                    break;
                }
                self.visit_stmt(statement);
            }
        }

        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            self.stopped = (self.cancelled)();
            if self.stopped {
                return;
            }
            if let Stmt::ImportFrom(import) = statement {
                self.found |= import.level == 0
                    && import.module.as_ref().is_some_and(|m| m.as_str() == "sys")
                    && import
                        .names
                        .iter()
                        .any(|a| matches!(a.name.as_str(), "set_lazy_imports" | "*"));
            }
            if self.found {
                return;
            }
            visitor::walk_stmt(self, statement);
        }
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if self.stopped || self.found {
                return;
            }
            self.stopped = (self.cancelled)();
            if self.stopped {
                return;
            }
            self.found |= match expression {
                Expr::Name(name) => name.id.as_str() == "__lazy_modules__",
                Expr::Attribute(attribute) => attribute.attr.as_str() == "set_lazy_imports",
                _ => false,
            };
            if !self.found {
                visitor::walk_expr(self, expression);
            }
        }
    }
    let mut scan = Scan {
        cancelled,
        stopped: false,
        found: false,
    };
    scan.visit_body(&module.body);
    if scan.stopped {
        Err(AnalysisCancelled)
    } else {
        Ok(scan.found)
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    #[rstest::rstest]
    #[case("from sys import set_lazy_imports\nlater = 1\n", 1)]
    #[case(
        "if flag:\n from sys import set_lazy_imports\nelse:\n action()\nlater = 1\n",
        3
    )]
    fn finding_configuration_stops_before_later_statements(
        #[case] source: &str,
        #[case] expected_polls: usize,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let polls = Cell::new(0);

        let actual = configuration_present(parsed.syntax(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= expected_polls
        });
        let actual_2 = polls.get();

        assert_eq!(actual, Ok(true));
        assert_eq!(actual_2, expected_polls);
    }
    #[rstest::rstest]
    #[case(0, Err(AnalysisCancelled))]
    #[case(1, Err(AnalysisCancelled))]
    #[case(2, Err(AnalysisCancelled))]
    #[case(3, Err(AnalysisCancelled))]
    #[case(4, Ok(true))]
    fn cancellation_before_configuration_is_reported(
        #[case] checkpoint: usize,
        #[case] expected: Result<bool, AnalysisCancelled>,
    ) {
        let parsed =
            ruff_python_parser::parse_module("value = 1\nfrom sys import set_lazy_imports\n")
                .unwrap();
        let polls = Cell::new(0);

        let actual = configuration_present(parsed.syntax(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });

        assert_eq!(actual, expected);
    }
}
