use super::*;

fn only(operator: MutationOperator) -> MutationOperatorSelection {
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(operator);
        }
    }
    operators.include(operator);
    operators
}

fn analyze(source: &str, operator: MutationOperator, limit: usize) -> AnalyzerOutput {
    analyze_source(
        &AnalyzeRequest {
            payload: super::payload::PayloadBudget::default(),
            path: Utf8Path::new("subject.py"),
            lines: &[],
            symbols: &[],
            operators: &only(operator),
            profile: MutationProfile::Full,
            max_candidates: limit,
        },
        source,
    )
}

fn reset() {
    METHOD_REPLACEMENT_HELPER_ENTRIES.set([0; 7]);
    METHOD_REPLACEMENT_ALLOCATIONS.set(0);
}

fn work() -> ([usize; 7], usize) {
    (
        METHOD_REPLACEMENT_HELPER_ENTRIES.get(),
        METHOD_REPLACEMENT_ALLOCATIONS.get(),
    )
}

// Catches eager helpers even when make_candidate later discards their result.
#[rstest::rstest]
#[case::obj_append_1_2("obj.append(1 + 2)")]
#[case::obj_insert_0_1_2("obj.insert(0, 1 + 2)")]
#[case::obj_extend_1_2("obj.extend([1 + 2])")]
#[case::obj_get_1_2("obj.get(1 + 2)")]
#[case::value_1_2_sort("(1 + 2).sort()")]
#[case::value_1_2_reverse("(1 + 2).reverse()")]
#[case::obj_1_2("obj[1 + 2]")]
fn unselected_methods_skip_helpers_but_keep_selected_children(#[case] source: &str) {
    reset();

    let output = analyze(source, MutationOperator::BinaryAddSub, 1);

    assert_eq!(output.candidates.len(), 1, "{source}");
    let candidate = &output.candidates[0];
    assert_eq!(candidate.original, "+", "{source}");
    assert_eq!(candidate.replacement, "-", "{source}");
    assert!(!output.truncated, "{source}");
    assert_eq!(work(), ([0; 7], 0), "{source}");
}

#[rstest::rstest]
#[case::value_1(1)]
#[case::value_8(8)]
#[case::value_16(16)]
#[case::value_32(32)]
fn nested_unselected_append_does_no_method_replacement_work(#[case] depth: usize) {
    let source = format!(
        "{}('{}', 1 + 2){}\n",
        "obj.append(".repeat(depth),
        "a".repeat(16_384),
        ")".repeat(depth)
    );
    reset();

    let output = analyze(&source, MutationOperator::BinaryAddSub, 1);

    assert_eq!(output.candidates.len(), 1, "depth {depth}");
    assert_eq!(output.candidates[0].original, "+");
    assert_eq!(output.candidates[0].replacement, "-");
    assert!(!output.truncated);
    assert_eq!(work(), ([0; 7], 0), "depth {depth}");
}

// Positive controls also catch a guard wired to the sibling append family.
#[rstest::rstest]
#[case::obj_append_item(
    "obj.append(item)",
    MutationOperator::CollectionAppendInsert,
    0,
    "obj.insert(0, item)"
)]
#[case::obj_insert_0_item(
    "obj.insert(0, item)",
    MutationOperator::CollectionAppendInsert,
    1,
    "obj.append(item)"
)]
#[case::obj_append_item_3(
    "obj.append(item)",
    MutationOperator::StructureAppendExtend,
    2,
    "obj.extend([item])"
)]
#[case::obj_extend_item(
    "obj.extend([item])",
    MutationOperator::StructureAppendExtend,
    3,
    "obj.append(item)"
)]
#[case::obj_get_key(
    "obj.get(key)",
    MutationOperator::StructureMappingGetSubscript,
    4,
    "obj[key]"
)]
#[case::obj_sort(
    "obj.sort()",
    MutationOperator::StructureSortReverse,
    5,
    "obj.reverse()"
)]
#[case::obj_reverse(
    "obj.reverse()",
    MutationOperator::StructureSortReverse,
    5,
    "obj.sort()"
)]
#[case::obj_key(
    "obj[key]",
    MutationOperator::StructureMappingGetSubscript,
    6,
    "obj.get(key)"
)]
fn selected_method_families_only_enter_their_own_helpers(
    #[case] source: &str,
    #[case] operator: MutationOperator,
    #[case] index: usize,
    #[case] expected: &str,
) {
    reset();

    let output = analyze(source, operator, 10);

    assert_eq!(output.candidates.len(), 1, "{source}");
    assert_eq!(output.candidates[0].replacement, expected, "{source}");
    let mut entries = [0; 7];
    entries[index] = 1;
    assert_eq!(work(), (entries, 1), "{source}");
}

