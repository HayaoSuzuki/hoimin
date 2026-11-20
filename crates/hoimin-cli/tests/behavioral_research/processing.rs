use super::{assert_python_output, fs, plan, run_python};
use rstest::rstest;

const LIST_SELECTION: &str = concat!(
    "trace=[]\n",
    "def keep(x): trace.append(x); return x % 2 == 0\n",
    "out=[x for x in [0,2,3,2,1] if keep(x) if x > 0]\n",
);
const LAZY_SELECTION: &str = concat!(
    "trace=[]\n",
    "def keep(x): trace.append(x); return x % 2 == 0\n",
    "out=(x for x in [1,2,1,4] if keep(x))\n",
);
const LAZY_FILTER: &str = concat!(
    "trace=[]\n",
    "def inputs():\n for item in [0,2,0,4]:\n  trace.append(item)\n  yield item\n",
    "out=filter(None, inputs())\n",
);
const REVERSE_FILTER: &str = concat!(
    "from itertools import filterfalse as without\n",
    "trace=[]\n",
    "def check(x):\n trace.append(x)\n if x==3: raise ValueError('predicate')\n return x%2==0\n",
    "out=without(check, iter([1,2,3,4]))\n",
);
const SET_METHOD: &str = concat!(
    "trace=[]\n",
    "def operand(values): trace.append(values); return iter(values)\n",
    "def run():\n",
    " original=frozenset((1,2))\n",
    " return original.intersection(operand([2,3]), operand([2,4]))\n",
);
const SUM_ITERATOR: &str = concat!(
    "trace=[]\n",
    "def mark(x): trace.append(x); return x\n",
    "out=sum(mark(x) for x in [7,2,7])\n",
);

