//! String-role and explicit type-alias exclusions for selected runtime operators.
use super::{AnalysisCancelled, ContainmentIndex};
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ModModule, Stmt};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Exclusions {
    all: ContainmentIndex,
    aliases: ContainmentIndex,
    segments: ContainmentIndex,
}
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
            aliases: Vec::new(),
            segments: Vec::new(),
            cancelled,
            stopped: false,
        };
        roles.docstring(&module.body);
        roles.visit_body(&module.body);
        if roles.stopped || cancelled() {
            return Err(AnalysisCancelled);
        }
        Ok(Self {
            all: ContainmentIndex::new(roles.ranges),
            aliases: ContainmentIndex::new(roles.aliases),
            segments: ContainmentIndex::new(roles.segments),
        })
    }
    pub(super) fn contains_alias(&self, range: TextRange) -> bool {
        self.aliases
            .contains(usize::from(range.start()), usize::from(range.end()))
    }
    pub(super) fn contains(&self, range: TextRange) -> bool {
        self.all
            .contains(usize::from(range.start()), usize::from(range.end()))
    }
    pub(super) fn excludes_segment(&self, range: TextRange) -> bool {
        self.segments
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
    aliases: Vec<(usize, usize)>,
    segments: Vec<(usize, usize)>,
    cancelled: &'a F,
    stopped: bool,
}
impl<F> Roles<'_, F> {
    fn exclude(&mut self, range: TextRange) {
        self.ranges
            .push((usize::from(range.start()), usize::from(range.end())));
        self.segments
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
                    self.aliases
                        .push((usize::from(value.start()), usize::from(value.end())));
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
        match expression {
            Expr::FString(value) => {
                let range = expression.range();
                // Existing runtime string operators still exclude the entire f-string.
                self.ranges
                    .push((usize::from(range.start()), usize::from(range.end())));
                for element in value.value.elements() {
                    self.stopped |= (self.cancelled)();
                    if self.stopped {
                        return;
                    }
                    if let Some(field) = element.as_interpolation() {
                        let range = field.range();
                        // This also excludes nested f-strings in fields/format specs.
                        self.segments
                            .push((usize::from(range.start()), usize::from(range.end())));
                    }
                }
            }
            Expr::TString(_) => self.exclude(expression.range()),
            _ => visitor::walk_expr(self, expression),
        }
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn run_build(
        module: &ModModule,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Exclusions, AnalysisCancelled> {
        Exclusions::build(module, &cancelled)
    }

    #[rstest::rstest]
    #[case("from typing import *\nAlias: TypeAlias = 'value'", "'value'", (true, true, true))]
    #[case("Alias: TypeAlias = 'value'", "'value'", (false, false, false))]
    #[case("from typing import TypeAlias as Marker\nAlias: Marker = 'value'", "'value'", (true, true, true))]
    #[case("from typing import TypeAlias\nAlias: 'TypeAlias' = 'value'", "'value'", (true, true, true))]
    #[case("import typing as t\nAlias: 't.TypeAlias' = 'value'", "'value'", (true, true, true))]
    #[case("import typing_extensions as t\nAlias: '''(\n t.TypeAlias # marker\n)''' = 'value'", "'value'", (true, true, true))]
    #[case("import typing as t\nAlias: other.TypeAlias = 'value'", "'value'", (false, false, false))]
    #[case("import typing as t\nAlias: wrapper.t.TypeAlias = 'value'", "'value'", (false, false, false))]
    #[case("Alias: int = 'value'", "'value'", (false, false, false))]
    #[case("import unrelated\nvalue = 'value'", "'value'", (false, false, false))]
    #[case("'value'", "'value'", (true, false, true))]
    #[case("def function():\n 'value'", "'value'", (true, false, true))]
    #[case("class C:\n 'value'", "'value'", (true, false, true))]
    #[case("value = f\"prefix {'value'}\"", "prefix", (true, false, false))]
    #[case("value = f\"prefix {'value'}\"", "'value'", (true, false, true))]
    #[case("value = t\"'value'\"", "'value'", (true, false, true))]
    #[case("import typing as t\nAlias: t.TypeAlias = 'value'", "'value'", (true, true, true))]
    #[case("Alias: list[int] = 'value'", "'value'", (false, false, false))]
    #[case("from .typing import TypeAlias\nAlias: TypeAlias = 'value'", "'value'", (false, false, false))]
    #[case("from typing import Optional\nAlias: Optional = 'value'", "'value'", (false, false, false))]
    fn string_role_index_separates_aliases_docstrings_fields_and_runtime_segments(
        #[case] source: &str,
        #[case] marker: &str,
        #[case] expected: (bool, bool, bool),
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), &|| false).unwrap();
        let start = source.find(marker).unwrap();
        let range = TextRange::new(
            start.try_into().unwrap(),
            (start + marker.len()).try_into().unwrap(),
        );
        assert_eq!(
            (
                index.contains(range),
                index.contains_alias(range),
                index.excludes_segment(range)
            ),
            expected
        );
    }

    #[test]
    fn a_type_alias_declaration_without_a_value_adds_no_literal_exclusion() {
        let source = "from typing import TypeAlias\nAlias: TypeAlias";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = run_build(parsed.syntax(), &|| false).unwrap();
        let annotation = &parsed.syntax().body[1]
            .as_ann_assign_stmt()
            .unwrap()
            .annotation;
        assert!(!index.contains_alias(annotation.range()));
    }

    #[rstest::rstest]
    fn string_roles_cancel_during_marker_collection_or_formatted_fields(
        #[values(
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
            46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63
        )]
        checkpoint: usize,
    ) {
        let source = "from typing import TypeAlias\nimport typing as t\nAlias: TypeAlias = 'value'\ndef function():\n 'documentation'\n return f'prefix {item!r} and {other}'\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let total = std::cell::Cell::new(0);
        assert!(
            run_build(parsed.syntax(), &|| {
                total.set(total.get() + 1);
                false
            })
            .is_ok()
        );
        let polls = std::cell::Cell::new(0);
        let result = run_build(parsed.syntax(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        assert_eq!(result.is_err(), checkpoint < total.get());
    }
}
