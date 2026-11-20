use super::{assert_python_output, fs, plan, run_python};
use rstest::rstest;

const HTML_OPTIONS: &str = concat!(
    "import html as h\n",
    "trace=[]\n",
    "def option(): trace.append('option'); return False\n",
    "def text(): trace.append('text'); return '&'\n",
    "result=h.escape(quote=option(), # keep evaluating this first\n                s=(text()))\n",
);
const URL_TYPED: &str = concat!(
    "import urllib.parse as p\n",
    "def run(text: str): return p.quote_plus(text, safe='')\n",
    "result=run('a b')\n",
);
const REGEX_BYTES: &str =
    "from re import escape as e, fullmatch\nresult=bool(fullmatch(e(b'.'),b'x'))\n";
const POSITIONAL: &str = concat!(
    "trace=[]\n",
    "def mark(x): trace.append(x); return x\n",
    "class Target:\n def consume(self, a,b): return a,b\n",
    "def receiver(): trace.append('receiver'); return Target()\n",
    "result=receiver().consume((mark(1)), # keep grouping\n ((mark(2))))\n",
);
const KEYWORD: &str = concat!(
    "trace=[]\n",
    "def mark(x): trace.append(x); return x\n",
    "def roles(*, a,b): return a,b\n",
    "result=roles(a=mark(1), b=mark(2))\n",
);
const MIXED: &str = concat!(
    "trace=[]\n",
    "def mark(x): trace.append(x); return x\n",
    "def roles(ã,b): return ã,b\n",
    "result=roles((mark(1)), # keep separator\n ｂ # keep label trivia\n = ((mark(2))))\n",
);
const ASYNC_RESULT: &str = concat!(
    "import asyncio\n",
    "trace=[]\n",
    "async def produce(*xs, **kw) -> list[int]:\n",
    " await asyncio.sleep(0)\n trace.extend(xs)\n return [kw['value']]\n",
    "async def run(xs,kw): return await (produce(*xs, **kw))\n",
);
const RAISES: &str = concat!(
    "trace=[]\n",
    "def write() -> int: trace.append(1); raise LookupError('write')\n",
    "def run(): return write()\n",
);

const HTML_EXCEPTION: &str = concat!(
    "import html\ntrace=[]\n",
    "def option(): trace.append('option'); raise ValueError('option')\n",
    "def text(): trace.append('text'); return '&'\n",
    "def run(): return html.escape(quote=option(), s=text())\n",
);
const SHORT_CIRCUIT: &str = concat!(
    "trace=[]\ndef write()->int: trace.append(1); return 8\n",
    "def run(reached): return reached and write()\n",
);
const FRESH_LIST: &str = concat!(
    "original = [9]\ndef make()->list[int]: return original\n",
    "def run(): return make()\n",
);
const CANCEL_AWAIT: &str = concat!(
    "import asyncio\ntrace=[]\n",
    "async def borrow()->int:\n",
    " try: await asyncio.Future()\n finally: trace.append('cleanup')\n",
    "async def run(): return await borrow()\n",
);