#[rstest::rstest]
#[case(0)]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
fn counters_observe_direct_helper_entries_and_allocation_boundaries(#[case] index: usize) {
    type Replacement = fn(&str, &ExprCall, &AstFacts) -> Option<String>;
    let cases: [(&str, &str, Replacement); 6] = [
        (
            "obj.append(item)",
            "obj.insert(0, item)",
            append_to_insert_replacement,
        ),
        (
            "obj.insert(0, item)",
            "obj.append(item)",
            insert_to_append_replacement,
        ),
        (
            "obj.append(item)",
            "obj.extend([item])",
            |source, call, _| append_to_extend_replacement(source, call),
        ),
        (
            "obj.extend([item])",
            "obj.append(item)",
            extend_to_append_replacement,
        ),
        (
            "obj.get(key)",
            "obj[key]",
            mapping_get_to_subscript_replacement,
        ),
        ("obj.sort()", "obj.reverse()", |source, call, _| {
            renamed_method_call_replacement(source, call, "reverse")
        }),
    ];
    let (source, expected, replace) = cases.into_iter().nth(index).unwrap();
    let parsed = parse_unchecked_source(source, ruff_python_ast::PySourceType::Python);

    assert!(parsed.has_valid_syntax());
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    let statement = parsed.syntax().body[0].as_expr_stmt().unwrap();
    let call = statement.value.as_call_expr().unwrap();
    let mut entries = [0; 7];
    entries[index] = 1;
    reset();

    let replacement = replace(source, call, &facts);

    assert_eq!(replacement.as_deref(), Some(expected));
    assert_eq!(work(), (entries, 1));
}

#[test]
fn counters_observe_direct_subscript_helper_entry_and_allocation() {
    let source = "obj[key]";
    let parsed = parse_unchecked_source(source, ruff_python_ast::PySourceType::Python);

    assert!(parsed.has_valid_syntax());
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    let statement = parsed.syntax().body[0].as_expr_stmt().unwrap();
    let subscript = statement.value.as_subscript_expr().unwrap();
    reset();

    let replacement = subscript_to_mapping_get_replacement(source, subscript, &facts);

    assert_eq!(replacement.as_deref(), Some("obj.get(key)"));
    assert_eq!(work(), ([0, 0, 0, 0, 0, 0, 1], 1));
}

#[rstest::rstest]
#[case::obj_sort_append_item("obj.sort().append(item)")]
#[case::obj_append_obj_sort("obj.append(obj.sort())")]
fn unselected_parent_keeps_selected_method_in_receiver_and_argument(#[case] source: &str) {
    reset();

    let output = analyze(source, MutationOperator::StructureSortReverse, 1);

    assert_eq!(output.candidates.len(), 1, "{source}");
    assert_eq!(output.candidates[0].original, "obj.sort()");
    assert_eq!(output.candidates[0].replacement, "obj.reverse()");
    assert_eq!(work(), ([0, 0, 0, 0, 0, 1, 0], 1), "{source}");
}

#[test]
fn subscript_payload_failure_stops_before_mapping_builder() {
    let budget = payload::PayloadBudget::new(1);
    let mut operators = only(MutationOperator::StructureIndexNeighbor);
    operators.include(MutationOperator::StructureMappingGetSubscript);
    let request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    reset();

    let result = analyze_source_cancellable(&request, "x = items[1]\n", || false);

    assert_eq!(result.err(), Some(AnalysisError::PayloadLimit { limit: 1 }));
    assert_eq!(budget.snapshot().used, 0);
    assert_eq!(budget.snapshot().generated, 1);
    assert_eq!(work(), ([0; 7], 0));
}

#[test]
fn unselected_mapping_helper_preserves_index_neighbors() {
    reset();

    let output = analyze("obj[1]", MutationOperator::StructureIndexNeighbor, 10);
    let replacements: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| candidate.replacement.as_str())
        .collect();

    assert_eq!(replacements, ["2", "0"]);
    assert_eq!(work(), ([0; 7], 0));
}

#[test]
fn rejected_helper_entry_does_not_count_a_string_construction() {
    let source = "obj.extend(items)";
    let parsed = parse_unchecked_source(source, ruff_python_ast::PySourceType::Python);
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    let statement = parsed.syntax().body[0].as_expr_stmt().unwrap();
    let call = statement.value.as_call_expr().unwrap();
    reset();

    assert!(extend_to_append_replacement(source, call, &facts).is_none());
    assert_eq!(work(), ([0, 0, 0, 1, 0, 0, 0], 0));
}

