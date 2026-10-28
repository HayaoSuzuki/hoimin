use super::{analyze, apply_candidate_and_reparse};
use super::{analyze_selected, analyze_selected_with};
use camino::Utf8Path;
use hoimin_core::LineRange;

#[test]
fn division_exchange_preserves_utf8_comments_and_binary_only_scope() {
    let source = concat!(
        "def divide(a, /, b):\n return a / b\n",
        "比率 = (left # numerator\n / (right))\n",
        "type Ratio = left / right\ntext = 'left / right'\n",
        "value = operator.truediv(left, right)\nvalue /= right\n",
    );
    let output = analyze_selected(&["binary_div_floor"], source);
    assert_eq!(output.candidates.len(), 2);
    for candidate in &output.candidates {
        assert_eq!((&*candidate.original, &*candidate.replacement), ("/", "//"));
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn division_exchange_uses_bounded_admission_and_cancellation() {
    let source = "first = a / b\nsecond = a // b\n";
    let output = analyze_selected_with(
        &["binary_div_floor"],
        Utf8Path::new("a.py"),
        &[],
        &[],
        1,
        source,
    );
    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
    let operators = serde_json::from_value(serde_json::json!([
        "binary_div_floor",
        "structure_slice_bound_omit",
        "structure_slice_bound_move"
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
fn division_exchange_is_default_and_preserves_existing_arithmetic() {
    let source = "true_value = a / b\nfloor_value = a // b\na /= b\na //= b\nannotation: a / b\n";
    let output = analyze(source);
    let exchanges: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "binary_div_floor")
        .collect();

    assert_eq!(
        exchanges
            .iter()
            .map(|c| (c.original.as_str(), c.replacement.as_str()))
            .collect::<Vec<_>>(),
        vec![("/", "//"), ("//", "/")]
    );
    assert!(
        output
            .candidates
            .iter()
            .any(|c| c.operator.as_str() == "binary_mul_div")
    );
    assert!(
        output
            .candidates
            .iter()
            .any(|c| c.operator.as_str() == "binary_floor_mod")
    );
    for candidate in exchanges {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn slice_bound_operators_are_default_with_a_bounded_candidate_set() {
    let source = "window = xs[a:b]\nprefix = xs[:n]\nsuffix = xs[n:]\nwhole = xs[:]\n";
    let output = analyze(source);
    let omit: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "structure_slice_bound_omit")
        .collect();
    let movement: Vec<_> = output
        .candidates
        .iter()
        .filter(|c| c.operator.as_str() == "structure_slice_bound_move")
        .collect();

    assert_eq!(omit.len(), 2);
    assert_eq!(movement.len(), 2);
    for candidate in omit.into_iter().chain(movement) {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn slice_bounds_keep_parentheses_comments_and_utf8_syntax_balanced() {
    let source = concat!(
        "窓 = xs[(開始):(終点)]\n",
        "prefix = xs[:(\n n # keep bound comment\n)]\n",
        "suffix = xs[(-2):]\n",
        "numbers = xs[0:1_000]\n",
    );
    let output = analyze_selected(
        &["structure_slice_bound_omit", "structure_slice_bound_move"],
        source,
    );
    assert_eq!(output.candidates.len(), 6);
    let actual: Vec<_> = output
        .candidates
        .iter()
        .map(|c| (c.original.as_str(), c.replacement.as_str()))
        .collect();
    assert_eq!(
        actual,
        vec![
            ("(開始)", ""),
            ("(終点)", ""),
            (
                ":(\n n # keep bound comment\n)",
                "(\n n # keep bound comment\n):"
            ),
            ("(-2):", ":(-2)"),
            ("0", ""),
            ("1_000", ""),
        ]
    );
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn slice_movement_preserves_comments_adjacent_to_the_separator() {
    let source = "prefix = xs[: # keep prefix\n n]\nsuffix = xs[n # keep suffix\n :]\n";
    let output = analyze_selected(&["structure_slice_bound_move"], source);
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|c| c.replacement.as_str())
            .collect::<Vec<_>>(),
        vec![" # keep prefix\n n:", ":n # keep suffix\n "]
    );
    for candidate in &output.candidates {
        apply_candidate_and_reparse(source, candidate);
    }
}

#[test]
fn slice_bounds_exclude_unsupported_contexts_and_side_effecting_bounds() {
    let source = concat!(
        "xs[a:b:c]\nxs[1:2, 3:4]\nxs[start():stop]\nxs[a:obj.stop]\n",
        "xs[a+1:b]\nxs[1.0:2]\nxs[a:b] = values\ndel xs[a:b]\n",
        "annotation: xs[a:b]\ntype Alias = xs[a:b]\nxs[:]\n",
    );
    let output = analyze_selected(
        &["structure_slice_bound_omit", "structure_slice_bound_move"],
        source,
    );
    assert!(output.candidates.is_empty(), "{:#?}", output.candidates);
}

#[test]
fn slice_bound_admission_preserves_caps_and_exact_target_selection() {
    let source = "def first(xs, a, b):\n return xs[a:b]\ndef second(xs, n):\n return xs[:n]\n";
    let selected = ["structure_slice_bound_omit", "structure_slice_bound_move"];
    let capped = analyze_selected_with(
        &selected,
        Utf8Path::new("pkg/example.py"),
        &[],
        &[],
        1,
        source,
    );
    assert_eq!(capped.candidates.len(), 1);
    assert!(capped.truncated);
    assert_eq!(capped.candidates[0].original, "a");
    let lines = [LineRange { start: 4, end: 4 }];
    let symbols = ["second".to_owned()];
    let output = analyze_selected_with(
        &selected,
        Utf8Path::new("pkg/example.py"),
        &lines,
        &symbols,
        1,
        source,
    );
    assert_eq!(output.candidates.len(), 1);
    assert!(!output.truncated);
    assert_eq!(output.candidates[0].replacement, "n:");
}
