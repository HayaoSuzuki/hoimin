//! Shared source and callable checks for collection and precision edits.

use super::{
    AnalysisCancelled, AstFacts, HelperSite, NameResolution, callable_imports::CallableImports,
    source_delimiters::DelimiterDepth, source_text,
};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::token::{Tokens, parenthesized_range};
use ruff_python_ast::{Expr, ExprCall, ModModule};
use ruff_text_size::{Ranged, TextRange, TextSize};

pub(super) struct Site<'a> {
    pub(super) facts: &'a AstFacts<'a>,
    pub(super) source: &'a str,
    pub(super) operators: &'a MutationOperatorSelection,
    pub(super) delimiters: &'a DelimiterDepth,
    pub(super) builtins: &'a Builtins,
    pub(super) cancelled: &'a dyn Fn() -> bool,
}

impl Site<'_> {
    pub(super) fn text(&self, range: TextRange) -> Option<&str> {
        source_text(self.source, range)
    }

    pub(super) fn tokens(&self) -> &Tokens {
        self.facts.tokens.expect("checked parse")
    }

    pub(super) fn builtin_at(&self, name: &str, anchor: &Expr) -> bool {
        let mut current = anchor;
        while let Expr::Attribute(attribute) = current {
            current = &attribute.value;
        }
        self.facts
            .name_resolution
            .resolution(usize::from(current.start()), name)
            == NameResolution::DefinitelyBuiltin
    }

    pub(super) fn supports(&self, op: MutationOperator) -> bool {
        self.operators.contains(op)
    }

    pub(super) fn argument_range(&self, expr: &Expr, call: &ExprCall) -> TextRange {
        parenthesized_range(expr.into(), (&call.arguments).into(), self.tokens())
            .unwrap_or_else(|| expr.range())
    }
}

/// Guards calls to builtins against both lexical shadowing and changes to the
/// builtin module. Module imports have their own family-local escape proof.
#[derive(Default)]
pub(super) struct Builtins(CallableImports);

impl Builtins {
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        Ok(Self(CallableImports::build_family(
            module,
            cancelled,
            &["builtins"],
        )?))
    }

    pub(super) fn ordinary_namespace(&self) -> bool {
        self.0.import_builtin_available()
    }

    pub(super) fn resolves(&self, expr: &Expr, name: &str, site: &Site<'_>) -> bool {
        self.0
            .resolve(expr)
            .is_some_and(|value| value.member == name)
            || (expr.as_name_expr().is_some_and(|value| value.id == name)
                && self.bare_name(name, expr, site))
    }

    fn bare_name(&self, name: &str, anchor: &Expr, site: &Site<'_>) -> bool {
        self.0.namespace_stable() && site.builtin_at(name, anchor)
    }

    /// Use an unshadowed builtin name or an eager, stable import at the site.
    pub(super) fn destination(&self, name: &str, anchor: &Expr, site: &Site<'_>) -> Option<String> {
        if self.bare_name(name, anchor, site) {
            Some(name.to_owned())
        } else {
            self.0.destination_spelling(name, anchor)
        }
    }

    /// Resolve a helper evaluated before the original context-manager operand.
    pub(super) fn destination_before_operand(
        &self,
        name: &str,
        offset: TextSize,
        site: &Site<'_>,
    ) -> Option<String> {
        self.generated_destination(name, HelperSite::BeforeOperand(offset), site)
    }

    pub(super) fn generated_destination(
        &self,
        name: &str,
        position: HelperSite,
        site: &Site<'_>,
    ) -> Option<String> {
        let resolution = &site.facts.name_resolution;
        if self.0.namespace_stable()
            && resolution.helper_resolution(position, name) == NameResolution::DefinitelyBuiltin
        {
            Some(name.to_owned())
        } else {
            self.0
                .destination_spelling_at(name, position.offset(), |local| {
                    resolution.allows_injected_import(position, local)
                })
        }
    }

    pub(super) fn round_arguments<'a>(
        &self,
        call: &'a ExprCall,
        site: &Site<'_>,
    ) -> Option<[Option<&'a Expr>; 2]> {
        if self.resolves(&call.func, "round", site) {
            bind(call, ["number", "ndigits"], 0, 2, 1)
        } else {
            None
        }
    }

    /// Admission is at the insertion point. Existing callee grouping stays
    /// outside the replacement and can put the new import at the parser limit.
    pub(super) fn import_member(
        &self,
        imports: &CallableImports,
        module: &str,
        member: &str,
        anchor: &Expr,
        site: &Site<'_>,
    ) -> Option<String> {
        imports.destination_spelling(member, anchor).or_else(|| {
            (self.ordinary_namespace()
                && imports.import_builtin_available()
                && site.delimiters.room_before(anchor.start(), 1))
            .then(|| format!("__import__('{module}').{member}"))
        })
    }
}

/// A single unparenthesized generator borrows the call's own delimiters.
pub(super) fn implicit_generator_interior(call: &ExprCall) -> Option<TextRange> {
    let [Expr::Generator(generator)] = call.arguments.args.as_ref() else {
        return None;
    };
    let args = call.arguments.range();
    (!generator.parenthesized).then(|| {
        TextRange::new(
            args.start() + TextSize::new(1),
            args.end() - TextSize::new(1),
        )
    })
}

/// Bind a fixed signature in supplied-argument order. The upfront arity
/// guard bounds all work even for calls with thousands of supplied values.
pub(super) fn bind<'a, const N: usize>(
    call: &'a ExprCall,
    names: [&str; N],
    positional_only: usize,
    positional: usize,
    required: usize,
) -> Option<[Option<&'a Expr>; N]> {
    if call.arguments.len() > N || call.arguments.args.len() > positional {
        return None;
    }
    let mut slots = [None; N];
    for (index, value) in call.arguments.args.iter().enumerate() {
        if value.is_starred_expr() {
            return None;
        }
        slots[index] = Some(value);
    }
    for keyword in &call.arguments.keywords {
        let name = keyword.arg.as_ref()?.as_str();
        let index = names.iter().position(|&expected| name == expected)?;
        if index < positional_only || slots[index].replace(&keyword.value).is_some() {
            return None;
        }
    }
    slots[..required]
        .iter()
        .all(Option::is_some)
        .then_some(slots)
}