#[rstest]
#[case::negative_ndigits(
    "round_digits_remove",
    "out=round(-147,-1)\n",
    "(-1, None)[1]",
    "import subject as s; print(s.out)",
    "-150\n",
    "-147\n"
)]
#[case::nan_round_protocol(
    "round_digits_remove",
    "def run(): return round(float('nan'),2)\n",
    "(2, None)[1]",
    "import math, subject as s\ntry: print(math.isnan(s.run()))\nexcept ValueError as e: print(type(e).__name__)",
    "True\n",
    "ValueError\n"
)]
#[case::list_removal(
    "comprehension_filter_remove",
    LIST_SELECTION,
    "True",
    "import subject as s; print(s.out,s.trace)",
    "[2, 2] [0, 2, 3, 2, 1]\n",
    "[2, 3, 2, 1] []\n"
)]
#[case::async_dictionary(
    "comprehension_filter_invert",
    "async def inputs():\n for item in [2,-1,2,-3]: yield item\nasync def keep(x): return x > 0\nasync def run(): return {x: -x async for x in inputs() if await keep(x)}\n",
    "not (await keep(x))",
    "import asyncio, subject as s; print(asyncio.run(s.run()))",
    "{2: -2}\n",
    "{-1: 1, -3: 3}\n"
)]
#[case::generator_inversion(
    "comprehension_filter_invert",
    LAZY_SELECTION,
    "not (keep(x))",
    "import subject as s; print(s.trace); print(next(s.out),s.trace); print(list(s.out),s.trace)",
    "[]\n2 [1, 2]\n[4] [1, 2, 1, 4]\n",
    "[]\n1 [1]\n[1] [1, 2, 1, 4]\n"
)]
#[case::lazy_none_predicate(
    "filter_predicate_swap",
    LAZY_FILTER,
    "__import__('itertools').filterfalse",
    "import subject as s; print(s.trace); print(next(s.out),s.trace); print(list(s.out),s.trace)",
    "[]\n2 [0, 2]\n[4] [0, 2, 0, 4]\n",
    "[]\n0 [0]\n[0] [0, 2, 0, 4]\n"
)]
#[case::predicate_exception(
    "filter_predicate_swap",
    REVERSE_FILTER,
    "filter",
    "import subject as s\nprint(next(s.out))\ntry: list(s.out)\nexcept ValueError as e: print(str(e),s.trace)",
    "1\npredicate [1, 2, 3]\n",
    "2\npredicate [1, 2, 3]\n"
)]
#[case::exact_frozenset(
    "set_union_intersection_swap",
    SET_METHOD,
    "union",
    "import subject as s; value=s.run(); print(type(value).__name__,sorted(value),s.trace)",
    "frozenset [2] [[2, 3], [2, 4]]\n",
    "frozenset [1, 2, 3, 4] [[2, 3], [2, 4]]\n"
)]
#[case::generator_sum(
    "aggregate_sum_count",
    SUM_ITERATOR,
    "1 for _ in (mark(x) for x in [7,2,7])",
    "import subject as s; print(s.out,s.trace)",
    "16 [7, 2, 7]\n",
    "3 [7, 2, 7]\n"
)]
#[case::trailing_comma_sum(
    "aggregate_sum_count",
    "out=sum(iter([7,2]), # keep comma\n)\n",
    "(1 for _ in (iter([7,2])))",
    "import subject as s; print(s.out)",
    "9\n",
    "2\n"
)]
#[case::empty_sum(
    "aggregate_sum_count",
    "out=sum(iter(()))\n",
    "1 for _ in (iter(()))",
    "import subject as s; print(s.out)",
    "0\n",
    "0\n"
)]
#[case::count_only_literal_values(
    "aggregate_sum_count",
    "out=len([7,-2,7])\n",
    "sum",
    "import subject as s; print(s.out)",
    "3\n",
    "12\n"
)]
#[case::negative_tie(
    "rounding_function_swap",
    "out=round(-2.5)\n",
    "__import__('math').floor",
    "import subject as s; print(s.out)",
    "-2\n",
    "-3\n"
)]
#[case::nonfinite_uses_original_numeric_protocol(
    "rounding_function_swap",
    "from math import floor\n\ndef run(): return floor(float('inf'))\n",
    "round",
    "import subject as s\ntry: print(s.run())\nexcept Exception as e: print(type(e).__name__)",
    "OverflowError\n",
    "OverflowError\n"
)]
#[case::digits_preserve_evaluation_order(
    "round_digits_remove",
    "trace=[]\ndef mark(value): trace.append(value); return value\nout=round(ndigits=mark(1), number=mark(-0.04))\n",
    "(mark(1), None)[1]",
    "import subject as s; print(s.out,type(s.out).__name__,s.trace)",
    "-0.0 float [1, -0.04]\n",
    "0 int [1, -0.04]\n"
)]
#[tokio::test]
async fn collection_and_builtin_rounding_contract(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] replacement: &str,
    #[case] harness: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    assert_edit(operator, source, replacement, harness, before, after).await;
}

async fn assert_edit(
    operator: &str,
    source: &str,
    replacement: &str,
    harness: &str,
    before: &str,
    after: &str,
) {
    let root = tempfile::tempdir().unwrap();
    let subject = root.path().join("subject.py");
    fs::write(&subject, source).unwrap();

    let manifest = plan(root.path(), operator).await;
    let original = run_python(root.path(), harness);
    let candidates = manifest["candidates"].as_array().unwrap();
    let candidate = candidates
        .iter()
        .find(|candidate| candidate["replacement"] == replacement)
        .unwrap_or_else(|| panic!("expected replacement {replacement}: {manifest}"));
    let changed = apply_edit(source, candidate);
    fs::write(subject, changed).unwrap();
    let changed = run_python(root.path(), harness);

    assert_python_output(operator, "original", &original, before);
    assert_python_output(operator, "mutated", &changed, after);
}

fn apply_edit(source: &str, candidate: &serde_json::Value) -> String {
    let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
    assert_eq!(&source[start..end], candidate["original"].as_str().unwrap());
    let mut changed = source.to_owned();
    changed.replace_range(start..end, candidate["replacement"].as_str().unwrap());
    changed
}

