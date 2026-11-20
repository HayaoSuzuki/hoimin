//! Conservative same-module signatures and optional-keyword call edits.
use super::{AnalysisCancelled, NameResolutionIndex, deletion, source_text};
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, ExprContext, ModModule, Stmt, StmtFunctionDef};
use ruff_text_size::Ranged;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

#[derive(Default)]
pub(super) struct FunctionIndex<'ast> {
    definitions: HashMap<String, Signature<'ast>>,
    resolution: NameResolutionIndex,
    optional_namespace_dynamic: bool,
}

pub(super) struct Signature<'ast> {
    pub(super) definition: &'ast StmtFunctionDef,
    positional_only: usize,
    positional: usize,
    required_positional: usize,
    required: usize,
    defaults: Vec<bool>,
    names: HashMap<String, usize>,
}

impl<'ast> FunctionIndex<'ast> {
    #[cfg(test)]
    pub(super) fn build(
        module: &'ast ModModule,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        Self::build_for_calls(module, false, cancelled)
    }

    pub(super) fn build_for_calls(
        module: &'ast ModModule,
        include_other_signatures: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let mut definitions = HashMap::new();
        let mut optional_dynamic_names = HashSet::new();
        for statement in &module.body {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            if !function.decorator_list.is_empty()
                || function.type_params.is_some()
                || (!include_other_signatures && !optional_signature(function))
            {
                continue;
            }
            if dynamic_name(function.name.as_str()) && optional_signature(function) {
                optional_dynamic_names.insert(function.name.to_string());
            }
            let parameters = &function.parameters;
            let positional_only = parameters.posonlyargs.len();
            let positional = positional_only + parameters.args.len();
            let mut required = 0;
            let mut required_positional = 0;
            let mut defaults = Vec::new();
            let mut names = HashMap::new();
            for parameter in parameters.iter_non_variadic_params() {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                let has_default = parameter.default.is_some();
                required += usize::from(!has_default);
                required_positional += usize::from(!has_default && defaults.len() < positional);
                names.insert(parameter.parameter.name.to_string(), defaults.len());
                defaults.push(has_default);
            }
            definitions.insert(
                function.name.to_string(),
                Signature {
                    definition: function,
                    positional_only,
                    positional,
                    required_positional,
                    required,
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
            optional_dynamic_names: &optional_dynamic_names,
            optional_namespace_dynamic: false,
            cancelled,
            stopped: false,
        };
        hazards.visit_body(&module.body);
        if hazards.stopped || cancelled() {
            return Err(AnalysisCancelled);
        }
        let optional_namespace_dynamic = hazards.optional_namespace_dynamic;
        definitions.retain(|name, _| {
            !hazards.dynamic
                && !hazards.invalid.contains(name)
                && resolution.unique_module_binding(name)
        });
        Ok(Self {
            definitions,
            resolution,
            optional_namespace_dynamic,
        })
    }

    pub(super) fn signature(&self, call: &ExprCall) -> Option<&Signature<'ast>> {
        let Expr::Name(name) = call.func.as_ref() else {
            return None;
        };
        let signature = self.definitions.get(name.id.as_str())?;
        let id = name.id.as_str();
        if !self.resolution.resolves_unique_module(&call.func, id) {
            return None;
        }
        let definition = signature.definition;
        let in_own_body = definition
            .body
            .first()
            .is_some_and(|first| first.start() <= call.start() && call.end() <= definition.end());
        let forward_reference =
            self.resolution.is_function_body_lookup(&call.func) && !can_fall_back_to_builtin(id);
        (call.start() >= definition.end() || in_own_body || forward_reference).then_some(signature)
    }

    pub(super) fn optional_keywords(
        &self,
        call: &ExprCall,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Vec<usize>> {
        #[cfg(test)]
        super::optional_keyword_selection_tests::record_lookup();
        if self.optional_namespace_dynamic {
            return None;
        }
        let signature = self.signature(call)?;
        if call.start() < signature.definition.end() || !optional_signature(signature.definition) {
            return None;
        }
        let slots = signature.bind_explicit(call, cancelled, || {
            #[cfg(test)]
            super::optional_keyword_selection_tests::record_keyword();
        })?;
        Some(
            slots
                .into_iter()
                .enumerate()
                .filter_map(|(index, slot)| signature.defaults[slot?].then_some(index))
                .collect(),
        )
    }
}

fn optional_signature(function: &StmtFunctionDef) -> bool {
    !function.is_async
        && function.parameters.vararg.is_none()
        && function.parameters.kwarg.is_none()
}

// An earlier function body can load a later unique module definition. Before
// that definition runs, Python searches builtins. Exclude possible builtin
// names so a successful early call cannot have a different contract.
fn can_fall_back_to_builtin(name: &str) -> bool {
    name.starts_with('_')
        || name.starts_with(char::is_uppercase)
        || matches!(
            name,
            "abs"
                | "aiter"
                | "all"
                | "anext"
                | "any"
                | "ascii"
                | "bin"
                | "bool"
                | "breakpoint"
                | "bytearray"
                | "bytes"
                | "callable"
                | "chr"
                | "classmethod"
                | "compile"
                | "complex"
                | "copyright"
                | "credits"
                | "delattr"
                | "dict"
                | "dir"
                | "divmod"
                | "enumerate"
                | "eval"
                | "exec"
                | "exit"
                | "filter"
                | "float"
                | "format"
                | "frozenset"
                | "getattr"
                | "globals"
                | "hasattr"
                | "hash"
                | "help"
                | "hex"
                | "id"
                | "input"
                | "int"
                | "isinstance"
                | "issubclass"
                | "iter"
                | "len"
                | "license"
                | "list"
                | "locals"
                | "map"
                | "max"
                | "memoryview"
                | "min"
                | "next"
                | "object"
                | "oct"
                | "open"
                | "ord"
                | "pow"
                | "print"
                | "property"
                | "quit"
                | "range"
                | "repr"
                | "reversed"
                | "round"
                | "set"
                | "setattr"
                | "slice"
                | "sorted"
                | "staticmethod"
                | "str"
                | "sum"
                | "super"
                | "tuple"
                | "type"
                | "vars"
                | "zip"
        )
}

impl Signature<'_> {
    /// A slot of None is an extra keyword accepted by the variadic keyword
    /// parameter. Expanded arguments cannot establish the complete binding.
    pub(super) fn bind_explicit(
        &self,
        call: &ExprCall,
        cancelled: &impl Fn() -> bool,
        mut visit_keyword: impl FnMut(),
    ) -> Option<Vec<Option<usize>>> {
        #[cfg(test)]
        super::call_contract_tests::record_signature_binding();
        let positional = call.arguments.args.len();
        let parameters = &self.definition.parameters;
        if positional > self.positional && parameters.vararg.is_none() {
            return None;
        }
        for argument in &call.arguments.args {
            if cancelled() || argument.is_starred_expr() {
                return None;
            }
        }
        // Python's required positional formals precede the optional ones.
        // Each unique keyword can cover at most one more required formal.
        let bound_positionally = positional.min(self.positional);
        let mut supplied_required = bound_positionally.min(self.required_positional);
        let mut names = HashSet::new();
        let mut keyword_slots = Vec::new();
        for keyword in &call.arguments.keywords {
            visit_keyword();
            if cancelled() {
                return None;
            }
            let name = keyword.arg.as_ref()?.as_str();
            if !names.insert(name) {
                return None;
            }
            let slot = self
                .names
                .get(name)
                .copied()
                .filter(|position| *position >= self.positional_only);
            if let Some(slot) = slot {
                if slot < bound_positionally {
                    return None;
                }
                supplied_required += usize::from(!self.defaults[slot]);
            } else if parameters.kwarg.is_none() {
                return None;
            }
            keyword_slots.push(slot);
        }
        (supplied_required == self.required && !cancelled()).then_some(keyword_slots)
    }

    pub(super) fn positional_keyword_name(&self, index: usize) -> Option<&str> {
        let index = index.checked_sub(self.positional_only)?;
        Some(
            self.definition
                .parameters
                .args
                .get(index)?
                .parameter
                .name
                .as_str(),
        )
    }
}

struct Hazards<'a, F> {
    names: &'a HashSet<String>,
    invalid: HashSet<String>,
    dynamic: bool,
    optional_dynamic_names: &'a HashSet<String>,
    optional_namespace_dynamic: bool,
    cancelled: &'a F,
    stopped: bool,
}
fn dynamic_name(name: &str) -> bool {
    matches!(
        name,
        "exec" | "eval" | "globals" | "locals" | "vars" | "setattr" | "delattr" | "__import__"
    )
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
            if dynamic_name(name.id.as_str()) {
                self.dynamic = true;
            }
        } else if let Expr::Attribute(attribute) = expression
            && attribute.attr.as_str() == "__globals__"
        {
            // A different function can expose the same module dictionary.
            self.dynamic = true;
        }
        // Only a direct callee load proves this function object has not escaped.
        if let Expr::Call(call) = expression
            && let Expr::Name(name) = call.func.as_ref()
            && self.names.contains(name.id.as_str())
        {
            if dynamic_name(name.id.as_str())
                && !self.optional_dynamic_names.contains(name.id.as_str())
            {
                self.optional_namespace_dynamic = true;
            }
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
    mut candidate: impl FnMut(String) -> bool,
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
        positional.push(text);
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
        keywords.push((name, value));
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
            write!(replacement, "({argument}),").unwrap();
        }
        for (index, keyword) in keywords.iter().enumerate() {
            if cancelled() {
                return;
            }
            if index != removed {
                write!(replacement, "{}=({}),", keyword.0, keyword.1).unwrap();
            }
        }
        replacement.push_str("))");
        #[cfg(test)]
        super::optional_keyword_selection_tests::record_replacement(&replacement);
        if !candidate(replacement) {
            return;
        }
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::cell::Cell;

    fn run_emit(
        call: &ExprCall,
        optional: &[usize],
        source: &str,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
        sink: &mut dyn FnMut(String) -> bool,
    ) {
        emit(call, optional, source, limit, &cancelled, sink);
    }

    #[rstest::rstest]
    #[case(0, 0)]
    #[case(1, 0)]
    #[case(2, 0)]
    #[case(3, 0)]
    #[case(4, 0)]
    #[case(5, 0)]
    #[case(6, 0)]
    #[case(7, 0)]
    #[case(8, 1)]
    #[case(9, 1)]
    #[case(10, 1)]
    #[case(11, 1)]
    #[case(12, 1)]
    #[case(13, 2)]
    fn cancellation_emits_only_complete_optional_keyword_replacements(
        #[case] checkpoint: usize,
        #[case] expected_count: usize,
    ) {
        let source = "function(1, option=2, other=3)";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let call = parsed.syntax().body.as_call_expr().unwrap();
        let polls = Cell::new(0);
        let mut replacements = Vec::new();
        run_emit(
            call,
            &[0, 1],
            source,
            100,
            &|| {
                let current = polls.get();
                polls.set(current + 1);
                current >= checkpoint
            },
            &mut |replacement| {
                replacements.push(replacement);
                true
            },
        );

        let actual = replacements.len();

        assert_eq!(actual, expected_count);
        assert_eq!(
            replacements,
            ["(function((1),other=(3),))", "(function((1),option=(2),))"][..expected_count]
        );
    }

    #[rstest::rstest]
    #[case("function(1, option=2)", "", vec![0], vec![])]
    #[case("function(1, option=2)", "function", vec![0], vec![])]
    #[case("function(1, option=2)", "function(1, opt", vec![0], vec![])]
    #[case("function(1, option=2)", "function(1, option=", vec![0], vec![])]
    #[case("function(1, option=2)", "function(1, option=2)", vec![0], vec!["(function((1),))"])]
    #[case("function(**options)", "function(**options)", vec![], vec![])]
    #[case("function(option=((side := 2), 3))", "function(option=((side := 2), 3))", vec![0], vec![])]
    fn emitter_requires_complete_named_keyword_source_ranges(
        #[case] parsed_source: &str,
        #[case] source: &str,
        #[case]
        #[by_ref]
        optional: &[usize],
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let parsed = ruff_python_parser::parse_expression(parsed_source).unwrap();
        let call = parsed.syntax().body.as_call_expr().unwrap();
        let mut replacements = Vec::new();
        run_emit(call, optional, source, 100, &|| false, &mut |replacement| {
            replacements.push(replacement);
            true
        });

        assert_eq!(replacements, expected);
    }

    #[test]
    fn rejected_optional_keyword_replacement_stops_later_candidates() {
        let source = "function(option=2, other=3)";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let call = parsed.syntax().body.as_call_expr().unwrap();
        let mut replacements = Vec::new();
        run_emit(call, &[0, 1], source, 100, &|| false, &mut |replacement| {
            replacements.push(replacement);
            false
        });

        assert_eq!(replacements, vec!["(function(other=(3),))"]);
    }
    fn run_build<'ast>(
        module: &'ast ModModule,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<FunctionIndex<'ast>, AnalysisCancelled> {
        FunctionIndex::build(module, &cancelled)
    }

    fn run_lookup(
        index: &FunctionIndex<'_>,
        call: &ExprCall,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<usize>> {
        index.optional_keywords(call, &cancelled)
    }

    #[rstest::rstest]
    #[case("required, optional=2", "function(1, optional=2)", Some(&[0][..]))]
    #[case("required, optional=2", "function(required=1, optional=2)", Some(&[1][..]))]
    #[case("required, optional=2", "function(1, 2)", Some(&[][..]))]
    #[case("required, optional=2", "function(optional=2)", None)]
    #[case("required, optional=2", "function(1, required=2)", None)]
    #[case("required, optional=2", "function(1, unknown=2)", None)]
    #[case("required, optional=2", "function(1, **options)", None)]
    #[case("required, optional=2", "function(*values, optional=2)", None)]
    #[case("required, optional=2", "function(1, 2, 3, optional=4)", None)]
    #[case("required, optional=2", "holder.function(1, optional=2)", None)]
    #[case("required, /, optional=2", "function(required=1, optional=2)", None)]
    #[case("required, *, optional=2", "function(1, optional=2)", Some(&[0][..]))]
    #[case("required, optional", "function(1, optional=2)", Some(&[][..]))]
    #[case("required=0, optional=2", "function(optional=2)", Some(&[0][..]))]
    #[case("*values", "function(optional=2)", None)]
    #[case("**keywords", "function(optional=2)", None)]
    fn optional_keyword_positions_require_a_complete_unambiguous_argument_binding(
        #[case] parameters: &str,
        #[case] invocation: &str,
        #[case] expected: Option<&[usize]>,
    ) {
        let source = format!("def function({parameters}): pass\n{invocation}\n");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let index = run_build(parsed.syntax(), &|| false).unwrap();
        let call = parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();

        let actual = run_lookup(&index, call, &|| false);

        assert_eq!(actual.as_deref(), expected);
    }

    #[rstest::rstest]
    #[case("global function", None)]
    #[case("global unrelated", Some(&[0][..]))]
    #[case("alias = function", None)]
    #[case("function = replacement", None)]
    #[case("exec('pass')", None)]
    #[case("unrelated = 0", Some(&[0][..]))]
    #[case::function_globals_exposure(
        "def changer(): pass\nchanger.__globals__.update(function=override)",
        None
    )]
    fn optional_keyword_lookup_rejects_escaped_or_rebound_function_identities(
        #[case] intervening: &str,
        #[case] expected: Option<&[usize]>,
    ) {
        let source = format!(
            "def function(required, optional=2): pass\n{intervening}\nfunction(1, optional=2)\n"
        );
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let index = run_build(parsed.syntax(), &|| false).unwrap();
        let call = parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();

        let actual = run_lookup(&index, call, &|| false);

        assert_eq!(actual.as_deref(), expected);
    }

    #[rstest::rstest]
    fn signature_collection_observes_cancellation_during_parameters_and_hazards(
        #[values(
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32
        )]
        checkpoint: usize,
    ) {
        let parsed = ruff_python_parser::parse_module(
            "def function(required, optional=2): pass\nfunction(1, optional=2)\n",
        )
        .unwrap();
        let complete_polls = Cell::new(0);
        let _complete = run_build(parsed.syntax(), &|| {
            complete_polls.set(complete_polls.get() + 1);
            false
        })
        .unwrap();
        let baseline_polled = complete_polls.get() > 0;
        let polls = Cell::new(0);
        let result = run_build(parsed.syntax(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        let cancelled_failed = result.is_err();

        assert!(baseline_polled);
        assert_eq!(cancelled_failed, checkpoint < complete_polls.get());
    }

    #[rstest::rstest]
    #[case(true, None)]
    #[case(false, Some(&[0][..]))]
    fn optional_keyword_binding_observes_cancellation(
        #[case] cancelled: bool,
        #[case] expected: Option<&[usize]>,
    ) {
        let parsed = ruff_python_parser::parse_module(
            "def function(required, optional=2): pass\nfunction(1, optional=2)\n",
        )
        .unwrap();
        let index = run_build(parsed.syntax(), &|| false).unwrap();
        let call = parsed
            .syntax()
            .body
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();

        let actual = run_lookup(&index, call, &|| cancelled);

        assert_eq!(actual.as_deref(), expected);
    }

    #[rstest::rstest]
    #[case(0, vec!["(function((1),other=(3),))"])]
    #[case(1, vec!["(function((1),other=(3),))", "(function((1),option=(2),))"])]
    fn optional_keyword_emission_preserves_the_shared_prefix_bound(
        #[case] limit: usize,
        #[case]
        #[by_ref]
        expected: &[&str],
    ) {
        let source = "function(1, option=2, other=3)";
        let parsed = ruff_python_parser::parse_expression(source).unwrap();
        let call = parsed.syntax().body.as_call_expr().unwrap();
        let mut replacements = Vec::new();
        run_emit(
            call,
            &[0, 1],
            source,
            limit,
            &|| false,
            &mut |replacement| {
                replacements.push(replacement);
                true
            },
        );

        assert_eq!(replacements, expected);
    }
    #[rstest::rstest]
    #[case::narrow(1)]
    #[case::wide(512)]
    fn explicit_binding_work_depends_on_supplied_slots(#[case] defaults: usize) {
        let parameters = (0..defaults)
            .map(|i| format!("p{i}=0"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!("def f(required, optional=0, {parameters}): pass\nf(1, optional=2)");
        let parsed = ruff_python_parser::parse_module(&source).unwrap();
        let index = run_build(parsed.syntax(), &|| false).unwrap();
        let call = parsed.syntax().body[1]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let polls = Cell::new(0);

        let optional = run_lookup(&index, call, &|| {
            polls.update(|count| count + 1);
            false
        });

        assert_eq!(optional, Some(vec![0]));
        assert!(
            polls.get() <= 4,
            "{} binding polls for two supplied arguments",
            polls.get()
        );
    }
}
