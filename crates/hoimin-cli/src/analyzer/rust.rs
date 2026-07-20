use std::collections::{BTreeSet, HashMap, HashSet};

use camino::Utf8Path;
use hoimin_core::{ByteSpan, LineRange, MutationOperator, MutationOperatorSelection};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{Expr, ModModule, Operator, Stmt, UnaryOp, visitor};
use ruff_python_parser::parse_module;
use ruff_text_size::Ranged;

use super::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

pub(crate) struct AnalyzeRequest<'a> {
    pub path: &'a Utf8Path,
    pub lines: &'a [LineRange],
    pub symbols: &'a [String],
    pub operators: &'a MutationOperatorSelection,
    pub max_candidates: usize,
}

pub(crate) struct AnalyzerOutput {
    pub candidates: Vec<AnalyzerCandidate>,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub truncated: bool,
}

pub(crate) fn analyze_source(request: &AnalyzeRequest<'_>, source: &str) -> AnalyzerOutput {
    let Ok(parsed) = parse_module(source) else {
        return invalid_syntax(request.path);
    };
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens());
    let mut candidates = Vec::new();
    let tokens: Vec<_> = parsed.tokens().iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        let range = token.range();
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let text = &source[start..end];
        let previous = index.checked_sub(1).and_then(|i| tokens.get(i));
        let next = tokens.get(index + 1);
        let (span_end, replacement, operator) = if text == "not"
            && next.is_some_and(|next| {
                &source[usize::from(next.range().start())..usize::from(next.range().end())] == "in"
            }) {
            (
                usize::from(next.unwrap().range().end()),
                "in".to_owned(),
                "membership",
            )
        } else if text == "is"
            && next.is_some_and(|next| {
                &source[usize::from(next.range().start())..usize::from(next.range().end())] == "not"
            })
        {
            (
                usize::from(next.unwrap().range().end()),
                "is".to_owned(),
                "identity",
            )
        } else if text == "not"
            && previous.is_some_and(|previous| {
                &source[usize::from(previous.range().start())..usize::from(previous.range().end())]
                    == "is"
            })
        {
            continue;
        } else if let Some((expression_start, expression_end)) = facts.not_operand_range(start) {
            (
                expression_end,
                source[expression_start..expression_end].to_owned(),
                "remove_not",
            )
        } else if previous.is_some_and(|previous| {
            &source[usize::from(previous.range().start())..usize::from(previous.range().end())]
                == "not"
                && matches!(text, "in" | "is")
        }) {
            continue;
        } else if let Some((replacement, operator)) = replacement(text, facts.is_unary_sign(start))
        {
            (end, replacement.to_owned(), operator)
        } else {
            continue;
        };
        let original = source[start..span_end].to_owned();
        let (line, column) = line_and_column(source, start);
        let symbol = facts.scope_at(start);
        let operator = MutationOperator::from_name(operator)
            .expect("token mutation operator must be configured");
        if selected(request, line, symbol.as_deref()) && request.operators.contains(operator) {
            candidates.push(AnalyzerCandidate {
                path: request.path.to_owned(),
                span: ByteSpan {
                    start: start as u64,
                    length: (span_end - start) as u64,
                },
                original,
                replacement,
                operator: operator.as_str().to_owned(),
                line,
                column,
                symbol,
            });
        }
    }
    candidates.extend(type_annotation_candidates(
        parsed.syntax(),
        source,
        &facts.imports,
        request,
    ));
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| {
        seen.insert((
            candidate.span.start,
            candidate.replacement.clone(),
            candidate.operator.clone(),
        ))
    });
    candidates.sort_by(|left, right| {
        left.span
            .start
            .cmp(&right.span.start)
            .then_with(|| left.operator.cmp(&right.operator))
    });
    let truncated = candidates.len() > request.max_candidates;
    if truncated {
        candidates.truncate(request.max_candidates);
    }
    let diagnostics = truncated
        .then(|| AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::CandidateLimitExceeded,
            path: Some(request.path.to_owned()),
            line: None,
            column: None,
            message: None,
        })
        .into_iter()
        .collect();
    AnalyzerOutput {
        candidates,
        diagnostics,
        truncated,
    }
}

fn invalid_syntax(path: &Utf8Path) -> AnalyzerOutput {
    AnalyzerOutput {
        candidates: Vec::new(),
        diagnostics: vec![AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::InvalidSyntax,
            path: Some(path.to_owned()),
            line: None,
            column: None,
            message: None,
        }],
        truncated: false,
    }
}

