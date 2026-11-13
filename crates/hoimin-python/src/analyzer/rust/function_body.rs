//! Own-scope eligibility and source spans for whole-function body mutations.
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Number, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn erased_range<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    cancelled: &F,
) -> Option<TextRange> {
    let name = definition.name.as_str();
    if definition.is_async || (name.starts_with("__") && name.ends_with("__")) || cancelled() {
        return None;
    }
    let body = definition.body.as_slice();
    let body = if body.first().is_some_and(is_docstring) {
        &body[1..]
    } else {
        body
    };
    let first = body.first()?;
    let mut check = OwnScope {
        safe: true,
        reject_value_return: true,
        has_value_return: false,
        cancelled,
    };
    let mut meaningful = false;
    for statement in body {
        if cancelled() {
            return None;
        }
        meaningful |= !is_noop(statement);
        check.visit_stmt(statement);
        if !check.safe {
            return None;
        }
    }
    meaningful.then(|| TextRange::new(first.start(), body.last().expect("nonempty body").end()))
}

pub(super) fn plain_synchronous<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    cancelled: &F,
) -> bool {
    if definition.is_async || cancelled() {
        return false;
    }
    let mut check = OwnScope {
        safe: true,
        reject_value_return: false,
        has_value_return: false,
        cancelled,
    };
    for statement in &definition.body {
        check.visit_stmt(statement);
        if !check.safe {
            return false;
        }
    }
    !cancelled()
}

/// The caller establishes that the annotation names a stable builtin.
pub(super) fn constant_returns<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    annotation: &str,
    cancelled: &F,
) -> Option<(TextRange, Vec<&'static str>)> {
    let name = definition.name.as_str();
    if definition.is_async || (name.starts_with("__") && name.ends_with("__")) || cancelled() {
        return None;
    }
    let mut replacements = match annotation {
        "bool" => vec!["return False", "return True"],
        "int" => vec!["return 0", "return 1"],
        "str" => vec!["return \"\"", "return \"A\""],
        _ => return None,
    };
    let body = definition.body.as_slice();
    let body = if body.first().is_some_and(is_docstring) {
        &body[1..]
    } else {
        body
    };
    let first = body.first()?;
    let mut check = OwnScope {
        safe: true,
        reject_value_return: false,
        has_value_return: false,
        cancelled,
    };
    for statement in body {
        check.visit_stmt(statement);
        if !check.safe {
            return None;
        }
    }
    if !check.has_value_return || cancelled() {
        return None;
    }
    if let [Stmt::Return(statement)] = body
        && let Some(value) = statement.value.as_deref()
    {
        replacements.retain(|replacement| !same_literal(value, replacement));
    }
    Some((
        // The first statement above established that this suffix is nonempty.
        TextRange::new(first.start(), body.last().expect("nonempty body").end()),
        replacements,
    ))
}

fn same_literal(value: &Expr, replacement: &str) -> bool {
    match (value, replacement) {
        (Expr::BooleanLiteral(value), "return False") => !value.value,
        (Expr::BooleanLiteral(value), "return True") => value.value,
        (Expr::NumberLiteral(value), "return 0") => {
            matches!(&value.value, Number::Int(n) if n.as_u64() == Some(0))
        }
        (Expr::NumberLiteral(value), "return 1") => {
            matches!(&value.value, Number::Int(n) if n.as_u64() == Some(1))
        }
        (Expr::StringLiteral(value), "return \"\"") => value.value.to_str().is_empty(),
        (Expr::StringLiteral(value), "return \"A\"") => value.value.to_str() == "A",
        _ => false,
    }
}

fn is_docstring(statement: &Stmt) -> bool {
    matches!(statement, Stmt::Expr(value) if matches!(value.value.as_ref(), Expr::StringLiteral(_)))
}
fn is_noop(statement: &Stmt) -> bool {
    match statement {
        Stmt::Pass(_) => true,
        Stmt::Return(value) => value
            .value
            .as_deref()
            .is_none_or(|value| matches!(value, Expr::NoneLiteral(_))),
        Stmt::Expr(value) => matches!(
            value.value.as_ref(),
            Expr::StringLiteral(_) | Expr::NoneLiteral(_) | Expr::EllipsisLiteral(_)
        ),
        _ => false,
    }
}

