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

#[rstest::rstest]
#[case::value_1usize(1usize)]
#[case::value_2(2)]
#[case::value_4(4)]
#[case::value_8(8)]
#[case::value_12(12)]
#[case::value_20(20)]
fn multi_exit_transfer_only_has_linear_statement_work(#[case] depth: usize) {
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

fn wrapped_nested_source(depth: usize, wrapper: &str, indent: &str) -> String {
    let mut source = String::from("from typing import Sequence\n");
    source.push_str(wrapper);
    for line in nested_source(depth).lines() {
        source.push_str(indent);
        source.push_str(line);
        source.push('\n');
    }
    source
}

#[rstest::rstest]
#[case::module("", "")]
#[case::class("class C:\n", "    ")]
#[case::function("def f():\n", "    ")]
fn nested_finally_bounds_actual_statement_and_annotation_visits(
    #[case] wrapper: &str,
    #[case] indent: &str,
    #[values(1, 2, 4, 8, 16, 17, 18, 19, 20)] depth: usize,
) {
    let source = wrapped_nested_source(depth, wrapper, indent);

    let (candidates, statements, leaves) = analyze(&source, true);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].original, "Sequence[int]");
    assert_eq!(candidates[0].replacement, "list[int]");
    assert!(leaves <= depth + 1);
    assert!(statements <= (depth + 1).pow(2) + 1 + usize::from(!wrapper.is_empty()));
}

#[rstest::rstest]
#[case::module("", "")]
#[case::class("class C:\n", "    ")]
#[case::function("def f():\n", "    ")]
fn unselected_nested_finally_skips_statement_and_annotation_visits(
    #[case] wrapper: &str,
    #[case] indent: &str,
    #[values(1, 2, 4, 8, 16, 17, 18, 19, 20)] depth: usize,
) {
    let source = wrapped_nested_source(depth, wrapper, indent);

    let (candidates, statements, leaves) = analyze(&source, false);

    assert_eq!(candidates, Vec::new());
    assert_eq!((statements, leaves), (0, 0));
}

#[rstest::rstest]
#[case::value_1(1)]
#[case::value_2(2)]
#[case::value_4(4)]
#[case::value_8(8)]
#[case::value_20(20)]
fn record_disabled_finally_visits_leaf_once(#[case] depth: usize) {
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

#[rstest::rstest]
#[case::flow_1(
    "from typing import Sequence\ntry:\n if a:\n  break\n elif b:\n  continue\n elif c:\n  return\n elif d:\n  raise Error\n hazard()\nfinally:\n try:\n  pass\n finally:\n  value: Sequence[int]\nafter: Sequence[int]\n"
)]
#[case::flow_2(
    "from typing import Sequence\ntry:\n hazard()\n Sequence = unknown\nfinally:\n try:\n  pass\n finally:\n  value: Sequence[int]\nafter: Sequence[int]\n"
)]
#[case::flow_3(
    "from typing import Sequence\nclass C:\n global Sequence\n try:\n  pass\n finally:\n  try:\n   pass\n  finally:\n   Sequence = unknown\n   def f(self, arg: Sequence[int]):\n    local: Sequence[int]\n   from typing import Sequence\n after: Sequence[int]\n"
)]
#[case::flow_4(
    "from typing import Sequence\ntry:\n return\nfinally:\n try:\n  pass\n finally:\n  class C:\n   def f(self, arg: Sequence[int]):\n    local: Sequence[int]\n"
)]
fn finally_routing_preserves_callbacks_and_all_exit_categories(#[case] source: &str) {
    let current = observe(source, false);
    let reference = observe(source, true);
    let candidates = analyze(source, true).0;
    let reference_candidates = {
        let _guard = OldRecordingGuard::enabled(true);
        analyze(source, true).0
    };

    assert_eq!(current, reference);
    assert_ne!(current.annotations, Vec::new());
    assert_eq!(candidates, reference_candidates);
}

