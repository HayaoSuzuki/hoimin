//! Narrow sorting-key and explicit encoding-policy mutations.
//!
//! Method matching is nominal: a custom encode/decode method can match. Reads in
//! a sorting key can run user code, so their evaluation order intentionally moves.
use super::{AnalysisCancelled, callable_imports::CallableImports};
use crate::visitor::{self, Visitor};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprCall, ModModule, Number, Stmt, token::Tokens};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Imports {
    codecs: Option<CallableImports>,
    codec_module_names: HashSet<String>,
}

impl Imports {
    pub(super) fn build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        if !operators.contains(MutationOperator::EncodingErrorsIgnore) {
            return Ok(Self::default());
        }
        let mut aliases = CodecsModuleNames {
            names: HashSet::new(),
            present: false,
            cancelled,
            stopped: false,
        };
        aliases.visit_body(&module.body);
        if aliases.stopped || cancelled() {
            return Err(AnalysisCancelled);
        }
        let codecs = if aliases.present {
            Some(CallableImports::build(module, cancelled, &["codecs"])?)
        } else {
            None
        };
        Ok(Self {
            codecs,
            codec_module_names: aliases.names,
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the shared call emitter receives source, tokens, selection, identity and cancellation evidence"
    )]
    pub(super) fn emit(
        &self,
        call: &ExprCall,
        source: &str,
        tokens: &Tokens,
        operators: &MutationOperatorSelection,
        sorted_is_builtin: bool,
        cancelled: &impl Fn() -> bool,
        mut selected: impl FnMut(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) -> bool {
        if cancelled() {
            return false;
        }
        let sorting = operators.contains(MutationOperator::SortKeySwap)
            && sorted_is_builtin
            && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "sorted");
        let encoding = operators.contains(MutationOperator::EncodingErrorsIgnore)
            && (matches!(call.func.as_ref(), Expr::Attribute(method) if matches!(method.attr.as_str(), "encode" | "decode"))
                || self.codecs.as_ref().is_some_and(|imports| {
                    imports.resolve(&call.func).is_some_and(|identity| {
                        identity.module == "codecs"
                            && matches!(identity.member, "encode" | "decode")
                    })
                }));
        if !sorting && !encoding {
            return true;
        }
        // Dynamic expansions can supply duplicate keywords or change the call
        // contract. Reject them before either producer allocates a replacement.
        let mut names = HashSet::new();
        for keyword in &call.arguments.keywords {
            if cancelled() {
                return false;
            }
            let Some(name) = keyword.arg.as_ref() else {
                return true;
            };
            if !names.insert(name.as_str()) {
                return true;
            }
        }
        for argument in &call.arguments.args {
            if cancelled() {
                return false;
            }
            if matches!(argument, Expr::Starred(_)) {
                return true;
            }
        }
        if sorting
            && let Some((range, replacement)) =
                sort_swap(call, source, tokens, cancelled, &mut selected)
            && (cancelled() || !emit(range, replacement, MutationOperator::SortKeySwap))
        {
            return false;
        }
        if encoding
            && let Some(range) = self.encoding_literal(call, cancelled)
            && selected(range)
            && (cancelled()
                || !emit(
                    range,
                    "'ignore'".to_owned(),
                    MutationOperator::EncodingErrorsIgnore,
                ))
        {
            return false;
        }
        !cancelled()
    }

