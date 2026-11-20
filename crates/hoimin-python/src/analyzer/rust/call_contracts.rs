//! Mutate explicit argument roles or a consumed result without moving any
//! argument expression into another scope or evaluating it twice.
//!
//! The collector supplies checked parser tokens, lexical facts, source-start
//! selection and a bounded sink. Each producer stops on cancellation or sink
//! rejection. Adjacent pairs have distinct starts, so each gets its own
//! selection check before a replacement is built.

use super::{
    AnalysisCancelled, AstFacts, NameResolution,
    callable_imports::CallableImports,
    function_body,
    optional_keywords::{FunctionIndex, Signature},
    processing_calls::Builtins,
    source_delimiters::DelimiterDepth,
    source_text,
};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::token::{TokenKind, Tokens, parenthesized_range};
use ruff_python_ast::{Expr, ExprCall, Keyword, ModModule, Number, Stmt, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

const ESCAPE: MutationOperator = MutationOperator::StdlibEscapeRemove;
const SWAP: MutationOperator = MutationOperator::CallArgumentSwap;
const DEFAULT: MutationOperator = MutationOperator::CallResultDefault;

#[derive(Default)]
pub(super) struct Contracts {
    active: bool,
    escapes: Vec<CallableImports>,
    builtin_namespace_stable: bool,
    declared_results: HashMap<TextSize, &'static str>,
}

impl Contracts {
    pub(super) fn build(
        module: &ModModule,
        facts: &AstFacts<'_>,
        operators: &MutationOperatorSelection,
        builtins: &Builtins,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        if ![ESCAPE, SWAP, DEFAULT]
            .into_iter()
            .any(|op| operators.contains(op))
        {
            return Ok(Self::default());
        }
        let mut escapes = Vec::new();
        if [ESCAPE, SWAP].into_iter().any(|op| operators.contains(op)) {
            for family in [&["re"], &["html"], &["urllib.parse"]] {
                if has_import_family(module, family[0], cancelled)? {
                    escapes.push(CallableImports::build_family(module, cancelled, family)?);
                }
            }
        }
        let builtin_namespace_stable = builtins.ordinary_namespace();
        let mut declared_results = HashMap::new();
        if builtin_namespace_stable
            && [ESCAPE, DEFAULT]
                .into_iter()
                .any(|op| operators.contains(op))
        {
            for statement in &module.body {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                if let Stmt::FunctionDef(definition) = statement
                    && let Some(result) = definition
                        .returns
                        .as_deref()
                        .and_then(|annotation| annotation_default(annotation, facts))
                    && function_body::ordinary_result(definition, cancelled)
                {
                    declared_results.insert(definition.start(), result);
                }
            }
        }
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        Ok(Self {
            active: true,
            escapes,
            builtin_namespace_stable,
            declared_results,
        })
    }

    pub(super) fn active(&self) -> bool {
        self.active
    }

    fn builtin<'a>(
        &self,
        call: &'a ExprCall,
        cx: &Context<'_, '_, impl Fn() -> bool>,
    ) -> Option<&'a str> {
        let name = call.func.as_name_expr()?;
        let id = name.id.as_str();
        (self.builtin_namespace_stable
            && cx
                .facts
                .name_resolution
                .resolution(usize::from(name.start()), id)
                == NameResolution::DefinitelyBuiltin)
            .then_some(id)
    }

    fn result_contract(
        &self,
        call: &ExprCall,
        awaited: bool,
        cx: &Context<'_, '_, impl Fn() -> bool>,
    ) -> Option<&'static str> {
        if let Some(signature) = cx.functions.signature(call) {
            let definition = signature.definition;
            return (definition.is_async == awaited)
                .then(|| self.declared_results.get(&definition.start()).copied())
                .flatten();
        }
        if awaited {
            return None;
        }
        let id = self.builtin(call, cx)?;
        match id {
            "all" | "any" | "callable" | "hasattr" | "isinstance" | "issubclass" => Some("False"),
            "hash" | "id" | "len" | "ord" => Some("0"),
            "ascii" | "bin" | "chr" | "format" | "hex" | "oct" | "repr" => Some("''"),
            "sorted" => Some("[]"),
            _ => literal_default(id),
        }
    }

    fn escape_signature(&self, call: &ExprCall) -> Option<EscapeSignature> {
        let callable = self
            .escapes
            .iter()
            .find_map(|imports| imports.resolve(&call.func))?;
        match (callable.module, callable.member) {
            ("re", "escape") => Some(EscapeSignature::Regex),
            ("html", "escape") => Some(EscapeSignature::Html),
            ("urllib.parse", "quote" | "quote_plus") => Some(EscapeSignature::Url),
            _ => None,
        }
    }

    /// A non-None result span is the consumed call, or the enclosing await
    /// for a syntactically direct operand. Argument operators use the call
    /// range independently of that value-use context.
    pub(super) fn emit(
        &self,
        call: &ExprCall,
        result_span: Option<ResultSpan>,
        cx: &Context<'_, '_, impl Fn() -> bool>,
        mut selected: impl FnMut(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) -> bool {
        if cx.operators.contains(DEFAULT)
            && let Some(span) = result_span
            && selected(span.range)
            && let Some(default) = self.result_contract(call, span.awaited, cx)
            && cx.delimiters.can_enclose(span.range, 1)
            && (default != "{*()}" || cx.delimiters.room_before(span.range.start(), 3))
            && let Some(original) = source_text(cx.source, span.range)
            && ((cx.cancelled)()
                || !emit(span.range, format!("({original}, {default})[1]"), DEFAULT))
        {
            return false;
        }

        let escape = cx.operators.contains(ESCAPE) && selected(call.range());
        let swap = cx.operators.contains(SWAP) && call.arguments.len() >= 2;
        if (!escape && !swap) || !explicit_arguments(call, cx.cancelled) {
            return !(cx.cancelled)();
        }
        let signature = self.escape_signature(call);
        let known = signature.map(EscapeSignature::shape).or_else(|| {
            Some(match self.builtin(call, cx)? {
                "sorted" => StaticSignature {
                    names: &["iterable", "key", "reverse"],
                    positional_only: 1,
                    positional: 1,
                },
                "len" => StaticSignature {
                    names: &["obj"],
                    positional_only: 1,
                    positional: 1,
                },
                _ => return None,
            })
        });
        if escape
            && let Some(signature) = signature
            && let Some(primary) = signature.shape().bind(call)
            && self.escape_type_compatible(signature, primary.value, cx)
            && let Some(replacement) = escape_replacement(call, primary.index, cx)
            && ((cx.cancelled)() || !emit(call.range(), replacement, ESCAPE))
        {
            return false;
        }
        if swap {
            let local = self
                .builtin_namespace_stable
                .then(|| cx.functions.signature(call))
                .flatten();
            return swap_arguments(call, known, local, cx.delimiters, cx, selected, emit);
        }
        !(cx.cancelled)()
    }
    fn escape_type_compatible(
        &self,
        signature: EscapeSignature,
        value: &Expr,
        cx: &Context<'_, '_, impl Fn() -> bool>,
    ) -> bool {
        if matches!(value, Expr::Generator(_)) {
            return false;
        }
        if matches!(signature, EscapeSignature::Regex) {
            return true;
        }
        let known_type = match value {
            Expr::StringLiteral(_) | Expr::FString(_) => Some("''"),
            Expr::BytesLiteral(_) => Some("b''"),
            Expr::Name(name)
                if self.builtin_namespace_stable
                    && cx.facts.name_resolution.stable_text_parameter(name) =>
            {
                Some("''")
            }
            Expr::Call(call) => self.result_contract(call, false, cx),
            _ => None,
        };
        match signature {
            EscapeSignature::Regex => unreachable!("regex accepts text and bytes"),
            EscapeSignature::Html => known_type != Some("b''"),
            EscapeSignature::Url => known_type == Some("''"),
        }
    }
}

