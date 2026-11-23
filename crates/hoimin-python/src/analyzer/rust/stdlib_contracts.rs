//! Standard library call contracts. Domains establish identity and binding before
//! passing source ranges and lazy renderers to the collector.
use super::{
    AnalysisCancelled,
    callable_imports::CallableImports,
    processing_calls::{Site, bind},
};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{ExprCall, ModModule};
use ruff_text_size::TextRange;

#[path = "stdlib_contracts/encoding.rs"]
mod encoding;
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
    unicode: CallableImports,
    encoding: encoding::Imports,
}

impl Imports {
    pub(super) fn build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let unicode = if [
            MutationOperator::UnicodeNormalizationForm,
            MutationOperator::UnicodeNormalizationRemove,
        ]
        .into_iter()
        .any(|op| operators.contains(op))
        {
            CallableImports::build_family(module, cancelled, &["unicodedata"])?
        } else {
            CallableImports::default()
        };
        Ok(Self {
            unicode,
            encoding: encoding::Imports::build(module, operators, cancelled)?,
        })
    }
    pub(super) fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        mut emit: impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
    ) {
        if !(site.cancelled)()
            && text::emit(&self.unicode, call, site, &mut emit)
            && !(site.cancelled)()
        {
            self.encoding.emit(call, site, &mut emit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AstFacts, processing_calls::Builtins, source_delimiters::DelimiterDepth};
    use super::*;
    use std::cell::Cell;

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
        imports.emit(call, &site, |_, _, _| {
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
        imports.emit(call, &site, |_, _, _| {
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
