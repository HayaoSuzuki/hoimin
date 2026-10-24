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
            for statement in body {
                if self.stopped || self.found {
                    break;
                }
                self.visit_stmt(statement);
            }
        }

        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if self.stopped || self.found {
                return;
            }
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
            if !self.found {
                visitor::walk_stmt(self, statement);
            }
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
