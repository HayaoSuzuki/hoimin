//! Conservative same-module signatures and optional-keyword call edits.
use super::{AnalysisCancelled, NameResolutionIndex, deletion, source_text};
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, ExprContext, ModModule, Stmt};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

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
        #[cfg(test)]
        super::optional_keyword_selection_tests::record_lookup();
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
            #[cfg(test)]
            super::optional_keyword_selection_tests::record_keyword();
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
        assert_eq!(replacements.len(), expected_count);
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
    fn run_build(
        module: &ModModule,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<FunctionIndex, AnalysisCancelled> {
        FunctionIndex::build(module, &cancelled)
    }

    fn run_lookup(
        index: &FunctionIndex,
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
        assert_eq!(run_lookup(&index, call, &|| false).as_deref(), expected);
    }

    #[rstest::rstest]
    #[case("global function", None)]
    #[case("global unrelated", Some(&[0][..]))]
    #[case("alias = function", None)]
    #[case("function = replacement", None)]
    #[case("exec('pass')", None)]
    #[case("unrelated = 0", Some(&[0][..]))]
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
        assert_eq!(run_lookup(&index, call, &|| false).as_deref(), expected);
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
        assert!(complete_polls.get() > 0);
        let polls = Cell::new(0);
        let result = run_build(parsed.syntax(), &|| {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        assert_eq!(result.is_err(), checkpoint < complete_polls.get());
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
        assert_eq!(run_lookup(&index, call, &|| cancelled).as_deref(), expected);
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
}
