//! Standard library call contracts. Domains establish identity and binding before
//! passing source ranges and lazy renderers to the collector.
use super::{
    AnalysisCancelled,
    callable_imports::CallableImports,
    processing_calls::{Site, bind, implicit_generator_interior},
};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{ExprCall, ModModule, token::TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

#[path = "stdlib_contracts/encoding.rs"]
mod encoding;
#[path = "stdlib_contracts/numeric.rs"]
mod numeric;
#[path = "stdlib_contracts/options.rs"]
mod options;
#[path = "stdlib_contracts/urls.rs"]
mod urls;
pub(super) use options::boolean_policy;
#[path = "stdlib_contracts/text.rs"]
mod text;

/// Match decoded parts without asking Ruff to cache their joined value. Only
/// the remaining suffixes of the fixed vocabulary are kept while scanning.
fn decoded_choice<T, const N: usize>(
    literal: Option<&ruff_python_ast::ExprStringLiteral>,
    spellings: [(&str, T); N],
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<T>, AnalysisCancelled> {
    let Some(literal) = literal else {
        return Ok(None);
    };
    let mut remaining = spellings.map(|(name, choice)| (Some(name), choice));
    for part in &literal.value {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let mut possible = false;
        for (suffix, _) in &mut remaining {
            *suffix = suffix.and_then(|s| s.strip_prefix(part.as_str()));
            possible |= suffix.is_some();
        }
        if !possible {
            return Ok(None);
        }
    }
    Ok(remaining
        .into_iter()
        .find_map(|(suffix, choice)| (suffix == Some("")).then_some(choice)))
}

pub(super) fn needs_delimiter_index(operators: &MutationOperatorSelection) -> bool {
    [
        MutationOperator::MathIscloseTolerance,
        MutationOperator::MathPrecisionLoss,
        MutationOperator::StatisticsSamplePopulation,
        MutationOperator::HeapPushpopReplace,
        MutationOperator::MapStrictToggle,
        MutationOperator::BatchedStrictToggle,
        MutationOperator::UrlQuoteSwap,
        MutationOperator::UrlQueryPolicy,
        MutationOperator::Base64ValidateToggle,
        MutationOperator::JsonAllowNanToggle,
        MutationOperator::UnicodeDigitAscii,
        MutationOperator::UnicodeNormalizationForm,
        MutationOperator::UnicodeNormalizationRemove,
        MutationOperator::EncodingErrorsIgnore,
        MutationOperator::EncodingErrorsReplace,
        MutationOperator::EncodingErrorsStrict,
    ]
    .into_iter()
    .any(|op| operators.contains(op))
}

#[derive(Default)]
pub(super) struct Imports {
    numeric: numeric::Imports,
    options: options::Imports,
    unicode: CallableImports,
    encoding: encoding::Imports,
}

/// Build the full identity proof only for a selected family with an eager import.
struct SelectedFamilies<'a, F> {
    module: &'a ModModule,
    operators: &'a MutationOperatorSelection,
    cancelled: &'a F,
}

impl<F: Fn() -> bool> SelectedFamilies<'_, F> {
    fn proof(
        &self,
        family: &'static [&'static str; 1],
        operators: &[MutationOperator],
    ) -> Result<CallableImports, AnalysisCancelled> {
        if operators.iter().any(|&op| self.operators.contains(op)) {
            return Ok(CallableImports::build_imported_family(
                self.module,
                self.cancelled,
                family,
            )?
            .unwrap_or_default());
        }
        Ok(CallableImports::default())
    }
}

impl Imports {
    pub(super) fn build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let families = SelectedFamilies {
            module,
            operators,
            cancelled,
        };
        Ok(Self {
            numeric: numeric::Imports::build(&families)?,
            options: options::Imports::build(&families)?,
            unicode: families.proof(
                &["unicodedata"],
                &[
                    MutationOperator::UnicodeNormalizationForm,
                    MutationOperator::UnicodeNormalizationRemove,
                ],
            )?,
            encoding: encoding::Imports::build(module, operators, cancelled)?,
        })
    }
    /// Pass each edit's physical range, line-selection offset, operator and
    /// deferred renderer to the sink. Returning false stops this call's edits.
    pub(super) fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        mut emit: impl FnMut(TextRange, TextSize, MutationOperator, &dyn Fn() -> String) -> bool,
    ) {
        if !(site.cancelled)()
            && text::emit(&self.unicode, call, site, &mut |r, op, render| {
                emit(r, r.start(), op, render)
            })
            && !(site.cancelled)()
        {
            if !self.options.emit(call, site, &mut emit) || (site.cancelled)() {
                return;
            }
            if !self.numeric.emit(call, site, &mut emit) || (site.cancelled)() {
                return;
            }
            self.encoding.emit(call, site, &mut emit);
        }
    }
}

