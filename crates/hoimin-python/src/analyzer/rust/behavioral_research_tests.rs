use super::{analyze, analyze_selected, analyze_selected_with, apply_candidate_and_reparse};
use camino::Utf8Path;
use hoimin_core::LineRange;

#[test]
fn behavioral_research_adopted_mutations_are_default() {
    for (source, operator, original, replacement) in [
        (
            "for x in xs:\n if x: continue\n consume(x)\n",
            "continue_remove",
            "continue",
            "pass",
        ),
        (
            "class Token:\n def __eq__(self, other):\n  return NotImplemented\n",
            "comparison_not_implemented",
            "NotImplemented",
            "False",
        ),
        (
            "raise LookupError('bad') from source\n",
            "exception_cause_remove",
            "source",
            "None",
        ),
    ] {
        let output = analyze(source);
        let rows: Vec<_> = output
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == operator)
            .collect();
        assert_eq!(rows.len(), 1, "{operator}");
        assert_eq!(rows[0].original, original);
        assert_eq!(rows[0].replacement, replacement);
        apply_candidate_and_reparse(source, rows[0]);
    }
}

#[test]
fn comparison_mutation_excludes_non_methods_shadowing_and_generators() {
    for source in [
        "def __eq__(self, other):\n return NotImplemented\n",
        "class A:\n def f(self):\n  return NotImplemented\n",
        "class A:\n def __add__(self, other):\n  return NotImplemented\n",
        "class A:\n def __eq__(self, NotImplemented):\n  return NotImplemented\n",
        "NotImplemented = sentinel\nclass A:\n def __eq__(self, other):\n  return NotImplemented\n",
        "class A:\n def __eq__(self, other):\n  def nested():\n   return NotImplemented\n  return nested()\n",
        "class A:\n @staticmethod\n def __eq__(other):\n  return NotImplemented\n",
        "class A:\n async def __eq__(self, other):\n  return NotImplemented\n",
        "class A:\n def __eq__(self, other):\n  yield other\n  return NotImplemented\n",
        "from somewhere import *\nclass A:\n def __eq__(self, other):\n  return NotImplemented\n",
        "exec(code)\nclass A:\n def __eq__(self, other):\n  return NotImplemented\n",
    ] {
        assert!(
            analyze_selected(&["comparison_not_implemented"], source)
                .candidates
                .is_empty(),
            "{source}"
        );
    }
}

#[test]
fn named_causes_and_continue_keep_comments_and_utf8() {
    let source = "結果 = 1\nfor item in xs:\n with resource():\n  try:\n   if item: continue # next item\n   consume(item)\n  finally:\n   close(item)\nelse:\n finish()\ntry:\n read()\nexcept ValueError as reason:\n raise LookupError('bad') from ( # preserve\n  reason # cause\n )\n";
    let output = analyze_selected(&["continue_remove", "exception_cause_remove"], source);
    assert_eq!(output.candidates.len(), 2);
    for candidate in &output.candidates {
        let changed = apply_candidate_and_reparse(source, candidate);
        for comment in ["# next item", "# preserve", "# cause"] {
            assert!(changed.contains(comment));
        }
    }
    for source in [
        "raise Error from None\n",
        "raise Error from cause()\n",
        "raise Error from holder.cause\n",
        "raise Error from (a if flag else b)\n",
        "raise Error\n",
    ] {
        assert!(
            analyze_selected(&["exception_cause_remove"], source)
                .candidates
                .is_empty(),
            "{source}"
        );
    }
}

#[test]
fn behavioral_research_caps_selection_and_cancellation() {
    let source =
        "def first(xs):\n for x in xs:\n  continue\ndef second(xs):\n for x in xs:\n  continue\n";
    let output = analyze_selected_with(
        &["continue_remove"],
        Utf8Path::new("a.py"),
        &[],
        &[],
        1,
        source,
    );
    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
    let output = analyze_selected_with(
        &["continue_remove"],
        Utf8Path::new("a.py"),
        &[LineRange { start: 6, end: 6 }],
        &["second".to_owned()],
        10,
        source,
    );
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].line, 6);
    let operators = serde_json::from_value(serde_json::json!([
        "continue_remove",
        "comparison_not_implemented",
        "exception_cause_remove"
    ]))
    .unwrap();
    let request = super::super::AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("a.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: hoimin_core::MutationProfile::Full,
        max_candidates: 1,
    };
    assert!(super::super::analyze_source_cancellable(&request, source, || true).is_err());
}

#[test]
fn comparison_methods_restore_scope_after_nested_definitions() {
    for method in ["__eq__", "__ne__", "__lt__", "__le__", "__gt__", "__ge__"] {
        let source = format!(
            "class A:\n def {method}(self, other):\n  def nested():\n   return NotImplemented\n  return (NotImplemented) # delegate\n"
        );
        let output = analyze_selected(&["comparison_not_implemented"], &source);
        assert_eq!(output.candidates.len(), 1, "{method}");
        assert!(
            apply_candidate_and_reparse(&source, &output.candidates[0])
                .contains("return (False) # delegate")
        );
    }
}
