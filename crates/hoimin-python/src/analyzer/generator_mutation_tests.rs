use super::{
    AnalyzeRequest, AnalyzerCandidate, LineRange, MutationProfile, analyze_selected,
    analyze_selected_with, analyze_source_cancellable, parse_module,
};
use camino::Utf8Path;

const OPERATORS: &[&str] = &[
    "list_comprehension_to_generator",
    "generator_return_insert",
    "generator_skip_first",
    "generator_empty",
];

fn apply(source: &str, candidate: &AnalyzerCandidate) -> String {
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[start..end], candidate.original);
    let result = format!(
        "{}{}{}",
        &source[..start],
        candidate.replacement,
        &source[end..]
    );
    assert!(parse_module(&result).is_ok(), "{result}");
    result
}

#[test]
fn list_comprehension_becomes_a_generator_with_original_interior() {
    for (source, expected) in [
        (
            "result = [x * 2 for x in items if x]\n",
            "result = (x * 2 for x in items if x)\n",
        ),
        ("result = [x for x in []]\n", "result = (x for x in [])\n"),
        (
            "result = ([x for x in items])\n",
            "result = ((x for x in items))\n",
        ),
        (
            "async def f(items):\n    return [x async for x in items]\n",
            "async def f(items):\n    return (x async for x in items)\n",
        ),
    ] {
        let output = analyze_selected(&["list_comprehension_to_generator"], source);
        assert_eq!(output.candidates.len(), 1);
        assert_eq!(apply(source, &output.candidates[0]), expected);
    }
}

#[test]
fn generator_return_retains_yield_docstrings_and_async_kind() {
    for (source, expected) in [
        (
            "def values():\n    \"doc\"\n    yield 1\n",
            "def values():\n    \"doc\"\n    return\n    yield 1\n",
        ),
        (
            "async def values(): yield 1\n",
            "async def values(): return; yield 1\n",
        ),
        (
            "def values():\n    \"doc\"; yield 1\n",
            "def values():\n    \"doc\"; return; yield 1\n",
        ),
        (
            "@decorate\ndef values(): \\\n    yield 1\n",
            "@decorate\ndef values(): \\\n    return; yield 1\n",
        ),
        (
            "def values():\n    def inner(value=(yield 1)): pass\n",
            "def values():\n    return\n    def inner(value=(yield 1)): pass\n",
        ),
        (
            "def values():\r    yield 1",
            "def values():\r    return\r    yield 1",
        ),
    ] {
        let output = analyze_selected(&["generator_return_insert"], source);
        assert_eq!(output.candidates.len(), 1, "{source}");
        assert_eq!(apply(source, &output.candidates[0]), expected);
        assert_eq!(output.candidates[0].symbol.as_deref(), Some("values"));
    }
    let source = "def outer():\n    def inner(): yield 1\n    return inner\n";
    let output = analyze_selected(&["generator_return_insert"], source);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("outer.inner"));
    assert_eq!(
        analyze_selected(
            &["generator_return_insert"],
            "def values(): return 0; yield 1\n"
        )
        .candidates
        .len(),
        0
    );
}

#[test]
fn generator_wrappers_support_implicit_arguments_and_known_empty_exclusions() {
    for source in [
        "result = consume(x for x in items)\n",
        "result = (値 for 値 in items)\n",
        "result = (x # comment\r\n          for x in items)\r\n",
        "async def f():\n    return (x for x in await items())\n",
    ] {
        let output = analyze_selected(&["generator_skip_first", "generator_empty"], source);
        assert_eq!(output.candidates.len(), 2, "{source}");
        for candidate in &output.candidates {
            apply(source, candidate);
        }
    }
    for source in ["result = (x for x in ())\n", "result = (x for x in [])\n"] {
        assert_eq!(
            analyze_selected(&["generator_skip_first", "generator_empty"], source)
                .candidates
                .len(),
            0
        );
    }
}

#[test]
fn next_proof_uses_parent_scope_after_construction_without_name_collision() {
    for source in [
        "result = consume(list(xs) for list in factories)\n",
        "result = consume(next(x) for next in functions)\n",
    ] {
        let output = analyze_selected(&["generator_skip_first", "collection_list_tuple"], source);
        assert_eq!(output.candidates.len(), 1, "{source}");
        assert_eq!(
            output.candidates[0].operator.as_str(),
            "generator_skip_first"
        );
        apply(source, &output.candidates[0]);
    }

    let source = "result = consume(x for next in callbacks for x in (y for y in values))\n";
    let output = analyze_selected(&["generator_skip_first"], source);
    assert_eq!(output.candidates.len(), 1);
    assert!(
        output.candidates[0]
            .original
            .contains("for next in callbacks")
    );

    for source in [
        "next = custom\nresult = (x for x in items)\n",
        "ｎｅｘｔ = custom\nresult = (x for x in items)\n",
        "result = ((next := x) for x in items)\n",
        "def f(next, items): return (x for x in items)\n",
        "def f[next](items): return (x for x in items)\n",
        "from provider import *\nresult = (x for x in items)\n",
        "exec(code)\nresult = (x for x in items)\n",
        "for item in rounds:\n    result = (x for x in values)\n    next = custom\n",
    ] {
        assert_eq!(
            analyze_selected(&["generator_skip_first"], source)
                .candidates
                .len(),
            0,
            "{source}"
        );
    }
}

#[test]
fn wrappers_distinguish_nested_async_frames() {
    for source in [
        "async def f(items): return (x async for x in items)\n",
        "async def f(items): return (await convert(x) for x in items)\n",
        "async def f(items, values): return ([x async for x in items] for y in values)\n",
    ] {
        assert_eq!(
            analyze_selected(&["generator_skip_first", "generator_empty"], source)
                .candidates
                .len(),
            0,
            "{source}"
        );
    }
    let source = "async def f(items, values): return ((x async for x in items) for y in values)\n";
    let output = analyze_selected(&["generator_skip_first", "generator_empty"], source);
    assert_eq!(output.candidates.len(), 2);
    for candidate in &output.candidates {
        apply(source, candidate);
    }
}

#[test]
fn generator_candidates_preserve_every_prefix_selection_and_payload_limit() {
    let source = "def values(items):\n    yield [x for x in items]\n    yield (x for x in items)\n";
    let all = analyze_selected(OPERATORS, source);
    assert_eq!(all.candidates.len(), 4);
    for cap in 0..=all.candidates.len() {
        let prefix = analyze_selected_with(
            OPERATORS,
            Utf8Path::new("pkg/sample.py"),
            &[],
            &[],
            cap,
            source,
        );
        assert_eq!(prefix.candidates, all.candidates[..cap]);
        assert_eq!(prefix.truncated, cap < all.candidates.len());
    }
    let chosen = analyze_selected_with(
        OPERATORS,
        Utf8Path::new("pkg/sample.py"),
        &[LineRange { start: 3, end: 3 }],
        &["values".to_owned()],
        10,
        source,
    );
    assert_eq!(chosen.candidates.len(), 2);
    let operators = serde_json::from_value(serde_json::json!(OPERATORS)).unwrap();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::new(1),
        path: Utf8Path::new("pkg/sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
    assert!(analyze_source_cancellable(&request, source, || false).is_err());
}
