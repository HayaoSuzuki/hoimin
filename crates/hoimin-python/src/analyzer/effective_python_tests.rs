use super::{
    AnalyzeRequest, AnalyzerCandidate, LineRange, MutationProfile, analyze_selected,
    analyze_selected_with, analyze_source_cancellable, parse_module,
};
use camino::Utf8Path;
const OPS: &[&str] = &[
    "none_truthiness",
    "mutable_default_share",
    "nonlocal_remove",
    "yield_from_to_yield",
    "mapping_setdefault_to_get",
];
fn apply(source: &str, c: &AnalyzerCandidate) -> String {
    let start = usize::try_from(c.span.start).unwrap();
    let end = start + usize::try_from(c.span.length).unwrap();
    assert_eq!(&source[start..end], c.original);
    let result = format!("{}{}{}", &source[..start], c.replacement, &source[end..]);
    assert!(parse_module(&result).is_ok(), "{result}");
    result
}
#[rstest::rstest]
#[case::result_x_is_none("result = x is None\n", "result = (not (x  ))\n")]
#[case::result_none_is_x("result = None is x\n", "result = (not (  x))\n")]
#[case::result_x_is_not_none("result = x is not None\n", "result = (not not (x   ))\n")]
#[case::result_none_is_not_x("result = None is not x\n", "result = (not not (   x))\n")]
fn none_truthiness_preserves_bool_operands_comments_and_parentheses_table_1(
    #[case] source: &str,
    #[case] expected: &str,
) {
    let out = analyze_selected(&[OPS[0]], source);

    assert_eq!(out.candidates.len(), 1, "{source}");
    assert_eq!(apply(source, &out.candidates[0]), expected);
}

#[rstest::rstest]
#[case::result_x_keep_is_none("result = (x # keep\n is None)\n", true)]
#[case::result_none_keep_is_not_x("result = (None # keep\n is not (x))\n", true)]
#[case::result_next_values_is_none("result = (next(values) is None)\n", false)]
#[case::result_x_value_is_not_none("result = (x := value) is not None\n", false)]
fn none_truthiness_preserves_bool_operands_comments_and_parentheses_table_2(
    #[case] source: &str,
    #[case] expected_comment: bool,
) {
    let out = analyze_selected(&[OPS[0]], source);

    assert_eq!(out.candidates.len(), 1, "{source}");
    let mutant = apply(source, &out.candidates[0]);
    assert_eq!(mutant.contains("# keep"), expected_comment);
}

#[rstest::rstest]
#[case::result_x_is_none_is_y("result = x is None is y\n")]
#[case::result_x_none("result = x == None\n")]
#[case::result_none_is_none("result = None is None\n")]
#[case::x_int_if_value_is_none_else_str("x: (int if value is None else str)\n")]
fn none_truthiness_preserves_bool_operands_comments_and_parentheses_table_3(#[case] source: &str) {
    assert!(
        analyze_selected(&[OPS[0]], source).candidates.is_empty(),
        "{source}"
    );
}
#[rstest::rstest]
#[case::def_f_x_none_if_x_is_none_x_return_x(
    "def f(x=None):\n    if x is None: x=[]\n    return x\n"
)]
#[case::def_f_x_none_doc_if_none_is_x_x_return_x(
    "def f(*, x=None):\n    'doc'\n    if None is x: x={}\n    return x\n"
)]
#[case::async_def_f_x_none_if_x_is_none_x_return_x(
    "async def f(x=None, /):\n    if x is None: x=[]\n    return x\n"
)]
#[case::class_c_def_init_self_x_none_if_x_is_none_x_self_x_x(
    "class C:\n    def __init__(self, x=None):\n        if x is None: x=[]\n        self.x=x\n"
)]
#[case::def_f_none_if_x_is_none_x_return_x(
    "def f(ｘ=None):\n    if x is None: x=[]\n    return x\n"
)]
fn mutable_default_matches_only_entry_literal_initialization_table_1(#[case] source: &str) {
    let out = analyze_selected(&[OPS[1]], source);

    assert_eq!(out.candidates.len(), 1, "{source}");
    let c = &out.candidates[0];
    assert_eq!(c.original, "None");
    assert!(matches!(c.replacement.as_str(), "[]" | "{}"));
    apply(source, c);
}

