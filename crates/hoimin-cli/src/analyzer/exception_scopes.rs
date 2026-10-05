//! Bounded lexical identities for the project summary (not a runtime value analysis).
use super::{MAX_SUMMARY_ENTRIES, mangled_name};
use ruff_python_ast::visitor::{self, Visitor};
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
    fn enclosing(&self, index: usize, name: &str) -> usize {
        for frame in self.frames[..index].iter().rev() {
            if matches!(frame.kind, Kind::Function | Kind::Comprehension) {
                if frame.declarations.globals.contains(name) {
                    return 0;
                }
                if frame.declarations.locals.contains(name)
                    && !frame.declarations.nonlocals.contains(name)
                {
                    return frame.id;
                }
            }
        }
        0
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
        let scopes = if frame.kind == Kind::Module || d.globals.contains(&name) {
            vec![0]
        } else if d.nonlocals.contains(&name) {
            vec![self.enclosing(index, &name)]
        } else if frame.kind == Kind::Class {
            if store {
                vec![frame.id]
            } else {
                vec![frame.id, self.enclosing(index, &name)]
            }
        } else if d.locals.contains(&name) {
            vec![frame.id]
        } else {
            vec![self.enclosing(index, &name)]
        };
        scopes
            .into_iter()
            .map(|scope| BindingKey {
                scope,
                name: name.clone(),
            })
            .collect()
    }
}
