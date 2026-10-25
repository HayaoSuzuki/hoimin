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
#[test]
fn none_truthiness_preserves_bool_operands_comments_and_parentheses() {
    for (source, expected) in [
        ("result = x is None\n", "result = (not (x  ))\n"),
        ("result = None is x\n", "result = (not (  x))\n"),
        ("result = x is not None\n", "result = (not not (x   ))\n"),
        ("result = None is not x\n", "result = (not not (   x))\n"),
    ] {
        let out = analyze_selected(&[OPS[0]], source);
        assert_eq!(out.candidates.len(), 1, "{source}");
        assert_eq!(apply(source, &out.candidates[0]), expected);
    }
    for source in [
        "result = (x # keep\n is None)\n",
        "result = (None # keep\n is not (x))\n",
        "result = (next(values) is None)\n",
        "result = (x := value) is not None\n",
    ] {
        let out = analyze_selected(&[OPS[0]], source);
        assert_eq!(out.candidates.len(), 1, "{source}");
        let mutant = apply(source, &out.candidates[0]);
        if source.contains("# keep") {
            assert!(mutant.contains("# keep"));
        }
    }
    for source in [
        "result = x is None is y\n",
        "result = x == None\n",
        "result = None is None\n",
        "x: (int if value is None else str)\n",
    ] {
        assert!(
            analyze_selected(&[OPS[0]], source).candidates.is_empty(),
            "{source}"
        );
    }
}
#[test]
fn mutable_default_matches_only_entry_literal_initialization() {
    for source in [
        "def f(x=None):\n    if x is None: x=[]\n    return x\n",
        "def f(*, x=None):\n    'doc'\n    if None is x: x={}\n    return x\n",
        "async def f(x=None, /):\n    if x is None: x=[]\n    return x\n",
        "class C:\n    def __init__(self, x=None):\n        if x is None: x=[]\n        self.x=x\n",
        "def f(ｘ=None):\n    if x is None: x=[]\n    return x\n",
    ] {
        let out = analyze_selected(&[OPS[1]], source);
        assert_eq!(out.candidates.len(), 1, "{source}");
        let c = &out.candidates[0];
        assert_eq!(c.original, "None");
        assert!(matches!(c.replacement.as_str(), "[]" | "{}"));
        apply(source, c);
    }
    for source in [
        "def f(x=()):\n    if x is None: x=[]\n",
        "def f(x=None):\n    x=1\n    if x is None: x=[]\n",
        "def f(x=None):\n    if x is None: x=[1]\n",
        "def f(x=None):\n    if x is None: x=[]\n    else: x={}\n",
        "def f(x=None):\n    if x is None: x=y=[]\n",
        "def f(x=None):\n    if x is None: x=[]; print(x)\n",
        "@deco\ndef f(x=None):\n    if x is None: x=[]\n",
        "def f[T](x=None):\n    if x is None: x=[]\n",
    ] {
        assert!(
            analyze_selected(&[OPS[1]], source).candidates.is_empty(),
            "{source}"
        );
    }
}
#[test]
fn nonlocal_remove_preserves_inline_and_multiname_layout() {
    for source in [
        "def outer():\n    x=0\n    def inner():\n        nonlocal x # keep\n        x=1\n",
        "def outer():\n    x=y=0\n    def inner(): nonlocal x, y; x=y=1\n",
    ] {
        for newline in ["\n", "\r\n", "\r"] {
            let source = source.replace('\n', newline);
            let out = analyze_selected(&[OPS[2]], &source);
            assert_eq!(out.candidates.len(), 1);
            let c = &out.candidates[0];
            assert_eq!(c.replacement, "pass");
            let mutant = apply(&source, c);
            assert_eq!(source.contains("# keep"), mutant.contains("# keep"));
        }
    }
    assert_eq!(
        analyze_selected(&[OPS[2]], "def f():\n    global x\n    x=1\n")
            .candidates
            .len(),
        0
    );
}
#[test]
fn yield_delegation_changes_only_the_from_keyword() {
    for source in [
        "def f(): yield from (1, 2)\n",
        "def f():\n    yield from (\n        # keep\n        x for x in items\n    )\n",
        "def f():\n    result = (yield from (yield from items))\n",
        "def f():\n    yield \\\n        from \\\n        values\n",
    ] {
        let out = analyze_selected(&[OPS[3]], source);
        assert!(!out.candidates.is_empty(), "{source}");
        for c in &out.candidates {
            assert_eq!(c.original, "from");
            assert_eq!(c.replacement, "");
            apply(source, c);
        }
    }
}
#[test]
fn setdefault_rename_preserves_call_source_and_rejects_splats() {
    for source in [
        "result = d.setdefault(key)\n",
        "d.setdefault(key, [])\n",
        "result = holder.data.setdefault(\n    key, # keep\n    (value := factory()),\n)\n",
    ] {
        let out = analyze_selected(&[OPS[4]], source);
        assert_eq!(out.candidates.len(), 1, "{source}");
        let c = &out.candidates[0];
        assert_eq!(c.original, "setdefault");
        assert_eq!(c.replacement, "get");
        let mutant = apply(source, c);
        assert_eq!(source.replace("setdefault", "get"), mutant);
    }
    for source in [
        "d.setdefault()\n",
        "d.setdefault(a,b,c)\n",
        "d.setdefault(*args)\n",
        "d.setdefault(k, **kwargs)\n",
        "d.setdefault(key=k, default=[])\n",
        "factory().setdefault(k,[])\n",
        "items[0].setdefault(k,[])\n",
    ] {
        assert!(
            analyze_selected(&[OPS[4]], source).candidates.is_empty(),
            "{source}"
        );
    }
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

#[test]
fn none_truthiness_removes_none_side_parenthesis_groups() {
    for source in [
        "result = value is (None)\n",
        "result = (None) is value\n",
        "result = value is not ((None))\n",
        "result = ((None)) is not value\n",
        "result = (value is (\n # keep\n None\n))\n",
        "result = ((\n None # keep\n) is value)\n",
    ] {
        let out = analyze_selected(&[OPS[0]], source);
        assert_eq!(out.candidates.len(), 1);
        let mutant = apply(source, &out.candidates[0]);
        assert_eq!(source.contains("# keep"), mutant.contains("# keep"));
    }
}

#[test]
fn effective_python_selects_changed_tokens_on_continuation_lines() {
    for (op, source, line, original) in [
        (
            "yield_from_to_yield",
            "def f():\n    yield \\\n        from \\\n        values\n",
            3,
            "from",
        ),
        (
            "mapping_setdefault_to_get",
            "def f():\n    return d.\\\n        setdefault('x', [])\n",
            3,
            "setdefault",
        ),
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
}
