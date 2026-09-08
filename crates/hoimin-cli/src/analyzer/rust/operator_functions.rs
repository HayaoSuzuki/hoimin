//! Conservative, module-wide identity checks for Python's `operator` callables.
//!
//! This index deliberately does not share the flow-sensitive builtin resolver:
//! one additional binding anywhere invalidates a trusted import everywhere.
use std::collections::{HashMap, HashSet};

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprContext, ModModule, Pattern, Stmt, TypeParam};
use ruff_text_size::{Ranged, TextRange};

use super::{AnalysisCancelled, ContainmentIndex};

// Python 3.13 canonical names, documented aliases, and mutation destinations.
// An empty alias means there is no documented dunder form.
const FUNCTIONS: &[(&str, &str, &str)] = &[
    ("eq", "__eq__", "ne"),
    ("ne", "__ne__", "eq"),
    ("lt", "__lt__", "le"),
    ("le", "__le__", "lt"),
    ("gt", "__gt__", "ge"),
    ("ge", "__ge__", "gt"),
    ("add", "__add__", "sub"),
    ("sub", "__sub__", "add"),
    ("mul", "__mul__", "truediv"),
    ("truediv", "__truediv__", "mul"),
    ("floordiv", "__floordiv__", "mod"),
    ("mod", "__mod__", "floordiv"),
    ("pow", "__pow__", "mul"),
    ("matmul", "__matmul__", "mul"),
    ("and_", "__and__", "or_"),
    ("or_", "__or__", "and_"),
    ("lshift", "__lshift__", "rshift"),
    ("rshift", "__rshift__", "lshift"),
    ("xor", "__xor__", "and_"),
    ("neg", "__neg__", "pos"),
    ("pos", "__pos__", "neg"),
    ("abs", "__abs__", "neg"),
    ("index", "__index__", "pos"),
    ("inv", "__inv__", "pos"),
    ("invert", "__invert__", "pos"),
    ("not_", "__not__", "truth"),
    ("truth", "", "not_"),
    ("is_", "", "is_not"),
    ("is_not", "", "is_"),
    ("iadd", "__iadd__", "isub"),
    ("isub", "__isub__", "iadd"),
    ("imul", "__imul__", "itruediv"),
    ("itruediv", "__itruediv__", "imul"),
    ("ifloordiv", "__ifloordiv__", "imod"),
    ("imod", "__imod__", "ifloordiv"),
    ("ipow", "__ipow__", "imul"),
    ("imatmul", "__imatmul__", "imul"),
    ("iand", "__iand__", "ior"),
    ("ior", "__ior__", "iand"),
    ("ilshift", "__ilshift__", "irshift"),
    ("irshift", "__irshift__", "ilshift"),
    ("ixor", "__ixor__", "iand"),
    ("concat", "__concat__", "iconcat"),
    ("iconcat", "__iconcat__", "concat"),
    ("countOf", "", "indexOf"),
    ("indexOf", "", "countOf"),
    ("getitem", "__getitem__", "contains"),
    (
        "contains",
        "__contains__",
        "(lambda container, item, /: item not in container)",
    ),
    (
        "setitem",
        "__setitem__",
        "(lambda container, key, value, /: None)",
    ),
    ("delitem", "__delitem__", "(lambda container, key, /: None)"),
    (
        "call",
        "__call__",
        "(lambda target, /, *args, **kwargs: None)",
    ),
];

fn replacement(name: &str) -> Option<&'static str> {
    let &(canonical, _, destination) = FUNCTIONS.iter().find(|(canonical, alias, _)| {
        name == *canonical || (!alias.is_empty() && name == *alias)
    })?;
    if name != canonical
        && let Some((_, alias, _)) = FUNCTIONS.iter().find(|(name, _, _)| *name == destination)
        && !alias.is_empty()
    {
        return Some(alias);
    }
    Some(destination)
}

fn dynamic_namespace_name(name: &str) -> bool {
    matches!(
        name,
        "exec"
            | "eval"
            | "globals"
            | "locals"
            | "vars"
            | "setattr"
            | "delattr"
            | "__import__"
            | "__builtins__"
            | "__dict__"
            | "__setattr__"
            | "__delattr__"
            | "__getattribute__"
    )
}

