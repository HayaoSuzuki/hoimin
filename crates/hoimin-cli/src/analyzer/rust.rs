use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};
use std::ops::Range;

use camino::Utf8Path;
use hoimin_core::{
    ByteSpan, LineRange, MutationOperator, MutationOperatorSelection, MutationProfile,
};
use ruff_python_ast::identifier;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    CmpOp, Expr, ExprCall, ExprContext, ExprList, ExprSlice, ExprSubscript, ExprTuple, ModModule,
    Number, Operator, Pattern, Stmt, UnaryOp, visitor,
};
use ruff_python_parser::parse_module;
use ruff_text_size::{Ranged, TextRange};

use super::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

pub(crate) struct AnalyzeRequest<'a> {
    pub path: &'a Utf8Path,
    pub lines: &'a [LineRange],
    pub symbols: &'a [String],
    pub operators: &'a MutationOperatorSelection,
    pub profile: MutationProfile,
    pub max_candidates: usize,
}

pub(crate) struct AnalyzerOutput {
    pub candidates: Vec<AnalyzerCandidate>,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub truncated: bool,
}

pub(crate) struct ProducerPrefix {
    pub(crate) candidates: Vec<AnalyzerCandidate>,
    pub(crate) overflowed: bool,
    pub(crate) retained_peak: usize,
}

type CandidateIdentity = (u64, String, String);

struct RetainedCandidate {
    candidate: AnalyzerCandidate,
    emission_sequence: u64,
}

impl Ord for RetainedCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.candidate
            .span
            .start
            .cmp(&other.candidate.span.start)
            .then_with(|| self.candidate.operator.cmp(&other.candidate.operator))
            .then_with(|| self.emission_sequence.cmp(&other.emission_sequence))
    }
}

impl PartialOrd for RetainedCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for RetainedCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for RetainedCandidate {}

pub(crate) struct CandidatePrefix {
    entries: BinaryHeap<RetainedCandidate>,
    identities: HashSet<CandidateIdentity>,
    capacity: usize,
    overflowed: bool,
    retained_peak: usize,
    next_emission_sequence: u64,
}

impl CandidatePrefix {
    pub(crate) fn new(max_candidates: usize) -> Self {
        Self {
            entries: BinaryHeap::new(),
            identities: HashSet::new(),
            capacity: max_candidates.saturating_add(1),
            overflowed: false,
            retained_peak: 0,
            next_emission_sequence: 0,
        }
    }

    pub(crate) fn push(&mut self, candidate: AnalyzerCandidate) {
        let identity = candidate_identity(&candidate);
        if self.identities.contains(&identity) {
            return;
        }
        let entry = RetainedCandidate {
            candidate,
            emission_sequence: self.next_emission_sequence,
        };
        self.next_emission_sequence = self.next_emission_sequence.saturating_add(1);

        if self.entries.len() < self.capacity {
            self.identities.insert(identity);
            self.entries.push(entry);
            self.retained_peak = self.retained_peak.max(self.entries.len());
            return;
        }

        self.overflowed = true;
        if self.entries.peek().is_some_and(|latest| entry < *latest) {
            let evicted = self.entries.pop().expect("a peeked heap entry exists");
            self.identities
                .remove(&candidate_identity(&evicted.candidate));
            self.identities.insert(identity);
            self.entries.push(entry);
        }
    }

    pub(crate) fn finish(self) -> ProducerPrefix {
        ProducerPrefix {
            candidates: self
                .entries
                .into_sorted_vec()
                .into_iter()
                .map(|entry| entry.candidate)
                .collect(),
            overflowed: self.overflowed,
            retained_peak: self.retained_peak,
        }
    }
}

fn candidate_identity(candidate: &AnalyzerCandidate) -> CandidateIdentity {
    (
        candidate.span.start,
        candidate.replacement.clone(),
        candidate.operator.clone(),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AnalysisCancelled;

pub(crate) fn analyze_source(request: &AnalyzeRequest<'_>, source: &str) -> AnalyzerOutput {
    analyze_source_cancellable(request, source, || false)
        .expect("the non-cancellable analyzer probe never cancels")
}

#[expect(
    clippy::too_many_lines,
    reason = "token and type-annotation candidates share local byte-span and source-order control flow"
)]
pub(crate) fn analyze_source_cancellable(
    request: &AnalyzeRequest<'_>,
    source: &str,
    cancelled: impl Fn() -> bool,
) -> Result<AnalyzerOutput, AnalysisCancelled> {
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    let Ok(parsed) = parse_module(source) else {
        return Ok(invalid_syntax(request.path));
    };
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    let line_index = LineIndex::new(source);
    let mut candidates = Vec::new();
    let tokens: Vec<_> = parsed.tokens().iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        if matches!(
            token.kind(),
            TokenKind::FStringMiddle | TokenKind::TStringMiddle
        ) {
            continue;
        }
        let range = token.range();
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let text = &source[start..end];
        if !facts.is_operator_token(start) {
            continue;
        }
        if facts.contains_annotation_span(range) && matches!(text, "&" | "|" | "<<" | ">>") {
            continue;
        }
        let previous = tokens[..index]
            .iter()
            .rev()
            .copied()
            .find(|token| !token.kind().is_trivia());
        let next = tokens[index + 1..]
            .iter()
            .copied()
            .find(|token| !token.kind().is_trivia());
        let trivia_before_next = tokens
            .get(index + 1)
            .is_some_and(|token| token.kind().is_trivia());
        let (span_end, replacement, operator) = if text == "not"
            && next.is_some_and(|next| {
                &source[usize::from(next.range().start())..usize::from(next.range().end())] == "in"
            }) {
            let next_end = usize::from(next.unwrap().range().end());
            (
                next_end,
                if trivia_before_next {
                    source[end..next_end].to_owned()
                } else {
                    "in".to_owned()
                },
                "membership",
            )
        } else if text == "is"
            && next.is_some_and(|next| {
                &source[usize::from(next.range().start())..usize::from(next.range().end())] == "not"
            })
        {
            let next = next.unwrap();
            (
                usize::from(next.range().end()),
                if trivia_before_next {
                    source[start..usize::from(next.range().start())].to_owned()
                } else {
                    "is".to_owned()
                },
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
        let symbol = facts.scope_at(start);
        let operator = MutationOperator::from_name(operator)
            .expect("token mutation operator must be configured");
        if let Some(candidate) = make_candidate(
            request,
            source,
            &line_index,
            start..span_end,
            replacement,
            operator,
            symbol,
        ) {
            candidates.push(candidate);
        }
    }
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    candidates.extend(ast_candidates(
        parsed.syntax(),
        source,
        &line_index,
        &facts,
        request,
        &cancelled,
    )?);
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    candidates.extend(type_annotation_candidates(
        parsed.syntax(),
        source,
        &line_index,
        &facts.imports,
        request,
    ));
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    if request.profile == MutationProfile::Focused {
        candidates.retain(|candidate| {
            if candidate.operator.starts_with("type_") {
                return true;
            }
            let Some(end) = candidate.span.start.checked_add(candidate.span.length) else {
                return true;
            };
            let (Ok(start), Ok(end)) =
                (usize::try_from(candidate.span.start), usize::try_from(end))
            else {
                return true;
            };
            !facts.contains_arid_span(start, end)
        });
    }
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
    Ok(AnalyzerOutput {
        candidates,
        diagnostics,
        truncated,
    })
}

fn make_candidate(
    request: &AnalyzeRequest<'_>,
    source: &str,
    line_index: &LineIndex,
    range: Range<usize>,
    replacement: String,
    operator: MutationOperator,
    symbol: Option<String>,
) -> Option<AnalyzerCandidate> {
    if range.start >= range.end || range.end > source.len() {
        return None;
    }
    let original = source.get(range.clone())?.to_owned();
    let start = u64::try_from(range.start).ok()?;
    let length = u64::try_from(range.len()).ok()?;
    let (line, column) = line_index.line_and_column(source, range.start);
    if !selected(request, line, symbol.as_deref()) || !request.operators.contains(operator) {
        return None;
    }
    Some(AnalyzerCandidate {
        path: request.path.to_owned(),
        span: ByteSpan { start, length },
        original,
        replacement,
        operator: operator.as_str().to_owned(),
        line,
        column,
        symbol,
    })
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
        "&" => ("|", "bitwise_and_or"),
        "|" => ("&", "bitwise_and_or"),
        "<<" => (">>", "bitwise_shift"),
        ">>" => ("<<", "bitwise_shift"),
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

struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        Self { starts }
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "Ruff TextSize offsets cap parsed source at u32::MAX bytes, and code-point counts cannot exceed byte counts."
    )]
    fn line_and_column(&self, source: &str, offset: usize) -> (u32, u32) {
        let line_index = self.starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.starts[line_index];
        let line = line_index as u32 + 1;
        let column = source[line_start..offset].chars().count() as u32;
        (line, column)
    }
}

