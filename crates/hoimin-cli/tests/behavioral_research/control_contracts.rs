use super::{assert_python_output, fs, plan, run_python};
use rstest::rstest;

#[rstest]
#[case::ordinary_handler(
    "exception_handler_swap",
    "try: raise KeyError()\nexcept LookupError: print('broad') # broad\nexcept KeyError: print('narrow'); # narrow\n",
    "except LookupError: print('broad') # broad\nexcept KeyError: print('narrow'); # narrow",
    None,
    "import subject",
    "broad\n",
    "narrow\n"
)]
#[case::star_handler(
    "exception_handler_swap",
    "try: raise ExceptionGroup('group', [KeyError(), IndexError()])\nexcept* LookupError as group: print('broad', len(group.exceptions))\nexcept* KeyError as group: print('narrow', len(group.exceptions))\n",
    "except* LookupError as group: print('broad', len(group.exceptions))\nexcept* KeyError as group: print('narrow', len(group.exceptions))",
    None,
    "import subject",
    "broad 2\n",
    "narrow 1\nbroad 1\n"
)]
#[case::star_boundary(
    "exception_base_boundary",
    "try: raise BaseExceptionGroup('group', [KeyboardInterrupt()])\nexcept* Exception: print('ordinary')\nexcept* KeyboardInterrupt: print('interrupt')\n",
    "Exception",
    Some("BaseException"),
    "import subject",
    "interrupt\n",
    "ordinary\n"
)]
#[case::star_tuple_add(
    "exception_tuple_add_pair",
    "try: raise ExceptionGroup('group', [TypeError()])\nexcept* (ValueError,): print('pair')\nexcept* TypeError: print('type')\n",
    "(ValueError,)",
    None,
    "import subject",
    "type\n",
    "pair\n"
)]
#[case::star_hierarchy(
    "exception_hierarchy",
    "class Parent(Exception): pass\nclass Child(Parent): pass\ntry: raise ExceptionGroup('group', [Parent()])\nexcept* Child: print('first')\nexcept* Parent: print('second')\n",
    "Child",
    Some("Parent"),
    "import subject",
    "second\n",
    "first\n"
)]
#[case::cause_none(
    "exception_cause_omit",
    "def run():\n try: raise TypeError()\n except TypeError: raise KeyError() from None\n",
    " from None",
    None,
    "from subject import run\ntry: run()\nexcept KeyError as e: print(e.__suppress_context__)",
    "True\n",
    "False\n"
)]
#[case::cause_walrus(
    "exception_cause_omit",
    "def make():\n print('cause')\n return RuntimeError()\ndef run():\n def read():\n  nonlocal cause\n  return cause\n raise KeyError() from (cause := make())\n",
    " from",
    None,
    "from subject import run\ntry: run()\nexcept KeyError as e: print(type(e.__cause__).__name__)",
    "cause\nRuntimeError\n",
    "NoneType\n"
)]
#[case::cause_yield(
    "exception_cause_omit",
    "def run():\n raise KeyError() from (yield 'suspended')\n",
    " from",
    None,
    "from subject import run\nimport inspect\nprint(inspect.isgeneratorfunction(run))\ntry: print(next(run()))\nexcept KeyError: print('raised')",
    "True\nsuspended\n",
    "True\nraised\n"
)]
#[case::case_order(
    "match_case_swap",
    "match [7]:\n case value if value: print('guard', value)\n case [value]: print('sequence', value)\n case _: print('fallback')\n",
    "case value if value: print('guard', value)\n case [value]: print('sequence', value)",
    None,
    "import subject",
    "guard [7]\n",
    "sequence 7\n"
)]
#[case::pattern_sequence(
    "match_pattern_wildcard",
    "match [3, 4]:\n case [1 as first, second]: print(first, second)\n case _: print('fallback')\n",
    "1",
    None,
    "import subject",
    "fallback\n",
    "3 4\n"
)]
#[case::pattern_mapping(
    "match_pattern_wildcard",
    "match {'key': 3, 'rest': 4}:\n case {'key': 1 | 2, **rest}: print(rest)\n case _: print('fallback')\n",
    "1 | 2",
    Some("_"),
    "import subject",
    "fallback\n",
    "{'rest': 4}\n"
)]
#[case::pattern_class(
    "match_pattern_wildcard",
    "class C:\n code = (3, 4)\nmatch C():\n case C(code=(1, 2)): print('class')\n case _: print('fallback')\n",
    "(1, 2)",
    Some("_"),
    "import subject",
    "fallback\n",
    "class\n"
)]
#[case::pattern_closure_guard(
    "match_pattern_wildcard",
    "class K:\n value = 1\nclass Guard:\n def __bool__(self):\n  print('tested')\n  return True\ndef make(K):\n def inner(subject):\n  match subject:\n   case [K.value] if Guard(): print('matched')\n return inner\n",
    "K.value] if Guard()",
    Some("_] if (Guard()) if True else K"),
    "from subject import make, K\ninner = make(K)\nprint(inner.__code__.co_freevars)\ninner([1])\ninner([2])",
    "('K',)\ntested\nmatched\n",
    "('K',)\ntested\nmatched\ntested\nmatched\n"
)]
#[case::pattern_class_closure(
    "match_pattern_wildcard",
    "def make(C):\n def inner(subject):\n  match subject:\n   case [C()]: print('matched')\n   case _: print('fallback')\n return inner\n",
    "C()]",
    Some("_] if True or C"),
    "from subject import make\ninner = make(str)\nprint(inner.__code__.co_freevars)\ninner([42])",
    "('C',)\nfallback\n",
    "('C',)\nmatched\n"
)]
#[case::position_swap(
    "match_class_position_swap",
    "class C:\n __match_args__ = ('left', 'middle', 'right')\n left, middle, right = 1, 2, 3\nmatch C():\n case C(first, 2, last): print(first,last)\n",
    "first, 2, last",
    Some("last, 2, first"),
    "import subject",
    "1 3\n",
    "3 1\n"
)]
#[tokio::test]
async fn reference_dispatch_changes_selected_behavior(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] original: &str,
    #[case] replacement: Option<&str>,
    #[case] harness: &str,
    #[case] baseline: &str,
    #[case] mutant: &str,
) {
    outcomes(
        source,
        operator,
        original,
        replacement,
        harness,
        baseline,
        mutant,
    )
    .await;
}

