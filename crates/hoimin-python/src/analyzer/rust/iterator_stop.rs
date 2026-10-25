//! Protocol-specific early exhaustion at a method's entry.
use crate::visitor::{self, Visitor};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange, TextSize};

pub(super) fn entry<F: Fn() -> bool>(
    definition: &StmtFunctionDef,
    source: &str,
    tokens: &Tokens,
    exception: &str,
    cancelled: &F,
) -> Option<(TextRange, String)> {
    if !definition.decorator_list.is_empty() || cancelled() {
        return None;
    }
    let mut check = EntryExclusions {
        found: false,
        exception,
        cancelled,
    };
    check.visit_body(&definition.body);
    if check.found || cancelled() {
        return None;
    }
    let body = definition.body.as_slice();
    let first_body = body.first()?;
    let first = if matches!(first_body, Stmt::Expr(value) if matches!(value.value.as_ref(), Expr::StringLiteral(_)))
    {
        body.get(1)?
    } else {
        first_body
    };
    if let Stmt::Raise(statement) = first
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
    let mut multiline = false;
    for token in tokens.in_range(TextRange::new(definition.start(), first_body.start())) {
        if cancelled() {
            return None;
        }
        multiline |= token.kind() == TokenKind::Newline;
    }
    let offset = usize::from(start);
    let width = source[offset..].chars().next()?.len_utf8();
    let original = &source[offset..offset + width];
    let replacement = if multiline {
        let line_start = source[..offset].rfind(['\r', '\n']).map_or(0, |i| i + 1);
        let indentation = &source[line_start..offset];
        if !indentation
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | 0x0c))
        {
            return Some((
                TextRange::at(start, TextSize::try_from(width).ok()?),
                format!("raise {exception}(); {original}"),
            ));
        }
        let newline = if line_start >= 2 && source.as_bytes()[line_start - 2] == b'\r' {
            "\r\n"
        } else if line_start > 0 && source.as_bytes()[line_start - 1] == b'\r' {
            "\r"
        } else {
            "\n"
        };
        format!("raise {exception}(){newline}{indentation}{original}")
    } else {
        format!("raise {exception}(); {original}")
    };
    Some((
        TextRange::at(start, TextSize::try_from(width).ok()?),
        replacement,
    ))
}

struct EntryExclusions<'a, F> {
    found: bool,
    exception: &'a str,
    cancelled: &'a F,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for EntryExclusions<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if self.found || (self.cancelled)() {
            return;
        }
        match statement {
            // Python rejects any earlier use of a name declared global later in
            // this scope, even when the declaration is in a conditional branch.
            Stmt::Global(global)
                if global
                    .names
                    .iter()
                    .any(|name| name.as_str() == self.exception) =>
            {
                self.found = true;
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
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.found || (self.cancelled)() {
            return;
        }
        match expression {
            Expr::Yield(_) | Expr::YieldFrom(_) => self.found = true,
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
            }
            _ => visitor::walk_expr(self, expression),
        }
    }
}
