//! Conservative direct inheritance, checked once per module and method.
use std::collections::{HashMap, HashSet};

use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprContext, ModModule, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use super::{AnalysisCancelled, NameResolution, NameResolutionIndex, source_text};

#[cfg(test)]
thread_local! {
    static WORK: std::cell::Cell<[usize; 3]> = const { std::cell::Cell::new([0; 3]) };
}
#[cfg(test)]
pub(super) fn reset_work() {
    WORK.set([0; 3]);
}
#[cfg(test)]
pub(super) fn work() -> [usize; 3] {
    WORK.get()
}
#[cfg(test)]
fn record(index: usize) {
    WORK.update(|mut v| {
        v[index] += 1;
        v
    });
}

#[derive(Default)]
pub(super) struct Index {
    methods: HashSet<TextRange>,
    resolution: NameResolutionIndex,
}
struct Class<'a> {
    definition: &'a StmtClassDef,
    methods: HashMap<&'a str, &'a StmtFunctionDef>,
}

impl Index {
    #[expect(
        clippy::too_many_lines,
        reason = "one shared inventory, identity proof and eligibility pass keep cancellation boundaries visible"
    )]
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        #[cfg(test)]
        record(0);
        let mut classes = HashMap::new();
        let mut names = HashSet::new();
        let mut bases = HashSet::new();
        let mut method_names = HashSet::new();
        for statement in &module.body {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            if let Stmt::ClassDef(class) = statement {
                names.insert(class.name.to_string());
                if let Some(methods) = class_methods(class, cancelled) {
                    method_names.extend(methods.keys().map(|name| (*name).to_owned()));
                    classes.insert(
                        class.name.as_str(),
                        Class {
                            definition: class,
                            methods,
                        },
                    );
                }
            }
        }
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        if !classes.values().any(|child| {
            let Some(arguments) = &child.definition.arguments else {
                return false;
            };
            let [Expr::Name(base)] = arguments.args.as_ref() else {
                return false;
            };
            classes.get(base.id.as_str()).is_some_and(|parent| {
                parent.definition.start() < child.definition.start()
                    && parent
                        .definition
                        .arguments
                        .as_ref()
                        .is_none_or(|args| args.args.is_empty())
            })
        }) {
            return Ok(Self::default());
        }
        let mut tracked = names.clone();
        tracked.insert("super".to_owned());
        let resolution = NameResolutionIndex::from_module_with_names(module, tracked);
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        // Passing a class into any unsupported header can invoke arbitrary metaclass code.
        for child in classes.values() {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let Some(arguments) = &child.definition.arguments else {
                continue;
            };
            let [base @ Expr::Name(name)] = arguments.args.as_ref() else {
                continue;
            };
            if let Some(parent) = classes.get(name.id.as_str())
                && parent.definition.start() < child.definition.start()
                && parent
                    .definition
                    .arguments
                    .as_ref()
                    .is_none_or(|args| args.args.is_empty())
                && resolution.resolves_unique_module(base, name.id.as_str())
            {
                bases.insert(base.range());
            }
        }
        let mut hazards = Hazards {
            names: &names,
            bases: &bases,
            method_names: &method_names,
            invalid: HashSet::new(),
            dynamic: false,
            cancelled,
            stopped: false,
        };
        hazards.visit_body(&module.body);
        if hazards.stopped || cancelled() {
            return Err(AnalysisCancelled);
        }
        if hazards.dynamic {
            return Ok(Self::default());
        }
        let mut eligible = HashSet::new();
        for (name, child) in &classes {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            if hazards.invalid.contains(*name) || !resolution.unique_module_binding(name) {
                continue;
            }
            let Some(arguments) = &child.definition.arguments else {
                continue;
            };
            let [base] = arguments.args.as_ref() else {
                continue;
            };
            let Expr::Name(base_name) = base else {
                continue;
            };
            let parent_name = base_name.id.as_str();
            let Some(parent) = classes.get(parent_name) else {
                continue;
            };
            if parent.definition.start() >= child.definition.start()
                || parent
                    .definition
                    .arguments
                    .as_ref()
                    .is_some_and(|args| !args.args.is_empty() || !args.keywords.is_empty())
                || hazards.invalid.contains(parent_name)
                || !resolution.resolves_unique_module(base, parent_name)
            {
                continue;
            }
            for (method_name, method) in &child.methods {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                if let Some(parent_method) = parent.methods.get(method_name)
                    && matching_signature(parent_method, method)
                {
                    eligible.insert(method.range());
                }
            }
        }
        Ok(Self {
            methods: eligible,
            resolution,
        })
    }
    pub(super) fn contains(&self, method: &StmtFunctionDef) -> bool {
        self.methods.contains(&method.range())
    }

    pub(super) fn move_span(
        &self,
        method: &StmtFunctionDef,
        cancelled: &impl Fn() -> bool,
    ) -> Option<(TextRange, usize, usize)> {
        if !self.contains(method) {
            return None;
        }
        let body = &method.body;
        let first = usize::from(body.first().is_some_and(is_docstring));
        let statements = &body[first..];
        if statements.len() < 2 {
            return None;
        }
        let mut calls = ExpressionCheck {
            safe: true,
            super_calls: 0,
            cancelled,
        };
        let mut delegation = None;
        for (index, statement) in statements.iter().enumerate() {
            if cancelled() {
                return None;
            }
            let Stmt::Expr(expression) = statement else {
                return None;
            };
            calls.visit_expr(&expression.value);
            if !calls.safe {
                return None;
            }
            if let Expr::Call(call) = expression.value.as_ref()
                && let Expr::Attribute(attribute) = call.func.as_ref()
                && attribute.attr.as_str() == method.name.as_str()
                && let Expr::Call(super_call) = attribute.value.as_ref()
                && let Expr::Name(name) = super_call.func.as_ref()
                && name.id == "super"
                && super_call.arguments.args.is_empty()
                && super_call.arguments.keywords.is_empty()
                && self
                    .resolution
                    .resolution(usize::from(name.start()), "super")
                    == NameResolution::DefinitelyBuiltin
            {
                delegation = Some(index);
            }
        }
        if calls.super_calls != 1 {
            return None;
        }
        let index = delegation?;
        // A literal in the first executable slot must never acquire docstring semantics.
        if first == 0 && index == 0 && is_docstring(&statements[1]) {
            return None;
        }
        Some((
            TextRange::new(statements.first()?.start(), statements.last()?.end()),
            first,
            index,
        ))
    }
}

