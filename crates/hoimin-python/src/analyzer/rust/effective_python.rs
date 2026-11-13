//! Local mutations of missing values, state ownership, scope and delegation.
use super::{
    AnalysisCancelled, callable_imports::CallableImports, formatted_conversions, function_entry,
    is_simple_receiver,
};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{CmpOp, Expr, ExprCall, ExprCompare, ModModule, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

/// A single identity comparison with exactly one None operand.
fn none_operand(compare: &ExprCompare) -> Option<(&Expr, &Expr, bool)> {
    if compare.ops.len() != 1 || compare.operands.len() != 2 {
        return None;
    }
    let negated = match compare.ops[0] {
        CmpOp::Is => false,
        CmpOp::IsNot => true,
        _ => return None,
    };
    let left = &compare.operands[0];
    let right = &compare.operands[1];
    match (
        matches!(left, Expr::NoneLiteral(_)),
        matches!(right, Expr::NoneLiteral(_)),
    ) {
        (true, false) => Some((right, left, negated)),
        (false, true) => Some((left, right, negated)),
        _ => None,
    }
}

pub(super) fn expression<F: Fn() -> bool>(
    expression: &Expr,
    source: &str,
    tokens: &Tokens,
    operators: &MutationOperatorSelection,
    cancelled: &F,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    if cancelled() {
        return;
    }
    match expression {
        Expr::Compare(compare) if operators.contains(MutationOperator::NoneTruthiness) => {
            if let Some(replacement) = none_replacement(compare, source, tokens, cancelled) {
                emit(
                    compare.range(),
                    replacement,
                    MutationOperator::NoneTruthiness,
                );
            }
        }
        Expr::YieldFrom(value) if operators.contains(MutationOperator::YieldFromToYield) => {
            for token in tokens.in_range(TextRange::new(value.start(), value.value.start())) {
                if cancelled() {
                    return;
                }
                if token.kind() == TokenKind::From {
                    emit(
                        token.range(),
                        String::new(),
                        MutationOperator::YieldFromToYield,
                    );
                    return;
                }
            }
        }
        Expr::FString(value) if operators.contains(MutationOperator::FStringConversionSwap) => {
            formatted_conversions::emit(value, tokens, cancelled, |range, replacement| {
                emit(range, replacement, MutationOperator::FStringConversionSwap)
            });
        }
        Expr::Call(call) => {
            let Expr::Attribute(attribute) = call.func.as_ref() else {
                return;
            };
            if operators.contains(MutationOperator::DequePopleftToPop)
                && attribute.attr.as_str() == "popleft"
                && is_simple_receiver(&attribute.value)
                && call.arguments.args.is_empty()
                && call.arguments.keywords.is_empty()
            {
                emit(
                    attribute.attr.range(),
                    "pop".to_owned(),
                    MutationOperator::DequePopleftToPop,
                );
            }
            if operators.contains(MutationOperator::MappingSetdefaultToGet)
                && attribute.attr.as_str() == "setdefault"
                && is_simple_receiver(&attribute.value)
                && (1..=2).contains(&call.arguments.args.len())
                && call.arguments.keywords.is_empty()
                && !call.arguments.args.iter().any(Expr::is_starred_expr)
                && !cancelled()
            {
                emit(
                    attribute.attr.range(),
                    "get".to_owned(),
                    MutationOperator::MappingSetdefaultToGet,
                );
            }
        }
        _ => {}
    }
}

fn none_replacement<F: Fn() -> bool>(
    compare: &ExprCompare,
    source: &str,
    tokens: &Tokens,
    cancelled: &F,
) -> Option<String> {
    let (_, none, negated) = none_operand(compare)?;
    // Keep trivia and the surviving operand's grouping. None's own grouping
    // must disappear too: retaining it would introduce an empty tuple/call.
    let mut edits = vec![none.range()];
    let operator_range = TextRange::new(compare.operands[0].end(), compare.operands[1].start());
    let mut operator_start = None;
    let mut operator_end = None;
    for token in tokens.in_range(operator_range) {
        if cancelled() {
            return None;
        }
        if token.kind() == TokenKind::Is {
            operator_start = Some(token.start());
        }
        if matches!(token.kind(), TokenKind::Is | TokenKind::Not) {
            operator_end = Some(token.end());
            edits.push(token.range());
        }
    }
    let none_range = if matches!(&compare.operands[0], Expr::NoneLiteral(_)) {
        TextRange::new(compare.start(), operator_start?)
    } else {
        TextRange::new(operator_end?, compare.end())
    };
    for token in tokens.in_range(none_range) {
        if cancelled() {
            return None;
        }
        if matches!(token.kind(), TokenKind::Lpar | TokenKind::Rpar) {
            edits.push(token.range());
        }
    }
    if cancelled() {
        return None;
    }
    let start = usize::from(compare.start());
    let mut original = source[start..usize::from(compare.end())].to_owned();
    edits.sort_unstable_by_key(|range| std::cmp::Reverse(range.start()));
    for edit in edits {
        if cancelled() {
            return None;
        }
        original.replace_range(
            usize::from(edit.start()) - start..usize::from(edit.end()) - start,
            "",
        );
    }
    let prefix = if negated { "not not" } else { "not" };
    Some(format!("({prefix} ({original}))"))
}

pub(super) fn statement<F: Fn() -> bool>(
    statement: &Stmt,
    operators: &MutationOperatorSelection,
    cancelled: &F,
    mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
) {
    if cancelled() {
        return;
    }
    match statement {
        Stmt::Continue(value) if operators.contains(MutationOperator::ContinueRemove) => {
            emit(
                value.range(),
                "pass".to_owned(),
                MutationOperator::ContinueRemove,
            );
        }
        Stmt::Raise(value) if operators.contains(MutationOperator::ExceptionCauseRemove) => {
            // Start with an explicit named cause: no constructor/await/compound
            // evaluation is removed, and `from None` stays excluded.
            if let Some(Expr::Name(cause)) = value.cause.as_deref() {
                emit(
                    cause.range(),
                    "None".to_owned(),
                    MutationOperator::ExceptionCauseRemove,
                );
            }
        }
        Stmt::Nonlocal(value) if operators.contains(MutationOperator::NonlocalRemove) => {
            emit(
                value.range(),
                "pass".to_owned(),
                MutationOperator::NonlocalRemove,
            );
        }
        Stmt::FunctionDef(function)
            if operators.contains(MutationOperator::MutableDefaultShare) =>
        {
            if let Some((range, replacement)) = shared_default(function, cancelled) {
                emit(
                    range,
                    replacement.to_owned(),
                    MutationOperator::MutableDefaultShare,
                );
            }
        }
        _ => {}
    }
}

fn shared_default<F: Fn() -> bool>(
    function: &StmtFunctionDef,
    cancelled: &F,
) -> Option<(TextRange, &'static str)> {
    if !function.decorator_list.is_empty() || function.type_params.is_some() {
        return None;
    }
    let Stmt::If(initializer) = function_entry::first_statement(function)? else {
        return None;
    };
    if !initializer.elif_else_clauses.is_empty() || initializer.body.len() != 1 {
        return None;
    }
    let Expr::Compare(compare) = initializer.test.as_ref() else {
        return None;
    };
    let (operand, _, negated) = none_operand(compare)?;
    if negated {
        return None;
    }
    let Expr::Name(name) = operand else {
        return None;
    };
    let Stmt::Assign(assignment) = &initializer.body[0] else {
        return None;
    };
    let [Expr::Name(target)] = assignment.targets.as_slice() else {
        return None;
    };
    if name.id != target.id {
        return None;
    }
    let replacement = match assignment.value.as_ref() {
        Expr::List(value) if value.elts.is_empty() => "[]",
        Expr::Dict(value) if value.items.is_empty() => "{}",
        _ => return None,
    };
    for parameter in function.parameters.iter_non_variadic_params() {
        if cancelled() {
            return None;
        }
        if parameter.parameter.name.as_str() == name.id.as_str() {
            let default = parameter.default.as_deref()?;
            return matches!(default, Expr::NoneLiteral(_))
                .then_some((default.range(), replacement));
        }
    }
    None
}

/// Reuse the shared conservative import proof for standard-library callables.
#[derive(Default)]
pub(super) struct Imports {
    imports: CallableImports,
}
impl Imports {
    pub(super) fn build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        if !operators.contains(MutationOperator::BisectBoundarySwap)
            && !operators.contains(MutationOperator::DeepcopyToCopy)
            && !operators.contains(MutationOperator::GatherReturnExceptions)
            && !operators.contains(MutationOperator::ItertoolsTakeDropSwap)
        {
            return Ok(Self::default());
        }
        Ok(Self {
            imports: CallableImports::build(
                module,
                cancelled,
                &["bisect", "copy", "asyncio", "itertools"],
            )?,
        })
    }
    pub(super) fn replacement(
        &self,
        expression: &Expr,
        operators: &MutationOperatorSelection,
        tokens: &Tokens,
        cancelled: &impl Fn() -> bool,
    ) -> Option<(TextRange, String, MutationOperator)> {
        let Expr::Call(call) = expression else {
            return None;
        };
        let resolved = self.imports.resolve(&call.func)?;
        if resolved.module == "asyncio"
            && resolved.member == "gather"
            && operators.contains(MutationOperator::GatherReturnExceptions)
        {
            return gather_replacement(call, tokens, cancelled)
                .map(|(range, text)| (range, text, MutationOperator::GatherReturnExceptions));
        }
        let (destination, operator) = match (resolved.module, resolved.member) {
            ("itertools", "takewhile") => ("dropwhile", MutationOperator::ItertoolsTakeDropSwap),
            ("itertools", "dropwhile") => ("takewhile", MutationOperator::ItertoolsTakeDropSwap),
            ("bisect", "bisect_left") => ("bisect_right", MutationOperator::BisectBoundarySwap),
            ("bisect", "bisect_right") => ("bisect_left", MutationOperator::BisectBoundarySwap),
            ("copy", "deepcopy")
                if call.arguments.args.len() == 1
                    && call.arguments.keywords.is_empty()
                    && !call.arguments.args[0].is_starred_expr() =>
            {
                ("copy", MutationOperator::DeepcopyToCopy)
            }
            _ => return None,
        };
        if !operators.contains(operator) {
            return None;
        }
        if let Some(range) = resolved.member_range {
            Some((range, destination.to_owned(), operator))
        } else if self.imports.import_builtin_available() {
            Some((
                call.func.range(),
                format!("__import__('{}').{destination}", resolved.module),
                operator,
            ))
        } else {
            None
        }
    }
}

