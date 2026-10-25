use super::{analyze, descriptor, python_observe};
use camino::Utf8Path;
use hoimin_core::{MutationProfile, validate_candidate};

async fn assert_change(
    source: &str,
    operator: &str,
    harness: &str,
    baseline: &str,
    expected: &str,
) {
    for newline in ["\n", "\r\n", "\r"] {
        let source = source.replace('\n', newline);
        let operators = serde_json::from_value(serde_json::json!([operator])).unwrap();
        let output = analyze(
            &hoimin_python::AnalyzeRequest {
                payload: hoimin_python::payload::PayloadBudget::default(),
                path: Utf8Path::new("subject.py"),
                lines: &[],
                symbols: &[],
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 10,
            },
            &source,
        );
        assert_eq!(output.diagnostics.len(), 0, "{source}");
        assert_eq!(output.candidates.len(), 1, "{operator}: {source}");
        let candidate = &output.candidates[0];
        validate_candidate(source.as_bytes(), &descriptor(&source, candidate)).unwrap();
        let mut mutant = source.as_bytes().to_vec();
        let start = usize::try_from(candidate.span.start).unwrap();
        let end = start + usize::try_from(candidate.span.length).unwrap();
        mutant.splice(start..end, candidate.replacement.bytes());
        let observations = python_observe(&[source.into_bytes(), mutant], harness)
            .await
            .unwrap();
        for observation in &observations {
            assert_eq!(observation["compiled"], true, "{operator}: {observation}");
            assert_eq!(
                observation["exception"],
                serde_json::Value::Null,
                "{operator}: {observation}"
            );
        }
        assert_eq!(observations[0]["stdout"], baseline, "{operator}");
        assert_eq!(observations[1]["stdout"], expected, "{operator}");
    }
}

#[tokio::test]
async fn generator_mutations_expose_truthiness_reuse_and_lazy_evaluation() {
    assert_change(
        "values = [x for x in []]\n",
        "list_comprehension_to_generator",
        "print(bool(values))",
        "False\n",
        "True\n",
    )
    .await;
    assert_change(
        "values = [x for x in (1, 2)]\n",
        "list_comprehension_to_generator",
        "print(list(values)); print(list(values))",
        "[1, 2]\n[1, 2]\n",
        "[1, 2]\n[]\n",
    )
    .await;
    assert_change(
        "seen = []\nvalues = [seen.append(x) for x in (1, 2)]\n",
        "list_comprehension_to_generator",
        "print(seen.copy()); print(list(values))",
        "[1, 2]\n[None, None]\n",
        "[]\n[None, None]\n",
    )
    .await;
    let source = "import asyncio\nasync def items():\n    yield 1\n    yield 2\nasync def collect():\n    return [x async for x in items()]\n";
    let harness = "async def main():\n    values = await collect()\n    if hasattr(values, '__aiter__'):\n        print([x async for x in values])\n    else:\n        print(list(values))\n    print(type(values).__name__)\nasyncio.run(main())";
    assert_change(
        source,
        "list_comprehension_to_generator",
        harness,
        "[1, 2]\nlist\n",
        "[1, 2]\nasync_generator\n",
    )
    .await;
}

#[tokio::test]
async fn generator_mutations_early_return_preserves_generator_kinds_and_docstrings() {
    let source = "import inspect\ndef identity(value): return value\n@identity\ndef values():\n    \"doc\"; yield 1; yield 2\n";
    assert_change(
        source,
        "generator_return_insert",
        "print(inspect.isgeneratorfunction(values)); print(values.__doc__); print(list(values()))",
        "True\ndoc\n[1, 2]\n",
        "True\ndoc\n[]\n",
    )
    .await;
    let source = "import inspect\ndef values():\n    def inner(value=(yield 1)): return value\n    return inner\n";
    assert_change(
        source,
        "generator_return_insert",
        "print(inspect.isgeneratorfunction(values)); print(list(values()))",
        "True\n[1]\n",
        "True\n[]\n",
    )
    .await;
    let source = "import inspect, asyncio\nasync def values():\n    \"doc\"\n    yield 1\n    yield 2\nasync def collect():\n    return [x async for x in values()]\n";
    assert_change(source, "generator_return_insert", "print(inspect.isasyncgenfunction(values)); print(values.__doc__); print(asyncio.run(collect()))", "True\ndoc\n[1, 2]\n", "True\ndoc\n[]\n").await;
}

#[tokio::test]
async fn generator_mutations_skip_and_empty_preserve_construction_once() {
    let source = "calls = []\nclass Values:\n    def __iter__(self):\n        calls.append('iter')\n        return iter((10, 20))\ndef factory():\n    calls.append('factory')\n    return Values()\ndef convert(x):\n    calls.append(x)\n    return x\nvalues = (convert(x) for x in factory())\n";
    let harness = "print(calls.copy()); print(list(values)); print(calls)";
    let baseline = "['factory', 'iter']\n[10, 20]\n['factory', 'iter', 10, 20]\n";
    assert_change(
        source,
        "generator_skip_first",
        harness,
        baseline,
        "['factory', 'iter', 10]\n[20]\n['factory', 'iter', 10, 20]\n",
    )
    .await;
    assert_change(
        source,
        "generator_empty",
        harness,
        baseline,
        "['factory', 'iter']\n[]\n['factory', 'iter']\n",
    )
    .await;
    assert_change(
        "def factory(): return []\nvalues = (x for x in factory())\n",
        "generator_skip_first",
        "print(list(values))",
        "[]\n",
        "[]\n",
    )
    .await;
    let source = "values = list(x for next in (10, 20) for x in (y for y in (1, 2)))\n";
    assert_change(
        source,
        "generator_skip_first",
        "print(values)",
        "[1, 2, 1, 2]\n",
        "[2, 1, 2]\n",
    )
    .await;
    let source = "import asyncio\ncalls = []\nasync def items():\n    calls.append('factory')\n    return [1, 2]\nasync def collect():\n    return (x for x in await items())\n";
    assert_change(
        source,
        "generator_skip_first",
        "print(list(asyncio.run(collect()))); print(calls)",
        "[1, 2]\n['factory']\n",
        "[2]\n['factory']\n",
    )
    .await;
}

#[tokio::test]
async fn generator_mutations_python315_unpacking_is_retained() {
    let version = python_observe(&[b"import sys".to_vec()], "print(sys.version_info.minor)")
        .await
        .unwrap();
    if version[0]["stdout"] == "14\n" {
        eprintln!("Python3.15-only unpacking fixture: skipped on3.14");
        return;
    }
    assert_eq!(version[0]["stdout"], "15\n");
    assert_change(
        "values = [*chunk for chunk in ((1, 2), (3, 4))]\n",
        "list_comprehension_to_generator",
        "print(type(values).__name__); print(list(values))",
        "list\n[1, 2, 3, 4]\n",
        "generator\n[1, 2, 3, 4]\n",
    )
    .await;
}

#[tokio::test]
async fn generator_mutations_skip_uses_construction_scope_builtin() {
    assert_change(
        "calls = []\ndef custom_next(*args):\n    calls.append('custom')\ndef outer(next):\n    class C:\n        global next\n        values = (x for x in (1, 2))\n    return C.values\n",
        "generator_skip_first",
        "print(list(outer(custom_next))); print(calls)",
        "[1, 2]\n[]\n",
        "[2]\n[]\n",
    ).await;
}