fn class_methods<'a>(
    class: &'a StmtClassDef,
    cancelled: &impl Fn() -> bool,
) -> Option<HashMap<&'a str, &'a StmtFunctionDef>> {
    if !class.decorator_list.is_empty()
        || class.type_params.is_some()
        || class
            .arguments
            .as_ref()
            .is_some_and(|args| !args.keywords.is_empty())
    {
        return None;
    }
    let method_names: HashSet<_> = class
        .body
        .iter()
        .filter_map(|statement| {
            if let Stmt::FunctionDef(method) = statement {
                Some(method.name.as_str())
            } else {
                None
            }
        })
        .collect();
    let mut bindings = HashSet::new();
    let mut methods = HashMap::new();
    for statement in &class.body {
        if cancelled() {
            return None;
        }
        match statement {
            Stmt::FunctionDef(method) => {
                let name = method.name.as_str();
                // Decorators can manufacture descriptors with owner-mutating __set_name__ hooks.
                if !method.decorator_list.is_empty() {
                    return None;
                }
                let mut headers = HeaderAliases {
                    methods: &method_names,
                    safe: true,
                    cancelled,
                };
                headers.visit_parameters(&method.parameters);
                if let Some(parameters) = &method.type_params {
                    headers.visit_type_params(parameters);
                }
                if let Some(annotation) = &method.returns {
                    headers.visit_expr(annotation);
                }
                if !headers.safe {
                    return None;
                }
                if !bindings.insert(name)
                    || matches!(
                        name,
                        "__getattribute__"
                            | "__getattr__"
                            | "__init_subclass__"
                            | "__setattr__"
                            | "__delattr__"
                    )
                {
                    return None;
                }
                #[cfg(test)]
                record(1);
                if ordinary_method(method, cancelled) {
                    methods.insert(name, method);
                }
            }
            Stmt::Pass(_) => {}
            Stmt::Expr(_) if is_docstring(statement) => {}
            Stmt::Assign(assign) if inert(&assign.value) => {
                for target in &assign.targets {
                    let Expr::Name(name) = target else {
                        return None;
                    };
                    if !bindings.insert(name.id.as_str())
                        || matches!(
                            name.id.as_str(),
                            "__getattribute__"
                                | "__getattr__"
                                | "__init_subclass__"
                                | "__setattr__"
                                | "__delattr__"
                        )
                    {
                        return None;
                    }
                }
            }
            _ => return None,
        }
    }
    Some(methods)
}
fn inert(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
    )
}
fn ordinary_method(method: &StmtFunctionDef, cancelled: &impl Fn() -> bool) -> bool {
    if method.is_async
        || !method.decorator_list.is_empty()
        || method.type_params.is_some()
        || method.name.starts_with("__")
        || method.returns.is_some()
        || method.parameters.vararg.is_some()
        || method.parameters.kwarg.is_some()
        || method
            .parameters
            .posonlyargs
            .iter()
            .chain(&method.parameters.args)
            .next()
            .is_none_or(|p| p.parameter.name.as_str() != "self")
        || method
            .parameters
            .iter_non_variadic_params()
            .any(|p| p.default.is_some() || p.parameter.annotation.is_some())
    {
        return false;
    }
    let mut check = MethodCheck {
        safe: true,
        cancelled,
    };
    check.visit_body(&method.body);
    check.safe
}
fn matching_signature(left: &StmtFunctionDef, right: &StmtFunctionDef) -> bool {
    let a = &left.parameters;
    let b = &right.parameters;
    a.posonlyargs.len() == b.posonlyargs.len()
        && a.args.len() == b.args.len()
        && a.kwonlyargs.len() == b.kwonlyargs.len()
        && a.iter_non_variadic_params()
            .zip(b.iter_non_variadic_params())
            .all(|(a, b)| a.parameter.name.as_str() == b.parameter.name.as_str())
}
fn is_docstring(statement: &Stmt) -> bool {
    matches!(statement, Stmt::Expr(value) if matches!(value.value.as_ref(), Expr::StringLiteral(_)))
}
struct HeaderAliases<'a, F> {
    methods: &'a HashSet<&'a str>,
    safe: bool,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for HeaderAliases<'_, F> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if (self.cancelled)()
            || matches!(expression, Expr::Name(name) if self.methods.contains(name.id.as_str()))
        {
            self.safe = false;
        }
        if self.safe {
            visitor::walk_expr(self, expression);
        }
    }
}
struct MethodCheck<'a, F> {
    safe: bool,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for MethodCheck<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if (self.cancelled)() || matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            self.safe = false;
        }
        if self.safe {
            visitor::walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if (self.cancelled)()
            || matches!(
                expression,
                Expr::Lambda(_) | Expr::Await(_) | Expr::Yield(_) | Expr::YieldFrom(_)
            )
        {
            self.safe = false;
        }
        if self.safe {
            visitor::walk_expr(self, expression);
        }
    }
}
struct ExpressionCheck<'a, F> {
    safe: bool,
    super_calls: usize,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for ExpressionCheck<'_, F> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if (self.cancelled)()
            || matches!(
                expression,
                Expr::Named(_)
                    | Expr::Await(_)
                    | Expr::Yield(_)
                    | Expr::YieldFrom(_)
                    | Expr::Starred(_)
                    | Expr::Lambda(_)
                    | Expr::Generator(_)
                    | Expr::ListComp(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            )
        {
            self.safe = false;
        }
        if matches!(expression, Expr::Name(name) if name.id == "super") {
            self.super_calls += 1;
        }
        if let Expr::Call(call) = expression
            && call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            self.safe = false;
        }
        if self.safe {
            visitor::walk_expr(self, expression);
        }
    }
}
fn reflective(name: &str) -> bool {
    matches!(
        name,
        "exec"
            | "eval"
            | "globals"
            | "locals"
            | "vars"
            | "getattr"
            | "setattr"
            | "delattr"
            | "__import__"
            | "__builtins__"
            | "__dict__"
            | "__bases__"
            | "__class__"
            | "__getattribute__"
            | "__getattr__"
            | "__setattr__"
            | "__delattr__"
    )
}
struct Hazards<'a, F> {
    names: &'a HashSet<String>,
    bases: &'a HashSet<TextRange>,
    method_names: &'a HashSet<String>,
    invalid: HashSet<String>,
    dynamic: bool,
    cancelled: &'a F,
    stopped: bool,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for Hazards<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        self.stopped |= (self.cancelled)();
        if self.stopped {
            return;
        }
        if let Stmt::ImportFrom(import) = statement
            && import
                .names
                .iter()
                .any(|name| name.name.as_str() == "*" || reflective(name.name.as_str()))
        {
            self.dynamic = true;
        }
        visitor::walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        self.stopped |= (self.cancelled)();
        if self.stopped {
            return;
        }
        if let Expr::Name(name) = expression {
            if self.names.contains(name.id.as_str())
                && name.ctx == ExprContext::Load
                && !self.bases.contains(&name.range())
            {
                self.invalid.insert(name.id.to_string());
            }
            if reflective(name.id.as_str()) {
                self.dynamic = true;
            }
        }
        if let Expr::Call(call) = expression
            && let Expr::Attribute(attribute) = call.func.as_ref()
        {
            if reflective(attribute.attr.as_str()) {
                self.dynamic = true;
            }
            self.visit_expr(&attribute.value);
            self.visit_arguments(&call.arguments);
            return;
        }
        if let Expr::Attribute(attribute) = expression {
            if reflective(attribute.attr.as_str()) {
                self.dynamic = true;
            }
            if self.method_names.contains(attribute.attr.as_str())
                || (attribute.ctx != ExprContext::Load && attribute.attr.as_str() == "super")
            {
                self.dynamic = true;
            }
        }
        if let Expr::Call(call) = expression
            && matches!(call.func.as_ref(), Expr::Name(name) if self.names.contains(name.id.as_str()))
        {
            self.visit_arguments(&call.arguments);
        } else {
            visitor::walk_expr(self, expression);
        }
    }
}

/// Substitute fragments into unchanged separator slots, including grouping parentheses.
pub(super) fn move_replacement(
    method: &StmtFunctionDef,
    range: TextRange,
    first: usize,
    delegation: usize,
    source: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<String> {
    let statements = &method.body[first..];
    let mut order: Vec<_> = (0..statements.len()).collect();
    if delegation == 0 {
        order.rotate_left(1);
    } else {
        order.remove(delegation);
        order.insert(0, delegation);
    }
    let mut result = String::with_capacity(usize::from(range.len()));
    let mut cursor = range.start();
    for (slot, statement) in statements.iter().enumerate() {
        if cancelled() {
            return None;
        }
        result.push_str(source_text(
            source,
            TextRange::new(cursor, statement.start()),
        )?);
        result.push_str(source_text(source, statements[order[slot]].range())?);
        cursor = statement.end();
    }
    result.push_str(source_text(source, TextRange::new(cursor, range.end()))?);
    #[cfg(test)]
    record(2);
    Some(result)
}
