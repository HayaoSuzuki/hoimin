//! Context exit decisions and individual yield sites, preserving entry and lexical scope.
use super::{
    AstCandidateCollector, EditSink, EnclosingBody, MutationOperator, Ranged, source_text,
};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef, StmtReturn, StmtWith};
use ruff_text_size::{TextRange, TextSize};

/// Names loaded by the generated proxy, also used to snapshot the lexical site.
pub(super) const BUILTIN_HELPERS: [&str; 4] = ["type", "property", "next", "TypeError"];

#[derive(Clone, Copy, Default)]
pub(super) enum ExitScope {
    #[default]
    Outside,
    Method,
}

impl ExitScope {
    pub(super) fn for_function(
        definition: &StmtFunctionDef,
        enclosing: EnclosingBody,
        cancelled: &impl Fn() -> bool,
    ) -> Self {
        if enclosing == EnclosingBody::Class
            && matches!(
                (definition.name.as_str(), definition.is_async),
                ("__exit__", false) | ("__aexit__", true)
            )
            && super::function_entry::inspect(definition, None, cancelled)
                .is_some_and(|facts| !facts.has_yield)
        {
            Self::Method
        } else {
            Self::Outside
        }
    }
}

impl<F: Fn() -> bool> AstCandidateCollector<'_, F> {
    pub(super) fn collect_lifecycle_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::With(context)
                if self
                    .request
                    .operators
                    .contains(MutationOperator::ContextExitOmit) =>
            {
                self.collect_context_exit_omit(context);
            }
            Stmt::Return(returned)
                if matches!(self.exit_scope, ExitScope::Method)
                    && self
                        .request
                        .operators
                        .contains(MutationOperator::ContextExitResultNegate) =>
            {
                self.collect_exit_result(returned);
            }
            Stmt::Expr(statement)
                if self
                    .request
                    .operators
                    .contains(MutationOperator::YieldStatementRemove) =>
            {
                if let Expr::Yield(yielded) = statement.value.as_ref() {
                    let range = yielded.range();
                    if !self.processing_site.delimiters.can_enclose(range, 3) {
                        return;
                    }
                    let source = self.source;
                    self.add_deferred_candidate(
                        range,
                        MutationOperator::YieldStatementRemove,
                        range,
                        || {
                            if let Some(value) = &yielded.value {
                                let original =
                                    source_text(source, value.range()).expect("parsed source");
                                format!("(({original}), (None if True else (yield)))[1]")
                            } else {
                                "(None if True else (yield))".into()
                            }
                        },
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_context_exit_omit(&mut self, context: &StmtWith) {
        let site = self.processing_site;
        for item in &context.items {
            if self.check_cancelled() {
                break;
            }
            let range = item.context_expr.range();
            // The original expression stays an argument in its original scope.
            // Static proxy syntax has at most seven extra delimiter levels.
            if !self.needs_validation(range.start(), MutationOperator::ContextExitOmit)
                || !site.delimiters.can_enclose(range, 2)
                || !site.delimiters.room_before(range.start(), 7)
                || !self.expression_selected(&item.context_expr)
            {
                continue;
            }
            let helpers = BUILTIN_HELPERS.map(|name| {
                site.builtins
                    .destination_before_operand(name, range.start(), site)
            });
            let [Some(class), Some(property), Some(next), Some(type_error)] = helpers else {
                continue;
            };
            let original = source_text(self.source, range).expect("parsed source");
            self.add_deferred_candidate(range, MutationOperator::ContextExitOmit, range, || {
                exit_omission(
                    original,
                    [&class, &property, &next, &type_error],
                    context.is_async,
                )
            });
        }
    }

    fn collect_exit_result(&mut self, returned: &StmtReturn) {
        let operator = MutationOperator::ContextExitResultNegate;
        if let Some(value) = &returned.value {
            let range = value.range();
            let delimiters = self.processing_site.delimiters;
            if !delimiters.can_enclose(range, 3) || !delimiters.room_before(range.start(), 4) {
                return;
            }
            let original = source_text(self.source, range).expect("parsed source");
            // Native with tests truth only for exceptional exit, after the
            // method has unwound its own cleanup. Move the decision onto the
            // stack and clear the frame's proxy reference before testing it,
            // so a retained traceback cannot extend the decision's lifetime.
            self.add_deferred_candidate(range, operator, range, || {
                format!(
                    "().__class__.__class__('_HoiminExitDecision', (().__class__,), \
                     {{'__slots__': (), '__bool__': lambda _hoimin_value: \
                     not (_hoimin_value[0], (_hoimin_value := None))[0]}})((({original}),))"
                )
            });
        } else {
            self.add_candidate(returned.range(), "return True".into(), operator);
        }
    }

    pub(super) fn collect_implicit_exit_result(&mut self, definition: &StmtFunctionDef) {
        let Some(last) = definition.body.last() else {
            return;
        };
        let operator = MutationOperator::ContextExitResultNegate;
        if !self.needs_validation(last.start(), operator)
            || !self.expression_selected(last)
            || matches!(last, Stmt::Return(_) | Stmt::Raise(_))
        {
            return;
        }
        let first = definition.body.first().expect("nonempty body");
        let tokens = self.processing_site.tokens();
        let mut after_def = false;
        let mut block_header = None;
        let mut block_indent = None;
        // Escaped physical newlines in an inline suite do not produce a
        // logical Newline token. Decorator newlines precede the def token.
        for token in tokens.in_range(TextRange::new(definition.start(), first.start())) {
            if self.check_cancelled() {
                return;
            }
            after_def |= token.kind() == TokenKind::Def;
            if after_def && token.kind() == TokenKind::Newline {
                block_header = Some(token.range());
            }
            if block_header.is_some() && token.kind() == TokenKind::Indent {
                block_indent = Some(token.range());
                break;
            }
        }
        let mut end = last.end();
        let (separator, indent) = if let Some(header) = block_header {
            let Some(raw_indent) = block_indent.and_then(|range| source_text(self.source, range))
            else {
                return;
            };
            let indent = &raw_indent[super::effective_indentation(raw_indent)];
            for token in tokens.after(last.end()) {
                if self.check_cancelled() {
                    return;
                }
                if token.kind() == TokenKind::Newline && !token.range().is_empty() {
                    let range = token.range();
                    let newline = source_text(self.source, range).expect("parsed source");
                    self.add_deferred_candidate_at(
                        range,
                        operator,
                        last.range(),
                        last.start(),
                        || format!("{newline}{indent}return True{newline}"),
                    );
                    return;
                }
                if !(token.kind().is_trivia() || token.kind() == TokenKind::Semi) {
                    break;
                }
                end = token.end();
            }
            (
                source_text(self.source, header).expect("parsed source"),
                indent,
            )
        } else {
            ("; ", "")
        };
        let tail = self.source[..usize::from(end)]
            .chars()
            .next_back()
            .expect("nonempty body");
        let width = TextSize::try_from(tail.len_utf8()).expect("UTF-8 width fits TextSize");
        let range = TextRange::new(end - width, end);
        self.add_deferred_candidate_at(range, operator, last.range(), last.start(), || {
            format!("{tail}{separator}{indent}return True")
        });
    }
}

/// Native with/async-with looks up the properties of the proxy in protocol
/// order. Each property resolves the original method on the actual class MRO
/// and binds it to the original manager, including custom descriptors.
/// The no-op holds the bound exit method for its normal lifetime. Only the
/// proxy instance owns the manager, so the proxy type cannot retain it.
/// Binding temporarily moves the manager onto the value stack. A failed
/// descriptor must leave the holder empty even if its traceback survives.
fn exit_omission(original: &str, helpers: [&str; 4], asynchronous: bool) -> String {
    let [class, property, next, type_error] = helpers;
    let (enter, leave, protocol, awaitable, exit_result) = if asynchronous {
        (
            "__aenter__",
            "__aexit__",
            "asynchronous context manager",
            concat!(
                "_hoimin_awaitable := _hoimin_type('_HoiminAwaitable', (), ",
                "{'__await__': lambda _hoimin_self: ().__iter__()}),",
            ),
            "_hoimin_awaitable()",
        )
    } else {
        ("__enter__", "__exit__", "context manager", "", "None")
    };
    format!(
        r#"(lambda _hoimin_type, _hoimin_property, _hoimin_next, _hoimin_error, _hoimin_cm: (
_hoimin_type_members := _hoimin_type.__dict__,
_hoimin_absent := [],
_hoimin_lookup := lambda _hoimin_cls, _hoimin_name: _hoimin_next(
    (_hoimin_value
     for _hoimin_base in _hoimin_type_members['__mro__'].__get__(_hoimin_cls)
     for _hoimin_ns in (_hoimin_type_members['__dict__'].__get__(_hoimin_base),)
     for _hoimin_value in (_hoimin_ns.get(_hoimin_name, _hoimin_absent),)
     if _hoimin_value is not _hoimin_absent), _hoimin_absent),
_hoimin_missing := lambda _hoimin_cls, _hoimin_name:
    (_hoimin_once for _hoimin_once in ()).throw(_hoimin_error(
        "'%s' object does not support the {protocol} protocol (missed %s method)" %
        (_hoimin_type_members['__qualname__'].__get__(_hoimin_cls), _hoimin_name))),
_hoimin_bind := lambda _hoimin_holder, _hoimin_name: (
    _hoimin_cls := _hoimin_type(_hoimin_holder[0]),
    _hoimin_raw := _hoimin_lookup(_hoimin_cls, _hoimin_name),
    _hoimin_get := _hoimin_lookup(_hoimin_type(_hoimin_raw), '__get__'),
    _hoimin_holder.extend((
        _hoimin_holder[0],
        _hoimin_get(_hoimin_raw, _hoimin_holder.pop(), _hoimin_cls)
        if _hoimin_get is not _hoimin_absent else
        (_hoimin_holder.pop(), _hoimin_raw if _hoimin_raw is not _hoimin_absent else
         _hoimin_missing(_hoimin_cls, _hoimin_name))[1],
    )),
    _hoimin_holder.pop(),
)[-1],
{awaitable}
_hoimin_type('_HoiminContext', (_hoimin_type([]),), {{
    '__slots__': (),
    '{enter}': _hoimin_property(lambda _hoimin_self: _hoimin_bind(_hoimin_self, '{enter}')),
    '{leave}': _hoimin_property(lambda _hoimin_self:
        lambda *_hoimin_exc, _hoimin_exit=_hoimin_bind(_hoimin_self, '{leave}'): {exit_result}),
}})([_hoimin_cm])
)[-1])({class}, {property}, {next}, {type_error}, ({original}))"#
    )
}
