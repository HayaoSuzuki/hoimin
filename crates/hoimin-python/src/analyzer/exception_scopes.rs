//! Bounded lexical identities for the project summary (not a runtime value analysis).
use super::{MAX_SUMMARY_ENTRIES, mangled_name};
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Comprehension, Expr, ExprContext, Parameters, Pattern, Stmt};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct BindingKey {
    pub scope: usize,
    pub name: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Module,
    Function,
    Class,
    Comprehension,
}

#[derive(Default)]
pub(super) struct Declarations {
    locals: BTreeSet<String>,
    globals: BTreeSet<String>,
    nonlocals: BTreeSet<String>,
    pub exceeded: bool,
}
impl Declarations {
    fn add(&mut self, name: &str) {
        if !self.exceeded {
            self.locals.insert(name.to_owned());
            self.exceeded = self.locals.len() > MAX_SUMMARY_ENTRIES;
        }
    }
    pub fn parameters(&mut self, parameters: &Parameters) {
        for parameter in parameters {
            self.add(parameter.as_parameter().name.as_str());
        }
    }
    pub fn targets(&mut self, generators: &[Comprehension]) {
        for generator in generators {
            self.visit_expr(&generator.target);
        }
    }
}

/// Visit definition-time expressions without entering the new body scope.
pub(super) fn headers<'a>(visitor: &mut impl Visitor<'a>, statement: &'a Stmt) {
    match statement {
        Stmt::FunctionDef(d) => {
            for decorator in &d.decorator_list {
                visitor.visit_decorator(decorator);
            }
            visitor.visit_parameters(&d.parameters);
            if let Some(returns) = &d.returns {
                visitor.visit_expr(returns);
            }
            if let Some(params) = &d.type_params {
                visitor.visit_type_params(params);
            }
        }
        Stmt::ClassDef(d) => {
            for decorator in &d.decorator_list {
                visitor.visit_decorator(decorator);
            }
            for base in d.bases() {
                visitor.visit_expr(base);
            }
            for keyword in d.keywords() {
                visitor.visit_expr(&keyword.value);
            }
            if let Some(params) = &d.type_params {
                visitor.visit_type_params(params);
            }
        }
        _ => {}
    }
}
impl<'a> Visitor<'a> for Declarations {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.exceeded {
            return;
        }
        match statement {
            Stmt::FunctionDef(d) => {
                self.add(d.name.as_str());
                headers(self, statement);
            }
            Stmt::ClassDef(d) => {
                self.add(d.name.as_str());
                headers(self, statement);
            }
            Stmt::Import(d) => {
                for alias in &d.names {
                    self.add(
                        alias
                            .asname
                            .as_ref()
                            .map_or_else(|| alias.name.split('.').next().unwrap(), |n| n.as_str()),
                    );
                }
            }
            Stmt::ImportFrom(d) => {
                for alias in &d.names {
                    self.add(alias.asname.as_ref().unwrap_or(&alias.name).as_str());
                }
            }
            Stmt::Global(d) => {
                for name in &d.names {
                    self.globals.insert(name.to_string());
                    if self.globals.len() > MAX_SUMMARY_ENTRIES {
                        break;
                    }
                }
                self.exceeded |= self.globals.len() > MAX_SUMMARY_ENTRIES;
            }
            Stmt::Nonlocal(d) => {
                for name in &d.names {
                    self.nonlocals.insert(name.to_string());
                    if self.nonlocals.len() > MAX_SUMMARY_ENTRIES {
                        break;
                    }
                }
                self.exceeded |= self.nonlocals.len() > MAX_SUMMARY_ENTRIES;
            }
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if self.exceeded {
            return;
        }
        match expression {
            Expr::Name(n) if n.ctx != ExprContext::Load => self.add(n.id.as_str()),
            Expr::Lambda(l) => {
                if let Some(p) = &l.parameters {
                    self.visit_parameters(p);
                }
            }
            _ => visitor::walk_expr(self, expression),
        }
    }
    fn visit_comprehension(&mut self, generator: &'a Comprehension) {
        // Targets belong to the comprehension; walruses in expressions belong outside.
        self.visit_expr(&generator.iter);
        for condition in &generator.ifs {
            self.visit_expr(condition);
        }
    }
    fn visit_except_handler(&mut self, handler: &'a ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(h) = handler;
        if let Some(name) = &h.name {
            self.add(name.as_str());
        }
        visitor::walk_except_handler(self, handler);
    }
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let name = match pattern {
            Pattern::MatchAs(p) => p.name.as_ref(),
            Pattern::MatchStar(p) => p.name.as_ref(),
            Pattern::MatchMapping(p) => p.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.add(name.as_str());
        }
        visitor::walk_pattern(self, pattern);
    }
}

