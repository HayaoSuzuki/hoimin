//! Static, conservative identity and value index for same-module enums.
use std::collections::HashMap;

use ruff_python_ast::{
    Expr, ExprContext, ModModule, Number, Stmt, StmtClassDef, UnaryOp,
    visitor::{self, Visitor},
};
use ruff_text_size::{Ranged, TextRange};

use super::{AnalysisCancelled, BindingEffect, NameResolutionIndex, NameScopeKind};

#[derive(Default)]
pub(super) struct EnumIndex {
    definitions: HashMap<String, Definition>,
    resolution: NameResolutionIndex,
    pub(super) diagnostics: Vec<(TextRange, String)>,
}
struct Definition {
    range: TextRange,
    members: HashMap<String, usize>,
    canonical: Vec<String>,
}
#[derive(Clone, PartialEq, Eq, Hash)]
enum Value {
    Integer(i128),
    String(String),
}
struct Member {
    name: String,
    value: Value,
    spelling: TextRange,
}

struct Import {
    member: Option<String>,
    end: ruff_text_size::TextSize,
}

impl EnumIndex {
    pub(super) fn build(
        module: &ModModule,
        source: &str,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let (imports, names) = module_bindings(module, cancelled)?;
        let resolution = NameResolutionIndex::from_module_with_names(module, names.clone());
        let mut index = Self {
            resolution,
            ..Self::default()
        };
        let mut hazards = Hazards {
            cancelled,
            unsafe_namespace: false,
            cancelled_observed: false,
            names: &names,
            modules: imports
                .iter()
                .filter(|(_, binding)| binding.member.is_none())
                .map(|(name, _)| name.clone())
                .collect(),
        };
        hazards.visit_body(&module.body);
        if hazards.cancelled_observed || cancelled() {
            return Err(AnalysisCancelled);
        }
        for statement in &module.body {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let Stmt::ClassDef(class) = statement else {
                continue;
            };
            let Some(args) = &class.arguments else {
                continue;
            };
            let Some(base) = args.args.first() else {
                continue;
            };
            let Some(kind) = imported_member(base, &imports) else {
                continue;
            };
            if !matches!(kind, "Enum" | "IntEnum" | "StrEnum" | "Flag" | "IntFlag") {
                continue;
            }
            let result = if hazards.unsafe_namespace {
                Err("dynamic namespace operation or attribute write")
            } else if !index.unique_module_binding(class.name.as_str())
                || !index.trusted_import(base, &imports)
            {
                Err("ambiguous enum or import binding")
            } else {
                index.members(class, kind, &imports, cancelled)
            };
            match result {
                Ok(members) => {
                    let mut values = HashMap::new();
                    let mut aliases = HashMap::new();
                    let mut canonical = Vec::new();
                    for Member {
                        name,
                        value,
                        spelling,
                    } in members
                    {
                        if cancelled() {
                            return Err(AnalysisCancelled);
                        }
                        let id = *values.entry(value).or_insert_with(|| {
                            canonical.push(
                                super::source_text(source, spelling)
                                    .expect("parser identifier span")
                                    .to_owned(),
                            );
                            canonical.len() - 1
                        });
                        aliases.insert(name, id);
                    }
                    index.definitions.insert(
                        class.name.to_string(),
                        Definition {
                            range: class.range(),
                            members: aliases,
                            canonical,
                        },
                    );
                }
                Err(reason) => index
                    .diagnostics
                    .push((class.range(), format!("{}: {reason}", class.name))),
            }
        }
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        Ok(index)
    }

    fn unique_module_binding(&self, name: &str) -> bool {
        let scope = &self.resolution.scopes[0];
        !scope.wildcard
            && scope
                .ordered
                .get(name)
                .is_some_and(|h| matches!(h.events.as_slice(), [(_, BindingEffect::Bind)]))
    }