fn selected(request: &AnalyzeRequest<'_>, line: u32, symbol: Option<&str>) -> bool {
    let line_selected = request.lines.is_empty()
        || request
            .lines
            .iter()
            .any(|range| range.start <= line && line <= range.end);
    let symbol_selected = request.symbols.is_empty()
        || request.symbols.iter().any(|selector| {
            let Some((module, qualname)) = selector.rsplit_once(':') else {
                return false;
            };
            let current_module = module_name(request.path);
            (current_module == module || current_module.ends_with(&format!(".{module}")))
                && symbol.is_some_and(|symbol| {
                    symbol == qualname || symbol.starts_with(&format!("{qualname}."))
                })
        });
    line_selected && symbol_selected
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
    bound_builtin_names: HashSet<String>,
    bound_exception_names: HashSet<String>,
    operator_token_starts: HashSet<usize>,
    unary_sign_starts: HashSet<usize>,
    not_operands: Vec<(usize, usize, usize)>,
    arid_ranges: Vec<(usize, usize)>,
    annotation_ranges: Vec<(usize, usize)>,
    scopes: Vec<ScopeRange>,
    qualname: Vec<String>,
    tokens: Option<&'tokens ruff_python_ast::token::Tokens>,
    source: &'tokens str,
}
struct ScopeRange {
    start: usize,
    end: usize,
    symbol: String,
}

impl<'tokens> AstFacts<'tokens> {
    fn from_module(
        module: &ModModule,
        tokens: &'tokens ruff_python_ast::token::Tokens,
        source: &'tokens str,
    ) -> Self {
        let mut facts = Self {
            imports: KnownImports::from_module(module),
            tokens: Some(tokens),
            source,
            ..Self::default()
        };
        for statement in &module.body {
            facts.visit_stmt(statement);
        }
        facts.normalize_arid_ranges();
        facts
    }

    fn record_arid_range(&mut self, range: ruff_text_size::TextRange) {
        self.arid_ranges
            .push((usize::from(range.start()), usize::from(range.end())));
    }

    fn normalize_arid_ranges(&mut self) {
        self.arid_ranges.sort_unstable_by_key(|range| range.0);
        let mut merged = Vec::with_capacity(self.arid_ranges.len());
        for (start, end) in self.arid_ranges.drain(..) {
            if let Some((_, previous_end)) = merged.last_mut()
                && start <= *previous_end
            {
                *previous_end = (*previous_end).max(end);
            } else {
                merged.push((start, end));
            }
        }
        self.arid_ranges = merged;
    }

    fn contains_arid_span(&self, start: usize, end: usize) -> bool {
        self.arid_ranges
            .iter()
            .any(|(range_start, range_end)| *range_start <= start && end <= *range_end)
    }

    fn record_annotation_range(&mut self, range: TextRange) {
        self.annotation_ranges
            .push((usize::from(range.start()), usize::from(range.end())));
    }

    fn contains_annotation_span(&self, range: TextRange) -> bool {
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        self.annotation_ranges
            .iter()
            .any(|(range_start, range_end)| *range_start <= start && end <= *range_end)
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

    fn is_operator_token(&self, start: usize) -> bool {
        self.operator_token_starts.contains(&start)
    }

    fn record_operator_tokens(&mut self, range: TextRange, spellings: &[&str]) {
        let source = self.source;
        let starts = self
            .tokens
            .expect("parser tokens are set")
            .in_range(range)
            .iter()
            .filter_map(|token| {
                let range = token.range();
                let start = usize::from(range.start());
                let end = usize::from(range.end());
                let text = &source[start..end];
                spellings.contains(&text).then_some(start)
            })
            .collect::<Vec<_>>();
        self.operator_token_starts.extend(starts);
    }

    fn is_builtin_bound(&self, name: &str) -> bool {
        self.bound_builtin_names.contains(name)
    }

    fn is_exception_bound(&self, name: &str) -> bool {
        self.bound_exception_names.contains(name)
    }

    fn record_builtin_name(&mut self, name: &str) {
        if MUTABLE_BUILTINS.contains(&name) {
            self.bound_builtin_names.insert(name.to_owned());
        }
        self.record_exception_name(name);
    }

    fn record_exception_name(&mut self, name: &str) {
        if EXCEPTION_NAMES.contains(&name) {
            self.bound_exception_names.insert(name.to_owned());
        }
    }

    fn record_builtin_target(&mut self, expression: &Expr) {
        match expression {
            Expr::Name(name) => self.record_builtin_name(name.id.as_str()),
            Expr::List(list) => {
                for element in &list.elts {
                    self.record_builtin_target(element);
                }
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.record_builtin_target(element);
                }
            }
            Expr::Starred(starred) => self.record_builtin_target(starred.value.as_ref()),
            _ => {}
        }
    }

