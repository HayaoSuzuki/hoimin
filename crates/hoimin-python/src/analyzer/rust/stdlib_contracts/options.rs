use super::super::processing_calls::implicit_generator_interior;
use super::{AnalysisCancelled, CallableImports, Site, bind, urls};
use hoimin_core::{MutationOperator as Op, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprCall, ModModule, token::TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

#[derive(Default)]
pub(super) struct Imports {
    itertools: CallableImports,
    url: CallableImports,
    base64: CallableImports,
    json: CallableImports,
}

impl Imports {
    pub(super) fn build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let family = |enabled, name: &'static [&'static str; 1]| {
            if enabled {
                CallableImports::build_family(module, cancelled, name)
            } else {
                Ok(CallableImports::default())
            }
        };
        Ok(Self {
            itertools: family(operators.contains(Op::BatchedStrictToggle), &["itertools"])?,
            url: family(
                operators.contains(Op::UrlQuoteSwap) || operators.contains(Op::UrlQueryPolicy),
                &["urllib.parse"],
            )?,
            base64: family(operators.contains(Op::Base64ValidateToggle), &["base64"])?,
            json: family(operators.contains(Op::JsonAllowNanToggle), &["json"])?,
        })
    }

    pub(super) fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        emit: &mut impl FnMut(TextRange, TextSize, Op, &dyn Fn() -> String) -> bool,
    ) -> bool {
        for (op, imports) in [
            (Op::BatchedStrictToggle, &self.itertools),
            (Op::Base64ValidateToggle, &self.base64),
            (Op::JsonAllowNanToggle, &self.json),
        ] {
            if site.supports(op)
                && let Some(id) = imports.resolve(&call.func)
                && let Some(api) = Api::recognize(id.module, id.member)
            {
                return api.emit(call, site, emit);
            }
        }
        if site.supports(Op::MapStrictToggle) && site.builtins.resolves(&call.func, "map", site) {
            return Api::Map.emit(call, site, emit);
        }
        if (site.supports(Op::UrlQuoteSwap) || site.supports(Op::UrlQueryPolicy))
            && let Some(id) = self.url.resolve(&call.func)
        {
            return urls::emit(id, &self.url, call, site, emit);
        }
        true
    }
}

enum Api {
    Map,
    Batched,
    Base64,
    Dump,
    Dumps,
}
impl Api {
    fn recognize(module: &str, member: &str) -> Option<Self> {
        Some(match (module, member) {
            ("itertools", "batched") => Self::Batched,
            ("base64", "b64decode") => Self::Base64,
            ("json", "dump") => Self::Dump,
            ("json", "dumps") => Self::Dumps,
            _ => return None,
        })
    }
    fn policy<'a>(
        &self,
        call: &'a ExprCall,
        site: &Site<'_>,
    ) -> Option<(Op, &'static str, bool, Option<&'a Expr>)> {
        Some(match self {
            Self::Map => {
                if call.arguments.args.len() < 3
                    || call
                        .arguments
                        .args
                        .iter()
                        .any(|arg| (site.cancelled)() || arg.is_starred_expr())
                {
                    return None;
                }
                let policy = match call.arguments.keywords.as_ref() {
                    [] => None,
                    [kw] if kw.arg.as_ref().is_some_and(|name| name == "strict") => Some(&kw.value),
                    _ => return None,
                };
                (Op::MapStrictToggle, "strict", false, policy)
            }
            Self::Batched => (
                Op::BatchedStrictToggle,
                "strict",
                false,
                bind(call, ["iterable", "n", "strict"], 0, 2, 2)?[2],
            ),
            Self::Base64 => {
                let slots = bind(
                    call,
                    [
                        "s",
                        "altchars",
                        "validate",
                        "padded",
                        "ignorechars",
                        "canonical",
                    ],
                    0,
                    3,
                    1,
                )?;
                (
                    Op::Base64ValidateToggle,
                    "validate",
                    slots[4].is_some(),
                    slots[2],
                )
            }
            Self::Dump | Self::Dumps => {
                let (policy, cls) = match self {
                    Self::Dump => {
                        let slots = bind(
                            call,
                            [
                                "obj",
                                "fp",
                                "allow_nan",
                                "cls",
                                "skipkeys",
                                "ensure_ascii",
                                "check_circular",
                                "indent",
                                "separators",
                                "default",
                                "sort_keys",
                            ],
                            0,
                            2,
                            2,
                        )?;
                        (slots[2], slots[3])
                    }
                    _ => {
                        let slots = bind(
                            call,
                            [
                                "obj",
                                "allow_nan",
                                "cls",
                                "skipkeys",
                                "ensure_ascii",
                                "check_circular",
                                "indent",
                                "separators",
                                "default",
                                "sort_keys",
                            ],
                            0,
                            1,
                            1,
                        )?;
                        (slots[1], slots[2])
                    }
                };
                if cls.is_some_and(|value| !value.is_none_literal_expr()) {
                    return None;
                }
                (Op::JsonAllowNanToggle, "allow_nan", true, policy)
            }
        })
    }
    fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        emit: &mut impl FnMut(TextRange, TextSize, Op, &dyn Fn() -> String) -> bool,
    ) -> bool {
        if let Some((op, name, default, policy)) = self.policy(call, site) {
            return boolean_policy(call, site, op, name, default, policy, emit);
        }
        true
    }
}

