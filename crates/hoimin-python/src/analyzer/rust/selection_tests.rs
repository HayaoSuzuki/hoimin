//! Selection operation counts observe actual production membership, not elapsed time.
use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Work {
    pub queries: usize,
    pub membership: usize,
    pub preparation: usize,
}
thread_local! {
    static WORK: Cell<Work> = const { Cell::new(Work { queries: 0, membership: 0, preparation: 0 }) };
}
pub(super) fn record_query() {
    WORK.update(|mut work| {
        work.queries += 1;
        work
    });
}
pub(super) fn record_membership() {
    WORK.update(|mut work| {
        work.membership += 1;
        work
    });
}
pub(super) fn record_preparation() {
    WORK.update(|mut work| {
        work.preparation += 1;
        work
    });
}

fn operators(operator: MutationOperator) -> MutationOperatorSelection {
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        for current in MutationOperatorSelection::parse_selector(&name).unwrap() {
            operators.exclude(current);
        }
    }
    operators.include(operator);
    operators
}
fn request<'a>(
    operators: &'a MutationOperatorSelection,
    lines: &'a [LineRange],
    symbols: &'a [String],
    max_candidates: usize,
) -> AnalyzeRequest<'a> {
    AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("pkg/subject.py"),
        lines,
        symbols,
        operators,
        profile: MutationProfile::Full,
        max_candidates,
    }
}

fn sparse_work(n: u32, sparse: bool) {
    // Arrange: all True sites are on odd lines; selectors admit only blank even lines.
    let source = "flag = True\n\n".repeat(usize::try_from(n).unwrap());
    let lines: Vec<_> = if sparse {
        (1..=n)
            .map(|i| LineRange {
                start: 2 * i,
                end: 2 * i,
            })
            .collect()
    } else {
        vec![LineRange {
            start: 2 * n + 1,
            end: 2 * n + 1,
        }]
    };
    let operators = operators(MutationOperator::BooleanLiteral);
    WORK.set(Work::default());

    // Act: max_candidates=1 cannot bypass selection work for discarded sites.
    let output = analyze_source(&request(&operators, &lines, &[], 1), &source);
    let work = WORK.get();

    // Assert: actual membership work grows logarithmically per observed query.
    assert_eq!(output.candidates, Vec::new());
    assert_eq!(output.diagnostics, Vec::new());
    assert!(!output.truncated);
    assert_eq!(work.queries, usize::try_from(n).unwrap());
    let log = lines.len().ilog2() as usize + 1;
    eprintln!(
        "selection n={n} sparse={sparse} queries={} membership={} preparation={}",
        work.queries, work.membership, work.preparation
    );
    assert!(work.membership >= work.queries, "{work:?}");
    assert!(work.membership <= work.queries * (log + 2), "{work:?}");
    assert!(work.preparation >= lines.len(), "{work:?}");
    assert!(work.preparation <= lines.len() * (log + 4), "{work:?}");
}
macro_rules! sparse_case {
    ($name:ident, $n:expr) => {
        #[test]
        fn $name() {
            sparse_work($n, true);
        }
    };
}
sparse_case!(selection_sparse_1000, 1000);
sparse_case!(selection_sparse_2000, 2000);
sparse_case!(selection_sparse_4000, 4000);
sparse_case!(selection_sparse_8000, 8000);
#[test]
fn selection_one_range_control() {
    sparse_work(1000, false);
}

fn membership(ranges: &[(u32, u32)], line: u32) -> bool {
    let operators = operators(MutationOperator::BooleanLiteral);
    let ranges: Vec<_> = ranges
        .iter()
        .map(|&(start, end)| LineRange { start, end })
        .collect();
    CandidateSelection::new(&request(&operators, &ranges, &[], 1), &|| false)
        .unwrap()
        .includes(line, None)
}
macro_rules! membership_case {
    ($name:ident, $ranges:expr, $line:expr, $expected:expr) => {
        #[test]
        fn $name() {
            // Arrange/Act: exercise raw public coordinates, with no caller normalization.
            let actual = membership($ranges, $line);
            // Assert: expected union membership is hand derived.
            assert_eq!(actual, $expected);
        }
    };
}
membership_case!(selection_empty_is_all, &[], 42, true);
membership_case!(selection_all_invalid_is_none, &[(8, 2), (4, 0)], 4, false);
membership_case!(selection_zero_inclusive, &[(0, 0)], 0, true);
membership_case!(
    selection_max_inclusive,
    &[(u32::MAX, u32::MAX)],
    u32::MAX,
    true
);
membership_case!(selection_unsorted, &[(7, 9), (1, 3)], 2, true);
membership_case!(
    selection_nested_prefix_end,
    &[(1, 100), (2, 3), (4, 5)],
    80,
    true
);
membership_case!(selection_overlap, &[(1, 5), (3, 8)], 7, true);
membership_case!(selection_duplicate, &[(3, 3), (3, 3)], 3, true);
membership_case!(selection_adjacent, &[(1, 3), (4, 6)], 4, true);
membership_case!(selection_gap_excluded, &[(1, 3), (5, 6)], 4, false);
membership_case!(selection_end_excluded, &[(1, 3)], 4, false);
membership_case!(selection_start_excluded, &[(3, 5)], 2, false);

