use super::{AnalysisCancelled, CallableImports, Site, bind};
use crate::visitor::{self, Visitor};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprCall, ModModule, Stmt, token::TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};
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
        if !POLICIES
            .iter()
            .any(|policy| operators.contains(policy.operator()))
        {
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
            Some(CallableImports::build_family(
                module,
                cancelled,
                &["codecs"],
            )?)
        } else {
            None
        };
        Ok(Self {
            codecs,
            codec_module_names: aliases.names,
        })
    }

    fn policy<'a>(&self, call: &'a ExprCall, site: &Site<'_>) -> Option<Option<&'a Expr>> {
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
                || !simple_receiver(&method.value, &site.cancelled)
                || matches!(method.value.as_ref(), Expr::Name(name) if self.codec_module_names.contains(name.id.as_str()))
            {
                return None;
            }
        }
        if trusted {
            Some(bind(call, ["obj", "encoding", "errors"], 0, 3, 1)?[2])
        } else {
            Some(bind(call, ["encoding", "errors"], 0, 2, 0)?[1])
        }
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    Ignore,
    Replace,
    Strict,
}
const POLICIES: [Policy; 3] = [Policy::Ignore, Policy::Replace, Policy::Strict];
impl Policy {
    fn operator(self) -> MutationOperator {
        match self {
            Self::Ignore => MutationOperator::EncodingErrorsIgnore,
            Self::Replace => MutationOperator::EncodingErrorsReplace,
            Self::Strict => MutationOperator::EncodingErrorsStrict,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Ignore => "ignore",
            Self::Replace => "replace",
            Self::Strict => "strict",
        }
    }
}

impl Imports {
    pub(super) fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
    ) {
        if !POLICIES.iter().any(|p| site.supports(p.operator())) {
            return;
        }
        let Some(policy) = self.policy(call, site) else {
            return;
        };
        let literal = policy.and_then(Expr::as_string_literal_expr);
        let Ok(original) =
            super::decoded_choice(literal, POLICIES.map(|p| (p.name(), p)), site.cancelled)
        else {
            return;
        };
        for destination in POLICIES {
            let op = destination.operator();
            let name = destination.name();
            if (site.cancelled)() {
                return;
            }
            if !site.supports(op) || original == Some(destination) {
                continue;
            }
            if let Some(expr) = policy {
                if literal.is_some_and(|lit| !lit.value.is_implicit_concatenated()) {
                    if !emit(expr.range(), op, &|| format!("'{name}'")) {
                        return;
                    }
                } else if site.delimiters.can_enclose(expr.range(), 2)
                    && let Some(text) = site.text(expr.range())
                    && !emit(expr.range(), op, &|| format!("(({text}), '{name}')[1]"))
                {
                    return;
                }
            } else if !matches!(destination, Policy::Strict) {
                let closing = call.end() - TextSize::new(1);
                let before_close = TextRange::new(call.arguments.start(), closing);
                let Some(last) = site
                    .tokens()
                    .in_range(before_close)
                    .iter()
                    .rev()
                    .find(|token| (site.cancelled)() || !token.kind().is_trivia())
                else {
                    return;
                };
                if (site.cancelled)() {
                    return;
                }
                let Some(prefix) = site.text(TextRange::new(call.start(), closing)) else {
                    return;
                };
                let comma = if matches!(last.kind(), TokenKind::Comma | TokenKind::Lpar) {
                    ""
                } else {
                    ", "
                };
                if let Some(range) =
                    super::super::processing_calls::implicit_generator_interior(call)
                {
                    if site.delimiters.can_enclose(range, 1) {
                        let callee = site
                            .text(TextRange::new(call.start(), range.start()))
                            .expect("parsed call start");
                        let generator = site.text(range).expect("parsed generator");
                        if !emit(call.range(), op, &|| {
                            format!("{callee}({generator}), errors='{name}')")
                        }) {
                            return;
                        }
                    }
                } else if !emit(call.range(), op, &|| {
                    format!("{prefix}{comma}errors='{name}')")
                }) {
                    return;
                }
            }
        }
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
