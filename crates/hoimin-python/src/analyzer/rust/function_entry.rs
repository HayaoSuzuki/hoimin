//! Entry insertion and facts from the executing scope of a function body.
use crate::visitor::{self, Visitor};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange, TextSize};

pub(super) struct ScopeFacts {
    pub has_yield: bool,
    pub declares_global: bool,
}

pub(super) fn first_statement(definition: &StmtFunctionDef) -> Option<&Stmt> {
    let body = definition.body.as_slice();
    match body {
        [Stmt::Expr(value), rest @ ..]
            if matches!(value.value.as_ref(), Expr::StringLiteral(_)) =>
        {
            rest.first()
        }
        _ => body.first(),
    }
}

pub(super) fn insert<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    source: &str,
    tokens: &Tokens,
    inserted: &str,
    cancelled: &F,
) -> Option<(TextRange, String)> {
    let first = first_statement(definition)?;
    let start = match first {
        Stmt::FunctionDef(value) => value
            .decorator_list
            .first()
            .map_or(first.start(), Ranged::start),
        Stmt::ClassDef(value) => value
            .decorator_list
            .first()
            .map_or(first.start(), Ranged::start),
        _ => first.start(),
    };
    let mut header = false;
    let mut multiline = false;
    // first_statement succeeded, which proves the definition's body is nonempty.
    let header_end = definition.body[0].start();
    for token in tokens.in_range(TextRange::new(definition.start(), header_end)) {
        if cancelled() {
            return None;
        }
        if token.kind() == TokenKind::Def {
            header = true;
        }
        if header && token.kind() == TokenKind::Newline {
            multiline = true;
            break;
        }
    }
    let offset = usize::from(start);
    let width = source[offset..].chars().next()?.len_utf8();
    let original = &source[offset..offset + width];
    let line_start = source[..offset].rfind(['\r', '\n']).map_or(0, |i| i + 1);
    let indentation = &source[line_start..offset];
    let replacement = if multiline
        && indentation
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | 0x0c))
    {
        let newline = if line_start >= 2 && source.as_bytes()[line_start - 2] == b'\r' {
            "\r\n"
        } else if line_start > 0 && source.as_bytes()[line_start - 1] == b'\r' {
            "\r"
        } else {
            "\n"
        };
        format!("{inserted}{newline}{indentation}{original}")
    } else {
        format!("{inserted}; {original}")
    };
    Some((
        // A single UTF-8 scalar occupies at most four bytes.
        TextRange::at(
            start,
            TextSize::try_from(width).expect("UTF-8 width fits TextSize"),
        ),
        replacement,
    ))
}

pub(super) fn inspect<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    introduced_name: Option<&str>,
    cancelled: &F,
) -> Option<ScopeFacts> {
    let mut inspection = OwnScope {
        has_yield: false,
        declares_global: false,
        cancelled_observed: false,
        introduced_name,
        cancelled,
    };
    inspection.visit_body(&definition.body);
    if inspection.cancelled_observed || cancelled() {
        return None;
    }
    Some(ScopeFacts {
        has_yield: inspection.has_yield,
        declares_global: inspection.declares_global,
    })
}

