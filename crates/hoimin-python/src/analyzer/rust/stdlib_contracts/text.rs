use super::{CallableImports, Site, bind};
use hoimin_core::MutationOperator::{
    self, StringAffixStrip, StringCasefoldLower, UnicodeDigitAscii, UnicodeDigitPredicateSwap,
    UnicodeNormalizationForm, UnicodeNormalizationRemove,
};
use ruff_python_ast::{Expr, ExprCall, token::TokenKind};
use ruff_text_size::{Ranged, TextRange};

#[derive(Clone, Copy, PartialEq)]
enum Predicate {
    Decimal,
    Digit,
    Numeric,
}
impl Predicate {
    fn name(self) -> &'static str {
        match self {
            Self::Decimal => "isdecimal",
            Self::Digit => "isdigit",
            Self::Numeric => "isnumeric",
        }
    }
}
#[derive(Clone, Copy)]
enum Affix {
    RemovePrefix,
    LeftStrip,
    RemoveSuffix,
    RightStrip,
}
impl Affix {
    fn swapped(self) -> &'static str {
        match self {
            Self::RemovePrefix => "lstrip",
            Self::LeftStrip => "removeprefix",
            Self::RemoveSuffix => "rstrip",
            Self::RightStrip => "removesuffix",
        }
    }
}
enum Method {
    Digit(Predicate),
    Casefold,
    Lower,
    Affix(Affix),
}
impl Method {
    fn recognize(name: &str) -> Option<Self> {
        Some(match name {
            "isdecimal" => Self::Digit(Predicate::Decimal),
            "isdigit" => Self::Digit(Predicate::Digit),
            "isnumeric" => Self::Digit(Predicate::Numeric),
            "casefold" => Self::Casefold,
            "lower" => Self::Lower,
            "removeprefix" => Self::Affix(Affix::RemovePrefix),
            "lstrip" => Self::Affix(Affix::LeftStrip),
            "removesuffix" => Self::Affix(Affix::RemoveSuffix),
            "rstrip" => Self::Affix(Affix::RightStrip),
            _ => return None,
        })
    }
}
enum Form {
    Nfc,
    Nfd,
    Nfkc,
    Nfkd,
}
impl Form {
    const SPELLINGS: [(&'static str, Self); 4] = [
        ("NFC", Self::Nfc),
        ("NFD", Self::Nfd),
        ("NFKC", Self::Nfkc),
        ("NFKD", Self::Nfkd),
    ];
    fn neighbors(self) -> [&'static str; 2] {
        match self {
            Self::Nfc => ["'NFD'", "'NFKC'"],
            Self::Nfd => ["'NFC'", "'NFKD'"],
            Self::Nfkc => ["'NFKD'", "'NFC'"],
            Self::Nfkd => ["'NFKC'", "'NFD'"],
        }
    }
}

pub(super) fn emit(
    imports: &CallableImports,
    call: &ExprCall,
    site: &Site<'_>,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) -> bool {
    if let Expr::Attribute(attr) = call.func.as_ref()
        && let Some(method) = Method::recognize(attr.attr.as_str())
    {
        let range = attr.attr.range();
        match method {
            Method::Digit(original) if call.arguments.is_empty() => {
                if site.supports(UnicodeDigitPredicateSwap) {
                    for other in [Predicate::Decimal, Predicate::Digit, Predicate::Numeric] {
                        if other != original
                            && !emit(range, UnicodeDigitPredicateSwap, &|| {
                                other.name().to_owned()
                            })
                        {
                            return false;
                        }
                    }
                }
                if site.supports(UnicodeDigitAscii) && site.delimiters.can_enclose(call.range(), 2)
                {
                    // The suffix contains only the attribute access, empty call and trivia.
                    // Its receiver becomes a local bound once to the original expression.
                    let before_name = TextRange::new(attr.value.end(), range.start());
                    let Some(dot) = site
                        .tokens()
                        .in_range(before_name)
                        .iter()
                        .rev()
                        .find(|token| (site.cancelled)() || token.kind() == TokenKind::Dot)
                    else {
                        return true;
                    };
                    if !(site.cancelled)()
                        && let Some(grouping) =
                            site.text(TextRange::new(call.start(), attr.start()))
                        && let Some(receiver) = site.text(TextRange::new(attr.start(), dot.start()))
                        && let Some(suffix) = site.text(TextRange::new(dot.start(), call.end()))
                    {
                        return emit(call.range(), UnicodeDigitAscii, &|| {
                            format!("(lambda r: {grouping}r{suffix} and r.isascii())(({receiver}))")
                        });
                    }
                }
            }
            Method::Casefold | Method::Lower
                if site.supports(StringCasefoldLower) && call.arguments.is_empty() =>
            {
                let other = if matches!(method, Method::Casefold) {
                    "lower"
                } else {
                    "casefold"
                };
                return emit(range, StringCasefoldLower, &|| other.to_owned());
            }
            Method::Affix(affix)
                if site.supports(StringAffixStrip) && bind(call, ["affix"], 1, 1, 1).is_some() =>
            {
                return emit(range, StringAffixStrip, &|| affix.swapped().to_owned());
            }
            _ => {}
        }
        return true;
    }
    emit_normalization(imports, call, site, emit)
}

fn emit_normalization(
    imports: &CallableImports,
    call: &ExprCall,
    site: &Site<'_>,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) -> bool {
    if !(site.supports(UnicodeNormalizationForm) || site.supports(UnicodeNormalizationRemove)) {
        return true;
    }
    let Some(identity) = imports.resolve(&call.func) else {
        return true;
    };
    let removes = match (identity.module, identity.member) {
        ("unicodedata", "normalize") => true,
        ("unicodedata", "is_normalized") => false,
        _ => return true,
    };
    let Some([Some(form), Some(_)]) = bind(call, ["form", "unistr"], 2, 2, 2) else {
        return true;
    };
    let literal = form.as_string_literal_expr();
    let known_form = if site.supports(UnicodeNormalizationForm) {
        let Ok(known) = super::decoded_choice(literal, Form::SPELLINGS, site.cancelled) else {
            return false;
        };
        known
    } else {
        None
    };
    if let Some(known) = known_form {
        let multipart = literal.is_some_and(|literal| literal.value.is_implicit_concatenated());
        if !multipart || site.delimiters.can_enclose(form.range(), 1) {
            for replacement in known.neighbors() {
                if !emit(form.range(), UnicodeNormalizationForm, &|| {
                    if multipart {
                        // Concatenated literals can carry comments between their parts.
                        let original = site.text(form.range()).expect("parsed form");
                        format!("({original}, {replacement})[1]")
                    } else {
                        replacement.to_owned()
                    }
                }) {
                    return false;
                }
            }
        }
    }
    if removes
        && site.supports(UnicodeNormalizationRemove)
        && site.delimiters.can_enclose(call.func.range(), 1)
    {
        return emit(call.func.range(), UnicodeNormalizationRemove, &|| {
            let callee = site.text(call.func.range()).expect("parsed callee");
            format!("({callee}, lambda form, text: text)[1]")
        });
    }
    true
}