    fn record_import_alias(&mut self, alias: &ruff_python_ast::Alias, from_import: bool) {
        let local = alias.asname.as_ref().map_or_else(
            || {
                if from_import {
                    alias.name.as_str()
                } else {
                    alias.name.as_str().split('.').next().unwrap_or_default()
                }
            },
            ruff_python_ast::Identifier::as_str,
        );
        if local == "*" {
            self.bound_builtin_names
                .extend(MUTABLE_BUILTINS.iter().map(|name| (*name).to_owned()));
            self.bound_exception_names
                .extend(EXCEPTION_NAMES.iter().map(|name| (*name).to_owned()));
        } else {
            self.record_builtin_name(local);
        }
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

    fn record_function_annotation_ranges(&mut self, definition: &ruff_python_ast::StmtFunctionDef) {
        for parameter in &definition.parameters {
            if let Some(annotation) = parameter.annotation() {
                self.record_annotation_range(annotation.range());
            }
        }
        if let Some(annotation) = definition.returns.as_deref() {
            self.record_annotation_range(annotation.range());
        }
    }
}

impl<'ast> Visitor<'ast> for AstFacts<'_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::Import(import) => {
                for alias in &import.names {
                    self.record_import_alias(alias, false);
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    self.record_import_alias(alias, true);
                }
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.record_builtin_target(target);
                }
            }
            Stmt::AugAssign(assign) => {
                self.record_builtin_target(assign.target.as_ref());
                self.record_operator_tokens(
                    TextRange::new(assign.target.range().end(), assign.value.range().start()),
                    &["+=", "-="],
                );
            }
            Stmt::AnnAssign(assign) => {
                self.record_builtin_target(assign.target.as_ref());
                self.record_annotation_range(assign.annotation.range());
            }
            Stmt::For(statement_for) => self.record_builtin_target(statement_for.target.as_ref()),
            Stmt::With(statement_with) => {
                for item in &statement_with.items {
                    if let Some(target) = &item.optional_vars {
                        self.record_builtin_target(target);
                    }
                }
            }
            Stmt::FunctionDef(definition) => {
                self.record_builtin_name(definition.name.as_str());
                self.record_function_annotation_ranges(definition);
            }
            Stmt::ClassDef(definition) => self.record_builtin_name(definition.name.as_str()),
            Stmt::TypeAlias(alias) => {
                self.record_builtin_target(alias.name.as_ref());
                self.record_annotation_range(alias.value.range());
            }
            Stmt::Break(statement_break) => {
                self.record_operator_tokens(statement_break.range(), &["break"]);
            }
            Stmt::Continue(statement_continue) => {
                self.record_operator_tokens(statement_continue.range(), &["continue"]);
            }
            _ => {}
        }
        match statement {
            Stmt::FunctionDef(definition) => {
                for parameter in definition.parameters.iter_non_variadic_params() {
                    if let Some(default) = parameter.default() {
                        self.record_arid_range(default.range());
                    }
                }
                self.visit_definition(
                    definition.name.as_str(),
                    definition.range(),
                    &definition.decorator_list,
                    statement,
                );
            }
            Stmt::If(statement_if) if is_main_guard(statement_if.test.as_ref()) => {
                self.record_arid_range(statement_if.test.range());
                for child in &statement_if.body {
                    self.record_arid_range(child.range());
                }
                visitor::walk_stmt(self, statement);
            }
            Stmt::Assert(_) => {
                self.record_arid_range(statement.range());
                visitor::walk_stmt(self, statement);
            }
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
        if let Expr::Named(named) = expression {
            self.record_builtin_target(named.target.as_ref());
        }
        if let Expr::Call(call) = expression
            && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "print")
        {
            self.record_arid_range(call.range());
        }
        match expression {
            Expr::Compare(compare) => {
                let mut preceding_range = compare.left.range();
                for comparator in &compare.comparators {
                    let comparator_range = comparator.range();
                    self.record_operator_tokens(
                        TextRange::new(preceding_range.end(), comparator_range.start()),
                        &["==", "!=", "<", "<=", ">", ">=", "in", "not", "is"],
                    );
                    preceding_range = comparator_range;
                }
            }
            Expr::BoolOp(boolean) => {
                for values in boolean.values.windows(2) {
                    self.record_operator_tokens(
                        TextRange::new(values[0].range().end(), values[1].range().start()),
                        &["and", "or"],
                    );
                }
            }
            Expr::BinOp(binary) => self.record_operator_tokens(
                TextRange::new(binary.left.range().end(), binary.right.range().start()),
                &["+", "-", "*", "/", "//", "%", "&", "|", "<<", ">>"],
            ),
            Expr::UnaryOp(unary) => {
                self.record_operator_tokens(
                    TextRange::new(unary.range().start(), unary.operand.range().start()),
                    &["not", "+", "-"],
                );
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
            Expr::BooleanLiteral(boolean) => {
                self.record_operator_tokens(boolean.range(), &["True", "False"]);
            }
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }

    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        match pattern {
            Pattern::MatchMapping(mapping) => {
                if let Some(rest) = &mapping.rest {
                    self.record_builtin_name(rest.as_str());
                }
            }
            Pattern::MatchStar(star) => {
                if let Some(name) = &star.name {
                    self.record_builtin_name(name.as_str());
                }
            }
            Pattern::MatchAs(as_pattern) => {
                if let Some(name) = &as_pattern.name {
                    self.record_builtin_name(name.as_str());
                }
            }
            _ => {}
        }
        visitor::walk_pattern(self, pattern);
    }

    fn visit_parameter(&mut self, parameter: &'ast ruff_python_ast::Parameter) {
        self.record_builtin_name(parameter.name().as_str());
        visitor::walk_parameter(self, parameter);
    }

    fn visit_except_handler(&mut self, except_handler: &'ast ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
        if let Some(name) = &handler.name {
            self.record_builtin_name(name.as_str());
        }
        visitor::walk_except_handler(self, except_handler);
    }

    fn visit_comprehension(&mut self, comprehension: &'ast ruff_python_ast::Comprehension) {
        self.record_builtin_target(&comprehension.target);
        visitor::walk_comprehension(self, comprehension);
    }
}

const MUTABLE_BUILTINS: &[&str] = &[
    "any",
    "all",
    "list",
    "tuple",
    "set",
    "frozenset",
    "min",
    "max",
    "sorted",
    "reversed",
];

const EXCEPTION_NAMES: &[&str] = &[
    "ValueError",
    "TypeError",
    "KeyError",
    "IndexError",
    "AttributeError",
    "FileNotFoundError",
    "PermissionError",
    "ConnectionError",
    "TimeoutError",
    "ImportError",
    "ModuleNotFoundError",
    "ZeroDivisionError",
    "OverflowError",
    "Exception",
    "BaseException",
    "SystemExit",
    "KeyboardInterrupt",
    "GeneratorExit",
];

const NO_EXCEPTION_REPLACEMENTS: &[&str] = &[];
const VALUE_TYPE_REPLACEMENTS: &[&str] = &["TypeError"];
const TYPE_VALUE_REPLACEMENTS: &[&str] = &["ValueError"];
const KEY_REPLACEMENTS: &[&str] = &["IndexError", "AttributeError"];
const INDEX_REPLACEMENTS: &[&str] = &["KeyError"];
const ATTRIBUTE_REPLACEMENTS: &[&str] = &["KeyError"];
const FILE_NOT_FOUND_REPLACEMENTS: &[&str] = &["PermissionError"];
const PERMISSION_REPLACEMENTS: &[&str] = &["FileNotFoundError"];
const CONNECTION_REPLACEMENTS: &[&str] = &["TimeoutError"];
const TIMEOUT_REPLACEMENTS: &[&str] = &["ConnectionError"];
const IMPORT_REPLACEMENTS: &[&str] = &["ModuleNotFoundError"];
const MODULE_NOT_FOUND_REPLACEMENTS: &[&str] = &["ImportError"];
const ZERO_DIVISION_REPLACEMENTS: &[&str] = &["OverflowError"];
const OVERFLOW_REPLACEMENTS: &[&str] = &["ZeroDivisionError"];