struct OwnScope<'a, F> {
    has_yield: bool,
    declares_global: bool,
    cancelled_observed: bool,
    introduced_name: Option<&'a str>,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for OwnScope<'_, F> {
    fn visit_annotation(&mut self, _: &'ast Expr) {}
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if self.cancelled_observed || (self.cancelled)() {
            self.cancelled_observed = true;
            return;
        }
        match statement {
            Stmt::Global(global) => {
                self.declares_global |= self
                    .introduced_name
                    .is_some_and(|name| global.names.iter().any(|item| item.as_str() == name));
            }
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
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.cancelled_observed || (self.cancelled)() {
            self.cancelled_observed = true;
            return;
        }
        match expression {
            Expr::Yield(_) | Expr::YieldFrom(_) => self.has_yield = true,
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

    fn run_insert(
        definition: &StmtFunctionDef,
        source: &str,
        tokens: &Tokens,
        inserted: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, String)> {
        insert(definition, source, tokens, inserted, &cancelled)
    }

    fn run_inspect(
        definition: &StmtFunctionDef,
        introduced_name: Option<&str>,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<ScopeFacts> {
        inspect(definition, introduced_name, &cancelled)
    }

    #[rstest::rstest]
    #[case("@decorate\n    def nested(): pass", "@decorate")]
    #[case("@decorate\n    class Nested: pass", "@decorate")]
    #[case("def nested(): pass", "def nested")]
    #[case("class Nested: pass", "class Nested")]
    fn inserted_statement_precedes_the_first_nested_definition_and_its_decorators(
        #[case] body: &str,
        #[case] first: &str,
    ) {
        let source = format!("def outer():\n    {body}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let (range, replacement) =
            run_insert(definition, &source, parsed.tokens(), "return", &|| false).unwrap();
        let mut mutated = source.clone();
        mutated.replace_range(
            usize::from(range.start())..usize::from(range.end()),
            &replacement,
        );

        let actual = ruff_python_parser::parse_module(&mutated).is_ok();

        assert_eq!(
            mutated,
            source.replacen(first, &format!("return\n    {first}"), 1)
        );
        assert!(actual);
    }
    #[rstest::rstest]
    #[case("    type Alias = int\n    return 0", false)]
    #[case("    callback = lambda: (yield 1)", false)]
    #[case("    callback = lambda arg=(yield 1): arg", true)]
    #[case("    def nested(arg=(yield 1)):\n        yield 2", true)]
    #[case(
        "    @decorator\n    def nested(arg: int) -> str:\n        yield 1",
        false
    )]
    #[case(
        "    @decorator\n    class Nested:\n        def method(self): yield 1",
        false
    )]
    #[case("    class Nested((yield base)):\n        pass", true)]
    fn own_scope_facts_visit_eager_headers_and_skip_deferred_bodies(
        #[case] body: &str,
        #[case] has_yield: bool,
    ) {
        let source = format!("def outer():\n{body}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let facts = run_inspect(definition, None, &|| false).unwrap();

        assert_eq!(facts.has_yield, has_yield);
        assert!(!facts.declares_global);
    }

    #[test]
    fn cancelled_header_scan_does_not_create_an_entry_insertion() {
        let source = "def outer():\n    pass\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();

        let actual = run_insert(definition, source, parsed.tokens(), "return", &|| true);

        assert_eq!(actual, None);
    }
    #[rstest::rstest]
    #[case(0, false)]
    #[case(1, false)]
    #[case(2, false)]
    #[case(3, false)]
    #[case(64, true)]
    fn own_scope_inspection_does_not_return_partial_facts_after_cancellation(
        #[case] checkpoint: usize,
        #[case] completed: bool,
    ) {
        let parsed = ruff_python_parser::parse_module("def function():\n    value = 1\n").unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let polls = std::cell::Cell::new(0);

        let actual = run_inspect(definition, None, &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        })
        .is_some();

        assert_eq!(actual, completed);
    }

    #[test]
    fn entry_insertion_rejects_source_ending_at_the_insertion_point() {
        let parsed = ruff_python_parser::parse_module("def function():\n    pass\n").unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let source = " ".repeat(usize::from(first_statement(definition).unwrap().start()));

        let actual = run_insert(definition, &source, parsed.tokens(), "return", &|| false);

        assert_eq!(actual, None);
    }
    #[rstest::rstest]
    #[case("変数 = 1", 3)]
    #[case("𐐀 = 1", 4)]
    fn insertion_retains_the_complete_first_unicode_scalar(
        #[case] body: &str,
        #[case] width: usize,
    ) {
        let source = format!("def function():\n    {body}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let (range, replacement) =
            run_insert(definition, &source, parsed.tokens(), "return", &|| false).unwrap();
        let replaced_scalar_width = usize::from(range.len());
        let mut mutated = source.clone();
        mutated.replace_range(
            usize::from(range.start())..usize::from(range.end()),
            &replacement,
        );
        let mutated_source = mutated;

        assert_eq!(replaced_scalar_width, width);
        assert_eq!(
            mutated_source,
            source.replacen(body, &format!("return\n    {body}"), 1)
        );
    }
    #[test]
    fn entry_insertion_leaves_a_docstring_only_body_without_an_insertion_target() {
        let source = "def function():\n    'documentation'\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();

        let actual = run_insert(definition, source, parsed.tokens(), "return", &|| false);

        assert_eq!(actual, None);
    }
    #[rstest::rstest]
    #[case("\n")]
    #[case("\r\n")]
    #[case("\r")]
    fn multiline_insertion_preserves_the_original_newline_sequence(#[case] newline: &str) {
        let source = format!("def function():{newline}    pass{newline}");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let (range, replacement) =
            run_insert(definition, &source, parsed.tokens(), "return", &|| false).unwrap();
        let mut mutated = source;
        mutated.replace_range(
            usize::from(range.start())..usize::from(range.end()),
            &replacement,
        );

        assert_eq!(
            mutated,
            format!("def function():{newline}    return{newline}    pass{newline}")
        );
    }

    #[rstest::rstest]
    #[case("def function(): pass\n", "def function(): return; pass\n")]
    #[case(
        "def function(): 'documentation'; pass\n",
        "def function(): 'documentation'; return; pass\n"
    )]
    fn inline_insertion_uses_a_semicolon_after_any_docstring(
        #[case] source: &str,
        #[case] expected: &str,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let definition = parsed.syntax().body[0].as_function_def_stmt().unwrap();
        let (range, replacement) =
            run_insert(definition, source, parsed.tokens(), "return", &|| false).unwrap();
        let mut mutated = source.to_owned();
        mutated.replace_range(
            usize::from(range.start())..usize::from(range.end()),
            &replacement,
        );

        assert_eq!(mutated, expected);
    }
}
