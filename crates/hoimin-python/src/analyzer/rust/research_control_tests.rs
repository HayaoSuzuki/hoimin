use super::*;
use rstest::rstest;

fn edits(source: &str, operator: &str) -> Vec<(String, String)> {
    let operators = MutationOperatorSelection::default();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 256,
    };
    analyze_source(&request, source)
        .candidates
        .into_iter()
        .filter(|c| c.operator.as_str() == operator)
        .map(|c| (c.original.clone(), c.replacement.clone()))
        .collect()
}

#[rstest]
#[case::bare_finally(
    "try: enter()\nfinally: exit()\n",
    "finally_unwrap",
    "try: enter()\nfinally",
    "if True: enter()\nif True"
)]
#[case::handled_finally(
    "try: enter()\nexcept OSError: recover()\nfinally: exit()",
    "finally_unwrap",
    "finally",
    "if True"
)]
#[case::case_inline(
    "match value:\n case _ : return_value()",
    "match_case_delete",
    "case _ :",
    "case _ if False:\n     match None:\n         case _ :"
)]
#[case::case_block(
    "match value:\n\tcase [x]:\r\n\t\tuse(x)",
    "match_case_delete",
    "case [x]:",
    "case _ if False:\r\n\t\tmatch None:\r\n\t\t    case [x]: pass"
)]
#[case::nonfinal_capture_still_guarded(
    "match x:\n case y if pred(y): pass\n case _: pass",
    "match_guard_constant",
    "pred(y)",
    "(True or (pred(y)))"
)]
#[case::never_run_guard(
    "match x:\n case y if pred(y): pass",
    "match_guard_constant",
    "pred(y)",
    "(False and (pred(y)))"
)]
#[case::grouped_or(
    "match x:\n case [(1) | ((2))]: pass",
    "match_or_alternative_delete",
    "(1) | ",
    ""
)]
#[case::right_grouped_or(
    "match x:\n case [(1) | ((2))]: pass",
    "match_or_alternative_delete",
    " | ((2))",
    ""
)]
#[case::separate_multiline_directive(
    "a(); b(); c(\n  # type: ignore\n )",
    "statement_order_swap",
    "a(); b()",
    "b(); a()"
)]
#[case::module_swap(
    "state.ready = True\nnotify()",
    "statement_order_swap",
    "state.ready = True\nnotify()",
    "notify()\nstate.ready = True"
)]
#[case::inline_swap(
    "if x: (first()); second()",
    "statement_order_swap",
    "(first()); second()",
    "second(); (first())"
)]
fn emits_control_edit(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    let expected = (before.to_owned(), after.to_owned());

    let candidates = edits(source, operator);

    assert!(candidates.contains(&expected), "{candidates:?}");
}

#[rstest]
#[case::name_dependency("value = load()\nuse(value)", "statement_order_swap")]
#[case::two_literal_stores("a = 1\nb = 2", "statement_order_swap")]
#[case::directive("a()\nb() # type: ignore", "statement_order_swap")]
#[case::binding_in_rhs("first()\nsecond(x := value)", "statement_order_swap")]
#[case::super_delegate("super().start()\nnotify()", "statement_order_swap")]
#[case::literal_guard("match x:\n case y if True: pass", "match_guard_constant")]
fn excludes_unsafe_or_redundant_edit(#[case] source: &str, #[case] operator: &str) {
    let expected: Vec<(String, String)> = Vec::new();

    let candidates = edits(source, operator);

    assert_eq!(candidates, expected);
}

#[rstest]
#[case::unreachable_unguarded("match x:\n case name: pass\n case 2: pass", "match_case_delete")]
#[case::different_alternative_bindings(
    "match x:\n case [a] | [b]: pass",
    "match_or_alternative_delete"
)]
#[case::duplicate_mapping_key("match x:\n case {1: a, True: b}: pass", "match_case_delete")]
#[case::debug_capture("match x:\n case __debug__: pass", "match_case_delete")]
#[case::duplicate_capture("match x:\n case [a, a] if yes: pass", "match_guard_constant")]
#[case::early_irrefutable_or("match x:\n case _ | 1: pass", "match_or_alternative_delete")]
fn rejects_uncompilable_dispatch(#[case] source: &str, #[case] operator: &str) {
    let selected = [operator];

    let result = analyze_selected(&selected, source);

    assert_eq!(result.candidates.len(), 0, "{:?}", result.candidates);
}