#[rstest::rstest]
#[case::def_f_x_if_x_is_none_x("def f(x=()):\n    if x is None: x=[]\n")]
#[case::def_f_x_none_x_1_if_x_is_none_x("def f(x=None):\n    x=1\n    if x is None: x=[]\n")]
#[case::def_f_x_none_if_x_is_none_x_1("def f(x=None):\n    if x is None: x=[1]\n")]
#[case::def_f_x_none_if_x_is_none_x_else_x(
    "def f(x=None):\n    if x is None: x=[]\n    else: x={}\n"
)]
#[case::def_f_x_none_if_x_is_none_x_y("def f(x=None):\n    if x is None: x=y=[]\n")]
#[case::def_f_x_none_if_x_is_none_x_print_x("def f(x=None):\n    if x is None: x=[]; print(x)\n")]
#[case::deco_def_f_x_none_if_x_is_none_x("@deco\ndef f(x=None):\n    if x is None: x=[]\n")]
#[case::def_f_t_x_none_if_x_is_none_x("def f[T](x=None):\n    if x is None: x=[]\n")]
fn mutable_default_matches_only_entry_literal_initialization_table_2(#[case] source: &str) {
    assert!(
        analyze_selected(&[OPS[1]], source).candidates.is_empty(),
        "{source}"
    );
}
#[rstest::rstest]
#[case::def_outer_x_0_def_inner_nonlocal_x_keep_x_1(
    "def outer():\n    x=0\n    def inner():\n        nonlocal x # keep\n        x=1\n"
)]
#[case::def_outer_x_y_0_def_inner_nonlocal_x_y_x_y_1(
    "def outer():\n    x=y=0\n    def inner(): nonlocal x, y; x=y=1\n"
)]
fn nonlocal_remove_preserves_inline_and_multiname_layout_table_1(
    #[case] source: &str,
    #[values(0, 1, 2)] scenario_index: usize,
) {
    let newline = ["\n", "\r\n", "\r"]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

    let source = source.replace('\n', newline);
    let out = analyze_selected(&[OPS[2]], &source);

    assert_eq!(out.candidates.len(), 1);
    let c = &out.candidates[0];
    assert_eq!(c.replacement, "pass");
    let mutant = apply(&source, c);
    assert_eq!(source.contains("# keep"), mutant.contains("# keep"));
}

#[test]
fn nonlocal_remove_preserves_inline_and_multiname_layout() {
    assert_eq!(
        analyze_selected(&[OPS[2]], "def f():\n    global x\n    x=1\n")
            .candidates
            .len(),
        0
    );
}
#[rstest::rstest]
#[case::def_f_yield_from_1_2("def f(): yield from (1, 2)\n")]
#[case::def_f_yield_from_keep_x_for_x_in_items(
    "def f():\n    yield from (\n        # keep\n        x for x in items\n    )\n"
)]
#[case::def_f_result_yield_from_yield_from_items(
    "def f():\n    result = (yield from (yield from items))\n"
)]
#[case::def_f_yield_from_values("def f():\n    yield \\\n        from \\\n        values\n")]
fn yield_delegation_changes_only_the_from_keyword(#[case] source: &str) {
    let out = analyze_selected(&[OPS[3]], source);

    assert!(!out.candidates.is_empty(), "{source}");
    for c in &out.candidates {
        assert_eq!(c.original, "from");
        assert_eq!(c.replacement, "");
        apply(source, c);
    }
}
#[rstest::rstest]
#[case::result_d_setdefault_key("result = d.setdefault(key)\n")]
#[case::d_setdefault_key("d.setdefault(key, [])\n")]
#[case::result_holder_data_setdefault_key_keep_value_factory(
    "result = holder.data.setdefault(\n    key, # keep\n    (value := factory()),\n)\n"
)]
fn setdefault_rename_preserves_call_source_and_rejects_splats_table_1(#[case] source: &str) {
    let out = analyze_selected(&[OPS[4]], source);

    assert_eq!(out.candidates.len(), 1, "{source}");
    let c = &out.candidates[0];
    assert_eq!(c.original, "setdefault");
    assert_eq!(c.replacement, "get");
    let mutant = apply(source, c);
    assert_eq!(source.replace("setdefault", "get"), mutant);
}

