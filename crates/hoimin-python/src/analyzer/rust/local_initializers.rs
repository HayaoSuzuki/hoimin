//! Find single reaching initializers for exact receiver and literal proofs.
//!
//! The caller supplies the candidate value shapes. This index owns lexical
//! identity, execution order and with-target lifetime checks.

use super::{
    AnalysisCancelled, NameResolutionBuilder, NameResolutionIndex, NameScopeKind, ScopeId,
};
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprName, ModModule, Stmt};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct Index<T> {
    names: NameResolutionIndex,
    values: HashMap<usize, (T, Option<TextRange>)>,
}

impl<T: Copy> Index<T> {
    pub(super) fn build<'ast>(
        module: &'ast ModModule,
        cancelled: &dyn Fn() -> bool,
        classify: impl Fn(&'ast Expr, bool) -> Option<T>,
    ) -> Result<Self, AnalysisCancelled> {
        let mut scan = InitializerScan {
            names: HashSet::new(),
            values: HashMap::new(),
            direct_body: true,
            cancelled,
            classify,
        };
        scan.visit_body(&module.body);
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        if scan.values.is_empty() {
            return Ok(Self {
                names: NameResolutionIndex::default(),
                values: scan.values,
            });
        }

        let mut builder = NameResolutionBuilder::new(cancelled);
        builder.initializer_targets = scan.values.keys().copied().collect();
        builder.imported_names.clone_from(&scan.names);
        builder.identity_names = scan.names;
        builder.visit_body(&module.body);
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let names = builder.finish();
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        Ok(Self {
            names,
            values: scan.values,
        })
    }

    pub(super) fn get(&self, name: &ExprName) -> Option<T> {
        let initialized_at = self.names.reaching_initializer(name)?;
        let (value, limited_to) = self.values.get(&initialized_at)?;
        limited_to
            .is_none_or(|range| range.contains_range(name.range()))
            .then_some(*value)
    }
}

impl NameResolutionIndex {
    fn reaching_initializer(&self, name: &ExprName) -> Option<usize> {
        let source_start = usize::from(name.start());
        let site = self.occurrences.get(&source_start)?;
        let id = name.id.as_str();
        if site.temporarily_shadowed || self.scopes[0].wildcard || id.starts_with("__") {
            return None;
        }
        let mut owner = site.scope;
        while self.scopes[owner.0].kind == NameScopeKind::Comprehension {
            let scope = &self.scopes[owner.0];
            if scope.locals.contains(id) || scope.wildcard {
                return None;
            }
            owner = scope.parent?;
        }
        if let Some(offset) = self.initialization_in(owner, id, site.event) {
            return Some(offset);
        }
        // A function body can only run after creating its definition. A prior
        // module store in lexical evaluation order therefore also precedes the
        // read, provided the ordinary lexical lookup reaches that module name.
        if owner != ScopeId(0) && self.resolves_unique_module_at(source_start, id) {
            return self.initialization_in(ScopeId(0), id, site.event);
        }
        None
    }

    fn initialization_in(&self, owner: ScopeId, name: &str, before: usize) -> Option<usize> {
        let scope = &self.scopes[owner.0];
        if !matches!(scope.kind, NameScopeKind::Module | NameScopeKind::Function)
            || scope.wildcard
            || scope.globals.contains(name)
            || scope.nonlocals.contains(name)
            || scope.ambiguous_private_import(name)
        {
            return None;
        }
        let &(offset, event) = self.initializers.get(&owner)?.get(name)?;
        (event < before && scope.import_writes.get(name) == Some(&event)).then_some(offset)
    }
}

impl NameResolutionBuilder<'_> {
    pub(super) fn record_initializer(&mut self, name: &ExprName) {
        let offset = usize::from(name.start());
        if !self.initializer_targets.contains(&offset) {
            self.record_binding(name.id.as_str());
            return;
        }
        let id = name.id.as_str();
        let owner = self.current;
        let scope = &self.index.scopes[owner.0];
        let first = !scope.locals.contains(id) && !scope.import_writes.contains_key(id);
        self.record_binding(id);
        if first {
            self.index
                .initializers
                .entry(owner)
                .or_default()
                .insert(id.to_owned(), (offset, self.event));
        }
    }
}

struct InitializerScan<'a, T, F> {
    names: HashSet<String>,
    values: HashMap<usize, (T, Option<TextRange>)>,
    direct_body: bool,
    cancelled: &'a dyn Fn() -> bool,
    classify: F,
}

impl<'ast, T, F: Fn(&'ast Expr, bool) -> Option<T>> InitializerScan<'_, T, F> {
    fn record(&mut self, target: &Expr, value: &'ast Expr, within: Option<TextRange>) {
        if let Some(name) = target.as_name_expr()
            && let Some(classified) = (self.classify)(value, within.is_some())
        {
            self.names.insert(name.id.to_string());
            self.values
                .insert(usize::from(name.start()), (classified, within));
        }
    }

    fn definition_body(&mut self, statements: &'ast [Stmt]) {
        let previous = std::mem::replace(&mut self.direct_body, true);
        self.visit_body(statements);
        self.direct_body = previous;
    }
}

impl<'ast, T, F: Fn(&'ast Expr, bool) -> Option<T>> Visitor<'ast> for InitializerScan<'_, T, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if (self.cancelled)() {
            return;
        }
        match statement {
            Stmt::Assign(assign) if self.direct_body => {
                for target in &assign.targets {
                    if (self.cancelled)() {
                        return;
                    }
                    self.record(target, &assign.value, None);
                }
            }
            Stmt::AnnAssign(assign) if self.direct_body => {
                if let Some(value) = &assign.value {
                    self.record(&assign.target, value, None);
                }
            }
            Stmt::With(with) => {
                for item in &with.items {
                    if (self.cancelled)() {
                        return;
                    }
                    if let Some(target) = &item.optional_vars {
                        self.record(
                            target,
                            &item.context_expr,
                            Some(TextRange::new(target.end(), with.end())),
                        );
                    }
                }
            }
            Stmt::FunctionDef(definition) => {
                self.definition_body(&definition.body);
                return;
            }
            _ => {}
        }
        let previous = std::mem::replace(&mut self.direct_body, false);
        visitor::walk_stmt(self, statement);
        self.direct_body = previous;
    }

    fn visit_expr(&mut self, _expression: &'ast Expr) {}
}
