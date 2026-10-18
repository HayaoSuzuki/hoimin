use super::*;
use std::fmt::Write as _;

fn multi_exit_source(depth: usize, leaf: &str) -> String {
    let mut source = String::from("def f():\n");
    for level in 0..depth {
        let indent = "    ".repeat(level + 1);
        writeln!(
            source,
            "{indent}try:\n{indent}    if c:\n{indent}        return\n{indent}finally:"
        )
        .unwrap();
    }
    writeln!(source, "{}{leaf}", "    ".repeat(depth + 1)).unwrap();
    source
}

#[test]
fn multi_exit_transfer_only_has_linear_statement_work() {
    for depth in [1usize, 2, 4, 8, 12, 20] {
        // Arrange: the reported homogeneous normal/return/implicit-exception family.
        let source = multi_exit_source(depth, "x: int");
        let parsed = parse_module(&source).unwrap();
        let mut collector = AnnotationCollector::empty();
        collector.record_annotations = false;
        LOOP_STATEMENT_VISITS.set(0);
        LOOP_ANNOTATION_VISITS.set(0);

        // Act.
        collector.visit_suite(&parsed.syntax().body);

        // Assert: actual visits, independently of transfer bookkeeping.
        assert_eq!(LOOP_ANNOTATION_VISITS.get(), 1, "depth={depth}");
        assert!(
            LOOP_STATEMENT_VISITS.get() <= 3 * depth + 2,
            "depth={depth}: {} visits",
            LOOP_STATEMENT_VISITS.get()
        );
    }
}

#[test]
fn annotation_free_analysis_cancels_during_finally_transfer() {
    // Arrange: cancellation becomes true only after actual annotation-collector work.
    let source = multi_exit_source(5, "pass");
    let operators = selected_operator(true);
    let request = AnalyzeRequest {
        payload: super::payload::PayloadBudget::default(),
        path: Utf8Path::new("cancel.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    LOOP_STATEMENT_VISITS.set(0);

    // Act.
    let result =
        analyze_source_cancellable(&request, &source, || LOOP_STATEMENT_VISITS.get() >= 20);

    // Assert: no successful partial output, and no continued walk after cancellation.
    assert!(matches!(result, Err(AnalysisError::Cancelled)));
    assert!(
        LOOP_STATEMENT_VISITS.get() <= 20,
        "{} visits",
        LOOP_STATEMENT_VISITS.get()
    );
}

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
            payload: super::payload::PayloadBudget::default(),
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
        output
            .candidates
            .into_iter()
            .map(super::payload::Charged::into_untracked)
            .collect(),
        LOOP_STATEMENT_VISITS.get(),
        LOOP_ANNOTATION_VISITS.get(),
    )
}

