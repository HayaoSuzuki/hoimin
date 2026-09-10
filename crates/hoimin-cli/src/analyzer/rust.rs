#[cfg(test)]
use std::cell::Cell;
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
    CmpOp, Expr, ExprBinOp, ExprCall, ExprContext, ExprList, ExprSlice, ExprSubscript, ExprTuple,
    ModModule, Number, Operator, Pattern, Singleton, Stmt, TypeParam, TypeParams, UnaryOp, visitor,
};
use ruff_python_parser::parse_module;
use ruff_text_size::{Ranged, TextRange};

use super::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

#[path = "rust/fact_index.rs"]
mod fact_index;
#[path = "rust/operator_functions.rs"]
mod operator_functions;
#[cfg(test)]
use fact_index::IndexLookupStats;
use fact_index::{ContainmentIndex, NotOperandIndex, ScopeIndex, ScopeInterval};
use operator_functions::OperatorImports;

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
    #[cfg(test)]
    pub retention: CandidateRetentionStats,
    #[cfg(test)]
    pub fact_lookups: FactLookupStats,
    #[cfg(test)]
    pub candidate_token_lookups: CandidateTokenLookupStats,
}

#[cfg(test)]
pub(crate) struct CandidateRetentionStats {
    pub producer_peaks: [usize; 3],
    pub merged_peak: usize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FactLookupStats {
    pub annotation: IndexLookupStats,
    pub arid: IndexLookupStats,
    pub not_operand: IndexLookupStats,
    pub scope: IndexLookupStats,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CandidateTokenLookupStats {
    pub lookups: usize,
    pub tokens_examined: usize,
}

pub(crate) struct ProducerPrefix {
    pub(crate) candidates: Vec<AnalyzerCandidate>,
    pub(crate) overflowed: bool,
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
    next_emission_sequence: u64,
}

impl CandidatePrefix {
    pub(crate) fn new(max_candidates: usize) -> Self {
        Self {
            entries: BinaryHeap::new(),
            identities: HashSet::new(),
            capacity: max_candidates.saturating_add(1),
            overflowed: false,
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
    let mut token_candidates = CandidatePrefix::new(request.max_candidates);
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
        if facts.contains_annotation_span(range) {
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
        } else if text == "not"
            && let Some((expression_start, expression_end)) = facts.not_operand_range(start)
        {
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
        ) && retained_by_profile(&candidate, request.profile, &facts)
        {
            token_candidates.push(candidate);
        }
    }
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    let token_candidates = token_candidates.finish();
    let ast_candidates = ast_candidates(
        parsed.syntax(),
        source,
        &line_index,
        &facts,
        request,
        &cancelled,
    )?;
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    let type_annotation_candidates =
        type_annotation_candidates(parsed.syntax(), source, &line_index, &facts, request);
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    let producer_overflowed = token_candidates.overflowed
        || ast_candidates.overflowed
        || type_annotation_candidates.overflowed;
    #[cfg(test)]
    let producer_peaks = [
        token_candidates.candidates.len(),
        ast_candidates.candidates.len(),
        type_annotation_candidates.candidates.len(),
    ];
    let mut candidates = token_candidates.candidates;
    candidates.extend(ast_candidates.candidates);
    candidates.extend(type_annotation_candidates.candidates);
    #[cfg(test)]
    let merged_peak = candidates.len();
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
    let truncated = producer_overflowed || candidates.len() > request.max_candidates;
    candidates.truncate(request.max_candidates);
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
    #[cfg(test)]
    let fact_lookups = facts.lookup_stats();
    #[cfg(test)]
    let candidate_token_lookups = facts.candidate_token_lookup_stats();
    Ok(AnalyzerOutput {
        candidates,
        diagnostics,
        truncated,
        #[cfg(test)]
        retention: CandidateRetentionStats {
            producer_peaks,
            merged_peak,
        },
        #[cfg(test)]
        fact_lookups,
        #[cfg(test)]
        candidate_token_lookups,
    })
}

fn retained_by_profile(
    candidate: &AnalyzerCandidate,
    profile: MutationProfile,
    facts: &AstFacts<'_>,
) -> bool {
    if profile != MutationProfile::Focused || candidate.operator.starts_with("type_") {
        return true;
    }
    let Some(end) = candidate.span.start.checked_add(candidate.span.length) else {
        return true;
    };
    let (Ok(start), Ok(end)) = (usize::try_from(candidate.span.start), usize::try_from(end)) else {
        return true;
    };
    !facts.contains_arid_span(start, end)
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
        #[cfg(test)]
        retention: CandidateRetentionStats {
            producer_peaks: [0; 3],
            merged_peak: 0,
        },
        #[cfg(test)]
        fact_lookups: FactLookupStats::default(),
        #[cfg(test)]
        candidate_token_lookups: CandidateTokenLookupStats::default(),
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
        "*=" => ("/=", "augmented_mul_div"),
        "/=" => ("*=", "augmented_mul_div"),
        "//=" => ("%=", "augmented_floor_mod"),
        "%=" => ("//=", "augmented_floor_mod"),
        "**=" => ("*=", "augmented_power"),
        "@=" => ("*=", "augmented_matmul"),
        "&=" => ("|=", "augmented_bitwise_and_or"),
        "|=" => ("&=", "augmented_bitwise_and_or"),
        "^=" => ("&=", "augmented_bitwise_xor"),
        "<<=" => (">>=", "augmented_bitwise_shift"),
        ">>=" => ("<<=", "augmented_bitwise_shift"),
        "*" => ("/", "binary_mul_div"),
        "/" => ("*", "binary_mul_div"),
        "//" => ("%", "binary_floor_mod"),
        "%" => ("//", "binary_floor_mod"),
        "**" => ("*", "binary_power"),
        "@" => ("*", "binary_matmul"),
        "&" => ("|", "bitwise_and_or"),
        "|" => ("&", "bitwise_and_or"),
        "^" => ("&", "bitwise_xor"),
        "<<" => (">>", "bitwise_shift"),
        ">>" => ("<<", "bitwise_shift"),
        "~" => ("+", "bitwise_invert"),
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
    starts: Vec<u32>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(u32::try_from(index + 1).expect("Ruff source offset fits u32"));
            }
        }
        Self { starts }
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "Ruff TextSize offsets cap parsed source at u32::MAX bytes, and code-point counts cannot exceed byte counts."
    )]
    fn line_and_column(&self, source: &str, offset: usize) -> (u32, u32) {
        let line_index = self
            .starts
            .partition_point(|start| (*start as usize) <= offset)
            - 1;
        let line_start = self.starts[line_index] as usize;
        let line = line_index as u32 + 1;
        let line_prefix = &source[line_start..offset];
        let column_prefix = if line_index == 0 {
            line_prefix.strip_prefix('\u{feff}').unwrap_or(line_prefix)
        } else {
            line_prefix
        };
        let column = column_prefix.chars().count() as u32;
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
            let (module_selected, qualname) = selector.rsplit_once(':').map_or_else(
                || (true, selector.as_str()),
                |(module, qualname)| {
                    let current_module = module_name(request.path);
                    (
                        current_module == module || current_module.ends_with(&format!(".{module}")),
                        qualname,
                    )
                },
            );
            module_selected
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

fn visit_type_param_expressions<'ast>(
    type_params: &'ast TypeParams,
    mut visit: impl FnMut(&'ast Expr),
) {
    for parameter in type_params.iter() {
        match parameter {
            TypeParam::TypeVar(parameter) => {
                if let Some(bound) = parameter.bound.as_deref() {
                    visit(bound);
                }
                if let Some(default) = parameter.default.as_deref() {
                    visit(default);
                }
            }
            TypeParam::TypeVarTuple(parameter) => {
                if let Some(default) = parameter.default.as_deref() {
                    visit(default);
                }
            }
            TypeParam::ParamSpec(parameter) => {
                if let Some(default) = parameter.default.as_deref() {
                    visit(default);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NameResolution {
    DefinitelyBuiltin,
    Shadowed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NameScopeKind {
    Module,
    Function,
    Class,
    Comprehension,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BindingEffect {
    Bind,
    MaybeBind,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ScopeId(usize);

struct NameScope {
    kind: NameScopeKind,
    parent: Option<ScopeId>,
    locals: HashSet<String>,
    possible_bindings: HashSet<String>,
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
    ordered: HashMap<String, Vec<(usize, BindingEffect)>>,
    wildcard: bool,
}

impl NameScope {
    fn new(kind: NameScopeKind, parent: Option<ScopeId>) -> Self {
        Self {
            kind,
            parent,
            locals: HashSet::new(),
            possible_bindings: HashSet::new(),
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            ordered: HashMap::new(),
            wildcard: false,
        }
    }
}

struct NameOccurrence {
    scope: ScopeId,
    temporarily_shadowed: bool,
}

#[derive(Default)]
struct NameResolutionIndex {
    scopes: Vec<NameScope>,
    occurrences: HashMap<usize, NameOccurrence>,
    back_edge_bindings: HashMap<usize, HashSet<String>>,
}

impl NameResolutionIndex {
    fn from_module(module: &ModModule) -> Self {
        let mut builder = NameResolutionBuilder::new();
        builder.visit_body(&module.body);
        builder.index
    }

    fn resolution(&self, offset: usize, name: &str) -> NameResolution {
        let Some(occurrence) = self.occurrences.get(&offset) else {
            return NameResolution::Unknown;
        };
        if occurrence.temporarily_shadowed {
            return NameResolution::Shadowed;
        }
        let resolution = self.resolve_scope(occurrence.scope, name, true, offset);
        if resolution == NameResolution::DefinitelyBuiltin
            && self
                .back_edge_bindings
                .get(&offset)
                .is_some_and(|bindings| bindings.contains(name))
        {
            NameResolution::Unknown
        } else {
            resolution
        }
    }

    fn resolve_scope(
        &self,
        scope_id: ScopeId,
        name: &str,
        direct: bool,
        offset: usize,
    ) -> NameResolution {
        let scope = &self.scopes[scope_id.0];
        if scope.kind != NameScopeKind::Module && scope.globals.contains(name) {
            return self.resolve_module(name);
        }
        if scope.nonlocals.contains(name) {
            return self.resolve_nonlocal(scope.parent, name);
        }
        match scope.kind {
            NameScopeKind::Module => {
                if direct {
                    Self::resolve_ordered_at(scope, name, offset)
                } else if scope.wildcard || scope.possible_bindings.contains(name) {
                    NameResolution::Unknown
                } else {
                    NameResolution::DefinitelyBuiltin
                }
            }
            NameScopeKind::Function | NameScopeKind::Comprehension => {
                if scope.locals.contains(name) {
                    NameResolution::Shadowed
                } else if scope.wildcard {
                    NameResolution::Unknown
                } else {
                    self.resolve_parent(scope.parent, name, offset)
                }
            }
            NameScopeKind::Class => {
                if direct {
                    match Self::resolve_ordered_at(scope, name, offset) {
                        NameResolution::DefinitelyBuiltin => {
                            self.resolve_class_parent(scope.parent, name, offset)
                        }
                        resolution => resolution,
                    }
                } else {
                    self.resolve_parent(scope.parent, name, offset)
                }
            }
        }
    }

    fn resolve_ordered_at(scope: &NameScope, name: &str, offset: usize) -> NameResolution {
        let mut resolution = NameResolution::DefinitelyBuiltin;
        if let Some(events) = scope.ordered.get(name) {
            for (event_offset, effect) in events {
                if *event_offset > offset {
                    continue;
                }
                resolution = match effect {
                    BindingEffect::Bind => NameResolution::Shadowed,
                    BindingEffect::MaybeBind => match resolution {
                        NameResolution::Shadowed => NameResolution::Shadowed,
                        NameResolution::DefinitelyBuiltin | NameResolution::Unknown => {
                            NameResolution::Unknown
                        }
                    },
                    BindingEffect::Unknown => NameResolution::Unknown,
                };
            }
        }
        resolution
    }

    fn resolve_parent(&self, parent: Option<ScopeId>, name: &str, offset: usize) -> NameResolution {
        parent.map_or(NameResolution::DefinitelyBuiltin, |parent| {
            self.resolve_scope(parent, name, false, offset)
        })
    }

    fn resolve_class_parent(
        &self,
        mut parent: Option<ScopeId>,
        name: &str,
        offset: usize,
    ) -> NameResolution {
        while let Some(scope_id) = parent {
            let scope = &self.scopes[scope_id.0];
            match scope.kind {
                NameScopeKind::Module => {
                    return self.resolve_scope(scope_id, name, true, offset);
                }
                NameScopeKind::Class => parent = scope.parent,
                NameScopeKind::Function | NameScopeKind::Comprehension => {
                    return self.resolve_scope(scope_id, name, false, offset);
                }
            }
        }
        NameResolution::DefinitelyBuiltin
    }

    fn resolve_module(&self, name: &str) -> NameResolution {
        let Some(module) = self
            .scopes
            .iter()
            .find(|scope| scope.kind == NameScopeKind::Module)
        else {
            return NameResolution::Unknown;
        };
        if module.wildcard || module.possible_bindings.contains(name) {
            NameResolution::Unknown
        } else {
            NameResolution::DefinitelyBuiltin
        }
    }

    fn resolve_nonlocal(&self, mut parent: Option<ScopeId>, name: &str) -> NameResolution {
        while let Some(scope_id) = parent {
            let scope = &self.scopes[scope_id.0];
            if matches!(
                scope.kind,
                NameScopeKind::Function | NameScopeKind::Comprehension
            ) && scope.locals.contains(name)
            {
                return NameResolution::Shadowed;
            }
            parent = scope.parent;
        }
        NameResolution::Unknown
    }
}

fn tracked_resolution_name(name: &str) -> bool {
    MUTABLE_BUILTINS.contains(&name) || EXCEPTION_NAMES.contains(&name)
}

struct NameResolutionBuilder {
    index: NameResolutionIndex,
    current: ScopeId,
    conditional_depth: usize,
    temporary_shadowed: Vec<(ScopeId, HashSet<String>)>,
    loop_back_edges: Vec<LoopBackEdgeContext>,
}

struct LoopBackEdgeContext {
    scope: ScopeId,
    occurrences: Vec<usize>,
    bindings: HashSet<String>,
}

impl NameResolutionBuilder {
    fn new() -> Self {
        Self {
            index: NameResolutionIndex {
                scopes: vec![NameScope::new(NameScopeKind::Module, None)],
                occurrences: HashMap::new(),
                back_edge_bindings: HashMap::new(),
            },
            current: ScopeId(0),
            conditional_depth: 0,
            temporary_shadowed: Vec::new(),
            loop_back_edges: Vec::new(),
        }
    }

    fn new_scope(&mut self, kind: NameScopeKind) -> ScopeId {
        let id = ScopeId(self.index.scopes.len());
        self.index
            .scopes
            .push(NameScope::new(kind, Some(self.current)));
        id
    }

    fn in_scope(&mut self, scope: ScopeId, visit: impl FnOnce(&mut Self)) {
        let outer = self.current;
        self.current = scope;
        visit(self);
        self.current = outer;
    }

    fn in_loop_back_edge(&mut self, visit: impl FnOnce(&mut Self)) {
        self.loop_back_edges.push(LoopBackEdgeContext {
            scope: self.current,
            occurrences: Vec::new(),
            bindings: HashSet::new(),
        });
        visit(self);
        let context = self
            .loop_back_edges
            .pop()
            .expect("loop back-edge context was pushed");
        if context.bindings.is_empty() {
            return;
        }
        for offset in context.occurrences {
            self.index
                .back_edge_bindings
                .entry(offset)
                .or_default()
                .extend(context.bindings.iter().cloned());
        }
    }

    fn loop_scope_visible(scopes: &[NameScope], current: ScopeId, owner: ScopeId) -> bool {
        if owner == current {
            return true;
        }
        if scopes[current.0].kind != NameScopeKind::Class {
            return false;
        }
        let mut parent = scopes[current.0].parent;
        while let Some(scope_id) = parent {
            let scope = &scopes[scope_id.0];
            match scope.kind {
                NameScopeKind::Module => return scope_id == owner,
                NameScopeKind::Class => parent = scope.parent,
                NameScopeKind::Function | NameScopeKind::Comprehension => return false,
            }
        }
        false
    }

    fn record_loop_back_edge_binding(&mut self, scope: ScopeId, name: &str) {
        for context in &mut self.loop_back_edges {
            if context.scope == scope {
                context.bindings.insert(name.to_owned());
            }
        }
    }

    fn add_local(&mut self, scope: ScopeId, name: &str) {
        if tracked_resolution_name(name) {
            self.index.scopes[scope.0].locals.insert(name.to_owned());
            self.index.scopes[scope.0]
                .possible_bindings
                .insert(name.to_owned());
        }
    }

    fn target_names(target: &Expr, names: &mut Vec<String>) {
        match target {
            Expr::Name(name) => names.push(name.id.to_string()),
            Expr::List(list) => {
                for element in &list.elts {
                    Self::target_names(element, names);
                }
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    Self::target_names(element, names);
                }
            }
            Expr::Starred(starred) => Self::target_names(starred.value.as_ref(), names),
            _ => {}
        }
    }

    fn record_target(&mut self, target: &Expr, offset: usize) {
        let mut names = Vec::new();
        Self::target_names(target, &mut names);
        for name in names {
            self.record_binding(&name, offset);
        }
    }

    fn record_binding(&mut self, name: &str, offset: usize) {
        if !tracked_resolution_name(name) {
            return;
        }
        let globals = self.index.scopes[self.current.0].globals.contains(name);
        let nonlocals = self.index.scopes[self.current.0].nonlocals.contains(name);
        if globals {
            self.record_loop_back_edge_binding(ScopeId(0), name);
            self.index.scopes[0]
                .possible_bindings
                .insert(name.to_owned());
            self.index.scopes[0]
                .ordered
                .entry(name.to_owned())
                .or_default()
                .push((offset, BindingEffect::Unknown));
            return;
        }
        if nonlocals {
            return;
        }
        self.record_loop_back_edge_binding(self.current, name);
        let scope = &mut self.index.scopes[self.current.0];
        scope.possible_bindings.insert(name.to_owned());
        match scope.kind {
            NameScopeKind::Function | NameScopeKind::Comprehension => {
                scope.locals.insert(name.to_owned());
            }
            NameScopeKind::Module | NameScopeKind::Class => {
                scope.ordered.entry(name.to_owned()).or_default().push((
                    offset,
                    if self.conditional_depth == 0 {
                        BindingEffect::Bind
                    } else {
                        BindingEffect::MaybeBind
                    },
                ));
            }
        }
    }

    fn record_unknown(&mut self, name: &str, offset: usize) {
        if !tracked_resolution_name(name) {
            return;
        }
        self.record_loop_back_edge_binding(self.current, name);
        let scope = &mut self.index.scopes[self.current.0];
        scope.possible_bindings.insert(name.to_owned());
        if matches!(scope.kind, NameScopeKind::Module | NameScopeKind::Class) {
            scope
                .ordered
                .entry(name.to_owned())
                .or_default()
                .push((offset, BindingEffect::Unknown));
        } else {
            scope.locals.insert(name.to_owned());
        }
    }

    fn record_alias(&mut self, alias: &ruff_python_ast::Alias, from_import: bool, offset: usize) {
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
            for name in MUTABLE_BUILTINS.iter().chain(EXCEPTION_NAMES) {
                self.record_loop_back_edge_binding(self.current, name);
            }
            let scope = &mut self.index.scopes[self.current.0];
            scope.wildcard = true;
            for name in MUTABLE_BUILTINS.iter().chain(EXCEPTION_NAMES) {
                scope.possible_bindings.insert((*name).to_owned());
                if matches!(scope.kind, NameScopeKind::Module | NameScopeKind::Class) {
                    scope
                        .ordered
                        .entry((*name).to_owned())
                        .or_default()
                        .push((offset, BindingEffect::Unknown));
                }
            }
        } else {
            self.record_binding(local, offset);
        }
    }

    fn record_dynamic_uncertainty(&mut self, offset: usize) {
        self.record_scope_wildcard(self.current, offset);
        if self.current != ScopeId(0) {
            self.record_scope_wildcard(ScopeId(0), offset);
        }
    }

    fn record_scope_wildcard(&mut self, scope_id: ScopeId, offset: usize) {
        for name in MUTABLE_BUILTINS.iter().chain(EXCEPTION_NAMES) {
            self.record_loop_back_edge_binding(scope_id, name);
        }
        let scope = &mut self.index.scopes[scope_id.0];
        scope.wildcard = true;
        for name in MUTABLE_BUILTINS.iter().chain(EXCEPTION_NAMES) {
            scope.possible_bindings.insert((*name).to_owned());
            if matches!(scope.kind, NameScopeKind::Module | NameScopeKind::Class) {
                scope
                    .ordered
                    .entry((*name).to_owned())
                    .or_default()
                    .push((offset, BindingEffect::Unknown));
            }
        }
    }

    fn record_occurrence(&mut self, name: &ruff_python_ast::ExprName) {
        let id = name.id.as_str();
        if name.ctx != ExprContext::Load || !tracked_resolution_name(id) {
            return;
        }
        let temporarily_shadowed = self
            .temporary_shadowed
            .iter()
            .rev()
            .any(|(scope, names)| names.contains(id) && self.temporary_binding_visible(*scope));
        let offset = usize::from(name.range.start());
        self.record_resolution_site(offset, temporarily_shadowed);
    }

    fn record_resolution_site(&mut self, offset: usize, temporarily_shadowed: bool) {
        self.index.occurrences.insert(
            offset,
            NameOccurrence {
                scope: self.current,
                temporarily_shadowed,
            },
        );
        let scopes = &self.index.scopes;
        for context in &mut self.loop_back_edges {
            if Self::loop_scope_visible(scopes, self.current, context.scope) {
                context.occurrences.push(offset);
            }
        }
    }

    fn temporary_binding_visible(&self, owner: ScopeId) -> bool {
        if owner == self.current {
            return true;
        }
        if self.index.scopes[self.current.0].kind != NameScopeKind::Class {
            return false;
        }
        let mut parent = self.index.scopes[self.current.0].parent;
        while let Some(scope_id) = parent {
            let scope = &self.index.scopes[scope_id.0];
            match scope.kind {
                NameScopeKind::Module => return scope_id == owner,
                NameScopeKind::Class => parent = scope.parent,
                NameScopeKind::Function | NameScopeKind::Comprehension => return false,
            }
        }
        false
    }

    fn visit_comprehension_expression(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        result: impl FnOnce(&mut Self),
    ) {
        let Some((first, rest)) = generators.split_first() else {
            result(self);
            return;
        };
        self.visit_expr(&first.iter);
        let scope = self.new_scope(NameScopeKind::Comprehension);
        for generator in generators {
            let mut names = Vec::new();
            Self::target_names(&generator.target, &mut names);
            for name in names {
                self.add_local(scope, &name);
            }
        }
        self.in_scope(scope, |this| {
            this.visit_expr(&first.target);
            for condition in &first.ifs {
                this.visit_expr(condition);
            }
            for generator in rest {
                this.visit_expr(&generator.iter);
                this.visit_expr(&generator.target);
                for condition in &generator.ifs {
                    this.visit_expr(condition);
                }
            }
            result(this);
        });
    }
}

impl<'ast> Visitor<'ast> for NameResolutionBuilder {
    #[expect(
        clippy::too_many_lines,
        reason = "the exhaustive statement-binding pass keeps evaluation order and scope entry visible in one match"
    )]
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        let end = usize::from(statement.range().end());
        match statement {
            Stmt::FunctionDef(definition) => {
                self.record_binding(definition.name.as_str(), end);
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = &definition.type_params {
                    self.visit_type_params(type_params);
                }
                self.visit_parameters(&definition.parameters);
                if let Some(returns) = &definition.returns {
                    self.visit_annotation(returns);
                }
                let scope = self.new_scope(NameScopeKind::Function);
                for parameter in definition.parameters.as_ref() {
                    self.add_local(scope, parameter.name().as_str());
                }
                self.in_scope(scope, |this| this.visit_body(&definition.body));
                return;
            }
            Stmt::ClassDef(definition) => {
                self.record_binding(definition.name.as_str(), end);
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = &definition.type_params {
                    self.visit_type_params(type_params);
                }
                if let Some(arguments) = &definition.arguments {
                    self.visit_arguments(arguments);
                }
                let scope = self.new_scope(NameScopeKind::Class);
                self.in_scope(scope, |this| this.visit_body(&definition.body));
                return;
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    self.record_alias(alias, false, end);
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    self.record_alias(alias, true, end);
                }
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.record_target(target, end);
                }
            }
            Stmt::AugAssign(assign) => self.record_target(&assign.target, end),
            Stmt::AnnAssign(assign) => self.record_target(&assign.target, end),
            Stmt::TypeAlias(alias) => self.record_target(&alias.name, end),
            Stmt::Delete(delete) => {
                let mut names = Vec::new();
                for target in &delete.targets {
                    Self::target_names(target, &mut names);
                }
                for name in names {
                    self.record_unknown(&name, end);
                }
            }
            Stmt::Global(global) => {
                for name in &global.names {
                    if tracked_resolution_name(name.as_str()) {
                        self.index.scopes[self.current.0]
                            .globals
                            .insert(name.to_string());
                    }
                }
            }
            Stmt::Nonlocal(nonlocal) => {
                for name in &nonlocal.names {
                    if tracked_resolution_name(name.as_str()) {
                        self.index.scopes[self.current.0]
                            .nonlocals
                            .insert(name.to_string());
                    }
                }
            }
            Stmt::For(statement_for) => {
                self.visit_expr(&statement_for.iter);
                self.conditional_depth += 1;
                self.in_loop_back_edge(|this| {
                    this.record_target(
                        &statement_for.target,
                        usize::from(statement_for.iter.range().end()),
                    );
                    this.visit_expr(&statement_for.target);
                    this.visit_body(&statement_for.body);
                });
                self.visit_body(&statement_for.orelse);
                self.conditional_depth -= 1;
                return;
            }
            Stmt::With(statement_with) => {
                self.conditional_depth += 1;
                for item in &statement_with.items {
                    self.visit_expr(&item.context_expr);
                    if let Some(target) = &item.optional_vars {
                        self.record_target(target, usize::from(item.context_expr.range().end()));
                        self.visit_expr(target);
                    }
                }
                self.visit_body(&statement_with.body);
                self.conditional_depth -= 1;
                return;
            }
            Stmt::While(statement_while) => {
                self.conditional_depth += 1;
                self.in_loop_back_edge(|this| {
                    this.visit_expr(&statement_while.test);
                    this.visit_body(&statement_while.body);
                });
                self.visit_body(&statement_while.orelse);
                self.conditional_depth -= 1;
                return;
            }
            Stmt::If(_) | Stmt::Try(_) | Stmt::Match(_) => {
                self.conditional_depth += 1;
                visitor::walk_stmt(self, statement);
                self.conditional_depth -= 1;
                return;
            }
            _ => {}
        }
        visitor::walk_stmt(self, statement);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if let Expr::Call(call) = expression
            && matches!(
                call.func.as_ref(),
                Expr::Name(name)
                    if matches!(name.id.as_str(), "exec" | "globals" | "locals" | "vars")
            )
        {
            self.record_dynamic_uncertainty(usize::from(call.range.end()));
        }
        match expression {
            Expr::Name(name) => self.record_occurrence(name),
            Expr::Named(named) => {
                self.record_target(&named.target, usize::from(named.range.end()));
            }
            Expr::Lambda(lambda) => {
                let scope = self.new_scope(NameScopeKind::Function);
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters.as_ref() {
                        self.add_local(scope, parameter.name().as_str());
                    }
                    self.visit_parameters(parameters);
                }
                self.in_scope(scope, |this| this.visit_expr(&lambda.body));
                return;
            }
            Expr::ListComp(comprehension) => {
                self.visit_comprehension_expression(&comprehension.generators, |this| {
                    this.visit_expr(&comprehension.elt);
                });
                return;
            }
            Expr::SetComp(comprehension) => {
                self.visit_comprehension_expression(&comprehension.generators, |this| {
                    this.visit_expr(&comprehension.elt);
                });
                return;
            }
            Expr::DictComp(comprehension) => {
                self.visit_comprehension_expression(&comprehension.generators, |this| {
                    this.visit_expr(&comprehension.key);
                    this.visit_expr(&comprehension.value);
                });
                return;
            }
            Expr::Generator(comprehension) => {
                self.visit_comprehension_expression(&comprehension.generators, |this| {
                    this.visit_expr(&comprehension.elt);
                });
                return;
            }
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }

    fn visit_except_handler(&mut self, except_handler: &'ast ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
        self.record_resolution_site(usize::from(handler.range.start()), false);
        if let Some(type_) = &handler.type_ {
            self.visit_expr(type_);
        }
        let Some(name) = &handler.name else {
            self.visit_body(&handler.body);
            return;
        };
        if tracked_resolution_name(name.as_str()) {
            let kind = self.index.scopes[self.current.0].kind;
            if matches!(kind, NameScopeKind::Function | NameScopeKind::Comprehension) {
                self.add_local(self.current, name.as_str());
            } else {
                self.index.scopes[self.current.0]
                    .possible_bindings
                    .insert(name.to_string());
            }
            self.temporary_shadowed
                .push((self.current, HashSet::from([name.to_string()])));
            self.visit_body(&handler.body);
            self.temporary_shadowed.pop();
        } else {
            self.visit_body(&handler.body);
        }
    }

    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        let offset = usize::from(pattern.range().end());
        match pattern {
            Pattern::MatchMapping(mapping) => {
                if let Some(rest) = &mapping.rest {
                    self.record_binding(rest.as_str(), offset);
                }
            }
            Pattern::MatchStar(star) => {
                if let Some(name) = &star.name {
                    self.record_binding(name.as_str(), offset);
                }
            }
            Pattern::MatchAs(as_pattern) => {
                if let Some(name) = &as_pattern.name {
                    self.record_binding(name.as_str(), offset);
                }
            }
            _ => {}
        }
        visitor::walk_pattern(self, pattern);
    }
}

#[derive(Default)]
struct AstFacts<'tokens> {
    operator_token_starts: HashSet<usize>,
    unary_sign_starts: HashSet<usize>,
    not_operands: Vec<(usize, usize, usize)>,
    not_operand_index: NotOperandIndex,
    arid_ranges: Vec<(usize, usize)>,
    arid_index: ContainmentIndex,
    annotation_ranges: Vec<(usize, usize)>,
    annotation_index: ContainmentIndex,
    name_resolution: NameResolutionIndex,
    scopes: Vec<ScopeInterval>,
    scope_index: ScopeIndex,
    qualname: Vec<String>,
    tokens: Option<&'tokens ruff_python_ast::token::Tokens>,
    source: &'tokens str,
    #[cfg(test)]
    candidate_token_lookups: Cell<usize>,
    #[cfg(test)]
    candidate_tokens_examined: Cell<usize>,
}
impl<'tokens> AstFacts<'tokens> {
    fn from_module(
        module: &ModModule,
        tokens: &'tokens ruff_python_ast::token::Tokens,
        source: &'tokens str,
    ) -> Self {
        let name_resolution = NameResolutionIndex::from_module(module);
        let mut facts = Self {
            tokens: Some(tokens),
            source,
            name_resolution,
            ..Self::default()
        };
        for statement in &module.body {
            facts.visit_stmt(statement);
        }
        facts.normalize_arid_ranges();
        facts.finalize_indexes();
        facts
    }