#[test]
fn stable_finally_routing_keeps_import_provenance_for_every_callback() {
    let source = "from typing import Sequence\ntry:\n if a:\n  break\n elif b:\n  continue\n elif c:\n  return\n elif d:\n  raise Error\n hazard()\nfinally:\n try:\n  pass\n finally:\n  value: Sequence[int]\nafter: Sequence[int]\n";

    let observation = observe(source, false);

    assert!(observation.annotations.iter().all(|(_, _, facts)| {
        facts
            .iter()
            .any(|fact| fact == "direct:Sequence=typing.Sequence")
    }));
}

#[test]
fn shadowed_finally_routing_removes_import_provenance_from_first_callback() {
    let source = "from typing import Sequence\ntry:\n hazard()\n Sequence = unknown\nfinally:\n try:\n  pass\n finally:\n  value: Sequence[int]\nafter: Sequence[int]\n";

    let observation = observe(source, false);

    assert!(
        !observation.annotations[0]
            .2
            .iter()
            .any(|fact| fact == "direct:Sequence=typing.Sequence")
    );
}

#[rstest::rstest]
#[case::breaks("break", true, false, false, false)]
#[case::continues("continue", false, true, false, false)]
#[case::returns("return", false, false, false, true)]
#[case::raises("raise Error", false, false, true, true)]
fn nested_abrupt_finally_overrides_pending_exit(
    #[values("pass", "break", "continue", "return", "raise Error", "hazard()")] pending: &str,
    #[case] replacement: &str,
    #[case] breaks: bool,
    #[case] continues: bool,
    #[case] raises: bool,
    #[case] terminates: bool,
) {
    let source = format!("try:\n {pending}\nfinally:\n try:\n  pass\n finally:\n  {replacement}\n");

    let current = observe(&source, false);
    let reference = observe(&source, true);

    assert_eq!(current, reference);
    assert_eq!(current.explicit.fallthrough, Vec::<Vec<String>>::new());
    assert_eq!(!current.explicit.breaks.is_empty(), breaks);
    assert_eq!(!current.explicit.continues.is_empty(), continues);
    assert_eq!(!current.raises.is_empty(), raises);
    assert_eq!(!current.explicit.terminates.is_empty(), terminates);
}

#[rstest::rstest]
fn nested_abrupt_finally_removes_pending_implicit_exception(
    #[values("pass", "break", "continue", "return", "raise Error", "hazard()")] pending: &str,
    #[values("break", "continue", "return")] replacement: &str,
) {
    let source = format!("try:\n {pending}\nfinally:\n try:\n  pass\n finally:\n  {replacement}\n");

    let current = observe(&source, false);

    assert!(current.implicit.is_none());
}

