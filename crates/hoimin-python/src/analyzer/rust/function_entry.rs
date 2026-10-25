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
    let first = definition.body.first()?;
    if matches!(first, Stmt::Expr(value) if matches!(value.value.as_ref(), Expr::StringLiteral(_)))
    {
        definition.body.get(1)
    } else {
        Some(first)
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
    for token in tokens.in_range(TextRange::new(
        definition.start(),
        definition.body.first()?.start(),
    )) {
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
        TextRange::at(start, TextSize::try_from(width).ok()?),
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