#[rstest]
#[case::html_order(
    "stdlib_escape_remove",
    HTML_OPTIONS,
    "h.escape(",
    "import subject as s; print(s.result, s.trace)",
    "&amp; ['option', 'text']\n",
    "& ['option', 'text']\n"
)]
#[case::url_typed(
    "stdlib_escape_remove",
    URL_TYPED,
    "p.quote_plus(",
    "import subject as s; print(s.result)",
    "a+b\n",
    "a b\n"
)]
#[case::regex_bytes(
    "stdlib_escape_remove",
    REGEX_BYTES,
    "e(b'.')",
    "import subject as s; print(s.result)",
    "False\n",
    "True\n"
)]
#[case::positional_receiver(
    "call_argument_swap",
    POSITIONAL,
    "(mark(1))",
    "import subject as s; print(s.result, s.trace)",
    "(1, 2) ['receiver', 1, 2]\n",
    "(2, 1) ['receiver', 1, 2]\n"
)]
#[case::keyword_placement(
    "call_argument_swap",
    KEYWORD,
    "a=mark(1)",
    "import subject as s; print(s.result, s.trace)",
    "(1, 2) [1, 2]\n",
    "(2, 1) [1, 2]\n"
)]
#[case::mixed_unicode_and_grouping(
    "call_argument_swap",
    MIXED,
    "(mark(1))",
    "import subject as s; print(s.result, s.trace)",
    "(1, 2) [1, 2]\n",
    "(2, 1) [1, 2]\n"
)]
#[case::awaited_variadic(
    "call_result_default",
    ASYNC_RESULT,
    "await (produce",
    "import asyncio, subject as s; print(asyncio.run(s.run([4],{'value':7})),s.trace)",
    "[7] [4]\n",
    "[] [4]\n"
)]
#[case::preserved_exception(
    "call_result_default",
    RAISES,
    "write()",
    "import subject as s\ntry: s.run()\nexcept Exception as e: print(type(e).__name__, str(e), s.trace)",
    "LookupError write [1]\n",
    "LookupError write [1]\n"
)]
#[case::html_raising_option(
    "stdlib_escape_remove",
    HTML_EXCEPTION,
    "html.escape(",
    "import subject as s\ntry: s.run()\nexcept Exception as e: print(type(e).__name__, str(e), s.trace)",
    "ValueError option ['option']\n",
    "ValueError option ['option']\n"
)]
#[case::not_reached(
    "call_result_default",
    SHORT_CIRCUIT,
    "write()",
    "import subject as s; print(s.run(False), s.trace)",
    "False []\n",
    "False []\n"
)]
#[case::fresh_defaults(
    "call_result_default",
    FRESH_LIST,
    "make()",
    "import subject as s; a=s.run(); b=s.run(); print(a, b, a is b)",
    "[9] [9] True\n",
    "[] [] False\n"
)]
#[case::cancelled_await(
    "call_result_default",
    CANCEL_AWAIT,
    "await borrow()",
    "import asyncio, subject as s\nasync def run():\n t=asyncio.create_task(s.run()); await asyncio.sleep(0); t.cancel()\n try: await t\n except asyncio.CancelledError: print(s.trace)\nasyncio.run(run())",
    "['cleanup']\n",
    "['cleanup']\n"
)]
#[tokio::test]
async fn call_mutations_preserve_evaluation_effects(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] prefix: &str,
    #[case] harness: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    let dir = tempfile::tempdir().unwrap();
    let subject = dir.path().join("subject.py");
    fs::write(&subject, source).unwrap();
    let manifest = plan(dir.path(), operator).await;
    let edits = manifest["candidates"].as_array().unwrap();
    assert_eq!(edits.len(), 1, "{manifest}");
    let edit = &edits[0];
    let original = edit["original"].as_str().unwrap();
    let offset = usize::try_from(edit["span"]["start"].as_u64().unwrap()).unwrap();
    let end = offset + usize::try_from(edit["span"]["length"].as_u64().unwrap()).unwrap();
    let replacement = edit["replacement"].as_str().unwrap();

    let initial = run_python(dir.path(), harness);
    let mut changed = source.to_owned();
    changed.replace_range(offset..end, replacement);
    fs::write(subject, changed).unwrap();
    let mutated = run_python(dir.path(), harness);

    assert!(original.starts_with(prefix), "{edit}");
    assert_eq!(&source[offset..end], original);
    assert_python_output(operator, "original", &initial, before);
    assert_python_output(operator, "mutation", &mutated, after);
}

#[rstest]
#[case::result_at_limit("call_result_default", "def f()->int: return 1", "f()", 199, 0)]
#[case::result_below_limit("call_result_default", "def f()->int: return 1", "f()", 198, 1)]
#[case::set_at_limit("call_result_default", "def f()->set: return {1}", "f()", 198, 0)]
#[case::set_below_limit("call_result_default", "def f()->set: return {1}", "f()", 197, 1)]
#[case::positional_at_limit("call_argument_swap", "def f(a,b): pass", "f(1,2)", 199, 0)]
#[case::positional_below_limit("call_argument_swap", "def f(a,b): pass", "f(1,2)", 198, 1)]
#[case::mixed_at_limit("call_argument_swap", "def f(a,b): pass", "f(1,b=2)", 198, 0)]
#[case::mixed_below_limit("call_argument_swap", "def f(a,b): pass", "f(1,b=2)", 197, 1)]
#[tokio::test]
async fn candidates_respect_cpython_delimiter_depth(
    #[case] operator: &str,
    #[case] definition: &str,
    #[case] call: &str,
    #[case] depth: usize,
    #[case] expected: usize,
) {
    let dir = tempfile::tempdir().unwrap();
    let subject = dir.path().join("subject.py");
    let source = format!(
        "{definition}\nout={}{}{}\n",
        "(".repeat(depth),
        call,
        ")".repeat(depth)
    );
    fs::write(&subject, &source).unwrap();
    let compile = "compile(open('subject.py', encoding='utf-8').read(), 'subject.py', 'exec')";
    let baseline = run_python(dir.path(), compile);

    let manifest = plan(dir.path(), operator).await;

    assert_python_output(operator, "original compile", &baseline, "");
    let candidates = manifest["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), expected);
    if let [candidate] = candidates.as_slice() {
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        let mut changed = source;
        changed.replace_range(start..end, candidate["replacement"].as_str().unwrap());
        fs::write(subject, changed).unwrap();
        assert_python_output(
            operator,
            "mutated compile",
            &run_python(dir.path(), compile),
            "",
        );
    }
}