#[tokio::test]
async fn reference_dispatch_moves_position_grouping_at_compiler_limit() {
    let grouped = format!("{}0{}", "(".repeat(198), ")".repeat(198));
    let original = format!("{grouped}, [[1]]");
    let replacement = format!("[[1]], {grouped}");
    let source = format!(
        "class C:\n __match_args__ = ('first', 'second')\n first, second = 0, [[1]]\nmatch C():\n case C({original}): print('matched')\n case _: print('fallback')\n",
    );

    outcomes(
        &source,
        "match_class_position_swap",
        &original,
        Some(&replacement),
        "import subject",
        "matched\n",
        "fallback\n",
    )
    .await;
}

// Select one precise edit from the real CLI plan. This also supports survivor
// observations, where the original and mutant intentionally print the same result.
async fn outcomes(
    source: &str,
    op: &str,
    before: &str,
    replacement: Option<&str>,
    harness: &str,
    baseline: &str,
    mutant: &str,
) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("subject.py");
    fs::write(&file, source).unwrap();
    let manifest = plan(dir.path(), op).await;
    let mut matching = manifest["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|candidate| {
            candidate["original"] == before
                && replacement.is_none_or(|text| candidate["replacement"] == text)
        });
    let edit = matching
        .next()
        .unwrap_or_else(|| panic!("missing {before:?} in {manifest}"));
    assert!(matching.next().is_none(), "ambiguous fixture");
    let start = usize::try_from(edit["span"]["start"].as_u64().unwrap()).unwrap();
    let len = usize::try_from(edit["span"]["length"].as_u64().unwrap()).unwrap();
    assert_eq!(&source[start..start + len], before);

    let original = run_python(dir.path(), harness);
    let mut changed = source.to_owned();
    changed.replace_range(start..start + len, edit["replacement"].as_str().unwrap());
    fs::write(file, changed).unwrap();
    let actual = run_python(dir.path(), harness);

    assert_python_output(op, "original", &original, baseline);
    assert_python_output(op, "mutated", &actual, mutant);
}

const FINALLY: &str = "events=[]\ndef f(mode):\n try:\n  if mode == 'return': return 7\n  if mode in ('handled','handler','group'): raise ValueError()\n  if mode == 'raise': raise KeyError()\n except ValueError:\n  if mode == 'handler': raise TypeError()\n  events.append('handled')\n else:\n  if mode == 'else': raise IndexError()\n finally:\n  events.append('cleanup')\n return 9\n";

