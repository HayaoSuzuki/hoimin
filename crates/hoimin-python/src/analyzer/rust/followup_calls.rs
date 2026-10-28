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
    let separator = super::source_text(
        source,
        TextRange::new(first_range.end(), second_range.start()),
    )?;
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

    #[test]
    fn codec_identity_scan_requires_a_codec_import() {
        for (source, expected) in [
            ("data.decode(errors='surrogateescape')\n", false),
            ("import unrelated\n", false),
            ("import codecs as c\n", true),
            ("from codecs import decode as dec\n", true),
            ("def f():\n    import codecs as c\n", true),
        ] {
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
}