#[rstest]
#[case::after_case("match x:\n case(1)|2: pass", "(1)|", " ")]
#[case::before_as("match x:\n case 1|(2)as y: pass", "|(2)", " ")]
fn deletion_preserves_token_separation(
    #[case] source: &str,
    #[case] before: &str,
    #[case] replacement: &str,
) {
    let expected = (before.to_owned(), replacement.to_owned());

    let actual = edits(source, "match_or_alternative_delete");

    assert!(actual.contains(&expected), "{actual:?}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn generated_or_edits_preserve_remaining_equal_binding_alternatives(
        width in 2usize..8, deleted in 0usize..8, grouped in any::<bool>(),
    ) {
        let selected = deleted % width;
        let alts: Vec<_> = (0..width).map(|i| if grouped { format!("([{i}, item])") } else { format!("[{i}, item]") }).collect();
        let source = format!("match v:\n case {}: use(item)\n", alts.join(" | "));
        let expected: Vec<_> = alts.iter().enumerate().filter_map(|(i, alt)| (i != selected).then_some(alt.as_str())).collect();

        let result = analyze_selected(&["match_or_alternative_delete"], &source);
        let mutants: Vec<_> = result.candidates.iter().map(|c| {
            let mut mutant = source.clone();
            let start = usize::try_from(c.span.start).unwrap();
            mutant.replace_range(start..start + usize::try_from(c.span.length).unwrap(), &c.replacement);
            mutant
        }).collect();

        prop_assert_eq!(mutants.len(), width);
        let expected = format!("match v:\n case {}: use(item)\n", expected.join(" | "));
        prop_assert!(mutants.contains(&expected));
        prop_assert!(parse_module(&expected).is_ok());
    }

    #[test]
    fn generated_adjacent_swaps_preserve_other_statements(
        count in 2usize..12, index in 0usize..12, inline in any::<bool>(),
    ) {
        let index = index % (count - 1);
        let separator = if inline { "; " } else { "\r\n" };
        let mut statements: Vec<_> = (0..count).map(|i| format!("record({i})")).collect();
        let source = statements.join(separator);
        statements.swap(index, index+1);
        let expected = statements.join(separator);

        let result = analyze_selected(&["statement_order_swap"], &source);

        prop_assert_eq!(result.candidates.len(), count-1);
        let candidate = &result.candidates[index];
        let mut mutated = source;
        let start = usize::try_from(candidate.span.start).unwrap();
        mutated.replace_range(start..start + usize::try_from(candidate.span.length).unwrap(), &candidate.replacement);
        prop_assert_eq!(&mutated, &expected);
        prop_assert!(parse_module(&mutated).is_ok());
    }

    #[test]
    fn generated_finally_keeps_suite_and_literal_bytes(
        tabs in any::<bool>(), crlf in any::<bool>(), depth in 1usize..6,
    ) {
        let unit = if tabs { "\t" } else { "  " };
        let nl = if crlf { "\r\n" } else { "\n" };
        let mut prelude = String::new();
        for level in 0..depth {
            write!(prelude, "{}if flag:{nl}", unit.repeat(level)).unwrap();
        }
        let indent = unit.repeat(depth);
        let middle = format!(":{nl}{indent}{unit}value = '''a{nl}b'''{nl}{indent}");
        let source = format!("{prelude}{indent}try{middle}finally:{nl}{indent}{unit}close()");

        let actual = edits(&source, "finally_unwrap");

        prop_assert_eq!(actual, vec![(format!("try{middle}finally"), format!("if True{middle}if True"))]);
    }
}

#[rstest]
#[case::final_header(
    "finally_unwrap",
    "def f():\n try: a()\n except OSError: b()\n finally: c()\n",
    4,
    1
)]
#[case::selected_arm(
    "match_case_delete",
    "def f(x):\n match x:\n  case 1: a()\n  case _: b()\n",
    4,
    1
)]
#[case::guard_line(
    "match_guard_constant",
    "def f(x):\n match x:\n  case y if (\n   pred(y)\n  ): return y",
    4,
    2
)]
#[case::nested_suite("statement_order_swap", "def f():\n if flag:\n  a()\n  b()\n", 3, 1)]
fn selection_anchors_at_the_real_edit_start(
    #[case] op: &str,
    #[case] source: &str,
    #[case] line: u32,
    #[case] count: usize,
) {
    let lines = [LineRange {
        start: line,
        end: line,
    }];
    let symbols = ["pkg.sample:f".to_owned()];

    let out = analyze_selected_with(
        &[op],
        Utf8Path::new("pkg/sample.py"),
        &lines,
        &symbols,
        10,
        source,
    );

    assert_eq!(out.candidates.len(), count);
    assert!(
        out.candidates
            .iter()
            .all(|c| c.line == line && c.operator.as_str() == op)
    );
}

#[rstest]
#[case::case_payload("match_case_delete", "match x:\n case 0: pass\n case _: pass")]
#[case::guard_payload("match_guard_constant", "match x:\n case y if pred(y): pass")]
#[case::or_payload("match_or_alternative_delete", "match x:\n case 1 | 2 | 3: pass")]
#[case::order_payload("statement_order_swap", "a()\nb()\nc()")]
#[case::cleanup_payload("finally_unwrap", "try: a()\nfinally: b()")]
fn strict_budget_charges_each_structural_edit(#[case] op: &str, #[case] source: &str) {
    let operators = serde_json::from_value(serde_json::json!([op])).unwrap();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::new(0),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 4,
    };

    let result = analyze_source_cancellable(&request, source, || false);

    assert!(matches!(
        result,
        Err(super::super::AnalysisError::PayloadLimit { limit: 0 })
    ));
}

#[test]
fn case_near_compiler_indent_limit_is_skipped() {
    // Match and function scopes stay below Ruff's independent analysis-depth limit.
    let mut source = String::new();
    for depth in 0..97 {
        writeln!(source, "{}if v:", " ".repeat(depth)).unwrap();
    }
    writeln!(source, "{}match v:", " ".repeat(97)).unwrap();
    writeln!(source, "{}case 1: pass", " ".repeat(98)).unwrap();

    let out = analyze_selected(&["match_case_delete"], &source);

    assert_eq!(out.diagnostics.len(), 0, "{:?}", out.diagnostics);
    assert_eq!(out.candidates.len(), 0, "{:?}", out.candidates);
}