#[rstest::rstest]
#[case::d_setdefault("d.setdefault()\n")]
#[case::d_setdefault_a_b_c("d.setdefault(a,b,c)\n")]
#[case::d_setdefault_args("d.setdefault(*args)\n")]
#[case::d_setdefault_k_kwargs("d.setdefault(k, **kwargs)\n")]
#[case::d_setdefault_key_k_default("d.setdefault(key=k, default=[])\n")]
#[case::factory_setdefault_k("factory().setdefault(k,[])\n")]
#[case::items_0_setdefault_k("items[0].setdefault(k,[])\n")]
fn setdefault_rename_preserves_call_source_and_rejects_splats_table_2(#[case] source: &str) {
    assert!(
        analyze_selected(&[OPS[4]], source).candidates.is_empty(),
        "{source}"
    );
}
#[test]
fn effective_python_caps_selection_payload_and_cancellation() {
    let source = "def outer(x=None):\n    if x is None: x=[]\n    def inner():\n        nonlocal x\n        x=[1]\n    yield from x\nd.setdefault('k', [])\n";
    let out = analyze_selected(OPS, source);

    assert_eq!(out.candidates.len(), 5);
    for cap in 0..=5 {
        let prefix =
            analyze_selected_with(OPS, Utf8Path::new("pkg/sample.py"), &[], &[], cap, source);
        assert_eq!(prefix.candidates, out.candidates[..cap]);
        assert_eq!(prefix.truncated, cap < 5);
    }
    let selected = analyze_selected_with(
        OPS,
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 1, end: 1 }],
        &["outer".to_owned()],
        10,
        source,
    );
    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(
        selected.candidates[0].operator,
        hoimin_core::MutationOperator::MutableDefaultShare
    );
    let operators = serde_json::from_value(serde_json::json!(OPS)).unwrap();
    let request = AnalyzeRequest {
        path: Utf8Path::new("pkg/sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
        payload: super::super::payload::PayloadBudget::new(1),
    };
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
    assert!(analyze_source_cancellable(&request, source, || false).is_err());
}

#[rstest::rstest]
#[case::result_value_is_none("result = value is (None)\n")]
#[case::result_none_is_value("result = (None) is value\n")]
#[case::result_value_is_not_none("result = value is not ((None))\n")]
#[case::result_none_is_not_value("result = ((None)) is not value\n")]
#[case::result_value_is_keep_none("result = (value is (\n # keep\n None\n))\n")]
#[case::result_none_keep_is_value("result = ((\n None # keep\n) is value)\n")]
fn none_truthiness_removes_none_side_parenthesis_groups(#[case] source: &str) {
    let out = analyze_selected(&[OPS[0]], source);

    assert_eq!(out.candidates.len(), 1);
    let mutant = apply(source, &out.candidates[0]);
    assert_eq!(source.contains("# keep"), mutant.contains("# keep"));
}

#[rstest::rstest]
#[case::yield_from_to_yield(
    "yield_from_to_yield",
    "def f():\n    yield \\\n        from \\\n        values\n",
    3,
    "from"
)]
#[case::mapping_setdefault_to_get(
    "mapping_setdefault_to_get",
    "def f():\n    return d.\\\n        setdefault('x', [])\n",
    3,
    "setdefault"
)]
fn effective_python_selects_changed_tokens_on_continuation_lines(
    #[case] op: &str,
    #[case] source: &str,
    #[case] line: u32,
    #[case] original: &str,
) {
    let out = analyze_selected_with(
        &[op],
        Utf8Path::new("pkg/sample.py"),
        &[LineRange {
            start: line,
            end: line,
        }],
        &["f".to_owned()],
        10,
        source,
    );

    assert_eq!(out.candidates.len(), 1, "{source}");
    assert_eq!(out.candidates[0].original, original);
    apply(source, &out.candidates[0]);
    let excluded = analyze_selected_with(
        &[op],
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 2, end: 2 }],
        &[],
        10,
        source,
    );
    assert_eq!(excluded.candidates.len(), 0);
}

