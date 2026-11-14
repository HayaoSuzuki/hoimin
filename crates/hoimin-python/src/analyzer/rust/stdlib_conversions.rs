//! Explicit conversion identities and evidence for factory-method stability.
//!
//! Constructors and factory methods share one mutation. Factory methods need
//! extra evidence: a mutable class may escape or have a method replaced through
//! another alias. This scan deliberately rejects even uncertain possible aliases;
//! external monkey patching and import-path shadowing remain outside the model.
use super::{AnalysisCancelled, callable_imports::CallableImports};
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, ModModule, Stmt};

const MODULES: &[&str] = &[
    "decimal",
    "fractions",
    "http",
    "socket",
    "signal",
    "ssl",
    "re",
    "calendar",
    "inspect",
    "annotationlib",
];
const FACTORY_CLASSES: [(&str, &str); 2] = [("decimal", "Decimal"), ("fractions", "Fraction")];

fn is_constructor(module: &str, member: &str) -> bool {
    matches!(
        (module, member),
        ("decimal", "Decimal")
            | ("fractions", "Fraction")
            | ("http", "HTTPStatus")
            | (
                "socket",
                "AddressFamily" | "SocketKind" | "MsgFlag" | "AddressInfo"
            )
            | ("signal", "Signals" | "Handlers" | "Sigmasks")
            | (
                "ssl",
                "VerifyMode"
                    | "VerifyFlags"
                    | "Options"
                    | "AlertDescription"
                    | "SSLErrorNumber"
                    | "TLSVersion"
            )
            | ("re", "RegexFlag")
            | ("calendar", "Day" | "Month")
            | ("inspect", "BufferFlags")
            | ("annotationlib", "Format")
    )
}

fn factory_class(imports: &CallableImports, call: &ExprCall) -> Option<usize> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    // Parsed call callees are load expressions; restricting this helper to a
    // call carries that invariant through both identity checks and safety scans.
    let class = imports.resolve(&attribute.value)?;
    let index = FACTORY_CLASSES
        .iter()
        .position(|&(module, member)| (class.module, class.member) == (module, member))?;
    let method = attribute.attr.as_str();
    (matches!(method, "from_float" | "from_number") || (index == 1 && method == "from_decimal"))
        .then_some(index)
}

#[derive(Default)]
pub(super) struct ConversionImports {
    imports: CallableImports,
    escaped_classes: [bool; 2],
}

impl ConversionImports {
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let imports = CallableImports::build(module, cancelled, MODULES)?;
        let mut scan = FactorySafetyScan {
            imports: &imports,
            cancelled,
            cancelled_observed: false,
            escaped_classes: [false; 2],
        };
        scan.visit_body(&module.body);
        if scan.cancelled_observed {
            return Err(AnalysisCancelled);
        }
        let escaped_classes = scan.escaped_classes;
        Ok(Self {
            imports,
            escaped_classes,
        })
    }

    pub(super) fn is_conversion(&self, call: &ExprCall) -> bool {
        self.imports
            .resolve(&call.func)
            .is_some_and(|callable| is_constructor(callable.module, callable.member))
            || factory_class(&self.imports, call).is_some_and(|class| !self.escaped_classes[class])
    }
}

struct FactorySafetyScan<'a, F> {
    imports: &'a CallableImports,
    cancelled: &'a F,
    cancelled_observed: bool,
    escaped_classes: [bool; 2],
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for FactorySafetyScan<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        visitor::walk_stmt(self, statement);
    }

    fn visit_body(&mut self, body: &'ast [Stmt]) {
        for statement in body {
            if self.cancelled_observed || (self.cancelled)() {
                self.cancelled_observed = true;
                break;
            }
            self.visit_stmt(statement);
        }
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.cancelled_observed || (self.cancelled)() {
            self.cancelled_observed = true;
            return;
        }
        if let Expr::Call(call) = expression
            && (factory_class(self.imports, call).is_some()
                || self.imports.resolve(&call.func).is_some_and(|callable| {
                    FACTORY_CLASSES.contains(&(callable.module, callable.member))
                }))
        {
            // Only the immediate callee is safe. Arguments can still expose or
            // modify the class (including nested calls and keyword arguments).
            self.visit_arguments(&call.arguments);
            return;
        }
        for (index, &(module, member)) in FACTORY_CLASSES.iter().enumerate() {
            if self.imports.may_reference(expression, module, member) {
                self.escaped_classes[index] = true;
            }
        }
        visitor::walk_expr(self, expression);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn factory_scan_does_not_repeat_elif_cancellation_work_in_nested_suites() {
        let source =
            "while outer:\n    if first:\n        pass\n    elif not second:\n        pass\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let imports = CallableImports::build(parsed.syntax(), &|| false, MODULES).unwrap();
        let calls = Cell::new(0);
        let cancelled = || {
            calls.set(calls.get() + 1);
            calls.get() > 8
        };
        let mut scan = FactorySafetyScan {
            imports: &imports,
            cancelled: &cancelled,
            cancelled_observed: false,
            escaped_classes: [false; 2],
        };

        scan.visit_body(&parsed.syntax().body);

        // Four suite statements and four expressions (including unary not).
        assert_eq!(calls.get(), 8);
        assert!(!scan.cancelled_observed);
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn run_build(
        module: &ModModule,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ConversionImports, AnalysisCancelled> {
        ConversionImports::build(module, &cancelled)
    }

    #[rstest::rstest]
    #[case("from decimal import Decimal\nDecimal(value)", true)]
    #[case("from fractions import Fraction\nFraction(value)", true)]
    #[case("from decimal import Decimal\nDecimal.from_float(value)", true)]
    #[case("from fractions import Fraction\nFraction.from_decimal(value)", true)]
    #[case("import decimal\ndecimal.Decimal.from_number(value)", true)]
    #[case("import decimal\ndecimal.Context.from_float(value)", false)]
    #[case(
        "from decimal import Decimal\nalias = Decimal\nDecimal.from_float(value)",
        false
    )]
    #[case(
        "import decimal\nexec('pass')\ndecimal.Decimal.from_float(value)",
        false
    )]
    #[case("custom(value)", false)]
    fn standard_library_conversions_require_constructor_or_stable_factory_identity(
        #[case] source: &str,
        #[case] expected: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let imports = run_build(parsed.syntax(), &|| false).unwrap();
        let call = parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();

        let actual = imports.is_conversion(call);

        assert_eq!(actual, expected);
    }

    #[rstest::rstest]
    fn conversion_identity_scan_stops_across_import_and_factory_phases(
        #[values(
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
            46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64
        )]
        checkpoint: usize,
    ) {
        let source = "from decimal import Decimal\nresult = Decimal.from_float(value)\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let total = std::cell::Cell::new(0);
        let baseline_built = run_build(parsed.syntax(), &|| {
            total.set(total.get() + 1);
            false
        })
        .is_ok();
        let polls = std::cell::Cell::new(0);
        let result = run_build(parsed.syntax(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        let cancelled_failed = result.is_err();

        assert!(baseline_built);
        assert_eq!(cancelled_failed, checkpoint < total.get());
    }
}
