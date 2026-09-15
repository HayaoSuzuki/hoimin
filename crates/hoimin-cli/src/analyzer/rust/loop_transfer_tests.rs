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
    assert!(output.candidates.is_empty());
    assert!(!output.truncated);
    (LOOP_STATEMENT_VISITS.get(), LOOP_ANNOTATION_VISITS.get())
}

#[test]
fn nested_loops_bound_actual_statement_and_annotation_visits() {
    for while_loop in [false, true] {
        for depth in [1, 2, 4, 8, 16, 20] {
            let source = nested_source(depth, while_loop);
            assert_eq!(analyze_visits(&source, false), (0, 0));
            let (statements, leaves) = analyze_visits(&source, true);
            eprintln!("while={while_loop} depth={depth} statements={statements} leaves={leaves}");
            assert!(leaves <= depth + 1, "depth={depth}: {leaves} leaf visits");
            assert!(
                statements <= (depth + 1) * (depth + 2) / 2,
                "depth={depth}: {statements} statement visits"
            );
        }
    }
}

#[test]
fn archived_audit_nested_loops_bound_visits() {
    for depth in [18, 19, 20] {
        let source = nested_source(depth, false);
        assert_eq!(analyze_visits(&source, false), (0, 0));
        let (statements, leaves) = analyze_visits(&source, true);
        assert!(leaves <= depth + 1, "depth={depth}: {leaves} leaf visits");
        assert!(
            statements <= (depth + 1) * (depth + 2) / 2,
            "depth={depth}: {statements} statement visits"
        );
    }
}

#[test]
fn archived_audit_import_annotations_bound_clone_cost() {
    use std::fmt::Write as _;

    for size in [512, 1024, 2048] {
        let mut source = String::new();
        for index in 0..size {
            writeln!(source, "import typing as t{index}").unwrap();
        }
        source.push_str(&"x: int\n".repeat(size));
        for selected in [false, true] {
            IMPORT_CLONE_CALLS.set(0);
            IMPORT_CLONE_ENTRIES.set(0);
            let visits = analyze_visits(&source, selected);
            let calls = IMPORT_CLONE_CALLS.get();
            let entries = IMPORT_CLONE_ENTRIES.get();
            if selected {
                assert!(calls <= 1, "size={size}: {calls} clone calls");
                assert!(entries <= size, "size={size}: {entries} copied entries");
            } else {
                assert_eq!(visits, (0, 0), "size={size}");
                assert_eq!((calls, entries), (0, 0), "size={size}");
            }
        }
    }
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

#[test]
fn nested_loop_old_replay_exceeds_bound_with_identical_candidates() {
    let depth = 8;
    for while_loop in [false, true] {
        let source = nested_source(depth, while_loop);
        let current = analyze_visits(&source, true);
        let _guard = ReuseGuard::disabled(true);
        let old = analyze_visits(&source, true);
        assert!(current.0 <= (depth + 1) * (depth + 2) / 2);
        assert!(old.0 > (depth + 1) * (depth + 2) / 2);
        assert_eq!(old.1, 1 << depth);
        assert_eq!(analyze_visits(&source, false), (0, 0));
    }
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

#[test]
fn nested_loop_reuse_preserves_callbacks_and_input_dependent_exits() {
    let cases = [
        // The continue edge changes the head, so the first transfer cannot be reused.
        (
            "from typing import Sequence\nwhile outer:\n while inner:\n  before: list[int]\n  Sequence = unknown\n  continue\nafter: list[int]\n",
            2,
        ),
        // Break keeps the import while the natural exit runs else.
        (
            "while outer:\n while inner:\n  from typing import Sequence\n  break\n else:\n  Sequence = unknown\n middle: list[int]\nafter: list[int]\n",
            2,
        ),
        // Finally both observes and changes the continue/break exit states.
        (
            "from typing import Sequence\nwhile outer:\n while inner:\n  try:\n   if flag:\n    continue\n   break\n  finally:\n   during: list[int]\n   Sequence = unknown\nafter: list[int]\n",
            2,
        ),
        // Imported loop targets are invalidated again on back edges.
        (
            "from typing import Sequence\nfor Sequence in items:\n for item in items:\n  during: list[int]\n  from typing import Sequence\nafter: list[int]\n",
            2,
        ),
        // Scope names and deferred function annotations remain observable.
        (
            "from typing import Sequence\ndef f():\n while outer:\n  while inner:\n   during: list[int]\n return None\nafter: list[int]\n",
            2,
        ),
        // Class globals can update the fallback used by method annotations.
        (
            "from typing import Sequence\nclass C:\n global Sequence\n while outer:\n  while inner:\n   Sequence = unknown\n   def f(self, value: list[int]):\n    local: list[int]\nafter: list[int]\n",
            3,
        ),
        // The body restores the head import but leaves a changed method fallback.
        (
            "from typing import Sequence\nclass C:\n global Sequence\n while outer:\n  while inner:\n   Sequence = unknown\n   def f(self, value: list[int]):\n    local: list[int]\n   from typing import Sequence\nafter: list[int]\n",
            3,
        ),
        // A class in an optimized transfer must restore enclosing fallback and qualname.
        (
            "from typing import Sequence\nwhile outer:\n while inner:\n  class C:\n   def f(self, value: list[int]):\n    local: list[int]\nafter: list[int]\n",
            3,
        ),
        // An abrupt function exit survives nested finally routing.
        (
            "from typing import Sequence\ndef f():\n while outer:\n  while inner:\n   try:\n    return None\n   finally:\n    during: list[int]\nafter: list[int]\n",
            2,
        ),
        // Aliases and type variables remain part of the incoming state.
        (
            "import typing as t\nT = t.TypeVar('T')\nwhile outer:\n while inner:\n  during: t.Sequence[T]\nafter: list[int]\n",
            2,
        ),
    ];
    for (source, annotation_count) in cases {
        let optimized = flow_observation(source, false);
        let old = flow_observation(source, true);
        assert_eq!(optimized, old, "{source}");
        assert_eq!(optimized.0.len(), annotation_count, "{source}");
    }

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

#[test]
fn nested_class_loops_bound_actual_statement_and_annotation_visits() {
    for while_loop in [false, true] {
        for depth in [1, 2, 4, 8, 16, 20] {
            let mut source = String::from("class C:\n");
            for line in nested_source(depth, while_loop).lines() {
                source.push(' ');
                source.push_str(line);
                source.push('\n');
            }
            assert_eq!(analyze_visits(&source, false), (0, 0));
            let (statements, leaves) = analyze_visits(&source, true);
            eprintln!(
                "class while={while_loop} depth={depth} statements={statements} leaves={leaves}"
            );
            assert!(
                leaves <= depth + 1,
                "depth={depth}: {leaves} class leaf visits"
            );
            assert!(
                statements <= 1 + (depth + 1) * (depth + 2) / 2,
                "depth={depth}: {statements} class statement visits"
            );
        }
    }
}