fn replacement(text: &str, unary: bool) -> Option<(&'static str, &'static str)> {
    let result = match text {
        "==" => ("!=", "compare_eq_ne"),
        "!=" => ("==", "compare_eq_ne"),
        "<" => ("<=", "compare_order"),
        "<=" => ("<", "compare_order"),
        ">" => (">=", "compare_order"),
        ">=" => (">", "compare_order"),
        "in" => ("not in", "membership"),
        "is" => ("is not", "identity"),
        "and" => ("or", "boolean_and_or"),
        "or" => ("and", "boolean_and_or"),
        "+=" => ("-=", "augmented_add_sub"),
        "-=" => ("+=", "augmented_add_sub"),
        "*" => ("/", "binary_mul_div"),
        "/" => ("*", "binary_mul_div"),
        "//" => ("%", "binary_floor_mod"),
        "%" => ("//", "binary_floor_mod"),
        "break" => ("continue", "break_continue"),
        "continue" => ("break", "break_continue"),
        "True" => ("False", "boolean_literal"),
        "False" => ("True", "boolean_literal"),
        "+" if unary => ("-", "unary_sign"),
        "-" if unary => ("+", "unary_sign"),
        "+" => ("-", "binary_add_sub"),
        "-" => ("+", "binary_add_sub"),
        _ => return None,
    };
    Some(result)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Ruff TextSize offsets cap parsed source at u32::MAX bytes, and code-point counts cannot exceed byte counts."
)]
fn line_and_column(source: &str, offset: usize) -> (u32, u32) {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.chars().count(), |(_, tail)| tail.chars().count()) as u32;
    (line, column)
}

fn selected(request: &AnalyzeRequest<'_>, line: u32, symbol: Option<&str>) -> bool {
    if request.lines.is_empty() && request.symbols.is_empty() {
        return true;
    }
    request
        .lines
        .iter()
        .any(|range| range.start <= line && line <= range.end)
        || request.symbols.iter().any(|selector| {
            let Some((module, qualname)) = selector.rsplit_once(':') else {
                return false;
            };
            let current_module = module_name(request.path);
            (current_module == module || current_module.ends_with(&format!(".{module}")))
                && symbol.is_some_and(|symbol| {
                    symbol == qualname || symbol.starts_with(&format!("{qualname}."))
                })
        })
}

fn module_name(path: &Utf8Path) -> String {
    let mut parts: Vec<_> = path.with_extension("").iter().map(str::to_owned).collect();
    if parts.last().is_some_and(|part| part == "__init__") {
        parts.pop();
    }
    parts.join(".")
}

#[derive(Default)]
struct AstFacts<'tokens> {
    imports: KnownImports,
    unary_sign_starts: HashSet<usize>,
    not_operands: Vec<(usize, usize, usize)>,
    scopes: Vec<ScopeRange>,
    qualname: Vec<String>,
    tokens: Option<&'tokens ruff_python_ast::token::Tokens>,
}
struct ScopeRange {
    start: usize,
    end: usize,
    symbol: String,
}

impl<'tokens> AstFacts<'tokens> {
    fn from_module(module: &ModModule, tokens: &'tokens ruff_python_ast::token::Tokens) -> Self {
        let mut facts = Self {
            imports: KnownImports::from_module(module),
            tokens: Some(tokens),
            ..Self::default()
        };
        for statement in &module.body {
            facts.visit_stmt(statement);
        }
        facts
    }

    fn not_operand_range(&self, start: usize) -> Option<(usize, usize)> {
        self.not_operands
            .iter()
            .find_map(|(not_start, operand_start, operand_end)| {
                (*not_start == start).then_some((*operand_start, *operand_end))
            })
    }

    fn is_unary_sign(&self, start: usize) -> bool {
        self.unary_sign_starts.contains(&start)
    }

    fn scope_at(&self, offset: usize) -> Option<String> {
        self.scopes
            .iter()
            .filter(|scope| scope.start <= offset && offset < scope.end)
            .max_by_key(|scope| scope.start)
            .map(|scope| scope.symbol.clone())
    }

    fn visit_definition(
        &mut self,
        name: &str,
        range: ruff_text_size::TextRange,
        decorators: &[ruff_python_ast::Decorator],
        statement: &Stmt,
    ) {
        let start = decorators
            .iter()
            .map(|decorator| usize::from(decorator.range().start()))
            .min()
            .unwrap_or_else(|| usize::from(range.start()));
        self.qualname.push(name.to_owned());
        self.scopes.push(ScopeRange {
            start,
            end: usize::from(range.end()),
            symbol: self.qualname.join("."),
        });
        visitor::walk_stmt(self, statement);
        self.qualname.pop();
    }
}