#[rstest]
#[case::normal("normal", "9 ['cleanup']\n", "9 ['cleanup']\n")]
#[case::handled("handled", "9 ['handled', 'cleanup']\n", "9 ['handled', 'cleanup']\n")]
#[case::returning("return", "7 ['cleanup']\n", "7 []\n")]
#[case::escaping("raise", "KeyError ['cleanup']\n", "KeyError []\n")]
#[case::reraised("handler", "TypeError ['cleanup']\n", "TypeError []\n")]
#[case::else_exception("else", "IndexError ['cleanup']\n", "IndexError []\n")]
#[tokio::test]
async fn cleanup_after_handler(#[case] mode: &str, #[case] baseline: &str, #[case] mutant: &str) {
    let harness = format!(
        "import subject as s\ntry: print(s.f('{mode}'),s.events)\nexcept Exception as e: print(type(e).__name__,s.events)"
    );

    outcomes(
        FINALLY,
        "finally_unwrap",
        "finally",
        None,
        &harness,
        baseline,
        mutant,
    )
    .await;
}

#[rstest]
#[case::cleanup_returns("return 7", "returned 7\n")]
#[case::cleanup_raises("raise ValueError()", "ValueError KeyError\n")]
#[tokio::test]
async fn cleanup_overrides_pending_exception(#[case] action: &str, #[case] original: &str) {
    // Arrange.
    let headers = "try: raise KeyError()\n finally";
    let source = format!("def run():\n {headers}: {action}\n");
    let harness = concat!(
        "from subject import run\n",
        "try: print('returned', run())\n",
        "except Exception as e: print(type(e).__name__, type(e.__context__).__name__)\n",
    );

    // Act and assert both executions of the selected edit.
    outcomes(
        &source,
        "finally_unwrap",
        headers,
        None,
        harness,
        original,
        "KeyError NoneType\n",
    )
    .await;
}

#[rstest]
#[case::loop_break("break")]
#[case::loop_continue("continue")]
#[tokio::test]
async fn cleanup_on_loop_transfer(#[case] action: &str) {
    let part = format!("try: {action}\n finally");
    let source = format!("events=[]\nfor i in (1,):\n {part}: events.append(i)\n");

    outcomes(
        &source,
        "finally_unwrap",
        &part,
        None,
        "import subject; print(subject.events)",
        "[1]\n",
        "[]\n",
    )
    .await;
}

#[rstest]
#[case::exhaust("list(g)", "[1]\n", "[1]\n")]
#[case::close("g.close()", "[1]\n", "[]\n")]
#[tokio::test]
async fn suspended_generator_cleanup(
    #[case] finish: &str,
    #[case] baseline: &str,
    #[case] mutant: &str,
) {
    let source = "events=[]\ndef gen():\n try: yield 4\n finally: events.append(1)\n";
    let harness = format!("import subject as s\ng=s.gen(); next(g); {finish}; print(s.events)");

    outcomes(
        source,
        "finally_unwrap",
        "try: yield 4\n finally",
        None,
        &harness,
        baseline,
        mutant,
    )
    .await;
}

#[tokio::test]
async fn cleanup_on_task_cancellation() {
    let source = "import asyncio\nevents=[]\nasync def wait():\n try: await asyncio.Future()\n finally: events.append('cleanup')\n";
    let harness = "import asyncio, subject as s\nasync def run():\n t=asyncio.create_task(s.wait()); await asyncio.sleep(0); t.cancel()\n try: await t\n except asyncio.CancelledError: pass\n print(s.events)\nasyncio.run(run())";

    outcomes(
        source,
        "finally_unwrap",
        "try: await asyncio.Future()\n finally",
        None,
        harness,
        "['cleanup']\n",
        "[]\n",
    )
    .await;
}

#[tokio::test]
async fn partial_exception_group_skips_cleanup() {
    let source = "events=[]\ndef run():\n try: raise ExceptionGroup('g',[KeyError(),ValueError()])\n except* ValueError: events.append('handled')\n finally: events.append('cleanup')\n";
    let harness = "import subject as s\ntry: s.run()\nexcept ExceptionGroup: print(s.events)";

    outcomes(
        source,
        "finally_unwrap",
        "finally",
        None,
        harness,
        "['handled', 'cleanup']\n",
        "['handled']\n",
    )
    .await;
}

#[tokio::test]
async fn dead_arm_keeps_nonlocal_and_multiline_debug_string() {
    let source = "def f(v):\n def inner():\n  nonlocal text\n  return text\n match v:\n  case [text]:\n   return f'''line\n{text=}'''\n  case _: return 'fallback'\n";
    let harness = "import subject; print(subject.f([3]))";

    outcomes(
        source,
        "match_case_delete",
        "case [text]:",
        None,
        harness,
        "line\ntext=3\n",
        "fallback\n",
    )
    .await;
}

