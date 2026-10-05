//! Conservative lexical identities for project-local Python exception classes.
use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprContext, ModModule, Pattern, Stmt};
use ruff_text_size::{Ranged, TextRange};

use super::{AnalysisCancelled, AnalysisError, depth};

const MAX_STEPS: usize = 256;
pub(crate) const MAX_SUMMARY_ENTRIES: usize = 65_536;
type ClassId = (Utf8PathBuf, String);

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

#[derive(Default)]
struct Module {
    bindings: BTreeMap<String, Binding>,
    dynamic: bool,
    loaded: BTreeMap<String, usize>,
}

#[derive(Clone)]
struct Class {
    parent: Option<ClassId>,
    constructible: bool,
}

/// Only compact summaries survive project parsing. No target module is executed.
#[derive(Default)]
pub(crate) struct ExceptionIndex {
    modules: BTreeMap<Utf8PathBuf, Module>,
    source_hashes: BTreeMap<Utf8PathBuf, blake3::Hash>,
    module_paths: BTreeMap<String, Utf8PathBuf>,
    classes: BTreeMap<ClassId, Class>,
    visible: BTreeMap<Utf8PathBuf, BTreeMap<ClassId, Vec<(String, usize)>>>,
    children: BTreeMap<ClassId, BTreeSet<ClassId>>,
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

/// Writes explicitly escaping function scope invalidate module identities too.
#[derive(Default)]
struct Escapes(Bindings, Option<String>);
impl<'a> Visitor<'a> for Escapes {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::Global(g) = statement {
            for name in &g.names {
                self.0.names.insert(name.to_string());
                if let Some(mangled) = mangled_name(self.1.as_deref(), name) {
                    self.0.names.insert(mangled);
                }
            }
        }
        if let Stmt::ClassDef(d) = statement {
            let old = self.1.replace(d.name.to_string());
            visitor::walk_stmt(self, statement);
            self.1 = old;
        } else {
            visitor::walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Attribute(a) if a.ctx != ExprContext::Load => {
                if let Some(name) = reference(&a.value) {
                    let root = name.split('.').next().unwrap();
                    self.0.names.insert(root.to_owned());
                    if let Some(mangled) = mangled_name(self.1.as_deref(), root) {
                        self.0.names.insert(mangled);
                    }
                }
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
                self.0.dynamic = true;
            }
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }
}

impl Module {
    fn imports(&self) -> impl Iterator<Item = &String> {
        self.loaded
            .keys()
            .chain(self.bindings.values().filter_map(|b| match &b.kind {
                BindingKind::Import {
                    module, level: 0, ..
                } => Some(module),
                _ => None,
            }))
    }
    fn insert(&mut self, name: String, kind: BindingKind, end: usize) {
        self.bindings
            .entry(name)
            .and_modify(|b| b.kind = BindingKind::Unknown)
            .or_insert(Binding { kind, end });
    }
    fn parse(module: &ModModule) -> Self {
        let mut result = Self::default();
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
                Stmt::Import(d) => {
                    for alias in &d.names {
                        let module = alias.name.to_string();
                        let name = alias.asname.as_ref().map_or_else(
                            || module.split('.').next().unwrap().to_owned(),
                            ToString::to_string,
                        );
                        // Unaliased dotted imports bind only the package root.
                        let bound_module = if alias.asname.is_none() {
                            name.clone()
                        } else {
                            module.clone()
                        };
                        result.loaded.entry(module).or_insert(end);
                        let kind = BindingKind::Import {
                            module: bound_module,
                            member: None,
                            level: 0,
                        };
                        result.insert(name, kind, end);
                    }
                }
                Stmt::ImportFrom(d) => {
                    let imported = d.module.as_ref().map_or("", |m| m.as_str());

                    for alias in &d.names {
                        if alias.name.as_str() == "*" {
                            result.dynamic = true;
                        }
                        result.insert(
                            alias.asname.as_ref().unwrap_or(&alias.name).to_string(),
                            BindingKind::Import {
                                module: imported.to_owned(),
                                member: Some(alias.name.to_string()),
                                level: d.level,
                            },
                            end,
                        );
                    }
                }
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
        result.dynamic |= escapes.0.dynamic;
        for name in escapes.0.names {
            result.insert(name, BindingKind::Unknown, 0);
        }
        result
    }
}

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

impl ExceptionIndex {
    pub(crate) fn from_sources(
        sources: &[(Utf8PathBuf, String)],
        roots: &[Utf8PathBuf],
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisError> {
        Self::from_sources_with_inputs(sources, roots, &[], cancelled)
    }

    pub(crate) fn from_sources_with_inputs(
        sources: &[(Utf8PathBuf, String)],
        roots: &[Utf8PathBuf],
        inputs: &[Utf8PathBuf],
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisError> {
        let mut index = Self {
            module_paths: map_modules(sources, roots, inputs, cancelled)?,
            ..Self::default()
        };
        let mut binding_count = 0usize;
        for (path, source) in sources {
            if cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            index
                .source_hashes
                .insert(path.clone(), blake3::hash(source.as_bytes()));
            let parsed = ruff_python_parser::parse_unchecked_source(
                source,
                ruff_python_ast::PySourceType::Python,
            );
            if !parsed.has_valid_syntax() {
                depth::dispose(parsed.into_syntax());
                continue;
            }
            if let Err(error) = depth::check(parsed.syntax(), cancelled) {
                depth::dispose(parsed.into_syntax());
                return Err(error);
            }
            let summary = Module::parse(parsed.syntax());
            binding_count += summary.bindings.len();
            if binding_count > MAX_SUMMARY_ENTRIES {
                return Err(AnalysisError::HierarchyLimit);
            }
            index.modules.insert(path.clone(), summary);
        }
        index.resolve_relative_imports(cancelled)?;
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
            if let Some(parent) = &class.parent {
                index
                    .children
                    .entry(parent.clone())
                    .or_default()
                    .insert(id.clone());
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
            .flat_map(Module::imports)
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
            for binding in module.bindings.values() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                if let BindingKind::Import { module, level, .. } = &binding.kind
                    && *level > 0
                    && let Some(target) = relative_module(path, &name, module, *level)
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

    fn exclude_import_cycles(
        &mut self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), AnalysisCancelled> {
        let mut rejected = BTreeSet::new();
        for start in self.modules.keys() {
            let mut pending = vec![(start.clone(), 0usize)];
            let mut visited = BTreeSet::new();
            while let Some((path, depth)) = pending.pop() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                if depth >= MAX_STEPS {
                    rejected.insert(start.clone());
                    break;
                }
                if !visited.insert(path.clone()) {
                    continue;
                }
                for module in self.modules[&path].imports() {
                    if let Some(next) = self.module_paths.get(module) {
                        if next == start {
                            rejected.insert(start.clone());
                        } else if self.modules.contains_key(next) {
                            pending.push((next.clone(), depth + 1));
                        }
                    }
                }
            }
        }
        for path in rejected {
            self.modules.get_mut(&path).unwrap().dynamic = true;
        }
        Ok(())
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
                    for (module_name, module_path) in &self.module_paths {
                        if (module_name != imported
                            && !module_name.starts_with(&format!("{imported}.")))
                            || !module.loaded.contains_key(module_name)
                        {
                            continue;
                        }
                        if let Some(target) = self.modules.get(module_path) {
                            for member in target.bindings.keys() {
                                if cancelled() {
                                    return Err(AnalysisError::Cancelled);
                                }
                                let spelling =
                                    format!("{name}{}.{member}", &module_name[imported.len()..]);
                                if let Some(id) = self
                                    .binding_id(path, &spelling, usize::MAX)
                                    .filter(|id| self.classes.contains_key(id))
                                {
                                    remaining = remaining
                                        .checked_sub(1)
                                        .ok_or(AnalysisError::HierarchyLimit)?;
                                    visible.entry(id).or_default().push((
                                        spelling,
                                        binding.end.max(module.loaded[module_name]),
                                    ));
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

    fn replacements(
        &self,
        path: &Utf8Path,
        name: &str,
        before: usize,
        raised: bool,
        excluded: &BTreeSet<String>,
        budget: (usize, &dyn Fn() -> bool),
    ) -> Vec<String> {
        if excluded.contains(name.split('.').next().unwrap()) {
            return Vec::new();
        }
        let Some(id) = self.binding_id(path, name, before) else {
            return Vec::new();
        };
        let Some(class) = self.classes.get(&id) else {
            return Vec::new();
        };
        if raised && !class.constructible {
            return Vec::new();
        }
        let siblings = class.parent.as_ref().and_then(|p| self.children.get(p));
        let children = self.children.get(&id);
        let related = class
            .parent
            .iter()
            .chain(siblings.into_iter().flatten())
            .chain(children.into_iter().flatten());
        let mut result = BTreeSet::new();
        for destination in related {
            if (budget.1)() {
                break;
            }
            if *destination == id
                || (raised
                    && !self
                        .classes
                        .get(destination)
                        .is_some_and(|c| c.constructible))
            {
                continue;
            }
            if let Some(aliases) = self.visible.get(path).and_then(|v| v.get(destination))
                && let Some((name, _)) = aliases.iter().find(|(n, end)| {
                    *end <= before && !excluded.contains(n.split('.').next().unwrap())
                })
            {
                result.insert(name.clone());
                if result.len() > budget.0.saturating_add(1) {
                    result.pop_last();
                }
            }
        }
        result.into_iter().collect()
    }

    pub(crate) fn collect(
        &self,
        path: &Utf8Path,
        module: &ModModule,
        limit: usize,
        cancelled: &impl Fn() -> bool,
        mut emit: impl FnMut(TextRange, String),
    ) -> Result<bool, AnalysisCancelled> {
        let mut collector = Collector {
            index: self,
            limit,
            path,
            excluded: BTreeSet::new(),
            deferred: None,
            class_name: None,
            disabled: false,
            cancelled,
            stopped: false,
            skipped: false,
            emit: &mut emit,
        };
        collector.visit_body(&module.body);
        if collector.stopped || cancelled() {
            Err(AnalysisCancelled)
        } else {
            Ok(collector.skipped)
        }
    }
}

struct Collector<'i, 'p, F, C> {
    index: &'i ExceptionIndex,
    limit: usize,
    path: &'p Utf8Path,
    excluded: BTreeSet<String>,
    deferred: Option<usize>,
    class_name: Option<String>,
    disabled: bool,
    cancelled: &'p C,
    stopped: bool,
    skipped: bool,
    emit: F,
}
impl<F: FnMut(TextRange, String), C: Fn() -> bool> Collector<'_, '_, F, C> {
    fn occurrence(&mut self, expression: &Expr, raised: bool) {
        if self.disabled || self.stopped {
            return;
        }
        let Some(name) = reference(expression) else {
            self.skipped = true;
            return;
        };
        let before = self
            .deferred
            .unwrap_or_else(|| usize::from(expression.start()));
        let replacements = self.index.replacements(
            self.path,
            &name,
            before,
            raised,
            &self.excluded,
            (self.limit, self.cancelled),
        );
        self.skipped |= replacements.is_empty();
        for replacement in replacements {
            if (self.cancelled)() {
                self.stopped = true;
                return;
            }
            (self.emit)(expression.range(), replacement);
        }
    }
}
impl<'a, F: FnMut(TextRange, String), C: Fn() -> bool> Visitor<'a> for Collector<'_, '_, F, C> {
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
                    if let Some(visible) = self.index.visible.get(self.path) {
                        for aliases in visible.values() {
                            for (name, _) in aliases {
                                if name
                                    .split('.')
                                    .any(|part| part.starts_with("__") && !part.ends_with("__"))
                                {
                                    self.excluded
                                        .insert(name.split('.').next().unwrap().to_owned());
                                }
                            }
                        }
                    }
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
                        Expr::Call(c) => self.occurrence(&c.func, true),
                        expr => self.occurrence(expr, true),
                    }
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
    fn visit_except_handler(&mut self, handler: &'a ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(type_) = &handler.type_ {
            self.occurrence(type_, false);
        }
        self.visit_body(&handler.body);
    }
}

fn map_modules(
    sources: &[(Utf8PathBuf, String)],
    roots: &[Utf8PathBuf],
    inputs: &[Utf8PathBuf],
    cancelled: &impl Fn() -> bool,
) -> Result<BTreeMap<String, Utf8PathBuf>, AnalysisError> {
    let mut module_paths = BTreeMap::new();
    let mut claimed = BTreeSet::new();
    for root in roots {
        let mut mappings = BTreeMap::<String, Option<Utf8PathBuf>>::new();
        for path in sources.iter().map(|(path, _)| path).chain(inputs) {
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