impl<'ast> Visitor<'ast> for AstFacts<'_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::FunctionDef(definition) => self.visit_definition(
                definition.name.as_str(),
                definition.range(),
                &definition.decorator_list,
                statement,
            ),
            Stmt::ClassDef(definition) => self.visit_definition(
                definition.name.as_str(),
                definition.range(),
                &definition.decorator_list,
                statement,
            ),
            _ => visitor::walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if let Expr::UnaryOp(unary) = expression {
            let start = usize::from(unary.range().start());
            match unary.op {
                UnaryOp::Not => {
                    let operand_range = ruff_python_ast::token::parenthesized_range(
                        unary.operand.as_ref().into(),
                        unary.into(),
                        self.tokens.expect("parser tokens are set"),
                    )
                    .unwrap_or_else(|| unary.operand.range());
                    self.not_operands.push((
                        start,
                        usize::from(operand_range.start()),
                        usize::from(operand_range.end()),
                    ));
                }
                UnaryOp::UAdd | UnaryOp::USub => {
                    self.unary_sign_starts.insert(start);
                }
                UnaryOp::Invert => {}
            }
        }
        visitor::walk_expr(self, expression);
    }
}

#[cfg(test)]
#[path = "rust_tests.rs"]
mod rust_tests;

#[derive(Default)]
struct KnownImports {
    direct: HashMap<String, String>,
    modules: HashMap<String, String>,
    type_vars: HashSet<String>,
}

impl KnownImports {
    fn from_module(module: &ModModule) -> Self {
        let mut imports = Self::default();
        for statement in &module.body {
            match statement {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        let name = alias.name.as_str();
                        if matches!(name, "typing" | "collections.abc") {
                            let local = alias.asname.as_ref().map_or_else(
                                || name.split('.').next().unwrap_or(name),
                                |asname| asname.as_str(),
                            );
                            imports.modules.insert(local.to_owned(), name.to_owned());
                        }
                    }
                }
                Stmt::ImportFrom(import) if import.level == 0 => {
                    let Some(module_name) = import.module.as_ref().map(|name| name.as_str()) else {
                        continue;
                    };
                    if !matches!(module_name, "typing" | "collections.abc") {
                        continue;
                    }
                    for alias in &import.names {
                        let imported = alias.name.as_str();
                        if is_known_type_name(imported) {
                            let local = alias
                                .asname
                                .as_ref()
                                .map_or(imported, |asname| asname.as_str());
                            imports
                                .direct
                                .insert(local.to_owned(), format!("{module_name}.{imported}"));
                        }
                    }
                }
                Stmt::Assign(assign) if is_type_var_call(assign.value.as_ref(), &imports) => {
                    for target in &assign.targets {
                        if let Expr::Name(name) = target {
                            imports.type_vars.insert(name.id.as_str().to_owned());
                        }
                    }
                }
                _ => {}
            }
        }
        imports
    }

    fn resolved_name(&self, expression: &Expr) -> Option<String> {
        let mut parts = Vec::new();
        let mut current = expression;
        loop {
            match current {
                Expr::Name(name) => {
                    let first = name.id.as_str();
                    if let Some(name) = self.direct.get(first) {
                        parts.push(name.clone());
                    } else if let Some(module) = self.modules.get(first) {
                        parts.push(module.clone());
                    } else {
                        parts.push(first.to_owned());
                    }
                    break;
                }
                Expr::Attribute(attribute) => {
                    parts.push(attribute.attr.as_str().to_owned());
                    current = attribute.value.as_ref();
                }
                _ => return None,
            }
        }
        parts.reverse();
        Some(parts.join("."))
    }
}

fn is_known_type_name(name: &str) -> bool {
    matches!(
        name,
        "Optional"
            | "Iterable"
            | "Iterator"
            | "Sequence"
            | "AbstractSet"
            | "Mapping"
            | "Annotated"
            | "Any"
            | "Callable"
            | "TypeVar"
    )
}

fn is_type_var_call(expression: &Expr, imports: &KnownImports) -> bool {
    matches!(expression, Expr::Call(call) if imports.resolved_name(call.func.as_ref()).as_deref() == Some("typing.TypeVar"))
}