    fn finalize_indexes(&mut self) {
        self.not_operand_index = NotOperandIndex::new(std::mem::take(&mut self.not_operands));
        self.arid_index = ContainmentIndex::new(std::mem::take(&mut self.arid_ranges));
        self.annotation_index = ContainmentIndex::new(std::mem::take(&mut self.annotation_ranges));
        self.scope_index = ScopeIndex::new(std::mem::take(&mut self.scopes));
    }

    fn record_arid_range(&mut self, range: TextRange) {
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
        self.arid_index.contains(start, end)
    }

    fn record_annotation_range(&mut self, range: TextRange) {
        self.annotation_ranges
            .push((usize::from(range.start()), usize::from(range.end())));
    }

    fn contains_annotation_span(&self, range: TextRange) -> bool {
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        self.annotation_index.contains(start, end)
    }

    fn not_operand_range(&self, start: usize) -> Option<(usize, usize)> {
        self.not_operand_index.operand_at(start)
    }

    fn is_unary_sign(&self, start: usize) -> bool {
        self.unary_sign_starts.contains(&start)
    }

    fn is_operator_token(&self, start: usize) -> bool {
        self.operator_token_starts.contains(&start)
    }

    fn candidate_tokens_in_range(&self, range: TextRange) -> &[ruff_python_ast::token::Token] {
        let tokens = self.tokens.expect("parser tokens are set").in_range(range);
        #[cfg(test)]
        {
            self.candidate_token_lookups
                .set(self.candidate_token_lookups.get().saturating_add(1));
            self.candidate_tokens_examined.set(
                self.candidate_tokens_examined
                    .get()
                    .saturating_add(tokens.len()),
            );
        }
        tokens
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

    fn resolves_builtin(&self, range: TextRange, name: &str) -> bool {
        self.name_resolution
            .resolution(usize::from(range.start()), name)
            == NameResolution::DefinitelyBuiltin
    }

    fn resolves_builtin_pair(&self, range: TextRange, source: &str, destination: &str) -> bool {
        self.resolves_builtin(range, source) && self.resolves_builtin(range, destination)
    }

    fn scope_at(&self, offset: usize) -> Option<String> {
        self.scope_index.symbol_at(offset).map(str::to_owned)
    }

    #[cfg(test)]
    fn lookup_stats(&self) -> FactLookupStats {
        FactLookupStats {
            annotation: self.annotation_index.stats(),
            arid: self.arid_index.stats(),
            not_operand: self.not_operand_index.stats(),
            scope: self.scope_index.stats(),
        }
    }

    #[cfg(test)]
    fn candidate_token_lookup_stats(&self) -> CandidateTokenLookupStats {
        CandidateTokenLookupStats {
            lookups: self.candidate_token_lookups.get(),
            tokens_examined: self.candidate_tokens_examined.get(),
        }
    }

    fn visit_definition(
        &mut self,
        name: &str,
        range: TextRange,
        decorators: &[ruff_python_ast::Decorator],
        statement: &Stmt,
    ) {
        let start = decorators
            .iter()
            .map(|decorator| usize::from(decorator.range().start()))
            .min()
            .unwrap_or_else(|| usize::from(range.start()));
        self.qualname.push(name.to_owned());
        self.scopes.push(ScopeInterval::new(
            start,
            usize::from(range.end()),
            self.qualname.join("."),
        ));
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

    fn record_type_param_ranges(&mut self, type_params: &TypeParams) {
        visit_type_param_expressions(type_params, |expression| {
            self.record_annotation_range(expression.range());
        });
    }
}

impl<'ast> Visitor<'ast> for AstFacts<'_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::AugAssign(assign) => {
                self.record_operator_tokens(
                    TextRange::new(assign.target.range().end(), assign.value.range().start()),
                    &[
                        "+=", "-=", "*=", "/=", "//=", "%=", "**=", "@=", "&=", "|=", "^=", "<<=",
                        ">>=",
                    ],
                );
            }
            Stmt::AnnAssign(assign) => {
                self.record_annotation_range(assign.annotation.range());
            }
            Stmt::FunctionDef(definition) => {
                self.record_function_annotation_ranges(definition);
                if let Some(type_params) = &definition.type_params {
                    self.record_type_param_ranges(type_params);
                }
            }
            Stmt::ClassDef(definition) => {
                if let Some(type_params) = &definition.type_params {
                    self.record_type_param_ranges(type_params);
                }
            }
            Stmt::TypeAlias(alias) => {
                if let Some(type_params) = &alias.type_params {
                    self.record_type_param_ranges(type_params);
                }
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
                &[
                    "+", "-", "*", "/", "//", "%", "**", "@", "&", "|", "^", "<<", ">>",
                ],
            ),
            Expr::UnaryOp(unary) => {
                self.record_operator_tokens(
                    TextRange::new(unary.range().start(), unary.operand.range().start()),
                    &["not", "+", "-", "~"],
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
        if let Pattern::MatchSingleton(singleton) = pattern
            && matches!(singleton.value, Singleton::True | Singleton::False)
        {
            self.record_operator_tokens(singleton.range(), &["True", "False"]);
        }
        visitor::walk_pattern(self, pattern);
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
    operator_imports: OperatorImports,
    in_pattern: bool,
    source: &'a str,
    line_index: &'a LineIndex,
    facts: &'a AstFacts<'a>,
    request: &'a AnalyzeRequest<'a>,
    cancelled: &'a F,
    cancelled_observed: bool,
    exception_handler_finality: Vec<bool>,
    exception_type_depth: usize,
    candidates: CandidatePrefix,
}

impl<'a, F: Fn() -> bool> AstCandidateCollector<'a, F> {
    fn collect(
        module: &'a ModModule,
        source: &'a str,
        line_index: &'a LineIndex,
        facts: &'a AstFacts<'a>,
        request: &'a AnalyzeRequest<'a>,
        cancelled: &'a F,
    ) -> Result<ProducerPrefix, AnalysisCancelled> {
        let mut collector = Self {
            in_pattern: false,
            operator_imports: if request
                .operators
                .contains(MutationOperator::OperatorFunction)
            {
                OperatorImports::build(module, cancelled)?
            } else {
                OperatorImports::default()
            },
            source,
            line_index,
            facts,
            request,
            cancelled,
            cancelled_observed: false,
            exception_handler_finality: Vec::new(),
            exception_type_depth: 0,
            candidates: CandidatePrefix::new(request.max_candidates),
        };
        for statement in &module.body {
            collector.visit_stmt(statement);
            if collector.cancelled_observed {
                return Err(AnalysisCancelled);
            }
        }
        Ok(collector.candidates.finish())
    }

    fn check_cancelled(&mut self) -> bool {
        if !self.cancelled_observed && (self.cancelled)() {
            self.cancelled_observed = true;
        }
        self.cancelled_observed
    }

    fn visit_exception_type(&mut self, expression: &'a Expr) {
        self.exception_type_depth += 1;
        self.visit_expr(expression);
        self.exception_type_depth -= 1;
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
        ) && retained_by_profile(&candidate, self.request.profile, self.facts)
        {
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
        let (replacement, operator) = match name {
            "any" | "all" if has_exact_positional_arguments(call, 1) => (
                if name == "any" { "all" } else { "any" },
                MutationOperator::CollectionAnyAll,
            ),
            "list" | "tuple"
                if self.exception_type_depth == 0 && has_at_most_one_positional_argument(call) =>
            {
                (
                    if name == "list" { "tuple" } else { "list" },
                    MutationOperator::CollectionListTuple,
                )
            }
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
        if !self.facts.resolves_builtin_pair(range, name, replacement) {
            return;
        }
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
                if let Some(replacement) =
                    append_to_insert_replacement(self.source, call, self.facts)
                {
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
                if let Some(replacement) =
                    insert_to_append_replacement(self.source, call, self.facts)
                {
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
            "split" | "rsplit" if has_supported_split_rsplit_arguments(call) => self.add_candidate(
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
                if let Some(replacement) =
                    extend_to_append_replacement(self.source, call, self.facts)
                {
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
                if let Some(replacement) =
                    mapping_get_to_subscript_replacement(self.source, call, self.facts)
                {
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
        if let Some(replacement) =
            subscript_to_mapping_get_replacement(self.source, subscript, self.facts)
        {
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
            .candidate_tokens_in_range(inner_range)
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
        if list.ctx != ExprContext::Load
            || self.exception_type_depth > 0
            || self.facts.contains_annotation_span(list.range())
        {
            return;
        }
        if let Some(replacement) = list_to_tuple_replacement(self.source, list, self.facts) {
            self.add_candidate(
                list.range(),
                replacement,
                MutationOperator::CollectionListTuple,
            );
        }
    }

    fn collect_tuple_literal(&mut self, tuple: &ExprTuple) {
        if tuple.ctx != ExprContext::Load
            || self.exception_type_depth > 0
            || self.facts.contains_annotation_span(tuple.range())
        {
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

    fn collect_raised_exception(&mut self, statement: &ruff_python_ast::StmtRaise) {
        let Some(primary) = statement.exc.as_deref() else {
            return;
        };
        let name = match primary {
            Expr::Name(name) => name,
            Expr::Call(call) => match call.func.as_ref() {
                Expr::Name(name) => name,
                _ => return,
            },
            _ => return,
        };
        for replacement in exception_pair_replacements(name.id.as_str()) {
            if self
                .facts
                .resolves_builtin_pair(name.range(), name.id.as_str(), replacement)
            {
                self.add_candidate(
                    name.range(),
                    (*replacement).to_owned(),
                    MutationOperator::ExceptionTypePair,
                );
            }
        }
    }

    fn collect_exception_handler(&mut self, except_handler: &ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
        self.collect_risky_exception_handler(except_handler, handler);
        self.collect_exception_type_pair(handler);
    }

    fn collect_exception_type_pair(
        &mut self,
        handler: &ruff_python_ast::ExceptHandlerExceptHandler,
    ) {
        let Some(Expr::Name(name)) = handler.type_.as_deref() else {
            return;
        };
        if !self.facts.resolves_builtin(name.range(), name.id.as_str()) {
            return;
        }
        for replacement in exception_pair_replacements(name.id.as_str()) {
            if self
                .facts
                .resolves_builtin_pair(name.range(), name.id.as_str(), replacement)
            {
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
            let except_range = identifier::except(except_handler, self.source);
            if self
                .request
                .operators
                .contains(MutationOperator::ExceptionBareToException)
                && self.facts.resolves_builtin(except_range, "Exception")
            {
                self.add_candidate(
                    except_range,
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
                if !self.facts.resolves_builtin(name.range(), name.id.as_str()) {
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
                    && self
                        .facts
                        .resolves_builtin_pair(name.range(), name.id.as_str(), replacement)
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
        if self
            .request
            .operators
            .contains(MutationOperator::ExceptionTupleAddPair)
        {
            let mut missing = HashSet::new();
            for name in &names {
                for replacement in exception_pair_replacements(name) {
                    if !names.iter().any(|member| member == replacement)
                        && self
                            .facts
                            .resolves_builtin(tuple.elts[0].range(), replacement)
                        && missing.insert(*replacement)
                        && let Some(tuple_replacement) =
                            tuple_add_replacement(self.source, tuple, self.facts, replacement)
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
                    tuple_remove_replacement(self.source, tuple, self.facts, index)
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

impl<'ast, F: Fn() -> bool> Visitor<'ast> for AstCandidateCollector<'ast, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if !self.check_cancelled() {
            if let Stmt::Raise(statement_raise) = statement {
                self.collect_raised_exception(statement_raise);
            }
            let Stmt::Try(try_statement) = statement else {
                visitor::walk_stmt(self, statement);
                return;
            };
            if try_statement.is_star {
                self.visit_body(&try_statement.body);
                for except_handler in &try_statement.handlers {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
                    if self.check_cancelled() {
                        return;
                    }
                    self.collect_exception_type_pair(handler);
                    if let Some(type_) = &handler.type_ {
                        self.visit_exception_type(type_);
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

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if !self.check_cancelled() && !self.facts.contains_annotation_span(expression.range()) {
            if !self.in_pattern
                && let Some((range, replacement)) = self.operator_imports.replacement(expression)
            {
                self.add_candidate(range, replacement, MutationOperator::OperatorFunction);
            }
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

    fn visit_except_handler(&mut self, except_handler: &'ast ruff_python_ast::ExceptHandler) {
        if !self.check_cancelled() {
            self.collect_exception_handler(except_handler);
        }
        if !self.check_cancelled() {
            let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
            if let Some(type_) = &handler.type_ {
                self.visit_exception_type(type_);
            }
            self.visit_body(&handler.body);
        }
    }

    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        if self.check_cancelled() {
            return;
        }
        let previous = std::mem::replace(&mut self.in_pattern, true);
        visitor::walk_pattern(self, pattern);
        self.in_pattern = previous;
    }
}

fn ast_candidates<'a, F: Fn() -> bool>(
    module: &'a ModModule,
    source: &'a str,
    line_index: &'a LineIndex,
    facts: &'a AstFacts<'a>,
    request: &'a AnalyzeRequest<'a>,
    cancelled: &'a F,
) -> Result<ProducerPrefix, AnalysisCancelled> {
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

fn has_supported_split_rsplit_arguments(call: &ExprCall) -> bool {
    has_supported_same_contract_arguments(call)
        && (call.arguments.args.len() >= 2
            || call.arguments.keywords.iter().any(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|argument| argument.as_str() == "maxsplit")
            }))
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

fn append_to_insert_replacement(
    source: &str,
    call: &ExprCall,
    facts: &AstFacts<'_>,
) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let argument = parenthesized_argument_range(call, call.arguments.args.first()?, facts);
    replace_within_call(
        source,
        call,
        [
            (attribute.attr.range(), "insert"),
            (TextRange::new(argument.start(), argument.start()), "0, "),
        ],
    )
}

fn insert_to_append_replacement(
    source: &str,
    call: &ExprCall,
    facts: &AstFacts<'_>,
) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let expression = call.arguments.args.get(1)?;
    if matches!(expression, Expr::Yield(_) | Expr::YieldFrom(_)) {
        return None;
    }
    let first = parenthesized_argument_range(call, call.arguments.args.first()?, facts);
    let expression = parenthesized_argument_range(call, expression, facts);
    let separator_range = TextRange::new(first.end(), expression.start());
    let comma = facts
        .candidate_tokens_in_range(separator_range)
        .iter()
        .find(|token| token.kind() == TokenKind::Comma)?;
    let comma_and_following_whitespace =
        comma_and_following_whitespace_range(source, comma.range(), expression.start())?;
    replace_within_call(
        source,
        call,
        [
            (attribute.attr.range(), "append"),
            (first, ""),
            (comma_and_following_whitespace, ""),
        ],
    )
}

fn append_to_extend_replacement(source: &str, call: &ExprCall) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let inner = call.arguments.inner_range();
    replace_within_call(
        source,
        call,
        [
            (attribute.attr.range(), "extend"),
            (TextRange::new(inner.start(), inner.start()), "["),
            (TextRange::new(inner.end(), inner.end()), "]"),
        ],
    )
}

fn extend_to_append_replacement(
    source: &str,
    call: &ExprCall,
    facts: &AstFacts<'_>,
) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let Expr::List(list) = call.arguments.args.first()? else {
        return None;
    };
    if list.ctx != ExprContext::Load || list.elts.len() != 1 || list.elts[0].is_starred_expr() {
        return None;
    }
    let tokens = facts.candidate_tokens_in_range(list.range());
    let opening = tokens
        .iter()
        .find(|token| token.kind() == TokenKind::Lsqb)?;
    let closing = tokens
        .iter()
        .rfind(|token| token.kind() == TokenKind::Rsqb)?;
    let trailing_comma = facts
        .candidate_tokens_in_range(TextRange::new(
            list.elts[0].range().end(),
            closing.range().start(),
        ))
        .iter()
        .find(|token| token.kind() == TokenKind::Comma);
    if let Some(trailing_comma) = trailing_comma {
        replace_within_call(
            source,
            call,
            [
                (attribute.attr.range(), "append"),
                (opening.range(), ""),
                (closing.range(), ""),
                (trailing_comma.range(), ""),
            ],
        )
    } else {
        replace_within_call(
            source,
            call,
            [
                (attribute.attr.range(), "append"),
                (opening.range(), ""),
                (closing.range(), ""),
            ],
        )
    }
}

fn mapping_get_to_subscript_replacement(
    source: &str,
    call: &ExprCall,
    facts: &AstFacts<'_>,
) -> Option<String> {
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let receiver_range = ruff_python_ast::token::parenthesized_range(
        attribute.value.as_ref().into(),
        attribute.into(),
        facts.tokens.expect("parser tokens are set"),
    )
    .unwrap_or_else(|| attribute.value.range());
    let receiver = source_text(source, receiver_range)?;
    let key = source_text(source, call.arguments.inner_range())?;
    Some(format!("{receiver}[{key}]"))
}

fn subscript_to_mapping_get_replacement(
    source: &str,
    subscript: &ExprSubscript,
    facts: &AstFacts<'_>,
) -> Option<String> {
    let receiver_range = ruff_python_ast::token::parenthesized_range(
        subscript.value.as_ref().into(),
        subscript.into(),
        facts.tokens.expect("parser tokens are set"),
    )
    .unwrap_or_else(|| subscript.value.range());
    let receiver = source_text(source, receiver_range)?;
    let tokens = facts.candidate_tokens_in_range(subscript.range());
    let opening = tokens.iter().find(|token| {
        token.kind() == TokenKind::Lsqb && token.range().start() >= receiver_range.end()
    })?;
    let closing = tokens
        .iter()
        .rfind(|token| token.kind() == TokenKind::Rsqb)?;
    let key = source_text(
        source,
        TextRange::new(opening.range().end(), closing.range().start()),
    )?;
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
    replace_within_call(source, call, [(attribute.attr.range(), replacement)])
}

fn parenthesized_argument_range(
    call: &ExprCall,
    argument: &Expr,
    facts: &AstFacts<'_>,
) -> TextRange {
    ruff_python_ast::token::parenthesized_range(
        argument.into(),
        (&call.arguments).into(),
        facts.tokens.expect("parser tokens are set"),
    )
    .unwrap_or_else(|| argument.range())
}

fn comma_and_following_whitespace_range(
    source: &str,
    comma: TextRange,
    expression_start: ruff_text_size::TextSize,
) -> Option<TextRange> {
    let end = usize::from(expression_start);
    let suffix = source.get(usize::from(comma.end())..end)?;
    let whitespace_end = suffix
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(suffix.len(), |(index, _)| index);
    let length = comma.len() + ruff_text_size::TextSize::try_from(whitespace_end).ok()?;
    Some(TextRange::at(comma.start(), length))
}

fn replace_within_call<const N: usize>(
    source: &str,
    call: &ExprCall,
    edits: [(TextRange, &str); N],
) -> Option<String> {
    let call_range = call.range();
    let call_start = usize::from(call_range.start());
    let call_end = usize::from(call_range.end());
    let mut replacement = source.get(call_start..call_end)?.to_owned();
    let mut edits = edits;
    edits.sort_unstable_by_key(|(range, _)| range.start());
    for (range, text) in edits.into_iter().rev() {
        let start = usize::from(range.start()).checked_sub(call_start)?;
        let end = usize::from(range.end()).checked_sub(call_start)?;
        if end > replacement.len() {
            return None;
        }
        replacement.replace_range(start..end, text);
    }
    Some(replacement)
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
    facts: &AstFacts<'_>,
) -> Option<String> {
    let range = list.range();
    let literal = source_text(source, range)?;
    let contents = literal.strip_prefix('[')?.strip_suffix(']')?;
    if list.elts.len() != 1 {
        return Some(format!("({contents})"));
    }
    let comma_range = TextRange::new(list.elts[0].range().end(), range.end());
    if has_comma_after_element(facts, comma_range) {
        return Some(format!("({contents})"));
    }
    Some(format!("({contents},)"))
}

fn has_comma_after_element(facts: &AstFacts<'_>, range: TextRange) -> bool {
    facts
        .candidate_tokens_in_range(range)
        .iter()
        .any(|token| token.kind() == TokenKind::Comma)
}

fn tuple_to_list_replacement(source: &str, tuple: &ExprTuple) -> Option<String> {
    let literal = source_text(source, tuple.range())?;
    let contents = if tuple.parenthesized {
        literal.strip_prefix('(')?.strip_suffix(')')?
    } else {
        literal
    };
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
            (EXCEPTION_NAMES.contains(&name) && facts.resolves_builtin(element.range(), name))
                .then_some(name)
        })
        .collect()
}

fn tuple_add_replacement(
    source: &str,
    tuple: &ExprTuple,
    facts: &AstFacts<'_>,
    name: &str,
) -> Option<String> {
    let literal = source_text(source, tuple.range())?;
    if !literal.starts_with('(') || !literal.ends_with(')') {
        return None;
    }
    let close_start = usize::from(tuple.range().end()).checked_sub(1)?;
    let has_trailing_comma = facts
        .candidate_tokens_in_range(tuple.range())
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
    facts: &AstFacts<'_>,
    index: usize,
) -> Option<String> {
    let literal = source_text(source, tuple.range())?;
    let tuple_start = usize::from(tuple.range().start());
    let tuple_end = usize::from(tuple.range().end());
    let element = tuple.elts.get(index)?;
    let element_range = element.range();
    let element_start = usize::from(element_range.start());
    let element_end = usize::from(element_range.end());
    let commas: Vec<_> = facts
        .candidate_tokens_in_range(tuple.range())
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

#[cfg(test)]
#[path = "nested_try_oracle_tests.rs"]
mod nested_try_oracle_tests;

#[cfg(test)]
#[path = "nested_match_exit_oracle_tests.rs"]
mod nested_match_exit_oracle_tests;

#[cfg(test)]
#[path = "multiple_handler_join_oracle_tests.rs"]
mod multiple_handler_join_oracle_tests;

#[cfg(test)]
#[path = "compound_pattern_guard_oracle_tests.rs"]
mod compound_pattern_guard_oracle_tests;

#[cfg(test)]
#[path = "except_star_flow_oracle_tests.rs"]
mod except_star_flow_oracle_tests;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct KnownImports {
    direct: HashMap<String, String>,
    modules: HashMap<String, String>,
    type_vars: HashSet<String>,
}

impl KnownImports {
    fn intersection(states: impl IntoIterator<Item = Self>) -> Option<Self> {
        let mut states = states.into_iter();
        let mut intersection = states.next()?;
        for state in states {
            intersection
                .direct
                .retain(|name, resolved| state.direct.get(name) == Some(resolved));
            intersection
                .modules
                .retain(|name, resolved| state.modules.get(name) == Some(resolved));
            intersection
                .type_vars
                .retain(|name| state.type_vars.contains(name));
        }
        Some(intersection)
    }

    fn invalidate(&mut self, name: &str) {
        self.direct.remove(name);
        self.modules.remove(name);
        self.type_vars.remove(name);
    }

    fn enter_type_params(&mut self, type_params: &TypeParams) {
        for parameter in type_params.iter() {
            let name = match parameter {
                TypeParam::TypeVar(parameter) => parameter.name.as_str(),
                TypeParam::TypeVarTuple(parameter) => parameter.name.as_str(),
                TypeParam::ParamSpec(parameter) => parameter.name.as_str(),
            };
            self.invalidate(name);
            self.type_vars.insert(name.to_owned());
        }
    }

    fn copy_name_from(&mut self, source: &Self, name: &str) {
        self.invalidate(name);
        if let Some(resolved) = source.direct.get(name) {
            self.direct.insert(name.to_owned(), resolved.clone());
        }
        if let Some(resolved) = source.modules.get(name) {
            self.modules.insert(name.to_owned(), resolved.clone());
        }
        if source.type_vars.contains(name) {
            self.type_vars.insert(name.to_owned());
        }
    }

    fn invalidate_target(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) => self.invalidate(name.id.as_str()),
            Expr::List(list) => {
                for element in &list.elts {
                    self.invalidate_target(element);
                }
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.invalidate_target(element);
                }
            }
            Expr::Starred(starred) => self.invalidate_target(starred.value.as_ref()),
            _ => {}
        }
    }

    fn mark_type_vars(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) => {
                self.type_vars.insert(name.id.as_str().to_owned());
            }
            Expr::List(list) => {
                for element in &list.elts {
                    self.mark_type_vars(element);
                }
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.mark_type_vars(element);
                }
            }
            Expr::Starred(starred) => self.mark_type_vars(starred.value.as_ref()),
            _ => {}
        }
    }

    fn transfer_import(&mut self, import: &ruff_python_ast::StmtImport) {
        for alias in &import.names {
            let name = alias.name.as_str();
            let local = alias.asname.as_ref().map_or_else(
                || name.split('.').next().unwrap_or(name),
                ruff_python_ast::Identifier::as_str,
            );
            self.invalidate(local);
            if matches!(name, "typing" | "collections.abc") {
                let resolved = if alias.asname.is_some() { name } else { local };
                self.modules.insert(local.to_owned(), resolved.to_owned());
            }
        }
    }

    fn transfer_import_from(&mut self, import: &ruff_python_ast::StmtImportFrom) {
        let module_name = (import.level == 0)
            .then(|| {
                import
                    .module
                    .as_ref()
                    .map(ruff_python_ast::Identifier::as_str)
            })
            .flatten();
        for alias in &import.names {
            let imported = alias.name.as_str();
            if imported == "*" {
                self.direct.clear();
                self.type_vars.clear();
                if !matches!(module_name, Some("typing" | "collections.abc")) {
                    self.modules.clear();
                }
                continue;
            }
            let local = alias
                .asname
                .as_ref()
                .map_or(imported, ruff_python_ast::Identifier::as_str);
            self.invalidate(local);
            if let Some(module_name) = module_name
                && matches!(module_name, "typing" | "collections.abc")
                && is_known_type_name(imported)
            {
                self.direct
                    .insert(local.to_owned(), format!("{module_name}.{imported}"));
            }
        }
    }

    fn transfer_assign(&mut self, targets: &[Expr], value: &Expr) {
        let is_type_var = is_type_var_call(value, self);
        for target in targets {
            self.invalidate_target(target);
        }
        if is_type_var {
            for target in targets {
                self.mark_type_vars(target);
            }
        }
    }

    fn transfer_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Import(import) => self.transfer_import(import),
            Stmt::ImportFrom(import) => self.transfer_import_from(import),
            Stmt::Assign(assign) => self.transfer_assign(&assign.targets, assign.value.as_ref()),
            Stmt::AnnAssign(assign) => {
                if let Some(value) = assign.value.as_deref() {
                    self.transfer_assign(std::slice::from_ref(assign.target.as_ref()), value);
                }
            }
            Stmt::AugAssign(assign) => self.invalidate_target(assign.target.as_ref()),
            Stmt::Delete(delete) => {
                for target in &delete.targets {
                    self.invalidate_target(target);
                }
            }
            Stmt::TypeAlias(alias) => self.invalidate_target(alias.name.as_ref()),
            Stmt::FunctionDef(definition) => self.invalidate(definition.name.as_str()),
            Stmt::ClassDef(definition) => self.invalidate(definition.name.as_str()),
            _ => {}
        }
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

fn collect_target_names(target: &Expr, names: &mut HashSet<String>) {
    match target {
        Expr::Name(name) => {
            names.insert(name.id.as_str().to_owned());
        }
        Expr::List(list) => {
            for element in &list.elts {
                collect_target_names(element, names);
            }
        }
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                collect_target_names(element, names);
            }
        }
        Expr::Starred(starred) => collect_target_names(starred.value.as_ref(), names),
        _ => {}
    }
}

#[derive(Default)]
struct FunctionLocalCollector {
    locals: HashSet<String>,
    external: HashSet<String>,
    unknown_wildcard: bool,
}

#[derive(Default)]
struct ClassExternalBindings {
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
}

impl ClassExternalBindings {
    fn collect(statements: &[Stmt]) -> Self {
        let mut collector = Self::default();
        for statement in statements {
            collector.visit_stmt(statement);
        }
        collector
    }

    fn copy_for_fallback(
        &self,
        source: &KnownImports,
        target: &mut KnownImports,
        parent_scope: ScopeKind,
    ) {
        let names = match parent_scope {
            ScopeKind::Module => &self.globals,
            ScopeKind::Function | ScopeKind::Class => &self.nonlocals,
        };
        for name in names {
            target.copy_name_from(source, name);
        }
    }
}

impl<'ast> Visitor<'ast> for ClassExternalBindings {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            Stmt::Global(global) => {
                self.globals
                    .extend(global.names.iter().map(|name| name.as_str().to_owned()));
            }
            Stmt::Nonlocal(nonlocal) => {
                self.nonlocals
                    .extend(nonlocal.names.iter().map(|name| name.as_str().to_owned()));
            }
            _ => {}
        }
        visitor::walk_stmt(self, statement);
    }
}

impl FunctionLocalCollector {
    fn collect(definition: &ruff_python_ast::StmtFunctionDef) -> HashSet<String> {
        let mut collector = Self::scan(&definition.body);
        for parameter in &definition.parameters {
            collector
                .locals
                .insert(parameter.name().as_str().to_owned());
        }
        collector
            .locals
            .retain(|name| !collector.external.contains(name));
        collector.locals
    }

    fn scan(statements: &[Stmt]) -> Self {
        let mut collector = Self::default();
        for statement in statements {
            collector.visit_stmt(statement);
        }
        collector
    }

    fn record_target(&mut self, target: &Expr) {
        collect_target_names(target, &mut self.locals);
    }

    fn record_import(&mut self, import: &ruff_python_ast::StmtImport) {
        for alias in &import.names {
            let imported = alias.name.as_str();
            let local = alias.asname.as_ref().map_or_else(
                || imported.split('.').next().unwrap_or(imported),
                ruff_python_ast::Identifier::as_str,
            );
            self.locals.insert(local.to_owned());
        }
    }

    fn record_import_from(&mut self, import: &ruff_python_ast::StmtImportFrom) {
        let supported_module = import.level == 0
            && matches!(
                import
                    .module
                    .as_ref()
                    .map(ruff_python_ast::Identifier::as_str),
                Some("typing" | "collections.abc")
            );
        for alias in &import.names {
            if alias.name.as_str() == "*" {
                self.unknown_wildcard |= !supported_module;
                continue;
            }
            self.locals.insert(
                alias
                    .asname
                    .as_ref()
                    .map_or(alias.name.as_str(), ruff_python_ast::Identifier::as_str)
                    .to_owned(),
            );
        }
    }
}

impl<'ast> Visitor<'ast> for FunctionLocalCollector {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::FunctionDef(definition) => {
                self.locals.insert(definition.name.as_str().to_owned());
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = &definition.type_params {
                    self.visit_type_params(type_params);
                }
                self.visit_parameters(&definition.parameters);
                if let Some(returns) = &definition.returns {
                    self.visit_annotation(returns);
                }
                return;
            }
            Stmt::ClassDef(definition) => {
                self.locals.insert(definition.name.as_str().to_owned());
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = &definition.type_params {
                    self.visit_type_params(type_params);
                }
                if let Some(arguments) = &definition.arguments {
                    self.visit_arguments(arguments);
                }
                return;
            }
            Stmt::Import(import) => self.record_import(import),
            Stmt::ImportFrom(import) => self.record_import_from(import),
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.record_target(target);
                }
            }
            Stmt::AnnAssign(assign) => self.record_target(assign.target.as_ref()),
            Stmt::AugAssign(assign) => self.record_target(assign.target.as_ref()),
            Stmt::Delete(delete) => {
                for target in &delete.targets {
                    self.record_target(target);
                }
            }
            Stmt::TypeAlias(alias) => self.record_target(alias.name.as_ref()),
            Stmt::For(statement_for) => self.record_target(statement_for.target.as_ref()),
            Stmt::With(statement_with) => {
                for item in &statement_with.items {
                    if let Some(target) = &item.optional_vars {
                        self.record_target(target);
                    }
                }
            }
            Stmt::Global(global) => {
                self.external
                    .extend(global.names.iter().map(|name| name.as_str().to_owned()));
            }
            Stmt::Nonlocal(nonlocal) => {
                self.external
                    .extend(nonlocal.names.iter().map(|name| name.as_str().to_owned()));
            }
            _ => {}
        }
        visitor::walk_stmt(self, statement);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        match expression {
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    for default in parameters
                        .iter_non_variadic_params()
                        .filter_map(ruff_python_ast::ParameterWithDefault::default)
                    {
                        self.visit_expr(default);
                    }
                }
                return;
            }
            Expr::Named(named) => self.record_target(named.target.as_ref()),
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }

    fn visit_except_handler(&mut self, except_handler: &'ast ruff_python_ast::ExceptHandler) {
        let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
        if let Some(name) = &handler.name {
            self.locals.insert(name.as_str().to_owned());
        }
        visitor::walk_except_handler(self, except_handler);
    }

    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        match pattern {
            Pattern::MatchMapping(mapping) => {
                if let Some(rest) = &mapping.rest {
                    self.locals.insert(rest.as_str().to_owned());
                }
            }
            Pattern::MatchStar(star) => {
                if let Some(name) = &star.name {
                    self.locals.insert(name.as_str().to_owned());
                }
            }
            Pattern::MatchAs(as_pattern) => {
                if let Some(name) = &as_pattern.name {
                    self.locals.insert(name.as_str().to_owned());
                }
            }
            _ => {}
        }
        visitor::walk_pattern(self, pattern);
    }
}

struct NamedBindingInvalidator<'imports> {
    imports: &'imports mut KnownImports,
}