struct ImportedBinding {
    // None is a module import; Some is the original imported callable name.
    member: Option<String>,
    import_range: TextRange,
}

#[derive(Default)]
pub(super) struct OperatorImports {
    trusted: HashMap<String, ImportedBinding>,
    import_builtin_available: bool,
    class_ranges: ContainmentIndex,
}

impl OperatorImports {
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let mut scan = ImportScan {
            cancelled,
            cancelled_observed: false,
            depth: 0,
            trusted: HashMap::new(),
            bindings: HashMap::new(),
            module_names: HashSet::new(),
            attribute_writes: HashSet::new(),
            escaped_names: HashSet::new(),
            namespace_uncertain: false,
            class_ranges: Vec::new(),
        };
        scan.visit_body(&module.body);
        if scan.cancelled_observed {
            return Err(AnalysisCancelled);
        }
        if scan.namespace_uncertain
            || scan.module_names.iter().any(|name| {
                scan.attribute_writes.contains(name) || scan.escaped_names.contains(name)
            })
        {
            return Ok(Self::default());
        }
        scan.trusted
            .retain(|name, _| scan.bindings.get(name) == Some(&1));
        Ok(Self {
            trusted: scan.trusted,
            import_builtin_available: !scan.bindings.contains_key("__import__"),
            class_ranges: ContainmentIndex::new(scan.class_ranges),
        })
    }

    fn trusted_binding(&self, name: &str, range: TextRange) -> Option<&ImportedBinding> {
        // Class compilation can mangle private names or provide implicit names
        // such as __class__. Raw AST spelling cannot prove identity there.
        if name.starts_with("__")
            && self
                .class_ranges
                .contains(usize::from(range.start()), usize::from(range.end()))
        {
            return None;
        }
        self.trusted.get(name)
    }

    pub(super) fn replacement(&self, expression: &Expr) -> Option<(TextRange, String)> {
        let (binding, name, member_range) = match expression {
            Expr::Attribute(attribute) if attribute.ctx == ExprContext::Load => {
                let Expr::Name(module) = attribute.value.as_ref() else {
                    return None;
                };
                let binding = self.trusted_binding(module.id.as_str(), expression.range())?;
                if binding.member.is_some() {
                    return None;
                }
                (
                    binding,
                    attribute.attr.as_str(),
                    Some(attribute.attr.range()),
                )
            }
            Expr::Name(name) if name.ctx == ExprContext::Load => {
                let binding = self.trusted_binding(name.id.as_str(), expression.range())?;
                (binding, binding.member.as_deref()?, None)
            }
            _ => return None,
        };
        if expression.range().start() < binding.import_range.end() {
            return None;
        }
        let destination = replacement(name)?;
        if destination.starts_with('(') {
            Some((expression.range(), destination.to_owned()))
        } else if let Some(range) = member_range {
            Some((range, destination.to_owned()))
        } else if self.import_builtin_available {
            Some((
                expression.range(),
                format!("__import__('operator').{destination}"),
            ))
        } else {
            None
        }
    }
}

struct ImportScan<'a, F> {
    cancelled: &'a F,
    cancelled_observed: bool,
    depth: usize,
    trusted: HashMap<String, ImportedBinding>,
    bindings: HashMap<String, usize>,
    module_names: HashSet<String>,
    attribute_writes: HashSet<String>,
    escaped_names: HashSet<String>,
    namespace_uncertain: bool,
    class_ranges: Vec<(usize, usize)>,
}