const QUANTUM_SIGNALS: &str = concat!(
    "import decimal as d\n",
    "def run(inexact, rounded):\n",
    " with d.localcontext() as ctx:\n",
    "  value=d.Decimal('-0.0049').quantize(d.Decimal('.01'))\n",
    "  return str(value),value.as_tuple().sign,value.as_tuple().exponent,bool(ctx.flags[inexact]),bool(ctx.flags[rounded])\n",
);
const INTERMEDIATE_DECIMAL: &str = concat!(
    "import decimal as d\n",
    "trace=[]\n",
    "def option(): trace.append('context'); return d.getcontext()\n",
    "def mode(): trace.append('rounding'); return d.ROUND_HALF_EVEN\n",
    "def run(inexact, rounded):\n",
    " with d.localcontext() as ctx:\n",
    "  inner=(d.Decimal('2.45').quantize(d.Decimal('.1'),context=option(),rounding=mode()) * 2).quantize(d.Decimal('.1'))\n",
    "  return str(inner),bool(ctx.flags[inexact]),bool(ctx.flags[rounded]),trace\n",
);

#[rstest]
#[case::builtin_round_inside_decimal_quantize(
    "intermediate_rounding_remove",
    "from decimal import Decimal as D\nout=(round(D('2.45'),1)*3).quantize(D('.1'))\n",
    "(D('2.45'),1)[0]",
    "import subject as s; print(s.out)",
    "7.2\n",
    "7.4\n"
)]
#[case::decimal_sequence_display(
    "decimal_quantum_shift",
    "from decimal import Decimal as D\nout=D('2.456').quantize(D([0,[1],-2]))\n",
    "-3",
    "import subject as s; print(s.out)",
    "2.46\n",
    "2.456\n"
)]
#[case::context_tie_mode(
    "decimal_rounding_mode_swap",
    "from decimal import Context, ROUND_HALF_EVEN as bankers\nout=Context(prec=2,rounding=bankers).create_decimal('-2.45')\n",
    "__import__('decimal').ROUND_HALF_UP",
    "import subject as s; print(s.out)",
    "-2.4\n",
    "-2.5\n"
)]
#[case::explicit_mode_overrides_context(
    "decimal_rounding_mode_swap",
    "import decimal as d\nwith d.localcontext(rounding=d.ROUND_DOWN):\n out=d.Decimal('2.45').quantize(d.Decimal('.1'),rounding=d.ROUND_HALF_EVEN)\n",
    "ROUND_HALF_UP",
    "import subject as s; print(s.out)",
    "2.4\n",
    "2.5\n"
)]
#[case::scoped_assignment_rounding(
    "decimal_rounding_mode_swap",
    "import decimal as d\nwith d.localcontext() as current:\n current.rounding=d.ROUND_CEILING\n out=d.Decimal('-2.41').quantize(d.Decimal('.1'))\n",
    "ROUND_FLOOR",
    "import subject as s; print(s.out)",
    "-2.4\n",
    "-2.5\n"
)]
#[case::quantum_finer_and_signed_zero(
    "decimal_quantum_shift",
    QUANTUM_SIGNALS,
    "'1e-3'",
    "from decimal import Inexact,Rounded; import subject as s; print(s.run(Inexact,Rounded))",
    "('-0.00', 1, -2, True, True)\n",
    "('-0.005', 1, -3, True, True)\n"
)]
#[case::quantum_coarser_and_signed_zero(
    "decimal_quantum_shift",
    QUANTUM_SIGNALS,
    "'1e-1'",
    "from decimal import Inexact,Rounded; import subject as s; print(s.run(Inexact,Rounded))",
    "('-0.00', 1, -2, True, True)\n",
    "('-0.0', 1, -1, True, True)\n"
)]
#[case::quantum_trap(
    "decimal_quantum_shift",
    "import decimal as d\ndef run(inexact):\n with d.localcontext() as ctx:\n  ctx.traps[inexact]=True\n  return d.Decimal('2.45').quantize(d.Decimal('.01'))\n",
    "'1e-1'",
    "from decimal import Inexact; import subject as s\ntry: print(s.run(Inexact))\nexcept Exception as e: print(type(e).__name__)",
    "2.45\n",
    "Inexact\n"
)]
#[case::positionally_bound_context_precision(
    "decimal_context_precision",
    "from decimal import Context, Decimal\nctx=Context(5)\nout=ctx.multiply(Decimal('123'),Decimal('1'))\n",
    "1",
    "from decimal import Inexact; import subject as s; print(s.out,bool(s.ctx.flags[Inexact]))",
    "123 False\n",
    "1E+2 True\n"
)]
#[case::local_context_precision_restoration(
    "decimal_context_precision",
    "from decimal import Decimal, getcontext, localcontext\ndef run():\n before=getcontext().prec\n with localcontext(prec=4): value=Decimal('1234') / 7\n return before,str(value),getcontext().prec\n",
    "1",
    "import subject as s; print(s.run())",
    "(28, '176.3', 28)\n",
    "(28, '2E+2', 28)\n"
)]
#[case::intermediate_decimal_and_flags(
    "intermediate_rounding_remove",
    INTERMEDIATE_DECIMAL,
    "(d.Decimal('2.45'),d.Decimal('.1'),option(),mode())[0]",
    "from decimal import Inexact,Rounded; import subject as s; print(s.run(Inexact,Rounded))",
    "('4.8', True, True, ['context', 'rounding'])\n",
    "('4.9', False, True, ['context', 'rounding'])\n"
)]
#[case::intermediate_keyword_order(
    "intermediate_rounding_remove",
    "trace=[]\ndef mark(x): trace.append(x); return x\nout=round(round(ndigits=mark(1),number=mark(2.44))*3,1)\n",
    "(mark(1),mark(2.44))[1]",
    "import subject as s; print(s.out,s.trace)",
    "7.2 [1, 2.44]\n",
    "7.3 [1, 2.44]\n"
)]
#[tokio::test]
async fn decimal_and_intermediate_precision_contract(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] replacement: &str,
    #[case] harness: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    assert_edit(operator, source, replacement, harness, before, after).await;
}

