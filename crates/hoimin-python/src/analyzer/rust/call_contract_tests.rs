use super::*;
use rstest::rstest;

fn candidates(source: &str, op: &str) -> Vec<String> {
    let operators = MutationOperatorSelection::default();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("contract.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 512,
    };
    analyze_source(&request, source)
        .candidates
        .into_iter()
        .filter(|c| c.operator.as_str() == op)
        .map(|c| c.replacement.clone())
        .collect()
}

#[rstest]
#[case::regex_alias("from re import escape as e\nout=e(b'.')", "stdlib_escape_remove", 1)]
#[case::html_keyword_order(
    "import html\nout=html.escape(quote=option(),s=text())",
    "stdlib_escape_remove",
    1
)]
#[case::url_dotted(
    "import urllib.parse\nout=urllib.parse.quote('a b')",
    "stdlib_escape_remove",
    1
)]
#[case::url_alias(
    "import urllib.parse as p\nout=p.quote_plus('a b')",
    "stdlib_escape_remove",
    1
)]
#[case::from_package(
    "from urllib import parse as p\nout=p.quote('a b')",
    "stdlib_escape_remove",
    1
)]
#[case::from_member(
    "from urllib.parse import quote as q\ndef f(s: str): return q(s)",
    "stdlib_escape_remove",
    1
)]
#[case::positional("factory().send(left(),right(),third())", "call_argument_swap", 2)]
#[case::variadic("def f(*args): pass\nf(left(),right())", "call_argument_swap", 1)]
#[case::keyword(
    "def f(*,left,right): pass\nf(right=a(),left=b())",
    "call_argument_swap",
    1
)]
#[case::mixed("def f(left,*,right): pass\nf(a(),right=b())", "call_argument_swap", 1)]
#[case::equal_calls("f(pop(),pop())", "call_argument_swap", 1)]
#[case::synchronous("def f()->int: return 7\nout=f()", "call_result_default", 1)]
#[case::awaited(
    "async def f()->list[int]: return [7]\nasync def use(): return await f()",
    "call_result_default",
    1
)]
#[case::spread_result(
    "def f(*args,**kw)->int: return 7\nout=f(*xs,**ys)",
    "call_result_default",
    1
)]
#[case::builtin("out=len(xs)", "call_result_default", 1)]
#[case::nested_consumed("def f()->int: return 1\nuse(f())", "call_result_default", 1)]
#[case::relative("from .re import escape\nout=escape('.')", "stdlib_escape_remove", 0)]
#[case::url_bytes(
    "import urllib.parse as p\nout=p.quote(b'.')",
    "stdlib_escape_remove",
    0
)]
#[case::wrong_path(
    "import urllib.parse\nout=urllib.quote('a')",
    "stdlib_escape_remove",
    0
)]
#[case::module_mutated(
    "import urllib.parse as p\nimport urllib.parse\nurllib.parse.quote=mine\nout=p.quote('a')",
    "stdlib_escape_remove",
    0
)]
#[case::submodule_escaped(
    "import urllib.parse\nexpose(urllib.parse)\nout=urllib.parse.quote('a')",
    "stdlib_escape_remove",
    0
)]
#[case::html_bytes("import html\nout=html.escape(b'a')", "stdlib_escape_remove", 0)]
#[case::spread_escape("import re\nout=re.escape(*args)", "stdlib_escape_remove", 0)]
#[case::unknown_keyword("f(a=x,b=y)", "call_argument_swap", 0)]
#[case::posonly("def f(a,/,b): pass\nf(x,b=y)", "call_argument_swap", 0)]
#[case::spread_swap("f(a,b,*more)", "call_argument_swap", 0)]
#[case::invalid_binding("def f(a,b): pass\nf(1,2,a=3)", "call_argument_swap", 0)]
#[case::equal_values("f(1,0x1)", "call_argument_swap", 0)]
#[case::discarded("def f()->int: return 1\nf()", "call_result_default", 0)]
#[case::unawaited("async def f()->int: return 1\nout=f()", "call_result_default", 0)]
#[case::generator("def f()->int: yield 1\nout=f()", "call_result_default", 0)]
#[case::async_generator(
    "async def f()->int: yield 1\nasync def use(): return await f()",
    "call_result_default",
    0
)]
#[case::rebound("int=mine\ndef f()->int: return 7\nout=f()", "call_result_default", 0)]
#[case::url_annotated_parameter(
    "import urllib.parse\ndef f(text: str): return urllib.parse.quote(text)",
    "stdlib_escape_remove",
    1
)]
#[case::url_overwritten_parameter(
    "from urllib.parse import quote\ndef f(text: str):\n text=b'bytes'\n return quote(text)",
    "stdlib_escape_remove",
    0
)]
#[case::url_nonlocal_write(
    "from urllib.parse import quote\ndef f(text: str):\n def set_value():\n  nonlocal text\n  text=b'bytes'\n set_value()\n return quote(text)",
    "stdlib_escape_remove",
    0
)]
#[case::package_loaded_by_other_import(
    "import urllib as u\nfrom urllib.parse import quote as q\nout=u.parse.quote('a')",
    "stdlib_escape_remove",
    1
)]
#[case::isolated_invalidation(
    "import html, re\nexpose(re)\nout=html.escape('&')",
    "stdlib_escape_remove",
    1
)]
#[case::callable_alias_escaped(
    "from html import escape as e\nexpose(e)\nout=e('&')",
    "stdlib_escape_remove",
    0
)]
#[case::generator_argument(
    "from re import escape\nout=escape(x for x in xs)",
    "stdlib_escape_remove",
    0
)]
#[case::discarded_boolean_tail("def f()->int: return 1\nready and f()", "call_result_default", 0)]
#[case::consumed_boolean_test("def f()->int: return 1\nf() and ready", "call_result_default", 1)]
#[case::discarded_conditional_branches(
    "def f()->int: return 1\nf() if ready else f()",
    "call_result_default",
    0
)]
#[case::discarded_await(
    "async def f()->int: return 1\nasync def g(): await f()",
    "call_result_default",
    0
)]
#[case::shadowed_builtin("def f(len): return len(xs)", "call_result_default", 0)]
#[case::replaced_builtin_namespace_member(
    "import builtins as b\nb.len=custom\nout=len(xs)",
    "call_result_default",
    0
)]
#[case::builtin_sorted_keywords("out=sorted(xs, key=f, reverse=True)", "call_argument_swap", 1)]
#[case::forward_typed_function(
    "def use(): return f()\ndef f()->int: return 1",
    "call_result_default",
    1
)]
#[case::recursive_typed_function(
    "def f(n)->int:\n if n == 0: return 1\n return f(n-1)",
    "call_result_default",
    1
)]
#[case::forward_keyword_roles(
    "def use(): return f(a=1,b=2)\ndef f(*,a,b): return a,b",
    "call_argument_swap",
    1
)]
#[case::forward_may_be_builtin(
    "def use(): return open()\ndef open()->int: return 1",
    "call_result_default",
    0
)]
#[case::changed_module_namespace(
    "import sys\ndef f(*,a,b): return a,b\nsys.modules[__name__].f=override\nf(a=1,b=2)",
    "call_argument_swap",
    0
)]
#[case::negative_equivalent_literal("f(-1,-0x1)", "call_argument_swap", 0)]
#[case::float_sign_distinction("f(-0.0,0.0)", "call_argument_swap", 1)]
#[case::consumed_subscript_target("def f()->list: return [1]\nf()[0]=5", "call_result_default", 1)]
#[case::function_globals_rewrites_regex_callable(
    "import re\nre.escape.__globals__.update(escape=lambda s:'custom')\nout=re.escape('.')",
    "stdlib_escape_remove",
    0
)]
#[case::another_function_globals_rewrites_typed_callable(
    "def f()->int: return 8\ndef change(): pass\nchange.__globals__.update(f=lambda:'custom')\nout=f()",
    "call_result_default",
    0
)]
#[case::regex_flag_is_safe(
    "import re\nout=re.compile(re.escape(text), re.IGNORECASE)",
    "stdlib_escape_remove",
    1
)]
#[case::regex_flag_alias_is_safe(
    "import re\nfrom re import IGNORECASE as mode\nout=re.compile(re.escape(text), mode)",
    "stdlib_escape_remove",
    1
)]
#[case::unknown_module_member_can_escape(
    "import re\nexpose(re.RegexFlag)\nout=re.escape('.')",
    "stdlib_escape_remove",
    0
)]
#[case::constant_store_invalidates_module(
    "import re\nre.IGNORECASE=override\nout=re.escape('.')",
    "stdlib_escape_remove",
    0
)]
#[case::variadic_arguments_keep_keyword_only_roles(
    "def f(a,*rest,option,mode): pass\nf(1,2,3,option=4,mode=5)",
    "call_argument_swap",
    3
)]
#[case::variadic_arguments_cannot_supply_required_keyword_only_roles(
    "def f(a,*rest,option): pass\nf(1,2,3)",
    "call_argument_swap",
    0
)]
fn eligible_sites(#[case] source: &str, #[case] op: &str, #[case] expected: usize) {
    let edits = candidates(source, op);

    assert_eq!(edits.len(), expected, "{edits:?}");
}