impl<'imports> NamedBindingInvalidator<'imports> {
    fn visit(imports: &'imports mut KnownImports, expression: &Expr) {
        Self { imports }.visit_expr(expression);
    }

    fn visit_statement(imports: &'imports mut KnownImports, statement: &Stmt) {
        visitor::walk_stmt(&mut Self { imports }, statement);
    }
}

impl<'ast> Visitor<'ast> for NamedBindingInvalidator<'_> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        match expression {
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    for default in parameters
                        .iter_non_variadic_params()
                        .filter_map(ruff_python_ast::ParameterWithDefault::default)
                    {
                        self.visit_expr(default);
                    }
                }
                return;
            }
            Expr::Named(named) => self.imports.invalidate_target(named.target.as_ref()),
            _ => {}
        }
        visitor::walk_expr(self, expression);
    }
}

struct PatternBindingFlow {
    matched: Option<KnownImports>,
    failed: Option<KnownImports>,
}

impl PatternBindingFlow {
    fn capture(mut self, name: &str) -> Self {
        if let Some(imports) = &mut self.matched {
            imports.invalidate(name);
        }
        self
    }
}

fn merge_pattern_states(
    left: Option<KnownImports>,
    right: Option<KnownImports>,
) -> Option<KnownImports> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => KnownImports::intersection([left, right]),
    }
}