struct Frame {
    id: usize,
    kind: Kind,
    class: Option<String>,
    declarations: Declarations,
    class_binding: Option<BindingKey>,
}
#[derive(Default)]
pub(super) struct Scopes {
    frames: Vec<Frame>,
    next_id: usize,
    total_names: usize,
    pub exceeded: bool,
}
impl Scopes {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    pub fn enter(&mut self, kind: Kind, class: Option<&str>, mut declarations: Declarations) {
        // A class cell refers to the class object, not to its class namespace.
        // Using the definition binding is conservative if that name is rebound.
        let class_binding = if kind == Kind::Class {
            class.and_then(|name| self.resolve(name, true, false).into_iter().next())
        } else {
            None
        };
        let class = class
            .map(str::to_owned)
            .or_else(|| self.frames.last().and_then(|f| f.class.clone()));
        for names in [
            &mut declarations.locals,
            &mut declarations.globals,
            &mut declarations.nonlocals,
        ] {
            *names = std::mem::take(names)
                .into_iter()
                .map(|name| mangled_name(class.as_deref(), &name).unwrap_or(name))
                .collect();
            self.total_names = self.total_names.saturating_add(names.len());
        }
        self.exceeded |= declarations.exceeded
            || self.next_id >= MAX_SUMMARY_ENTRIES
            || self.total_names > 2 * MAX_SUMMARY_ENTRIES;
        self.frames.push(Frame {
            id: self.next_id,
            kind,
            class,
            declarations,
            class_binding,
        });
        self.next_id += 1;
    }
    pub fn leave(&mut self) {
        self.frames.pop();
    }
    fn canonical(&self, name: &str) -> String {
        mangled_name(self.frames.last().and_then(|f| f.class.as_deref()), name)
            .unwrap_or_else(|| name.to_owned())
    }
    fn enclosing(&self, index: usize, name: &str) -> BindingKey {
        let key = |scope| BindingKey {
            scope,
            name: name.to_owned(),
        };
        for frame in self.frames[..index].iter().rev() {
            if name == "__class__"
                && let Some(binding) = &frame.class_binding
            {
                return binding.clone();
            }
            if matches!(frame.kind, Kind::Function | Kind::Comprehension) {
                if frame.declarations.globals.contains(name) {
                    return key(0);
                }
                if frame.declarations.locals.contains(name)
                    && !frame.declarations.nonlocals.contains(name)
                {
                    return key(frame.id);
                }
            }
        }
        key(0)
    }
    pub fn resolve(&self, name: &str, store: bool, walrus: bool) -> Vec<BindingKey> {
        let name = self.canonical(name);
        let mut index = self.frames.len() - 1;
        if walrus {
            while self.frames[index].kind == Kind::Comprehension {
                index -= 1;
            }
        }
        let frame = &self.frames[index];
        let d = &frame.declarations;
        let key = |scope| BindingKey {
            scope,
            name: name.clone(),
        };
        if frame.kind == Kind::Module || d.globals.contains(&name) {
            vec![key(0)]
        } else if d.nonlocals.contains(&name) {
            vec![self.enclosing(index, &name)]
        } else if frame.kind == Kind::Class {
            if store {
                vec![key(frame.id)]
            } else {
                vec![key(frame.id), self.enclosing(index, &name)]
            }
        } else if d.locals.contains(&name) {
            vec![key(frame.id)]
        } else {
            vec![self.enclosing(index, &name)]
        }
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use ruff_python_parser::parse_module;

    #[rstest::rstest]
    #[case("@(deco := identity)\ndef function[T: (bound := object)](arg: (annotation := int) = (default := 1)) -> (result := int):\n    body_only = 1\n", &["deco", "function", "bound", "annotation", "default", "result"])]
    #[case("@(class_deco := identity)\nclass Child[T: (class_bound := object)]((base := Parent), metaclass=(meta := type)):\n    body_only = 1\n", &["class_deco", "Child", "class_bound", "base", "meta"])]
    fn declarations_collect_definition_headers_without_entering_bodies(
        #[case] source: &str,
        #[case] expected: &[&str],
    ) {
        let parsed = parse_module(source).unwrap();
        let mut declarations = Declarations::default();
        declarations.visit_body(&parsed.syntax().body);

        assert_eq!(
            declarations.locals,
            expected.iter().map(|name| (*name).to_owned()).collect()
        );
    }

    #[rstest::rstest]
    #[case("global")]
    #[case("nonlocal")]
    fn declaration_limits_stop_later_statements_and_expressions(#[case] declaration: &str) {
        let names = (0..MAX_SUMMARY_ENTRIES + 2)
            .map(|index| format!("name_{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let source = format!("{declaration} {names}\nlater = (walrus := 1)\n");
        let parsed = parse_module(&source).unwrap();
        let mut declarations = Declarations::default();
        declarations.visit_body(&parsed.syntax().body);
        let limit_exceeded = declarations.exceeded;
        let declaration_count = declarations.globals.len() + declarations.nonlocals.len();
        let locals_empty_before = declarations.locals.is_empty();
        let later_expression = ruff_python_parser::parse_expression("(after_limit := 1)").unwrap();
        declarations.visit_expr(&later_expression.syntax().body);
        let locals_empty_after = declarations.locals.is_empty();

        assert!(limit_exceeded);
        assert_eq!(declaration_count, MAX_SUMMARY_ENTRIES + 1);
        assert!(locals_empty_before);
        assert!(locals_empty_after);
    }
    #[test]
    fn enclosing_global_declaration_overrides_an_outer_local_binding() {
        let mut scopes = Scopes::default();
        scopes.enter(Kind::Module, None, Declarations::default());
        let mut outer = Declarations::default();
        outer.add("name");
        scopes.enter(Kind::Function, None, outer);
        let mut middle = Declarations::default();
        middle.globals.insert("name".into());
        scopes.enter(Kind::Function, None, middle);
        scopes.enter(Kind::Function, None, Declarations::default());

        let actual = scopes.resolve("name", false, false);

        assert_eq!(
            actual,
            vec![BindingKey {
                scope: 0,
                name: "name".into()
            }]
        );
    }

    #[test]
    fn declarations_stop_adding_names_at_the_summary_limit() {
        let mut declarations = Declarations {
            locals: (0..MAX_SUMMARY_ENTRIES)
                .map(|index| format!("name_{index}"))
                .collect(),
            ..Declarations::default()
        };
        declarations.add("overflow");
        declarations.add("ignored");

        let actual = declarations.locals.len();
        let actual_2 = declarations.locals.contains("overflow");
        let actual_3 = !declarations.locals.contains("ignored");

        assert!(declarations.exceeded);
        assert_eq!(actual, MAX_SUMMARY_ENTRIES + 1);
        assert!(actual_2);
        assert!(actual_3);
    }

    #[rstest::rstest]
    #[case("match value:\n    case [head, *tail] as whole: pass\n", &["head", "tail", "whole"])]
    #[case("match value:\n    case {'key': item, **rest}: pass\n", &["item", "rest"])]
    #[case("try:\n    pass\nexcept Exception as error:\n    pass\n", &["error"])]
    fn declarations_collect_pattern_and_handler_bindings(
        #[case] source: &str,
        #[case] expected: &[&str],
    ) {
        let parsed = parse_module(source).unwrap();
        let mut declarations = Declarations::default();
        declarations.visit_body(&parsed.syntax().body);

        assert_eq!(
            declarations.locals,
            expected.iter().map(|name| (*name).to_owned()).collect()
        );
        assert!(!declarations.exceeded);
    }
    #[test]
    fn comprehension_conditions_bind_walruses_in_the_enclosing_scope() {
        let parsed =
            parse_module("result = [item for item in values if (condition := item)]\n").unwrap();
        let mut declarations = Declarations::default();
        declarations.visit_body(&parsed.syntax().body);

        assert_eq!(
            declarations.locals,
            BTreeSet::from(["result", "condition"].map(str::to_owned))
        );
    }
    #[test]
    fn non_definition_statement_has_no_definition_time_bindings() {
        let parsed = parse_module("pass\n").unwrap();
        let mut declarations = Declarations::default();
        headers(&mut declarations, &parsed.syntax().body[0]);

        let actual = declarations.locals.is_empty();

        assert!(actual);
    }
}