#[tokio::test]
async fn deleted_pattern_does_not_read_class_attribute() {
    let source = "events=[]\nclass Obj:\n @property\n def x(self): events.append('x'); return 1\ndef subject(): events.append('subject'); return Obj()\nmatch subject():\n case Obj(x=1): events.append('arm')\n case _: events.append('fallback')\n";
    let harness = "import subject; print(subject.events)";

    outcomes(
        source,
        "match_case_delete",
        "case Obj(x=1):",
        None,
        harness,
        "['subject', 'x', 'arm']\n",
        "['subject', 'fallback']\n",
    )
    .await;
}

#[rstest]
#[case::never(
    "(False and (pred(y)))",
    "('fallback', 5) [5]\n",
    "('fallback', 5) []\n"
)]
#[case::always("(True or (pred(y)))", "('fallback', 5) [5]\n", "('selected', 5) []\n")]
#[tokio::test]
async fn guard_short_circuits_after_capture(
    #[case] replacement: &str,
    #[case] baseline: &str,
    #[case] mutant: &str,
) {
    let source = "events=[]\ndef pred(x): events.append(x); return False\ndef f(value):\n match value:\n  case y if pred(y): return 'selected', y\n  case _: return 'fallback', y\n";

    outcomes(
        source,
        "match_guard_constant",
        "pred(y)",
        Some(replacement),
        "import subject as s; print(s.f(5), s.events)",
        baseline,
        mutant,
    )
    .await;
}

#[tokio::test]
async fn never_guard_preserves_generator_classification() {
    let source = "def gen(v):\n match v:\n  case [x] if (yield x): return 1\n return 2\n";
    let harness = "import subject\ng=subject.gen([6]); print(type(g).__name__)\ntry: print(next(g))\nexcept StopIteration as e: print('returned',e.value)";

    outcomes(
        source,
        "match_guard_constant",
        "yield x",
        Some("(False and (yield x))"),
        harness,
        "generator\n6\n",
        "generator\nreturned 2\n",
    )
    .await;
}

#[tokio::test]
async fn nested_or_removal_preserves_surviving_capture() {
    let source = "def f(v):\n match v:\n  case {'v': (([1, x]) | ([2, x])) as obj}: return x\n  case _: return None\n";

    outcomes(
        source,
        "match_or_alternative_delete",
        "([1, x]) | ",
        None,
        "import subject as s; print(s.f({'v':[1,3]}), s.f({'v':[2,5]}))",
        "3 5\n",
        "None 5\n",
    )
    .await;
}

#[rstest]
#[case::write_then_notify(
    "state['v'] = 1\n notify(state)",
    "state={'v':0}; events=[]\ndef notify(s): events.append(s['v'])",
    "[1]\n",
    "[0]\n"
)]
#[case::assert_before_write(
    "assert flag\n events.append(1)",
    "events=[]; flag=False",
    "[]\n",
    "[1]\n"
)]
#[case::independent_calls(
    "events.append('a'); events.append('b')",
    "events=[]",
    "['a', 'b']\n",
    "['b', 'a']\n"
)]
#[tokio::test]
async fn externally_observed_order(
    #[case] pair: &str,
    #[case] setup: &str,
    #[case] baseline: &str,
    #[case] mutant: &str,
) {
    let source = format!("{setup}\ndef run():\n {pair}\n");
    let harness = "import subject as s\ntry: s.run()\nexcept AssertionError: pass\nprint(s.events)";

    outcomes(
        &source,
        "statement_order_swap",
        pair,
        None,
        harness,
        baseline,
        mutant,
    )
    .await;
}

#[tokio::test]
async fn awaited_call_order() {
    let pair = "await emit('a')\n await emit('b')";
    let source =
        format!("events=[]\nasync def emit(x): events.append(x)\nasync def run():\n {pair}\n");

    outcomes(
        &source,
        "statement_order_swap",
        pair,
        None,
        "import asyncio, subject as s; asyncio.run(s.run()); print(s.events)",
        "['a', 'b']\n",
        "['b', 'a']\n",
    )
    .await;
}