fn exception_pair_replacements(name: &str) -> &'static [&'static str] {
    match name {
        "ValueError" => VALUE_TYPE_REPLACEMENTS,
        "TypeError" => TYPE_VALUE_REPLACEMENTS,
        "KeyError" => KEY_REPLACEMENTS,
        "IndexError" => INDEX_REPLACEMENTS,
        "AttributeError" => ATTRIBUTE_REPLACEMENTS,
        "FileNotFoundError" => FILE_NOT_FOUND_REPLACEMENTS,
        "PermissionError" => PERMISSION_REPLACEMENTS,
        "ConnectionError" => CONNECTION_REPLACEMENTS,
        "TimeoutError" => TIMEOUT_REPLACEMENTS,
        "ImportError" => IMPORT_REPLACEMENTS,
        "ModuleNotFoundError" => MODULE_NOT_FOUND_REPLACEMENTS,
        "ZeroDivisionError" => ZERO_DIVISION_REPLACEMENTS,
        "OverflowError" => OVERFLOW_REPLACEMENTS,
        _ => NO_EXCEPTION_REPLACEMENTS,
    }
}

struct AstCandidateCollector<'a, F> {
    source: &'a str,
    line_index: &'a LineIndex,
    facts: &'a AstFacts<'a>,
    request: &'a AnalyzeRequest<'a>,
    cancelled: &'a F,
    cancelled_observed: bool,
    exception_handler_finality: Vec<bool>,
    candidates: Vec<AnalyzerCandidate>,
}

impl<'a, F: Fn() -> bool> AstCandidateCollector<'a, F> {
    fn collect(
        module: &'a ModModule,
        source: &'a str,
        line_index: &'a LineIndex,
        facts: &'a AstFacts<'a>,
        request: &'a AnalyzeRequest<'a>,
        cancelled: &'a F,
    ) -> Result<Vec<AnalyzerCandidate>, AnalysisCancelled> {
        let mut collector = Self {
            source,
            line_index,
            facts,
            request,
            cancelled,
            cancelled_observed: false,
            exception_handler_finality: Vec::new(),
            candidates: Vec::new(),
        };
        for statement in &module.body {
            collector.visit_stmt(statement);
            if collector.cancelled_observed {
                return Err(AnalysisCancelled);
            }
        }
        Ok(collector.candidates)
    }

    fn check_cancelled(&mut self) -> bool {
        if !self.cancelled_observed && (self.cancelled)() {
            self.cancelled_observed = true;
        }
        self.cancelled_observed
    }

    fn add_candidate(&mut self, range: TextRange, replacement: String, operator: MutationOperator) {
        let range = byte_range(range);
        let symbol = self.facts.scope_at(range.start);
        if let Some(candidate) = make_candidate(
            self.request,
            self.source,
            self.line_index,
            range,
            replacement,
            operator,
            symbol,
        ) {
            self.candidates.push(candidate);
        }
    }

