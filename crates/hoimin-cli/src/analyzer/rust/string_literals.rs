//! Role exclusions for runtime string erasure; built only for this operator.
use super::{AnalysisCancelled, ContainmentIndex};
use ruff_python_ast::{
    Expr, ModModule, Stmt,
    visitor::{self, Visitor},
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Exclusions(ContainmentIndex);
impl Exclusions {
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let mut markers = Markers {
            names: HashSet::new(),
            modules: HashSet::new(),
            cancelled,
            stopped: false,
        };
        markers.visit_body(&module.body);
        if markers.stopped || cancelled() {
            return Err(AnalysisCancelled);
        }
        let mut roles = Roles {
            markers: &markers,
            ranges: Vec::new(),
            cancelled,
            stopped: false,
        };
        roles.docstring(&module.body);
        roles.visit_body(&module.body);
        if roles.stopped || cancelled() {
            return Err(AnalysisCancelled);
        }
        Ok(Self(ContainmentIndex::new(roles.ranges)))
    }
    pub(super) fn contains(&self, range: TextRange) -> bool {
        self.0
            .contains(usize::from(range.start()), usize::from(range.end()))
    }
}
struct Markers<'a, F> {
    names: HashSet<String>,
    modules: HashSet<String>,
    cancelled: &'a F,
    stopped: bool,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for Markers<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        self.stopped |= (self.cancelled)();
        if self.stopped {
            return;
        }
        match statement {
            Stmt::Import(s) => {
                for alias in &s.names {
                    if matches!(alias.name.as_str(), "typing" | "typing_extensions") {
                        self.modules
                            .insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                    }
                }
            }
            Stmt::ImportFrom(s)
                if s.level == 0
                    && s.module
                        .as_ref()
                        .is_some_and(|m| matches!(m.as_str(), "typing" | "typing_extensions")) =>
            {
                for alias in &s.names {
                    if alias.name.as_str() == "TypeAlias" {
                        self.names
                            .insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                    } else if alias.name.as_str() == "*" {
                        self.names.insert("TypeAlias".to_owned());
                    }
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        self.stopped |= (self.cancelled)();
        if !self.stopped {
            visitor::walk_expr(self, expression);
        }
    }
}
impl<F> Markers<'_, F> {
    fn alias_annotation(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Name(n) => self.names.contains(n.id.as_str()),
            Expr::StringLiteral(s) => {
                // Conservatively normalize trivia and redundant grouping in a
                // quoted marker. This bounded linear scan does not parse or execute
                // arbitrary forward-reference expressions.
                let name: String = s
                    .value
                    .to_str()
                    .lines()
                    .flat_map(|line| line.split('#').next().unwrap_or_default().chars())
                    .filter(|c| !c.is_whitespace() && !matches!(c, '(' | ')'))
                    .collect();
                self.names.contains(&name)
                    || name.split_once('.').is_some_and(|(module, member)| {
                        self.modules.contains(module) && member == "TypeAlias"
                    })
            }
            Expr::Attribute(a) if a.attr.as_str() == "TypeAlias" => {
                matches!(a.value.as_ref(), Expr::Name(n) if self.modules.contains(n.id.as_str()))
            }
            _ => false,
        }
    }
}
struct Roles<'a, F> {
    markers: &'a Markers<'a, F>,
    ranges: Vec<(usize, usize)>,
    cancelled: &'a F,
    stopped: bool,
}
impl<F> Roles<'_, F> {
    fn exclude(&mut self, range: TextRange) {
        self.ranges
            .push((usize::from(range.start()), usize::from(range.end())));
    }
    fn docstring(&mut self, body: &[Stmt]) {
        if let Some(Stmt::Expr(s)) = body.first()
            && matches!(s.value.as_ref(), Expr::StringLiteral(_))
        {
            self.exclude(s.value.range());
        }
    }
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for Roles<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        self.stopped |= (self.cancelled)();
        if self.stopped {
            return;
        }
        match statement {
            Stmt::FunctionDef(f) => self.docstring(&f.body),
            Stmt::ClassDef(c) => self.docstring(&c.body),
            Stmt::AnnAssign(a) if self.markers.alias_annotation(&a.annotation) => {
                if let Some(value) = &a.value {
                    self.exclude(value.range());
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        self.stopped |= (self.cancelled)();
        if self.stopped {
            return;
        }
        if matches!(expression, Expr::FString(_) | Expr::TString(_)) {
            self.exclude(expression.range());
        } else {
            visitor::walk_expr(self, expression);
        }
    }
}
