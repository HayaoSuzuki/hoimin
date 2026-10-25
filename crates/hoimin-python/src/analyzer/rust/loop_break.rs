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
    let last = body.last()?;
    let end = usize::from(last.end());
    let line_start = source[..first].rfind(['\r', '\n']).map_or(0, |i| i + 1);
    let indentation = &source[line_start..first];
    if !indentation
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | 0x0c))
    {
        // Inline suites keep their existing trailing semicolon/comment on the line.
        let (start, _) = source[..end].char_indices().next_back()?;
        return Some((
            TextRange::new(TextSize::try_from(start).ok()?, last.end()),
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
            let (start, _) = source.char_indices().next_back()?;
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
            TextSize::try_from(start).ok()?,
            TextSize::try_from(anchor_end).ok()?,
        ),
        text,
    ))
}
