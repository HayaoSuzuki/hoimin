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
