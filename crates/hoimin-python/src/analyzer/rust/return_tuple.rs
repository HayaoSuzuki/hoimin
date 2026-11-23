//! Exchange adjacent return tuple values after evaluating them in source order.
use super::{MutationOperator, source_delimiters::DelimiterDepth, source_text};
use ruff_python_ast::token::{TokenKind, Tokens, parenthesized_range};
use ruff_python_ast::{Expr, ExprTuple, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

/// The collector proves return scope and source selection before requesting edits.
pub(super) fn emit(
    tuple: &ExprTuple,
    source: &str,
    tokens: &Tokens,
    delimiters: &DelimiterDepth,
    max_candidates: usize,
    cancelled: &impl Fn() -> bool,
    mut emit: impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let mut emitted = 0;
    for pair in tuple.elts.windows(2) {
        if cancelled() || emitted > max_candidates {
            return;
        }
        let [first, second] = pair else {
            unreachable!("two-element window");
        };
        if first.is_starred_expr() || second.is_starred_expr() {
            continue;
        }
        let mode = if tuple.elts.len() == 2 && inert(first) && inert(second) {
            Swap::Source
        } else {
            Swap::Evaluated
        };
        if same_value(first, second, source) {
            continue;
        }
        let Some(pieces) = Pieces::new(tuple, first, second, source, tokens, mode) else {
            continue;
        };
        if !pieces.fits(delimiters, mode) {
            continue;
        }
        emitted += 1;
        if !emit(tuple.range(), MutationOperator::ReturnTupleSwap, &|| {
            pieces.render(mode)
        }) {
            return;
        }
    }
}

#[derive(Clone, Copy)]
enum Swap {
    Source,
    Evaluated,
}

struct Pieces<'a> {
    before: &'a str,
    first: &'a str,
    comma: &'a str,
    second: &'a str,
    after: &'a str,
    pair: TextRange,
    tuple: TextRange,
    must_group: bool,
    must_trail_comma: bool,
}

impl<'a> Pieces<'a> {
    fn new(
        tuple: &ExprTuple,
        first: &Expr,
        second: &Expr,
        source: &'a str,
        tokens: &Tokens,
        mode: Swap,
    ) -> Option<Self> {
        let operand_range = |operand: &Expr| match mode {
            Swap::Source => operand.range(),
            Swap::Evaluated => parenthesized_range(operand.into(), tuple.into(), tokens)
                .unwrap_or_else(|| operand.range()),
        };
        let first = operand_range(first);
        let second = operand_range(second);
        let after = TextRange::new(second.end(), tuple.end());
        let must_trail_comma = tuple.elts.len() == 2
            && !tokens
                .in_range(after)
                .iter()
                .any(|token| token.kind() == TokenKind::Comma);
        let must_group = matches!(mode, Swap::Source)
            && !tuple.parenthesized
            && usize::from(tuple.start())
                .checked_sub(1)
                .is_some_and(|start| source.as_bytes()[start].is_ascii_alphabetic());
        Some(Self {
            before: source_text(source, TextRange::new(tuple.start(), first.start()))?,
            first: source_text(source, first)?,
            comma: source_text(source, TextRange::new(first.end(), second.start()))?,
            second: source_text(source, second)?,
            after: source_text(source, after)?,
            pair: TextRange::new(first.start(), second.end()),
            tuple: tuple.range(),
            must_group,
            must_trail_comma,
        })
    }

    fn fits(&self, delimiters: &DelimiterDepth, mode: Swap) -> bool {
        match mode {
            Swap::Source => !self.must_group || delimiters.can_enclose(self.tuple, 1),
            Swap::Evaluated => {
                delimiters.can_enclose(self.pair, 2) && delimiters.room_before(self.pair.start(), 2)
            }
        }
    }

    fn render(&self, mode: Swap) -> String {
        let Self {
            before,
            first,
            comma,
            second,
            after,
            ..
        } = self;
        match mode {
            Swap::Source if self.must_group => format!("({before}{second}{comma}{first}{after})"),
            Swap::Source => format!("{before}{second}{comma}{first}{after}"),
            Swap::Evaluated => {
                let tail = if self.must_trail_comma { "," } else { "" };
                // Keep both original operands in call-argument scope. The
                // lambda receives their completed values and reverses them.
                format!("{before}*(lambda a, b: (b, a))(({first}){comma}({second})){tail}{after}")
            }
        }
    }
}

fn same_value(first: &Expr, second: &Expr, source: &str) -> bool {
    if let (Expr::Name(a), Expr::Name(b)) = (first, second) {
        return a.id == b.id;
    }
    inert(first)
        && inert(second)
        && source_text(source, first.range()) == source_text(source, second.range())
}

fn inert(expression: &Expr) -> bool {
    match expression {
        Expr::NumberLiteral(_) | Expr::BooleanLiteral(_) | Expr::NoneLiteral(_) => true,
        Expr::StringLiteral(s) => !s.value.is_implicit_concatenated(),
        Expr::UnaryOp(u) => {
            matches!(u.op, UnaryOp::UAdd | UnaryOp::USub)
                && matches!(u.operand.as_ref(), Expr::NumberLiteral(_))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest::rstest]
    #[case::identical_name("return (same, same)", &[])]
    #[case::identical_literal("return (1, 1)", &[])]
    #[case::inline_keyword("return'x', name", &["*(lambda a, b: (b, a))(('x'), (name)),"])]
    #[case::signed_literal("return (-1, 2,)", &["(2, -1,)"])]
    #[case::evaluated_pair("return first(), second()", &["*(lambda a, b: (b, a))((first()), (second())),"])]
    #[case::existing_star("return (*before, first(), second())", &["(*before, *(lambda a, b: (b, a))((first()), (second())))"])]
    fn return_tuple_edit_keeps_supported_syntax(
        #[case] statement: &str,
        #[case] expected: &[&str],
    ) {
        let source = format!("def f(): {statement}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let tokens = parsed.tokens();
        let delimiters = DelimiterDepth::build(tokens, &|| false).unwrap();
        let function = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let ret = function.body[0].as_return_stmt().unwrap();
        let tuple = ret.value.as_ref().unwrap().as_tuple_expr().unwrap();
        let mut edits = Vec::new();

        emit(
            tuple,
            &source,
            tokens,
            &delimiters,
            10,
            &|| false,
            |_, _, render| {
                edits.push(render());
                true
            },
        );

        assert_eq!(edits, expected);
        for edit in edits {
            assert!(ruff_python_parser::parse_module(&format!("def f(): return {edit}\n")).is_ok());
        }
    }
}
