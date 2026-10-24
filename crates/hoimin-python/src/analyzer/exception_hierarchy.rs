//! Conservative lexical identities for project-local Python exception classes.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Included, Unbounded};

use crate::visitor::{self, Visitor};
use camino::{Utf8Path, Utf8PathBuf};
use ruff_python_ast::{Expr, ExprContext, ModModule, Pattern, Stmt};
use ruff_text_size::{Ranged, TextRange};

use super::{AnalysisCancelled, AnalysisError, depth};

#[path = "exception_import_graph.rs"]
mod import_graph;

const MAX_STEPS: usize = 256;
pub const MAX_SUMMARY_ENTRIES: usize = 65_536;
type ClassId = (Utf8PathBuf, String);
#[path = "exception_scopes.rs"]
mod scopes;
use scopes::{BindingKey, Declarations, Kind, Scopes};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SkipReason {
    DisabledScope,
    UnsupportedExpression,
    UnresolvedBinding,
    UntrustedModule,
}
impl SkipReason {
    fn label(self) -> &'static str {
        match self {
            Self::DisabledScope => "disabled_scope",
            Self::UnsupportedExpression => "unsupported_expression",
            Self::UnresolvedBinding => "unresolved_binding",
            Self::UntrustedModule => "untrusted_module",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum IneligibleReason {
    NoUserDefinedClass,
    NoRelatedClass,
    NoVisibleDestination,
    ConstructorPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExceptionSite {
    Raise,
    Handler,
}

/// Eligibility before intersecting relatives with visible, allowed aliases.
enum RelatedEligibility {
    NoRelated,
    ConstructorRejected,
    Eligible,
}

impl RelatedEligibility {
    fn empty_reason(self) -> IneligibleReason {
        match self {
            Self::NoRelated => IneligibleReason::NoRelatedClass,
            Self::ConstructorRejected => IneligibleReason::ConstructorPolicy,
            Self::Eligible => IneligibleReason::NoVisibleDestination,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum SiteOutcome<'a> {
    Candidates(Vec<&'a str>),
    Ineligible(IneligibleReason),
    Skipped(SkipReason),
}

/// Fixed reason vocabulary bounds storage independently of the number of sites.
#[derive(Default)]
pub(crate) struct HierarchyReport {
    emitted: usize,
    ineligible: BTreeMap<IneligibleReason, (usize, TextRange)>,
    skipped: BTreeMap<SkipReason, (usize, TextRange)>,
}
impl HierarchyReport {
    fn skip(&mut self, reason: SkipReason, range: TextRange) {
        Self::record(&mut self.skipped, reason, range);
    }
    fn record<K: Ord>(entries: &mut BTreeMap<K, (usize, TextRange)>, reason: K, range: TextRange) {
        let entry = entries.entry(reason).or_insert((0, range));
        entry.0 = entry.0.saturating_add(1);
        if range.start() < entry.1.start() {
            entry.1 = range;
        }
    }
    pub(crate) fn diagnostic(&self) -> Option<(TextRange, String)> {
        let first = self
            .skipped
            .values()
            .map(|(_, range)| *range)
            .min_by_key(Ranged::start)?;
        let reasons = self
            .skipped
            .iter()
            .map(|(reason, (count, _))| format!("{}={count}", reason.label()))
            .collect::<Vec<_>>()
            .join(", ");
        Some((
            first,
            format!("exception hierarchy: analysis skipped ({reasons})"),
        ))
    }
}

#[derive(Clone)]
enum BindingKind {
    Class {
        base: String,
        transparent: bool,
    },
    Import {
        module: String,
        member: Option<String>,
        level: u32,
    },
    Unknown,
}

#[derive(Clone)]
struct Binding {
    kind: BindingKind,
    end: usize,
}

#[derive(Clone, Copy, Default)]
struct ImportUse {
    eager: bool,
    mutated: bool,
}
impl ImportUse {
    fn merge(&mut self, other: Self) {
        self.eager |= other.eager;
        self.mutated |= other.mutated;
    }
}

#[derive(Default)]
struct Module {
    bindings: BTreeMap<String, Binding>,
    dynamic: bool,
    loaded: BTreeMap<String, usize>,
    dependencies: BTreeMap<(String, u32), ImportUse>,
}

#[derive(Clone)]
struct Class {
    parent: Option<ClassId>,
    constructible: bool,
}

#[derive(Default)]
struct ChildClasses {
    ids: BTreeSet<ClassId>,
    constructible: usize,
}

impl ChildClasses {
    // Class identities have immutable constructibility in the resolved index.
    // Repeated insertion of an identity must not increase the summary count.
    fn insert(&mut self, id: ClassId, constructible: bool) {
        if self.ids.insert(id) {
            self.constructible += usize::from(constructible);
        }
    }

    fn iter(&self) -> impl Iterator<Item = &ClassId> {
        self.ids.iter()
    }

    fn len(&self) -> usize {
        self.ids.len()
    }

    fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn has_constructible(&self) -> bool {
        self.constructible > 0
    }

    // The source belongs to this parent's child group.
    fn has_constructible_sibling(&self, source: &Class) -> bool {
        self.constructible > usize::from(source.constructible)
    }
}

/// Only compact summaries survive project parsing. No target module is executed.
#[derive(Default)]
pub struct ExceptionIndex {
    modules: BTreeMap<Utf8PathBuf, Module>,
    source_hashes: BTreeMap<Utf8PathBuf, blake3::Hash>,
    module_paths: BTreeMap<String, Utf8PathBuf>,
    classes: BTreeMap<ClassId, Class>,
    visible: BTreeMap<Utf8PathBuf, BTreeMap<ClassId, Vec<(String, usize)>>>,
    children: BTreeMap<ClassId, ChildClasses>,
}

fn reference(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => Some(format!(
            "{}.{}",
            reference(&attribute.value)?,
            attribute.attr
        )),
        _ => None,
    }
}

fn ordinary_builtin(name: &str) -> bool {
    matches!(
        name,
        "Exception"
            | "ArithmeticError"
            | "AssertionError"
            | "AttributeError"
            | "BufferError"
            | "EOFError"
            | "ImportError"
            | "LookupError"
            | "MemoryError"
            | "NameError"
            | "OSError"
            | "ReferenceError"
            | "RuntimeError"
            | "StopAsyncIteration"
            | "StopIteration"
            | "SyntaxError"
            | "SystemError"
            | "TypeError"
            | "ValueError"
            | "Warning"
            | "FloatingPointError"
            | "OverflowError"
            | "ZeroDivisionError"
            | "ModuleNotFoundError"
            | "IndexError"
            | "KeyError"
            | "UnboundLocalError"
            | "BlockingIOError"
            | "ChildProcessError"
            | "ConnectionError"
            | "FileExistsError"
            | "FileNotFoundError"
            | "InterruptedError"
            | "IsADirectoryError"
            | "NotADirectoryError"
            | "PermissionError"
            | "ProcessLookupError"
            | "TimeoutError"
            | "BrokenPipeError"
            | "ConnectionAbortedError"
            | "ConnectionRefusedError"
            | "ConnectionResetError"
            | "NotImplementedError"
            | "RecursionError"
            | "IndentationError"
            | "TabError"
            | "UnicodeError"
            | "UnicodeDecodeError"
            | "UnicodeEncodeError"
            | "UnicodeTranslateError"
            | "BytesWarning"
            | "DeprecationWarning"
            | "EncodingWarning"
            | "FutureWarning"
            | "ImportWarning"
            | "PendingDeprecationWarning"
            | "ResourceWarning"
            | "RuntimeWarning"
            | "SyntaxWarning"
            | "UnicodeWarning"
            | "UserWarning"
            | "EnvironmentError"
            | "IOError"
    )
}

/// A scope-wide may-bind scan deliberately sacrifices candidates after any ambiguity.
#[derive(Default)]
struct Bindings {
    names: BTreeSet<String>,
    dynamic: bool,
}

impl<'a> Visitor<'a> for Bindings {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(d) => {
                self.names.insert(d.name.to_string());
                let headers = header_bindings(statement);
                self.names.extend(headers.names);
                self.dynamic |= headers.dynamic;
            }
            Stmt::ClassDef(d) => {
                self.names.insert(d.name.to_string());
                let headers = header_bindings(statement);
                self.names.extend(headers.names);
                self.dynamic |= headers.dynamic;
            }
            Stmt::Import(d) => {
                for alias in &d.names {
                    self.names.insert(alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_owned(),
                        ToString::to_string,
                    ));
                }
            }
            Stmt::ImportFrom(d) => {
                for alias in &d.names {
                    if alias.name.as_str() == "*" {
                        self.dynamic = true;
                    }
                    self.names
                        .insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
            Stmt::Global(d) => {
                self.names.extend(d.names.iter().map(ToString::to_string));
            }
            Stmt::Nonlocal(d) => {
                self.names.extend(d.names.iter().map(ToString::to_string));
            }
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
                return;
            }
            Expr::Name(n) if n.ctx != ExprContext::Load => {
                self.names.insert(n.id.to_string());
            }
            Expr::Attribute(a) if a.ctx != ExprContext::Load => {
                if let Some(name) = reference(&a.value) {
                    self.names
                        .insert(name.split('.').next().unwrap().to_owned());
                }
            }
            Expr::Call(c)
                if reference(&c.func).is_some_and(|name| {
                    matches!(
                        name.as_str(),
                        "exec"
                            | "eval"
                            | "globals"
                            | "locals"
                            | "vars"
                            | "setattr"
                            | "delattr"
                            | "__import__"
                    )
                }) =>
            {
                self.dynamic = true;
            }
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }
    fn visit_comprehension(&mut self, generator: &'a ruff_python_ast::Comprehension) {
        // Iteration targets are local to the implicit comprehension scope.
        // Assignment expressions in its expressions still bind in the outer scope.
        self.visit_expr(&generator.iter);
        for condition in &generator.ifs {
            self.visit_expr(condition);
        }
    }
    fn visit_except_handler(&mut self, handler: &'a ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(name) = &handler.name {
            self.names.insert(name.to_string());
        }
        if let Some(type_) = &handler.type_ {
            self.visit_expr(type_);
        }
        self.visit_body(&handler.body);
    }
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        match pattern {
            Pattern::MatchAs(p) => {
                if let Some(name) = &p.name {
                    self.names.insert(name.to_string());
                }
            }
            Pattern::MatchStar(p) => {
                if let Some(name) = &p.name {
                    self.names.insert(name.to_string());
                }
            }
            Pattern::MatchMapping(p) => {
                if let Some(name) = &p.rest {
                    self.names.insert(name.to_string());
                }
            }
            _ => {}
        }
        visitor::walk_pattern(self, pattern);
    }
}

fn header_bindings(statement: &Stmt) -> Bindings {
    let mut bindings = Bindings::default();
    match statement {
        Stmt::FunctionDef(d) => {
            for decorator in &d.decorator_list {
                bindings.visit_decorator(decorator);
            }
            bindings.visit_parameters(&d.parameters);
            if let Some(returns) = &d.returns {
                bindings.visit_expr(returns);
            }
        }
        Stmt::ClassDef(d) => {
            for decorator in &d.decorator_list {
                bindings.visit_decorator(decorator);
            }
            for base in d.bases() {
                bindings.visit_expr(base);
            }
            for keyword in d.keywords() {
                bindings.visit_expr(&keyword.value);
            }
        }
        _ => {}
    }
    bindings
}

fn mangled_name(class: Option<&str>, name: &str) -> Option<String> {
    let class = class?.trim_start_matches('_');
    (!class.is_empty() && name.starts_with("__") && !name.ends_with("__"))
        .then(|| format!("_{class}{name}"))
}

/// Escaping writes and imports can change module identities from nested scopes.
#[derive(Default)]
struct Escapes {
    bindings: Bindings,
    scopes: Scopes,
    imports: BTreeMap<(String, u32), ImportUse>,
    import_aliases: BTreeSet<(BindingKey, String, u32)>,
    assignments: BTreeSet<(BindingKey, BindingKey)>,
    attribute_roots: BTreeSet<BindingKey>,
    limit_exceeded: bool,
    deferred: bool,
}
impl Escapes {
    fn comprehension(&mut self, generators: &[ruff_python_ast::Comprehension], elements: &[&Expr]) {
        let Some(first) = generators.first() else {
            return;
        };
        self.visit_expr(&first.iter);
        let mut declarations = Declarations::default();
        declarations.targets(generators);
        self.scopes.enter(Kind::Comprehension, None, declarations);
        for (index, generator) in generators.iter().enumerate() {
            if index != 0 {
                self.visit_expr(&generator.iter);
            }
            self.visit_expr(&generator.target);
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
        }
        for element in elements {
            self.visit_expr(element);
        }
        self.scopes.leave();
    }
    fn assignment(&mut self, target: &Expr, source: &Expr, walrus: bool) {
        if let Expr::Name(target) = target
            && let Some(source) = reference(source)
        {
            for target in self.scopes.resolve(&target.id, true, walrus) {
                for source in self
                    .scopes
                    .resolve(source.split('.').next().unwrap(), false, false)
                {
                    self.assignments.insert((target.clone(), source));
                }
            }
            self.limit_exceeded |= self.assignments.len() > MAX_SUMMARY_ENTRIES;
        }
    }

    /// May-alias edges retain every assignment, including rebinding and cycles.
    /// Each reached name is queued once, so reverse source order cannot lose writes.
    fn affected_roots(&self) -> BTreeSet<BindingKey> {
        let mut sources: BTreeMap<&BindingKey, Vec<&BindingKey>> = BTreeMap::new();
        for (target, source) in &self.assignments {
            sources.entry(target).or_default().push(source);
        }
        let mut marked = self.attribute_roots.clone();
        let mut pending: Vec<_> = self.attribute_roots.iter().collect();
        while let Some(target) = pending.pop() {
            if let Some(aliases) = sources.get(target) {
                for &source in aliases {
                    if marked.insert(source.clone()) {
                        pending.push(source);
                    }
                }
            }
        }
        marked
    }

    fn import(&mut self, module: String, level: u32, bound: &str, origin: &str, eager: bool) {
        if self.limit_exceeded {
            return;
        }
        self.imports.entry((module, level)).or_default().eager |= eager && !self.deferred;
        for name in self.scopes.resolve(bound, true, false) {
            self.import_aliases.insert((name, origin.to_owned(), level));
        }
        self.limit_exceeded = self.import_aliases.len() > MAX_SUMMARY_ENTRIES
            || self.imports.len() > MAX_SUMMARY_ENTRIES;
    }
    fn attribute_target(&mut self, expression: &Expr) {
        if let Some(name) = reference(expression) {
            let root = name.split('.').next().unwrap();
            self.attribute_roots
                .extend(self.scopes.resolve(root, false, false));
            self.limit_exceeded |= self.attribute_roots.len() > MAX_SUMMARY_ENTRIES;
        }
    }
}
impl<'a> Visitor<'a> for Escapes {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        let module = self.scopes.is_empty();
        if module {
            let mut declarations = Declarations::default();
            declarations.visit_body(body);
            self.scopes.enter(Kind::Module, None, declarations);
        }
        for statement in body {
            self.visit_stmt(statement);
        }
        self.limit_exceeded |= self.scopes.exceeded;
        if module {
            self.scopes.leave();
        }
    }
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.limit_exceeded || self.scopes.exceeded {
            self.limit_exceeded = true;
            return;
        }
        match statement {
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.assignment(target, &assign.value, false);
                }
            }
            Stmt::AnnAssign(assign) => {
                if let Some(value) = &assign.value {
                    self.assignment(&assign.target, value, false);
                }
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    let module = alias.name.to_string();
                    let bound = alias
                        .asname
                        .as_ref()
                        .map_or_else(|| module.split('.').next().unwrap(), |name| name.as_str());
                    let origin = if alias.asname.is_some() {
                        module.clone()
                    } else {
                        bound.to_owned()
                    };
                    self.import(module.clone(), 0, bound, &origin, !import.is_lazy);
                }
            }
            Stmt::ImportFrom(import) => {
                let module = import
                    .module
                    .as_ref()
                    .map_or_else(String::new, ToString::to_string);
                for alias in &import.names {
                    self.import(
                        module.clone(),
                        import.level,
                        alias.asname.as_ref().unwrap_or(&alias.name),
                        &module,
                        !import.is_lazy,
                    );
                }
            }
            _ => {}
        }
        match statement {
            Stmt::FunctionDef(d) => {
                scopes::headers(self, statement);
                let mut declarations = Declarations::default();
                declarations.parameters(&d.parameters);
                declarations.visit_body(&d.body);
                self.scopes.enter(Kind::Function, None, declarations);
                let old = std::mem::replace(&mut self.deferred, true);
                self.visit_body(&d.body);
                self.deferred = old;
                self.scopes.leave();
            }
            Stmt::ClassDef(d) => {
                scopes::headers(self, statement);
                let mut declarations = Declarations::default();
                declarations.visit_body(&d.body);
                self.scopes
                    .enter(Kind::Class, Some(d.name.as_str()), declarations);
                self.visit_body(&d.body);
                self.scopes.leave();
            }
            Stmt::Global(g) => {
                // Keep the existing conservative treatment of global declarations.
                for name in &g.names {
                    for key in self.scopes.resolve(name, true, false) {
                        if key.scope == 0 {
                            self.bindings.names.insert(key.name);
                        }
                    }
                }
            }
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if self.limit_exceeded || self.scopes.exceeded {
            self.limit_exceeded = true;
            return;
        }
        match expression {
            Expr::Named(named) => self.assignment(&named.target, &named.value, true),
            Expr::Attribute(a) if a.ctx != ExprContext::Load => self.attribute_target(&a.value),
            Expr::Lambda(l) => {
                let mut declarations = Declarations::default();
                if let Some(parameters) = &l.parameters {
                    self.visit_parameters(parameters);
                    declarations.parameters(parameters);
                }
                declarations.visit_expr(&l.body);
                self.scopes.enter(Kind::Function, None, declarations);
                let old = std::mem::replace(&mut self.deferred, true);
                self.visit_expr(&l.body);
                self.deferred = old;
                self.scopes.leave();
                return;
            }
            Expr::ListComp(c) => {
                self.comprehension(&c.generators, &[&c.elt]);
                return;
            }
            Expr::SetComp(c) => {
                self.comprehension(&c.generators, &[&c.elt]);
                return;
            }
            Expr::DictComp(c) => {
                if let Some(key) = &c.key {
                    self.comprehension(&c.generators, &[key, &c.value]);
                } else {
                    self.comprehension(&c.generators, &[&c.value]);
                }
                return;
            }
            Expr::Generator(c) => {
                self.comprehension(&c.generators, &[&c.elt]);
                return;
            }
            Expr::Call(c)
                if reference(&c.func).is_some_and(|n| {
                    matches!(
                        n.as_str(),
                        "exec"
                            | "eval"
                            | "globals"
                            | "locals"
                            | "vars"
                            | "setattr"
                            | "delattr"
                            | "__import__"
                    )
                }) =>
            {
                self.bindings.dynamic = true;
                if reference(&c.func)
                    .is_some_and(|name| matches!(name.as_str(), "setattr" | "delattr"))
                    && let Some(target) = c.arguments.args.first()
                {
                    self.attribute_target(target);
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }
}

impl Module {
    fn possible_imports(&self) -> impl Iterator<Item = &String> {
        self.dependencies
            .keys()
            .filter(|(_, level)| *level == 0)
            .map(|(name, _)| name)
    }
    fn insert(&mut self, name: String, kind: BindingKind, end: usize) {
        self.bindings
            .entry(name)
            .and_modify(|b| b.kind = BindingKind::Unknown)
            .or_insert(Binding { kind, end });
    }
    fn record_import(&mut self, import: &ruff_python_ast::StmtImport, end: usize, uncertain: bool) {
        for alias in &import.names {
            let module = alias.name.to_string();
            let name = alias.asname.as_ref().map_or_else(
                || module.split('.').next().unwrap().to_owned(),
                ToString::to_string,
            );
            let kind = if import.is_lazy || uncertain {
                BindingKind::Unknown
            } else {
                self.loaded.entry(module.clone()).or_insert(end);
                BindingKind::Import {
                    module: if alias.asname.is_none() {
                        name.clone()
                    } else {
                        module
                    },
                    member: None,
                    level: 0,
                }
            };
            self.insert(name, kind, end);
        }
    }
    fn record_import_from(
        &mut self,
        import: &ruff_python_ast::StmtImportFrom,
        end: usize,
        uncertain: bool,
    ) {
        let module = import.module.as_ref().map_or("", |m| m.as_str());
        for alias in &import.names {
            self.dynamic |= alias.name.as_str() == "*";
            let kind = if import.is_lazy || uncertain {
                BindingKind::Unknown
            } else {
                BindingKind::Import {
                    module: module.to_owned(),
                    member: Some(alias.name.to_string()),
                    level: import.level,
                }
            };
            self.insert(
                alias.asname.as_ref().unwrap_or(&alias.name).to_string(),
                kind,
                end,
            );
        }
    }
    fn parse(module: &ModModule) -> Result<Self, AnalysisError> {
        let imports_uncertain = super::lazy_imports::configuration_present(module, &|| false)?;
        let mut result = Self {
            dynamic: imports_uncertain,
            ..Self::default()
        };
        for statement in &module.body {
            let headers = header_bindings(statement);
            result.dynamic |= headers.dynamic;
            for name in headers.names {
                result.insert(name, BindingKind::Unknown, 0);
            }
            let end = usize::from(statement.end());
            match statement {
                Stmt::ClassDef(d) => {
                    let base = d.bases().first().and_then(reference);
                    let safe_body = d.body.iter().all(|s| matches!(s, Stmt::Pass(_) | Stmt::FunctionDef(_)) || matches!(s, Stmt::Expr(e) if matches!(e.value.as_ref(), Expr::StringLiteral(_))));
                    let has_subclass_hook = d.body.iter().any(|s| matches!(s, Stmt::FunctionDef(f) if f.name.as_str() == "__init_subclass__"));
                    let kind = if d.bases().len() == 1
                        && d.keywords().is_empty()
                        && d.decorator_list.is_empty()
                        && d.type_params.is_none()
                        && safe_body
                        && !has_subclass_hook
                    {
                        base.map_or(BindingKind::Unknown, |base| BindingKind::Class { base, transparent: !d.body.iter().any(|s| matches!(s, Stmt::FunctionDef(f) if matches!(f.name.as_str(), "__init__" | "__new__"))) })
                    } else {
                        BindingKind::Unknown
                    };
                    result.insert(d.name.to_string(), kind, end);
                }
                Stmt::Import(d) => result.record_import(d, end, imports_uncertain),
                Stmt::ImportFrom(d) => result.record_import_from(d, end, imports_uncertain),
                _ => {
                    let mut bindings = Bindings::default();
                    bindings.visit_stmt(statement);
                    result.dynamic |= bindings.dynamic;
                    for name in bindings.names {
                        result.insert(name, BindingKind::Unknown, end);
                    }
                }
            }
        }
        let mut escapes = Escapes::default();
        escapes.visit_body(&module.body);
        if escapes.limit_exceeded {
            return Err(AnalysisError::HierarchyLimit);
        }
        let affected = escapes.affected_roots();
        escapes.bindings.names.extend(
            affected
                .iter()
                .filter(|key| key.scope == 0)
                .map(|key| key.name.clone()),
        );
        for (name, origin, level) in escapes.import_aliases {
            if affected.contains(&name) {
                escapes.imports.entry((origin, level)).or_default().mutated = true;
            }
        }
        result.dynamic |= escapes.bindings.dynamic;
        result.dependencies = escapes.imports;
        if imports_uncertain {
            for dependency in result.dependencies.values_mut() {
                dependency.eager = false;
            }
        }
        for name in escapes.bindings.names {
            result.insert(name, BindingKind::Unknown, 0);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "exception_alias_oracle_tests.rs"]
mod alias_oracle_tests;

// Shared by extraction and snapshot adapters. In the standalone parser tests,
// only the extraction adapter is compiled, leaving the workspace helpers unused.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/support/exception_hierarchy_oracle.rs"]
pub(crate) mod oracle_corpus;

fn relative_module(path: &Utf8Path, current: &str, imported: &str, level: u32) -> Option<String> {
    if level == 0 {
        return Some(imported.to_owned());
    }
    let mut parts = current.split('.').collect::<Vec<_>>();
    if path.file_name() != Some("__init__.py") {
        parts.pop();
    }
    for _ in 1..level {
        parts.pop()?;
    }
    if parts.is_empty() {
        return None;
    }
    if !imported.is_empty() {
        parts.push(imported);
    }
    Some(parts.join("."))
}

fn module_name(path: &Utf8Path, root: &Utf8Path) -> Option<String> {
    let relative = if root == "." {
        path
    } else {
        path.strip_prefix(root).ok()?
    };
    let stem = relative.with_extension("");
    let mut parts = stem
        .components()
        .map(|c| c.as_str())
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>();
    if parts.last() == Some(&"__init__") {
        parts.pop();
    }
    // Interpreter modules can take precedence over project search roots. The
    // analyzer must not invent classes from a coincidentally named local file.
    if parts.is_empty()
        || include_str!("python_stdlib_names.txt")
            .lines()
            .any(|name| name == parts[0])
    {
        None
    } else {
        Some(parts.join("."))
    }
}

/// Builds an index from reserved paths and borrowed, per-file source text.
///
/// Only owned summaries survive `add_source`. Mapping and summary errors are
/// retained until `finish`, allowing callers to finish bounded input validation
/// and preserve acquisition-error precedence without retaining source bodies.
#[must_use]
pub struct ExceptionIndexBuilder {
    pending: Result<IndexBuild, AnalysisError>,
}

struct IndexBuild {
    index: ExceptionIndex,
    binding_count: usize,
    dependency_count: usize,
}

impl ExceptionIndexBuilder {
    pub fn new(
        paths: &[Utf8PathBuf],
        roots: &[Utf8PathBuf],
        inputs: &[Utf8PathBuf],
        cancelled: &impl Fn() -> bool,
    ) -> Self {
        Self {
            pending: map_modules(paths, roots, inputs, cancelled).map(|module_paths| IndexBuild {
                index: ExceptionIndex {
                    module_paths,
                    ..ExceptionIndex::default()
                },
                binding_count: 0,
                dependency_count: 0,
            }),
        }
    }

    /// Borrows this source only during summary extraction. The first mapping or
    /// analysis failure is reported by `finish`; subsequent sources are ignored.
    pub fn add_source(&mut self, path: &Utf8Path, source: &str, cancelled: &impl Fn() -> bool) {
        if let Ok(build) = &mut self.pending
            && let Err(error) = build.add_source(path, source, cancelled)
        {
            self.pending = Err(error);
        }
    }

    /// Resolves imports, classes and visibility after all source acquisition.
    ///
    /// # Errors
    /// Returns the first mapping/summary failure, cancellation or hierarchy bounds.
    pub fn finish(self, cancelled: &impl Fn() -> bool) -> Result<ExceptionIndex, AnalysisError> {
        self.pending?.index.finish_index(cancelled)
    }
}

impl IndexBuild {
    fn add_source(
        &mut self,
        path: &Utf8Path,
        source: &str,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), AnalysisError> {
        if cancelled() {
            return Err(AnalysisError::Cancelled);
        }
        self.index
            .source_hashes
            .insert(path.to_owned(), blake3::hash(source.as_bytes()));
        let parsed = match depth::parse_checked(source, cancelled) {
            Ok(parsed) => parsed,
            Err(depth::CheckedParseError::InvalidSyntax) => return Ok(()),
            Err(depth::CheckedParseError::Analysis(error)) => return Err(error),
        };
        let summary = Module::parse(parsed.syntax())?;
        self.binding_count += summary.bindings.len();
        self.dependency_count += summary.dependencies.len();
        if self.binding_count > MAX_SUMMARY_ENTRIES || self.dependency_count > MAX_SUMMARY_ENTRIES {
            return Err(AnalysisError::HierarchyLimit);
        }
        self.index.modules.insert(path.to_owned(), summary);
        Ok(())
    }
}

impl ExceptionIndex {
    pub(crate) fn from_sources(
        sources: &[(Utf8PathBuf, String)],
        roots: &[Utf8PathBuf],
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisError> {
        let paths = sources
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let mut builder = ExceptionIndexBuilder::new(&paths, roots, &[], cancelled);
        for (path, source) in sources {
            builder.add_source(path, source, cancelled);
        }
        builder.finish(cancelled)
    }

    fn finish_index(self, cancelled: &impl Fn() -> bool) -> Result<Self, AnalysisError> {
        let mut index = self;
        index.resolve_relative_imports(cancelled)?;
        index.exclude_submodule_collisions(cancelled)?;
        index.exclude_mutated_imports(cancelled)?;
        index.exclude_import_cycles(cancelled)?;
        let ids = index
            .modules
            .iter()
            .flat_map(|(p, m)| {
                m.bindings.iter().filter_map(move |(n, b)| {
                    matches!(b.kind, BindingKind::Class { .. }).then_some((p.clone(), n.clone()))
                })
            })
            .collect::<Vec<_>>();
        for id in ids {
            if cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            if let Some(class) = index.resolve_class(&id) {
                index.classes.insert(id, class);
            }
        }
        for (id, class) in &index.classes {
            if cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            if let Some(parent) = &class.parent {
                let group = index.children.entry(parent.clone()).or_default();
                group.insert(id.clone(), class.constructible);
            }
        }
        index.build_visible(cancelled)?;
        Ok(index)
    }

    pub(crate) fn source_matches(&self, path: &Utf8Path, source: &str) -> bool {
        self.source_hashes
            .get(path)
            .is_none_or(|hash| *hash == blake3::hash(source.as_bytes()))
    }

    /// A relative import depends on the name used to load its containing module,
    /// not on the first filesystem root that happens to contain its source.
    fn resolve_relative_imports(
        &mut self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), AnalysisCancelled> {
        let mut aliases = BTreeMap::<Utf8PathBuf, Vec<String>>::new();
        for (name, path) in &self.module_paths {
            aliases.entry(path.clone()).or_default().push(name.clone());
        }
        let mut pending = self
            .modules
            .values()
            .flat_map(Module::possible_imports)
            .filter(|name| self.module_paths.contains_key(*name))
            .cloned()
            .collect::<BTreeSet<_>>();
        pending.extend(
            aliases
                .values()
                .filter(|names| names.len() == 1)
                .flatten()
                .cloned(),
        );
        let mut contexts = BTreeMap::<Utf8PathBuf, BTreeSet<String>>::new();
        while let Some(name) = pending.pop_first() {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let Some(path) = self.module_paths.get(&name) else {
                continue;
            };
            if !contexts
                .entry(path.clone())
                .or_default()
                .insert(name.clone())
            {
                continue;
            }
            let Some(module) = self.modules.get(path) else {
                continue;
            };
            for (imported, level) in module.dependencies.keys() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                if *level > 0
                    && let Some(target) = relative_module(path, &name, imported, *level)
                    && self.module_paths.contains_key(&target)
                {
                    pending.insert(target);
                }
            }
        }
        for (path, module) in &mut self.modules {
            let names = contexts.get(path);
            // A file loaded under two names defines distinct runtime class identities.
            module.dynamic |= names.is_some_and(|names| names.len() > 1);
            let context = names
                .filter(|names| names.len() == 1)
                .and_then(|names| names.first());
            for ((imported, level), usage) in std::mem::take(&mut module.dependencies) {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                let resolved = if level == 0 {
                    Some(imported)
                } else {
                    context.and_then(|name| relative_module(path, name, &imported, level))
                };
                if let Some(name) = resolved {
                    module
                        .dependencies
                        .entry((name, 0))
                        .and_modify(|previous| previous.merge(usage))
                        .or_insert(usage);
                }
            }
            for binding in module.bindings.values_mut() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                if let BindingKind::Import { module, level, .. } = &mut binding.kind
                    && *level > 0
                {
                    if let Some(resolved) =
                        context.and_then(|name| relative_module(path, name, module, *level))
                    {
                        *module = resolved;
                        *level = 0;
                    } else {
                        binding.kind = BindingKind::Unknown;
                    }
                }
            }
        }
        Ok(())
    }

    /// Importing a child module assigns it to the same-name parent attribute.
    /// Imports in nested scopes may execute before any analyzed function call.
    fn exclude_submodule_collisions(
        &mut self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), AnalysisCancelled> {
        let imports = self
            .modules
            .values()
            .flat_map(Module::possible_imports)
            .cloned()
            .collect::<BTreeSet<_>>();
        for imported in &imports {
            let mut name = imported.as_str();
            while let Some((parent, member)) = name.rsplit_once('.') {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                if let Some(path) = self.module_paths.get(parent)
                    && let Some(module) = self.modules.get_mut(path)
                    && let Some(binding) = module.bindings.get_mut(member)
                {
                    binding.kind = BindingKind::Unknown;
                }
                name = parent;
            }
        }
        Ok(())
    }

    fn exclude_mutated_imports(
        &mut self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), AnalysisCancelled> {
        let origins = self
            .modules
            .values()
            .flat_map(|module| module.dependencies.iter())
            .filter(|((_, level), usage)| *level == 0 && usage.mutated)
            .map(|((name, _), _)| name.clone())
            .collect::<BTreeSet<_>>();
        if origins.contains("builtins") {
            for module in self.modules.values_mut() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                module.dynamic = true;
            }
            return Ok(());
        }
        for (name, path) in &self.module_paths {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let mut prefix = name.as_str();
            let affected = loop {
                if origins.contains(prefix) {
                    break true;
                }
                if let Some((parent, _)) = prefix.rsplit_once('.') {
                    prefix = parent;
                } else {
                    break false;
                }
            };
            if affected && let Some(module) = self.modules.get_mut(path) {
                module.dynamic = true;
            }
        }
        Ok(())
    }

    fn exclude_import_cycles(
        &mut self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), AnalysisCancelled> {
        import_graph::exclude(&mut self.modules, &self.module_paths, cancelled)
    }

    fn binding_id(&self, path: &Utf8Path, name: &str, before: usize) -> Option<ClassId> {
        let module = self.modules.get(path)?;
        if module.dynamic {
            return None;
        }
        let (root, attr) = name
            .split_once('.')
            .map_or((name, None), |(r, a)| (r, Some(a)));
        let binding = module.bindings.get(root)?;
        if binding.end > before {
            return None;
        }
        match (&binding.kind, attr) {
            (BindingKind::Class { .. }, None) => Some((path.to_owned(), root.to_owned())),
            (BindingKind::Import { module, member, .. }, attr) => {
                let (module, member) = match (member, attr) {
                    (Some(member), None) => (module.clone(), member.clone()),
                    (None, Some(attr)) => {
                        let (prefix, member) = attr
                            .rsplit_once('.')
                            .map_or((String::new(), attr), |(p, n)| (format!(".{p}"), n));
                        (format!("{module}{prefix}"), member.to_owned())
                    }
                    _ => return None,
                };
                if member.is_empty() {
                    return None;
                }
                if attr.is_some()
                    && self
                        .modules
                        .get(path)?
                        .loaded
                        .get(&module)
                        .is_none_or(|end| *end > before)
                {
                    return None;
                }
                let path = self.module_paths.get(&module)?;
                let imported = self.modules.get(path)?;
                if imported.dynamic
                    || !matches!(
                        imported.bindings.get(&member)?.kind,
                        BindingKind::Class { .. }
                    )
                {
                    return None;
                }
                Some((path.clone(), member))
            }
            _ => None,
        }
    }

    fn builtin_base(&self, path: &Utf8Path, name: &str, before: usize) -> Option<String> {
        let module = self.modules.get(path)?;
        if module.dynamic {
            return None;
        }
        if !name.contains('.') && !module.bindings.contains_key(name) && ordinary_builtin(name) {
            return Some(name.to_owned());
        }
        let (root, attr) = name
            .split_once('.')
            .map_or((name, None), |(r, a)| (r, Some(a)));
        let binding = module.bindings.get(root)?;
        if binding.end > before {
            return None;
        }
        if let BindingKind::Import { module, member, .. } = &binding.kind
            && module == "builtins"
        {
            let builtin = match (member.as_deref(), attr) {
                (Some(member), None) => member,
                (None, Some(attribute)) => attribute,
                _ => return None,
            };
            if ordinary_builtin(builtin) {
                return Some(builtin.to_owned());
            }
        }
        None
    }

    fn resolve_class(&self, id: &ClassId) -> Option<Class> {
        let mut current = id.clone();
        let mut seen = BTreeSet::new();
        let mut first_parent = None;
        let mut constructible = true;
        for _ in 0..MAX_STEPS {
            if !seen.insert(current.clone()) {
                return None;
            }
            let module = self.modules.get(&current.0)?;
            if module.dynamic {
                return None;
            }
            let binding = module.bindings.get(&current.1)?;
            let BindingKind::Class { base, transparent } = &binding.kind else {
                return None;
            };
            constructible &= transparent;
            if let Some(builtin) = self.builtin_base(&current.0, base, binding.end) {
                return Some(Class {
                    parent: first_parent,
                    constructible: constructible && builtin == "Exception",
                });
            }
            let parent = self.binding_id(&current.0, base, binding.end)?;
            if current == *id {
                first_parent = Some(parent.clone());
            }
            current = parent;
        }
        None
    }

    fn build_visible(&mut self, cancelled: &impl Fn() -> bool) -> Result<(), AnalysisError> {
        // Qualified resolution accepts only direct class bindings present in
        // `classes`. Borrow their names once; resolving each alias below still
        // enforces the importer's identity, loaded-module and shadowing rules.
        let mut members = BTreeMap::<&Utf8Path, Vec<&str>>::new();
        for (path, name) in self.classes.keys() {
            #[cfg(test)]
            visibility_tests::record_member_scan();
            if cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            members.entry(path).or_default().push(name);
        }
        let mut remaining = MAX_SUMMARY_ENTRIES;
        for (path, module) in &self.modules {
            let mut visible = BTreeMap::<ClassId, Vec<(String, usize)>>::new();
            for (name, binding) in &module.bindings {
                if cancelled() {
                    return Err(AnalysisError::Cancelled);
                }
                if let Some(id) = self
                    .binding_id(path, name, usize::MAX)
                    .filter(|id| self.classes.contains_key(id))
                {
                    remaining = remaining
                        .checked_sub(1)
                        .ok_or(AnalysisError::HierarchyLimit)?;
                    visible
                        .entry(id)
                        .or_default()
                        .push((name.clone(), binding.end));
                }
                if let BindingKind::Import {
                    module: imported,
                    member: None,
                    ..
                } = &binding.kind
                {
                    // Exact and descendant names are ordered as in module_paths,
                    // but only explicitly loaded modules can expose class spellings.
                    let prefix = format!("{imported}.");
                    let loaded = module.loaded.get_key_value(imported).into_iter().chain(
                        module
                            .loaded
                            .range::<str, _>((Included(prefix.as_str()), Unbounded))
                            .take_while(|(name, _)| name.starts_with(&prefix)),
                    );
                    for (module_name, loaded_end) in loaded {
                        #[cfg(test)]
                        visibility_tests::record_module_visit();
                        if cancelled() {
                            return Err(AnalysisError::Cancelled);
                        }
                        let Some(module_path) = self.module_paths.get(module_name) else {
                            continue;
                        };
                        if let Some(target_members) = members.get(module_path.as_path()) {
                            for member in target_members {
                                if cancelled() {
                                    return Err(AnalysisError::Cancelled);
                                }
                                let spelling =
                                    format!("{name}{}.{member}", &module_name[imported.len()..]);
                                #[cfg(test)]
                                visibility_tests::record_member_resolution();
                                if let Some(id) = self
                                    .binding_id(path, &spelling, usize::MAX)
                                    .filter(|id| self.classes.contains_key(id))
                                {
                                    remaining = remaining
                                        .checked_sub(1)
                                        .ok_or(AnalysisError::HierarchyLimit)?;
                                    visible
                                        .entry(id)
                                        .or_default()
                                        .push((spelling, binding.end.max(*loaded_end)));
                                }
                            }
                        }
                    }
                }
            }
            for aliases in visible.values_mut() {
                aliases.sort();
            }
            self.visible.insert(path.clone(), visible);
        }
        Ok(())
    }

    fn related_eligibility(
        &self,
        class: &Class,
        site: ExceptionSite,
        siblings: Option<&ChildClasses>,
        children: Option<&ChildClasses>,
    ) -> RelatedEligibility {
        if class.parent.is_none() && children.is_none_or(ChildClasses::is_empty) {
            return RelatedEligibility::NoRelated;
        }
        match site {
            ExceptionSite::Handler => RelatedEligibility::Eligible,
            ExceptionSite::Raise
                if class.parent.as_ref().is_some_and(|parent| {
                    self.classes.get(parent).is_some_and(|c| c.constructible)
                }) || siblings.is_some_and(|s| s.has_constructible_sibling(class))
                    || children.is_some_and(ChildClasses::has_constructible) =>
            {
                RelatedEligibility::Eligible
            }
            ExceptionSite::Raise => RelatedEligibility::ConstructorRejected,
        }
    }

    fn replacements(
        &self,
        path: &Utf8Path,
        name: &str,
        before: usize,
        site: ExceptionSite,
        excluded: &BTreeSet<String>,
        budget: (usize, &dyn Fn() -> bool),
    ) -> SiteOutcome<'_> {
        if excluded.contains(name.split('.').next().unwrap()) {
            return SiteOutcome::Skipped(SkipReason::UnresolvedBinding);
        }
        if self.modules.get(path).is_some_and(|module| module.dynamic) {
            return SiteOutcome::Skipped(SkipReason::UntrustedModule);
        }
        let Some(id) = self.binding_id(path, name, before) else {
            return if self.builtin_base(path, name, before).is_some() {
                SiteOutcome::Ineligible(IneligibleReason::NoUserDefinedClass)
            } else {
                SiteOutcome::Skipped(SkipReason::UnresolvedBinding)
            };
        };
        let Some(class) = self.classes.get(&id) else {
            return SiteOutcome::Skipped(SkipReason::UnresolvedBinding);
        };
        if site == ExceptionSite::Raise && !class.constructible {
            return SiteOutcome::Ineligible(IneligibleReason::ConstructorPolicy);
        }
        let siblings = class.parent.as_ref().and_then(|p| self.children.get(p));
        let children = self.children.get(&id);
        // Parent, siblings (excluding self), and children are disjoint in the
        // resolved single-parent acyclic graph. Counts retain eligibility facts
        // even when none of those identities is visible in this module.
        let eligibility = self.related_eligibility(class, site, siblings, children);
        let related_count = usize::from(class.parent.is_some())
            + siblings.map_or(0, |s| s.len().saturating_sub(1))
            + children.map_or(0, ChildClasses::len);
        let visible = self.visible.get(path);
        let mut result = BTreeSet::new();
        let mut visit = |destination: &ClassId| {
            #[cfg(test)]
            lookup_tests::RELATED_VISITS.update(|n| n + 1);
            if *destination == id {
                return;
            }
            if site == ExceptionSite::Raise
                && !self
                    .classes
                    .get(destination)
                    .is_some_and(|c| c.constructible)
            {
                return;
            }
            if let Some(aliases) = visible.and_then(|v| v.get(destination))
                && let Some((name, _)) = aliases.iter().find(|(n, end)| {
                    *end <= before && !excluded.contains(n.split('.').next().unwrap())
                })
            {
                result.insert(name.as_str());
                if result.len() > budget.0.saturating_add(1) {
                    result.pop_last();
                }
            }
        };
        if let Some(visible) = visible.filter(|v| v.len() < related_count) {
            for destination in visible.keys().take_while(|_| !(budget.1)()) {
                let related = class.parent.as_ref() == Some(destination)
                    || self.classes.get(destination).is_some_and(|other| {
                        other.parent.as_ref() == Some(&id)
                            || (class.parent.is_some() && other.parent == class.parent)
                    });
                if related {
                    visit(destination);
                }
                #[cfg(test)]
                if !related {
                    lookup_tests::RELATED_VISITS.update(|n| n + 1);
                }
            }
        } else {
            for destination in class
                .parent
                .iter()
                .chain(siblings.into_iter().flat_map(ChildClasses::iter))
                .chain(children.into_iter().flat_map(ChildClasses::iter))
            {
                if (budget.1)() {
                    break;
                }
                visit(destination);
            }
        }
        if result.is_empty() {
            SiteOutcome::Ineligible(eligibility.empty_reason())
        } else {
            SiteOutcome::Candidates(result.into_iter().collect())
        }
    }

    pub(crate) fn collect(
        &self,
        path: &Utf8Path,
        module: &ModModule,
        limit: usize,
        cancelled: &impl Fn() -> bool,
        mut emit: impl FnMut(TextRange, String) -> Result<(), AnalysisError>,
    ) -> Result<HierarchyReport, AnalysisError> {
        let mut collector = Collector {
            index: self,
            limit,
            path,
            excluded: BTreeSet::new(),
            private_roots: None,
            deferred: None,
            class_name: None,
            disabled: false,
            cancelled,
            stopped: false,
            report: HierarchyReport::default(),
            failure: None,
            emit: &mut emit,
        };
        collector.visit_body(&module.body);
        if let Some(error) = collector.failure {
            Err(error)
        } else if collector.stopped || cancelled() {
            Err(AnalysisError::Cancelled)
        } else {
            Ok(collector.report)
        }
    }
}

struct Collector<'i, 'p, F, C> {
    index: &'i ExceptionIndex,
    limit: usize,
    path: &'p Utf8Path,
    excluded: BTreeSet<String>,
    private_roots: Option<BTreeSet<&'i str>>,
    deferred: Option<usize>,
    class_name: Option<String>,
    disabled: bool,
    cancelled: &'p C,
    stopped: bool,
    report: HierarchyReport,
    failure: Option<AnalysisError>,
    emit: F,
}
impl<F: FnMut(TextRange, String) -> Result<(), AnalysisError>, C: Fn() -> bool>
    Collector<'_, '_, F, C>
{
    fn occurrence(&mut self, expression: &Expr, site: ExceptionSite) {
        if self.stopped {
            return;
        }
        if self.disabled {
            self.report
                .skip(SkipReason::DisabledScope, expression.range());
            return;
        }
        let Some(name) = reference(expression) else {
            self.report
                .skip(SkipReason::UnsupportedExpression, expression.range());
            return;
        };
        let before = self
            .deferred
            .unwrap_or_else(|| usize::from(expression.start()));
        let outcome = self.index.replacements(
            self.path,
            &name,
            before,
            site,
            &self.excluded,
            (self.limit, self.cancelled),
        );
        let replacements = match outcome {
            SiteOutcome::Candidates(replacements) => replacements,
            SiteOutcome::Ineligible(reason) => {
                HierarchyReport::record(&mut self.report.ineligible, reason, expression.range());
                return;
            }
            SiteOutcome::Skipped(reason) => {
                self.report.skip(reason, expression.range());
                return;
            }
        };
        for replacement in replacements {
            if (self.cancelled)() {
                self.stopped = true;
                return;
            }
            self.report.emitted = self.report.emitted.saturating_add(1);
            if let Err(error) = (self.emit)(expression.range(), replacement.to_owned()) {
                self.failure = Some(error);
                self.stopped = true;
                return;
            }
        }
    }
}
impl<'a, F: FnMut(TextRange, String) -> Result<(), AnalysisError>, C: Fn() -> bool> Visitor<'a>
    for Collector<'_, '_, F, C>
{
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if self.stopped || (self.cancelled)() {
            self.stopped = true;
            return;
        }
        match stmt {
            Stmt::FunctionDef(d) => {
                let old = (self.excluded.clone(), self.deferred, self.disabled);
                let mut bindings = Bindings::default();
                bindings.visit_body(&d.body);
                for p in &d.parameters {
                    bindings.names.insert(p.name().to_string());
                }
                self.excluded.extend(
                    bindings
                        .names
                        .iter()
                        .filter_map(|name| mangled_name(self.class_name.as_deref(), name)),
                );
                self.excluded.extend(bindings.names);
                self.deferred = self.deferred.or(Some(usize::from(d.start())));
                self.disabled |= bindings.dynamic || d.type_params.is_some();
                self.visit_body(&d.body);
                (self.excluded, self.deferred, self.disabled) = old;
            }
            // Class locals are not closures for methods. Do not mutate class bodies.
            Stmt::ClassDef(d) => {
                if d.type_params.is_none() {
                    let old = (
                        self.excluded.clone(),
                        self.class_name.replace(d.name.to_string()),
                    );
                    self.excluded.insert("__class__".into());
                    if self.private_roots.is_none() {
                        let mut roots = BTreeSet::new();
                        if let Some(visible) = self.index.visible.get(self.path) {
                            for aliases in visible.values() {
                                for (name, _) in aliases {
                                    #[cfg(test)]
                                    lookup_tests::PRIVATE_ALIAS_VISITS.update(|n| n + 1);
                                    if (self.cancelled)() {
                                        self.stopped = true;
                                        return;
                                    }
                                    if name
                                        .split('.')
                                        .any(|part| part.starts_with("__") && !part.ends_with("__"))
                                    {
                                        roots.insert(name.split('.').next().unwrap());
                                    }
                                }
                            }
                        }
                        self.private_roots = Some(roots);
                    }
                    self.excluded.extend(
                        self.private_roots
                            .as_ref()
                            .unwrap()
                            .iter()
                            .map(|root| (*root).to_owned()),
                    );
                    for stmt in &d.body {
                        if matches!(stmt, Stmt::FunctionDef(_)) {
                            self.visit_stmt(stmt);
                        }
                    }
                    (self.excluded, self.class_name) = old;
                }
            }
            Stmt::Raise(r) => {
                if let Some(expr) = &r.exc {
                    match expr.as_ref() {
                        Expr::Call(c) => self.occurrence(&c.func, ExceptionSite::Raise),
                        expr => self.occurrence(expr, ExceptionSite::Raise),
                    }
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
    fn visit_except_handler(&mut self, handler: &'a ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(type_) = &handler.type_ {
            self.occurrence(type_, ExceptionSite::Handler);
        }
        self.visit_body(&handler.body);
    }
}

fn map_modules(
    paths: &[Utf8PathBuf],
    roots: &[Utf8PathBuf],
    inputs: &[Utf8PathBuf],
    cancelled: &impl Fn() -> bool,
) -> Result<BTreeMap<String, Utf8PathBuf>, AnalysisError> {
    let mut module_paths = BTreeMap::new();
    let mut claimed = BTreeSet::new();
    for root in roots {
        let mut mappings = BTreeMap::<String, Option<Utf8PathBuf>>::new();
        for path in paths.iter().chain(inputs) {
            if cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            if let Some(name) = module_name(path, root) {
                mappings
                    .entry(name)
                    .and_modify(|p| {
                        if p.as_ref() != Some(path) {
                            *p = None;
                        }
                    })
                    .or_insert_with(|| Some(path.clone()));
                if mappings.len() > MAX_SUMMARY_ENTRIES {
                    return Err(AnalysisError::HierarchyLimit);
                }
            }
        }
        for (name, path) in mappings {
            let first = claimed.insert(name.clone());
            if claimed.len() > MAX_SUMMARY_ENTRIES {
                return Err(AnalysisError::HierarchyLimit);
            }
            if first && let Some(path) = path {
                module_paths.insert(name, path);
            }
        }
    }
    // A regular package beats namespace portions in earlier search roots.
    // Never combine a child from one directory with a selected parent from another.
    let mismatched = module_paths
        .iter()
        .filter_map(|(name, path)| {
            let mut prefix = name.as_str();
            while let Some((parent, _)) = prefix.rsplit_once('.') {
                if let Some(package) = module_paths.get(parent)
                    && (package.file_name() != Some("__init__.py")
                        || !path.starts_with(package.parent().unwrap()))
                {
                    return Some(name.clone());
                }
                prefix = parent;
            }
            None
        })
        .collect::<Vec<_>>();
    for name in mismatched {
        module_paths.remove(&name);
    }
    Ok(module_paths)
}

#[cfg(test)]
mod checked_parse_tests {
    use super::*;

    #[test]
    fn invalid_partial_module_is_skipped_while_valid_neighbor_is_indexed() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let invalid_path = Utf8PathBuf::from("broken.py");
                let valid_path = Utf8PathBuf::from("valid.py");
                let sources = [
                    (
                        invalid_path.clone(),
                        format!(
                            "class PartialError(Exception):\n    pass\nvalue = {}\ndef broken(:\n",
                            vec!["1"; 25_000].join("+")
                        ),
                    ),
                    (
                        valid_path.clone(),
                        "class ValidError(Exception):\n    pass\n".to_owned(),
                    ),
                ];
                let index = ExceptionIndex::from_sources(&sources, &[], &|| false).unwrap();
                assert!(!index.modules.contains_key(&invalid_path));
                assert!(index.modules.contains_key(&valid_path));
                assert!(index.source_matches(&invalid_path, &sources[0].1));
                assert!(!index.source_matches(&invalid_path, "changed"));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(test)]
#[path = "exception_visibility_tests.rs"]
mod visibility_tests;

#[cfg(test)]
#[path = "exception_import_graph_tests.rs"]
mod exception_import_graph_tests;

#[cfg(test)]
#[path = "exception_lookup_tests.rs"]
mod lookup_tests;