    fn encoding_literal(
        &self,
        call: &ExprCall,
        cancelled: &impl Fn() -> bool,
    ) -> Option<TextRange> {
        let trusted = self.codecs.as_ref().is_some_and(|imports| {
            imports.resolve(&call.func).is_some_and(|identity| {
                identity.module == "codecs" && matches!(identity.member, "encode" | "decode")
            })
        });
        if !trusted {
            // An uncertain imported codecs binding must not become a nominal
            // receiver match after losing its imported-callable proof.
            if self.codecs.as_ref().is_some_and(|imports| {
                imports.may_reference(&call.func, "codecs", "encode")
                    || imports.may_reference(&call.func, "codecs", "decode")
            }) {
                return None;
            }
            let Expr::Attribute(method) = call.func.as_ref() else {
                return None;
            };
            if !matches!(method.attr.as_str(), "encode" | "decode")
                || !simple_receiver(&method.value, cancelled)
                || matches!(method.value.as_ref(), Expr::Name(name) if self.codec_module_names.contains(name.id.as_str()))
            {
                return None;
            }
        }
        // With errors explicit, at most the object and encoding may be
        // positional for codecs; methods take only the encoding positional.
        if call.arguments.args.len() > if trusted { 2 } else { 1 } {
            return None;
        }
        for keyword in &call.arguments.keywords {
            if cancelled() {
                return None;
            }
            if keyword.arg.as_ref()?.as_str() == "errors" {
                let Expr::StringLiteral(literal) = &keyword.value else {
                    return None;
                };
                return (!literal.value.is_implicit_concatenated()
                    && literal.value.to_str() == "surrogateescape")
                    .then_some(literal.range());
            }
        }
        None
    }
}

// CallableImports deliberately drops all evidence on namespace uncertainty.
// Retain module spellings only as exclusions so that failure cannot turn an
// imported codecs call into a nominal method call. Identity never comes from
// this scan, and collecting nested bindings only makes the rule conservative.
struct CodecsModuleNames<'a, F> {
    names: HashSet<String>,
    present: bool,
    cancelled: &'a F,
    stopped: bool,
}

impl<'ast, F: Fn() -> bool> Visitor<'ast> for CodecsModuleNames<'_, F> {
    fn visit_body(&mut self, body: &'ast [Stmt]) {
        for statement in body {
            if self.stopped || (self.cancelled)() {
                self.stopped = true;
                return;
            }
            self.visit_stmt(statement);
        }
    }

    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if self.stopped || (self.cancelled)() {
            self.stopped = true;
            return;
        }
        if let Stmt::Import(import) = statement {
            for alias in &import.names {
                if (self.cancelled)() {
                    self.stopped = true;
                    return;
                }
                if alias.name.as_str() == "codecs" {
                    self.present = true;
                    self.names
                        .insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
        }
        if let Stmt::ImportFrom(import) = statement
            && import.level == 0
            && import
                .module
                .as_ref()
                .is_some_and(|module| module.as_str() == "codecs")
        {
            self.present = true;
        }
        visitor::walk_stmt(self, statement);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.stopped || (self.cancelled)() {
            self.stopped = true;
            return;
        }
        visitor::walk_expr(self, expression);
    }
}

fn sort_swap(
    call: &ExprCall,
    source: &str,
    tokens: &Tokens,
    cancelled: &impl Fn() -> bool,
    selected: &mut impl FnMut(TextRange) -> bool,
) -> Option<(TextRange, String)> {
    if call.arguments.args.len() != 1 {
        return None;
    }
    let mut key = None;
    for keyword in &call.arguments.keywords {
        if cancelled() {
            return None;
        }
        match keyword.arg.as_ref()?.as_str() {
            "key" => key = Some(&keyword.value),
            "reverse" => {}
            _ => return None,
        }
    }
    let Expr::Lambda(lambda) = key? else {
        return None;
    };
    let Expr::Tuple(tuple) = lambda.body.as_ref() else {
        return None;
    };
    let [first, second] = tuple.elts.as_slice() else {
        return None;
    };
    if !simple_read(first, cancelled)
        || !simple_read(second, cancelled)
        || same_read(first, second, cancelled)
        || cancelled()
    {
        return None;
    }
    let first_range =
        ruff_python_ast::token::parenthesized_range(first.into(), tuple.into(), tokens)
            .unwrap_or_else(|| first.range());
    let second_range =
        ruff_python_ast::token::parenthesized_range(second.into(), tuple.into(), tokens)
            .unwrap_or_else(|| second.range());
    let range = TextRange::new(first_range.start(), second_range.end());
    if !selected(range) || cancelled() {
        return None;
    }
    let first_text = super::source_text(source, first_range)?;
    let second_text = super::source_text(source, second_range)?;
    // Both neighboring slices validated these UTF-8 endpoints; parser tuple
    // ordering places the separator between them within the same source.
    let separator = &source[usize::from(first_range.end())..usize::from(second_range.start())];
    Some((range, format!("{second_text}{separator}{first_text}")))
}