fn refutable_pattern_test(imports: &KnownImports) -> PatternBindingFlow {
    PatternBindingFlow {
        matched: Some(imports.clone()),
        failed: Some(imports.clone()),
    }
}

fn sequence_pattern_bindings<'pattern>(
    imports: &KnownImports,
    patterns: impl IntoIterator<Item = &'pattern Pattern>,
) -> PatternBindingFlow {
    let mut flow = refutable_pattern_test(imports);
    for pattern in patterns {
        let Some(matched) = flow.matched.take() else {
            break;
        };
        let child = pattern_binding_flow(&matched, pattern);
        flow.matched = child.matched;
        flow.failed = merge_pattern_states(flow.failed, child.failed);
    }
    flow
}

fn pattern_binding_flow(imports: &KnownImports, pattern: &Pattern) -> PatternBindingFlow {
    match pattern {
        Pattern::MatchValue(_) | Pattern::MatchSingleton(_) => refutable_pattern_test(imports),
        Pattern::MatchSequence(sequence) => sequence_pattern_bindings(imports, &sequence.patterns),
        Pattern::MatchMapping(mapping) => {
            let flow = sequence_pattern_bindings(imports, &mapping.patterns);
            match &mapping.rest {
                Some(rest) => flow.capture(rest.as_str()),
                None => flow,
            }
        }
        Pattern::MatchClass(class) => sequence_pattern_bindings(
            imports,
            class.arguments.patterns.iter().chain(
                class
                    .arguments
                    .keywords
                    .iter()
                    .map(|keyword| &keyword.pattern),
            ),
        ),
        Pattern::MatchStar(star) => {
            let flow = PatternBindingFlow {
                matched: Some(imports.clone()),
                failed: None,
            };
            match &star.name {
                Some(name) => flow.capture(name.as_str()),
                None => flow,
            }
        }
        Pattern::MatchAs(as_pattern) => {
            let flow = match &as_pattern.pattern {
                Some(child) => pattern_binding_flow(imports, child),
                None => PatternBindingFlow {
                    matched: Some(imports.clone()),
                    failed: None,
                },
            };
            match &as_pattern.name {
                Some(name) => flow.capture(name.as_str()),
                None => flow,
            }
        }
        Pattern::MatchOr(or_pattern) => {
            let mut matched = None;
            let mut failed = None;
            for arm in &or_pattern.patterns {
                let arm = pattern_binding_flow(imports, arm);
                matched = merge_pattern_states(matched, arm.matched);
                failed = merge_pattern_states(failed, arm.failed);
            }
            PatternBindingFlow { matched, failed }
        }
    }
}

