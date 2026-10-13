//! Explicit conversion identities and evidence for factory-method stability.
//!
//! Constructors and factory methods share one mutation. Factory methods need
//! extra evidence: a mutable class may escape or have a method replaced through
//! another alias. This scan deliberately rejects even uncertain possible aliases;
//! external monkey patching and import-path shadowing remain outside the model.
use super::{AnalysisCancelled, callable_imports::CallableImports};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprContext, ModModule, Stmt};

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

fn factory_class(imports: &CallableImports, expression: &Expr) -> Option<usize> {
    let Expr::Attribute(attribute) = expression else {
        return None;
    };
    if attribute.ctx != ExprContext::Load {
        return None;
    }
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

    pub(super) fn is_conversion(&self, expression: &Expr) -> bool {
        self.imports
            .resolve(expression)
            .is_some_and(|callable| is_constructor(callable.module, callable.member))
            || factory_class(&self.imports, expression)
                .is_some_and(|class| !self.escaped_classes[class])
    }
}

struct FactorySafetyScan<'a, F> {
    imports: &'a CallableImports,
    cancelled: &'a F,
    cancelled_observed: bool,
    escaped_classes: [bool; 2],
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for FactorySafetyScan<'_, F> {
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
            && (factory_class(self.imports, &call.func).is_some()
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