#[rstest]
#[case::filter_selected("[x for x in xs if pred(x)]", "comprehension_filter_invert", 197, 1)]
#[case::filter_at_limit("[x for x in xs if pred(x)]", "comprehension_filter_invert", 198, 0)]
#[case::sum_selected("sum(xs)", "aggregate_sum_count", 198, 1)]
#[case::sum_at_limit("sum(xs)", "aggregate_sum_count", 199, 0)]
#[case::digits_selected("round(x, y)", "round_digits_remove", 198, 1)]
#[case::digits_at_limit("round(x, y)", "round_digits_remove", 199, 0)]
#[tokio::test]
async fn precision_and_collection_wrappers_respect_delimiter_limit(
    #[case] expression: &str,
    #[case] operator: &str,
    #[case] depth: usize,
    #[case] expected: usize,
) {
    let source = format!(
        "out={}{}{}\n",
        "(".repeat(depth),
        expression,
        ")".repeat(depth)
    );

    assert_compiles_with_candidate_count(&source, operator, expected).await;
}

#[rstest]
#[case::below_limit(199, 1)]
#[case::at_limit(200, 0)]
#[tokio::test]
async fn import_destination_respects_existing_callee_parentheses(
    #[case] depth: usize,
    #[case] expected: usize,
) {
    let source = format!(
        "out={}filter{}(None, [1])\n",
        "(".repeat(depth),
        ")".repeat(depth)
    );

    assert_compiles_with_candidate_count(&source, "filter_predicate_swap", expected).await;
}