    fn collect_call(&mut self, call: &ExprCall) {
        if let Expr::Name(name) = call.func.as_ref() {
            self.collect_builtin_call(call, name.id.as_str(), name.range());
        }
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            self.collect_method_call(call, attribute.attr.as_str(), attribute.attr.range());
        }
    }

    fn collect_builtin_call(&mut self, call: &ExprCall, name: &str, range: TextRange) {
        if self.facts.is_builtin_bound(name) {
            return;
        }
        let (replacement, operator) = match name {
            "any" | "all" if has_exact_positional_arguments(call, 1) => (
                if name == "any" { "all" } else { "any" },
                MutationOperator::CollectionAnyAll,
            ),
            "list" | "tuple" if has_at_most_one_positional_argument(call) => (
                if name == "list" { "tuple" } else { "list" },
                MutationOperator::CollectionListTuple,
            ),
            "set" | "frozenset" if has_at_most_one_positional_argument(call) => (
                if name == "set" { "frozenset" } else { "set" },
                MutationOperator::CollectionSetFrozenset,
            ),
            "min" | "max" if has_supported_same_contract_arguments(call) => (
                if name == "min" { "max" } else { "min" },
                MutationOperator::CollectionMinMax,
            ),
            "sorted" | "reversed"
                if has_exact_positional_arguments(call, 1)
                    && !self.facts.contains_annotation_span(call.range()) =>
            {
                (
                    if name == "sorted" {
                        "reversed"
                    } else {
                        "sorted"
                    },
                    MutationOperator::StructureSortedReversed,
                )
            }
            _ => return,
        };
        self.add_candidate(range, replacement.to_owned(), operator);
    }

    fn collect_method_call(&mut self, call: &ExprCall, name: &str, range: TextRange) {
        if !self.facts.contains_annotation_span(call.range()) {
            self.collect_structural_method_call(call, name);
        }
        let same_contract = has_supported_same_contract_arguments(call);
        let exact_one = has_exact_positional_arguments(call, 1);
        match name {
            "append" if has_supported_append_insert_arguments(call) => {
                if let Some(replacement) = append_to_insert_replacement(self.source, call) {
                    self.add_candidate(
                        call.range(),
                        replacement,
                        MutationOperator::CollectionAppendInsert,
                    );
                }
            }
            "insert"
                if has_exact_positional_arguments(call, 2)
                    && is_zero_literal(&call.arguments.args[0]) =>
            {
                if let Some(replacement) = insert_to_append_replacement(self.source, call) {
                    self.add_candidate(
                        call.range(),
                        replacement,
                        MutationOperator::CollectionAppendInsert,
                    );
                }
            }
            "add" if exact_one => self.add_candidate(
                range,
                "discard".to_owned(),
                MutationOperator::CollectionSetAddDiscard,
            ),
            "discard" if exact_one => {
                self.add_candidate(
                    range,
                    "add".to_owned(),
                    MutationOperator::CollectionSetAddDiscard,
                );
                self.add_candidate(
                    range,
                    "remove".to_owned(),
                    MutationOperator::CollectionSetRemoveDiscard,
                );
            }
            "remove" if exact_one => self.add_candidate(
                range,
                "discard".to_owned(),
                MutationOperator::CollectionSetRemoveDiscard,
            ),
            "startswith" | "endswith" if same_contract => self.add_candidate(
                range,
                if name == "startswith" {
                    "endswith"
                } else {
                    "startswith"
                }
                .to_owned(),
                MutationOperator::CollectionStringStartsEnds,
            ),
            "split" | "rsplit" if same_contract => self.add_candidate(
                range,
                if name == "split" { "rsplit" } else { "split" }.to_owned(),
                MutationOperator::CollectionStringSplitRsplit,
            ),
            _ => {}
        }
    }

    fn collect_structural_method_call(&mut self, call: &ExprCall, name: &str) {
        let exact_one = has_exact_positional_arguments(call, 1);
        match name {
            "append" if has_supported_append_insert_arguments(call) => {
                if let Some(replacement) = append_to_extend_replacement(self.source, call) {
                    self.add_candidate(
                        call.range(),
                        replacement,
                        MutationOperator::StructureAppendExtend,
                    );
                }
            }
            "extend" if exact_one => {
                if let Some(replacement) = extend_to_append_replacement(self.source, call) {
                    self.add_candidate(
                        call.range(),
                        replacement,
                        MutationOperator::StructureAppendExtend,
                    );
                }
            }
            "get"
                if exact_one
                    && !self.has_trailing_argument_comma(call)
                    && is_supported_mapping_key(&call.arguments.args[0])
                    && matches!(call.func.as_ref(), Expr::Attribute(attribute) if is_simple_receiver(attribute.value.as_ref())) =>
            {
                if let Some(replacement) = mapping_get_to_subscript_replacement(self.source, call) {
                    self.add_candidate(
                        call.range(),
                        replacement,
                        MutationOperator::StructureMappingGetSubscript,
                    );
                }
            }
            "sort" | "reverse" if has_exact_positional_arguments(call, 0) => {
                if let Some(replacement) = renamed_method_call_replacement(
                    self.source,
                    call,
                    if name == "sort" { "reverse" } else { "sort" },
                ) {
                    self.add_candidate(
                        call.range(),
                        replacement,
                        MutationOperator::StructureSortReverse,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_subscript(&mut self, subscript: &ExprSubscript) {
        if self.facts.contains_annotation_span(subscript.range())
            || subscript.ctx != ExprContext::Load
        {
            return;
        }
        match subscript.slice.as_ref() {
            Expr::Slice(slice) => self.collect_slice_neighbors(slice),
            expression => self.collect_index_neighbors(expression),
        }
        if !is_simple_receiver(subscript.value.as_ref())
            || !is_supported_mapping_key(subscript.slice.as_ref())
        {
            return;
        }
        if let Some(replacement) = subscript_to_mapping_get_replacement(self.source, subscript) {
            self.add_candidate(
                subscript.range(),
                replacement,
                MutationOperator::StructureMappingGetSubscript,
            );
        }
    }

    fn collect_index_neighbors(&mut self, expression: &Expr) {
        for replacement in decimal_literal_neighbors(self.source, expression, false) {
            self.add_candidate(
                expression.range(),
                replacement,
                MutationOperator::StructureIndexNeighbor,
            );
        }
    }

    fn collect_slice_neighbors(&mut self, slice: &ExprSlice) {
        for (bound, step) in [
            (slice.lower.as_deref(), false),
            (slice.upper.as_deref(), false),
            (slice.step.as_deref(), true),
        ] {
            let Some(bound) = bound else {
                continue;
            };
            for replacement in decimal_literal_neighbors(self.source, bound, step) {
                self.add_candidate(
                    bound.range(),
                    replacement,
                    MutationOperator::StructureSliceNeighbor,
                );
            }
        }
    }

    fn has_trailing_argument_comma(&self, call: &ExprCall) -> bool {
        let inner_range = call.arguments.inner_range();
        self.facts
            .tokens
            .expect("parser tokens are set")
            .iter()
            .rfind(|token| {
                let range = token.range();
                inner_range.start() <= range.start()
                    && range.end() <= inner_range.end()
                    && !token.kind().is_trivia()
            })
            .is_some_and(|token| token.kind() == TokenKind::Comma)
    }

    fn collect_list_literal(&mut self, list: &ExprList) {
        if list.ctx != ExprContext::Load || self.facts.contains_annotation_span(list.range()) {
            return;
        }
        if let Some(replacement) = list_to_tuple_replacement(
            self.source,
            list,
            self.facts.tokens.expect("parser tokens are set"),
        ) {
            self.add_candidate(
                list.range(),
                replacement,
                MutationOperator::CollectionListTuple,
            );
        }
    }

    fn collect_tuple_literal(&mut self, tuple: &ExprTuple) {
        if tuple.ctx != ExprContext::Load || self.facts.contains_annotation_span(tuple.range()) {
            return;
        }
        if let Some(replacement) = tuple_to_list_replacement(self.source, tuple) {
            self.add_candidate(
                tuple.range(),
                replacement,
                MutationOperator::CollectionListTuple,
            );
        }
    }

    fn collect_exception_handler(&mut self, except_handler: &ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
        self.collect_risky_exception_handler(except_handler, handler);
        let Some(Expr::Name(name)) = handler.type_.as_deref() else {
            return;
        };
        if self.facts.is_exception_bound(name.id.as_str()) {
            return;
        }
        for replacement in exception_pair_replacements(name.id.as_str()) {
            if !self.facts.is_exception_bound(replacement) {
                self.add_candidate(
                    name.range(),
                    (*replacement).to_owned(),
                    MutationOperator::ExceptionTypePair,
                );
            }
        }
    }

    fn collect_risky_exception_handler(
        &mut self,
        except_handler: &ruff_python_ast::ExceptHandler,
        handler: &ruff_python_ast::ExceptHandlerExceptHandler,
    ) {
        if handler.type_.is_none() {
            if self
                .request
                .operators
                .contains(MutationOperator::ExceptionBareToException)
                && !self.facts.is_exception_bound("Exception")
            {
                self.add_candidate(
                    identifier::except(except_handler, self.source),
                    "except Exception".to_owned(),
                    MutationOperator::ExceptionBareToException,
                );
            }
            return;
        }

        let Some(type_) = handler.type_.as_deref() else {
            return;
        };
        match type_ {
            Expr::Name(name) => {
                if self.facts.is_exception_bound(name.id.as_str()) {
                    return;
                }
                if name.id.as_str() == "Exception"
                    && handler.name.is_none()
                    && self
                        .exception_handler_finality
                        .last()
                        .copied()
                        .unwrap_or(false)
                    && self
                        .request
                        .operators
                        .contains(MutationOperator::ExceptionExceptionToBare)
                {
                    self.add_candidate(
                        name.range(),
                        String::new(),
                        MutationOperator::ExceptionExceptionToBare,
                    );
                }
                if let Some(replacement) = base_exception_boundary_replacement(name.id.as_str())
                    && !self.facts.is_exception_bound(replacement)
                    && self
                        .request
                        .operators
                        .contains(MutationOperator::ExceptionBaseBoundary)
                {
                    self.add_candidate(
                        name.range(),
                        replacement.to_owned(),
                        MutationOperator::ExceptionBaseBoundary,
                    );
                }
            }
            Expr::Tuple(tuple) => self.collect_tuple_exception_handler(tuple),
            _ => {}
        }
    }

    fn collect_tuple_exception_handler(&mut self, tuple: &ExprTuple) {
        let Some(names) = supported_exception_tuple_names(tuple, self.facts) else {
            return;
        };
        let tokens = self.facts.tokens.expect("parser tokens are set");
        if self
            .request
            .operators
            .contains(MutationOperator::ExceptionTupleAddPair)
        {
            let mut missing = HashSet::new();
            for name in &names {
                for replacement in exception_pair_replacements(name) {
                    if !names.iter().any(|member| member == replacement)
                        && !self.facts.is_exception_bound(replacement)
                        && missing.insert(*replacement)
                        && let Some(tuple_replacement) =
                            tuple_add_replacement(self.source, tuple, tokens, replacement)
                    {
                        self.add_candidate(
                            tuple.range(),
                            tuple_replacement,
                            MutationOperator::ExceptionTupleAddPair,
                        );
                    }
                }
            }
        }
        if names.len() >= 2
            && self
                .request
                .operators
                .contains(MutationOperator::ExceptionTupleRemoveMember)
        {
            for index in 0..names.len() {
                if names.len() == 2 && is_termination_exception(names[1 - index]) {
                    continue;
                }
                if let Some(tuple_replacement) =
                    tuple_remove_replacement(self.source, tuple, tokens, index)
                {
                    self.add_candidate(
                        tuple.range(),
                        tuple_replacement,
                        MutationOperator::ExceptionTupleRemoveMember,
                    );
                }
            }
        }
    }
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for AstCandidateCollector<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if !self.check_cancelled() {
            let Stmt::Try(try_statement) = statement else {
                visitor::walk_stmt(self, statement);
                return;
            };
            if try_statement.is_star {
                self.visit_body(&try_statement.body);
                for except_handler in &try_statement.handlers {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
                    if let Some(type_) = &handler.type_ {
                        self.visit_expr(type_);
                    }
                    self.visit_body(&handler.body);
                }
                self.visit_body(&try_statement.orelse);
                self.visit_body(&try_statement.finalbody);
                return;
            }
            self.visit_body(&try_statement.body);
            for (index, except_handler) in try_statement.handlers.iter().enumerate() {
                if self.check_cancelled() {
                    return;
                }
                self.exception_handler_finality
                    .push(index + 1 == try_statement.handlers.len());
                self.visit_except_handler(except_handler);
                self.exception_handler_finality.pop();
            }
            self.visit_body(&try_statement.orelse);
            self.visit_body(&try_statement.finalbody);
        }
    }

    fn visit_except_handler(&mut self, except_handler: &'ast ruff_python_ast::ExceptHandler) {
        if !self.check_cancelled() {
            self.collect_exception_handler(except_handler);
        }
        if !self.check_cancelled() {
            visitor::walk_except_handler(self, except_handler);
        }
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if !self.check_cancelled() && !self.facts.contains_annotation_span(expression.range()) {
            match expression {
                Expr::Call(call) => self.collect_call(call),
                Expr::Subscript(subscript) => self.collect_subscript(subscript),
                Expr::List(list) => self.collect_list_literal(list),
                Expr::Tuple(tuple) => self.collect_tuple_literal(tuple),
                _ => {}
            }
            visitor::walk_expr(self, expression);
        }
    }
}

fn ast_candidates<'a, F: Fn() -> bool>(
    module: &'a ModModule,
    source: &'a str,
    line_index: &'a LineIndex,
    facts: &'a AstFacts<'a>,
    request: &'a AnalyzeRequest<'a>,
    cancelled: &'a F,
) -> Result<Vec<AnalyzerCandidate>, AnalysisCancelled> {
    AstCandidateCollector::collect(module, source, line_index, facts, request, cancelled)
}

fn byte_range(range: TextRange) -> Range<usize> {
    usize::from(range.start())..usize::from(range.end())
}

fn source_text(source: &str, range: TextRange) -> Option<&str> {
    source.get(byte_range(range))
}

fn has_exact_positional_arguments(call: &ExprCall, count: usize) -> bool {
    call.arguments.args.len() == count
        && call.arguments.keywords.is_empty()
        && !call.arguments.args.iter().any(Expr::is_starred_expr)
}

fn has_supported_append_insert_arguments(call: &ExprCall) -> bool {
    has_exact_positional_arguments(call, 1)
        && !matches!(call.arguments.args.first(), Some(Expr::Generator(_)))
}

fn has_at_most_one_positional_argument(call: &ExprCall) -> bool {
    call.arguments.args.len() <= 1
        && call.arguments.keywords.is_empty()
        && !call.arguments.args.iter().any(Expr::is_starred_expr)
}

fn has_supported_same_contract_arguments(call: &ExprCall) -> bool {
    !call.arguments.args.iter().any(Expr::is_starred_expr)
        && call
            .arguments
            .keywords
            .iter()
            .all(|keyword| keyword.arg.is_some())
}

fn is_zero_literal(expression: &Expr) -> bool {
    matches!(expression, Expr::NumberLiteral(number) if matches!(&number.value, Number::Int(value) if value.as_usize() == Some(0)))
}

fn decimal_literal_neighbors(source: &str, expression: &Expr, excludes_zero: bool) -> Vec<String> {
    let Some(value) = decimal_literal_value(source, expression) else {
        return Vec::new();
    };
    let mut replacements = Vec::with_capacity(2);
    if let Some(next) = value.checked_add(1) {
        replacements.push(next.to_string());
    }
    if let Some(previous) = value
        .checked_sub(1)
        .filter(|previous| !excludes_zero || *previous != 0)
    {
        replacements.push(previous.to_string());
    }
    replacements
}

fn decimal_literal_value(source: &str, expression: &Expr) -> Option<u64> {
    if !matches!(expression, Expr::NumberLiteral(_)) {
        return None;
    }
    let literal = source_text(source, expression.range())?;
    if literal.is_empty() || !literal.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    literal.parse().ok()
}

fn append_to_insert_replacement(source: &str, call: &ExprCall) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let function_start = usize::from(call.func.range().start());
    let attribute_start = usize::from(attribute.attr.range().start());
    let prefix = source.get(function_start..attribute_start)?;
    let arguments = source_text(source, call.arguments.inner_range())?;
    Some(format!("{prefix}insert(0, {arguments})"))
}

fn insert_to_append_replacement(source: &str, call: &ExprCall) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let function_start = usize::from(call.func.range().start());
    let attribute_start = usize::from(attribute.attr.range().start());
    let prefix = source.get(function_start..attribute_start)?;
    let expression = call.arguments.args.get(1)?;
    if matches!(expression, Expr::Yield(_) | Expr::YieldFrom(_)) {
        return None;
    }
    let argument = source_text(source, expression.range())?;
    Some(format!("{prefix}append({argument})"))
}