#[test]
fn unselected_mapping_helper_preserves_slice_neighbors() {
    reset();

    let output = analyze("obj[1:3]", MutationOperator::StructureSliceNeighbor, 10);
    let replacements: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| (candidate.original.as_str(), candidate.replacement.as_str()))
        .collect();

    assert_eq!(
        replacements,
        [("1", "2"), ("1", "0"), ("3", "4"), ("3", "2")]
    );
    assert_eq!(work(), ([0; 7], 0));
}

type BoundaryReplacement = fn(&str, &ExprCall, &AstFacts) -> Option<String>;
fn boundary_append_to_extend(source: &str, call: &ExprCall, _: &AstFacts) -> Option<String> {
    append_to_extend_replacement(source, call)
}

#[rstest::rstest]
#[case("function(item)", append_to_insert_replacement)]
#[case("obj.append()", append_to_insert_replacement)]
#[case("function(0, item)", insert_to_append_replacement)]
#[case("obj.insert(0)", insert_to_append_replacement)]
#[case("obj.insert(0, (yield item))", insert_to_append_replacement)]
#[case("obj.insert(0, (yield from items))", insert_to_append_replacement)]
#[case("function(item)", boundary_append_to_extend)]
#[case("function([item])", extend_to_append_replacement)]
#[case("obj.extend()", extend_to_append_replacement)]
#[case("obj.extend(items)", extend_to_append_replacement)]
#[case("obj.extend([])", extend_to_append_replacement)]
#[case("obj.extend([first, second])", extend_to_append_replacement)]
#[case("obj.extend([*items])", extend_to_append_replacement)]
#[case("function(key)", mapping_get_to_subscript_replacement)]
fn method_edit_helpers_reject_unsupported_call_shapes(
    #[case] source: &str,
    #[case] helper: BoundaryReplacement,
) {
    let parsed = parse_module(source).unwrap();
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    let call = parsed.syntax().body[0]
        .as_expr_stmt()
        .unwrap()
        .value
        .as_call_expr()
        .unwrap();

    let actual = helper(source, call, &facts);

    assert_eq!(actual, None);
}

#[rstest::rstest]
#[case("", TextRange::new(0.into(), 0.into()))]
#[case("prefix; obj.call()", TextRange::new(0.into(), 1.into()))]
#[case("prefix; obj.call()", TextRange::new(8.into(), 99.into()))]
fn call_replacement_rejects_source_or_edits_outside_the_call(
    #[case] source: &str,
    #[case] edit: TextRange,
) {
    let original = "prefix; obj.call()";
    let parsed = parse_module(original).unwrap();
    let call = parsed.syntax().body[1]
        .as_expr_stmt()
        .unwrap()
        .value
        .as_call_expr()
        .unwrap();

    let actual = replace_within_call(source, call, [(edit, "replacement")]);

    assert_eq!(actual, None);
}

type LiteralReplacement = fn(&str, &Expr, &AstFacts) -> Option<String>;
fn rewrite_list_literal(source: &str, expression: &Expr, facts: &AstFacts) -> Option<String> {
    list_to_tuple_replacement(source, expression.as_list_expr().unwrap(), facts)
}
fn rewrite_tuple_literal(source: &str, expression: &Expr, _: &AstFacts) -> Option<String> {
    tuple_to_list_replacement(source, expression.as_tuple_expr().unwrap())
}
fn add_exception_member(source: &str, expression: &Expr, facts: &AstFacts) -> Option<String> {
    tuple_add_replacement(
        source,
        expression.as_tuple_expr().unwrap(),
        facts,
        "TypeError",
    )
}

#[rstest::rstest]
#[case("[item]", "", rewrite_list_literal, None)]
#[case("[item]", "(item)", rewrite_list_literal, None)]
#[case("[item]", "[item)", rewrite_list_literal, None)]
#[case("[item]", "[item]", rewrite_list_literal, Some("(item,)"))]
#[case("(item,)", "", rewrite_tuple_literal, None)]
#[case("(item,)", "[item,]", rewrite_tuple_literal, None)]
#[case("(item,)", "(item,]", rewrite_tuple_literal, None)]
#[case("(item,)", "(item,)", rewrite_tuple_literal, Some("[item,]"))]
#[case("(ValueError,)", "", add_exception_member, None)]
#[case("(ValueError,)", "[ValueError,]", add_exception_member, None)]
#[case("(ValueError,)", "(ValueError,]", add_exception_member, None)]
#[case(
    "(ValueError,)",
    "(ValueError,)",
    add_exception_member,
    Some("(ValueError, TypeError)")
)]
#[case(
    "(ValueError, RuntimeError)",
    "(ValueError, RuntimeError)",
    add_exception_member,
    Some("(ValueError, RuntimeError, TypeError)")
)]
fn literal_rewrites_require_complete_matching_delimiters(
    #[case] original: &str,
    #[case] source: &str,
    #[case] rewrite: LiteralReplacement,
    #[case] expected: Option<&str>,
) {
    let parsed = parse_module(original).unwrap();
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), original);
    let expression = &parsed.syntax().body[0].as_expr_stmt().unwrap().value;

    let actual = rewrite(source, expression, &facts);

    assert_eq!(actual.as_deref(), expected);
}