#[rstest::rstest]
#[case::disabled(false)]
#[case::enabled(true)]
fn empty_entry_finally_records_only_when_enabled(#[case] record: bool) {
    let parsed = parse_module("value: Sequence[int]\n").unwrap();

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

#[rstest::rstest]
#[case::annotation("x: int", 1)]
#[case::pass("pass", 0)]
fn multi_exit_recording_has_quadratic_work_and_constant_vectors(
    #[case] leaf: &str,
    #[case] annotation_count: usize,
    #[values(1, 2, 4, 8, 12, 20)] depth: usize,
) {
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
    assert_eq!(leaves, annotation_count * (depth + 1));
    assert_eq!(collector.annotations.len(), annotation_count);
    assert!(misses <= depth * (depth + 1) / 2);
    assert!(hits > 0);
    assert!(peak <= 4);
}

#[rstest::rstest]
#[case::value_1usize(1usize)]
#[case::value_2(2)]
#[case::value_4(4)]
#[case::value_8(8)]
#[case::value_12(12)]
#[case::value_20(20)]
fn multi_exit_transfer_vectors_and_misses_are_bounded(#[case] depth: usize) {
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

#[rstest::rstest]
#[case::value_1usize(1usize)]
#[case::value_2(2)]
#[case::value_4(4)]
fn small_unoptimized_reference_exceeds_work_bound_with_identical_candidates(#[case] depth: usize) {
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

#[rstest::rstest]
fn normal_finally_preserves_pending_completion(#[values(0, 1, 2, 3, 4, 5)] pending: usize) {
    let parsed = parse_module("pass").unwrap();
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    let mut expected = [false; 6];
    expected[pending] = true;

    let actual = collector.apply_finally(pending_exit(pending), &parsed.syntax().body);

    assert_eq!(membership(&actual), expected);
}

#[rstest::rstest]
#[case::breaks("break", 1)]
#[case::continues("continue", 2)]
#[case::returns("return", 3)]
#[case::raises("raise", 4)]
fn abrupt_finally_overrides_pending_completion(
    #[values(0, 1, 2, 3, 4, 5)] pending: usize,
    #[case] body: &str,
    #[case] replacement: usize,
) {
    let parsed = parse_module(body).unwrap();
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    let mut expected = [false; 6];
    expected[replacement] = true;

    let actual = collector.apply_finally(pending_exit(pending), &parsed.syntax().body);

    assert_eq!(membership(&actual), expected);
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

#[rstest::rstest]
#[case::sequence_then_mapping(KnownBinding::Direct("typing.Sequence".into()), KnownBinding::Direct("typing.Mapping".into()))]
#[case::sequence_then_typing_module(KnownBinding::Direct("typing.Sequence".into()), KnownBinding::Module("typing".into()))]
#[case::sequence_then_collections_module(KnownBinding::Direct("typing.Sequence".into()), KnownBinding::Module("collections.abc".into()))]
#[case::sequence_then_typevar(KnownBinding::Direct("typing.Sequence".into()), KnownBinding::TypeVar)]
#[case::mapping_then_sequence(KnownBinding::Direct("typing.Mapping".into()), KnownBinding::Direct("typing.Sequence".into()))]
#[case::mapping_then_typing_module(KnownBinding::Direct("typing.Mapping".into()), KnownBinding::Module("typing".into()))]
#[case::mapping_then_collections_module(KnownBinding::Direct("typing.Mapping".into()), KnownBinding::Module("collections.abc".into()))]
#[case::mapping_then_typevar(KnownBinding::Direct("typing.Mapping".into()), KnownBinding::TypeVar)]
#[case::typing_module_then_sequence(KnownBinding::Module("typing".into()), KnownBinding::Direct("typing.Sequence".into()))]
#[case::typing_module_then_mapping(KnownBinding::Module("typing".into()), KnownBinding::Direct("typing.Mapping".into()))]
#[case::typing_module_then_collections_module(KnownBinding::Module("typing".into()), KnownBinding::Module("collections.abc".into()))]
#[case::typing_module_then_typevar(KnownBinding::Module("typing".into()), KnownBinding::TypeVar)]
#[case::collections_module_then_sequence(KnownBinding::Module("collections.abc".into()), KnownBinding::Direct("typing.Sequence".into()))]
#[case::collections_module_then_mapping(KnownBinding::Module("collections.abc".into()), KnownBinding::Direct("typing.Mapping".into()))]
#[case::collections_module_then_typing_module(KnownBinding::Module("collections.abc".into()), KnownBinding::Module("typing".into()))]
#[case::collections_module_then_typevar(KnownBinding::Module("collections.abc".into()), KnownBinding::TypeVar)]
#[case::typevar_then_sequence(KnownBinding::TypeVar, KnownBinding::Direct("typing.Sequence".into()))]
#[case::typevar_then_mapping(KnownBinding::TypeVar, KnownBinding::Direct("typing.Mapping".into()))]
#[case::typevar_then_typing_module(KnownBinding::TypeVar, KnownBinding::Module("typing".into()))]
#[case::typevar_then_collections_module(KnownBinding::TypeVar, KnownBinding::Module("collections.abc".into()))]
fn distinct_typed_imports_are_not_deduplicated_or_reused(
    #[case] first: KnownBinding,
    #[case] second: KnownBinding,
) {
    // Arrange: A/B/A tests one-slot eviction as well as exact typed keys.
    let a = KnownImports {
        bindings: HashMap::from([("T".into(), first)]),
    };
    let b = KnownImports {
        bindings: HashMap::from([("T".into(), second)]),
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

    // Assert: last-occurrence normalization preserves both distinct return states.
    assert_eq!(actual.fallthrough, Some(a.clone()));
    assert_eq!(actual.breaks, vec![b.clone()]);
    assert_eq!(actual.continues, vec![a.clone()]);
    assert_eq!(actual.terminates, vec![b, a]);
    assert_eq!(
        (finally_work::MISSES.get(), finally_work::HITS.get()),
        (5, 0)
    );
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

#[rstest::rstest]
#[case::from_typing_import_sequence("from typing import Sequence")]
#[case::import_typing_as_sequence("import typing as Sequence")]
#[case::sequence_unknown("Sequence = unknown")]
fn generated_distinct_binding_flows_preserve_production_callbacks_and_candidates(
    #[case] binding: &str,
    #[values("pass", "return", "raise Error", "break", "continue")] completion: &str,
) {
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

#[rstest::rstest]
#[case::global_sequence("global Sequence")]
#[case::nonlocal_sequence("nonlocal Sequence")]
fn class_fallback_mutation_bypasses_reuse_and_normalization(#[case] declaration: &str) {
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

#[rstest::rstest]
#[case::value_1(1)]
#[case::value_2(2)]
#[case::value_4(4)]
#[case::value_8(8)]
#[case::value_12(12)]
#[case::value_20(20)]
fn unselected_type_operators_skip_multi_exit_annotation_work(#[case] depth: usize) {
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

fn marker_projection(collector: &mut AnnotationCollector<'_, '_>) {
    collector.marker_projection = Some(BindingFlowMarkerProjection {
        source: "pass".into(),
        marker: 0..4,
        line_start: 0,
        entry: None,
    });
}

fn handler_projection(collector: &mut AnnotationCollector<'_, '_>) {
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

fn try_projection(collector: &mut AnnotationCollector<'_, '_>) {
    collector.try_exit_projection = Some(BindingFlowTryExitProjection {
        marker: 0..4,
        matching_tries: 0,
        matched_try: None,
        exits: None,
    });
}

fn mutation_hook(collector: &mut AnnotationCollector<'_, '_>) {
    collector.test_mutation = Some(BindingFlowTestMutation::OmitLoopContinueBackEdge);
}

#[rstest::rstest]
#[case::marker_projection(marker_projection)]
#[case::handler_projection(handler_projection)]
#[case::try_projection(try_projection)]
#[case::mutation_hook(mutation_hook)]
fn observer_and_mutation_hooks_bypass_reuse_and_normalization(
    #[case] setup: fn(&mut AnnotationCollector<'_, '_>),
) {
    // Arrange.
    let parsed = parse_module("pass").unwrap();
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    setup(&mut collector);
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

const RESIDUAL_ORDER_SOURCE: &str = "def f():\n    from typing import Sequence\n    try:\n        return\n    except ():\n        Sequence = 0\n        return\n    except ():\n        return\n    finally:\n        pass\n    after: Sequence[int]\n";

#[test]
fn residual_order_preserves_unreachable_callback_provenance() {
    // Arrange: valid Python; literal handler types create no implicit exit.
    let expected = (
        "direct:Sequence=typing.Sequence".to_owned(),
        Some("f".to_owned()),
    );

    // Act.
    let actual = observe(RESIDUAL_ORDER_SOURCE, false);
    let _guard = SharingGuard::disabled();
    let reference = observe(RESIDUAL_ORDER_SOURCE, false);

    // Assert: compare callback order/ranges/symbols/imports and independent provenance.
    assert_eq!(reference.annotations.len(), 1);
    assert!(reference.annotations[0].2.contains(&expected.0));
    assert_eq!(reference.annotations[0].1, expected.1);
    assert_eq!(actual, reference);
}

#[test]
fn residual_order_preserves_complete_candidate_descriptors() {
    // Arrange.
    let start = RESIDUAL_ORDER_SOURCE.find("Sequence[int]").unwrap();

    // Act.
    let actual = analyze(RESIDUAL_ORDER_SOURCE, true).0;
    let _guard = SharingGuard::disabled();
    let reference = analyze(RESIDUAL_ORDER_SOURCE, true).0;

    // Assert: independent expected mutation plus complete descriptor/order equality.
    assert_eq!(reference.len(), 1);
    assert_eq!(reference[0].original, "Sequence[int]");
    assert_eq!(reference[0].replacement, "list[int]");
    assert_eq!(reference[0].symbol.as_deref(), Some("f"));
    assert_eq!(reference[0].span.start, u64::try_from(start).unwrap());
    assert_eq!(actual, reference);
}

#[test]
fn residual_order_aba_retains_last_transfer_imports() {
    // Arrange: there is no later raise or implicit entry to overwrite this residual.
    let a = KnownImports {
        bindings: HashMap::from([(
            "Sequence".into(),
            KnownBinding::Direct("typing.Sequence".into()),
        )]),
    };
    let b = KnownImports::default();
    let parsed = parse_module("pass").unwrap();
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    let entries = ControlFlowExits {
        terminates: vec![a.clone(), b.clone(), a.clone()],
        ..ControlFlowExits::default()
    };

    // Act.
    let exits = collector.apply_finally(entries, &parsed.syntax().body);

    // Assert: exact environment set and last transfer residual are separate obligations.
    assert_eq!(exits.terminates.len(), 2);
    assert!(exits.terminates.contains(&a) && exits.terminates.contains(&b));
    assert_eq!(collector.imports, a);
}

fn assert_residual_order_normalization(word: &[usize]) {
    let states = [
        KnownBinding::Direct("typing.Sequence".into()),
        KnownBinding::Module("typing".into()),
        KnownBinding::TypeVar,
    ]
    .map(|binding| KnownImports {
        bindings: HashMap::from([("T".into(), binding)]),
    });
    // Arrange: independent index-based oracle keeps entries with no equal successor.
    let original = word
        .iter()
        .map(|&index| states[index].clone())
        .collect::<Vec<_>>();
    let expected = original
        .iter()
        .enumerate()
        .filter(|(index, state)| !original[index + 1..].contains(state))
        .map(|(_, state)| state.clone())
        .collect::<Vec<_>>();
    let mut exits = ControlFlowExits {
        breaks: original.clone(),
        continues: original.clone(),
        terminates: original.clone(),
        raises: original,
        ..ControlFlowExits::default()
    };

    // Act.
    exits.deduplicate_abrupt();

    assert_eq!(
        (
            &exits.breaks,
            &exits.continues,
            &exits.terminates,
            &exits.raises
        ),
        (&expected, &expected, &expected, &expected)
    );
    assert!(
        [
            &exits.breaks,
            &exits.continues,
            &exits.terminates,
            &exits.raises
        ]
        .iter()
        .all(|actual| {
            (0..8).all(|contributors| {
                let contributes = |state: &KnownImports| {
                    states.iter().enumerate().any(|(index, candidate)| {
                        contributors & (1 << index) != 0 && candidate == state
                    })
                };
                let before = word
                    .iter()
                    .rev()
                    .map(|&index| &states[index])
                    .find(|state| contributes(state));
                let after = actual.iter().rev().find(|state| contributes(state));
                after == before
            })
        }),
        "word={word:?}"
    );
}

#[rstest::rstest]
#[case::empty(vec![])]
#[case::direct(vec![0])]
#[case::module(vec![1])]
#[case::typevar(vec![2])]
#[case::aba(vec![0, 1, 0])]
#[case::abcba(vec![0, 1, 2, 1, 0])]
#[case::repeated(vec![2; 5])]
fn residual_order_normalization_preserves_each_last_category_contributor(#[case] word: Vec<usize>) {
    assert_residual_order_normalization(&word);
}

proptest::proptest! {
    #[test]
    fn generated_residual_order_normalization_preserves_last_contributors(word in proptest::collection::vec(0usize..3, 0..=16)) {
        assert_residual_order_normalization(&word);
    }
}

fn residual_order_source(word: &[usize], nested: bool) -> String {
    let mut inner = String::from("try:\n    return\n");
    for &state in word {
        let assignment = ["pass", "Sequence = 0", "Mapping = 0"][state];
        writeln!(inner, "except ():\n    {assignment}\n    return").unwrap();
    }
    inner.push_str("finally:\n    pass\n");
    if nested {
        let mut outer = String::from("try:\n");
        for line in inner.lines() {
            writeln!(outer, "    {line}").unwrap();
        }
        outer.push_str("finally:\n    pass\n");
        inner = outer;
    }
    let mut source = String::from("def f():\n    from typing import Sequence, Mapping\n");
    for line in inner.lines() {
        writeln!(source, "    {line}").unwrap();
    }
    source.push_str("    after: Sequence[int]\n    other: Mapping[str, int]\n");
    source
}

fn assert_residual_order_handlers(word: &[usize], nested: bool) {
    // Arrange: literal handler types/bindings leave no implicit entry to hide the tail.
    let source = residual_order_source(word, nested);
    let last = word.last().copied().unwrap_or(0);

    // Act.
    let actual = observe(&source, false);
    let candidates = analyze(&source, true).0;
    let _guard = SharingGuard::disabled();
    let reference = observe(&source, false);
    let reference_candidates = analyze(&source, true).0;

    // Assert: independent expected facts as well as disabled-reference equality.
    assert_eq!(actual, reference, "word={word:?}, nested={nested}");
    assert_eq!(
        candidates, reference_candidates,
        "word={word:?}, nested={nested}"
    );
    assert_eq!(candidates.len(), usize::from(last != 1));
    assert_eq!(actual.annotations.len(), 2);
    assert_eq!(
        actual.annotations[0]
            .2
            .contains(&"direct:Sequence=typing.Sequence".to_owned()),
        last != 1
    );
    assert_eq!(
        actual.annotations[1]
            .2
            .contains(&"direct:Mapping=typing.Mapping".to_owned()),
        last != 2
    );
}

#[rstest::rstest]
#[case::empty(vec![])]
#[case::direct(vec![0])]
#[case::sequence_shadow(vec![1])]
#[case::mapping_shadow(vec![2])]
#[case::sequence_aba(vec![1, 2, 1])]
#[case::mapping_aba(vec![2, 1, 2])]
#[case::four_handlers(vec![0, 1, 2, 0])]
fn residual_order_generated_handlers_preserve_callbacks_and_full_candidates(
    #[case] word: Vec<usize>,
    #[values(false, true)] nested: bool,
) {
    assert_residual_order_handlers(&word, nested);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]
    #[test]
    fn generated_residual_handlers_preserve_callbacks_and_candidates(
        word in proptest::collection::vec(0usize..3, 0..=8), nested in proptest::bool::ANY,
    ) {
        assert_residual_order_handlers(&word, nested);
    }
}

proptest::proptest! {
    #[test]
    fn generated_multi_exit_transfer_work_stays_linear(depth in 1usize..=24) {
        let source = multi_exit_source(depth, "x: int");
        let parsed = parse_module(&source).unwrap();
        let mut collector = AnnotationCollector::empty();
        collector.record_annotations = false;
        LOOP_STATEMENT_VISITS.set(0);
        LOOP_ANNOTATION_VISITS.set(0);

        collector.visit_suite(&parsed.syntax().body);

        proptest::prop_assert_eq!(LOOP_ANNOTATION_VISITS.get(), 1);
        proptest::prop_assert!(LOOP_STATEMENT_VISITS.get() <= 3 * depth + 2);
    }
}