fn append_to_extend_replacement(source: &str, call: &ExprCall) -> Option<String> {
    let prefix = method_call_prefix(source, call)?;
    let argument = source_text(source, call.arguments.inner_range())?;
    Some(format!("{prefix}extend([{argument}])"))
}

fn extend_to_append_replacement(source: &str, call: &ExprCall) -> Option<String> {
    let Expr::List(list) = call.arguments.args.first()? else {
        return None;
    };
    if list.ctx != ExprContext::Load || list.elts.len() != 1 || list.elts[0].is_starred_expr() {
        return None;
    }
    let prefix = method_call_prefix(source, call)?;
    let literal = source_text(source, list.range())?;
    let contents = literal.strip_prefix('[')?.strip_suffix(']')?;
    Some(format!("{prefix}append({contents})"))
}

fn mapping_get_to_subscript_replacement(source: &str, call: &ExprCall) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let receiver = source_text(source, attribute.value.range())?;
    let key = source_text(source, call.arguments.inner_range())?;
    Some(format!("{receiver}[{key}]"))
}

fn subscript_to_mapping_get_replacement(source: &str, subscript: &ExprSubscript) -> Option<String> {
    let receiver = source_text(source, subscript.value.range())?;
    let receiver_end = usize::from(subscript.value.range().end());
    let subscript_end = usize::from(subscript.range().end());
    let indexed = source.get(receiver_end..subscript_end)?;
    let opening_bracket = indexed.find('[')?;
    let key = indexed.get(opening_bracket + 1..)?.strip_suffix(']')?;
    Some(format!("{receiver}.get({key})"))
}

