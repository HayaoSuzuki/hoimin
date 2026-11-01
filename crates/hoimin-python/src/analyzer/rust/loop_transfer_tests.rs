use super::*;

fn nested_source(depth: usize, while_loop: bool) -> String {
    let mut source = String::new();
    for level in 0..depth {
        source.push_str(&" ".repeat(level));
        source.push_str(if while_loop {
            "while flag:\n"
        } else {
            "for _ in []:\n"
        });
    }
    source.push_str(&" ".repeat(depth));
    source.push_str("x: list[int]\n");
    source
}

fn selected_operator(name: &str) -> MutationOperatorSelection {
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        for operator in MutationOperatorSelection::parse_selector(&name).unwrap() {
            operators.exclude(operator);
        }
    }
    for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
        operators.include(operator);
    }
    operators
}

fn analyze_visits(source: &str, selected: bool) -> (usize, usize) {
    let operators = selected_operator(if selected {
        "type_list_sequence"
    } else {
        "boolean_literal"
    });
    LOOP_STATEMENT_VISITS.set(0);
    LOOP_ANNOTATION_VISITS.set(0);
    let output = analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("nested.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 100,
        },
        source,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.candidates, Vec::new());
    assert!(!output.truncated);
    (LOOP_STATEMENT_VISITS.get(), LOOP_ANNOTATION_VISITS.get())
}

#[rstest::rstest]
#[case::disabled(false)]
#[case::enabled(true)]
fn nested_loops_bound_actual_statement_and_annotation_visits(
    #[case] while_loop: bool,
    #[values(1, 2, 4, 8, 16, 20)] depth: usize,
) {
    let source = nested_source(depth, while_loop);

    let (statements, leaves) = analyze_visits(&source, true);

    eprintln!("while={while_loop} depth={depth} statements={statements} leaves={leaves}");
    assert!(leaves <= depth + 1, "depth={depth}: {leaves} leaf visits");
    assert!(
        statements <= (depth + 1) * (depth + 2) / 2,
        "depth={depth}: {statements} statement visits"
    );
}

#[rstest::rstest]
#[case::value_18(18)]
#[case::value_19(19)]
#[case::value_20(20)]
fn archived_audit_nested_loops_bound_visits(#[case] depth: usize) {
    let source = nested_source(depth, false);
    let (statements, leaves) = analyze_visits(&source, true);
    assert!(leaves <= depth + 1, "depth={depth}: {leaves} leaf visits");
    assert!(
        statements <= (depth + 1) * (depth + 2) / 2,
        "depth={depth}: {statements} statement visits"
    );
}

fn import_annotation_source(size: usize) -> String {
    use std::fmt::Write as _;
    let mut source = String::new();
    for index in 0..size {
        writeln!(source, "import typing as t{index}").unwrap();
    }
    source.push_str(&"x: int\n".repeat(size));
    source
}

#[rstest::rstest]
fn archived_audit_import_annotations_bound_clone_cost(#[values(512, 1024, 2048)] size: usize) {
    let source = import_annotation_source(size);
    IMPORT_CLONE_CALLS.set(0);
    IMPORT_CLONE_ENTRIES.set(0);

    analyze_visits(&source, true);

    assert!(IMPORT_CLONE_CALLS.get() <= 1);
    assert!(IMPORT_CLONE_ENTRIES.get() <= size);
}

#[rstest::rstest]
fn unselected_import_annotations_skip_clone_work(#[values(512, 1024, 2048)] size: usize) {
    let source = import_annotation_source(size);
    IMPORT_CLONE_CALLS.set(0);
    IMPORT_CLONE_ENTRIES.set(0);

    let visits = analyze_visits(&source, false);

    assert_eq!(visits, (0, 0));
    assert_eq!(
        (IMPORT_CLONE_CALLS.get(), IMPORT_CLONE_ENTRIES.get()),
        (0, 0)
    );
}

struct ReuseGuard(bool);

impl ReuseGuard {
    fn disabled(disabled: bool) -> Self {
        Self(DISABLE_LOOP_TRANSFER_REUSE.replace(disabled))
    }
}

impl Drop for ReuseGuard {
    fn drop(&mut self) {
        DISABLE_LOOP_TRANSFER_REUSE.set(self.0);
    }
}

#[rstest::rstest]
#[case::disabled(false)]
#[case::enabled(true)]
fn nested_loop_old_replay_exceeds_bound_with_identical_candidates(#[case] while_loop: bool) {
    let depth = 8;

    let source = nested_source(depth, while_loop);
    let current = analyze_visits(&source, true);
    let _guard = ReuseGuard::disabled(true);
    let old = analyze_visits(&source, true);
    assert!(current.0 <= (depth + 1) * (depth + 2) / 2);
    assert!(old.0 > (depth + 1) * (depth + 2) / 2);
    assert_eq!(old.1, 1 << depth);
}

type AnnotationObservation = (TextRange, Option<String>, Vec<String>);

fn flow_observation(
    source: &str,
    disabled: bool,
) -> (Vec<AnnotationObservation>, BindingFlowTestSnapshot) {
    let _guard = ReuseGuard::disabled(disabled);
    let parsed = parse_module(source).expect("valid nested loop fixture");
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
    (annotations, normalize_binding_flow_exits(&exits))
}

