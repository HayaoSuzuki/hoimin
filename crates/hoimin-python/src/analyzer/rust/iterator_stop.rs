//! Protocol-specific early exhaustion at a method's entry.
use super::function_entry;
use ruff_python_ast::token::Tokens;
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_text_size::TextRange;

pub(super) fn entry<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    source: &str,
    tokens: &Tokens,
    exception: &str,
    cancelled: &F,
) -> Option<(TextRange, String)> {
    if !definition.decorator_list.is_empty() {
        return None;
    }
    let facts = function_entry::inspect(definition, Some(exception), cancelled)?;
    if facts.has_yield || facts.declares_global {
        return None;
    }
    if let Stmt::Raise(statement) = function_entry::first_statement(definition)?
        && let Some(exc) = &statement.exc
    {
        let name = match exc.as_ref() {
            Expr::Name(name) => Some(name.id.as_str()),
            Expr::Call(call) => match call.func.as_ref() {
                Expr::Name(name) => Some(name.id.as_str()),
                _ => None,
            },
            _ => None,
        };
        if name == Some(exception) {
            return None;
        }
    }
    function_entry::insert(
        definition,
        source,
        tokens,
        &format!("raise {exception}()"),
        cancelled,
    )
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_entry(
        definition: &StmtFunctionDef,
        source: &str,
        tokens: &Tokens,
        exception: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, String)> {
        entry(definition, source, tokens, exception, &cancelled)
    }

    #[rstest::rstest]
    #[case("raise StopIteration", None)]
    #[case("raise StopIteration()", None)]
    #[case("raise package.StopIteration()", Some("raise StopIteration()\n    r"))]
    #[case("raise package.StopIteration", Some("raise StopIteration()\n    r"))]
    #[case("raise errors[0]", Some("raise StopIteration()\n    r"))]
    fn existing_raise_suppresses_entry_only_for_the_exact_protocol_exception(
        #[case] statement: &str,
        #[case] expected: Option<&str>,
    ) {
        let source = format!("def __next__(self):\n    {statement}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let result = run_entry(
            definition,
            &source,
            parsed.tokens(),
            "StopIteration",
            &|| false,
        );

        let actual = result.map(|(_, replacement)| replacement);

        assert_eq!(actual, expected.map(str::to_owned));
    }
    #[rstest::rstest]
    #[case("@decorator\ndef __next__(self):\n    pass\n", None)]
    #[case("def __next__(self):\n    yield 1\n", None)]
    #[case("def __next__(self):\n    global StopIteration\n", None)]
    #[case("def __next__(self):\n    'documentation'\n", None)]
    #[case(
        "def __next__(self):\n    pass\n",
        Some("raise StopIteration()\n    p")
    )]
    fn exhaustion_entry_requires_an_unadorned_non_generator_body(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();

        let actual = run_entry(
            definition,
            source,
            parsed.tokens(),
            "StopIteration",
            &|| false,
        )
        .map(|(_, replacement)| replacement);

        assert_eq!(actual, expected.map(str::to_owned));
    }

    #[rstest::rstest]
    #[case(0, false)]
    #[case(1, false)]
    #[case(2, false)]
    #[case(64, true)]
    fn exhaustion_entry_stops_during_inspection_and_insertion(
        #[case] checkpoint: usize,
        #[case] completed: bool,
    ) {
        let source = "def __next__(self):\n    pass\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let polls = Cell::new(0);
        let result = run_entry(
            definition,
            source,
            parsed.tokens(),
            "StopIteration",
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
        );

        let actual = result.is_some();

        assert_eq!(actual, completed);
    }
}