#[derive(Default)]
struct ControlFlowExits {
    fallthrough: Option<KnownImports>,
    breaks: Vec<KnownImports>,
    continues: Vec<KnownImports>,
    terminates: Vec<KnownImports>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExitCategory {
    Fallthrough,
    Break,
    Continue,
    Terminate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScopeKind {
    Module,
    Function,
    Class,
}

impl ControlFlowExits {
    fn fallthrough(imports: KnownImports) -> Self {
        Self {
            fallthrough: Some(imports),
            ..Self::default()
        }
    }

    fn merge_abrupt(&mut self, other: Self) {
        self.breaks.extend(other.breaks);
        self.continues.extend(other.continues);
        self.terminates.extend(other.terminates);
    }
}

struct AnnotationSite<'ast> {
    annotation: &'ast Expr,
    symbol: Option<String>,
    imports: KnownImports,
    #[cfg(test)]
    scope_kind: ScopeKind,
}

struct AnnotationCollector<'ast> {
    annotations: Vec<AnnotationSite<'ast>>,
    imports: KnownImports,
    class_body_fallback: Option<KnownImports>,
    class_external_bindings: Option<ClassExternalBindings>,
    class_parent_scope: Option<ScopeKind>,
    scope_kind: ScopeKind,
    qualname: Vec<String>,
    record_annotations: bool,
    #[cfg(test)]
    marker_projection: Option<BindingFlowMarkerProjection>,
    #[cfg(test)]
    handler_exit_projection: Option<BindingFlowHandlerExitProjection>,
    #[cfg(test)]
    try_exit_projection: Option<BindingFlowTryExitProjection>,
    #[cfg(test)]
    test_mutation: Option<BindingFlowTestMutation>,
}

#[cfg(test)]
struct BindingFlowMarkerProjection {
    source: String,
    marker: Range<usize>,
    line_start: usize,
    entry: Option<((usize, usize), KnownImports)>,
}

#[cfg(test)]
struct BindingFlowHandlerExitProjection {
    source: String,
    marker: Range<usize>,
    line_start: usize,
    category: ExitCategory,
    matching_handlers: usize,
    matched_handler: Option<(usize, usize)>,
    states: Vec<KnownImports>,
}

#[cfg(test)]
struct BindingFlowTryExitProjection {
    marker: Range<usize>,
    matching_tries: usize,
    matched_try: Option<(usize, usize)>,
    exits: Option<BindingFlowTestSnapshot>,
}

