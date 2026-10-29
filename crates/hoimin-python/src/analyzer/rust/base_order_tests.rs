use super::{AnalyzeRequest, analyze_selected, analyze_source, analyze_source_cancellable};
use camino::Utf8Path;
use hoimin_core::{LineRange, MutationOperator, MutationOperatorSelection, MutationProfile};
use ruff_python_ast::{Expr, Stmt};
use ruff_python_parser::parse_module;

const OP: &str = "base_order_shuffle";

fn apply(source: &str, candidate: &crate::AnalyzerCandidate) -> String {
    let mut result = source.to_owned();
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    assert_eq!(&source[start..end], candidate.original);
    result.replace_range(start..end, &candidate.replacement);
    assert!(parse_module(&result).is_ok(), "{result}");
    result
}

#[test]
fn base_order_is_default_and_individually_excludable() {
    let operator = MutationOperator::from_name(OP).expect("registered operator");
    let mut operators = MutationOperatorSelection::default();
    assert!(operators.contains(operator));
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[test]
fn base_order_swaps_names_and_preserves_every_source_separator() {
    for (source, expected) in [
        ("class C(A, B): pass\n", "class C(B, A): pass\n"),
        ("class C((A), (B),): pass\n", "class C((B), (A),): pass\n"),
        (
            "class C(\n A, # first\n B, # second\n):\n 'doc'\n pass\n",
            "class C(\n B, # first\n A, # second\n):\n 'doc'\n pass\n",
        ),
        (
            "class 子(親甲, 親乙): pass\n",
            "class 子(親乙, 親甲): pass\n",
        ),
        (
            "def f():\n class C(A,B): pass\n",
            "def f():\n class C(B,A): pass\n",
        ),
    ] {
        for newline in ["\n", "\r\n", "\r"] {
            let source = source.replace('\n', newline);
            let expected = expected.replace('\n', newline);
            let output = analyze_selected(&[OP], &source);
            assert_eq!(output.candidates.len(), 1, "{source}");
            assert_eq!(apply(&source, &output.candidates[0]), expected);
            assert_eq!(
                output.candidates,
                analyze_selected(&[OP], &source).candidates
            );
        }
    }
}

#[test]
fn base_order_multi_base_shuffle_changes_order_without_losing_bases() {
    let mut observed = std::collections::BTreeSet::new();
    for count in 3..=12 {
        let original: Vec<_> = (0..count).map(|i| format!("Base{i}")).collect();
        for padding in 0..24 {
            let separator = format!(",{}", " ".repeat(padding));
            let source = format!("class C({}): pass\n", original.join(&separator));
            let output = analyze_selected(&[OP], &source);
            assert_eq!(output.candidates.len(), 1);
            let mutant = apply(&source, &output.candidates[0]);
            let parsed = parse_module(&mutant).unwrap();
            let Stmt::ClassDef(class) = &parsed.syntax().body[0] else {
                panic!()
            };
            let changed: Vec<_> = class
                .arguments
                .as_ref()
                .unwrap()
                .args
                .iter()
                .map(|base| {
                    let Expr::Name(name) = base else { panic!() };
                    name.id.to_string()
                })
                .collect();
            assert_ne!(changed, original);
            let mut sorted = changed.clone();
            sorted.sort();
            let mut expected = original.clone();
            expected.sort();
            assert_eq!(sorted, expected);
            if count == 3 {
                observed.insert(changed);
            }
        }
    }
    assert!(observed.len() > 1);
}

#[test]
fn base_order_rejects_dynamic_decorated_generic_keyword_and_duplicate_bases() {
    for source in [
        "class C: pass\n",
        "class C(A): pass\n",
        "class C(A,A): pass\n",
        "class C(K,\u{212a}): pass\n",
        "class C(*bases): pass\n",
        "class C(A,*bases): pass\n",
        "class C(A,f()): pass\n",
        "class C(A,mod.B): pass\n",
        "class C(A,B[int]): pass\n",
        "class C(A,B,metaclass=M): pass\n",
        "class C(A,B,**kw): pass\n",
        "class C[T](A,B): pass\n",
        "@decorate\nclass C(A,B): pass\n",
    ] {
        assert!(
            analyze_selected(&[OP], source).candidates.is_empty(),
            "{source}"
        );
    }
}

#[test]
fn base_order_selectors_caps_and_cancellation_preserve_candidate_identity() {
    let operators = MutationOperatorSelection::parse_selector(OP).unwrap();
    let mut selected = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            selected.exclude(operator);
        }
    }
    for operator in operators {
        selected.include(operator);
    }
    let source = "class First(A,B): pass\nclass Chosen(A,B,C): pass\n";
    let mut request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &selected,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    let full = analyze_source(&request, source);
    assert_eq!(full.candidates.len(), 2);
    request.lines = &[LineRange { start: 2, end: 2 }];
    let symbols = ["Chosen".to_owned()];
    request.symbols = &symbols;
    let output = analyze_source(&request, source);
    assert_eq!(output.candidates, full.candidates[1..]);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("Chosen"));
    assert!(analyze_source_cancellable(&request, source, || true).is_err());
    request.lines = &[];
    request.symbols = &[];
    request.max_candidates = 1;
    let capped = analyze_source(&request, source);
    assert_eq!(capped.candidates, full.candidates[..1]);
    assert!(capped.truncated);
}

#[test]
fn base_order_excluded_work_is_zero_and_cancellation_and_payload_limits_release_leases() {
    use super::super::whole_expression_selection_tests::{reset_work, work};
    use super::super::{AnalysisError, payload::PayloadBudget};
    let operators = serde_json::from_value(serde_json::json!([OP])).unwrap();
    let budget = PayloadBudget::default();
    let mut request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("subject.py"),
        lines: &[LineRange { start: 3, end: 3 }],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    let source = "class First(A,B): pass\nclass Second(A,B): pass\n";
    reset_work();
    assert!(
        analyze_source(&request, source).candidates.is_empty(),
        "excluded classes must not emit candidates"
    );
    assert_eq!(work(), (0, 0));
    request.lines = &[];
    assert!(matches!(
        analyze_source_cancellable(&request, source, || work().0 > 0),
        Err(AnalysisError::Cancelled)
    ));
    assert_eq!(work(), (1, 3));
    assert_eq!(budget.snapshot().used, 0);
    let tiny = PayloadBudget::new(0);
    request.payload = tiny.clone();
    assert!(matches!(
        analyze_source_cancellable(&request, source, || false),
        Err(AnalysisError::PayloadLimit { limit: 0 })
    ));
    assert_eq!(tiny.snapshot().used, 0);
}
