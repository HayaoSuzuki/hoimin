use super::{assert_python_output, fs, plan, run_python};
use rstest::rstest;

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
