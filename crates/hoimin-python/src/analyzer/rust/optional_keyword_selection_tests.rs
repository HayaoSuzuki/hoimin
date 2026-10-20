//! Work at the optional-keyword enumerator and builder, before candidate admission.
use super::*;
use proptest::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Work {
    lookups: usize,
    keywords: usize,
    replacements: usize,
    replacement_bytes: usize,
}

thread_local! {
    static WORK: Cell<Work> = const { Cell::new(Work {
        lookups: 0, keywords: 0, replacements: 0, replacement_bytes: 0,
    }) };
}

pub(super) fn record_lookup() {
    WORK.update(|work| Work {
        lookups: work.lookups + 1,
        ..work
    });
}

pub(super) fn record_keyword() {
    WORK.update(|work| Work {
        keywords: work.keywords + 1,
        ..work
    });
}

pub(super) fn record_replacement(replacement: &str) {
    WORK.update(|work| Work {
        replacements: work.replacements + 1,
        replacement_bytes: work.replacement_bytes + replacement.len(),
        ..work
    });
}

fn operators() -> MutationOperatorSelection {
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        for current in MutationOperatorSelection::parse_selector(&name).unwrap() {
            operators.exclude(current);
        }
    }
    operators.include(MutationOperator::OptionalKeywordDelete);
    operators
}

fn request<'a>(
    operators: &'a MutationOperatorSelection,
    lines: &'a [LineRange],
    symbols: &'a [String],
    max_candidates: usize,
) -> AnalyzeRequest<'a> {
    AnalyzeRequest {
        payload: payload::PayloadBudget::default(),
        path: Utf8Path::new("pkg/subject.py"),
        lines,
        symbols,
        operators,
        profile: MutationProfile::Full,
        max_candidates,
    }
}

fn large_call(keywords: usize) -> String {
    let arguments = (0..keywords)
        .map(|index| format!("k{index}=0"))
        .collect::<Vec<_>>()
        .join(",");
    format!("def f({arguments}): pass\nf({arguments})\nchosen = True\n")
}

fn check_excluded(
    source: &str,
    lines: &[LineRange],
    symbols: &[String],
    limit: usize,
    bytes: usize,
) {
    let operators = operators();
    let mut request = request(&operators, lines, symbols, limit);
    request.payload = payload::PayloadBudget::new(bytes);
    WORK.set(Work::default());

    let output = analyze_source_cancellable(&request, source, || false).unwrap();

    assert_eq!(WORK.get(), Work::default(), "excluded optional work");
    assert_eq!(output.candidates.len(), 0);
    assert_eq!(output.diagnostics.len(), 0);
    assert!(!output.truncated);
}

macro_rules! excluded_large_call {
    ($name:ident, $limit:expr, $bytes:expr) => {
        #[test]
        fn $name() {
            let source = large_call(1_000);
            let lines = [LineRange { start: 3, end: 3 }];

            check_excluded(&source, &lines, &[], $limit, $bytes);
        }
    };
}

excluded_large_call!(
    excluded_large_call_with_zero_payload_and_one_candidate,
    1,
    0
);
excluded_large_call!(
    excluded_large_call_with_zero_payload_and_many_candidates,
    10_000,
    0
);
excluded_large_call!(
    excluded_large_call_with_payload_and_many_candidates,
    10_000,
    usize::MAX
);
excluded_large_call!(excluded_large_call_with_zero_candidate_limit, 0, 0);

#[test]
fn excluded_symbol_skips_optional_enumeration() {
    let source = "def f(a=0): pass\ndef excluded():\n f(a=1)\ndef chosen():\n pass\n";
    let symbols = ["pkg.subject:chosen".to_owned()];

    check_excluded(source, &[], &symbols, 10, 0);
}

#[test]
fn reversed_only_range_skips_optional_enumeration() {
    let source = "def f(a=0): pass\nf(a=1)\n";
    let lines = [LineRange { start: 3, end: 2 }];

    check_excluded(source, &lines, &[], 10, 0);
}

#[test]
fn multiline_call_interior_does_not_admit_whole_call() {
    let source = "def f(a=0): pass\nf(\n a=1,\n)\n";
    let lines = [LineRange { start: 3, end: 3 }];

    check_excluded(source, &lines, &[], 10, 0);
}

#[test]
fn selected_optional_child_survives_excluded_multiline_call() {
    let source = "def f(a=0, b=0): pass\nf(\n a=f(a=3),\n b=4,\n)\n";
    let operators = operators();
    let lines = [LineRange { start: 3, end: 3 }];
    WORK.set(Work::default());

    let output = analyze_source(&request(&operators, &lines, &[], 10), source);

    assert_eq!(
        WORK.get(),
        Work {
            lookups: 1,
            keywords: 1,
            replacements: 1,
            replacement_bytes: 5
        }
    );
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].line, 3);
    assert_eq!(output.candidates[0].original, "f(a=3)");
    assert_eq!(output.candidates[0].replacement, "(f())");
    assert!(!output.truncated);
}

