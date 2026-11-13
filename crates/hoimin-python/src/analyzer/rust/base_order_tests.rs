use super::{AnalyzeRequest, analyze_selected, analyze_source, analyze_source_cancellable};
use camino::Utf8Path;
use hoimin_core::{LineRange, MutationOperator, MutationOperatorSelection, MutationProfile};
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

#[rstest::rstest]
#[case::class_c_a_b_pass("class C(A, B): pass\n", "class C(B, A): pass\n")]
#[case::class_c_a_b_pass_2("class C((A), (B),): pass\n", "class C((B), (A),): pass\n")]
#[case::class_c_a_first_b_second_doc_pass(
    "class C(\n A, # first\n B, # second\n):\n 'doc'\n pass\n",
    "class C(\n B, # first\n A, # second\n):\n 'doc'\n pass\n"
)]
#[case::class_pass("class 子(親甲, 親乙): pass\n", "class 子(親乙, 親甲): pass\n")]
#[case::def_f_class_c_a_b_pass(
    "def f():\n class C(A,B): pass\n",
    "def f():\n class C(B,A): pass\n"
)]
fn base_order_swaps_names_and_preserves_every_source_separator(
    #[case] source: &str,
    #[case] expected: &str,
    #[values(0, 1, 2)] scenario_index: usize,
) {
    let newline = ["\n", "\r\n", "\r"]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

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

fn observed_base_order(count: usize, padding: usize) -> Vec<String> {
    let original = (0..count)
        .map(|index| format!("Base{index}"))
        .collect::<Vec<_>>();
    let separator = format!(",{}", " ".repeat(padding));
    let source = format!("class C({}): pass\n", original.join(&separator));
    let output = analyze_selected(&[OP], &source);
    assert_eq!(output.candidates.len(), 1);
    let mutant = apply(&source, &output.candidates[0]);
    let parsed = parse_module(&mutant).unwrap();
    let class = parsed.syntax().body[0].as_class_def_stmt().unwrap();
    class
        .arguments
        .as_ref()
        .unwrap()
        .args
        .iter()
        .map(|base| base.as_name_expr().unwrap().id.to_string())
        .collect()
}

#[rstest::rstest]
fn base_order_multi_base_shuffle_changes_order_without_losing_bases(
    #[values(3, 4, 5, 6, 7, 8, 9, 10, 11, 12)] count: usize,
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23
    )]
    padding: usize,
) {
    let changed = observed_base_order(count, padding);
    let original = (0..count)
        .map(|index| format!("Base{index}"))
        .collect::<Vec<_>>();
    assert_ne!(changed, original);
    let mut sorted = changed;
    sorted.sort();
    let mut expected = original;
    expected.sort();
    assert_eq!(sorted, expected);
}

#[test]
fn base_order_seeded_layouts_have_more_than_one_three_base_permutation() {
    let observed = (0..24)
        .map(|padding| observed_base_order(3, padding))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(observed.len() > 1);
}

#[rstest::rstest]
#[case::class_c_pass("class C: pass\n")]
#[case::class_c_a_pass("class C(A): pass\n")]
#[case::class_c_a_a_pass("class C(A,A): pass\n")]
#[case::class_c_k_u_212a_pass("class C(K,\u{212a}): pass\n")]
#[case::class_c_bases_pass("class C(*bases): pass\n")]
#[case::class_c_a_bases_pass("class C(A,*bases): pass\n")]
#[case::class_c_a_f_pass("class C(A,f()): pass\n")]
#[case::class_c_a_mod_b_pass("class C(A,mod.B): pass\n")]
#[case::class_c_a_b_int_pass("class C(A,B[int]): pass\n")]
#[case::class_c_a_b_metaclass_m_pass("class C(A,B,metaclass=M): pass\n")]
#[case::class_c_a_b_kw_pass("class C(A,B,**kw): pass\n")]
#[case::class_c_t_a_b_pass("class C[T](A,B): pass\n")]
#[case::decorate_class_c_a_b_pass("@decorate\nclass C(A,B): pass\n")]
fn base_order_rejects_dynamic_decorated_generic_keyword_and_duplicate_bases(#[case] source: &str) {
    assert!(
        analyze_selected(&[OP], source).candidates.is_empty(),
        "{source}"
    );
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