#[rstest::rstest]
#[case::flow_1(
    "from typing import Sequence\nwhile outer:\n while inner:\n  before: list[int]\n  Sequence = unknown\n  continue\nafter: list[int]\n",
    2
)]
#[case::flow_2(
    "while outer:\n while inner:\n  from typing import Sequence\n  break\n else:\n  Sequence = unknown\n middle: list[int]\nafter: list[int]\n",
    2
)]
#[case::flow_3(
    "from typing import Sequence\nwhile outer:\n while inner:\n  try:\n   if flag:\n    continue\n   break\n  finally:\n   during: list[int]\n   Sequence = unknown\nafter: list[int]\n",
    2
)]
#[case::flow_4(
    "from typing import Sequence\nfor Sequence in items:\n for item in items:\n  during: list[int]\n  from typing import Sequence\nafter: list[int]\n",
    2
)]
#[case::flow_5(
    "from typing import Sequence\ndef f():\n while outer:\n  while inner:\n   during: list[int]\n return None\nafter: list[int]\n",
    2
)]
#[case::flow_6(
    "from typing import Sequence\nclass C:\n global Sequence\n while outer:\n  while inner:\n   Sequence = unknown\n   def f(self, value: list[int]):\n    local: list[int]\nafter: list[int]\n",
    3
)]
#[case::flow_7(
    "from typing import Sequence\nclass C:\n global Sequence\n while outer:\n  while inner:\n   Sequence = unknown\n   def f(self, value: list[int]):\n    local: list[int]\n   from typing import Sequence\nafter: list[int]\n",
    3
)]
#[case::flow_8(
    "from typing import Sequence\nwhile outer:\n while inner:\n  class C:\n   def f(self, value: list[int]):\n    local: list[int]\nafter: list[int]\n",
    3
)]
#[case::flow_9(
    "from typing import Sequence\ndef f():\n while outer:\n  while inner:\n   try:\n    return None\n   finally:\n    during: list[int]\nafter: list[int]\n",
    2
)]
#[case::flow_10(
    "import typing as t\nT = t.TypeVar('T')\nwhile outer:\n while inner:\n  during: t.Sequence[T]\nafter: list[int]\n",
    2
)]
fn nested_loop_reuse_preserves_callbacks_and_input_dependent_exits(
    #[case] source: &str,
    #[case] annotation_count: usize,
) {
    let optimized = flow_observation(source, false);
    let old = flow_observation(source, true);

    assert_eq!(optimized, old, "{source}");
    assert_eq!(optimized.0.len(), annotation_count, "{source}");
}

#[test]
fn nested_loop_reuse_preserves_shadowed_input_dependent_exits() {
    let stable = "from typing import Sequence\nwhile outer:\n while inner:\n  during: list[int]\nafter: list[int]\n";
    let shadowed = stable.replace("  during:", "  Sequence = unknown\n  during:");
    let known = flow_observation(stable, false);
    let unknown = flow_observation(&shadowed, false);
    assert_eq!(known, flow_observation(stable, true));
    assert_eq!(unknown, flow_observation(&shadowed, true));
    assert!(
        known
            .0
            .iter()
            .all(|(_, _, facts)| facts.iter().any(|fact| fact.contains("Sequence")))
    );
    assert!(unknown.0.iter().all(|(_, _, facts)| facts.is_empty()));
    assert_ne!(known.1, unknown.1);
}

#[rstest::rstest]
#[case::disabled(false)]
#[case::enabled(true)]
fn nested_class_loops_bound_actual_statement_and_annotation_visits(
    #[case] while_loop: bool,
    #[values(1, 2, 4, 8, 16, 20)] depth: usize,
) {
    let mut source = String::from("class C:\n");
    for line in nested_source(depth, while_loop).lines() {
        source.push(' ');
        source.push_str(line);
        source.push('\n');
    }

    let (statements, leaves) = analyze_visits(&source, true);

    eprintln!("class while={while_loop} depth={depth} statements={statements} leaves={leaves}");
    assert!(
        leaves <= depth + 1,
        "depth={depth}: {leaves} class leaf visits"
    );
    assert!(
        statements <= 1 + (depth + 1) * (depth + 2) / 2,
        "depth={depth}: {statements} class statement visits"
    );
}

#[rstest::rstest]
#[case::for_loop(false)]
#[case::while_loop(true)]
fn unselected_nested_loop_operators_skip_visits(
    #[case] while_loop: bool,
    #[values(1, 2, 4, 8, 16, 18, 19, 20)] depth: usize,
    #[values(false, true)] in_class: bool,
) {
    let source = nested_source(depth, while_loop);
    let prefix = ["", "class C:\n"][usize::from(in_class)];
    let indent = ["", " "][usize::from(in_class)];
    let mut wrapped = String::from(prefix);
    for line in source.lines() {
        wrapped.push_str(indent);
        wrapped.push_str(line);
        wrapped.push('\n');
    }
    let source = wrapped;

    let visits = analyze_visits(&source, false);

    assert_eq!(visits, (0, 0));
}

proptest::proptest! {
    #[test]
    fn generated_nested_loop_visits_stay_quadratic(depth in 1usize..=24, while_loop in proptest::bool::ANY) {
        let source = nested_source(depth, while_loop);

        let (statements, leaves) = analyze_visits(&source, true);

        proptest::prop_assert!(leaves <= depth + 1);
        proptest::prop_assert!(statements <= (depth + 1) * (depth + 2) / 2);
    }
}

#[rstest::rstest]
#[case::for_loop(false)]
#[case::while_loop(true)]
fn old_unselected_nested_loops_skip_work(#[case] while_loop: bool) {
    let source = nested_source(8, while_loop);
    let _guard = ReuseGuard::disabled(true);

    let visits = analyze_visits(&source, false);

    assert_eq!(visits, (0, 0));
}