#[test]
fn selected_boolean_child_survives_excluded_multiline_call() {
    let source = "def f(a=False): pass\nf(\n a=True,\n)\n";
    let mut operators = operators();
    operators.include(MutationOperator::BooleanLiteral);
    let lines = [LineRange { start: 3, end: 3 }];
    WORK.set(Work::default());

    let output = analyze_source(&request(&operators, &lines, &[], 10), source);

    assert_eq!(WORK.get(), Work::default());
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(
        output.candidates[0].operator,
        MutationOperator::BooleanLiteral
    );
    assert_eq!(output.candidates[0].line, 3);
    assert_eq!(output.candidates[0].original, "True");
    assert_eq!(output.candidates[0].replacement, "False");
    assert!(!output.truncated);
}

#[test]
fn selected_method_name_survives_excluded_call_start() {
    let source = "value = (text\n .startswith('a')\n)\n";
    let mut operators = operators();
    operators.include(MutationOperator::CollectionStringStartsEnds);
    let lines = [LineRange { start: 2, end: 2 }];
    WORK.set(Work::default());

    let output = analyze_source(&request(&operators, &lines, &[], 10), source);

    assert_eq!(WORK.get(), Work::default());
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(
        output.candidates[0].operator,
        MutationOperator::CollectionStringStartsEnds
    );
    assert_eq!(output.candidates[0].line, 2);
    assert_eq!(output.candidates[0].original, "startswith");
    assert_eq!(output.candidates[0].replacement, "endswith");
    assert!(!output.truncated);
}

fn check_selected(limit: usize, replacements: &[&str], truncated: bool) {
    let source = "def f(a=0, b=0): pass\nf(a=9, b=8)\ndef chosen():\n return f(\n  b=2, a=1,\n )\n";
    let operators = operators();
    let lines = [LineRange { start: 4, end: 4 }];
    let symbols = ["pkg.subject:chosen".to_owned()];
    let full = analyze_source(&request(&operators, &[], &[], 10), source);
    let expected = full
        .candidates
        .iter()
        .filter(|candidate| candidate.line == 4)
        .take(limit)
        .collect::<Vec<_>>();
    WORK.set(Work::default());

    let output = analyze_source(&request(&operators, &lines, &symbols, limit), source);

    assert_eq!(output.candidates.iter().collect::<Vec<_>>(), expected);
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| candidate.replacement.as_str())
            .collect::<Vec<_>>(),
        replacements
    );
    assert_eq!(output.truncated, truncated);
    assert_eq!(output.diagnostics.len(), usize::from(truncated));
    for diagnostic in &output.diagnostics {
        assert_eq!(
            diagnostic.code,
            AnalyzerDiagnosticCode::CandidateLimitExceeded
        );
    }
    assert_eq!(WORK.get().lookups, 1);
    assert_eq!(WORK.get().keywords, 2);
}

#[test]
fn selected_multiline_call_preserves_candidate_identity_and_order() {
    check_selected(10, &["(f(a=(1),))", "(f(b=(2),))"], false);
}

#[test]
fn selected_multiline_call_preserves_count_truncation() {
    check_selected(1, &["(f(a=(1),))"], true);
}

#[test]
fn selected_call_still_charges_payload() {
    let source = "def f(a=0): pass\nf(a=1)\n";
    let operators = operators();
    let lines = [LineRange { start: 2, end: 2 }];
    let mut request = request(&operators, &lines, &[], 10);
    request.payload = payload::PayloadBudget::new(0);
    WORK.set(Work::default());

    let result = analyze_source_cancellable(&request, source, || false);

    assert!(matches!(
        result,
        Err(AnalysisError::PayloadLimit { limit: 0 })
    ));
    assert_eq!(WORK.get().replacements, 1);
}

#[test]
fn selected_call_cancels_after_first_built_replacement() {
    let source = "def f(a=0, b=0): pass\nf(a=1, b=2)\n";
    let operators = operators();
    let request = request(&operators, &[], &[], 10);
    WORK.set(Work::default());

    let result = analyze_source_cancellable(&request, source, || WORK.get().replacements >= 1);

    assert!(matches!(result, Err(AnalysisError::Cancelled)));
    assert_eq!(WORK.get().replacements, 1);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn excluded_call_work_stays_zero_across_keyword_counts_and_limits(
        keywords in 1usize..65,
        limit in 0usize..129,
        payload_bytes in 0usize..1_025,
    ) {
        let source = large_call(keywords);
        let lines = [LineRange { start: 3, end: 3 }];

        check_excluded(&source, &lines, &[], limit, payload_bytes);
    }
}
