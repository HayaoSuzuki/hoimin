//! Counts completed replacement strings at the builders, before candidate admission.
use super::*;
use proptest::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ReplacementWork {
    builds: usize,
    bytes: usize,
}

thread_local! {
    static REPLACEMENT_WORK: Cell<ReplacementWork> = const {
        Cell::new(ReplacementWork { builds: 0, bytes: 0 })
    };
}

pub(super) fn record_replacement(operator: MutationOperator, replacement: &str) {
    assert!(matches!(
        operator,
        MutationOperator::ContainerElementDelete | MutationOperator::ConditionClauseDelete
    ));
    REPLACEMENT_WORK.set(ReplacementWork {
        builds: REPLACEMENT_WORK.get().builds + 1,
        bytes: REPLACEMENT_WORK.get().bytes + replacement.len(),
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
        path: Utf8Path::new("pkg/subject.py"),
        lines,
        symbols,
        operators,
        profile: MutationProfile::Full,
        max_candidates,
    }
}

fn check_excluded(source: &str, operator: MutationOperator, line: u32, symbol: Option<&str>) {
    // Arrange: the requested start line or scope excludes the whole expression.
    let operators = operators(operator);
    let lines = (line != 0)
        .then_some(LineRange {
            start: line,
            end: line,
        })
        .into_iter()
        .collect::<Vec<_>>();
    let symbols = symbol.map(str::to_owned).into_iter().collect::<Vec<_>>();
    REPLACEMENT_WORK.set(ReplacementWork::default());

    // Act: use the full analyzer, including child traversal and semantic analysis.
    let output = analyze_source(&request(&operators, &lines, &symbols, 10), source);

    // Assert: discarded strings were never constructed, rather than just rejected.
    assert_eq!(
        REPLACEMENT_WORK.get(),
        ReplacementWork {
            builds: 0,
            bytes: 0
        }
    );
    assert_eq!(output.candidates, Vec::new());
    assert_eq!(output.diagnostics, Vec::new());
    assert!(!output.truncated);
}

macro_rules! excluded_case {
    ($name:ident, $source:expr, $operator:ident, $line:expr, $symbol:expr) => {
        #[test]
        fn $name() {
            check_excluded($source, MutationOperator::$operator, $line, $symbol);
        }
    };
}

excluded_case!(
    container_excluded_line,
    "value = ['aaaaaaaa', 'bbbbbbbb', 'cccccccc']\nflag = True\n",
    ContainerElementDelete,
    2,
    None
);
excluded_case!(
    container_excluded_symbol,
    "def excluded():\n return ['aaaaaaaa', 'bbbbbbbb']\ndef chosen():\n return True\n",
    ContainerElementDelete,
    0,
    Some("subject:chosen")
);
excluded_case!(
    container_multiline_interior_line,
    "value = [\n 'aaaaaaaa',\n 'bbbbbbbb',\n]\n",
    ContainerElementDelete,
    2,
    None
);
excluded_case!(
    condition_excluded_line,
    "if aaaaaaaaa and bbbbbbbbb and ccccccccc: pass\nflag = True\n",
    ConditionClauseDelete,
    2,
    None
);
excluded_case!(
    condition_excluded_symbol,
    "def excluded():\n if aaaaaaaaa and bbbbbbbbb: pass\ndef chosen():\n return True\n",
    ConditionClauseDelete,
    0,
    Some("subject:chosen")
);
excluded_case!(
    condition_multiline_interior_line,
    "if (aaaaaaaaa and\n bbbbbbbbb): pass\n",
    ConditionClauseDelete,
    2,
    None
);

fn check_selected_child(
    source: &str,
    operator: MutationOperator,
    line: u32,
    expected_original: &str,
    expected_replacements: &[&str],
    expected_bytes: usize,
) {
    // Arrange: select only the child; its multiline parent starts elsewhere.
    let operators = operators(operator);
    let lines = [LineRange {
        start: line,
        end: line,
    }];
    REPLACEMENT_WORK.set(ReplacementWork::default());

    // Act.
    let output = analyze_source(&request(&operators, &lines, &[], 10), source);

    // Assert: traversal reaches the child, but never builds the parent's edits.
    assert_eq!(
        REPLACEMENT_WORK.get(),
        ReplacementWork {
            builds: expected_replacements.len(),
            bytes: expected_bytes,
        }
    );
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| candidate.replacement.as_str())
            .collect::<Vec<_>>(),
        expected_replacements
    );
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.line == line && candidate.original == expected_original)
    );
    assert_eq!(output.diagnostics, Vec::new());
    assert!(!output.truncated);
}

#[test]
fn container_selected_child_inside_excluded_parent() {
    check_selected_child(
        "value = [\n [1, 2],\n 3,\n]\n",
        MutationOperator::ContainerElementDelete,
        2,
        "[1, 2]",
        &["[(2),]", "[(1),]"],
        12,
    );
}

#[test]
fn condition_selected_child_inside_excluded_parent() {
    check_selected_child(
        "if (a and\n b):\n if c and d: pass\n",
        MutationOperator::ConditionClauseDelete,
        3,
        "c and d",
        &["((d))", "((c))"],
        10,
    );
}