struct OwnScope<'a, F> {
    safe: bool,
    reject_value_return: bool,
    has_value_return: bool,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for OwnScope<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if !self.safe || (self.cancelled)() {
            self.safe = false;
            return;
        }
        match statement {
            Stmt::FunctionDef(definition) => {
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                self.visit_parameters(&definition.parameters);
            }
            Stmt::ClassDef(definition) => {
                for decorator in &definition.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &definition.arguments {
                    self.visit_arguments(arguments);
                }
            }
            Stmt::TypeAlias(_) => {}
            Stmt::Return(value) => {
                let has_value = value
                    .value
                    .as_deref()
                    .is_some_and(|value| !matches!(value, Expr::NoneLiteral(_)));
                self.has_value_return |= has_value;
                if self.reject_value_return && has_value {
                    self.safe = false;
                } else {
                    visitor::walk_stmt(self, statement);
                }
            }
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_annotation(&mut self, _: &'ast Expr) {}
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if !self.safe || (self.cancelled)() {
            self.safe = false;
            return;
        }
        match expression {
            Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_) => self.safe = false,
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
            }
            _ => visitor::walk_expr(self, expression),
        }
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn run_erased(definition: &StmtFunctionDef, cancelled: &dyn Fn() -> bool) -> Option<TextRange> {
        erased_range(definition, &cancelled)
    }

    fn run_constants(
        definition: &StmtFunctionDef,
        annotation: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, Vec<&'static str>)> {
        constant_returns(definition, annotation, &cancelled)
    }

    #[rstest::rstest]
    #[case("def outer():\n    @decorate\n    def inner():\n        return 1\n")]
    #[case("def outer():\n    @decorate\n    class Inner:\n        pass\n")]
    fn body_erasure_removes_the_complete_first_decorated_declaration(#[case] source: &str) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let range = run_erased(definition, &|| false).unwrap();
        let mut mutated = source.to_owned();
        mutated.replace_range(usize::from(range.start())..usize::from(range.end()), "pass");
        assert!(
            ruff_python_parser::parse_module(&mutated).is_ok(),
            "{mutated}"
        );
        assert_eq!(mutated, "def outer():\n    pass\n");
    }

    #[rstest::rstest]
    #[case("def outer() -> int:\n    @decorate\n    def inner():\n        pass\n    return 1\n")]
    #[case("def outer() -> int:\n    @decorate\n    class Inner:\n        pass\n    return 1\n")]
    fn constant_body_replacement_removes_the_complete_first_decorated_declaration(
        #[case] source: &str,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let (range, replacements) = run_constants(definition, "int", &|| false).unwrap();
        let mut mutated = source.to_owned();
        mutated.replace_range(
            usize::from(range.start())..usize::from(range.end()),
            replacements[0],
        );
        assert!(
            ruff_python_parser::parse_module(&mutated).is_ok(),
            "{mutated}"
        );
        assert_eq!(mutated, "def outer() -> int:\n    return 0\n");
    }
    fn run_plain(definition: &StmtFunctionDef, cancelled: &dyn Fn() -> bool) -> bool {
        plain_synchronous(definition, &cancelled)
    }

    #[rstest::rstest]
    #[case("def function(): act()\n", true)]
    #[case("def function(): pass\n", false)]
    #[case("def function(): return\n", false)]
    #[case("def function(): return None\n", false)]
    #[case("def function(): None\n", false)]
    #[case("def function(): ...\n", false)]
    #[case("def function(): \"documentation\"\n", false)]
    #[case("def function(): \"documentation\"; \"another string\"\n", false)]
    #[case("def function(): return 1\n", false)]
    #[case("async def function(): act()\n", false)]
    #[case("def __special__(): act()\n", false)]
    #[case("def function(): yield 1\n", false)]
    #[case("def function(): yield from values\n", false)]
    #[case("def function():\n def inner(value: int) -> str: return value\n", true)]
    #[case("def function(): callback = lambda: (yield 1)\n", true)]
    #[case("def function(): callback = lambda value=(yield 1): value\n", false)]
    #[case("def function(): type Alias = int\n", true)]
    #[case("def function():\n if condition:\n  yield 1\n else:\n  act()\n", false)]
    #[case("def function(): value = ((yield 1), 2)\n", false)]
    #[case("def function():\n class Inner(Parent): pass\n", true)]
    #[case("def function():\n class Inner((yield base)): pass\n", false)]
    fn body_erasure_requires_meaningful_work_without_own_scope_value_returns_or_yields(
        #[case] source: &str,
        #[case] eligible: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        assert_eq!(run_erased(definition, &|| false).is_some(), eligible);
    }

    #[rstest::rstest]
    #[case("def function(): return 1\n", true)]
    #[case("def function(): yield 1\n", false)]
    #[case("def function(): yield from values\n", false)]
    #[case("async def function(): return 1\n", false)]
    #[case("def function(): callback = lambda: (yield 1)\n", true)]
    #[case("def function():\n def inner(value: int): yield value\n", true)]
    fn synchronous_scope_check_allows_value_returns_but_not_own_scope_yields(
        #[case] source: &str,
        #[case] synchronous: bool,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        assert_eq!(run_plain(definition, &|| false), synchronous);
    }

    #[rstest::rstest]
    #[case("bool", " return False", Some(&["return True"][..]))]
    #[case("bool", " return True", Some(&["return False"][..]))]
    #[case("bool", " return value", Some(&["return False", "return True"][..]))]
    #[case("int", " return 0", Some(&["return 1"][..]))]
    #[case("int", " return 1", Some(&["return 0"][..]))]
    #[case("int", " return 2", Some(&["return 0", "return 1"][..]))]
    #[case("int", " return 0.0", Some(&["return 0", "return 1"][..]))]
    #[case("int", " return True", Some(&["return 0", "return 1"][..]))]
    #[case("str", " return \"\"", Some(&["return \"A\""][..]))]
    #[case("str", " return \"A\"", Some(&["return \"\""][..]))]
    #[case("str", " return \"B\"", Some(&["return \"\"", "return \"A\""][..]))]
    #[case("int", " \"documentation\"\n return 0", Some(&["return 1"][..]))]
    #[case("int", " \"documentation\"", None)]
    #[case("int", " pass", None)]
    #[case("int", " return", None)]
    #[case("int", " return None", None)]
    #[case("int", " if flag:\n  return value\n return None", Some(&["return 0", "return 1"][..]))]
    #[case("float", " return 0", None)]
    #[case("int", " yield 1", None)]
    fn constant_returns_follow_the_annotation_and_remove_only_identical_literals(
        #[case] annotation: &str,
        #[case] body: &str,
        #[case] expected: Option<&[&str]>,
    ) {
        let source = format!("def function() -> {annotation}:\n{body}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        assert_eq!(
            run_constants(definition, annotation, &|| false)
                .map(|(_, replacements)| replacements)
                .as_deref(),
            expected
        );
    }

    #[rstest::rstest]
    #[case("async def function() -> int: return 2\n", None)]
    #[case("def __special__() -> int: return 2\n", None)]
    #[case("def function() -> int: return 2\n", Some(&["return 0", "return 1"][..]))]
    fn constant_returns_skip_async_and_protocol_definitions(
        #[case] source: &str,
        #[case] expected: Option<&[&str]>,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        assert_eq!(
            run_constants(definition, "int", &|| false)
                .map(|(_, replacements)| replacements)
                .as_deref(),
            expected
        );
    }

    #[rstest::rstest]
    #[case(0, false)]
    #[case(1, false)]
    #[case(2, false)]
    #[case(3, false)]
    #[case(4, false)]
    #[case(64, true)]
    fn cancellation_does_not_return_partial_erasure_eligibility(
        #[case] checkpoint: usize,
        #[case] completed: bool,
    ) {
        let parsed = ruff_python_parser::parse_module("def function(): value = 1\n").unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let polls = std::cell::Cell::new(0);
        let cancelled = || {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        };
        assert_eq!(run_erased(definition, &cancelled).is_some(), completed);
    }

    #[rstest::rstest]
    #[case(0, false)]
    #[case(1, false)]
    #[case(2, false)]
    #[case(3, false)]
    #[case(4, false)]
    #[case(64, true)]
    fn cancellation_does_not_return_partial_synchronous_eligibility(
        #[case] checkpoint: usize,
        #[case] completed: bool,
    ) {
        let parsed = ruff_python_parser::parse_module("def function(): value = 1\n").unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let polls = std::cell::Cell::new(0);
        let cancelled = || {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        };
        assert_eq!(run_plain(definition, &cancelled), completed);
    }

    #[rstest::rstest]
    #[case(0, false)]
    #[case(1, false)]
    #[case(2, false)]
    #[case(3, false)]
    #[case(64, true)]
    fn cancellation_does_not_return_partial_constant_eligibility(
        #[case] checkpoint: usize,
        #[case] completed: bool,
    ) {
        let parsed = ruff_python_parser::parse_module("def function() -> int: return 2\n").unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let polls = std::cell::Cell::new(0);
        let cancelled = || {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        };
        assert_eq!(
            run_constants(definition, "int", &cancelled).is_some(),
            completed
        );
    }
}