#[rstest]
#[case::below_limit(197, 1)]
#[case::at_limit(198, 0)]
#[tokio::test]
async fn intermediate_removal_accounts_for_grouped_decimal_receiver(
    #[case] depth: usize,
    #[case] expected: usize,
) {
    let source = format!(
        "from decimal import Decimal\nout=round({}Decimal('2.45'){}.quantize(Decimal('.1')) * 2, 1)\n",
        "(".repeat(depth),
        ")".repeat(depth)
    );

    assert_compiles_with_candidate_count(&source, "intermediate_rounding_remove", expected).await;
}

#[tokio::test]
async fn sum_walrus_stays_in_the_original_compiler_scope() {
    let source = "out=sum((items := source()))\n";

    assert_compiles_with_candidate_count(source, "aggregate_sum_count", 0).await;
}

async fn assert_compiles_with_candidate_count(source: &str, operator: &str, expected: usize) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    fs::write(&path, source).unwrap();
    let compile = "compile(open('subject.py',encoding='utf-8').read(),'subject.py','exec')";

    let baseline = run_python(root.path(), compile);
    let manifest = plan(root.path(), operator).await;
    let candidates = manifest["candidates"].as_array().unwrap();

    assert_python_output(operator, "original compile", &baseline, "");
    assert_eq!(candidates.len(), expected, "{manifest}");
    for candidate in candidates {
        fs::write(&path, apply_edit(source, candidate)).unwrap();
        assert_python_output(
            operator,
            "mutated compile",
            &run_python(root.path(), compile),
            "",
        );
    }
}

#[tokio::test]
async fn decimal_quantum_matches_integer_reference_across_signed_midpoints() {
    let source = "from decimal import Decimal\ndef run(value): return Decimal(value).quantize(Decimal('1e-1'))\n";
    let harness = concat!(
        "import subject as s\n",
        "from decimal import Inexact, localcontext\n",
        "for coefficient in range(-97,98):\n",
        " with localcontext() as context:\n",
        "  result=s.run(str(coefficient)+'e-2')\n",
        "  magnitude, remainder=divmod(abs(coefficient),10)\n",
        "  magnitude += remainder > 5 or (remainder == 5 and magnitude%2 == 1)\n",
        "  expected=(abs(coefficient) if EXPECTED_EXP == -2 else magnitude)\n",
        "  sign, digits, exponent=result.as_tuple()\n",
        "  assert sign == (coefficient < 0) and exponent == EXPECTED_EXP\n",
        "  assert int(''.join(map(str,digits))) == expected\n",
        "  assert context.flags[Inexact] == (EXPECTED_EXP == -1 and remainder != 0)\n",
        "print('195 coefficients')\n",
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    fs::write(&path, source).unwrap();

    let manifest = plan(root.path(), "decimal_quantum_shift").await;
    let baseline = run_python(root.path(), &harness.replace("EXPECTED_EXP", "-1"));
    let candidate = manifest["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["replacement"] == "'1e-2'")
        .expect("finer quantum is available");
    fs::write(&path, apply_edit(source, candidate)).unwrap();
    let mutant = run_python(root.path(), &harness.replace("EXPECTED_EXP", "-2"));

    assert_python_output("quantum", "baseline", &baseline, "195 coefficients\n");
    assert_python_output("quantum", "mutant", &mutant, "195 coefficients\n");
}

#[rstest]
#[case::remove_left("[x for x in xs if[x]]", "comprehension_filter_remove", 1)]
#[case::invert_left("[x for x in xs if[x]]", "comprehension_filter_invert", 1)]
#[case::remove_right("[x for x in xs if pred(x)if ok(x)]", "comprehension_filter_remove", 2)]
#[case::remove_for(
    "[x for x in xs if pred(x)for item in x]",
    "comprehension_filter_remove",
    1
)]
#[case::remove_async_for(
    "async def run(xs): return [x for x in xs if pred(x)async for i in x]",
    "comprehension_filter_remove",
    1
)]
#[tokio::test]
async fn adjoining_comprehension_tokens_compile(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] expected: usize,
) {
    assert_compiles_with_candidate_count(source, operator, expected).await;
}