#[rstest::rstest]
#[case::bisect_boundary_swap(
    "bisect_boundary_swap",
    "import bisect\nx=bisect.bisect_left([1, 1], 1)\n",
    "bisect_right"
)]
#[case::bisect_boundary_swap_2(
    "bisect_boundary_swap",
    "from bisect import bisect_right as boundary\nx=boundary([1, 1], 1)\n",
    "__import__('bisect').bisect_left"
)]
#[case::deepcopy_to_copy("deepcopy_to_copy", "import copy as c\nx=c.deepcopy(value)\n", "copy")]
#[case::deepcopy_to_copy_4(
    "deepcopy_to_copy",
    "from copy import deepcopy as clone\nx=clone(value)\n",
    "__import__('copy').copy"
)]
#[case::deque_popleft_to_pop("deque_popleft_to_pop", "x=q.popleft()\n", "pop")]
#[case::deque_popleft_to_pop_6("deque_popleft_to_pop", "x=self.q.popleft()\n", "pop")]
fn effective_python_extra_boundaries_copy_and_queue_table_1(
    #[case] op: &str,
    #[case] source: &str,
    #[case] expected: &str,
) {
    let out = analyze_selected(&[op], source);

    assert_eq!(out.candidates.len(), 1, "{source}");
    assert_eq!(out.candidates[0].replacement, expected);
    apply(source, &out.candidates[0]);
}

#[rstest::rstest]
#[case::bisect_boundary_swap(
    "bisect_boundary_swap",
    "import bisect\nbisect=other\nx=bisect.bisect_left(xs, 1)\n"
)]
#[case::bisect_boundary_swap_2(
    "bisect_boundary_swap",
    "import bisect\ndef f(bisect): return bisect.bisect_left(xs, 1)\n"
)]
#[case::bisect_boundary_swap_3("bisect_boundary_swap", "import bisect\nx=bisect.bisect(xs, 1)\n")]
#[case::deepcopy_to_copy("deepcopy_to_copy", "import copy\nx=copy.deepcopy(xs, {})\n")]
#[case::deepcopy_to_copy_5("deepcopy_to_copy", "import copy\nx=copy.deepcopy(xs, memo={})\n")]
#[case::deepcopy_to_copy_6("deepcopy_to_copy", "import copy\nx=copy.deepcopy(*args)\n")]
#[case::deepcopy_to_copy_7(
    "deepcopy_to_copy",
    "from copy import deepcopy\n__import__=None\nx=deepcopy(xs)\n"
)]
#[case::deque_popleft_to_pop("deque_popleft_to_pop", "x=q.popleft(1)\n")]
#[case::deque_popleft_to_pop_9("deque_popleft_to_pop", "x=q.popleft(**kw)\n")]
#[case::deque_popleft_to_pop_10("deque_popleft_to_pop", "x=make().popleft()\n")]
#[case::deque_popleft_to_pop_11("deque_popleft_to_pop", "x=q.pop()\n")]
fn effective_python_extra_boundaries_copy_and_queue_table_2(
    #[case] op: &str,
    #[case] source: &str,
) {
    assert!(
        analyze_selected(&[op], source).candidates.is_empty(),
        "{source}"
    );
}

#[test]
fn effective_python_extra_selection_prefix_and_layout() {
    let ops = [
        "bisect_boundary_swap",
        "deepcopy_to_copy",
        "deque_popleft_to_pop",
    ];
    let source = "import bisect\nimport copy\ndef f(xs, q):\n    a=bisect.\\\n        bisect_left(xs, 1)\n    b=copy.\\\n        deepcopy(xs)\n    c=q.\\\n        popleft()\n    return a, b, c\n";
    let all = analyze_selected(&ops, source);

    assert_eq!(all.candidates.len(), 3);
    for cap in 0..=4 {
        let out =
            analyze_selected_with(&ops, Utf8Path::new("pkg/sample.py"), &[], &[], cap, source);
        assert_eq!(out.candidates, all.candidates[..cap.min(3)]);
        assert_eq!(out.truncated, cap < 3);
    }
    for (op, line) in ops.into_iter().zip([5, 7, 9]) {
        for source in [
            source.to_owned(),
            source.replace('\n', "\r\n"),
            source.replace('\n', "\r"),
        ] {
            let out = analyze_selected_with(
                &[op],
                Utf8Path::new("pkg/sample.py"),
                &[LineRange {
                    start: line,
                    end: line,
                }],
                &["f".to_owned()],
                10,
                &source,
            );
            assert_eq!(out.candidates.len(), 1, "{source}");
            apply(&source, &out.candidates[0]);
        }
    }
    let operators = serde_json::from_value(serde_json::json!(ops)).unwrap();
    let request = AnalyzeRequest {
        path: Utf8Path::new("pkg/sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
        payload: super::super::payload::PayloadBudget::new(1),
    };
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
    assert!(analyze_source_cancellable(&request, source, || false).is_err());
}

#[rstest::rstest]
#[case::import_asyncio_x_asyncio_gather_task("import asyncio\nx=asyncio.gather(task())\n")]
#[case::from_asyncio_import_gather_as_collect_x_collect_task(
    "from asyncio import gather as collect\nx=collect(task(),)\n"
)]
#[case::import_asyncio_as_a_x_a_gather_task_keep(
    "import asyncio as a\nx=a.gather(\n task(), # keep\n)\n"
)]
#[case::import_asyncio_x_asyncio_gather_task_keep(
    "import asyncio\nx=asyncio.gather((task()) # keep\n)\n"
)]
fn effective_python_gather_eligibility_and_layout_table_1(#[case] source: &str) {
    let out = analyze_selected(&["gather_return_exceptions"], source);

    assert_eq!(out.candidates.len(), 1, "{source}");
    let mutant = apply(source, &out.candidates[0]);
    assert!(mutant.contains("return_exceptions=True"));
    assert_eq!(source.contains("# keep"), mutant.contains("# keep"));
}