struct AnnotationCollector<'ast> {
    annotations: Vec<(&'ast Expr, Option<String>)>,
    qualname: Vec<String>,
}

impl<'ast> AnnotationCollector<'ast> {
    fn collect(module: &'ast ModModule) -> Vec<(&'ast Expr, Option<String>)> {
        let mut collector = Self {
            annotations: Vec::new(),
            qualname: Vec::new(),
        };
        for statement in &module.body {
            collector.visit_stmt(statement);
        }
        collector.annotations
    }

    fn symbol(&self) -> Option<String> {
        (!self.qualname.is_empty()).then(|| self.qualname.join("."))
    }

    fn record_function_annotations(&mut self, definition: &'ast ruff_python_ast::StmtFunctionDef) {
        for parameter in definition.parameters.iter() {
            if let Some(annotation) = parameter.annotation() {
                self.annotations.push((annotation, self.symbol()));
            }
        }
        if let Some(annotation) = definition.returns.as_deref() {
            self.annotations.push((annotation, self.symbol()));
        }
    }
}

impl<'ast> Visitor<'ast> for AnnotationCollector<'ast> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::FunctionDef(definition) => {
                self.qualname.push(definition.name.as_str().to_owned());
                self.record_function_annotations(definition);
                visitor::walk_stmt(self, statement);
                self.qualname.pop();
            }
            Stmt::ClassDef(definition) => {
                self.qualname.push(definition.name.as_str().to_owned());
                visitor::walk_stmt(self, statement);
                self.qualname.pop();
            }
            Stmt::AnnAssign(assign) => {
                self.annotations
                    .push((assign.annotation.as_ref(), self.symbol()));
                visitor::walk_stmt(self, statement);
            }
            _ => visitor::walk_stmt(self, statement),
        }
    }
}

fn type_annotation_candidates(
    module: &ModModule,
    source: &str,
    imports: &KnownImports,
    request: &AnalyzeRequest<'_>,
) -> Vec<AnalyzerCandidate> {
    AnnotationCollector::collect(module)
        .into_iter()
        .flat_map(|(annotation, symbol)| {
            annotation_replacements(annotation, source, imports)
                .into_iter()
                .filter_map(move |(replacement, operator)| {
                    let range = annotation.range();
                    let start = usize::from(range.start());
                    let end = usize::from(range.end());
                    let (line, column) = line_and_column(source, start);
                    selected(request, line, symbol.as_deref()).then(|| AnalyzerCandidate {
                        path: request.path.to_owned(),
                        span: ByteSpan {
                            start: start as u64,
                            length: (end - start) as u64,
                        },
                        original: source[start..end].to_owned(),
                        replacement,
                        operator: operator.as_str().to_owned(),
                        line,
                        column,
                        symbol: symbol.clone(),
                    })
                })
        })
        .filter(|candidate| {
            request.operators.contains(
                MutationOperator::from_name(&candidate.operator)
                    .expect("type mutation operator is configured"),
            )
        })
        .collect()
}

fn annotation_replacements(
    annotation: &Expr,
    source: &str,
    imports: &KnownImports,
) -> Vec<(String, MutationOperator)> {
    let mut replacements = Vec::new();
    if let Some(replacement) = nullable_removal(annotation, source, imports) {
        replacements.push((replacement, MutationOperator::TypeNullableRemove));
    } else if nullable_add_allowed(annotation, imports) {
        let range = annotation.range();
        replacements.push((
            format!(
                "{} | None",
                &source[usize::from(range.start())..usize::from(range.end())]
            ),
            MutationOperator::TypeNullableAdd,
        ));
    }
    if let Some((replacement, operator)) = collection_replacement(annotation, source, imports) {
        replacements.push((replacement, operator));
    }
    replacements
}

fn nullable_removal(annotation: &Expr, source: &str, imports: &KnownImports) -> Option<String> {
    if let Expr::BinOp(binary) = annotation {
        if binary.op == Operator::BitOr {
            if is_none(binary.left.as_ref()) {
                return Some(expression_source(binary.right.as_ref(), source));
            }
            if is_none(binary.right.as_ref()) {
                return Some(expression_source(binary.left.as_ref(), source));
            }
        }
    }
    let Expr::Subscript(subscript) = annotation else {
        return None;
    };
    (imports.resolved_name(subscript.value.as_ref()).as_deref() == Some("typing.Optional"))
        .then(|| expression_source(subscript.slice.as_ref(), source))
}

