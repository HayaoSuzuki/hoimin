use super::*;

fn nested_source(depth: usize) -> String {
    let mut source = String::from("value: Sequence[int]\n");
    for _ in 0..depth {
        let mut outer = String::from("try:\n    pass\nfinally:\n");
        for line in source.lines() {
            outer.push_str("    ");
            outer.push_str(line);
            outer.push('\n');
        }
        source = outer;
    }
    source
}

fn selected_operator(selected: bool) -> MutationOperatorSelection {
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        for operator in MutationOperatorSelection::parse_selector(&name).unwrap() {
            operators.exclude(operator);
        }
    }
    operators.include(if selected {
        MutationOperator::TypeListSequence
    } else {
        MutationOperator::BooleanLiteral
    });
    operators
}

fn analyze(source: &str, selected: bool) -> (Vec<AnalyzerCandidate>, usize, usize) {
    LOOP_STATEMENT_VISITS.set(0);
    LOOP_ANNOTATION_VISITS.set(0);
    let output = analyze_source(
        &AnalyzeRequest {
            path: Utf8Path::new("nested.py"),
            lines: &[],
            symbols: &[],
            operators: &selected_operator(selected),
            profile: MutationProfile::Full,
            max_candidates: 100,
        },
        source,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(!output.truncated);
    (
        output.candidates,
        LOOP_STATEMENT_VISITS.get(),
        LOOP_ANNOTATION_VISITS.get(),
    )
}

#[test]
fn nested_finally_bounds_actual_statement_and_annotation_visits() {
    for wrapper in ["", "class C:\n", "def f():\n"] {
        for depth in [1, 2, 4, 8, 16, 17, 18, 19, 20] {
            let mut source = String::from("from typing import Sequence\n");
            source.push_str(wrapper);
            for line in nested_source(depth).lines() {
                if !wrapper.is_empty() {
                    source.push_str("    ");
                }
                source.push_str(line);
                source.push('\n');
            }
            let (unselected, statements, leaves) = analyze(&source, false);
            assert_eq!(unselected, Vec::new());
            assert_eq!((statements, leaves), (0, 0));
            let (candidates, statements, leaves) = analyze(&source, true);
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].original, "Sequence[int]");
            assert_eq!(candidates[0].replacement, "list[int]");
            eprintln!("wrapper={wrapper:?} depth={depth} statements={statements} leaves={leaves}");
            assert!(leaves <= depth + 1, "depth={depth}: {leaves} leaf visits");
            assert!(
                statements <= (depth + 1).pow(2) + 1 + usize::from(!wrapper.is_empty()),
                "depth={depth}: {statements} statement visits"
            );
        }
    }
}

#[test]
fn record_disabled_finally_visits_leaf_once() {
    for depth in [1, 2, 4, 8, 20] {
        let source = nested_source(depth);
        let parsed = parse_module(&source).unwrap();
        let mut collector = AnnotationCollector::empty();
        collector.record_annotations = false;
        LOOP_STATEMENT_VISITS.set(0);
        LOOP_ANNOTATION_VISITS.set(0);
        collector.visit_suite(&parsed.syntax().body);
        assert!(collector.annotations.is_empty());
        assert_eq!(LOOP_ANNOTATION_VISITS.get(), 1, "depth={depth}");
        assert_eq!(LOOP_STATEMENT_VISITS.get(), 2 * depth + 1, "depth={depth}");
    }
}

struct OldRecordingGuard(bool);

impl OldRecordingGuard {
    fn enabled(enabled: bool) -> Self {
        Self(REDUNDANT_FINALLY_RECORDING.replace(enabled))
    }
}

impl Drop for OldRecordingGuard {
    fn drop(&mut self) {
        REDUNDANT_FINALLY_RECORDING.set(self.0);
    }
}

#[test]
fn old_finally_replay_exceeds_bound_with_identical_candidates() {
    let depth = 8;
    let source = format!(
        "from typing import Sequence\nbefore: Sequence[int]\n{}after: Sequence[int]\n",
        nested_source(depth)
    );
    let current = analyze(&source, true);
    let _guard = OldRecordingGuard::enabled(true);
    let old = analyze(&source, true);
    assert_eq!(current.0, old.0);
    assert_eq!(current.0.len(), 3);
    assert!(
        current
            .0
            .windows(2)
            .all(|pair| pair[0].span.start < pair[1].span.start)
    );
    assert!(current.1 <= (depth + 1).pow(2) + 3);
    assert!(old.1 > (depth + 1).pow(2) + 3);
    assert_eq!(old.2, (1 << depth) + 2);
}

type AnnotationObservation = (TextRange, Option<String>, Vec<String>);

#[derive(Debug, Eq, PartialEq)]
struct FlowObservation {
    annotations: Vec<AnnotationObservation>,
    explicit: BindingFlowTestSnapshot,
    raises: Vec<Vec<String>>,
    implicit: Option<Vec<String>>,
    fallback: Option<Vec<String>>,
}