#[rstest::rstest]
#[case::import_asyncio_x_asyncio_gather("import asyncio\nx=asyncio.gather()\n")]
#[case::import_asyncio_x_asyncio_gather_task("import asyncio\nx: asyncio.gather(task())\n")]
#[case::import_asyncio_x_asyncio_gather_task_return_exceptions_false(
    "import asyncio\nx=asyncio.gather(task(), return_exceptions=False)\n"
)]
#[case::import_asyncio_x_asyncio_gather_task_return_exceptions_true(
    "import asyncio\nx=asyncio.gather(task(), return_exceptions=True)\n"
)]
#[case::import_asyncio_x_asyncio_gather_tasks("import asyncio\nx=asyncio.gather(*tasks)\n")]
#[case::import_asyncio_x_asyncio_gather_task_kw("import asyncio\nx=asyncio.gather(task(), **kw)\n")]
#[case::import_asyncio_x_asyncio_gather_task_for_task_in_tasks(
    "import asyncio\nx=asyncio.gather(task for task in tasks)\n"
)]
#[case::import_asyncio_x_asyncio_gather_task_for_task_in_tasks_8(
    "import asyncio\nx=asyncio.gather((task for task in tasks))\n"
)]
#[case::import_asyncio_asyncio_other_x_asyncio_gather_task(
    "import asyncio\nasyncio=other\nx=asyncio.gather(task())\n"
)]
#[case::import_asyncio_def_f_asyncio_return_asyncio_gather_task(
    "import asyncio\ndef f(asyncio): return asyncio.gather(task())\n"
)]
fn effective_python_gather_eligibility_and_layout_table_2(#[case] source: &str) {
    assert!(
        analyze_selected(&["gather_return_exceptions"], source)
            .candidates
            .is_empty(),
        "{source}"
    );
}
#[rstest::rstest]
#[case::import_itertools_as_it_x_it_takewhile_pred_xs(
    "import itertools as it\nx=it.takewhile(pred, xs)\n",
    "dropwhile"
)]
#[case::from_itertools_import_dropwhile_as_remaining_x_remaining_pred_xs(
    "from itertools import dropwhile as remaining\nx=remaining(pred, xs)\n",
    "__import__('itertools').takewhile"
)]
fn effective_python_iterator_prefix_swap_uses_import_proof_table_1(
    #[case] source: &str,
    #[case] expected: &str,
) {
    let out = analyze_selected(&["itertools_take_drop_swap"], source);

    assert_eq!(out.candidates.len(), 1);
    assert_eq!(out.candidates[0].replacement, expected);
    apply(source, &out.candidates[0]);
}