fn nullable_add_allowed(annotation: &Expr, imports: &KnownImports) -> bool {
    !contains_disallowed_annotation(annotation, imports)
        && !is_nullable(annotation, imports)
        && is_supported_annotation(annotation, imports)
}

fn is_supported_annotation(annotation: &Expr, imports: &KnownImports) -> bool {
    match annotation {
        Expr::Name(name) => matches!(
            name.id.as_str(),
            "str" | "int" | "float" | "bool" | "bytes" | "object"
        ),
        Expr::Subscript(subscript) => matches!(
            imports.resolved_name(subscript.value.as_ref()).as_deref(),
            Some(
                "list"
                    | "set"
                    | "dict"
                    | "typing.Iterable"
                    | "collections.abc.Iterable"
                    | "typing.Iterator"
                    | "collections.abc.Iterator"
                    | "typing.Sequence"
                    | "collections.abc.Sequence"
                    | "typing.AbstractSet"
                    | "collections.abc.AbstractSet"
                    | "typing.Mapping"
                    | "collections.abc.Mapping"
            )
        ),
        _ => false,
    }
}

fn is_nullable(annotation: &Expr, imports: &KnownImports) -> bool {
    matches!(annotation, Expr::BinOp(binary) if binary.op == Operator::BitOr && (is_none(binary.left.as_ref()) || is_none(binary.right.as_ref())))
        || matches!(annotation, Expr::Subscript(subscript) if imports.resolved_name(subscript.value.as_ref()).as_deref() == Some("typing.Optional"))
}

fn contains_disallowed_annotation(annotation: &Expr, imports: &KnownImports) -> bool {
    match annotation {
        Expr::StringLiteral(_) => true,
        Expr::Name(name) => {
            imports.type_vars.contains(name.id.as_str())
                || imports.resolved_name(annotation).as_deref() == Some("typing.Any")
        }
        Expr::Subscript(subscript) => {
            matches!(
                imports.resolved_name(subscript.value.as_ref()).as_deref(),
                Some("typing.Annotated" | "typing.Callable")
            ) || contains_disallowed_annotation(subscript.slice.as_ref(), imports)
        }
        Expr::BinOp(binary) if binary.op == Operator::BitOr => {
            is_none(binary.left.as_ref())
                || is_none(binary.right.as_ref())
                || contains_disallowed_annotation(binary.left.as_ref(), imports)
                || contains_disallowed_annotation(binary.right.as_ref(), imports)
        }
        _ => false,
    }
}

fn collection_replacement(
    annotation: &Expr,
    source: &str,
    imports: &KnownImports,
) -> Option<(String, MutationOperator)> {
    let Expr::Subscript(subscript) = annotation else {
        return None;
    };
    let resolved = imports.resolved_name(subscript.value.as_ref())?;
    let (replacement_name, operator) = match resolved.as_str() {
        "list" => ("Sequence", MutationOperator::TypeListSequence),
        "set" => ("AbstractSet", MutationOperator::TypeSetAbstractSet),
        "dict" => ("Mapping", MutationOperator::TypeMapping),
        "typing.Iterable" | "collections.abc.Iterable" => {
            ("Iterator", MutationOperator::TypeIterableIterator)
        }
        "typing.Sequence" | "collections.abc.Sequence" => {
            ("Iterable", MutationOperator::TypeSequenceIterable)
        }
        _ => return None,
    };
    Some((
        replace_subscript_base(
            annotation,
            subscript.value.as_ref(),
            source,
            replacement_name,
        ),
        operator,
    ))
}

fn replace_subscript_base(
    annotation: &Expr,
    base: &Expr,
    source: &str,
    replacement: &str,
) -> String {
    let annotation_range = annotation.range();
    let base_range = base.range();
    let start = usize::from(annotation_range.start());
    let base_start = usize::from(base_range.start());
    let base_end = usize::from(base_range.end());
    let end = usize::from(annotation_range.end());
    let base_text = &source[base_start..base_end];
    let replacement_base = base_text.rsplit_once('.').map_or_else(
        || replacement.to_owned(),
        |(prefix, _)| format!("{prefix}.{replacement}"),
    );
    format!(
        "{}{}{}",
        &source[start..base_start],
        replacement_base,
        &source[base_end..end]
    )
}

fn expression_source(expression: &Expr, source: &str) -> String {
    let range = expression.range();
    source[usize::from(range.start())..usize::from(range.end())].to_owned()
}

fn is_none(expression: &Expr) -> bool {
    matches!(expression, Expr::NoneLiteral(_))
}
