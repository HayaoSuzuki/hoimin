//! Conservative same-module signatures and optional-keyword call edits.
use super::{AnalysisCancelled, NameResolutionIndex, deletion, source_text};
use ruff_python_ast::{
    Expr, ExprCall, ExprContext, ModModule, Stmt,
    visitor::{self, Visitor},
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct FunctionIndex {
    definitions: HashMap<String, Signature>,
    resolution: NameResolutionIndex,
}
struct Signature {
    range: TextRange,
    positional_only: usize,
    positional: usize,
    defaults: Vec<bool>,
    names: HashMap<String, usize>,
}
impl FunctionIndex {
    pub(super) fn build(
        module: &ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let mut definitions = HashMap::new();
        for statement in &module.body {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            let parameters = &function.parameters;
            if function.is_async
                || !function.decorator_list.is_empty()
                || function.type_params.is_some()
                || parameters.vararg.is_some()
                || parameters.kwarg.is_some()
            {
                continue;
            }
            let mut defaults = Vec::new();
            let mut names = HashMap::new();
            for parameter in parameters.iter_non_variadic_params() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                names.insert(parameter.parameter.name.to_string(), defaults.len());
                defaults.push(parameter.default.is_some());
            }
            definitions.insert(
                function.name.to_string(),
                Signature {
                    range: function.range(),
                    positional_only: parameters.posonlyargs.len(),
                    positional: parameters.posonlyargs.len() + parameters.args.len(),
                    defaults,
                    names,
                },
            );
        }
        if definitions.is_empty() {
            return Ok(Self::default());
        }
        let names = definitions.keys().cloned().collect::<HashSet<_>>();
        let resolution = NameResolutionIndex::from_module_with_names(module, names.clone());
        let mut hazards = Hazards {
            names: &names,
            invalid: HashSet::new(),
            dynamic: false,
            cancelled,
            stopped: false,
        };
        hazards.visit_body(&module.body);
        if hazards.stopped || cancelled() {
            return Err(AnalysisCancelled);
        }
        definitions.retain(|name, _| {
            !hazards.dynamic
                && !hazards.invalid.contains(name)
                && resolution.unique_module_binding(name)
        });
        Ok(Self {
            definitions,
            resolution,
        })
    }

    pub(super) fn optional_keywords(
        &self,
        call: &ExprCall,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Vec<usize>> {
        let Expr::Name(name) = call.func.as_ref() else {
            return None;
        };
        let signature = self.definitions.get(name.id.as_str())?;
        if call.start() < signature.range.end()
            || !self
                .resolution
                .resolves_unique_module(&call.func, name.id.as_str())
            || call.arguments.args.len() > signature.positional
            || call.arguments.args.iter().any(Expr::is_starred_expr)
        {
            return None;
        }
        let mut assigned = vec![false; signature.defaults.len()];
        for present in assigned.iter_mut().take(call.arguments.args.len()) {
            *present = true;
        }
        let mut optional = Vec::new();
        for (index, keyword) in call.arguments.keywords.iter().enumerate() {
            if cancelled() {
                return None;
            }
            let name = keyword.arg.as_ref()?;
            let position = *signature.names.get(name.as_str())?;
            if position < signature.positional_only || assigned[position] {
                return None;
            }
            assigned[position] = true;
            if signature.defaults[position] {
                optional.push(index);
            }
        }
        if assigned
            .iter()
            .zip(&signature.defaults)
            .any(|(provided, default)| !provided && !default)
        {
            return None;
        }
        Some(optional)
    }
}

struct Hazards<'a, F> {
    names: &'a HashSet<String>,
    invalid: HashSet<String>,
    dynamic: bool,
    cancelled: &'a F,
    stopped: bool,
}
impl<'ast, F: Fn() -> bool> Visitor<'ast> for Hazards<'_, F> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        self.stopped |= (self.cancelled)();
        if self.stopped {
            return;
        }
        if let Stmt::Global(global) = statement {
            for name in &global.names {
                if self.names.contains(name.as_str()) {
                    self.invalid.insert(name.to_string());
                }
            }
        }
        visitor::walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        self.stopped |= (self.cancelled)();
        if self.stopped {
            return;
        }
        if let Expr::Name(name) = expression {
            if name.ctx == ExprContext::Load && self.names.contains(name.id.as_str()) {
                self.invalid.insert(name.id.to_string());
            }
            if matches!(
                name.id.as_str(),
                "exec"
                    | "eval"
                    | "globals"
                    | "locals"
                    | "vars"
                    | "setattr"
                    | "delattr"
                    | "__import__"
            ) {
                self.dynamic = true;
            }
        }
        // Only a direct callee load proves this function object has not escaped.
        if let Expr::Call(call) = expression
            && matches!(call.func.as_ref(), Expr::Name(n) if self.names.contains(n.id.as_str()))
        {
            self.visit_arguments(&call.arguments);
        } else {
            visitor::walk_expr(self, expression);
        }
    }
}

pub(super) fn emit(
    call: &ExprCall,
    optional: &[usize],
    source: &str,
    limit: usize,
    cancelled: &impl Fn() -> bool,
    mut candidate: impl FnMut(String),
) {
    let Some(callee) = source_text(source, call.func.range()) else {
        return;
    };
    let mut positional = Vec::new();
    for argument in &call.arguments.args {
        if cancelled() {
            return;
        }
        let Some(text) = source_text(source, argument.range()) else {
            return;
        };
        positional.push(format!("({text}),"));
    }
    let mut keywords = Vec::new();
    for keyword in &call.arguments.keywords {
        if cancelled() {
            return;
        }
        let Some(name) = keyword
            .arg
            .as_ref()
            .and_then(|name| source_text(source, name.range()))
        else {
            return;
        };
        let Some(value) = source_text(source, keyword.value.range()) else {
            return;
        };
        keywords.push(format!("{name}=({value}),"));
    }
    let mut emitted = 0usize;
    for &removed in optional {
        if cancelled() {
            return;
        }
        if !deletion::can_remove(&call.arguments.keywords[removed].value, cancelled) {
            continue;
        }
        if emitted > limit {
            break;
        }
        emitted += 1;
        let mut replacement = format!("({callee}(");
        for argument in &positional {
            if cancelled() {
                return;
            }
            replacement.push_str(argument);
        }
        for (index, keyword) in keywords.iter().enumerate() {
            if cancelled() {
                return;
            }
            if index != removed {
                replacement.push_str(keyword);
            }
        }
        replacement.push_str("))");
        candidate(replacement);
    }
}