#[cfg(test)]
fn binding_flow_marker_rank(
    source: &str,
    marker: &Range<usize>,
    line_start: usize,
    range: TextRange,
) -> Option<(usize, usize)> {
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    let contains = start <= marker.start && marker.end <= end;
    let trailing_comment = line_start <= start
        && end <= marker.start
        && source
            .get(end..marker.end)
            .is_some_and(|suffix| suffix.trim_start_matches([' ', '\t']).starts_with('#'));
    (contains || trailing_comment)
        .then(|| (if contains { 0 } else { marker.start - end }, end - start))
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BindingFlowTestMutation {
    BindHandlerTargetBeforeType,
    OmitHandlerFallthroughCleanup,
    OmitHandlerBreakCleanup,
    OmitHandlerContinueCleanup,
    OmitHandlerTerminateCleanup,
    UsePrePatternFailureEnvironment,
    UseMatchedPatternFailureEnvironment,
    KeepLastOrPatternFailure,
    OverbroadPatternCleanup,
    UsePreGuardFailureEnvironment,
    DropRefutableUnmatched,
    FlattenMatchAbruptToFallthrough,
    OmitLoopContinueBackEdge,
    KeepFirstHandlerOnly,
    KeepLastHandlerOnly,
    FlattenHandlerAbruptToFallthrough,
    DropBodyTerminates,
}

impl<'ast> AnnotationCollector<'ast> {
    fn collect(module: &'ast ModModule) -> Vec<AnnotationSite<'ast>> {
        let mut collector = Self::empty();
        collector.visit_suite(&module.body);
        collector.annotations
    }

    fn empty() -> Self {
        Self {
            annotations: Vec::new(),
            imports: KnownImports::default(),
            class_body_fallback: None,
            class_external_bindings: None,
            class_parent_scope: None,
            scope_kind: ScopeKind::Module,
            qualname: Vec::new(),
            record_annotations: true,
            #[cfg(test)]
            marker_projection: None,
            #[cfg(test)]
            handler_exit_projection: None,
            #[cfg(test)]
            try_exit_projection: None,
            #[cfg(test)]
            test_mutation: None,
        }
    }

    #[cfg(test)]
    fn capture_marker_entry(&mut self, range: TextRange) {
        let Some(projection) = &mut self.marker_projection else {
            return;
        };
        if let Some(rank) = binding_flow_marker_rank(
            &projection.source,
            &projection.marker,
            projection.line_start,
            range,
        ) && projection
            .entry
            .as_ref()
            .is_none_or(|(previous_rank, _)| rank < *previous_rank)
        {
            projection.entry = Some((rank, self.imports.clone()));
        }
    }

    #[cfg(test)]
    fn capture_compound_header_entry(&mut self, range: TextRange, body: &[Stmt]) {
        let end = body
            .first()
            .map_or(range.end(), |statement| statement.range().start());
        self.capture_marker_entry(TextRange::new(range.start(), end));
    }

    #[cfg(test)]
    fn capture_handler_exit(
        &mut self,
        handler_range: TextRange,
        body: &[Stmt],
        exits: &ControlFlowExits,
    ) {
        let Some(projection) = &mut self.handler_exit_projection else {
            return;
        };
        let marker_selects_handler = body.iter().any(|statement| {
            binding_flow_marker_rank(
                &projection.source,
                &projection.marker,
                projection.line_start,
                statement.range(),
            )
            .is_some()
        });
        if !marker_selects_handler {
            return;
        }
        let handler_key = (
            usize::from(handler_range.start()),
            usize::from(handler_range.end()),
        );
        if projection.matched_handler != Some(handler_key) {
            projection.matching_handlers += 1;
            projection.matched_handler = Some(handler_key);
        }
        projection.states.clear();
        match projection.category {
            ExitCategory::Fallthrough => {
                projection.states.extend(exits.fallthrough.iter().cloned());
            }
            ExitCategory::Break => projection.states.extend(exits.breaks.iter().cloned()),
            ExitCategory::Continue => projection.states.extend(exits.continues.iter().cloned()),
            ExitCategory::Terminate => projection.states.extend(exits.terminates.iter().cloned()),
        }
    }

    #[cfg(test)]
    fn capture_try_exit(&mut self, range: TextRange, exits: &ControlFlowExits) {
        let Some(projection) = &mut self.try_exit_projection else {
            return;
        };
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        if start <= projection.marker.start && projection.marker.end <= end {
            let try_key = (start, end);
            if projection.matched_try != Some(try_key) {
                projection.matching_tries += 1;
                projection.matched_try = Some(try_key);
            }
            projection.exits = Some(normalize_binding_flow_exits(exits));
        }
    }

    #[cfg(test)]
    fn mutate_multiple_handler_exits(&self, exits: &mut ControlFlowExits) {
        if self.test_mutation != Some(BindingFlowTestMutation::FlattenHandlerAbruptToFallthrough) {
            return;
        }
        let mut states = exits.fallthrough.take().into_iter().collect::<Vec<_>>();
        states.append(&mut exits.breaks);
        states.append(&mut exits.continues);
        states.append(&mut exits.terminates);
        exits.fallthrough = KnownImports::intersection(states);
    }

    #[cfg(test)]
    fn includes_multiple_handler(&self, index: usize, count: usize) -> bool {
        match self.test_mutation {
            Some(BindingFlowTestMutation::KeepFirstHandlerOnly) => index == 0,
            Some(BindingFlowTestMutation::KeepLastHandlerOnly) => index + 1 == count,
            _ => true,
        }
    }

    #[cfg(not(test))]
    fn includes_multiple_handler(_index: usize, _count: usize) -> bool {
        true
    }

    #[cfg(test)]
    fn merge_try_body_terminates(
        &self,
        joined: &mut ControlFlowExits,
        terminates: Vec<KnownImports>,
    ) {
        if self.test_mutation != Some(BindingFlowTestMutation::DropBodyTerminates) {
            joined.terminates.extend(terminates);
        }
    }

    #[cfg(not(test))]
    fn merge_try_body_terminates(joined: &mut ControlFlowExits, terminates: Vec<KnownImports>) {
        joined.terminates.extend(terminates);
    }

    fn try_handler_imports(mut incoming: KnownImports, body: &[Stmt]) -> KnownImports {
        let try_effects = FunctionLocalCollector::scan(body);
        if try_effects.unknown_wildcard {
            incoming.direct.clear();
            incoming.modules.clear();
            incoming.type_vars.clear();
        }
        for name in try_effects.locals {
            incoming.invalidate(&name);
        }
        incoming
    }

    fn symbol(&self) -> Option<String> {
        (!self.qualname.is_empty()).then(|| self.qualname.join("."))
    }

    fn record_type_params(&mut self, type_params: &'ast TypeParams) {
        visit_type_param_expressions(type_params, |expression| self.record(expression));
    }

    fn in_type_param_scope(
        &mut self,
        name: &str,
        type_params: Option<&'ast TypeParams>,
        visit: impl FnOnce(&mut Self),
    ) {
        let outer = self.imports.clone();
        if let Some(type_params) = type_params {
            self.imports.enter_type_params(type_params);
        }
        self.qualname.push(name.to_owned());
        visit(self);
        self.qualname.pop();
        self.imports = outer;
    }

    fn visit_function_header(&mut self, definition: &'ast ruff_python_ast::StmtFunctionDef) {
        for decorator in &definition.decorator_list {
            NamedBindingInvalidator::visit(&mut self.imports, &decorator.expression);
        }
        for default in definition
            .parameters
            .iter_non_variadic_params()
            .filter_map(ruff_python_ast::ParameterWithDefault::default)
        {
            NamedBindingInvalidator::visit(&mut self.imports, default);
        }
        self.in_type_param_scope(
            definition.name.as_str(),
            definition.type_params.as_deref(),
            |collector| {
                if let Some(type_params) = definition.type_params.as_deref() {
                    collector.record_type_params(type_params);
                }
                for parameter in &definition.parameters {
                    if let Some(annotation) = parameter.annotation() {
                        collector.record(annotation);
                    }
                }
                if let Some(annotation) = definition.returns.as_deref() {
                    collector.record(annotation);
                }
            },
        );
        for parameter in &definition.parameters {
            if let Some(annotation) = parameter.annotation() {
                NamedBindingInvalidator::visit(&mut self.imports, annotation);
            }
        }
        if let Some(annotation) = definition.returns.as_deref() {
            NamedBindingInvalidator::visit(&mut self.imports, annotation);
        }
    }

    fn visit_class_header(&mut self, definition: &'ast ruff_python_ast::StmtClassDef) {
        for decorator in &definition.decorator_list {
            NamedBindingInvalidator::visit(&mut self.imports, &decorator.expression);
        }
        self.in_type_param_scope(
            definition.name.as_str(),
            definition.type_params.as_deref(),
            |collector| {
                if let Some(type_params) = definition.type_params.as_deref() {
                    collector.record_type_params(type_params);
                }
                if let Some(arguments) = &definition.arguments {
                    for argument in &arguments.args {
                        NamedBindingInvalidator::visit(&mut collector.imports, argument);
                    }
                    for keyword in &arguments.keywords {
                        NamedBindingInvalidator::visit(&mut collector.imports, &keyword.value);
                    }
                }
            },
        );
        if let Some(arguments) = &definition.arguments {
            for argument in &arguments.args {
                NamedBindingInvalidator::visit(&mut self.imports, argument);
            }
            for keyword in &arguments.keywords {
                NamedBindingInvalidator::visit(&mut self.imports, &keyword.value);
            }
        }
    }

    fn visit_type_alias(
        &mut self,
        statement: &'ast Stmt,
        alias: &'ast ruff_python_ast::StmtTypeAlias,
    ) -> ControlFlowExits {
        let Expr::Name(name) = alias.name.as_ref() else {
            unreachable!("a parsed type alias name is always an identifier");
        };
        self.in_type_param_scope(
            name.id.as_str(),
            alias.type_params.as_deref(),
            |collector| {
                if let Some(type_params) = alias.type_params.as_deref() {
                    collector.record_type_params(type_params);
                }
                collector.record(alias.value.as_ref());
            },
        );
        self.imports.transfer_statement(statement);
        ControlFlowExits::fallthrough(self.imports.clone())
    }

    fn record(&mut self, annotation: &'ast Expr) {
        if !self.record_annotations {
            return;
        }
        self.annotations.push(AnnotationSite {
            annotation,
            symbol: self.symbol(),
            imports: self.imports.clone(),
            #[cfg(test)]
            scope_kind: self.scope_kind,
        });
    }

    fn visit_suite(&mut self, statements: &'ast [Stmt]) {
        let _ = self.visit_suite_flow(statements);
    }

    fn visit_suite_from(
        &mut self,
        imports: KnownImports,
        statements: &'ast [Stmt],
    ) -> ControlFlowExits {
        self.imports = imports;
        self.visit_suite_flow(statements)
    }

    fn visit_suite_flow(&mut self, statements: &'ast [Stmt]) -> ControlFlowExits {
        let mut exits = ControlFlowExits::default();
        let mut fallthrough = Some(self.imports.clone());
        for statement in statements {
            let Some(imports) = fallthrough.take() else {
                let inherited = self.imports.clone();
                #[cfg(test)]
                let marker_projection = self.marker_projection.take();
                let _ = self.visit_statement_flow(statement);
                #[cfg(test)]
                {
                    self.marker_projection = marker_projection;
                }
                self.imports = inherited;
                continue;
            };
            self.imports = imports;
            let statement_exits = self.visit_statement_flow(statement);
            fallthrough.clone_from(&statement_exits.fallthrough);
            exits.merge_abrupt(statement_exits);
        }
        if let Some(imports) = &fallthrough {
            self.imports = imports.clone();
        }
        exits.fallthrough = fallthrough;
        exits
    }

    fn merge_branch(
        branches: &mut Vec<KnownImports>,
        exits: &mut ControlFlowExits,
        branch: ControlFlowExits,
    ) {
        if let Some(imports) = branch.fallthrough.clone() {
            branches.push(imports);
        }
        exits.merge_abrupt(branch);
    }

    fn visit_function_definition(
        &mut self,
        statement: &'ast Stmt,
        definition: &'ast ruff_python_ast::StmtFunctionDef,
    ) -> ControlFlowExits {
        self.visit_function_header(definition);
        self.imports.transfer_statement(statement);
        let inherited = self.imports.clone();
        if let (Some(bindings), Some(fallback), Some(parent_scope)) = (
            &self.class_external_bindings,
            &mut self.class_body_fallback,
            self.class_parent_scope,
        ) {
            bindings.copy_for_fallback(&self.imports, fallback, parent_scope);
        }
        let inherited_fallback = self.class_body_fallback.clone();
        let inherited_bindings = self.class_external_bindings.take();
        let inherited_class_parent_scope = self.class_parent_scope.take();
        self.imports = inherited_fallback
            .clone()
            .unwrap_or_else(|| inherited.clone());
        if let Some(type_params) = definition.type_params.as_deref() {
            self.imports.enter_type_params(type_params);
        }
        for local in FunctionLocalCollector::collect(definition) {
            self.imports.invalidate(&local);
        }
        self.class_body_fallback = None;
        let inherited_scope = self.scope_kind;
        self.scope_kind = ScopeKind::Function;
        self.qualname.push(definition.name.as_str().to_owned());
        self.visit_suite(&definition.body);
        self.qualname.pop();
        self.scope_kind = inherited_scope;
        self.imports = inherited;
        self.class_body_fallback = inherited_fallback;
        self.class_external_bindings = inherited_bindings;
        self.class_parent_scope = inherited_class_parent_scope;
        ControlFlowExits::fallthrough(self.imports.clone())
    }

    fn visit_class_definition(
        &mut self,
        statement: &'ast Stmt,
        definition: &'ast ruff_python_ast::StmtClassDef,
    ) -> ControlFlowExits {
        self.visit_class_header(definition);
        let mut inherited = self.imports.clone();
        let inherited_fallback = self.class_body_fallback.clone();
        let inherited_bindings = self.class_external_bindings.take();
        let inherited_class_parent_scope = self.class_parent_scope.take();
        let inherited_scope = self.scope_kind;
        let mut class_scope = inherited.clone();
        let mut class_fallback = inherited_fallback
            .clone()
            .unwrap_or_else(|| inherited.clone());
        if let Some(type_params) = definition.type_params.as_deref() {
            class_scope.enter_type_params(type_params);
            class_fallback.enter_type_params(type_params);
        }
        class_fallback.invalidate(definition.name.as_str());
        self.class_body_fallback = Some(class_fallback);
        self.class_external_bindings = Some(ClassExternalBindings::collect(&definition.body));
        self.class_parent_scope = Some(inherited_scope);
        self.scope_kind = ScopeKind::Class;
        self.imports = class_scope;
        self.qualname.push(definition.name.as_str().to_owned());
        let body_exits = self.visit_suite_flow(&definition.body);
        self.qualname.pop();
        if let Some(body_imports) = &body_exits.fallthrough
            && let Some(bindings) = &self.class_external_bindings
        {
            match inherited_scope {
                ScopeKind::Module => {
                    for name in &bindings.globals {
                        inherited.copy_name_from(body_imports, name);
                    }
                }
                ScopeKind::Function => {
                    for name in &bindings.nonlocals {
                        inherited.copy_name_from(body_imports, name);
                    }
                }
                ScopeKind::Class => {}
            }
        }
        self.imports = inherited;
        self.class_body_fallback = inherited_fallback;
        self.class_external_bindings = inherited_bindings;
        self.class_parent_scope = inherited_class_parent_scope;
        self.scope_kind = inherited_scope;
        self.imports.transfer_statement(statement);
        ControlFlowExits::fallthrough(self.imports.clone())
    }

    fn visit_if(&mut self, statement: &'ast ruff_python_ast::StmtIf) -> ControlFlowExits {
        NamedBindingInvalidator::visit(&mut self.imports, statement.test.as_ref());
        let mut remaining = Some(self.imports.clone());
        let mut branches = Vec::new();
        let mut exits = ControlFlowExits::default();
        let body = self.visit_suite_from(
            remaining.clone().expect("if test has a false path"),
            &statement.body,
        );
        Self::merge_branch(&mut branches, &mut exits, body);
        for clause in &statement.elif_else_clauses {
            let Some(mut clause_imports) = remaining.take() else {
                break;
            };
            if let Some(test) = &clause.test {
                NamedBindingInvalidator::visit(&mut clause_imports, test);
                remaining = Some(clause_imports.clone());
            }
            let clause_exits = self.visit_suite_from(clause_imports, &clause.body);
            Self::merge_branch(&mut branches, &mut exits, clause_exits);
            if clause.test.is_none() {
                remaining = None;
            }
        }
        if let Some(imports) = remaining {
            branches.push(imports);
        }
        exits.fallthrough = KnownImports::intersection(branches);
        exits
    }

    fn visit_loop(
        &mut self,
        body_imports: &KnownImports,
        zero_iteration: KnownImports,
        iteration_target: Option<&'ast Expr>,
        body: &'ast [Stmt],
        orelse: &'ast [Stmt],
    ) -> ControlFlowExits {
        let body_imports = self.loop_head_fixed_point(body_imports, iteration_target, body);
        let body_exits = self.visit_suite_from(body_imports, body);
        let mut natural = vec![zero_iteration];
        natural.extend(body_exits.fallthrough.clone());
        natural.extend(body_exits.continues.iter().cloned());
        let natural =
            KnownImports::intersection(natural).expect("a loop always has its zero-iteration path");
        let orelse_exits = self.visit_suite_from(natural, orelse);
        let mut after_loop = body_exits.breaks.clone();
        after_loop.extend(orelse_exits.fallthrough.clone());
        after_loop.extend(orelse_exits.breaks.iter().cloned());
        after_loop.extend(orelse_exits.continues.iter().cloned());
        let mut exits = ControlFlowExits {
            fallthrough: KnownImports::intersection(after_loop),
            terminates: body_exits.terminates,
            ..ControlFlowExits::default()
        };
        exits.terminates.extend(orelse_exits.terminates);
        exits
    }

    fn loop_head_fixed_point(
        &mut self,
        initial: &KnownImports,
        iteration_target: Option<&'ast Expr>,
        body: &'ast [Stmt],
    ) -> KnownImports {
        let mut head = initial.clone();
        loop {
            let record_annotations = self.record_annotations;
            self.record_annotations = false;
            let body_exits = self.visit_suite_from(head.clone(), body);
            self.record_annotations = record_annotations;

            let mut entries = vec![initial.clone()];
            if let Some(mut imports) = body_exits.fallthrough {
                if let Some(target) = iteration_target {
                    imports.invalidate_target(target);
                }
                entries.push(imports);
            }
            #[cfg(test)]
            let include_continues =
                self.test_mutation != Some(BindingFlowTestMutation::OmitLoopContinueBackEdge);
            #[cfg(not(test))]
            let include_continues = true;
            if include_continues {
                for mut imports in body_exits.continues {
                    if let Some(target) = iteration_target {
                        imports.invalidate_target(target);
                    }
                    entries.push(imports);
                }
            }
            let next = KnownImports::intersection(entries)
                .expect("a loop head always includes its initial entry");
            if next == head {
                return head;
            }
            head = next;
        }
    }

    #[cfg(test)]
    fn loop_head_fixed_point_without_continues(
        &mut self,
        initial: &KnownImports,
        iteration_target: Option<&'ast Expr>,
        body: &'ast [Stmt],
    ) -> KnownImports {
        let mut head = initial.clone();
        loop {
            let record_annotations = self.record_annotations;
            self.record_annotations = false;
            let body_exits = self.visit_suite_from(head.clone(), body);
            self.record_annotations = record_annotations;

            let mut entries = vec![initial.clone()];
            if let Some(mut imports) = body_exits.fallthrough {
                if let Some(target) = iteration_target {
                    imports.invalidate_target(target);
                }
                entries.push(imports);
            }
            let next = KnownImports::intersection(entries)
                .expect("a loop head always includes its initial entry");
            if next == head {
                return head;
            }
            head = next;
        }
    }

    fn visit_for(&mut self, statement: &'ast ruff_python_ast::StmtFor) -> ControlFlowExits {
        NamedBindingInvalidator::visit(&mut self.imports, statement.iter.as_ref());
        let zero_iteration = self.imports.clone();
        let mut body_imports = zero_iteration.clone();
        body_imports.invalidate_target(statement.target.as_ref());
        self.visit_loop(
            &body_imports,
            zero_iteration,
            Some(statement.target.as_ref()),
            &statement.body,
            &statement.orelse,
        )
    }

    fn visit_while(&mut self, statement: &'ast ruff_python_ast::StmtWhile) -> ControlFlowExits {
        NamedBindingInvalidator::visit(&mut self.imports, statement.test.as_ref());
        let zero_iteration = self.imports.clone();
        self.visit_loop(
            &zero_iteration,
            zero_iteration.clone(),
            None,
            &statement.body,
            &statement.orelse,
        )
    }

    fn visit_with(&mut self, statement: &'ast ruff_python_ast::StmtWith) -> ControlFlowExits {
        for item in &statement.items {
            NamedBindingInvalidator::visit(&mut self.imports, &item.context_expr);
            if let Some(target) = &item.optional_vars {
                self.imports.invalidate_target(target);
            }
        }
        self.visit_suite_flow(&statement.body)
    }

    fn visit_try(&mut self, statement: &'ast ruff_python_ast::StmtTry) -> ControlFlowExits {
        let incoming = self.imports.clone();
        let body_exits = self.visit_suite_from(incoming.clone(), &statement.body);
        let normal_exits = body_exits
            .fallthrough
            .clone()
            .map(|imports| self.visit_suite_from(imports, &statement.orelse));

        let handler_imports = Self::try_handler_imports(incoming, &statement.body);
        let mut joined = ControlFlowExits::default();
        let mut fallthrough = Vec::new();
        if let Some(normal) = normal_exits {
            Self::merge_branch(&mut fallthrough, &mut joined, normal);
        }
        joined.breaks.extend(body_exits.breaks);
        joined.continues.extend(body_exits.continues);
        #[cfg(test)]
        self.merge_try_body_terminates(&mut joined, body_exits.terminates);
        #[cfg(not(test))]
        Self::merge_try_body_terminates(&mut joined, body_exits.terminates);
        for (handler_index, except_handler) in statement.handlers.iter().enumerate() {
            let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
            let mut imports = handler_imports.clone();
            #[cfg(test)]
            if self.test_mutation != Some(BindingFlowTestMutation::BindHandlerTargetBeforeType) {
                self.imports.clone_from(&imports);
                self.capture_compound_header_entry(handler.range, &handler.body);
            }
            if let Some(type_) = &handler.type_ {
                NamedBindingInvalidator::visit(&mut imports, type_.as_ref());
            }
            if let Some(name) = &handler.name {
                imports.invalidate(name.as_str());
            }
            #[cfg(test)]
            if self.test_mutation == Some(BindingFlowTestMutation::BindHandlerTargetBeforeType) {
                self.imports.clone_from(&imports);
                self.capture_compound_header_entry(handler.range, &handler.body);
            }
            let mut handler_exits = self.visit_suite_from(imports, &handler.body);
            if let Some(name) = &handler.name {
                let invalidate = |state: &mut KnownImports| state.invalidate(name.as_str());
                #[cfg(test)]
                let omitted_category = match self.test_mutation {
                    Some(BindingFlowTestMutation::OmitHandlerFallthroughCleanup) => {
                        Some(ExitCategory::Fallthrough)
                    }
                    Some(BindingFlowTestMutation::OmitHandlerBreakCleanup) => {
                        Some(ExitCategory::Break)
                    }
                    Some(BindingFlowTestMutation::OmitHandlerContinueCleanup) => {
                        Some(ExitCategory::Continue)
                    }
                    Some(BindingFlowTestMutation::OmitHandlerTerminateCleanup) => {
                        Some(ExitCategory::Terminate)
                    }
                    _ => None,
                };
                #[cfg(not(test))]
                let omitted_category = None;
                if let Some(state) = &mut handler_exits.fallthrough
                    && omitted_category != Some(ExitCategory::Fallthrough)
                {
                    invalidate(state);
                }
                for (category, states) in [
                    (ExitCategory::Break, &mut handler_exits.breaks),
                    (ExitCategory::Continue, &mut handler_exits.continues),
                    (ExitCategory::Terminate, &mut handler_exits.terminates),
                ] {
                    if omitted_category != Some(category) {
                        for state in states {
                            invalidate(state);
                        }
                    }
                }
            }
            #[cfg(test)]
            self.mutate_multiple_handler_exits(&mut handler_exits);
            #[cfg(test)]
            self.capture_handler_exit(handler.range, &handler.body, &handler_exits);
            #[cfg(test)]
            let include_handler =
                self.includes_multiple_handler(handler_index, statement.handlers.len());
            #[cfg(not(test))]
            let include_handler =
                Self::includes_multiple_handler(handler_index, statement.handlers.len());
            if include_handler {
                Self::merge_branch(&mut fallthrough, &mut joined, handler_exits);
            }
        }
        joined.fallthrough = KnownImports::intersection(fallthrough);
        let exits = self.apply_finally(joined, &statement.finalbody);
        #[cfg(test)]
        self.capture_try_exit(statement.range, &exits);
        exits
    }

    fn apply_finally(
        &mut self,
        exits: ControlFlowExits,
        finalbody: &'ast [Stmt],
    ) -> ControlFlowExits {
        if finalbody.is_empty() {
            return exits;
        }

        let mut annotation_entries = Vec::new();
        annotation_entries.extend(exits.fallthrough.iter().cloned());
        annotation_entries.extend(exits.breaks.iter().cloned());
        annotation_entries.extend(exits.continues.iter().cloned());
        annotation_entries.extend(exits.terminates.iter().cloned());
        let Some(annotation_entry) = KnownImports::intersection(annotation_entries) else {
            let _ = self.visit_suite_from(self.imports.clone(), finalbody);
            return ControlFlowExits::default();
        };
        let _ = self.visit_suite_from(annotation_entry, finalbody);

        let mut result = ControlFlowExits::default();
        let mut fallthrough = Vec::new();
        if let Some(imports) = exits.fallthrough {
            self.route_finally_entry(
                imports,
                ExitCategory::Fallthrough,
                finalbody,
                &mut fallthrough,
                &mut result,
            );
        }
        for (category, entries) in [
            (ExitCategory::Break, exits.breaks),
            (ExitCategory::Continue, exits.continues),
            (ExitCategory::Terminate, exits.terminates),
        ] {
            for imports in entries {
                self.route_finally_entry(
                    imports,
                    category,
                    finalbody,
                    &mut fallthrough,
                    &mut result,
                );
            }
        }
        result.fallthrough = KnownImports::intersection(fallthrough);
        result
    }

    fn route_finally_entry(
        &mut self,
        imports: KnownImports,
        category: ExitCategory,
        finalbody: &'ast [Stmt],
        fallthrough: &mut Vec<KnownImports>,
        result: &mut ControlFlowExits,
    ) {
        let record_annotations = self.record_annotations;
        self.record_annotations = false;
        let final_exits = self.visit_suite_from(imports, finalbody);
        self.record_annotations = record_annotations;

        if let Some(imports) = final_exits.fallthrough {
            match category {
                ExitCategory::Fallthrough => fallthrough.push(imports),
                ExitCategory::Break => result.breaks.push(imports),
                ExitCategory::Continue => result.continues.push(imports),
                ExitCategory::Terminate => result.terminates.push(imports),
            }
        }
        result.breaks.extend(final_exits.breaks);
        result.continues.extend(final_exits.continues);
        result.terminates.extend(final_exits.terminates);
    }

    fn visit_match(&mut self, statement: &'ast ruff_python_ast::StmtMatch) -> ControlFlowExits {
        NamedBindingInvalidator::visit(&mut self.imports, statement.subject.as_ref());
        let mut remaining = Some(self.imports.clone());
        let mut fallthrough = Vec::new();
        let mut exits = ControlFlowExits::default();
        for case in &statement.cases {
            let Some(pre_pattern) = remaining.take() else {
                break;
            };
            #[cfg(test)]
            {
                self.imports.clone_from(&pre_pattern);
                self.capture_compound_header_entry(case.range, &case.body);
            }
            let pattern_flow = pattern_binding_flow(&pre_pattern, &case.pattern);
            let Some(mut imports) = pattern_flow.matched else {
                continue;
            };
            let mut failed = Vec::new();
            if !case.pattern.is_irrefutable() {
                let pattern_failure = pattern_flow.failed;
                #[cfg(test)]
                let mut pattern_failure = pattern_failure;
                #[cfg(test)]
                if self.test_mutation == Some(BindingFlowTestMutation::KeepLastOrPatternFailure)
                    && let Pattern::MatchOr(or_pattern) = &case.pattern
                    && let Some(last) = or_pattern.patterns.last()
                {
                    pattern_failure = pattern_binding_flow(&pre_pattern, last).failed;
                }
                #[cfg(test)]
                if self.test_mutation == Some(BindingFlowTestMutation::OverbroadPatternCleanup)
                    && let Some(pattern_failure) = &mut pattern_failure
                {
                    pattern_failure.invalidate("Mapping");
                }
                #[cfg(test)]
                if self.test_mutation != Some(BindingFlowTestMutation::DropRefutableUnmatched)
                    && let Some(pattern_failure) = pattern_failure
                {
                    failed.push(match self.test_mutation {
                        Some(BindingFlowTestMutation::UsePrePatternFailureEnvironment) => {
                            pre_pattern.clone()
                        }
                        Some(BindingFlowTestMutation::UseMatchedPatternFailureEnvironment) => {
                            imports.clone()
                        }
                        _ => pattern_failure,
                    });
                }
                #[cfg(not(test))]
                if let Some(pattern_failure) = pattern_failure {
                    failed.push(pattern_failure);
                }
            }
            if let Some(guard) = &case.guard {
                #[cfg(test)]
                let pre_guard = imports.clone();
                NamedBindingInvalidator::visit(&mut imports, guard.as_ref());
                #[cfg(test)]
                failed.push(
                    if self.test_mutation
                        == Some(BindingFlowTestMutation::UsePreGuardFailureEnvironment)
                    {
                        pre_guard
                    } else {
                        imports.clone()
                    },
                );
                #[cfg(not(test))]
                failed.push(imports.clone());
            }
            let case_exits = self.visit_suite_from(imports, &case.body);
            #[cfg(test)]
            let case_exits = if self.test_mutation
                == Some(BindingFlowTestMutation::FlattenMatchAbruptToFallthrough)
            {
                let mut case_exits = case_exits;
                let mut flattened = case_exits
                    .fallthrough
                    .take()
                    .into_iter()
                    .collect::<Vec<_>>();
                flattened.append(&mut case_exits.breaks);
                flattened.append(&mut case_exits.continues);
                flattened.append(&mut case_exits.terminates);
                case_exits.fallthrough = KnownImports::intersection(flattened);
                case_exits
            } else {
                case_exits
            };
            Self::merge_branch(&mut fallthrough, &mut exits, case_exits);
            remaining = KnownImports::intersection(failed);
        }
        if let Some(imports) = remaining {
            fallthrough.push(imports);
        }
        exits.fallthrough = KnownImports::intersection(fallthrough);
        exits
    }

    fn visit_statement_flow(&mut self, statement: &'ast Stmt) -> ControlFlowExits {
        #[cfg(test)]
        if !matches!(
            statement,
            Stmt::FunctionDef(_)
                | Stmt::ClassDef(_)
                | Stmt::If(_)
                | Stmt::For(_)
                | Stmt::While(_)
                | Stmt::With(_)
                | Stmt::Try(_)
                | Stmt::Match(_)
        ) {
            self.capture_marker_entry(statement.range());
        }
        match statement {
            Stmt::FunctionDef(definition) => self.visit_function_definition(statement, definition),
            Stmt::ClassDef(definition) => self.visit_class_definition(statement, definition),
            Stmt::TypeAlias(alias) => self.visit_type_alias(statement, alias),
            Stmt::If(statement_if) => self.visit_if(statement_if),
            Stmt::For(statement_for) => self.visit_for(statement_for),
            Stmt::While(statement_while) => self.visit_while(statement_while),
            Stmt::With(statement_with) => self.visit_with(statement_with),
            Stmt::Try(statement_try) => self.visit_try(statement_try),
            Stmt::Match(statement_match) => self.visit_match(statement_match),
            Stmt::AnnAssign(assign) => {
                if let Some(value) = assign.value.as_deref() {
                    NamedBindingInvalidator::visit(&mut self.imports, value);
                    NamedBindingInvalidator::visit(&mut self.imports, assign.target.as_ref());
                    self.imports
                        .transfer_assign(std::slice::from_ref(assign.target.as_ref()), value);
                } else {
                    NamedBindingInvalidator::visit(&mut self.imports, assign.target.as_ref());
                }
                self.record(assign.annotation.as_ref());
                NamedBindingInvalidator::visit(&mut self.imports, assign.annotation.as_ref());
                ControlFlowExits::fallthrough(self.imports.clone())
            }
            Stmt::Break(_) => {
                visitor::walk_stmt(self, statement);
                ControlFlowExits {
                    breaks: vec![self.imports.clone()],
                    ..ControlFlowExits::default()
                }
            }
            Stmt::Continue(_) => {
                visitor::walk_stmt(self, statement);
                ControlFlowExits {
                    continues: vec![self.imports.clone()],
                    ..ControlFlowExits::default()
                }
            }
            Stmt::Return(_) | Stmt::Raise(_) => {
                visitor::walk_stmt(self, statement);
                NamedBindingInvalidator::visit_statement(&mut self.imports, statement);
                ControlFlowExits {
                    terminates: vec![self.imports.clone()],
                    ..ControlFlowExits::default()
                }
            }
            _ => {
                visitor::walk_stmt(self, statement);
                NamedBindingInvalidator::visit_statement(&mut self.imports, statement);
                self.imports.transfer_statement(statement);
                ControlFlowExits::fallthrough(self.imports.clone())
            }
        }
    }
}

impl<'ast> Visitor<'ast> for AnnotationCollector<'ast> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        let exits = self.visit_statement_flow(statement);
        if let Some(imports) = exits.fallthrough {
            self.imports = imports;
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BindingFlowTestSnapshot {
    pub(super) fallthrough: Vec<Vec<String>>,
    pub(super) breaks: Vec<Vec<String>>,
    pub(super) continues: Vec<Vec<String>>,
    pub(super) terminates: Vec<Vec<String>>,
}

#[cfg(test)]
fn normalize_binding_flow_imports(imports: &KnownImports) -> Vec<String> {
    let mut facts = imports
        .direct
        .iter()
        .map(|(name, resolved)| format!("direct:{name}={resolved}"))
        .chain(
            imports
                .modules
                .iter()
                .map(|(name, resolved)| format!("module:{name}={resolved}")),
        )
        .chain(
            imports
                .type_vars
                .iter()
                .map(|name| format!("type-var:{name}")),
        )
        .collect::<Vec<_>>();
    facts.sort();
    facts
}

#[cfg(test)]
fn normalize_binding_flow_states(states: &[KnownImports]) -> Vec<Vec<String>> {
    let mut normalized = states
        .iter()
        .map(normalize_binding_flow_imports)
        .collect::<Vec<_>>();
    normalized.sort();
    normalized
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AnnotationSiteTestSnapshot {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) symbol: Option<String>,
    pub(super) scope: &'static str,
    pub(super) facts: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NameResolutionTestSnapshot {
    pub(super) start: usize,
    pub(super) resolution: &'static str,
}

#[cfg(test)]
fn unique_marker_range(source: &str, marker: &str) -> Result<Range<usize>, String> {
    if marker.is_empty() {
        return Err("infrastructure-error: marker must not be empty".to_owned());
    }
    let offsets = source
        .match_indices(marker)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let [start] = offsets.as_slice() else {
        return Err(format!(
            "infrastructure-error: marker occurs {} times",
            offsets.len()
        ));
    };
    Ok(*start..start + marker.len())
}

#[cfg(test)]
fn annotation_scope_label(scope: ScopeKind) -> &'static str {
    match scope {
        ScopeKind::Module => "module",
        ScopeKind::Function => "function",
        ScopeKind::Class => "class",
    }
}

#[cfg(test)]
pub(super) fn annotation_site_test_snapshot(
    source: &str,
    marker: &str,
) -> Result<AnnotationSiteTestSnapshot, String> {
    let marker = unique_marker_range(source, marker)?;
    let parsed = parse_module(source)
        .map_err(|error| format!("infrastructure-error: source did not parse: {error}"))?;
    let matching = AnnotationCollector::collect(parsed.syntax())
        .into_iter()
        .filter(|site| {
            let range = site.annotation.range();
            usize::from(range.start()) <= marker.start && marker.end <= usize::from(range.end())
        })
        .collect::<Vec<_>>();
    let [site] = matching.as_slice() else {
        return Err(format!(
            "infrastructure-error: marker matched {} annotation sites",
            matching.len()
        ));
    };
    let range = site.annotation.range();
    Ok(AnnotationSiteTestSnapshot {
        start: range.start().into(),
        end: range.end().into(),
        symbol: site.symbol.clone(),
        scope: annotation_scope_label(site.scope_kind),
        facts: normalize_binding_flow_imports(&site.imports),
    })
}

#[cfg(test)]
fn name_resolution_label(resolution: NameResolution) -> &'static str {
    match resolution {
        NameResolution::DefinitelyBuiltin => "definitely-builtin",
        NameResolution::Shadowed => "shadowed",
        NameResolution::Unknown => "unknown",
    }
}

#[cfg(test)]
pub(super) fn name_resolution_test_snapshot(
    source: &str,
    marker: &str,
    name: &str,
) -> Result<NameResolutionTestSnapshot, String> {
    if !tracked_resolution_name(name) {
        return Err("infrastructure-error: name is not tracked for resolution".to_owned());
    }
    let marker_range = unique_marker_range(source, marker)?;
    let marker_source = &source[marker_range.clone()];
    let relative = marker_source
        .match_indices(name)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let [relative_start] = relative.as_slice() else {
        return Err(format!(
            "infrastructure-error: name occurs {} times inside marker",
            relative.len()
        ));
    };
    let start = marker_range.start + *relative_start;
    let parsed = parse_module(source)
        .map_err(|error| format!("infrastructure-error: source did not parse: {error}"))?;
    let index = NameResolutionIndex::from_module(parsed.syntax());
    if !index.occurrences.contains_key(&start) {
        return Err("infrastructure-error: marker does not select a tracked load name".to_owned());
    }
    Ok(NameResolutionTestSnapshot {
        start,
        resolution: name_resolution_label(index.resolution(start, name)),
    })
}

#[cfg(test)]
fn normalize_binding_flow_exits(exits: &ControlFlowExits) -> BindingFlowTestSnapshot {
    BindingFlowTestSnapshot {
        fallthrough: exits
            .fallthrough
            .iter()
            .map(normalize_binding_flow_imports)
            .collect(),
        breaks: normalize_binding_flow_states(&exits.breaks),
        continues: normalize_binding_flow_states(&exits.continues),
        terminates: normalize_binding_flow_states(&exits.terminates),
    }
}

#[cfg(test)]
pub(super) fn binding_flow_test_snapshot(source: &str) -> BindingFlowTestSnapshot {
    let parsed = parse_module(source).expect("binding-flow fixture must parse");
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    let exits = collector.visit_suite_flow(&parsed.syntax().body);
    normalize_binding_flow_exits(&exits)
}

#[cfg(test)]
pub(super) fn binding_flow_try_exit_snapshot(
    source: &str,
    marker: &str,
) -> Result<BindingFlowTestSnapshot, String> {
    binding_flow_try_exit_snapshot_inner(source, marker, None)
}

#[cfg(test)]
pub(super) fn binding_flow_try_exit_snapshot_with_mutation(
    source: &str,
    marker: &str,
    mutation: BindingFlowTestMutation,
) -> Result<BindingFlowTestSnapshot, String> {
    binding_flow_try_exit_snapshot_inner(source, marker, Some(mutation))
}

#[cfg(test)]
fn binding_flow_try_exit_snapshot_inner(
    source: &str,
    marker: &str,
    mutation: Option<BindingFlowTestMutation>,
) -> Result<BindingFlowTestSnapshot, String> {
    let marker = unique_marker_range(source, marker)?;
    let parsed = parse_module(source)
        .map_err(|error| format!("infrastructure-error: source did not parse: {error}"))?;
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    collector.test_mutation = mutation;
    collector.try_exit_projection = Some(BindingFlowTryExitProjection {
        marker,
        matching_tries: 0,
        matched_try: None,
        exits: None,
    });
    let _ = collector.visit_suite_flow(&parsed.syntax().body);
    let projection = collector
        .try_exit_projection
        .expect("try exit projection was configured");
    if projection.matching_tries != 1 {
        return Err(format!(
            "infrastructure-error: marker matched {} try exits",
            projection.matching_tries
        ));
    }
    projection
        .exits
        .ok_or_else(|| "infrastructure-error: selected try produced no exit snapshot".to_owned())
}

#[cfg(test)]
pub(super) fn binding_flow_marker_snapshot(
    source: &str,
    marker: &str,
) -> Result<Vec<String>, String> {
    binding_flow_marker_snapshot_inner(source, marker, None)
}

#[cfg(test)]
pub(super) fn binding_flow_marker_snapshot_with_mutation(
    source: &str,
    marker: &str,
    mutation: BindingFlowTestMutation,
) -> Result<Vec<String>, String> {
    binding_flow_marker_snapshot_inner(source, marker, Some(mutation))
}

#[cfg(test)]
fn binding_flow_marker_snapshot_inner(
    source: &str,
    marker: &str,
    mutation: Option<BindingFlowTestMutation>,
) -> Result<Vec<String>, String> {
    let marker = unique_marker_range(source, marker)?;
    let line_start = source[..marker.start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let parsed = parse_module(source)
        .map_err(|error| format!("infrastructure-error: source did not parse: {error}"))?;
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    collector.test_mutation = mutation;
    collector.marker_projection = Some(BindingFlowMarkerProjection {
        source: source.to_owned(),
        marker,
        line_start,
        entry: None,
    });
    let _ = collector.visit_suite_flow(&parsed.syntax().body);
    let (_, imports) = collector
        .marker_projection
        .and_then(|projection| projection.entry)
        .ok_or_else(|| "infrastructure-error: marker did not select a flow entry".to_owned())?;
    Ok(normalize_binding_flow_imports(&imports))
}

#[cfg(test)]
pub(super) fn binding_flow_handler_exit_snapshot(
    source: &str,
    marker: &str,
    category: &str,
) -> Result<Vec<String>, String> {
    binding_flow_handler_exit_snapshot_inner(source, marker, category, None)
}

#[cfg(test)]
pub(super) fn binding_flow_handler_exit_snapshot_with_mutation(
    source: &str,
    marker: &str,
    category: &str,
    mutation: BindingFlowTestMutation,
) -> Result<Vec<String>, String> {
    binding_flow_handler_exit_snapshot_inner(source, marker, category, Some(mutation))
}

#[cfg(test)]
fn binding_flow_handler_exit_snapshot_inner(
    source: &str,
    marker: &str,
    category: &str,
    mutation: Option<BindingFlowTestMutation>,
) -> Result<Vec<String>, String> {
    let marker = unique_marker_range(source, marker)?;
    let line_start = source[..marker.start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let category = match category {
        "fallthrough" => ExitCategory::Fallthrough,
        "break" => ExitCategory::Break,
        "continue" => ExitCategory::Continue,
        "terminate" => ExitCategory::Terminate,
        other => {
            return Err(format!(
                "infrastructure-error: unsupported handler exit category {other}"
            ));
        }
    };
    let parsed = parse_module(source)
        .map_err(|error| format!("infrastructure-error: source did not parse: {error}"))?;
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    collector.test_mutation = mutation;
    collector.marker_projection = Some(BindingFlowMarkerProjection {
        source: source.to_owned(),
        marker: marker.clone(),
        line_start,
        entry: None,
    });
    collector.handler_exit_projection = Some(BindingFlowHandlerExitProjection {
        source: source.to_owned(),
        marker,
        line_start,
        category,
        matching_handlers: 0,
        matched_handler: None,
        states: Vec::new(),
    });
    let _ = collector.visit_suite_flow(&parsed.syntax().body);
    if collector
        .marker_projection
        .and_then(|projection| projection.entry)
        .is_none()
    {
        return Err("infrastructure-error: handler exit marker was not reached".to_owned());
    }
    let projection = collector
        .handler_exit_projection
        .expect("handler exit projection was configured");
    if projection.matching_handlers != 1 {
        return Err(format!(
            "infrastructure-error: marker matched {} handler exits",
            projection.matching_handlers
        ));
    }
    let [state] = projection.states.as_slice() else {
        return Err(format!(
            "infrastructure-error: selected handler category produced {} exits",
            projection.states.len()
        ));
    };
    Ok(normalize_binding_flow_imports(state))
}

#[cfg(test)]
pub(super) fn binding_flow_loop_head_snapshot(
    source: &str,
    include_continues: bool,
) -> Vec<String> {
    let parsed = parse_module(source).expect("binding-flow loop fixture must parse");
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    for statement in &parsed.syntax().body {
        if let Stmt::While(statement_while) = statement {
            NamedBindingInvalidator::visit(&mut collector.imports, statement_while.test.as_ref());
            let initial = collector.imports.clone();
            let head = if include_continues {
                collector.loop_head_fixed_point(&initial, None, &statement_while.body)
            } else {
                collector.loop_head_fixed_point_without_continues(
                    &initial,
                    None,
                    &statement_while.body,
                )
            };
            return normalize_binding_flow_imports(&head);
        }
        let exits = collector.visit_statement_flow(statement);
        let Some(imports) = exits.fallthrough else {
            break;
        };
        collector.imports = imports;
    }
    panic!("binding-flow loop fixture has no while statement")
}

#[cfg(test)]
pub(super) fn binding_flow_loop_head_snapshot_at_marker(
    source: &str,
    marker: &str,
    mutation: Option<BindingFlowTestMutation>,
) -> Result<Vec<String>, String> {
    let marker = unique_marker_range(source, marker)?;
    let line_start = source[..marker.start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let parsed = parse_module(source)
        .map_err(|error| format!("infrastructure-error: source did not parse: {error}"))?;
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    collector.test_mutation = mutation;
    let mut matching = 0;
    let mut observed = None;
    for statement in &parsed.syntax().body {
        if let Stmt::While(statement_while) = statement
            && binding_flow_marker_rank(source, &marker, line_start, statement_while.range)
                .is_some()
        {
            matching += 1;
            NamedBindingInvalidator::visit(&mut collector.imports, statement_while.test.as_ref());
            let initial = collector.imports.clone();
            let head = collector.loop_head_fixed_point(&initial, None, &statement_while.body);
            observed = Some(normalize_binding_flow_imports(&head));
        }
        let exits = collector.visit_statement_flow(statement);
        let Some(imports) = exits.fallthrough else {
            break;
        };
        collector.imports = imports;
    }
    if matching != 1 {
        return Err(format!(
            "infrastructure-error: marker matched {matching} loop heads"
        ));
    }
    observed.ok_or_else(|| "infrastructure-error: selected loop produced no head".to_owned())
}

fn type_annotation_candidates(
    module: &ModModule,
    source: &str,
    line_index: &LineIndex,
    facts: &AstFacts<'_>,
    request: &AnalyzeRequest<'_>,
) -> ProducerPrefix {
    let mut candidates = CandidatePrefix::new(request.max_candidates);
    for site in AnnotationCollector::collect(module) {
        for (replacement, operator) in
            annotation_replacements(site.annotation, source, facts, &site.imports)
        {
            let range = site.annotation.range();
            let start = usize::from(range.start());
            let end = usize::from(range.end());
            if let Some(candidate) = make_candidate(
                request,
                source,
                line_index,
                start..end,
                replacement,
                operator,
                site.symbol.clone(),
            ) && retained_by_profile(&candidate, request.profile, facts)
            {
                candidates.push(candidate);
            }
        }
    }
    candidates.finish()
}

fn annotation_replacements(
    annotation: &Expr,
    source: &str,
    facts: &AstFacts<'_>,
    imports: &KnownImports,
) -> Vec<(String, MutationOperator)> {
    if contains_disallowed_annotation(annotation, imports) {
        return Vec::new();
    }
    let mut replacements = Vec::new();
    if let Some(replacement) = nullable_removal(annotation, source, facts, imports) {
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

fn nullable_removal(
    annotation: &Expr,
    source: &str,
    facts: &AstFacts<'_>,
    imports: &KnownImports,
) -> Option<String> {
    if let Expr::BinOp(binary) = annotation
        && binary.op == Operator::BitOr
    {
        if is_none(binary.left.as_ref()) {
            return nullable_union_operand_source(binary.right.as_ref(), binary, source, facts);
        }
        if is_none(binary.right.as_ref()) {
            return nullable_union_operand_source(binary.left.as_ref(), binary, source, facts);
        }
    }
    let Expr::Subscript(subscript) = annotation else {
        return None;
    };
    (imports.resolved_name(subscript.value.as_ref()).as_deref() == Some("typing.Optional"))
        .then(|| nullable_optional_inner_source(subscript, source, facts))
        .flatten()
}

fn nullable_union_operand_source(
    retained: &Expr,
    parent: &ExprBinOp,
    source: &str,
    facts: &AstFacts<'_>,
) -> Option<String> {
    let range = ruff_python_ast::token::parenthesized_range(
        retained.into(),
        parent.into(),
        facts.tokens.expect("parser tokens are set"),
    )
    .unwrap_or_else(|| retained.range());
    let retained_source = source_text(source, range)?;
    if range == retained.range() && range_contains_annotation_trivia(source, range, facts) {
        return Some(format!("({retained_source})"));
    }
    Some(retained_source.to_owned())
}

fn nullable_optional_inner_source(
    optional: &ExprSubscript,
    source: &str,
    facts: &AstFacts<'_>,
) -> Option<String> {
    let tokens = facts.tokens.expect("parser tokens are set");
    let base_range = ruff_python_ast::token::parenthesized_range(
        optional.value.as_ref().into(),
        optional.into(),
        tokens,
    )
    .unwrap_or_else(|| optional.value.range());
    let optional_tokens = facts.candidate_tokens_in_range(optional.range());
    let opening = optional_tokens.iter().find(|token| {
        token.kind() == TokenKind::Lsqb && token.range().start() >= base_range.end()
    })?;
    let closing = optional_tokens
        .iter()
        .rfind(|token| token.kind() == TokenKind::Rsqb)?;
    let interior_range = TextRange::new(opening.range().end(), closing.range().start());
    let interior = source_text(source, interior_range)?;
    let retained_range = ruff_python_ast::token::parenthesized_range(
        optional.slice.as_ref().into(),
        optional.into(),
        tokens,
    )
    .unwrap_or_else(|| optional.slice.range());
    let retained = source_text(source, retained_range)?;
    if interior == retained && retained_range != optional.slice.range() {
        return Some(retained.to_owned());
    }
    if !range_contains_annotation_trivia(source, interior_range, facts) {
        return Some(retained.to_owned());
    }
    Some(format!("({interior})"))
}

fn range_contains_annotation_trivia(source: &str, range: TextRange, facts: &AstFacts<'_>) -> bool {
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    let mut previous_end = start;
    for token in facts.candidate_tokens_in_range(range) {
        if matches!(
            token.kind(),
            TokenKind::Comment | TokenKind::Newline | TokenKind::NonLogicalNewline
        ) {
            return true;
        }
        let token_start = usize::from(token.range().start()).clamp(start, end);
        if source[previous_end..token_start].contains(['\n', '\r', '#']) {
            return true;
        }
        previous_end = usize::from(token.range().end()).clamp(previous_end, end);
    }
    source[previous_end..end].contains(['\n', '\r', '#'])
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