pub(super) struct Context<'a, 'ast, F> {
    pub(super) facts: &'a AstFacts<'ast>,
    pub(super) functions: &'a FunctionIndex<'ast>,
    pub(super) source: &'ast str,
    pub(super) operators: &'a MutationOperatorSelection,
    pub(super) cancelled: &'a F,
    pub(super) ordinary_name_reads: bool,
    pub(super) delimiters: &'a DelimiterDepth,
}

impl<F: Fn() -> bool> Context<'_, '_, F> {
    fn tokens(&self) -> &Tokens {
        self.facts.tokens.expect("parser tokens are set")
    }
}

pub(super) struct ResultSpan {
    pub(super) range: TextRange,
    pub(super) awaited: bool,
}

#[derive(Clone, Copy)]
enum EscapeSignature {
    Regex,
    Html,
    Url,
}

struct Primary<'a> {
    index: usize,
    value: &'a Expr,
}

impl EscapeSignature {
    fn shape(self) -> StaticSignature {
        let names: &'static [&'static str] = match self {
            Self::Regex => &["pattern"],
            Self::Html => &["s", "quote"],
            Self::Url => &["string", "safe", "encoding", "errors"],
        };
        StaticSignature {
            names,
            positional_only: 0,
            positional: names.len(),
        }
    }
}