use proptest::prelude::*;

fn analysis(source: &str, op: MutationOperator, budget: usize) -> AnalyzerOutput {
    let mut operators = MutationOperatorSelection::default();
    for configured in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(configured).unwrap() {
            operators.exclude(operator);
        }
    }
    operators.include(op);
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("contract.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: budget,
    };
    analyze_source(&request, source)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]
    #[test]
    fn grouped_adjacent_arguments_compile_and_keep_all_outside_bytes(
        first in 1u8..50,
        third in 1u8..50,
    ) {
        let source = format!("def 対象():\r\n    return f(({first}), # first\r\n             (90), ({third})) # end\r\n");

        let output = analysis(&source, MutationOperator::CallArgumentSwap, 10);

        prop_assert_eq!(output.candidates.len(), 2);
        for candidate in output.candidates {
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            let mut changed = source.clone();
            changed.replace_range(start..end, &candidate.replacement);
            prop_assert!(ruff_python_parser::parse_module(&changed).is_ok(), "{}", changed);
            prop_assert_eq!(&changed[..start], &source[..start]);
            prop_assert_eq!(&changed[(start + candidate.replacement.len())..], &source[end..]);
        }
    }
}

#[rstest]
#[case::zero(0)]
#[case::one(1)]
#[case::two(2)]
#[case::four(4)]
fn wide_calls_follow_analyzer_prefix_admission(#[case] limit: usize) {
    let values = (0..64)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("f({values})");
    let unlimited = analysis(&source, MutationOperator::CallArgumentSwap, 64);

    let bounded = analysis(&source, MutationOperator::CallArgumentSwap, limit);

    assert_eq!(unlimited.candidates.len(), 63);
    let spans = |candidates: &[Charged<AnalyzerCandidate>]| {
        candidates
            .iter()
            .map(|c| (c.span.start, c.span.length))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        spans(&bounded.candidates),
        spans(&unlimited.candidates[..limit])
    );
}