fn renamed_method_call_replacement(
    source: &str,
    call: &ExprCall,
    replacement: &str,
) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let function_start = usize::from(call.func.range().start());
    let attribute_start = usize::from(attribute.attr.range().start());
    let attribute_end = usize::from(attribute.attr.range().end());
    let call_end = usize::from(call.range().end());
    let prefix = source.get(function_start..attribute_start)?;
    let suffix = source.get(attribute_end..call_end)?;
    Some(format!("{prefix}{replacement}{suffix}"))
}

fn method_call_prefix<'a>(source: &'a str, call: &ExprCall) -> Option<&'a str> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let function_start = usize::from(call.func.range().start());
    let attribute_start = usize::from(attribute.attr.range().start());
    source.get(function_start..attribute_start)
}

fn is_simple_receiver(expression: &Expr) -> bool {
    matches!(expression, Expr::Name(_) | Expr::Attribute(_))
}

fn is_supported_mapping_key(expression: &Expr) -> bool {
    !matches!(
        expression,
        Expr::Generator(_) | Expr::Slice(_) | Expr::Starred(_)
    ) && !matches!(expression, Expr::Tuple(tuple) if !tuple.parenthesized)
}

fn list_to_tuple_replacement(
    source: &str,
    list: &ExprList,
    tokens: &ruff_python_ast::token::Tokens,
) -> Option<String> {
    let range = list.range();
    let literal = source_text(source, range)?;
    let contents = literal.strip_prefix('[')?.strip_suffix(']')?;
    if list.elts.len() != 1 {
        return Some(format!("({contents})"));
    }
    let element_end = usize::from(list.elts[0].range().end());
    let content_end = usize::from(range.end()).checked_sub(1)?;
    if has_comma_after_element(tokens, element_end, content_end) {
        return Some(format!("({contents})"));
    }
    Some(format!("({contents},)"))
}

fn has_comma_after_element(
    tokens: &ruff_python_ast::token::Tokens,
    element_end: usize,
    content_end: usize,
) -> bool {
    tokens.iter().any(|token| {
        let range = token.range();
        usize::from(range.start()) >= element_end
            && usize::from(range.end()) <= content_end
            && token.kind() == TokenKind::Comma
    })
}

fn tuple_to_list_replacement(source: &str, tuple: &ExprTuple) -> Option<String> {
    let literal = source_text(source, tuple.range())?;
    let contents = literal
        .strip_prefix('(')
        .and_then(|literal| literal.strip_suffix(')'))
        .unwrap_or(literal);
    Some(format!("[{contents}]"))
}

fn base_exception_boundary_replacement(name: &str) -> Option<&'static str> {
    match name {
        "Exception" => Some("BaseException"),
        "BaseException" => Some("Exception"),
        _ => None,
    }
}

fn is_termination_exception(name: &str) -> bool {
    matches!(name, "SystemExit" | "KeyboardInterrupt" | "GeneratorExit")
}

fn supported_exception_tuple_names<'a>(
    tuple: &'a ExprTuple,
    facts: &AstFacts<'_>,
) -> Option<Vec<&'a str>> {
    if !tuple.parenthesized || tuple.elts.is_empty() {
        return None;
    }
    tuple
        .elts
        .iter()
        .map(|element| {
            let Expr::Name(name) = element else {
                return None;
            };
            let name = name.id.as_str();
            (EXCEPTION_NAMES.contains(&name) && !facts.is_exception_bound(name)).then_some(name)
        })
        .collect()
}

fn tuple_add_replacement(
    source: &str,
    tuple: &ExprTuple,
    tokens: &ruff_python_ast::token::Tokens,
    name: &str,
) -> Option<String> {
    let literal = source_text(source, tuple.range())?;
    if !literal.starts_with('(') || !literal.ends_with(')') {
        return None;
    }
    let close_start = usize::from(tuple.range().end()).checked_sub(1)?;
    let has_trailing_comma = tokens
        .iter()
        .filter(|token| {
            let range = token.range();
            usize::from(range.start()) >= usize::from(tuple.range().start())
                && usize::from(range.end()) <= close_start
                && !token.kind().is_trivia()
        })
        .last()
        .is_some_and(|token| token.kind() == TokenKind::Comma);
    let insertion = if has_trailing_comma {
        format!(" {name}")
    } else {
        format!(", {name}")
    };
    let insertion_offset = literal.len().checked_sub(1)?;
    let mut replacement = literal.to_owned();
    replacement.insert_str(insertion_offset, &insertion);
    Some(replacement)
}

fn tuple_remove_replacement(
    source: &str,
    tuple: &ExprTuple,
    tokens: &ruff_python_ast::token::Tokens,
    index: usize,
) -> Option<String> {
    let literal = source_text(source, tuple.range())?;
    let tuple_start = usize::from(tuple.range().start());
    let tuple_end = usize::from(tuple.range().end());
    let element = tuple.elts.get(index)?;
    let element_range = element.range();
    let element_start = usize::from(element_range.start());
    let element_end = usize::from(element_range.end());
    let commas: Vec<_> = tokens
        .iter()
        .filter_map(|token| {
            let range = token.range();
            (token.kind() == TokenKind::Comma
                && usize::from(range.start()) >= tuple_start
                && usize::from(range.end()) <= tuple_end)
                .then_some(range)
        })
        .collect();
    let comma = if let Some(comma) = commas
        .iter()
        .find(|range| usize::from(range.start()) >= element_end)
    {
        *comma
    } else {
        *commas
            .iter()
            .rfind(|range| usize::from(range.end()) <= element_start)?
    };
    let mut removal_ranges = [element_range, comma];
    removal_ranges.sort_by_key(|range| (*range).start());
    let mut replacement = literal.to_owned();
    for range in removal_ranges.into_iter().rev() {
        let start = usize::from(range.start()).checked_sub(tuple_start)?;
        let end = usize::from(range.end()).checked_sub(tuple_start)?;
        replacement.replace_range(start..end, "");
    }
    Some(replacement)
}