#[rstest]
#[case::untouched_call_trivia(" # keep this comment\n(x)\n", 1)]
#[case::below_limit(&format!("{}x{}", "(".repeat(197), ")".repeat(197)), 1)]
#[case::at_limit(&format!("{}x{}", "(".repeat(198), ")".repeat(198)), 0)]
#[tokio::test]
async fn implicit_generator_becomes_a_separate_receiver_tuple_value(
    #[case] item: &str,
    #[case] expected: usize,
) {
    let source = format!(
        "from decimal import Decimal as D\ndef run(xs): return round(D('2.45').quantize({item} for x in xs)*3,1)"
    );

    assert_compiles_with_candidate_count(&source, "intermediate_rounding_remove", expected).await;
}

#[tokio::test]
async fn single_implicit_generator_retains_the_shared_call_argument_grouping() {
    let source = "def run(xs): return round(round(x for x in xs)*3,1)";

    assert_compiles_with_candidate_count(source, "intermediate_rounding_remove", 1).await;
}

#[rstest]
#[case::removed("comprehension_filter_remove", "True", "removed")]
#[case::inverted("comprehension_filter_invert", "not (x > 0)", "inverted")]
#[tokio::test]
async fn collection_filters_match_a_reference_loop_over_generated_lists(
    #[case] operator: &str,
    #[case] replacement: &str,
    #[case] mode: &str,
) {
    let source = "def run(items): return [x for x in items if x > 0 if x != 2]\n";
    let harness = concat!(
        "import itertools, subject\n",
        "mode='EXPECTED_MODE'\n",
        "checked=0\n",
        "for width in range(5):\n",
        " for values in itertools.product(range(-2,3),repeat=width):\n",
        "  expected=[]\n",
        "  for value in values:\n",
        "   if mode=='original' and value <= 0: continue\n",
        "   if mode=='inverted' and value > 0: continue\n",
        "   if value != 2: expected.append(value)\n",
        "  assert subject.run(list(values)) == expected, (mode,values,expected)\n",
        "  checked+=1\n",
        "print(checked)\n",
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    fs::write(&path, source).unwrap();

    let manifest = plan(root.path(), operator).await;
    let initial = run_python(root.path(), &harness.replace("EXPECTED_MODE", "original"));
    let candidate = manifest["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| {
            candidate["original"] == "x > 0" && candidate["replacement"] == replacement
        })
        .expect("the first condition is eligible");
    fs::write(&path, apply_edit(source, candidate)).unwrap();
    let changed = run_python(root.path(), &harness.replace("EXPECTED_MODE", mode));

    assert_python_output(operator, "original reference", &initial, "781\n");
    assert_python_output(operator, "mutated reference", &changed, "781\n");
}

#[tokio::test]
async fn already_grouped_generator_keeps_its_delimiter_depth() {
    let elt = format!("{}x{}", "(".repeat(197), ")".repeat(197));
    let source = format!(
        "from decimal import Decimal as D\ndef run(xs): return round(D('2.45').quantize(({elt} for x in xs))*3,1)"
    );

    assert_compiles_with_candidate_count(&source, "intermediate_rounding_remove", 1).await;
}

#[rstest]
#[case::module("out=s.union({2})")]
#[case::module_read_in_function("def f(): return s.union({2})\nout=f()")]
#[tokio::test]
async fn reflective_set_rebinding(#[case] usage: &str) {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "class Custom:\n def union(self, x): return 'custom'\ndef marker(): pass\ns={{1}}\nmarker.__globals__['s']=Custom()\n{usage}\n"
    );
    fs::write(root.path().join("subject.py"), source).unwrap();
    let op = "set_union_intersection_swap";

    let manifest = plan(root.path(), op).await;
    let original = run_python(root.path(), "from subject import out; print(out)");

    assert_eq!(manifest["candidates"], serde_json::json!([]));
    assert_python_output(op, "original", &original, "custom\n");
}