fn check_symbol(selector: &str, symbol: Option<&str>, line: u32, expected: bool) {
    let operators = operators(MutationOperator::BooleanLiteral);
    let lines = [LineRange { start: 3, end: 3 }];
    let symbols = [selector.to_owned()];
    let actual = CandidateSelection::new(&request(&operators, &lines, &symbols, 1), &|| false)
        .unwrap()
        .includes(line, symbol);
    assert_eq!(actual, expected);
}
macro_rules! symbol_case {
    ($name:ident, $selector:expr, $symbol:expr, $line:expr, $expected:expr) => {
        #[test]
        fn $name() {
            check_symbol($selector, $symbol, $line, $expected);
        }
    };
}
symbol_case!(
    selection_nested_symbol,
    "outer",
    Some("outer.inner"),
    3,
    true
);
symbol_case!(
    selection_module_symbol,
    "subject:outer",
    Some("outer"),
    3,
    true
);
symbol_case!(
    selection_qualified_module_symbol,
    "pkg.subject:outer",
    Some("outer"),
    3,
    true
);
symbol_case!(
    selection_other_module,
    "other:outer",
    Some("outer"),
    3,
    false
);
symbol_case!(
    selection_symbol_boundary,
    "outer",
    Some("outermost"),
    3,
    false
);
symbol_case!(selection_missing_symbol, "outer", None, 3, false);
symbol_case!(
    selection_line_symbol_conjunction,
    "outer",
    Some("outer"),
    2,
    false
);

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(192))]
    #[test]
    fn selection_arbitrary_u32_membership_matches_linear_oracle(
        ranges in proptest::collection::vec((proptest::prelude::any::<u32>(), proptest::prelude::any::<u32>()), 0..96),
        line in proptest::prelude::any::<u32>(),
    ) {
        let expected = ranges.is_empty() || ranges.iter().any(|&(start, end)| start <= line && line <= end);
        proptest::prop_assert_eq!(membership(&ranges, line), expected);
    }
}

fn producer_selection(source: &str, operator: MutationOperator, line: u32, original: &str) {
    // Arrange: repeated candidates in separate functions exercise scope and line conjunction.
    let operators = operators(operator);
    let lines = [
        LineRange {
            start: 999,
            end: 1000,
        },
        LineRange {
            start: line,
            end: line,
        },
        LineRange {
            start: line,
            end: line,
        },
    ];
    let symbols = ["subject:chosen".to_owned()];
    // Act.
    let output = analyze_source(&request(&operators, &lines, &symbols, 10), source);
    // Assert.
    assert_ne!(output.candidates, Vec::new());
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.line == line
                && candidate.original == original
                && candidate.symbol.as_deref() == Some("chosen"))
    );
    assert_eq!(output.diagnostics, Vec::new());
    assert!(!output.truncated);
}
#[test]
fn selection_token_producer() {
    producer_selection(
        "def other():\n return True\ndef chosen():\n return True\n",
        MutationOperator::BooleanLiteral,
        4,
        "True",
    );
}
#[test]
fn selection_ast_producer() {
    producer_selection(
        "def other():\n return [1, 2]\ndef chosen():\n return [1, 2]\n",
        MutationOperator::ContainerElementDelete,
        4,
        "[1, 2]",
    );
}
#[test]
fn selection_annotation_producer() {
    producer_selection(
        "def other(value: int):\n pass\ndef chosen(value: int):\n pass\n",
        MutationOperator::TypeNullableAdd,
        3,
        "int",
    );
}

fn cancelled_preparation(stop_at: usize) {
    // Arrange: callbacks cover entry, copying, post-sort, and prefix preparation.
    let operators = operators(MutationOperator::BooleanLiteral);
    let lines = [
        LineRange { start: 1, end: 8 },
        LineRange { start: 2, end: 3 },
        LineRange { start: 5, end: 6 },
    ];
    let calls = Cell::new(0);
    // Act.
    let result = CandidateSelection::new(&request(&operators, &lines, &[], 1), &|| {
        calls.set(calls.get() + 1);
        calls.get() == stop_at
    });
    // Assert: no partially prepared owner escapes cancellation.
    assert!(matches!(result, Err(AnalysisCancelled)));
    assert_eq!(calls.get(), stop_at);
}
#[test]
fn selection_cancel_before_preparation() {
    cancelled_preparation(1);
}
#[test]
fn selection_cancel_during_copy() {
    cancelled_preparation(3);
}
#[test]
fn selection_cancel_after_sort() {
    cancelled_preparation(5);
}
#[test]
fn selection_cancel_during_prefix() {
    cancelled_preparation(7);
}

#[test]
fn selection_invalid_syntax_keeps_priority_over_preparation() {
    let operators = operators(MutationOperator::BooleanLiteral);
    let lines = [LineRange { start: 3, end: 1 }];
    WORK.set(Work::default());
    let output = analyze_source(&request(&operators, &lines, &[], 1), "def broken(:\n");
    assert_eq!(
        output.diagnostics[0].code,
        AnalyzerDiagnosticCode::InvalidSyntax
    );
    assert_eq!(WORK.get().preparation, 0);
}

#[test]
fn selection_equivalent_ranges_keep_candidate_order_and_truncation() {
    // Arrange: equivalent unions, including nesting, reversed ranges and duplicates.
    let source = "a = True\nb = False\nc = True\nd = False\n";
    let operators = operators(MutationOperator::BooleanLiteral);
    let canonical = [LineRange { start: 1, end: 4 }];
    let raw = [
        LineRange { start: 3, end: 4 },
        LineRange { start: 1, end: 4 },
        LineRange { start: 2, end: 2 },
        LineRange { start: 5, end: 0 },
    ];
    // Act.
    let expected = analyze_source(&request(&operators, &canonical, &[], 2), source);
    let actual = analyze_source(&request(&operators, &raw, &[], 2), source);
    // Assert: keep the exact prefix, rather than only the candidate count.
    assert_eq!(actual.candidates, expected.candidates);
    assert_eq!(actual.diagnostics, expected.diagnostics);
    assert_eq!(
        actual
            .candidates
            .iter()
            .map(|candidate| candidate.line)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(actual.truncated);
    assert_eq!(actual.truncated, expected.truncated);
}