// These known signatures each have one required first parameter.
#[derive(Clone, Copy)]
struct StaticSignature {
    names: &'static [&'static str],
    positional_only: usize,
    positional: usize,
}

impl StaticSignature {
    fn bind(self, call: &ExprCall) -> Option<Primary<'_>> {
        let args = &call.arguments;
        if args.args.len() > self.positional || args.len() > self.names.len() {
            return None;
        }
        let mut primary = args.args.first().map(|value| Primary { index: 0, value });
        for (index, keyword) in args.keywords.iter().enumerate() {
            let name = keyword.arg.as_ref()?.as_str();
            let slot = self.names.iter().position(|candidate| *candidate == name)?;
            if slot < self.positional_only || slot < args.args.len() {
                return None;
            }
            if slot == 0 {
                primary = Some(Primary {
                    index: args.args.len() + index,
                    value: &keyword.value,
                });
            }
        }
        primary
    }

    fn positional_keyword_name(self, index: usize) -> Option<&'static str> {
        (index >= self.positional_only && index < self.positional).then(|| self.names[index])
    }
}

fn has_import_family(
    module: &ModModule,
    name: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<bool, AnalysisCancelled> {
    let package = name.split_once('.').map_or(name, |(package, _)| package);
    for statement in &module.body {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        match statement {
            Stmt::Import(import) if !import.is_lazy => {
                for alias in &import.names {
                    if cancelled() {
                        return Err(AnalysisCancelled);
                    }
                    if [name, package].contains(&alias.name.as_str()) {
                        return Ok(true);
                    }
                }
            }
            Stmt::ImportFrom(import)
                if !import.is_lazy
                    && import.level == 0
                    && import
                        .module
                        .as_ref()
                        .is_some_and(|source| [name, package].contains(&source.as_str())) =>
            {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

fn explicit_arguments(call: &ExprCall, cancelled: &impl Fn() -> bool) -> bool {
    for arg in &call.arguments.args {
        if cancelled() || arg.is_starred_expr() {
            return false;
        }
    }
    let mut names = HashSet::new();
    for keyword in &call.arguments.keywords {
        if cancelled()
            || !keyword
                .arg
                .as_ref()
                .is_some_and(|name| names.insert(name.as_str()))
        {
            return false;
        }
    }
    true
}

fn annotation_default(annotation: &Expr, facts: &AstFacts<'_>) -> Option<&'static str> {
    let identity = match annotation {
        Expr::Name(name) => name,
        Expr::Subscript(subscript) => {
            let name = subscript.value.as_name_expr()?;
            if !matches!(name.id.as_str(), "list" | "tuple" | "dict" | "set") {
                return None;
            }
            name
        }
        _ => return None,
    };
    let name = identity.id.as_str();
    (facts
        .name_resolution
        .annotation_resolution(usize::from(identity.start()), name)
        == NameResolution::DefinitelyBuiltin)
        .then(|| literal_default(name))
        .flatten()
}

fn literal_default(name: &str) -> Option<&'static str> {
    match name {
        "bool" => Some("False"),
        "int" => Some("0"),
        "float" => Some("0.0"),
        "complex" => Some("0j"),
        "str" => Some("''"),
        "bytes" => Some("b''"),
        "list" => Some("[]"),
        "tuple" => Some("()"),
        "dict" => Some("{}"),
        "set" => Some("{*()}"),
        _ => None,
    }
}

fn equals_token(keyword: &Keyword, tokens: &Tokens) -> Option<TextRange> {
    let name = keyword.arg.as_ref()?;
    tokens
        .in_range(TextRange::new(name.end(), keyword.value.start()))
        .iter()
        .find(|token| token.kind() == TokenKind::Equal)
        .map(Ranged::range)
}

fn escape_replacement(
    call: &ExprCall,
    primary: usize,
    cx: &Context<'_, '_, impl Fn() -> bool>,
) -> Option<String> {
    evaluated_arguments(call, primary, None, cx.source, cx.tokens(), cx.cancelled)
}

/// Evaluate the supplied arguments in Python's original order and retain one
/// value. An optional leading value represents the receiver of a bound call.
pub(super) fn evaluated_arguments(
    call: &ExprCall,
    retained_index: usize,
    receiver: Option<&str>,
    source: &str,
    tokens: &Tokens,
    cancelled: &dyn Fn() -> bool,
) -> Option<String> {
    let args = &call.arguments;
    let mut cursor = args.start();
    let mut replacement = String::new();
    if let Some(receiver) = receiver {
        replacement.push('(');
        replacement.push_str(receiver);
        replacement.push(',');
        cursor += TextSize::new(1);
    }
    for keyword in &args.keywords {
        if cancelled() {
            return None;
        }
        let name = keyword.arg.as_ref()?;
        let equal = equals_token(keyword, tokens)?;
        replacement.push_str(source_text(source, TextRange::new(cursor, name.start()))?);
        replacement.push_str(source_text(
            source,
            TextRange::new(name.end(), equal.start()),
        )?);
        cursor = equal.end();
    }
    if args.len() == 1 && receiver.is_none() {
        // Grouping alone suffices. Remove a trailing comma to keep the value
        // from becoming a tuple. The original interior trivia stays in place.
        let last = tokens
            .in_range(args.range())
            .iter()
            .rev()
            .filter(|token| !token.kind().is_trivia())
            .nth(1)?;
        if last.kind() == TokenKind::Comma {
            replacement.push_str(source_text(source, TextRange::new(cursor, last.start()))?);
            cursor = last.end();
        }
    }
    replacement.push_str(source_text(source, TextRange::new(cursor, args.end()))?);
    if args.len() > 1 || receiver.is_some() {
        write!(replacement, "[{retained_index}]").expect("write to String");
    }
    Some(replacement)
}

fn argument_range(expr: &Expr, call: &ExprCall, tokens: &Tokens) -> TextRange {
    parenthesized_range(expr.into(), (&call.arguments).into(), tokens)
        .unwrap_or_else(|| expr.range())
}

fn swap_arguments(
    call: &ExprCall,
    known: Option<StaticSignature>,
    signature: Option<&Signature<'_>>,
    delimiters: &DelimiterDepth,
    cx: &Context<'_, '_, impl Fn() -> bool>,
    mut selected: impl FnMut(TextRange) -> bool,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) -> bool {
    let args = &call.arguments;
    // The first selected, eligible pair proves the original call binding.
    // Later pairs reuse that proof, including a failed binding.
    let binding = OnceCell::new();
    let bound_slots = || {
        binding
            .get_or_init(|| {
                if let Some(shape) = known {
                    shape.bind(call)?;
                }
                signature.map_or_else(
                    || Some(Vec::new()),
                    |signature| signature.bind_explicit(call, cx.cancelled, || {}),
                )
            })
            .as_deref()
    };
    let tokens = cx.tokens();
    for pair in args.args.windows(2) {
        if (cx.cancelled)() {
            return false;
        }
        let [first, second] = pair else {
            unreachable!("two argument window")
        };
        let range = TextRange::new(
            argument_range(first, call, tokens).start(),
            argument_range(second, call, tokens).end(),
        );
        if selected(range)
            && delimiters.can_enclose(range, 1)
            && !same_simple_value(first, second, cx.ordinary_name_reads)
            && bound_slots().is_some()
            && let Some(original) = source_text(cx.source, range)
            && !emit(range, format!("*({original})[::-1]"), SWAP)
        {
            return false;
        }
    }

    if known.is_none() && signature.is_none() {
        return !(cx.cancelled)();
    }
    let explicit_slot = |index: usize| {
        bound_slots()
            .is_some_and(|slots| known.is_some() || slots.get(index).is_some_and(Option::is_some))
    };
    if let (Some(positional), Some(keyword)) = (args.args.last(), args.keywords.first()) {
        let position = args.args.len() - 1;
        let formal = signature
            .and_then(|signature| signature.positional_keyword_name(position))
            .or_else(|| known.and_then(|sig| sig.positional_keyword_name(position)));
        let first = argument_range(positional, call, tokens);
        let last = argument_range(&keyword.value, call, tokens);
        let range = TextRange::new(first.start(), last.end());
        if selected(range)
            && delimiters.can_enclose(range, 2)
            && !same_simple_value(positional, &keyword.value, cx.ordinary_name_reads)
            && let Some(formal) = formal
            && explicit_slot(0)
            && let Some(replacement) = mixed_swap(formal, keyword, first, last, cx)
            && ((cx.cancelled)() || !emit(range, replacement, SWAP))
        {
            return false;
        }
    }
    for (index, pair) in args.keywords.windows(2).enumerate() {
        if (cx.cancelled)() {
            return false;
        }
        let [first, second] = pair else {
            unreachable!("two keyword window")
        };
        let (Some(left), Some(right)) = (&first.arg, &second.arg) else {
            continue;
        };
        let range = TextRange::new(left.start(), right.end());
        if selected(range)
            && !same_simple_value(&first.value, &second.value, cx.ordinary_name_reads)
            && explicit_slot(index)
            && explicit_slot(index + 1)
            && let (Some(left_text), Some(between), Some(right_text)) = (
                source_text(cx.source, left.range()),
                source_text(cx.source, TextRange::new(left.end(), right.start())),
                source_text(cx.source, right.range()),
            )
            && !emit(range, format!("{right_text}{between}{left_text}"), SWAP)
        {
            return false;
        }
    }
    !(cx.cancelled)()
}

fn mixed_swap(
    formal: &str,
    keyword: &Keyword,
    first: TextRange,
    last: TextRange,
    cx: &Context<'_, '_, impl Fn() -> bool>,
) -> Option<String> {
    let key = keyword.arg.as_ref()?;
    let equals = equals_token(keyword, cx.tokens())?;
    let first_text = source_text(cx.source, first)?;
    let separator = source_text(cx.source, TextRange::new(first.end(), key.start()))?;
    let after_key = source_text(cx.source, TextRange::new(key.end(), equals.start()))?;
    let last_text = source_text(cx.source, TextRange::new(equals.end(), last.end()))?;
    Some(format!(
        "**{{'{}': ({first_text}){separator}'{formal}': ({after_key}{last_text})}}",
        key.as_str()
    ))
}

fn same_simple_value(left: &Expr, right: &Expr, ordinary_name_reads: bool) -> bool {
    match (left, right) {
        (Expr::Name(left), Expr::Name(right)) => ordinary_name_reads && left.id == right.id,
        (Expr::BooleanLiteral(left), Expr::BooleanLiteral(right)) => left.value == right.value,
        (Expr::NoneLiteral(_), Expr::NoneLiteral(_))
        | (Expr::EllipsisLiteral(_), Expr::EllipsisLiteral(_)) => true,
        (Expr::StringLiteral(left), Expr::StringLiteral(right)) => {
            left.value.to_str() == right.value.to_str()
        }
        (Expr::BytesLiteral(left), Expr::BytesLiteral(right)) => {
            left.value.bytes().eq(right.value.bytes())
        }
        _ => signed_number(left)
            .zip(signed_number(right))
            .is_some_and(|(left, right)| same_number(left, right)),
    }
}

fn signed_number(expression: &Expr) -> Option<(&Number, bool)> {
    match expression {
        Expr::NumberLiteral(literal) => Some((&literal.value, false)),
        Expr::UnaryOp(unary) if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub) => {
            let literal = unary.operand.as_number_literal_expr()?;
            Some((&literal.value, unary.op == UnaryOp::USub))
        }
        _ => None,
    }
}

fn same_number(
    (left, left_negative): (&Number, bool),
    (right, right_negative): (&Number, bool),
) -> bool {
    let same_float = |left: f64, right: f64| {
        left.to_bits()
            == (if left_negative == right_negative {
                right
            } else {
                -right
            })
            .to_bits()
    };
    match (left, right) {
        (Number::Int(left), Number::Int(right)) => {
            left == right && (left_negative == right_negative || left.as_u64() == Some(0))
        }
        (Number::Float(left), Number::Float(right)) => same_float(*left, *right),
        (Number::Complex { real: lr, imag: li }, Number::Complex { real: rr, imag: ri }) => {
            same_float(*lr, *rr) && same_float(*li, *ri)
        }
        _ => false,
    }
}