    // Intentionally reject class namespace loads and any shadow in an enclosing
    // lexical scope. Method bodies skip class namespaces as Python does.
    fn resolves_module(&self, expression: &Expr, name: &str) -> bool {
        if !self.unique_module_binding(name) || name.starts_with("__") {
            return false;
        }
        let Some(site) = self
            .resolution
            .occurrences
            .get(&usize::from(expression.start()))
        else {
            return false;
        };
        if site.temporarily_shadowed {
            return false;
        }
        let mut scope_id = site.scope;
        let mut skip_classes = false;
        loop {
            let scope = &self.resolution.scopes[scope_id.0];
            if scope.kind == NameScopeKind::Module {
                return true;
            }
            if scope.kind == NameScopeKind::Class && !skip_classes {
                return false;
            }
            if scope.kind != NameScopeKind::Class {
                if scope.wildcard
                    || scope.locals.contains(name)
                    || scope.nonlocals.contains(name)
                    || scope.possible_bindings.contains(name)
                {
                    return false;
                }
                skip_classes = true;
            }
            let Some(parent) = scope.parent else {
                return false;
            };
            scope_id = parent;
        }
    }
    fn trusted_import(&self, expression: &Expr, imports: &HashMap<String, Import>) -> bool {
        let root = match expression {
            Expr::Name(_) => expression,
            Expr::Attribute(a) => a.value.as_ref(),
            _ => return false,
        };
        let Expr::Name(name) = root else {
            return false;
        };
        imports
            .get(name.id.as_str())
            .is_some_and(|i| root.start() >= i.end)
            && self.resolves_module(root, name.id.as_str())
    }

