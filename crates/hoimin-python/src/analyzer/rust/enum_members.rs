//! Static, conservative identity and value index for same-module enums.
use crate::visitor::{self, Visitor};
use std::collections::HashMap;

use ruff_python_ast::{Expr, ExprContext, ModModule, Number, Stmt, StmtClassDef, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

use super::{AnalysisCancelled, NameResolutionIndex, NameScopeKind};

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
    is_lazy: bool,
    member: Option<String>,
    end: ruff_text_size::TextSize,
}

impl EnumIndex {
    pub(super) fn build(
        module: &ModModule,
        source: &str,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        if super::lazy_imports::configuration_present(module, cancelled)? {
            return Ok(Self::default());
        }
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
            let Some(imported) = imported_member(base, &imports) else {
                continue;
            };
            let kind = imported.member;
            if !matches!(kind, "Enum" | "IntEnum" | "StrEnum" | "Flag" | "IntFlag") {
                continue;
            }
            let result = if hazards.unsafe_namespace {
                Err("dynamic namespace operation or attribute write")
            } else if !index.unique_module_binding(class.name.as_str())
                || !index.trusted_import(&imported)
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
        self.resolution.unique_module_binding(name)
    }

    fn resolves_module(&self, expression: &Expr, name: &str) -> bool {
        self.resolution.resolves_unique_module(expression, name)
    }
    fn trusted_import(&self, imported: &ImportedMember<'_>) -> bool {
        imported.root.start() >= imported.binding.end
            && self.resolves_module(imported.root, imported.name)
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
                let imported = imported_member(&call.func, imports);
                if imported
                    .as_ref()
                    .is_none_or(|member| member.member != "auto")
                    || !call.arguments.args.is_empty()
                    || !call.arguments.keywords.is_empty()
                    || imported
                        .as_ref()
                        .is_none_or(|member| !self.auto_is_trusted(member, class))
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
    fn auto_is_trusted(&self, imported: &ImportedMember<'_>, class: &StmtClassDef) -> bool {
        if !self.unique_module_binding(imported.name) || imported.binding.end > class.start() {
            return false;
        }
        self.resolution
            .occurrences
            .get(&usize::from(imported.root.start()))
            .is_some_and(|site| {
                let scope = &self.resolution.scopes[site.scope.0];
                scope.kind == NameScopeKind::Class
                    && !scope.possible_bindings.contains(imported.name)
                    && !scope.wildcard
            })
    }
    pub(super) fn destinations(
        &self,
        expression: &Expr,
        limit: usize,
        cancelled: &impl Fn() -> bool,
    ) -> Option<(TextRange, Vec<&str>)> {
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
                result.push(name.as_str());
                if result.len() >= limit.saturating_add(1) {
                    break;
                }
            }
        }
        Some((attribute.attr.range(), result))
    }
}
// Carries the eager import and name root established by import recognition, so
// resolution checks cannot accidentally repeat only part of that validation.
struct ImportedMember<'a> {
    root: &'a Expr,
    name: &'a str,
    member: &'a str,
    binding: &'a Import,
}
fn imported_member<'a>(
    expression: &'a Expr,
    imports: &'a HashMap<String, Import>,
) -> Option<ImportedMember<'a>> {
    let root = match expression {
        Expr::Name(_) => expression,
        Expr::Attribute(attribute) => attribute.value.as_ref(),
        _ => return None,
    };
    let Expr::Name(name) = root else {
        return None;
    };
    let binding = imports
        .get(name.id.as_str())
        .filter(|binding| !binding.is_lazy)?;
    let member = match expression {
        Expr::Name(_) => binding.member.as_deref()?,
        Expr::Attribute(attribute) if binding.member.is_none() => match attribute.attr.as_str() {
            "Enum" => "Enum",
            "IntEnum" => "IntEnum",
            "StrEnum" => "StrEnum",
            "Flag" => "Flag",
            "IntFlag" => "IntFlag",
            "auto" => "auto",
            _ => return None,
        },
        _ => return None,
    };
    Some(ImportedMember {
        root,
        name: name.id.as_str(),
        member,
        binding,
    })
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
                                is_lazy: s.is_lazy,
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
                            is_lazy: s.is_lazy,
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

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_build(
        module: &ModModule,
        source: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<EnumIndex, AnalysisCancelled> {
        EnumIndex::build(module, source, &cancelled)
    }

    fn run_destinations<'a>(
        index: &'a EnumIndex,
        expression: &Expr,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, Vec<&'a str>)> {
        index.destinations(expression, limit, &cancelled)
    }

    #[rstest::rstest]
    #[case("Enum", "    pass\n    FIRST = 1\n    SECOND = 2", Some(&["SECOND"][..]))]
    #[case("IntEnum", "    FIRST = -1\n    SECOND = +2", Some(&["SECOND"][..]))]
    #[case("StrEnum", "    FIRST = 'one'\n    SECOND = 'two'", Some(&["SECOND"][..]))]
    #[case("Enum", "    FIRST = auto()\n    SECOND = auto()", Some(&["SECOND"][..]))]
    #[case("IntEnum", "    FIRST = auto()\n    SECOND = auto()", Some(&["SECOND"][..]))]
    #[case("StrEnum", "    FIRST = auto()\n    SECOND = auto()", Some(&["SECOND"][..]))]
    #[case("Enum", "    FIRST = 1\n    ALIAS = 1\n    SECOND = 2", Some(&["SECOND"][..]))]
    #[case("Enum", "    'documentation'\n    FIRST = 1\n    def method(self): pass\n    SECOND = 2", Some(&["SECOND"][..]))]
    #[case("Enum", "    FIRST = 1\n    SECOND = 'two'", None)]
    #[case("IntEnum", "    FIRST = 'one'\n    SECOND = 'two'", None)]
    #[case("StrEnum", "    FIRST = 1\n    SECOND = 2", None)]
    #[case("Flag", "    FIRST = 1\n    SECOND = 2", None)]
    #[case("IntFlag", "    FIRST = 1\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = 1\n    _private = 2", None)]
    #[case("Enum", "    FIRST = 1\n    FIRST = 2", None)]
    #[case("Enum", "    [FIRST] = [1]\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST: int = 1\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = factory()\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = auto(1)\n    SECOND = auto()", None)]
    #[case("Enum", "    FIRST = auto(value=1)\n    SECOND = auto()", None)]
    #[case("Enum", "    FIRST = auto()\n    auto = replacement", None)]
    #[case("Enum", "    FIRST = 1\n    SECOND = auto()", None)]
    #[case("StrEnum", "    FÍRST = auto()\n    SECOND = auto()", None)]
    #[case("Enum", "    FIRST = 1.5\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = -18446744073709551616\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = 18446744073709551615\n    SECOND = 2", Some(&["SECOND"][..]))]
    #[case("Enum", "    FIRST = 18446744073709551616\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = -1.5\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = -value\n    SECOND = 2", None)]
    #[case("Enum", "    FIRST = 1 + 2\n    SECOND = 3", None)]
    #[case("Enum", "    FIRST = '\\ufffd'\n    SECOND = 'two'", None)]
    fn enum_destinations_require_consistent_static_member_values(
        #[case] kind: &str,
        #[case] body: &str,
        #[case] expected: Option<&[&str]>,
    ) {
        let source =
            format!("from enum import {kind}, auto\nclass Choice({kind}):\n{body}\nChoice.FIRST\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let index = run_build(parsed.syntax(), &source, &|| false).unwrap();
        let expression = &parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value;

        let actual = index
            .destinations(expression, 100, &|| false)
            .map(|(_, names)| names);

        assert_eq!(actual.as_deref(), expected);
    }

    #[rstest::rstest]
    #[case(0, vec!["SECOND"])]
    #[case(1, vec!["SECOND", "THIRD"])]
    #[case(2, vec!["SECOND", "THIRD", "FOURTH"])]
    fn enum_destination_prefix_retains_one_extra_for_truncation(
        #[case] limit: usize,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let source = "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\n THIRD = 3\n FOURTH = 4\nChoice.FIRST\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), source, &|| false).unwrap();
        let expression = &parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value;

        let actual = run_destinations(&index, expression, limit, &|| false)
            .unwrap()
            .1;

        assert_eq!(actual, expected);
    }

    #[test]
    fn cancelled_member_collection_rejects_partial_definitions() {
        let source = "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), source, &|| false).unwrap();
        let no_cancel: &dyn Fn() -> bool = &|| false;
        let (imports, _) = module_bindings(parsed.syntax(), &no_cancel).unwrap();
        let class = parsed.syntax().body[1].as_class_def_stmt().unwrap();
        let cancelled: &dyn Fn() -> bool = &|| true;

        let actual = matches!(
            index.members(class, "Enum", &imports, &cancelled),
            Err("analysis cancelled")
        );

        assert!(actual);
    }
    #[rstest::rstest]
    #[case("import enum", "class Choice(enum.Enum):", " FIRST = enum.auto()\n SECOND = enum.auto()", Some(&["SECOND"][..]))]
    #[case("import enum", "class Choice(enum.IntEnum):", " FIRST = 1\n SECOND = 2", Some(&["SECOND"][..]))]
    #[case("import enum", "class Choice(enum.StrEnum):", " FIRST = 'one'\n SECOND = 'two'", Some(&["SECOND"][..]))]
    #[case(
        "import enum",
        "class Choice(enum.Flag):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "import enum",
        "class Choice(enum.IntFlag):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "import enum",
        "class Choice(enum.Unknown):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Unknown):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(factory()):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(holder.Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(holder.inner.Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum\n@decorator",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Enum, object):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Enum, metaclass=meta):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice[T](Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "__lazy_modules__ = ['enum']\nfrom enum import Enum",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice:",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice():",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "import enum",
        "class Choice(enum.auto):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "lazy import enum",
        "class Choice(enum.Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Enum.Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2\nfrom enum import Enum",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Enum):",
        " FIRST = auto()\n SECOND = auto()\nfrom enum import auto",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2\nChoice = replacement",
        None
    )]
    #[case(
        "import enum\ncached = enum",
        "class Choice(enum.Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "import enum\ncached: object = enum",
        "class Choice(enum.Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "import enum\ncached = (saved := enum)",
        "class Choice(enum.Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2\nglobal Choice",
        None
    )]
    #[case(
        "lazy from enum import Enum",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case("import unrelated\nfrom enum import Enum", "class Choice(Enum):", " FIRST = 1\n SECOND = 2", Some(&["SECOND"][..]))]
    #[case(
        "from enum import Enum\nexec(\"pass\")",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2",
        None
    )]
    #[case(
        "from enum import Enum",
        "class Choice(Enum):",
        " FIRST = 1\n SECOND = 2\nChoice.method.extra = 3",
        None
    )]
    #[case("from enum import Enum", "class Choice(Enum):", " FIRST = 1\n SECOND = 2\nfactory().attribute = 3", Some(&["SECOND"][..]))]
    #[case("import enum", "class Choice(enum):", " FIRST = 1\n SECOND = 2", None)]
    fn enum_identity_requires_a_supported_eager_import_and_simple_class_header(
        #[case] prefix: &str,
        #[case] header: &str,
        #[case] body: &str,
        #[case] expected: Option<&[&str]>,
    ) {
        let source = format!("{prefix}\n{header}\n{body}\nChoice.FIRST\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let index = run_build(parsed.syntax(), &source, &|| false).unwrap();
        let expression = &parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value;

        let actual = run_destinations(&index, expression, 100, &|| false).map(|(_, names)| names);

        assert_eq!(actual.as_deref(), expected);
    }

    #[rstest::rstest]
    fn enum_index_obeys_cancellation_at_each_polled_checkpoint(
        #[values(
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
            46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64
        )]
        checkpoint: usize,
    ) {
        let source =
            "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\nChoice.FIRST\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let complete_polls = Cell::new(0);
        let _complete = run_build(parsed.syntax(), source, &|| {
            complete_polls.set(complete_polls.get() + 1);
            false
        })
        .unwrap();
        let baseline_polled = complete_polls.get() > 0;
        let polls = Cell::new(0);
        let result = run_build(parsed.syntax(), source, &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        let build_failed = result.is_err();

        assert!(baseline_polled);
        assert_eq!(build_failed, checkpoint < complete_polls.get());
    }
    #[rstest::rstest]
    #[case("value", None)]
    #[case("holder.Choice.FIRST", None)]
    #[case("Missing.FIRST", None)]
    #[case("Choice.MISSING", None)]
    #[case("Choice.FIRST", Some(&["SECOND"][..]))]
    fn enum_destinations_require_a_known_direct_class_member(
        #[case] expression: &str,
        #[case] expected: Option<&[&str]>,
    ) {
        let source = format!(
            "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\n{expression}\n"
        );
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let index = run_build(parsed.syntax(), &source, &|| false).unwrap();
        let expression = &parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value;

        let actual = run_destinations(&index, expression, 100, &|| false).map(|(_, names)| names);

        assert_eq!(actual.as_deref(), expected);
    }

    #[test]
    fn enum_member_store_is_not_a_load_destination() {
        let source = "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\nChoice.FIRST = 3\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), source, &|| false).unwrap();
        let target = &parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_assign_stmt()
            .unwrap()
            .targets[0];

        let actual = run_destinations(&index, target, 100, &|| false);

        assert_eq!(actual, None);
    }

    #[test]
    fn enum_member_reference_inside_its_definition_is_not_yet_available() {
        let source = "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\n def method(self): return Choice.FIRST\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), source, &|| false).unwrap();
        let enum_definition_count = index.definitions.len();
        let class = parsed.syntax().body[1].as_class_def_stmt().unwrap();
        let method = class.body[2].as_function_def_stmt().unwrap();
        let expression = method.body[0]
            .as_return_stmt()
            .unwrap()
            .value
            .as_ref()
            .unwrap();
        let destinations = run_destinations(&index, expression, 100, &|| false);

        assert_eq!(enum_definition_count, 1);
        assert_eq!(destinations, None);
    }

    #[test]
    fn enum_member_lookup_does_not_resolve_a_shadowing_parameter_as_the_class() {
        let source = "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\ndef function(Choice): return Choice.FIRST\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), source, &|| false).unwrap();
        let enum_definition_count = index.definitions.len();
        let function = parsed.syntax().body[2].as_function_def_stmt().unwrap();
        let expression = function.body[0]
            .as_return_stmt()
            .unwrap()
            .value
            .as_ref()
            .unwrap();
        let destinations = run_destinations(&index, expression, 100, &|| false);

        assert_eq!(enum_definition_count, 1);
        assert_eq!(destinations, None);
    }

    #[rstest::rstest]
    #[case(0, None)]
    #[case(1, None)]
    #[case(2, None)]
    #[case(4, Some(&["SECOND", "THIRD", "FOURTH"][..]))]
    fn cancelled_destination_collection_discards_partial_results(
        #[case] checkpoint: usize,
        #[case] expected: Option<&[&str]>,
    ) {
        let source = "from enum import Enum\nclass Choice(Enum):\n FIRST = 1\n SECOND = 2\n THIRD = 3\n FOURTH = 4\nChoice.FIRST\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), source, &|| false).unwrap();
        let expression = &parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value;
        let polls = Cell::new(0);

        let actual = run_destinations(&index, expression, 100, &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        })
        .map(|(_, names)| names);

        assert_eq!(actual.as_deref(), expected);
    }
}
