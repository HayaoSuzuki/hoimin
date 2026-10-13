//! Mutation destinations for trusted Python `operator` callables.
use super::{AnalysisCancelled, callable_imports::CallableImports};
use ruff_python_ast::{Expr, ModModule};
use ruff_text_size::{Ranged, TextRange};

// Python 3.14 canonical names, documented aliases, and mutation destinations.
// An empty alias means there is no documented dunder form.
const FUNCTIONS: &[(&str, &str, &str)] = &[
    ("eq", "__eq__", "ne"),
    ("ne", "__ne__", "eq"),
    ("lt", "__lt__", "le"),
    ("le", "__le__", "lt"),
    ("gt", "__gt__", "ge"),
    ("ge", "__ge__", "gt"),
    ("add", "__add__", "sub"),
    ("sub", "__sub__", "add"),
    ("mul", "__mul__", "truediv"),
    ("truediv", "__truediv__", "mul"),
    ("floordiv", "__floordiv__", "mod"),
    ("mod", "__mod__", "floordiv"),
    ("pow", "__pow__", "mul"),
    ("matmul", "__matmul__", "mul"),
    ("and_", "__and__", "or_"),
    ("or_", "__or__", "and_"),
    ("lshift", "__lshift__", "rshift"),
    ("rshift", "__rshift__", "lshift"),
    ("xor", "__xor__", "and_"),
    ("neg", "__neg__", "pos"),
    ("pos", "__pos__", "neg"),
    ("abs", "__abs__", "neg"),
    ("index", "__index__", "pos"),
    ("inv", "__inv__", "pos"),
    ("invert", "__invert__", "pos"),
    ("not_", "__not__", "truth"),
    ("truth", "", "not_"),
    ("is_", "", "is_not"),
    ("is_not", "", "is_"),
    ("is_none", "", "is_not_none"),
    ("is_not_none", "", "is_none"),
    ("iadd", "__iadd__", "isub"),
    ("isub", "__isub__", "iadd"),
    ("imul", "__imul__", "itruediv"),
    ("itruediv", "__itruediv__", "imul"),
    ("ifloordiv", "__ifloordiv__", "imod"),
    ("imod", "__imod__", "ifloordiv"),
    ("ipow", "__ipow__", "imul"),
    ("imatmul", "__imatmul__", "imul"),
    ("iand", "__iand__", "ior"),
    ("ior", "__ior__", "iand"),
    ("ilshift", "__ilshift__", "irshift"),
    ("irshift", "__irshift__", "ilshift"),
    ("ixor", "__ixor__", "iand"),
    ("concat", "__concat__", "iconcat"),
    ("iconcat", "__iconcat__", "concat"),
    ("countOf", "", "indexOf"),
    ("indexOf", "", "countOf"),
    ("getitem", "__getitem__", "contains"),
    (
        "contains",
        "__contains__",
        "(lambda container, item, /: item not in container)",
    ),
    (
        "setitem",
        "__setitem__",
        "(lambda container, key, value, /: None)",
    ),
    ("delitem", "__delitem__", "(lambda container, key, /: None)"),
    (
        "call",
        "__call__",
        "(lambda target, /, *args, **kwargs: None)",
    ),
];

fn replacement(name: &str) -> Option<&'static str> {
    let &(canonical, _, destination) = FUNCTIONS.iter().find(|(canonical, alias, _)| {
        name == *canonical || (!alias.is_empty() && name == *alias)
    })?;
    if name != canonical
        && let Some((_, alias, _)) = FUNCTIONS.iter().find(|(name, _, _)| *name == destination)
        && !alias.is_empty()
    {
        return Some(alias);
    }
    Some(destination)
}

#[derive(Default)]
pub(super) struct OperatorImports {
    imports: CallableImports,
}

impl OperatorImports {
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        Ok(Self {
            imports: CallableImports::build(module, cancelled, &["operator"])?,
        })
    }

    pub(super) fn replacement(&self, expression: &Expr) -> Option<(TextRange, String)> {
        let resolved = self.imports.resolve(expression)?;
        let destination = replacement(resolved.member)?;
        if destination.starts_with('(') {
            Some((expression.range(), destination.to_owned()))
        } else if let Some(range) = resolved.member_range {
            Some((range, destination.to_owned()))
        } else if self.imports.import_builtin_available() {
            Some((
                expression.range(),
                format!("__import__('operator').{destination}"),
            ))
        } else {
            None
        }
    }
}