    fn members(
        &self,
        class: &StmtClassDef,
        kind: &str,
        imports: &HashMap<String, Import>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<Member>, &'static str> {
        let args = class.arguments.as_ref().expect("checked base");
        if matches!(kind, "Flag" | "IntFlag")
            || args.args.len() != 1
            || !args.keywords.is_empty()
            || !class.decorator_list.is_empty()
            || class.type_params.is_some()
        {
            return Err("unsupported base, decorator, metaclass or type parameters");
        }
        let mut members = Vec::new();
        let mut mode = None;
        let mut member_names = std::collections::HashSet::new();
        for statement in &class.body {
            if cancelled() {
                return Err("analysis cancelled");
            }
            let (name, spelling, value) = match statement {
                Stmt::Assign(s) if s.targets.len() == 1 => {
                    let Expr::Name(name) = &s.targets[0] else {
                        return Err("non-name member assignment");
                    };
                    (name.id.as_str(), name.range(), s.value.as_ref())
                }
                Stmt::FunctionDef(f) if !f.name.starts_with('_') && f.decorator_list.is_empty() => {
                    continue;
                }
                Stmt::Pass(_) => continue,
                Stmt::Expr(e) if matches!(e.value.as_ref(), Expr::StringLiteral(_)) => continue,
                _ => return Err("unsupported class statement or method decorator"),
            };
            if name.starts_with('_') {
                return Err("reserved or private class attribute");
            }
            if !member_names.insert(name) {
                return Err("repeated member name");
            }
            let (value, current_mode) = if let Some(value) = literal_value(value) {
                let mode = match &value {
                    Value::Integer(_) => 0,
                    Value::String(_) => 1,
                };
                if (kind == "IntEnum" && mode != 0) || (kind == "StrEnum" && mode != 1) {
                    return Err("member value incompatible with base");
                }
                (value, mode)
            } else if let Expr::Call(call) = value {
                // In a class body the normal resolver deliberately rejects class
                // namespaces. Resolve auto's module binding separately and reject
                // any class-local binding of its root name, even if later.
                if imported_member(&call.func, imports) != Some("auto")
                    || !call.arguments.args.is_empty()
                    || !call.arguments.keywords.is_empty()
                    || !self.auto_is_trusted(&call.func, class, imports)
                {
                    return Err("unsupported or shadowed auto call");
                }
                let value = if kind == "StrEnum" {
                    if !name.is_ascii() {
                        return Err("non-ASCII StrEnum auto name");
                    }
                    Value::String(name.to_ascii_lowercase())
                } else {
                    Value::Integer(
                        i128::try_from(members.len()).expect("member count fits i128") + 1,
                    )
                };
                (value, 2)
            } else {
                return Err("unsupported member value");
            };
            if mode.is_some_and(|m| m != current_mode) {
                return Err("mixed member value modes");
            }
            mode = Some(current_mode);
            members.push(Member {
                name: name.to_owned(),
                value,
                spelling,
            });
        }
        Ok(members)
    }
    fn auto_is_trusted(
        &self,
        expression: &Expr,
        class: &StmtClassDef,
        imports: &HashMap<String, Import>,
    ) -> bool {
        let root = match expression {
            Expr::Name(_) => expression,
            Expr::Attribute(a) => a.value.as_ref(),
            _ => return false,
        };
        let Expr::Name(name) = root else {
            return false;
        };
        if !self.unique_module_binding(name.id.as_str())
            || imports
                .get(name.id.as_str())
                .is_none_or(|i| i.end > class.start())
        {
            return false;
        }
        let Some(site) = self.resolution.occurrences.get(&usize::from(root.start())) else {
            return false;
        };
        let scope = &self.resolution.scopes[site.scope.0];
        scope.kind == NameScopeKind::Class
            && !scope.possible_bindings.contains(name.id.as_str())
            && !scope.wildcard
    }
    pub(super) fn destinations(
        &self,
        expression: &Expr,
        limit: usize,
        cancelled: &impl Fn() -> bool,
    ) -> Option<(TextRange, Vec<String>)> {
        let Expr::Attribute(attribute) = expression else {
            return None;
        };
        if attribute.ctx != ExprContext::Load {
            return None;
        }
        let Expr::Name(name) = attribute.value.as_ref() else {
            return None;
        };
        let definition = self.definitions.get(name.id.as_str())?;
        if expression.start() < definition.range.end()
            || !self.resolves_module(&attribute.value, name.id.as_str())
        {
            return None;
        }
        let source = *definition.members.get(attribute.attr.as_str())?;
        let mut result = Vec::new();
        for (id, name) in definition.canonical.iter().enumerate() {
            if cancelled() {
                return None;
            }
            if id != source {
                result.push(name.clone());
                if result.len() >= limit.saturating_add(1) {
                    break;
                }
            }
        }
        Some((attribute.attr.range(), result))
    }
}
fn imported_member<'a>(expression: &Expr, imports: &'a HashMap<String, Import>) -> Option<&'a str> {
    match expression {
        Expr::Name(name) => imports.get(name.id.as_str())?.member.as_deref(),
        Expr::Attribute(a) => {
            let Expr::Name(name) = a.value.as_ref() else {
                return None;
            };
            if imports.get(name.id.as_str())?.member.is_some() {
                return None;
            }
            // Attribute's lifetime differs; return known static names.
            match a.attr.as_str() {
                "Enum" => Some("Enum"),
                "IntEnum" => Some("IntEnum"),
                "StrEnum" => Some("StrEnum"),
                "Flag" => Some("Flag"),
                "IntFlag" => Some("IntFlag"),
                "auto" => Some("auto"),
                _ => None,
            }
        }
        _ => None,
    }
}
fn literal_value(expression: &Expr) -> Option<Value> {
    match expression {
        Expr::NumberLiteral(n) => match &n.value {
            Number::Int(i) => Some(Value::Integer(i128::from(i.as_u64()?))),
            _ => None,
        },
        Expr::StringLiteral(s) if !s.value.to_str().contains('\u{fffd}') => {
            Some(Value::String(s.value.to_str().to_owned()))
        }
        Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub | UnaryOp::UAdd) => {
            let Expr::NumberLiteral(n) = u.operand.as_ref() else {
                return None;
            };
            let Number::Int(i) = &n.value else {
                return None;
            };
            let value = i128::from(i.as_u64()?);
            Some(Value::Integer(if u.op == UnaryOp::USub {
                -value
            } else {
                value
            }))
        }
        _ => None,
    }
}
struct Hazards<'a, F> {
    cancelled: &'a F,
    unsafe_namespace: bool,
    cancelled_observed: bool,
    names: &'a std::collections::HashSet<String>,
    modules: std::collections::HashSet<String>,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for Hazards<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if (self.cancelled)() {
            self.cancelled_observed = true;
            return;
        }
        match statement {
            Stmt::Global(g) if g.names.iter().any(|n| self.names.contains(n.as_str())) => {
                self.unsafe_namespace = true;
            }
            Stmt::AnnAssign(a)
                if a.value.as_deref().is_some_and(
                    |value| matches!(value, Expr::Name(n) if self.names.contains(n.id.as_str())),
                ) =>
            {
                self.unsafe_namespace = true;
            }
            Stmt::Assign(a) if matches!(a.value.as_ref(), Expr::Name(n) if self.names.contains(n.id.as_str())) =>
            {
                self.unsafe_namespace = true;
            }
            _ => {}
        }
        visitor::walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if (self.cancelled)() {
            self.cancelled_observed = true;
            return;
        }
        match expression {
            Expr::Name(n) if n.ctx == ExprContext::Load && self.modules.contains(n.id.as_str()) => {
                self.unsafe_namespace = true;
            }
            Expr::Named(n) if matches!(n.value.as_ref(), Expr::Name(name) if self.names.contains(name.id.as_str())) =>
            {
                self.unsafe_namespace = true;
            }
            Expr::Name(n)
                if matches!(
                    n.id.as_str(),
                    "exec"
                        | "eval"
                        | "globals"
                        | "locals"
                        | "vars"
                        | "setattr"
                        | "delattr"
                        | "__import__"
                ) =>
            {
                self.unsafe_namespace = true;
            }
            Expr::Attribute(a) if a.ctx != ExprContext::Load || a.attr.as_str() == "__dict__" => {
                let mut root = a.value.as_ref();
                while let Expr::Attribute(parent) = root {
                    root = parent.value.as_ref();
                }
                if let Expr::Name(n) = root {
                    self.unsafe_namespace |= self.names.contains(n.id.as_str());
                }
            }
            _ => {}
        }
        // A module used directly as an attribute receiver has not escaped.
        // All other loads (including tuple, annotated and walrus aliases) are uncertain.
        if let Expr::Attribute(attribute) = expression {
            if !matches!(attribute.value.as_ref(), Expr::Name(_)) {
                self.visit_expr(&attribute.value);
            }
        } else {
            visitor::walk_expr(self, expression);
        }
    }
}

fn module_bindings(
    module: &ModModule,
    cancelled: &impl Fn() -> bool,
) -> Result<(HashMap<String, Import>, std::collections::HashSet<String>), AnalysisCancelled> {
    let mut imports = HashMap::new();
    let mut names = std::collections::HashSet::new();
    for statement in &module.body {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        match statement {
            Stmt::Import(s) => {
                for alias in &s.names {
                    if alias.name.as_str() == "enum" {
                        imports.insert(
                            alias.asname.as_ref().unwrap_or(&alias.name).to_string(),
                            Import {
                                member: None,
                                end: statement.end(),
                            },
                        );
                    }
                }
            }
            Stmt::ImportFrom(s)
                if s.level == 0 && s.module.as_ref().is_some_and(|m| m.as_str() == "enum") =>
            {
                for alias in &s.names {
                    imports.insert(
                        alias.asname.as_ref().unwrap_or(&alias.name).to_string(),
                        Import {
                            member: Some(alias.name.to_string()),
                            end: statement.end(),
                        },
                    );
                }
            }
            Stmt::ClassDef(s) => {
                names.insert(s.name.to_string());
            }
            _ => {}
        }
    }
    names.extend(imports.keys().cloned());
    Ok((imports, names))
}