#[derive(Clone, Copy)]
enum KeywordSpan {
    Call,
    Arguments,
}

/// Derive punctuation and grouping from the original source. An option edit
/// covers its argument list (only the interior for an implicit generator).
/// Codec edits historically cover the whole call. Both select the callee line.
fn insert_keyword(
    call: &ExprCall,
    site: &Site<'_>,
    op: MutationOperator,
    span: KeywordSpan,
    keyword: std::fmt::Arguments<'_>,
    emit: &mut impl FnMut(TextRange, TextSize, MutationOperator, &dyn Fn() -> String) -> bool,
) -> bool {
    if (site.cancelled)() {
        return false;
    }
    let args = call.arguments.range();
    let generator = implicit_generator_interior(call);
    let range = match span {
        KeywordSpan::Call => call.range(),
        KeywordSpan::Arguments => generator.unwrap_or(args),
    };
    if let Some(interior) = generator {
        if site.delimiters.can_enclose(interior, 1) {
            let before = site
                .text(TextRange::new(range.start(), interior.start()))
                .unwrap();
            let value = site.text(interior).unwrap();
            let after = site
                .text(TextRange::new(interior.end(), range.end()))
                .unwrap();
            return emit(range, call.start(), op, &|| {
                format!("{before}({value}), {keyword}{after}")
            });
        }
        return true;
    }
    let closing = args.end() - TextSize::new(1);
    for token in site
        .tokens()
        .in_range(TextRange::new(args.start(), closing))
        .iter()
        .rev()
    {
        if (site.cancelled)() {
            return false;
        }
        if !token.kind().is_trivia() {
            let comma = if matches!(token.kind(), TokenKind::Comma | TokenKind::Lpar) {
                ""
            } else {
                ", "
            };
            let prefix = site.text(TextRange::new(range.start(), closing)).unwrap();
            return emit(range, call.start(), op, &|| {
                format!("{prefix}{comma}{keyword})")
            });
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::{AstFacts, processing_calls::Builtins, source_delimiters::DelimiterDepth};
    use super::*;
    use std::cell::Cell;

    #[rstest::rstest]
    #[case::import_free("", 0)]
    #[case::unrelated("import unrelated", 0)]
    #[case::nested("def local():\n    import math", 0)]
    #[case::relative("from .math import isclose", 0)]
    #[case::lazy("lazy import math", 0)]
    #[case::module("import math as m", 1)]
    #[case::direct("from statistics import variance", 1)]
    #[case::dotted("import urllib.parse", 1)]
    #[case::package_member("from urllib import parse as p", 1)]
    #[case::two_families("import json, itertools as it", 2)]
    fn final_review_class_lookup_indexes_require_relevant_imports(
        #[case] imports: &str,
        #[case] expected: usize,
    ) {
        // Arrange
        use super::super::callable_imports::CLASS_LOOKUP_INDEX_BUILDS;
        use std::fmt::Write;
        let mut source = format!("{imports}\nclass Record:\n");
        for i in 0..512 {
            writeln!(source, "    field_{i} = value_{i}").unwrap();
        }
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let before = CLASS_LOOKUP_INDEX_BUILDS.with(Cell::get);

        // Act
        let _imports = Imports::build(
            parsed.syntax(),
            &MutationOperatorSelection::default(),
            &|| false,
        )
        .unwrap();

        // Assert
        assert_eq!(CLASS_LOOKUP_INDEX_BUILDS.with(Cell::get) - before, expected);
    }

    #[test]
    fn final_review_preflight_observes_cancellation_within_top_level_aliases() {
        // Arrange
        let source = format!(
            "import {}\n",
            (0..128)
                .map(|i| format!("unrelated_{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let operators = serde_json::from_value(serde_json::json!(["url_quote_swap"])).unwrap();
        let polls = Cell::new(0);
        let cancelled = || {
            polls.set(polls.get() + 1);
            polls.get() >= 64
        };

        // Act
        let result = Imports::build(parsed.syntax(), &operators, &cancelled);

        // Assert
        assert!(result.is_err());
    }

    #[rstest::rstest]
    #[case::cancel_immediately(true, true, 0)]
    #[case::stop_on_rejection(false, false, 1)]
    #[case::accept_all(false, true, 3)]
    fn codec_sink_observes_cancellation_and_rejection(
        #[case] cancelled: bool,
        #[case] accept: bool,
        #[case] expected: usize,
    ) {
        // Arrange
        let source = "s.decode(errors=policy)";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let module = parsed.syntax();
        let operators = MutationOperatorSelection::default();
        let imports = Imports::build(module, &operators, &|| false).unwrap();
        let call = module.body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let facts = AstFacts::from_module(module, parsed.tokens(), source);
        let builtins = Builtins::default();
        let delimiters = DelimiterDepth::default();
        let site = Site {
            facts: &facts,
            source,
            operators: &operators,
            delimiters: &delimiters,
            builtins: &builtins,
            cancelled: &|| cancelled,
        };
        let emitted = Cell::new(0);

        // Act
        imports.emit(call, &site, |_, _, _, _| {
            emitted.set(emitted.get() + 1);
            accept
        });

        // Assert
        assert_eq!(emitted.get(), expected);
    }

    #[rstest::rstest]
    #[case::unicode_cache("unicode_normalization_form", "u.normalize({parts}'NFC', text)", false)]
    #[case::codec_cache(
        "encoding_errors_ignore",
        "data.decode('utf8', {parts}'strict')",
        false
    )]
    #[case::unicode_empty_parts_cancel(
        "unicode_normalization_form",
        "u.normalize({parts}'NFC', text)",
        true
    )]
    #[case::codec_empty_parts_cancel(
        "encoding_errors_ignore",
        "data.decode('utf8', {parts}'strict')",
        true
    )]
    fn multipart_recognition_preserves_cold_cache_and_polls_empty_parts(
        #[case] id: &str,
        #[case] expression: &str,
        #[case] cancel_during_parts: bool,
    ) {
        // Arrange
        let source = format!(
            "import unicodedata as u\n{}",
            expression.replace("{parts}", &"'' ".repeat(128))
        );
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let module = parsed.syntax();
        let operators = serde_json::from_value(serde_json::json!([id])).unwrap();
        let imports = Imports::build(module, &operators, &|| false).unwrap();
        let call = module.body[1]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let literal = call
            .arguments
            .args
            .iter()
            .filter_map(ruff_python_ast::Expr::as_string_literal_expr)
            .find(|literal| literal.value.is_implicit_concatenated())
            .unwrap();
        let facts = AstFacts::from_module(module, parsed.tokens(), &source);

        let polls = Cell::new(0);
        let site = Site {
            facts: &facts,
            source: &source,
            operators: &operators,
            delimiters: &DelimiterDepth::default(),
            builtins: &Builtins::default(),
            cancelled: &|| {
                polls.set(polls.get() + 1);
                cancel_during_parts && polls.get() >= 32
            },
        };
        let emitted = Cell::new(0);

        // Act
        imports.emit(call, &site, |_, _, _, _| {
            emitted.set(emitted.get() + 1);
            false
        });

        // Assert
        if cancel_during_parts {
            assert_eq!(emitted.get(), 0);
            assert!(polls.get() >= 32);
        } else {
            assert_eq!(emitted.get(), 1);
            // A clone retains the cache state. Changing a part before the first join
            // makes the next read observe the sentinel only if the cache stayed cold.
            let mut probe = literal.value.clone();
            probe.iter_mut().next().unwrap().value = "sentinel".into();
            assert!(probe.to_str().starts_with("sentinel"));
        }
    }
    proptest::proptest! {
        #[test]
        fn codec_proof_cancels_during_alias_and_identity_scans(checkpoint in 0..90usize) {
            // Arrange
            let source = "import codecs as c\nfrom codecs import decode\ndef read(x): return c.decode(x)";
            let parsed = ruff_python_parser::parse_module(source).unwrap();
            let operators = serde_json::from_value(serde_json::json!(["encoding_errors_ignore"])).unwrap();
            let total = Cell::new(0);
            Imports::build(parsed.syntax(), &operators, &|| {total.set(total.get()+1); false}).unwrap();
            let count = Cell::new(0);

            // Act
            let actual = Imports::build(parsed.syntax(), &operators, &|| {let n=count.get(); count.set(n+1); n>=checkpoint});

            // Assert
            proptest::prop_assert_eq!(actual.is_err(), checkpoint < total.get());
        }
    }
}