#[rstest::rstest]
#[case("1", Some((1, false)))]
#[case("-1", Some((-1, true)))]
#[case("-0", Some((0, true)))]
#[case("-(1)", Some((-1, true)))]
#[case("0x1", None)]
#[case("1_0", None)]
#[case("+1", None)]
#[case("1.0", None)]
#[case("18446744073709551616", None)]
#[case("'1'", None)]
fn integer_neighbors_accept_only_supported_decimal_spelling_and_magnitude(
    #[case] source: &str,
    #[case] expected: Option<(i128, bool)>,
) {
    let parsed = ruff_python_parser::parse_expression(source).unwrap();

    let actual = decimal_literal_value(source, &parsed.syntax().body);

    assert_eq!(actual, expected);
}

fn boundary_rename_ordering(source: &str, call: &ExprCall, _: &AstFacts) -> Option<String> {
    renamed_method_call_replacement(source, call, "reverse")
}

#[rstest::rstest]
#[case("obj.insert(0, item)", vec![TokenKind::Comma], 19, insert_to_append_replacement)]
#[case("obj.extend([item])", vec![TokenKind::Lsqb], 18, extend_to_append_replacement)]
#[case("obj.extend([item])", vec![TokenKind::Rsqb], 18, extend_to_append_replacement)]
#[case("obj.get(key)", vec![], 0, mapping_get_to_subscript_replacement)]
#[case("obj.get(key)", vec![], 3, mapping_get_to_subscript_replacement)]
#[case("function()", vec![], 10, boundary_rename_ordering)]
fn call_edit_helpers_reject_missing_metadata_or_unavailable_fragments(
    #[case] source: &str,
    #[case]
    #[by_ref]
    omitted: &[TokenKind],
    #[case] available_len: usize,
    #[case] helper: BoundaryReplacement,
) {
    let parsed = parse_module(source).unwrap();
    let tokens = ruff_python_ast::token::Tokens::new(
        parsed
            .tokens()
            .iter()
            .filter(|token| !omitted.contains(&token.kind()))
            .copied()
            .collect(),
    );
    let mut facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    facts.tokens = Some(&tokens);
    let call = parsed.syntax().body[0]
        .as_expr_stmt()
        .unwrap()
        .value
        .as_call_expr()
        .unwrap();

    let actual = helper(&source[..available_len.min(source.len())], call, &facts);

    assert_eq!(actual, None);
}

fn boundary_subscript_to_get(source: &str, expression: &Expr, facts: &AstFacts) -> Option<String> {
    subscript_to_mapping_get_replacement(source, expression.as_subscript_expr().unwrap(), facts)
}

#[rstest::rstest]
#[case(vec![], 0)]
#[case(vec![], 3)]
#[case(vec![TokenKind::Lsqb], 8)]
#[case(vec![TokenKind::Rsqb], 8)]
fn subscript_edit_requires_bracket_metadata_and_available_key_source(
    #[case]
    #[by_ref]
    omitted: &[TokenKind],
    #[case] available_len: usize,
) {
    let source = "obj[key]";
    let parsed = parse_module(source).unwrap();
    let tokens = ruff_python_ast::token::Tokens::new(
        parsed
            .tokens()
            .iter()
            .filter(|token| !omitted.contains(&token.kind()))
            .copied()
            .collect(),
    );
    let mut facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    facts.tokens = Some(&tokens);
    let expression = &parsed.syntax().body[0].as_expr_stmt().unwrap().value;

    let actual = boundary_subscript_to_get(&source[..available_len], expression, &facts);

    assert_eq!(actual, None);
}

#[rstest::rstest]
#[case("1")]
#[case("-1")]
fn decimal_literal_decoder_rejects_unavailable_operand_source(#[case] source: &str) {
    let parsed = parse_module(source).unwrap();
    let expression = &parsed.syntax().body[0].as_expr_stmt().unwrap().value;

    let actual = decimal_literal_value("", expression);

    assert_eq!(actual, None);
}
