use super::super::callable_imports::ResolvedCallable;
use super::{CallableImports, Site, bind, options::boolean_policy};
use hoimin_core::MutationOperator as Op;
use ruff_python_ast::ExprCall;
use ruff_text_size::{Ranged, TextRange, TextSize};

enum Api {
    Quote,
    QuotePlus,
    Unquote,
    UnquotePlus,
    Urlencode,
    ParseQs,
    ParseQsl,
}

pub(super) fn emit(
    id: &ResolvedCallable<'_>,
    imports: &CallableImports,
    call: &ExprCall,
    site: &Site<'_>,
    emit: &mut impl FnMut(TextRange, TextSize, Op, &dyn Fn() -> String) -> bool,
) -> bool {
    let api = match id.member {
        "quote" => Api::Quote,
        "quote_plus" => Api::QuotePlus,
        "unquote" => Api::Unquote,
        "unquote_plus" => Api::UnquotePlus,
        "urlencode" => Api::Urlencode,
        "parse_qs" => Api::ParseQs,
        "parse_qsl" => Api::ParseQsl,
        _ => return true,
    };
    let destination = match api {
        Api::Quote | Api::QuotePlus
            if site.supports(Op::UrlQuoteSwap)
                && bind(call, ["string", "safe", "encoding", "errors"], 0, 4, 1).is_some() =>
        {
            if matches!(api, Api::Quote) {
                "quote_plus"
            } else {
                "quote"
            }
        }
        Api::Unquote | Api::UnquotePlus
            if site.supports(Op::UrlQuoteSwap)
                && bind(call, ["string", "encoding", "errors"], 0, 3, 1).is_some() =>
        {
            if matches!(api, Api::Unquote) {
                "unquote_plus"
            } else {
                "unquote"
            }
        }
        Api::Urlencode if site.supports(Op::UrlQueryPolicy) => {
            if let Some(slots) = bind(
                call,
                ["query", "doseq", "safe", "encoding", "errors", "quote_via"],
                0,
                6,
                1,
            ) {
                return boolean_policy(
                    call,
                    site,
                    Op::UrlQueryPolicy,
                    "doseq",
                    false,
                    slots[1],
                    emit,
                );
            }
            return true;
        }
        Api::ParseQs | Api::ParseQsl if site.supports(Op::UrlQueryPolicy) => {
            let names = [
                "qs",
                "keep_blank_values",
                "strict_parsing",
                "encoding",
                "errors",
                "max_num_fields",
                "separator",
                "_stacklevel",
            ];
            if let Some(slots) = bind(call, names, 0, 7, 1) {
                if matches!(api, Api::ParseQs) && slots[7].is_some() {
                    return true;
                }
                return boolean_policy(
                    call,
                    site,
                    Op::UrlQueryPolicy,
                    "keep_blank_values",
                    false,
                    slots[1],
                    emit,
                );
            }
            return true;
        }
        _ => return true,
    };
    if let Some(member) = id.member_range {
        return emit(member, member.start(), Op::UrlQuoteSwap, &|| {
            destination.to_owned()
        });
    }
    if let Some(dest) =
        site.builtins
            .import_destination(imports, id.module, destination, &call.func, site)
    {
        return emit(
            call.func.range(),
            call.func.start(),
            Op::UrlQuoteSwap,
            &|| dest.to_string(),
        );
    }
    true
}