fn observe(source: &str, old: bool) -> FlowObservation {
    let _guard = OldRecordingGuard::enabled(old);
    let parsed = parse_module(source).expect("valid fixture");
    let mut annotations = Vec::new();
    let mut callback = |annotation: &Expr, symbol, imports: &KnownImports| {
        annotations.push((
            annotation.range(),
            symbol,
            normalize_binding_flow_imports(imports),
        ));
    };
    let mut collector = AnnotationCollector::empty();
    collector.annotation_callback = Some(&mut callback);
    let exits = collector.visit_suite_flow(&parsed.syntax().body);
    let fallback = collector
        .class_body_fallback
        .as_ref()
        .map(normalize_binding_flow_imports);
    FlowObservation {
        annotations,
        explicit: normalize_binding_flow_exits(&exits),
        raises: normalize_binding_flow_states(&exits.raises),
        implicit: exits
            .implicit_raises
            .as_ref()
            .map(normalize_binding_flow_imports),
        fallback,
    }
}

#[test]
fn finally_routing_preserves_callbacks_and_all_exit_categories() {
    let cases = [
        "from typing import Sequence\ntry:\n if a:\n  break\n elif b:\n  continue\n elif c:\n  return\n elif d:\n  raise Error\n hazard()\nfinally:\n try:\n  pass\n finally:\n  value: Sequence[int]\nafter: Sequence[int]\n",
        "from typing import Sequence\ntry:\n hazard()\n Sequence = unknown\nfinally:\n try:\n  pass\n finally:\n  value: Sequence[int]\nafter: Sequence[int]\n",
        "from typing import Sequence\nclass C:\n global Sequence\n try:\n  pass\n finally:\n  try:\n   pass\n  finally:\n   Sequence = unknown\n   def f(self, arg: Sequence[int]):\n    local: Sequence[int]\n   from typing import Sequence\n after: Sequence[int]\n",
        "from typing import Sequence\ntry:\n return\nfinally:\n try:\n  pass\n finally:\n  class C:\n   def f(self, arg: Sequence[int]):\n    local: Sequence[int]\n",
    ];
    for source in cases {
        let current = observe(source, false);
        assert_eq!(current, observe(source, true), "{source}");
        assert_ne!(current.annotations, Vec::new());
        assert_eq!(analyze(source, true).0, {
            let _guard = OldRecordingGuard::enabled(true);
            analyze(source, true).0
        });
    }
    let stable = observe(cases[0], false);
    assert!(stable.annotations.iter().all(|(_, _, facts)| {
        facts
            .iter()
            .any(|fact| fact == "direct:Sequence=typing.Sequence")
    }));
    let mixed = observe(cases[1], false);
    assert!(
        !mixed.annotations[0]
            .2
            .iter()
            .any(|fact| fact == "direct:Sequence=typing.Sequence")
    );

    // A nested abrupt finally overrides each pending exit, including implicit
    // exceptions; assert categories independently of the old implementation.
    for pending in [
        "pass",
        "break",
        "continue",
        "return",
        "raise Error",
        "hazard()",
    ] {
        for replacement in ["break", "continue", "return", "raise Error"] {
            let source =
                format!("try:\n {pending}\nfinally:\n try:\n  pass\n finally:\n  {replacement}\n");
            let current = observe(&source, false);
            assert_eq!(current, observe(&source, true), "{source}");
            assert!(current.explicit.fallthrough.is_empty(), "{source}");
            if replacement != "raise Error" {
                assert!(
                    current.implicit.is_none(),
                    "pending exception survived: {source}"
                );
            }
            assert_eq!(
                !current.explicit.breaks.is_empty(),
                replacement == "break",
                "{source}"
            );
            assert_eq!(
                !current.explicit.continues.is_empty(),
                replacement == "continue",
                "{source}"
            );
            assert_eq!(
                !current.raises.is_empty(),
                replacement == "raise Error",
                "{source}"
            );
            // The legacy snapshot combines explicit raises with returns.
            assert_eq!(
                !current.explicit.terminates.is_empty(),
                matches!(replacement, "return" | "raise Error"),
                "{source}"
            );
        }
    }
}

#[test]
fn empty_entry_finally_records_only_when_enabled() {
    let parsed = parse_module("value: Sequence[int]\n").unwrap();
    for record in [false, true] {
        let mut collector = AnnotationCollector::empty();
        collector.record_annotations = record;
        LOOP_STATEMENT_VISITS.set(0);
        LOOP_ANNOTATION_VISITS.set(0);
        let exits = collector.apply_finally(ControlFlowExits::default(), &parsed.syntax().body);
        assert_eq!(LOOP_ANNOTATION_VISITS.get(), usize::from(record));
        assert_eq!(collector.annotations.len(), usize::from(record));
        assert!(exits.fallthrough.is_none());
        assert!(
            exits.breaks.is_empty()
                && exits.continues.is_empty()
                && exits.terminates.is_empty()
                && exits.raises.is_empty()
                && exits.implicit_raises.is_none()
        );
    }
}