fn check_selected_identity(
    source: &str,
    operator: MutationOperator,
    replacements: &[&str],
    limit: usize,
) {
    // Arrange: an earlier unselected expression and repeated adjacent entries.
    let operators = operators(operator);
    let lines = [LineRange { start: 4, end: 4 }];
    let symbols = ["pkg.subject:chosen".to_owned()];
    let full = analyze_source(&request(&operators, &[], &[], 20), source);
    let expected = full
        .candidates
        .into_iter()
        .filter(|candidate| candidate.line == 4)
        .collect::<Vec<_>>();
    REPLACEMENT_WORK.set(ReplacementWork::default());

    // Act.
    let output = analyze_source(&request(&operators, &lines, &symbols, limit), source);

    // Assert: literal expectations check ordering/dedup; reference checks all identity fields.
    assert_eq!(
        output
            .candidates
            .iter()
            .map(|candidate| candidate.replacement.as_str())
            .collect::<Vec<_>>(),
        replacements
    );
    assert_eq!(output.candidates, expected[..limit.min(expected.len())]);
    assert_eq!(output.truncated, expected.len() > limit);
    let identity = |candidate: AnalyzerCandidate| hoimin_core::CandidateIdentity {
        schema_version: hoimin_core::CANDIDATE_SCHEMA_VERSION,
        file_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        path: candidate.path,
        span: candidate.span,
        operator: candidate.operator.as_str().to_owned(),
        replacement: candidate.replacement,
    };
    for (candidate, expected) in output.candidates.into_iter().zip(expected) {
        assert_eq!(
            hoimin_core::stable_mutant_id(&identity(candidate)),
            hoimin_core::stable_mutant_id(&identity(expected)),
        );
    }
}

macro_rules! identity_case {
    ($name:ident, $source:expr, $operator:ident, $replacements:expr, $limit:expr) => {
        #[test]
        fn $name() {
            check_selected_identity($source, MutationOperator::$operator, $replacements, $limit);
        }
    };
}
identity_case!(
    container_selected_order_dedup_identity,
    "def excluded():\n return [4, 5]\ndef chosen():\n return [1, 1, 2]\n",
    ContainerElementDelete,
    &["[(1),(2),]", "[(1),(1),]"],
    10
);
identity_case!(
    container_selected_count_truncation,
    "def excluded():\n return [4, 5]\ndef chosen():\n return [1, 1, 2]\n",
    ContainerElementDelete,
    &["[(1),(2),]"],
    1
);
identity_case!(
    condition_selected_order_dedup_identity,
    "def excluded():\n if d and e: pass\ndef chosen():\n if a and a and b: pass\n",
    ConditionClauseDelete,
    &["((a) and (b))", "((a) and (a))"],
    10
);
identity_case!(
    condition_selected_count_truncation,
    "def excluded():\n if d and e: pass\ndef chosen():\n if a and a and b: pass\n",
    ConditionClauseDelete,
    &["((a) and (b))"],
    1
);

fn check_cancel_during_construction(source: &str, operator: MutationOperator) {
    // Arrange: cancellation becomes true only after one real replacement is built.
    let operators = operators(operator);
    let request = request(&operators, &[], &[], 10);
    REPLACEMENT_WORK.set(ReplacementWork::default());

    // Act.
    let result =
        analyze_source_cancellable(&request, source, || REPLACEMENT_WORK.get().builds >= 1);

    // Assert: in-loop cancellation aborts instead of returning partial success.
    assert!(result.is_err());
    assert_eq!(REPLACEMENT_WORK.get().builds, 1);
    assert!(REPLACEMENT_WORK.get().bytes > 0);
}

#[test]
fn container_cancels_after_first_constructed_replacement() {
    check_cancel_during_construction(
        "value = [1, 2, 3]\n",
        MutationOperator::ContainerElementDelete,
    );
}

#[test]
fn condition_cancels_after_first_constructed_replacement() {
    check_cancel_during_construction(
        "if a and b and c: pass\n",
        MutationOperator::ConditionClauseDelete,
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn excluded_container_work_is_zero_for_bounded_payloads(elements in 2usize..33, width in 1usize..129) {
        // Arrange: at most 32 strings with 128-byte suffixes, excluded by start line.
        let elements = (0..elements).map(|index| format!("'{index}{}'", "x".repeat(width)))
            .collect::<Vec<_>>().join(",");
        let source = format!("value = [{elements}]\nflag = True\n");

        // Act / Assert: a missing pre-generation guard makes this fail for every size.
        check_excluded(&source, MutationOperator::ContainerElementDelete, 2, None);
    }

    #[test]
    fn excluded_condition_work_is_zero_for_bounded_payloads(operands in 2usize..33, width in 1usize..129) {
        // Arrange: at most 32 names with 128-byte suffixes, excluded by start line.
        let operands = (0..operands).map(|index| format!("x{index}{}", "x".repeat(width)))
            .collect::<Vec<_>>().join(" and ");
        let source = format!("if {operands}: pass\nflag = True\n");

        // Act / Assert.
        check_excluded(&source, MutationOperator::ConditionClauseDelete, 2, None);
    }
}