/// The caller resolves the callable and binds its signature, passing the explicit
/// policy expression or the native default for an omitted slot. Select an
/// insertion on the call start and an explicit option on its value. The sink
/// receives the physical edit range independently and renders after admission.
///
/// Delay inversion until native code tests the option. The holder restores the
/// operand after each successful test. During a failing test only the original
/// truth conversion's frames can retain the operand via the active exception.
pub(in super::super) fn boolean_policy(
    call: &ExprCall,
    site: &Site<'_>,
    op: Op,
    name: &str,
    default: bool,
    policy: Option<&Expr>,
    emit: &mut impl FnMut(TextRange, TextSize, Op, &dyn Fn() -> String) -> bool,
) -> bool {
    if let Some(value) = policy {
        let range = value.range();
        if !value.is_boolean_literal_expr()
            && site.delimiters.can_enclose(range, 2)
            && site.delimiters.room_before(range.start(), 5)
            && let Some(flag) = site.text(range)
        {
            return emit(range, range.start(), op, &|| {
                format!(
                    "().__class__.__class__('_HoiminStrict', (), {{\
                 '__slots__': ('flag',), \
                 '__init__': lambda self, value: self.__setattr__('flag', [value]), \
                 '__bool__': lambda self: self.flag.extend((self.flag[-1], not self.flag.pop())) \
                 or self.flag.pop()}})(({flag}))"
                )
            });
        }
        return true;
    }
    let toggled = if default { "False" } else { "True" };
    if let Some(interior) = implicit_generator_interior(call) {
        if site.delimiters.can_enclose(interior, 1)
            && let Some(generator) = site.text(interior)
        {
            return emit(interior, call.start(), op, &|| {
                format!("({generator}), {name}={toggled}")
            });
        }
        return true;
    }
    let range = call.arguments.range();
    let prefix = TextRange::new(range.start(), range.end() - TextSize::new(1));
    let last = site
        .tokens()
        .in_range(prefix)
        .iter()
        .rev()
        .find(|token| (site.cancelled)() || !token.kind().is_trivia());
    if !(site.cancelled)()
        && let (Some(last), Some(text)) = (last, site.text(prefix))
    {
        let comma = if matches!(last.kind(), TokenKind::Comma | TokenKind::Lpar) {
            ""
        } else {
            ", "
        };
        return emit(range, call.start(), op, &|| {
            format!("{text}{comma}{name}={toggled})")
        });
    }
    true
}
