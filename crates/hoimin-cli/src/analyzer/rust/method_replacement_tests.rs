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
#[test]
fn unselected_methods_skip_helpers_but_keep_selected_children() {
    let mut unexpected_work = Vec::new();
    for source in [
        "obj.append(1 + 2)",
        "obj.insert(0, 1 + 2)",
        "obj.extend([1 + 2])",
        "obj.get(1 + 2)",
        "(1 + 2).sort()",
        "(1 + 2).reverse()",
        "obj[1 + 2]",
    ] {
        reset();
        let output = analyze(source, MutationOperator::BinaryAddSub, 1);
        assert_eq!(output.candidates.len(), 1, "{source}");
        let candidate = &output.candidates[0];
        assert_eq!(candidate.original, "+", "{source}");
        assert_eq!(candidate.replacement, "-", "{source}");
        assert!(!output.truncated, "{source}");
        if work() != ([0; 7], 0) {
            unexpected_work.push((source, work()));
        }
    }
    assert!(unexpected_work.is_empty(), "{unexpected_work:?}");
}

#[test]
fn nested_unselected_append_does_no_method_replacement_work() {
    for depth in [1, 8, 16, 32] {
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
}

// Positive controls also catch a guard wired to the sibling append family.
#[test]
fn selected_method_families_only_enter_their_own_helpers() {
    for (source, operator, index, expected) in [
        (
            "obj.append(item)",
            MutationOperator::CollectionAppendInsert,
            0,
            "obj.insert(0, item)",
        ),
        (
            "obj.insert(0, item)",
            MutationOperator::CollectionAppendInsert,
            1,
            "obj.append(item)",
        ),
        (
            "obj.append(item)",
            MutationOperator::StructureAppendExtend,
            2,
            "obj.extend([item])",
        ),
        (
            "obj.extend([item])",
            MutationOperator::StructureAppendExtend,
            3,
            "obj.append(item)",
        ),
        (
            "obj.get(key)",
            MutationOperator::StructureMappingGetSubscript,
            4,
            "obj[key]",
        ),
        (
            "obj.sort()",
            MutationOperator::StructureSortReverse,
            5,
            "obj.reverse()",
        ),
        (
            "obj.reverse()",
            MutationOperator::StructureSortReverse,
            5,
            "obj.sort()",
        ),
        (
            "obj[key]",
            MutationOperator::StructureMappingGetSubscript,
            6,
            "obj.get(key)",
        ),
    ] {
        reset();
        let output = analyze(source, operator, 10);
        assert_eq!(output.candidates.len(), 1, "{source}");
        assert_eq!(output.candidates[0].replacement, expected, "{source}");
        let mut entries = [0; 7];
        entries[index] = 1;
        assert_eq!(work(), (entries, 1), "{source}");
    }
}

#[test]
fn counters_observe_direct_helper_entries_and_allocation_boundaries() {
    let sources = [
        "obj.append(item)",
        "obj.insert(0, item)",
        "obj.append(item)",
        "obj.extend([item])",
        "obj.get(key)",
        "obj.sort()",
        "obj[key]",
    ];
    let expected = [
        "obj.insert(0, item)",
        "obj.append(item)",
        "obj.extend([item])",
        "obj.append(item)",
        "obj[key]",
        "obj.reverse()",
        "obj.get(key)",
    ];
    for (index, source) in sources.into_iter().enumerate() {
        let parsed = parse_unchecked_source(source, ruff_python_ast::PySourceType::Python);
        assert!(parsed.has_valid_syntax());
        let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
        let Stmt::Expr(statement) = &parsed.syntax().body[0] else {
            panic!("expression fixture")
        };
        reset();
        let replacement = match statement.value.as_ref() {
            Expr::Call(call) => match index {
                0 => append_to_insert_replacement(source, call, &facts),
                1 => insert_to_append_replacement(source, call, &facts),
                2 => append_to_extend_replacement(source, call),
                3 => extend_to_append_replacement(source, call, &facts),
                4 => mapping_get_to_subscript_replacement(source, call, &facts),
                5 => renamed_method_call_replacement(source, call, "reverse"),
                _ => unreachable!(),
            },
            Expr::Subscript(subscript) => {
                subscript_to_mapping_get_replacement(source, subscript, &facts)
            }
            _ => panic!("call or subscript fixture"),
        };
        assert_eq!(replacement.as_deref(), Some(expected[index]));
        let mut entries = [0; 7];
        entries[index] = 1;
        assert_eq!(work(), (entries, 1));
    }
}

#[test]
fn unselected_parent_keeps_selected_method_in_receiver_and_argument() {
    for source in ["obj.sort().append(item)", "obj.append(obj.sort())"] {
        reset();
        let output = analyze(source, MutationOperator::StructureSortReverse, 1);
        assert_eq!(output.candidates.len(), 1, "{source}");
        assert_eq!(output.candidates[0].original, "obj.sort()");
        assert_eq!(output.candidates[0].replacement, "obj.reverse()");
        assert_eq!(work(), ([0, 0, 0, 0, 0, 1, 0], 1), "{source}");
    }
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
    let Stmt::Expr(statement) = &parsed.syntax().body[0] else {
        panic!("expression fixture")
    };
    let Expr::Call(call) = statement.value.as_ref() else {
        panic!("call fixture")
    };
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