#[test]
fn nested_finally_bounds_actual_statement_and_annotation_visits() {
    for (wrapper, indent) in [("", ""), ("class C:\n", "    "), ("def f():\n", "    ")] {
        for depth in [1, 2, 4, 8, 16, 17, 18, 19, 20] {
            let mut source = String::from("from typing import Sequence\n");
            source.push_str(wrapper);
            for line in nested_source(depth).lines() {
                source.push_str(indent);
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
    returns: Vec<Vec<String>>,
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
        .class_context
        .as_ref()
        .map(|context| normalize_binding_flow_imports(&context.fallback));
    let mut observation = FlowObservation {
        annotations,
        explicit: normalize_binding_flow_exits(&exits),
        raises: normalize_binding_flow_states(&exits.raises),
        returns: normalize_binding_flow_states(&exits.terminates),
        implicit: exits
            .implicit_raises
            .as_ref()
            .map(normalize_binding_flow_imports),
        fallback,
    };
    // Multiplicity is measured by separate vector-size gates. Semantic observations
    // compare exact environments as sets, keeping return and raise distinct.
    observation.explicit.breaks.dedup();
    observation.explicit.continues.dedup();
    observation.explicit.terminates.dedup();
    observation.raises.dedup();
    observation.returns.dedup();
    observation
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
    for pending in [
        "pass",
        "break",
        "continue",
        "return",
        "raise Error",
        "hazard()",
    ] {
        for replacement in ["break", "continue", "return"] {
            let source =
                format!("try:\n {pending}\nfinally:\n try:\n  pass\n finally:\n  {replacement}\n");

            let current = observe(&source, false);

            assert!(
                current.implicit.is_none(),
                "pending exception survived: {source}"
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

struct SharingGuard(bool);

impl SharingGuard {
    fn disabled() -> Self {
        Self(finally_work::DISABLED.replace(true))
    }
}

impl Drop for SharingGuard {
    fn drop(&mut self) {
        finally_work::DISABLED.set(self.0);
    }
}

fn reset_finally_work() {
    finally_work::MISSES.set(0);
    finally_work::HITS.set(0);
    finally_work::MAX_VECTOR.set(0);
    LOOP_STATEMENT_VISITS.set(0);
    LOOP_ANNOTATION_VISITS.set(0);
}

#[test]
fn multi_exit_recording_has_quadratic_work_and_constant_vectors() {
    for leaf in ["x: int", "pass"] {
        for depth in [1usize, 2, 4, 8, 12, 20] {
            // Arrange.
            let source = multi_exit_source(depth, leaf);
            let parsed = parse_module(&source).unwrap();
            let mut collector = AnnotationCollector::empty();
            reset_finally_work();

            // Act.
            collector.visit_suite(&parsed.syntax().body);

            // Assert: the actual raw vector peak is measured before normalization.
            let statements = LOOP_STATEMENT_VISITS.get();
            let leaves = LOOP_ANNOTATION_VISITS.get();
            let misses = finally_work::MISSES.get();
            let hits = finally_work::HITS.get();
            let peak = finally_work::MAX_VECTOR.get();
            eprintln!(
                "multi record depth={depth} leaf={leaf:?} statements={statements} leaves={leaves} misses={misses} hits={hits} max_vector={peak}"
            );
            assert!(statements <= 3 * (depth + 1).pow(2) + 2);
            assert_eq!(leaves, if leaf == "pass" { 0 } else { depth + 1 });
            assert_eq!(collector.annotations.len(), usize::from(leaf != "pass"));
            assert!(misses <= depth * (depth + 1) / 2);
            assert!(hits > 0);
            assert!(peak <= 4);
        }
    }
}

#[test]
fn multi_exit_transfer_vectors_and_misses_are_bounded() {
    for depth in [1usize, 2, 4, 8, 12, 20] {
        // Arrange: inspect the function body exits instead of swallowing them at def.
        let source = multi_exit_source(depth, "x: int");
        let parsed = parse_module(&source).unwrap();
        let Stmt::FunctionDef(definition) = &parsed.syntax().body[0] else {
            panic!()
        };
        let mut collector = AnnotationCollector::empty();
        collector.record_annotations = false;
        reset_finally_work();

        // Act.
        let exits = collector.visit_suite_flow(&definition.body);

        // Assert.
        eprintln!(
            "multi transfer depth={depth} statements={} leaves={} misses={} hits={} max_vector={}",
            LOOP_STATEMENT_VISITS.get(),
            LOOP_ANNOTATION_VISITS.get(),
            finally_work::MISSES.get(),
            finally_work::HITS.get(),
            finally_work::MAX_VECTOR.get()
        );
        assert_eq!(finally_work::MISSES.get(), depth);
        assert_eq!(finally_work::HITS.get(), 2 * depth);
        assert_eq!(LOOP_STATEMENT_VISITS.get(), 3 * depth + 1);
        assert!(finally_work::MAX_VECTOR.get() <= 4);
        assert_eq!(exits.terminates.len(), 1);
        assert!(exits.breaks.is_empty() && exits.continues.is_empty() && exits.raises.is_empty());
        assert!(exits.fallthrough.is_some() && exits.implicit_raises.is_some());
    }
}

#[test]
fn small_unoptimized_reference_exceeds_work_bound_with_identical_candidates() {
    for depth in [1usize, 2, 4] {
        // Arrange.
        let source = multi_exit_source(depth, "x: Sequence[int]");
        let source = format!("from typing import Sequence\n{source}");
        reset_finally_work();

        // Act.
        let current = analyze(&source, true);
        let hits = finally_work::HITS.get();
        let _guard = SharingGuard::disabled();
        let reference = analyze(&source, true);

        // Assert: complete descriptors include ID, span, replacement, symbol and order.
        assert_eq!(current.0, reference.0);
        assert_eq!(current.0.len(), 1);
        assert!(hits > 0);
        assert!(reference.1 > current.1);
        assert!(reference.2 > depth + 1);
    }
}

fn membership(exits: &ControlFlowExits) -> [bool; 6] {
    [
        exits.fallthrough.is_some(),
        !exits.breaks.is_empty(),
        !exits.continues.is_empty(),
        !exits.terminates.is_empty(),
        !exits.raises.is_empty(),
        exits.implicit_raises.is_some(),
    ]
}

fn pending_exit(category: usize) -> ControlFlowExits {
    let mut exits = ControlFlowExits::default();
    let imports = KnownImports::default();
    match category {
        0 => exits.fallthrough = Some(imports),
        1 => exits.breaks.push(imports),
        2 => exits.continues.push(imports),
        3 => exits.terminates.push(imports),
        4 => exits.raises.push(imports),
        5 => exits.implicit_raises = Some(imports),
        _ => unreachable!(),
    }
    exits
}

#[test]
fn finally_category_table_preserves_or_overrides_each_pending_completion() {
    for pending in 0..6 {
        for (replacement, body) in ["pass", "break", "continue", "return", "raise"]
            .iter()
            .enumerate()
        {
            // Arrange: no ambient implicit tracking; test the routing law independently.
            let parsed = parse_module(body).unwrap();
            let mut collector = AnnotationCollector::empty();
            collector.record_annotations = false;
            let mut expected = [false; 6];
            expected[if replacement == 0 {
                pending
            } else {
                replacement
            }] = true;

            // Act.
            let actual = collector.apply_finally(pending_exit(pending), &parsed.syntax().body);

            // Assert: successful returns and explicit/implicit raises stay separate.
            assert_eq!(
                membership(&actual),
                expected,
                "pending={pending}, finally={body}"
            );
        }
    }
}

#[test]
fn identical_imports_hit_across_all_pending_categories_without_flattening() {
    // Arrange.
    let parsed = parse_module("pass").unwrap();
    let mut entries = pending_exit(0);
    for category in 1..6 {
        entries.merge_abrupt(pending_exit(category));
    }
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    reset_finally_work();

    // Act.
    let actual = collector.apply_finally(entries, &parsed.syntax().body);

    // Assert.
    assert_eq!(membership(&actual), [true; 6]);
    assert_eq!(
        (finally_work::MISSES.get(), finally_work::HITS.get()),
        (1, 5)
    );
}

#[test]
fn distinct_typed_imports_are_not_deduplicated_or_reused() {
    let bindings = [
        KnownBinding::Direct("typing.Sequence".into()),
        KnownBinding::Direct("typing.Mapping".into()),
        KnownBinding::Module("typing".into()),
        KnownBinding::Module("collections.abc".into()),
        KnownBinding::TypeVar,
    ];
    for first in &bindings {
        for second in &bindings {
            if first == second {
                continue;
            }
            // Arrange: A/B/A tests one-slot eviction as well as exact typed keys.
            let a = KnownImports {
                bindings: HashMap::from([("T".into(), first.clone())]),
            };
            let b = KnownImports {
                bindings: HashMap::from([("T".into(), second.clone())]),
            };
            let parsed = parse_module("pass").unwrap();
            let mut collector = AnnotationCollector::empty();
            collector.record_annotations = false;
            let entries = ControlFlowExits {
                fallthrough: Some(a.clone()),
                breaks: vec![b.clone()],
                continues: vec![a.clone()],
                terminates: vec![a.clone(), b.clone(), a.clone()],
                ..ControlFlowExits::default()
            };
            reset_finally_work();

            // Act.
            let actual = collector.apply_finally(entries, &parsed.syntax().body);

            // Assert: stable exact normalization preserves both distinct return states.
            assert_eq!(actual.fallthrough, Some(a.clone()));
            assert_eq!(actual.breaks, vec![b.clone()]);
            assert_eq!(actual.continues, vec![a.clone()]);
            assert_eq!(actual.terminates, vec![a, b]);
            assert_eq!(
                (finally_work::MISSES.get(), finally_work::HITS.get()),
                (4, 1)
            );
        }
    }
}

#[test]
fn abrupt_cache_hit_restores_residual_imports_for_unreachable_annotations() {
    // Arrange.
    let parsed = parse_module("from typing import Sequence\nreturn\n").unwrap();
    let unreachable = parse_module("after: Sequence[int]\n").unwrap();
    let mut collector = AnnotationCollector::empty();
    let mut transfer = FinallyTransfer::new(&parsed.syntax().body, true);
    reset_finally_work();
    let _ = transfer.evaluate(&mut collector, KnownImports::default());
    collector.imports = KnownImports::default();

    // Act.
    let exits = transfer.evaluate(&mut collector, KnownImports::default());
    let _ = collector.apply_finally(ControlFlowExits::default(), &unreachable.syntax().body);

    // Assert: cached transfer did not emit callbacks; empty-entry scan sees its residual.
    assert_eq!(finally_work::HITS.get(), 1);
    assert_eq!(
        membership(&exits),
        [false, false, false, true, false, false]
    );
    assert_eq!(collector.annotations.len(), 1);
    assert_eq!(
        collector.annotations[0].imports.bindings.get("Sequence"),
        Some(&KnownBinding::Direct("typing.Sequence".into()))
    );
}

#[test]
fn generated_distinct_binding_flows_preserve_production_callbacks_and_candidates() {
    for binding in [
        "from typing import Sequence",
        "import typing as Sequence",
        "Sequence = unknown",
    ] {
        for completion in ["pass", "return", "raise Error", "break", "continue"] {
            // Arrange: multiple annotations and nested scopes give provenance/order evidence.
            let source = format!(
                "from typing import Sequence\nbefore: Sequence[int]\ndef f[T]():\n try:\n  if c:\n   {binding}\n   {completion}\n finally:\n  try:\n   if d:\n    return\n  finally:\n   value: Sequence[int]\n   class C:\n    def method(self, arg: Sequence[int]):\n     local: Sequence[int]\nafter: Sequence[int]\n"
            );
            reset_finally_work();

            // Act.
            let current = observe(&source, false);
            let candidates = analyze(&source, true).0;
            let hits = finally_work::HITS.get();
            let _guard = SharingGuard::disabled();
            let reference = observe(&source, false);
            let reference_candidates = analyze(&source, true).0;

            // Assert: no test observer hooks were installed; these are actual cache hits.
            assert!(hits > 0);
            assert_eq!(current, reference, "{source}");
            assert_eq!(candidates, reference_candidates, "{source}");
            assert!(
                candidates
                    .windows(2)
                    .all(|pair| pair[0].span.start < pair[1].span.start)
            );
        }
    }
}

#[test]
fn class_fallback_mutation_bypasses_reuse_and_normalization() {
    for declaration in ["global Sequence", "nonlocal Sequence"] {
        // Arrange: global/nonlocal projection and method fallback execute in class context.
        let source = format!(
            "from typing import Sequence\ndef outer():\n from typing import Sequence\n class C[T]:\n  {declaration}\n  try:\n   if c:\n    raise Error\n  finally:\n   Sequence = unknown\n   def method(self, arg: Sequence[int]):\n    local: Sequence[int]\n   from typing import Sequence\n  after: Sequence[int]\n"
        );
        reset_finally_work();

        // Act.
        let actual = observe(&source, false);
        let hits = finally_work::HITS.get();
        let _guard = SharingGuard::disabled();
        let reference = observe(&source, false);

        // Assert.
        assert_eq!(actual, reference);
        assert_eq!(hits, 0);
    }
}

#[test]
fn class_context_preserves_duplicate_pending_entries() {
    // Arrange: directly check that the no-class gate covers input AND output normalization.
    let parsed = parse_module("pass").unwrap();
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    collector.class_context = Some(ClassContext {
        fallback: KnownImports::default(),
        external_bindings: ClassExternalBindings::collect(&[]),
        parent_scope: ScopeKind::Module,
    });
    let entries = ControlFlowExits {
        terminates: vec![KnownImports::default(), KnownImports::default()],
        ..ControlFlowExits::default()
    };
    reset_finally_work();

    // Act.
    let actual = collector.apply_finally(entries, &parsed.syntax().body);

    // Assert.
    assert_eq!(actual.terminates.len(), 2);
    assert_eq!(
        (finally_work::MISSES.get(), finally_work::HITS.get()),
        (2, 0)
    );
}

#[test]
fn transfer_only_cancellation_restores_scope_flags_and_stops_statement_work() {
    // Arrange: no annotations; cancellation happens inside a nested transfer, including a loop.
    let parsed = parse_module(
        "class C:\n def f():\n  try:\n   while c:\n    pass\n  finally:\n   pass\npass\n",
    )
    .unwrap();
    let polls = Cell::new(0);
    let cancelled = || {
        polls.set(polls.get() + 1);
        LOOP_STATEMENT_VISITS.get() >= 5
    };
    let mut collector = AnnotationCollector::empty();
    collector.cancelled = Some(&cancelled);
    collector.track_implicit_exceptions = true;
    let mut transfer = FinallyTransfer::new(&parsed.syntax().body, true);
    reset_finally_work();

    // Act.
    let actual = transfer.evaluate(&mut collector, KnownImports::default());
    let stopped_polls = polls.get();
    let second = transfer.evaluate(&mut collector, KnownImports::default());

    // Assert: cancelled partial transfers are empty, unadmitted and never callbacks.
    assert!(collector.cancellation_observed);
    assert_eq!(membership(&actual), [false; 6]);
    assert_eq!(membership(&second), [false; 6]);
    assert_eq!(LOOP_STATEMENT_VISITS.get(), 5);
    assert_eq!(finally_work::HITS.get(), 0);
    assert_eq!(polls.get(), stopped_polls);
    assert!(collector.record_annotations && collector.track_implicit_exceptions);
    assert_eq!(collector.scope_kind, ScopeKind::Module);
    assert!(collector.qualname.is_empty() && collector.class_context.is_none());
}

#[test]
fn cached_finally_entry_still_polls_cancellation() {
    // Arrange.
    let parsed = parse_module("pass").unwrap();
    let stop = Cell::new(false);
    let cancelled = || stop.get();
    let mut collector = AnnotationCollector::empty();
    collector.cancelled = Some(&cancelled);
    let mut transfer = FinallyTransfer::new(&parsed.syntax().body, true);
    reset_finally_work();
    let _ = transfer.evaluate(&mut collector, KnownImports::default());
    stop.set(true);

    // Act.
    let actual = transfer.evaluate(&mut collector, KnownImports::default());

    // Assert.
    assert_eq!(membership(&actual), [false; 6]);
    assert!(collector.cancellation_observed);
    assert_eq!(finally_work::HITS.get(), 0);
    assert_eq!(LOOP_STATEMENT_VISITS.get(), 1);
}

#[test]
fn unselected_type_operators_skip_multi_exit_annotation_work() {
    for depth in [1, 2, 4, 8, 12, 20] {
        // Arrange.
        let source = multi_exit_source(depth, "x: int");
        reset_finally_work();

        // Act.
        let (candidates, statements, annotations) = analyze(&source, false);

        // Assert.
        assert_eq!(candidates, Vec::new());
        assert_eq!((statements, annotations), (0, 0));
        assert_eq!(
            (finally_work::MISSES.get(), finally_work::HITS.get()),
            (0, 0)
        );
    }
}

#[test]
fn observed_payload_failure_keeps_priority_over_later_cancellation() {
    // Arrange: failed candidate generation deterministically arms cancellation.
    let budget = payload::PayloadBudget::new(1);
    let operators = selected_operator(true);
    let request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("payload.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let observed_cancellation = Cell::new(false);

    // Act.
    let result = analyze_source_cancellable(
        &request,
        "from typing import Sequence\nx: Sequence[int]\n",
        || {
            let cancelled = budget.snapshot().generated > 0;
            observed_cancellation.set(observed_cancellation.get() || cancelled);
            cancelled
        },
    );

    // Assert.
    assert!(observed_cancellation.get());
    assert_eq!(result.err(), Some(AnalysisError::PayloadLimit { limit: 1 }));
}

#[test]
fn observer_and_mutation_hooks_bypass_reuse_and_normalization() {
    for hook in 0..4 {
        // Arrange.
        let parsed = parse_module("pass").unwrap();
        let mut collector = AnnotationCollector::empty();
        collector.record_annotations = false;
        match hook {
            0 => {
                collector.marker_projection = Some(BindingFlowMarkerProjection {
                    source: "pass".into(),
                    marker: 0..4,
                    line_start: 0,
                    entry: None,
                });
            }
            1 => {
                collector.handler_exit_projection = Some(BindingFlowHandlerExitProjection {
                    source: "pass".into(),
                    marker: 0..4,
                    line_start: 0,
                    category: ExitCategory::Terminate,
                    matching_handlers: 0,
                    matched_handler: None,
                    states: vec![],
                });
            }
            2 => {
                collector.try_exit_projection = Some(BindingFlowTryExitProjection {
                    marker: 0..4,
                    matching_tries: 0,
                    matched_try: None,
                    exits: None,
                });
            }
            3 => collector.test_mutation = Some(BindingFlowTestMutation::OmitLoopContinueBackEdge),
            _ => unreachable!(),
        }
        let entries = ControlFlowExits {
            terminates: vec![KnownImports::default(), KnownImports::default()],
            ..ControlFlowExits::default()
        };
        reset_finally_work();

        // Act.
        let actual = collector.apply_finally(entries, &parsed.syntax().body);

        // Assert.
        assert_eq!(actual.terminates.len(), 2);
        assert_eq!(
            (finally_work::MISSES.get(), finally_work::HITS.get()),
            (2, 0)
        );
    }
}

#[test]
fn class_finally_bypass_still_cancels_without_annotations() {
    // Arrange: preserve the unshared class path, but stop its repeated transfer work.
    let source = multi_exit_source(5, "pass")
        .replace("def f():", "class C:")
        .replace("return", "raise Error");
    let operators = selected_operator(true);
    let request = AnalyzeRequest {
        payload: payload::PayloadBudget::default(),
        path: Utf8Path::new("class-cancel.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    reset_finally_work();

    // Act.
    let result =
        analyze_source_cancellable(&request, &source, || LOOP_STATEMENT_VISITS.get() >= 20);

    // Assert.
    assert_eq!(result.err(), Some(AnalysisError::Cancelled));
    assert_eq!(LOOP_STATEMENT_VISITS.get(), 20);
    assert_eq!(finally_work::HITS.get(), 0);
}