fn simple_receiver(expression: &Expr, cancelled: &impl Fn() -> bool) -> bool {
    if cancelled() {
        return false;
    }
    match expression {
        Expr::Name(_) => true,
        Expr::Attribute(attribute) => simple_receiver(&attribute.value, cancelled),
        _ => false,
    }
}

fn simple_read(expression: &Expr, cancelled: &impl Fn() -> bool) -> bool {
    if cancelled() {
        return false;
    }
    match expression {
        Expr::Name(_) => true,
        Expr::Attribute(attribute) => simple_read(&attribute.value, cancelled),
        Expr::Subscript(subscript) => {
            simple_read(&subscript.value, cancelled) && literal_index(&subscript.slice)
        }
        _ => false,
    }
}

fn literal_index(expression: &Expr) -> bool {
    matches!(expression, Expr::StringLiteral(value) if !value.value.is_implicit_concatenated())
        || matches!(expression, Expr::NumberLiteral(value) if matches!(value.value, Number::Int(_)))
}

fn same_read(first: &Expr, second: &Expr, cancelled: &impl Fn() -> bool) -> bool {
    if cancelled() {
        return false;
    }
    match (first, second) {
        (Expr::Name(a), Expr::Name(b)) => a.id == b.id,
        (Expr::Attribute(a), Expr::Attribute(b)) => {
            a.attr.as_str() == b.attr.as_str() && same_read(&a.value, &b.value, cancelled)
        }
        (Expr::Subscript(a), Expr::Subscript(b)) => {
            same_read(&a.value, &b.value, cancelled)
                && match (a.slice.as_ref(), b.slice.as_ref()) {
                    (Expr::StringLiteral(a), Expr::StringLiteral(b)) => {
                        a.value.to_str() == b.value.to_str()
                    }
                    (Expr::NumberLiteral(a), Expr::NumberLiteral(b)) => a.value == b.value,
                    _ => false,
                }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::Imports;
    use hoimin_core::MutationOperatorSelection;

    #[rstest::rstest]
    #[case::data_decode_errors_surrogateescape("data.decode(errors='surrogateescape')\n", false)]
    #[case::import_unrelated("import unrelated\n", false)]
    #[case::import_codecs_as_c("import codecs as c\n", true)]
    #[case::from_codecs_import_decode_as_dec("from codecs import decode as dec\n", true)]
    #[case::def_f_import_codecs_as_c("def f():\n    import codecs as c\n", true)]
    fn codec_identity_scan_requires_a_codec_import(#[case] source: &str, #[case] expected: bool) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let imports = Imports::build(
            parsed.syntax(),
            &MutationOperatorSelection::default(),
            &|| false,
        )
        .unwrap();

        assert_eq!(imports.codecs.is_some(), expected, "{source}");
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn run_encoding(
        imports: &Imports,
        call: &ExprCall,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<TextRange> {
        imports.encoding_literal(call, &cancelled)
    }

    #[rstest::rstest]
    #[case("decode(errors='surrogateescape')")]
    #[case("data.other(errors='surrogateescape')")]
    #[case("data.decode('utf8', 'extra', errors='surrogateescape')")]
    #[case("data.decode(encoding='utf8')")]
    #[case("data.decode(**options)")]
    #[case("data.decode(errors=policy)")]
    #[case("data.decode(errors='surrogate' 'escape')")]
    fn encoding_policy_requires_a_simple_method_and_explicit_single_literal(#[case] source: &str) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();

        let actual = run_encoding(
            &Imports::default(),
            parsed.syntax().body.as_call_expr().unwrap(),
            &|| false,
        );

        assert_eq!(actual, None);
    }

    fn run_same(first: &Expr, second: &Expr, cancelled: &dyn Fn() -> bool) -> bool {
        same_read(first, second, &cancelled)
    }

    #[rstest::rstest]
    #[case("first", "first", false, true)]
    #[case("first", "second", false, false)]
    #[case("item.key", "item.key", false, true)]
    #[case("item.key", "item.other", false, false)]
    #[case("item['key']", "item['key']", false, true)]
    #[case("item[1]", "item[1]", false, true)]
    #[case("item[1]", "item['1']", false, false)]
    #[case("item", "item.key", false, false)]
    #[case("item", "item", true, false)]
    fn sorting_read_identity_preserves_access_shape_and_literal_type(
        #[case] first: &str,
        #[case] second: &str,
        #[case] cancelled: bool,
        #[case] expected: bool,
    ) {
        let first = ruff_python_parser::parse_expression(first).unwrap();
        let second = ruff_python_parser::parse_expression(second).unwrap();

        let actual = run_same(&first.syntax().body, &second.syntax().body, &|| cancelled);

        assert_eq!(actual, expected);
    }

    fn run_sort(
        call: &ExprCall,
        source: &str,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
        mut selected: &mut dyn FnMut(TextRange) -> bool,
    ) -> Option<(TextRange, String)> {
        sort_swap(call, source, tokens, &cancelled, &mut selected)
    }

    #[rstest::rstest]
    #[case("", true, false)]
    #[case("sorted(items, key=lambda item: (item.a,", true, false)]
    #[case("sorted(items, key=lambda item: (item.a, item.b))", false, false)]
    #[case("sorted(items, key=lambda item: (item.a, item.b))", true, true)]
    fn sorting_key_edits_require_available_source_and_selected_span(
        #[case] source: &str,
        #[case] selected: bool,
        #[case] expected: bool,
    ) {
        let parsed =
            ruff_python_parser::parse_module("sorted(items, key=lambda item: (item.a, item.b))")
                .unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();

        let actual =
            run_sort(call, source, parsed.tokens(), &|| false, &mut |_| selected).is_some();

        assert_eq!(actual, expected);
    }
    #[rstest::rstest]
    #[case("data.decode(errors='surrogateescape')")]
    #[case("holder.data.decode(encoding='utf8', errors='surrogateescape')")]
    fn encoding_policy_retains_the_explicit_error_literal_span(#[case] source: &str) {
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let range = run_encoding(
            &Imports::default(),
            parsed.syntax().body.as_call_expr().unwrap(),
            &|| false,
        )
        .unwrap();

        let actual = super::super::source_text(source, range);

        assert_eq!(actual, Some("'surrogateescape'"));
    }

    #[rstest::rstest]
    fn encoding_policy_stops_during_receiver_or_keyword_checks(
        #[values(0, 1, 2, 3)] checkpoint: usize,
    ) {
        let source = "holder.data.decode(encoding='utf8', errors='surrogateescape')";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let polls = std::cell::Cell::new(0);

        let actual = run_encoding(
            &Imports::default(),
            parsed.syntax().body.as_call_expr().unwrap(),
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
        );
        let actual_2 = polls.get();

        assert_eq!(actual, None);
        assert_eq!(actual_2, checkpoint + 1);
    }

    #[rstest::rstest]
    #[case("sorted(items, **options)", false)]
    #[case("sorted(first, second, key=lambda item: (item.a, item.b))", false)]
    #[case("sorted(items, key=lambda item: item.a)", false)]
    #[case("sorted(items, key=lambda item: (item.a, item.b, item.c))", false)]
    #[case("sorted(items, key=lambda item: (item[dynamic], item.b))", false)]
    #[case("sorted(items, key=lambda item: (item[1], item['a']))", true)]
    fn sorting_keys_reject_dynamic_contracts_but_allow_distinct_literal_reads(
        #[case] source: &str,
        #[case] expected: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let result = run_sort(call, source, parsed.tokens(), &|| false, &mut |_| true);

        let actual = result.is_some();

        assert_eq!(actual, expected);
    }

    #[rstest::rstest]
    fn sorting_key_stops_at_each_receiver_and_identity_phase(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12)] checkpoint: usize,
    ) {
        let source = "sorted(items, key=lambda item: (item.a, item[1]))";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let total = std::cell::Cell::new(0);
        let baseline_emitted = run_sort(
            call,
            source,
            parsed.tokens(),
            &|| {
                total.set(total.get() + 1);
                false
            },
            &mut |_| true,
        )
        .is_some();
        let polls = std::cell::Cell::new(0);
        let result = run_sort(
            call,
            source,
            parsed.tokens(),
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
            &mut |_| true,
        );
        let cancelled_emitted = result.is_some();

        assert!(baseline_emitted);
        assert_eq!(cancelled_emitted, checkpoint >= total.get());
    }
    fn run_import_build(
        module: &ModModule,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Imports, AnalysisCancelled> {
        let operators = serde_json::from_str("[\"encoding_errors_ignore\"]").unwrap();
        Imports::build(module, &operators, &cancelled)
    }

    #[rstest::rstest]
    fn codecs_import_index_stops_across_alias_and_identity_scans(
        #[values(
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
            46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64
        )]
        checkpoint: usize,
    ) {
        let source = "import codecs as codec_module\nfrom codecs import decode\ndef function(value):\n return codec_module.decode(value, errors='surrogateescape')\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let total = std::cell::Cell::new(0);
        let baseline_built = run_import_build(parsed.syntax(), &|| {
            total.set(total.get() + 1);
            false
        })
        .is_ok();
        let polls = std::cell::Cell::new(0);
        let result = run_import_build(parsed.syntax(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        let cancelled_failed = result.is_err();

        assert!(baseline_built);
        assert_eq!(cancelled_failed, checkpoint < total.get());
    }
    fn run_call(
        imports: &Imports,
        call: &ExprCall,
        source: &str,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
        selected: &mut dyn FnMut(TextRange) -> bool,
        sink: &mut dyn FnMut(TextRange, String, MutationOperator) -> bool,
    ) -> bool {
        let operators = serde_json::from_value(serde_json::json!([
            "sort_key_swap",
            "encoding_errors_ignore"
        ]))
        .unwrap();
        imports.emit(
            call, source, tokens, &operators, true, &cancelled, selected, sink,
        )
    }
    fn collecting(
        replacements: &mut Vec<String>,
        accepted: bool,
    ) -> impl FnMut(TextRange, String, MutationOperator) -> bool + '_ {
        move |_, replacement, _| {
            replacements.push(replacement);
            accepted
        }
    }

    #[rstest::rstest]
    #[case("data.decode(errors='surrogateescape')", true, true, vec!["'ignore'"], true)]
    #[case("data.decode(errors='surrogateescape')", true, false, vec!["'ignore'"], false)]
    #[case("data.decode(errors='surrogateescape')", false, true, vec![], true)]
    #[case("data.decode(errors='surrogateescape', errors='strict')", true, true, vec![], true)]
    #[case("data.decode('utf8', errors='surrogateescape')", true, true, vec!["'ignore'"], true)]
    #[case("data.decode(*arguments, errors='surrogateescape')", true, true, vec![], true)]
    #[case("sorted(items, key=lambda item: (item.a, item.b))", true, true, vec!["item.b, item.a"], true)]
    #[case("sorted(items, key=lambda item: (item.a, item.b), key=other)", true, true, vec![], true)]
    #[case("other(items)", true, true, vec![], true)]
    fn followup_call_contracts_reject_duplicate_or_expanded_arguments_and_stop_on_sink_rejection(
        #[case] source: &str,
        #[case] selected: bool,
        #[case] accepted: bool,
        #[case]
        #[by_ref]
        expected: &[&str],
        #[case] completed: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let imports = run_import_build(parsed.syntax(), &|| false).unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let mut replacements = Vec::new();

        let actual = run_call(
            &imports,
            call,
            source,
            parsed.tokens(),
            &|| false,
            &mut |_| selected,
            &mut collecting(&mut replacements, accepted),
        );

        assert_eq!(actual, completed);
        assert_eq!(replacements, expected);
    }
}