fn is_main_guard(expression: &Expr) -> bool {
    let Expr::Compare(compare) = expression else {
        return false;
    };
    if compare.ops.len() != 1 || compare.ops[0] != CmpOp::Eq || compare.comparators.len() != 1 {
        return false;
    }
    let right = &compare.comparators[0];
    (is_dunder_name(compare.left.as_ref()) && is_main_literal(right))
        || (is_main_literal(compare.left.as_ref()) && is_dunder_name(right))
}

fn is_dunder_name(expression: &Expr) -> bool {
    matches!(expression, Expr::Name(name) if name.id.as_str() == "__name__")
}

fn is_main_literal(expression: &Expr) -> bool {
    matches!(expression, Expr::StringLiteral(value) if value.value.to_str() == "__main__")
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
                            let resolved = if alias.asname.is_some() { name } else { local };
                            imports
                                .modules
                                .insert(local.to_owned(), resolved.to_owned());
                        }
                    }
                }
                Stmt::ImportFrom(import) if import.level == 0 => {
                    let Some(module_name) = import
                        .module
                        .as_ref()
                        .map(ruff_python_ast::Identifier::as_str)
                    else {
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

    fn spelling_for(&self, source: &str, targets: &[&str]) -> Option<String> {
        if let Some((prefix, _)) = source.rsplit_once('.') {
            let imported_module = self.modules.iter().any(|(local, module)| {
                (prefix == local
                    || prefix
                        .strip_prefix(local)
                        .is_some_and(|suffix| suffix.starts_with('.')))
                    && targets
                        .iter()
                        .any(|target| target.starts_with(module.as_str()))
            });
            return imported_module.then(|| {
                let (_, name) = targets
                    .first()
                    .and_then(|target| target.rsplit_once('.'))
                    .expect("abstract type targets contain a qualified name");
                format!("{prefix}.{name}")
            });
        }
        let direct = self
            .direct
            .iter()
            .filter(|(_, resolved)| targets.contains(&resolved.as_str()))
            .map(|(local, _)| local.clone())
            .min();
        direct.or_else(|| {
            self.modules
                .iter()
                .flat_map(|(local, module)| {
                    targets.iter().filter_map(move |target| {
                        target
                            .strip_prefix(module)
                            .and_then(|suffix| suffix.strip_prefix('.'))
                            .map(|suffix| format!("{local}.{suffix}"))
                    })
                })
                .min()
        })
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
            | "Literal"
            | "Protocol"
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
        for parameter in &definition.parameters {
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
    line_index: &LineIndex,
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
                    make_candidate(
                        request,
                        source,
                        line_index,
                        start..end,
                        replacement,
                        operator,
                        symbol.clone(),
                    )
                })
        })
        .collect()
}

fn annotation_replacements(
    annotation: &Expr,
    source: &str,
    imports: &KnownImports,
) -> Vec<(String, MutationOperator)> {
    if contains_disallowed_annotation(annotation, imports) {
        return Vec::new();
    }
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
    replacements.extend(collection_replacements(annotation, source, imports));
    replacements
}

fn nullable_removal(annotation: &Expr, source: &str, imports: &KnownImports) -> Option<String> {
    if let Expr::BinOp(binary) = annotation
        && binary.op == Operator::BitOr
    {
        if is_none(binary.left.as_ref()) {
            return Some(expression_source(binary.right.as_ref(), source));
        }
        if is_none(binary.right.as_ref()) {
            return Some(expression_source(binary.left.as_ref(), source));
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
        Expr::Name(name) => matches!(name.id.as_str(), "str" | "int" | "float" | "bool" | "bytes"),
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
            !matches!(name.id.as_str(), "str" | "int" | "float" | "bool" | "bytes")
                || imports.type_vars.contains(name.id.as_str())
                || imports.resolved_name(annotation).as_deref() == Some("typing.Any")
        }
        Expr::Attribute(_) => matches!(
            imports.resolved_name(annotation).as_deref(),
            Some("typing.Any" | "typing.Protocol")
        ),
        Expr::Subscript(subscript) => {
            matches!(
                imports.resolved_name(subscript.value.as_ref()).as_deref(),
                Some("typing.Annotated" | "typing.Callable" | "typing.Literal" | "typing.Protocol")
            ) || contains_disallowed_annotation(subscript.slice.as_ref(), imports)
        }
        Expr::BinOp(binary) if binary.op == Operator::BitOr => {
            contains_disallowed_annotation(binary.left.as_ref(), imports)
                || contains_disallowed_annotation(binary.right.as_ref(), imports)
        }
        _ => false,
    }
}

fn collection_replacements(
    annotation: &Expr,
    source: &str,
    imports: &KnownImports,
) -> Vec<(String, MutationOperator)> {
    let Expr::Subscript(subscript) = annotation else {
        return Vec::new();
    };
    let Some(resolved) = imports.resolved_name(subscript.value.as_ref()) else {
        return Vec::new();
    };
    let base = expression_source(subscript.value.as_ref(), source);
    let replacement = |name: String, operator| {
        (
            replace_subscript_base(annotation, subscript.value.as_ref(), source, &name),
            operator,
        )
    };
    match resolved.as_str() {
        "list" => imports
            .spelling_for(&base, &["typing.Sequence", "collections.abc.Sequence"])
            .map(|name| vec![replacement(name, MutationOperator::TypeListSequence)])
            .unwrap_or_default(),
        "typing.Sequence" | "collections.abc.Sequence" => {
            let mut replacements = vec![replacement(
                "list".to_owned(),
                MutationOperator::TypeListSequence,
            )];
            if let Some(name) =
                imports.spelling_for(&base, &["typing.Iterable", "collections.abc.Iterable"])
            {
                replacements.push(replacement(name, MutationOperator::TypeSequenceIterable));
            }
            replacements
        }
        "set" => imports
            .spelling_for(
                &base,
                &["typing.AbstractSet", "collections.abc.AbstractSet"],
            )
            .map(|name| vec![replacement(name, MutationOperator::TypeSetAbstractSet)])
            .unwrap_or_default(),
        "typing.AbstractSet" | "collections.abc.AbstractSet" => vec![replacement(
            "set".to_owned(),
            MutationOperator::TypeSetAbstractSet,
        )],
        "dict" => imports
            .spelling_for(&base, &["typing.Mapping", "collections.abc.Mapping"])
            .map(|name| vec![replacement(name, MutationOperator::TypeMapping)])
            .unwrap_or_default(),
        "typing.Mapping" | "collections.abc.Mapping" => vec![replacement(
            "dict".to_owned(),
            MutationOperator::TypeMapping,
        )],
        "typing.Iterable" | "collections.abc.Iterable" => imports
            .spelling_for(&base, &["typing.Iterator", "collections.abc.Iterator"])
            .map(|name| vec![replacement(name, MutationOperator::TypeIterableIterator)])
            .unwrap_or_default(),
        "typing.Iterator" | "collections.abc.Iterator" => imports
            .spelling_for(&base, &["typing.Iterable", "collections.abc.Iterable"])
            .map(|name| vec![replacement(name, MutationOperator::TypeIterableIterator)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
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
    let replacement_base = replacement;
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