#[rstest]
#[case::body_prefix_continuation("\tcase 1:\n\t\t\\\nevents.append(1)")]
#[case::inline_keyword_after_continuation("          \\\ncase 1: events.append(1)")]
#[case::case_and_body_continuations("          \\\ncase 1:\n           \\\nevents.append(1)")]
#[tokio::test]
async fn case_indentation_uses_logical_prefix(#[case] suite: &str) {
    let source = format!("events=[]\nmatch 1:\n{suite}\n");

    outcomes(
        &source,
        "match_case_delete",
        "case 1:",
        None,
        "import subject; print(subject.events)",
        "[1]\n",
        "[]\n",
    )
    .await;
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(12))]
    #[test]
    fn generated_or_dispatch_matches_the_reference_decision(
        width in 2usize..7, deletion in 0usize..7, subject in 0usize..8,
    ) {
        let deleted = deletion % width;
        let alternatives = (0..width).map(|i| format!("[{i}, value]")).collect::<Vec<_>>();
        let source = format!("def f(v):\n match v:\n  case {}: return value\n return -1\n", alternatives.join(" | "));
        let before = if deleted+1 == width { format!(" | {}", alternatives[deleted]) } else { format!("{} | ", alternatives[deleted]) };
        let harness = format!("import subject; print(subject.f([{subject}, 42]))");
        let baseline = if subject < width { "42\n" } else { "-1\n" };
        let expected = if subject < width && subject != deleted { "42\n" } else { "-1\n" };

        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        runtime.block_on(outcomes(&source, "match_or_alternative_delete", &before, None, &harness, baseline, expected));
    }
}

#[rstest]
#[case::two_slots(198, false, 2)]
#[case::one_slot(199, false, 0)]
#[case::full(200, false, 0)]
#[case::nested_in_guard(198, true, 0)]
#[tokio::test]
async fn review_guard_compiler_delimiter_boundary(
    #[case] groups: usize,
    #[case] call: bool,
    #[case] count: usize,
) {
    let guard = format!("{}ready{}", "(".repeat(groups), ")".repeat(groups));
    let guard = if call {
        format!("bool({guard})")
    } else {
        guard
    };
    let source = format!(
        "ready=False\nmatch 1:\n case _ if {guard}: selected=True\n case _: selected=False\n"
    );
    let dir = tempfile::tempdir().unwrap();
    let subject = dir.path().join("subject.py");
    fs::write(&subject, &source).unwrap();
    let harness = "import subject; print(subject.selected)";
    assert_python_output(
        "delimiter guard",
        "original",
        &run_python(dir.path(), harness),
        "False\n",
    );

    let manifest = plan(dir.path(), "match_guard_constant").await;
    let candidates = manifest["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), count);
    if count > 0 {
        let edit = candidates
            .iter()
            .find(|c| c["replacement"].as_str().unwrap().contains("True or"))
            .unwrap();
        let start = usize::try_from(edit["span"]["start"].as_u64().unwrap()).unwrap();
        let len = usize::try_from(edit["span"]["length"].as_u64().unwrap()).unwrap();
        let mut changed = source;
        changed.replace_range(start..start + len, edit["replacement"].as_str().unwrap());
        fs::write(subject, changed).unwrap();
        assert_python_output(
            "delimiter guard",
            "mutant",
            &run_python(dir.path(), harness),
            "True\n",
        );
    }
}

#[rstest]
#[case::integer_and_float("18446744073709551616", "18446744073709551616.0", false)]
#[case::integer_and_complex("18446744073709551616", "18446744073709551616+0j", false)]
#[case::adjacent_integer_and_float("18446744073709551617", "18446744073709551616.0", true)]
#[tokio::test]
async fn review_mapping_keys_match_cpython_compilation(
    #[case] left: &str,
    #[case] right: &str,
    #[case] valid: bool,
) {
    let dir = tempfile::tempdir().unwrap();
    let source = format!("def f(v):\n match v:\n  case {{{left}: a, {right}: b}}: return a, b\n");
    fs::write(dir.path().join("subject.py"), source).unwrap();
    let harness = "try: import subject\nexcept SyntaxError as e: print('duplicate' if 'duplicate key' in str(e) else type(e).__name__)\nelse: print('valid')";

    let compilation = run_python(dir.path(), harness);
    let manifest = plan(dir.path(), "match_case_delete").await;

    assert_python_output(
        "numeric mapping keys",
        "compile",
        &compilation,
        if valid { "valid\n" } else { "duplicate\n" },
    );
    assert_eq!(
        manifest["candidates"].as_array().unwrap().len(),
        usize::from(valid)
    );
}

#[tokio::test]
async fn review_private_store_precedes_explicit_mangled_read() {
    let dir = tempfile::tempdir().unwrap();
    let source = "class __K:\n def f(self):\n  __x = int(2)\n  print(_K__x)\n";
    fs::write(dir.path().join("subject.py"), source).unwrap();

    let original = run_python(dir.path(), "import subject; subject.__K().f()");
    let manifest = plan(dir.path(), "statement_order_swap").await;

    assert_python_output("private binding", "original", &original, "2\n");
    assert_eq!(manifest["candidates"].as_array().unwrap().len(), 0);
}