fn gather_replacement(
    call: &ExprCall,
    tokens: &Tokens,
    cancelled: &impl Fn() -> bool,
) -> Option<(TextRange, String)> {
    if call.arguments.args.is_empty()
        || !call.arguments.keywords.is_empty()
        || call
            .arguments
            .args
            .iter()
            .any(|arg| matches!(arg, Expr::Starred(_) | Expr::Generator(_)))
    {
        return None;
    }
    let mut significant = tokens.in_range(call.arguments.range()).iter().rev();
    let closing = significant.next()?;
    if closing.kind() != TokenKind::Rpar {
        return None;
    }
    for token in significant {
        if cancelled() {
            return None;
        }
        if !token.kind().is_trivia() {
            let separator = if token.kind() == TokenKind::Comma {
                ""
            } else {
                ", "
            };
            return Some((
                closing.range(),
                format!("{separator}return_exceptions=True)"),
            ));
        }
    }
    None
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_shared(
        function: &StmtFunctionDef,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, &'static str)> {
        shared_default(function, &cancelled)
    }

    #[rstest::rstest]
    #[case("def f(x=None):\n if x is None:\n  x = []", Some("[]"))]
    #[case("def f(x=None):\n if None is x:\n  x = {}", Some("{}"))]
    #[case("@decorate\ndef f(x=None):\n if x is None:\n  x = []", None)]
    #[case("def f[T](x=None):\n if x is None:\n  x = []", None)]
    #[case("def f(x=None):\n pass", None)]
    #[case("def f(x=None):\n if x is None:\n  x = []\n else:\n  pass", None)]
    #[case("def f(x=None):\n if x is None:\n  x = []\n  pass", None)]
    #[case("def f(x=None):\n if x:\n  x = []", None)]
    #[case("def f(x=None):\n if x is not None:\n  x = []", None)]
    #[case("def f(x=None):\n if x.attr is None:\n  x = []", None)]
    #[case("def f(x=None):\n if x is None:\n  pass", None)]
    #[case("def f(x=None):\n if x is None:\n  x.attr = []", None)]
    #[case("def f(x=None):\n if x is None:\n  y = []", None)]
    #[case("def f(x=None):\n if x is None:\n  x = [1]", None)]
    #[case("def f(x=None):\n if x is None:\n  x = {'a': 1}", None)]
    #[case("def f(x=None):\n if x is None:\n  x = set()", None)]
    #[case("def f(x):\n if x is None:\n  x = []", None)]
    #[case("def f(x=1):\n if x is None:\n  x = []", None)]
    #[case("def f(y=None):\n if x is None:\n  x = []", None)]
    #[case("def f(x=None):\n 'documentation'", None)]
    #[case("def f(x=None):\n if x == None:\n  x = []", None)]
    fn shared_default_requires_a_simple_parameter_none_initializer(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let function = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        assert_eq!(
            run_shared(function, &|| false).map(|(_, replacement)| replacement),
            expected
        );
    }

    #[rstest::rstest]
    fn shared_default_stops_during_parameter_lookup(#[values(0, 1)] checkpoint: usize) {
        let parsed =
            ruff_python_parser::parse_module("def f(y=None, x=None):\n if x is None:\n  x = []")
                .unwrap();
        let function = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let polls = Cell::new(0);
        assert_eq!(
            run_shared(function, &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            }),
            None
        );
        assert_eq!(polls.get(), checkpoint + 1);
    }

    fn run_none(
        compare: &ExprCompare,
        source: &str,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<String> {
        none_replacement(compare, source, tokens, &cancelled)
    }

    #[rstest::rstest]
    #[case("None is value", Some("(not (  value))"))]
    #[case("value is not None", Some("(not not (value   ))"))]
    #[case("None is None", None)]
    #[case("first is second", None)]
    #[case("value == None", None)]
    #[case("first is None is second", None)]
    fn none_truthiness_requires_exactly_one_identity_none_operand(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let compare = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_compare_expr()
            .unwrap();
        assert_eq!(
            run_none(compare, source, parsed.tokens(), &|| false).as_deref(),
            expected
        );
    }

    #[rstest::rstest]
    fn none_truthiness_stops_at_each_edit_phase(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16)] checkpoint: usize,
    ) {
        let source = "value is not ((None))";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let compare = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_compare_expr()
            .unwrap();
        let polls = Cell::new(0);
        assert_eq!(
            run_none(compare, source, parsed.tokens(), &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            }),
            None
        );
        assert_eq!(polls.get(), checkpoint + 1);
    }
    fn run_gather(
        call: &ExprCall,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, String)> {
        gather_replacement(call, tokens, &cancelled)
    }

    #[rstest::rstest]
    #[case(vec![], None)]
    #[case(vec![TokenKind::Lpar, TokenKind::Identifier], None)]
    #[case(vec![TokenKind::Rpar], None)]
    #[case(vec![TokenKind::Lpar, TokenKind::Identifier, TokenKind::Rpar], Some(", return_exceptions=True)"))]
    fn gather_edit_requires_a_complete_argument_token_boundary(
        #[case]
        #[by_ref]
        kinds: &[TokenKind],
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_module("gather(task)").unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let tokens = Tokens::new(
            parsed
                .tokens()
                .iter()
                .filter(|token| kinds.contains(&token.kind()))
                .copied()
                .collect(),
        );
        assert_eq!(
            run_gather(call, &tokens, &|| false)
                .as_ref()
                .map(|(_, replacement)| replacement.as_str()),
            expected
        );
    }

    #[test]
    fn gather_edit_stops_before_choosing_an_insertion_point() {
        let parsed = ruff_python_parser::parse_module("gather(task)").unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        assert_eq!(run_gather(call, parsed.tokens(), &|| true), None);
    }

    #[rstest::rstest]
    #[case("None is value")]
    #[case("value is None")]
    fn none_edit_rejects_missing_identity_operator_tokens(#[case] source: &str) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let compare = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_compare_expr()
            .unwrap();
        assert_eq!(
            run_none(compare, source, &Tokens::new(vec![]), &|| false),
            None
        );
    }

    #[rstest::rstest]
    fn yield_from_removal_stops_during_keyword_scan(#[values(0, 1, 2)] checkpoint: usize) {
        let source = "yield from values";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let expression = &parsed.syntax().body[0].as_expr_stmt().unwrap().value;
        let polls = Cell::new(0);
        let cancelled: &dyn Fn() -> bool = &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        };
        let mut replacements = Vec::new();
        expression_emit(
            expression,
            source,
            parsed.tokens(),
            cancelled,
            &mut replacements,
        );
        assert_eq!(replacements, [] as [String; 0]);
        assert_eq!(polls.get(), checkpoint + 1);
    }

    #[test]
    fn yield_from_removal_changes_only_the_delegating_keyword() {
        let source = "yield from values";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let expression = &parsed.syntax().body[0].as_expr_stmt().unwrap().value;
        let mut replacements = Vec::new();
        expression_emit(
            expression,
            source,
            parsed.tokens(),
            &|| false,
            &mut replacements,
        );
        assert_eq!(replacements, [String::new()]);
    }

    fn expression_emit(
        expression_node: &Expr,
        source: &str,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
        replacements: &mut Vec<String>,
    ) {
        expression(
            expression_node,
            source,
            tokens,
            &MutationOperatorSelection::default(),
            &cancelled,
            |_, replacement, _| {
                replacements.push(replacement);
                true
            },
        );
    }
    #[rstest::rstest]
    #[case("gather()", None)]
    #[case("gather(task, return_exceptions=False)", None)]
    #[case("gather(*tasks)", None)]
    #[case("gather(task for task in tasks)", None)]
    #[case("gather(task,)", Some("return_exceptions=True)"))]
    #[case("gather(task, # keep\n)", Some("return_exceptions=True)"))]
    fn gather_edit_preserves_explicit_argument_boundaries(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let call = parsed.syntax().body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        assert_eq!(
            run_gather(call, parsed.tokens(), &|| false)
                .as_ref()
                .map(|(_, replacement)| replacement.as_str()),
            expected
        );
    }

    #[rstest::rstest]
    #[case("None is value", vec!["(not (  value))"])]
    #[case("first is second", vec![])]
    #[case("f'{value!s}'", vec!["r"])]
    #[case("receiver.popleft()", vec!["pop"])]
    #[case("receiver.setdefault(key)", vec!["get"])]
    #[case("receiver.setdefault(*keys)", vec![])]
    #[case("receiver.setdefault(key, default=0)", vec![])]
    #[case("factory().popleft()", vec![])]
    #[case("receiver.popleft(value)", vec![])]
    #[case("function()", vec![])]
    #[case("value", vec![])]
    fn local_expression_edits_require_supported_shapes(
        #[case] source: &str,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let expression = &parsed.syntax().body[0].as_expr_stmt().unwrap().value;
        let mut replacements = Vec::new();
        expression_emit(
            expression,
            source,
            parsed.tokens(),
            &|| false,
            &mut replacements,
        );
        assert_eq!(replacements, expected);
    }

    fn statement_emit(
        statement_node: &Stmt,
        cancelled: &dyn Fn() -> bool,
        replacements: &mut Vec<String>,
    ) {
        statement(
            statement_node,
            &MutationOperatorSelection::default(),
            &cancelled,
            |_, replacement, _| {
                replacements.push(replacement);
                true
            },
        );
    }

    #[rstest::rstest]
    #[case("continue", false, vec!["pass"])]
    #[case("continue", true, vec![])]
    #[case("raise error from cause", false, vec!["None"])]
    #[case("raise error from None", false, vec![])]
    #[case("nonlocal captured", false, vec!["pass"])]
    #[case("def f(x=None):\n if x is None:\n  x = []", false, vec!["[]"])]
    #[case("def f(x=None): pass", false, vec![])]
    #[case("pass", false, vec![])]
    fn local_statement_edits_preserve_unsupported_or_cancelled_statements(
        #[case] source: &str,
        #[case] cancelled: bool,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let mut replacements = Vec::new();
        statement_emit(&parsed.syntax().body[0], &|| cancelled, &mut replacements);
        assert_eq!(replacements, expected);
    }
    fn run_import_build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Imports, AnalysisCancelled> {
        Imports::build(module, operators, &cancelled)
    }

    #[rstest::rstest]
    #[case(vec!["deepcopy_to_copy"], false, false, Some("copy"))]
    #[case(vec![], false, false, None)]
    #[case(vec!["deepcopy_to_copy"], true, true, None)]
    fn callable_import_index_is_built_only_when_selected_and_not_cancelled(
        #[case]
        #[by_ref]
        selected: &[&str],
        #[case] cancelled: bool,
        #[case] expected_error: bool,
        #[case] expected_replacement: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_module("import copy\ncopy.deepcopy(value)").unwrap();
        let operators = serde_json::from_value(serde_json::json!(selected)).unwrap();
        let result = run_import_build(parsed.syntax(), &operators, &|| cancelled);
        assert_eq!(result.is_err(), expected_error);
        let expression = &parsed.syntax().body[1].as_expr_stmt().unwrap().value;
        let replacement = result.as_ref().ok().and_then(|imports| {
            imports.replacement(expression, &operators, parsed.tokens(), &|| false)
        });
        assert_eq!(
            replacement.as_ref().map(|(_, text, _)| text.as_str()),
            expected_replacement
        );
    }
    #[test]
    fn yield_from_removal_requires_the_delegating_keyword_token() {
        let source = "yield from values";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let expression = &parsed.syntax().body[0].as_expr_stmt().unwrap().value;
        let tokens = Tokens::new(
            parsed
                .tokens()
                .iter()
                .filter(|token| token.kind() != TokenKind::From)
                .copied()
                .collect(),
        );
        let mut replacements = Vec::new();
        expression_emit(expression, source, &tokens, &|| false, &mut replacements);
        assert_eq!(replacements, [] as [String; 0]);
    }
}