#[rstest::rstest]
#[case::import_itertools_itertools_other_x_itertools_takewhile_pred_xs(
    "import itertools\nitertools=other\nx=itertools.takewhile(pred,xs)\n"
)]
#[case::import_itertools_x_itertools_filterfalse_pred_xs(
    "import itertools\nx=itertools.filterfalse(pred,xs)\n"
)]
#[case::import_itertools_x_itertools_takewhile_pred_xs(
    "import itertools\nx: itertools.takewhile(pred,xs)\n"
)]
#[case::from_itertools_import_takewhile_def_f_takewhile_return_takewhile(
    "from itertools import takewhile\ndef f(takewhile): return takewhile(pred,xs)\n"
)]
fn effective_python_iterator_prefix_swap_uses_import_proof_table_2(#[case] source: &str) {
    assert!(
        analyze_selected(&["itertools_take_drop_swap"], source)
            .candidates
            .is_empty(),
        "{source}"
    );
}
#[rstest::rstest]
#[case::x_f_value_r("x=f'{value!r}'\n", 1)]
#[case::x_f_value_s_10("x=f'{value!s:>10}'\n", 1)]
#[case::x_f_value_r_3("x=f'{value=!r}'\n", 1)]
#[case::x_f_value_r_width_s("x=f'{value!r:{width!s}}'\n", 2)]
#[case::x_f_value_r_f_other_s("x=f'{value!r}' f'{other!s}'\n", 2)]
#[case::x_f_f_value_r_s("x=f\"{f'{value!r}'!s}\"\n", 2)]
#[case::x_t_f_value_r_s("x=t\"{f'{value!r}'!s}\"\n", 1)]
#[case::x_f_value_s_keep_r("x=f'''{(value # !s keep\n)!r}'''\n", 1)]
#[case::x_f_value("x=f'{value}'\n", 0)]
#[case::x_f_value_r_10("x=f'{{value!r}}'\n", 0)]
#[case::x_f_value_r_11("x: f'{value!r}'\n", 0)]
#[case::x_f_value_12("x=f'{value=}'\n", 0)]
#[case::x_f_value_a("x=f'{value!a}'\n", 0)]
#[case::x_t_value_r("x=t'{value!r}'\n", 0)]
#[case::x_literal_r("x='literal !r'\n", 0)]
#[case::x_f_literal_r("x=f\"{'literal !r'}\"\n", 0)]
fn effective_python_fstring_swaps_only_explicit_conversions(
    #[case] source: &str,
    #[case] count: usize,
) {
    let out = analyze_selected(&["fstring_conversion_swap"], source);

    assert_eq!(out.candidates.len(), count, "{source}");
    for candidate in &out.candidates {
        assert!(matches!(
            (candidate.original.as_str(), candidate.replacement.as_str()),
            ("r", "s") | ("s", "r")
        ));
        apply(source, candidate);
    }
}

#[test]
fn effective_python_third_wave_selection_and_admission() {
    let ops = [
        "gather_return_exceptions",
        "itertools_take_drop_swap",
        "fstring_conversion_swap",
    ];
    let source = "import asyncio\nimport itertools\ndef f(task,xs,value):\n    a=asyncio.gather(\n        task,\n    )\n    b=itertools.\\\n        takewhile(lambda x:x<3,xs)\n    c=f'''{value\n        !r}'''\n    return a,b,c\n";
    let all = analyze_selected(&ops, source);

    assert_eq!(all.candidates.len(), 3);
    for cap in 0..=4 {
        let out =
            analyze_selected_with(&ops, Utf8Path::new("pkg/sample.py"), &[], &[], cap, source);
        assert_eq!(out.candidates, all.candidates[..cap.min(3)]);
        assert_eq!(out.truncated, cap < 3);
    }
    for (op, line) in ops.into_iter().zip([6, 8, 10]) {
        for source in [
            source.to_owned(),
            source.replace('\n', "\r\n"),
            source.replace('\n', "\r"),
        ] {
            let out = analyze_selected_with(
                &[op],
                Utf8Path::new("pkg/sample.py"),
                &[LineRange {
                    start: line,
                    end: line,
                }],
                &["f".to_owned()],
                10,
                &source,
            );
            assert_eq!(out.candidates.len(), 1, "{source}");
            apply(&source, &out.candidates[0]);
        }
        let excluded = analyze_selected_with(
            &[op],
            Utf8Path::new("pkg/sample.py"),
            &[LineRange { start: 3, end: 3 }],
            &[],
            10,
            source,
        );
        assert_eq!(excluded.candidates.len(), 0);
    }
    let operators = serde_json::from_value(serde_json::json!(ops)).unwrap();
    let request = AnalyzeRequest {
        path: Utf8Path::new("pkg/sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
        payload: super::super::payload::PayloadBudget::new(1),
    };
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
    assert!(analyze_source_cancellable(&request, source, || false).is_err());
    let source = "x=f\"{f'{value!r}'!s:{width!r}}\"\n";
    let all = analyze_selected(&ops, source);
    assert_eq!(all.candidates.len(), 3);
    for cap in 0..=3 {
        let out =
            analyze_selected_with(&ops, Utf8Path::new("pkg/sample.py"), &[], &[], cap, source);
        assert_eq!(out.candidates, all.candidates[..cap]);
    }
}