impl<F: Fn() -> bool> ImportScan<'_, F> {
    fn stop(&mut self) -> bool {
        if !self.cancelled_observed {
            self.cancelled_observed = (self.cancelled)();
        }
        self.cancelled_observed
    }

    fn bind(&mut self, name: &str) {
        *self.bindings.entry(name.to_owned()).or_default() += 1;
    }

    fn import(&mut self, local: &str, member: Option<&str>, range: TextRange) {
        if member.is_none() {
            self.module_names.insert(local.to_owned());
        }
        if self.depth == 0 {
            self.trusted.insert(
                local.to_owned(),
                ImportedBinding {
                    member: member.map(str::to_owned),
                    import_range: range,
                },
            );
        }
    }
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for ImportScan<'_, F> {
    fn visit_body(&mut self, body: &'ast [Stmt]) {
        for statement in body {
            self.visit_stmt(statement);
            if self.cancelled_observed {
                break;
            }
        }
    }

    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if self.stop() {
            return;
        }
        match statement {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let local = alias.asname.as_ref().map_or_else(
                        || {
                            alias
                                .name
                                .as_str()
                                .split('.')
                                .next()
                                .unwrap_or(alias.name.as_str())
                        },
                        ruff_python_ast::Identifier::as_str,
                    );
                    self.bind(local);
                    if alias.name.as_str() == "operator" {
                        self.import(local, None, statement.range());
                    }
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    if import.level == 0
                        && import.module.as_ref().is_some_and(|module| {
                            (matches!(module.as_str(), "builtins" | "operator")
                                && dynamic_namespace_name(alias.name.as_str()))
                                || (module.as_str() == "sys" && alias.name.as_str() == "modules")
                        })
                    {
                        self.namespace_uncertain = true;
                    }
                    if alias.name.as_str() == "*" {
                        self.namespace_uncertain = true;
                    }
                    let local = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
                    self.bind(local);
                    if import.level == 0
                        && import
                            .module
                            .as_ref()
                            .is_some_and(|m| m.as_str() == "operator")
                    {
                        self.import(local, Some(alias.name.as_str()), statement.range());
                    }
                }
            }
            Stmt::FunctionDef(definition) => self.bind(definition.name.as_str()),
            Stmt::ClassDef(definition) => {
                self.bind(definition.name.as_str());
                self.class_ranges.push((
                    usize::from(definition.range().start()),
                    usize::from(definition.range().end()),
                ));
            }
            Stmt::Global(global) => {
                for name in &global.names {
                    self.bind(name.as_str());
                }
            }
            Stmt::Nonlocal(nonlocal) => {
                for name in &nonlocal.names {
                    self.bind(name.as_str());
                }
            }
            _ => {}
        }
        self.depth += 1;
        visitor::walk_stmt(self, statement);
        self.depth -= 1;
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.stop() {
            return;
        }
        match expression {
            Expr::Name(name) => {
                if name.ctx == ExprContext::Load {
                    self.escaped_names.insert(name.id.to_string());
                    // Even taking a reference can hide later indirect namespace writes.
                    if dynamic_namespace_name(name.id.as_str()) {
                        self.namespace_uncertain = true;
                    }
                } else {
                    self.bind(name.id.as_str());
                }
            }
            Expr::Attribute(attribute) => {
                if dynamic_namespace_name(attribute.attr.as_str())
                    || attribute.attr.as_str() == "modules"
                {
                    self.namespace_uncertain = true;
                }
                if let Expr::Name(base) = attribute.value.as_ref() {
                    if attribute.ctx != ExprContext::Load {
                        self.attribute_writes.insert(base.id.to_string());
                    }
                    // A direct member lookup doesn't let the module object escape.
                    return;
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }

    fn visit_parameter(&mut self, parameter: &'ast ruff_python_ast::Parameter) {
        if self.stop() {
            return;
        }
        self.bind(parameter.name.as_str());
        visitor::walk_parameter(self, parameter);
    }

    fn visit_type_param(&mut self, parameter: &'ast TypeParam) {
        if self.stop() {
            return;
        }
        let name = match parameter {
            TypeParam::TypeVar(parameter) => &parameter.name,
            TypeParam::TypeVarTuple(parameter) => &parameter.name,
            TypeParam::ParamSpec(parameter) => &parameter.name,
        };
        self.bind(name.as_str());
        visitor::walk_type_param(self, parameter);
    }

    fn visit_except_handler(&mut self, handler: &'ast ruff_python_ast::ExceptHandler) {
        if self.stop() {
            return;
        }
        let ruff_python_ast::ExceptHandler::ExceptHandler(exception) = handler;
        if let Some(name) = &exception.name {
            self.bind(name.as_str());
        }
        visitor::walk_except_handler(self, handler);
    }

    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        if self.stop() {
            return;
        }
        let name = match pattern {
            Pattern::MatchMapping(pattern) => pattern.rest.as_ref(),
            Pattern::MatchStar(pattern) => pattern.name.as_ref(),
            Pattern::MatchAs(pattern) => pattern.name.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.bind(name.as_str());
        }
        visitor::walk_pattern(self, pattern);
    }
}
