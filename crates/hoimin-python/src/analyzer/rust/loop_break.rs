//! Source anchors for a break at the end of a loop's own suite.
use ruff_python_ast::Stmt;
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};

pub(super) fn replacement<F: Fn() -> bool>(
    statement: &Stmt,
    source: &str,
    tokens: &Tokens,
    cancelled: &F,
) -> Option<(TextRange, String)> {
    let body = match statement {
        Stmt::For(statement) => &statement.body,
        Stmt::While(statement) => &statement.body,
        _ => return None,
    };
    for statement in body {
        if cancelled()
            || matches!(
                statement,
                Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_) | Stmt::Raise(_)
            )
        {
            return None;
        }
    }
    let first = usize::from(body.first()?.start());
    // The successful first lookup proves this suite is nonempty.
    let last = body.last().expect("nonempty loop suite");
    let end = usize::from(last.end());
    let line_start = source[..first].rfind(['\r', '\n']).map_or(0, |i| i + 1);
    let indentation = &source[line_start..first];
    if !indentation
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | 0x0c))
    {
        // Inline suites keep their existing trailing semicolon/comment on the line.
        // A parsed statement has a nonempty range within the parsed source.
        let (start, _) = source[..end]
            .char_indices()
            .next_back()
            .expect("statement has source text");
        return Some((
            TextRange::new(
                TextSize::try_from(start).expect("offset is bounded by the parsed statement end"),
                last.end(),
            ),
            format!("{}; break", &source[start..end]),
        ));
    }
    // Insert after the logical line, retaining comments and explicit continuations.
    // A newline (or final character at EOF) is a nonempty source anchor; it
    // avoids retaining a second copy of an arbitrarily large loop body.
    let mut logical_newline = None;
    for token in tokens.after(last.end()) {
        if cancelled() {
            return None;
        }
        if token.kind() == TokenKind::Newline {
            logical_newline = Some(token.range());
            break;
        }
    }
    let (start, anchor_end, newline) =
        if let Some(range) = logical_newline.filter(|range| !range.is_empty()) {
            let anchor = &source[usize::from(range.start())..usize::from(range.end())];
            (usize::from(range.start()), usize::from(range.end()), anchor)
        } else {
            let (start, _) = source
                .char_indices()
                .next_back()
                .expect("source contains the loop suite");
            let newline = if line_start >= 2 && source.as_bytes()[line_start - 2] == b'\r' {
                "\r\n"
            } else if line_start > 0 && source.as_bytes()[line_start - 1] == b'\r' {
                "\r"
            } else {
                "\n"
            };
            (start, source.len(), newline)
        };
    let anchor = &source[start..anchor_end];
    let text = if anchor.ends_with(['\r', '\n']) {
        format!("{anchor}{indentation}break{newline}")
    } else {
        format!("{anchor}{newline}{indentation}break")
    };
    Some((
        TextRange::new(
            TextSize::try_from(start).expect("parsed source offsets fit TextSize"),
            TextSize::try_from(anchor_end).expect("parsed source offsets fit TextSize"),
        ),
        text,
    ))
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn run(
        statement: &Stmt,
        source: &str,
        tokens: &Tokens,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<(TextRange, String)> {
        replacement(statement, source, tokens, &cancelled)
    }

    #[rstest::rstest]
    #[case(
        "while condition:\n action()",
        Some("while condition:\n action()\n break")
    )]
    #[case(
        "while condition:\r\n action()",
        Some("while condition:\r\n action()\r\n break")
    )]
    #[case(
        "while condition:\r action()",
        Some("while condition:\r action()\r break")
    )]
    #[case("while condition: action()", Some("while condition: action(); break"))]
    #[case("while condition: é", Some("while condition: é; break"))]
    #[case("while condition:\n é", Some("while condition:\n é\n break"))]
    #[case(
        "for item in values:\n action()\n",
        Some("for item in values:\n action()\n break\n")
    )]
    #[case("pass", None)]
    #[case("while condition:\n break", None)]
    #[case("while condition:\n continue", None)]
    #[case("while condition:\n return", None)]
    #[case("while condition:\n raise", None)]
    fn loop_break_anchor_preserves_line_endings_and_rejects_terminal_suites(
        #[case] source: &str,
        #[case] expected: Option<&str>,
    ) {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let result = run(&parsed.syntax().body[0], source, parsed.tokens(), &|| false).map(
            |(range, replacement)| {
                let mut result = source.to_owned();
                result.replace_range(
                    usize::from(range.start())..usize::from(range.end()),
                    &replacement,
                );
                result
            },
        );

        let actual = result.as_deref();

        assert_eq!(actual, expected);
    }

    #[rstest::rstest]
    fn cancellation_stops_during_suite_or_trailing_token_scan(
        #[values(0, 1, 2)] checkpoint: usize,
    ) {
        let source = "while condition:\n action() # retained comment\n";
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let polls = std::cell::Cell::new(0);

        let actual = run(&parsed.syntax().body[0], source, parsed.tokens(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        let actual_2 = polls.get();

        assert_eq!(actual, None);
        assert_eq!(actual_2, checkpoint + 1);
    }

    #[test]
    fn empty_loop_suite_has_no_source_anchor() {
        let source = "while condition:\n pass";
        let mut parsed = ruff_python_parser::parse_module(source)
            .unwrap()
            .into_syntax();
        parsed.body[0].as_while_stmt_mut().unwrap().body.clear();
        let tokens = ruff_python_parser::parse_module(source).unwrap();

        let actual = run(&parsed.body[0], source, tokens.tokens(), &|| false);

        assert_eq!(actual, None);
    }
}