thread_local! {
    static SIGNATURE_BINDINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn record_signature_binding() {
    SIGNATURE_BINDINGS.update(|count| count + 1);
}

#[rstest]
#[case::outside_selected_arguments(1, 0)]
#[case::only_last_call_selected(97, 1)]
fn wide_signature_binding_waits_for_an_argument_candidate(
    #[case] selected_line: u32,
    #[case] expected: usize,
) {
    let parameters = (0..512)
        .map(|i| format!("p{i}=0"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("def f(a,b,{parameters}): pass\n{}", "f(1,2)\n".repeat(96));
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        operators.exclude(MutationOperator::from_name(&name).unwrap());
    }
    operators.include(MutationOperator::CallArgumentSwap);
    let lines = [LineRange {
        start: selected_line,
        end: selected_line,
    }];
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("contract.py"),
        lines: &lines,
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 512,
    };
    SIGNATURE_BINDINGS.set(0);

    let output = analyze_source(&request, &source);

    assert_eq!(output.candidates.len(), expected);
    assert_eq!(SIGNATURE_BINDINGS.get(), expected);
}
#[rstest::rstest]
#[case::sibling_only("import urllib.response\nurllib.parse.quote(s)", false)]
#[case::aliased_sibling(
    "import urllib.response as r\nfrom urllib.parse import quote as q\nexpose(r)\nq(s)",
    true
)]
#[case::sibling_root_escape(
    "import urllib.response\nfrom urllib.parse import quote as q\nexpose(urllib)\nq(s)",
    false
)]
fn sibling_package_identity(#[case] source: &str, #[case] trusted: bool) {
    // Arrange
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let module = parsed.syntax();
    let call = module
        .body
        .last()
        .unwrap()
        .as_expr_stmt()
        .unwrap()
        .value
        .as_call_expr()
        .unwrap();
    // Act
    let imports =
        callable_imports::CallableImports::build_family(module, &|| false, &["urllib.parse"])
            .unwrap();
    // Assert
    assert_eq!(imports.resolve(&call.func).is_some(), trusted);
}
