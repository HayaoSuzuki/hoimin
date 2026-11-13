//! Producers must preserve the first failure without polling or retaining work.
use super::*;

thread_local! {
    static POLLS: Cell<usize> = const { Cell::new(0) };
}
fn counted_poll() -> bool {
    POLLS.set(POLLS.get() + 1);
    false
}

#[derive(Clone, Copy)]
enum Attempt {
    StringSegment,
    Integer,
    ConditionDelete,
    ConditionConstant,
    Call,
    MethodRemove,
    StdlibConversion,
    Conversion,
    Builtin,
    Method,
    StructuralMethod,
    Subscript,
    Index,
    Slice,
    List,
    Tuple,
    Raise,
    Handler,
    ExceptionPair,
    RiskyHandler,
    FunctionBody,
    Augmented,
    TupleHandler,
}

fn expression(module: &ModModule) -> &Expr {
    &module.body.last().unwrap().as_expr_stmt().unwrap().value
}
fn call(module: &ModModule) -> &ExprCall {
    expression(module).as_call_expr().unwrap()
}
fn handler(module: &ModModule) -> &ruff_python_ast::ExceptHandler {
    &module.body[0].as_try_stmt().unwrap().handlers[0]
}
fn handler_body(module: &ModModule) -> &ruff_python_ast::ExceptHandlerExceptHandler {
    let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = handler(module);
    handler
}

fn invoke(
    collector: &mut AstCandidateCollector<'_, &dyn Fn() -> bool>,
    module: &ModModule,
    attempt: Attempt,
) {
    match attempt {
        Attempt::StringSegment => {
            collector.collect_string_segment(expression(module));
        }
        Attempt::Integer => {
            let _signed = collector.collect_integer_literal(expression(module));
        }
        Attempt::ConditionDelete => {
            collector.collect_condition_clause_delete(expression(module));
        }
        Attempt::ConditionConstant => {
            collector.collect_condition_constant(expression(module));
        }
        Attempt::Call => {
            collector.collect_call(call(module));
        }
        Attempt::MethodRemove => {
            collector.collect_method_call_remove(
                call(module),
                &call(module).func.as_attribute_expr().unwrap().value,
            );
        }
        Attempt::StdlibConversion => {
            collector.collect_stdlib_conversion_call_remove(call(module));
        }
        Attempt::Conversion => {
            collector.collect_conversion_call_remove(
                call(module),
                call(module).func.as_name_expr().unwrap(),
            );
        }
        Attempt::Builtin => {
            collector.collect_builtin_call(
                call(module),
                call(module).func.as_name_expr().unwrap().id.as_str(),
                call(module).func.range(),
            );
        }
        Attempt::Method => {
            collector.collect_method_call(
                call(module),
                "append",
                call(module).func.as_attribute_expr().unwrap().attr.range(),
            );
        }
        Attempt::StructuralMethod => {
            collector.collect_structural_method_call(call(module), "get");
        }
        Attempt::Subscript => {
            collector.collect_subscript(expression(module).as_subscript_expr().unwrap());
        }
        Attempt::Index => {
            collector.collect_index_neighbors(expression(module));
        }
        Attempt::Slice => {
            collector.collect_slice_neighbors(
                expression(module)
                    .as_subscript_expr()
                    .unwrap()
                    .slice
                    .as_slice_expr()
                    .unwrap(),
            );
        }
        Attempt::List => {
            collector.collect_list_literal(expression(module).as_list_expr().unwrap());
        }
        Attempt::Tuple => {
            collector.collect_tuple_literal(expression(module).as_tuple_expr().unwrap());
        }
        Attempt::Raise => {
            collector.collect_raised_exception(module.body[0].as_raise_stmt().unwrap());
        }
        Attempt::Handler => {
            collector.collect_exception_handler(handler(module));
        }
        Attempt::ExceptionPair => {
            collector.collect_exception_type_pair(handler_body(module));
        }
        Attempt::RiskyHandler => {
            collector.collect_risky_exception_handler(handler(module), handler_body(module));
        }
        Attempt::FunctionBody => {
            collector.collect_function_body_return_constant(&module.body[0]);
        }
        Attempt::Augmented => {
            collector.collect_augmented_to_assignment(module.body[0].as_aug_assign_stmt().unwrap());
        }
        Attempt::TupleHandler => {
            collector.collect_tuple_exception_handler(expression(module).as_tuple_expr().unwrap());
        }
    }
}

#[derive(Clone, Copy)]
enum State {
    Active,
    Failed,
    Pattern,
    Target,
    SuppressedInteger,
    ExceptionType,
    Cancelled,
}

fn prefix_for_state(
    budget: &PayloadBudget,
    state: State,
) -> (CandidatePrefix, Option<AnalysisError>) {
    let mut prefix = CandidatePrefix::new(10, budget.clone());
    let failure = match state {
        State::Failed => Some(
            prefix
                .push(AnalyzerCandidate {
                    path: "sample.py".into(),
                    span: ByteSpan {
                        start: 0,
                        length: 1,
                    },
                    original: "1".into(),
                    replacement: "2".into(),
                    operator: MutationOperator::IntegerLiteralNeighbor,
                    line: 1,
                    column: 1,
                    symbol: None,
                })
                .unwrap_err(),
        ),
        _ => None,
    };
    (prefix, failure)
}

fn observe_attempt(
    source: &str,
    attempt: Attempt,
    state: State,
) -> (usize, bool, usize, usize, usize, bool) {
    let parsed = parse_module(source).unwrap();
    let module = parsed.syntax();
    let facts = AstFacts::from_module(module, parsed.tokens(), source);
    let line_index = LineIndex::new(source);
    let mut operators = MutationOperatorSelection::default();
    for selector in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(selector).unwrap() {
            operators.include(operator);
        }
    }
    let budget = match state {
        State::Failed => PayloadBudget::new(0),
        _ => PayloadBudget::default(),
    };
    let request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    // Index setup completes before the direct producer's cancellation phase.
    let invoking = Cell::new(false);
    let cancelled: &dyn Fn() -> bool = &|| {
        counted_poll();
        invoking.get() && matches!(state, State::Cancelled)
    };
    let selection = CandidateSelection::new(&request, &cancelled).unwrap();
    let (candidates, failure) = prefix_for_state(&budget, state);
    let enum_index = enum_members::EnumIndex::default();
    let dataclass_factories = dataclass_factory::DataclassFactories::default();
    let followup_calls = followup_calls::Imports::default();
    let mut collector = AstCandidateCollector {
        inheritance: inheritance::Index::default(),
        string_exclusions: string_literals::Exclusions::build(module, &cancelled).unwrap(),
        enum_index: &enum_index,
        dataclass_factories: &dataclass_factories,
        followup_calls: &followup_calls,
        optional_functions: optional_keywords::FunctionIndex::default(),
        operator_imports: OperatorImports::default(),
        behavior_imports: BehaviorImports::default(),
        conversion_imports: stdlib_conversions::ConversionImports::build(module, &cancelled)
            .unwrap(),
        return_scope: ReturnScope::Excluded,
        enclosing_body: EnclosingBody::Module,
        comparison_scope: ComparisonReturnScope::Excluded,
        integer_suppressed: matches!(state, State::SuppressedInteger),
        runtime_role: match state {
            State::Target => RuntimeRole::AssignmentTarget,
            _ => RuntimeRole::Value,
        },
        in_pattern: matches!(state, State::Pattern),
        source,
        line_index: &line_index,
        facts: &facts,
        request: &request,
        selection: &selection,
        cancelled: &cancelled,
        cancelled_observed: false,
        exception_handler_finality: Vec::new(),
        exception_type_depth: usize::from(matches!(state, State::ExceptionType)),
        candidates,
    };
    POLLS.set(0);
    invoking.set(true);
    invoke(&mut collector, module, attempt);
    let snapshot = budget.snapshot();
    (
        collector.candidates.entries.len(),
        collector.candidates.failure == failure,
        POLLS.get(),
        snapshot.generated,
        snapshot.used,
        collector.cancelled_observed,
    )
}

#[rstest::rstest]
#[case::stringsegment("'first' 'second'", Attempt::StringSegment)]
#[case::integer("1", Attempt::Integer)]
#[case::conditiondelete("first and second", Attempt::ConditionDelete)]
#[case::conditionconstant("condition", Attempt::ConditionConstant)]
#[case::call("any(items)", Attempt::Call)]
#[case::methodremove("items.copy()", Attempt::MethodRemove)]
#[case::stdlibconversion(
    "from decimal import Decimal\nDecimal(value)",
    Attempt::StdlibConversion
)]
#[case::conversion("int(value)", Attempt::Conversion)]
#[case::builtin("any(items)", Attempt::Builtin)]
#[case::method("items.append(item)", Attempt::Method)]
#[case::structuralmethod("items.get(key)", Attempt::StructuralMethod)]
#[case::subscript("items[index]", Attempt::Subscript)]
#[case::index("1", Attempt::Index)]
#[case::slice("items[1:2:3]", Attempt::Slice)]
#[case::list("[first, second]", Attempt::List)]
#[case::tuple("(first, second)", Attempt::Tuple)]
#[case::raise("raise ValueError()", Attempt::Raise)]
#[case::handler("try:\n pass\nexcept ValueError:\n pass", Attempt::Handler)]
#[case::exceptionpair("try:\n pass\nexcept ValueError:\n pass", Attempt::ExceptionPair)]
#[case::riskyhandler("try:\n pass\nexcept:\n pass", Attempt::RiskyHandler)]
#[case::functionbody("def function() -> int:\n return 1", Attempt::FunctionBody)]
#[case::augmented("value += 1", Attempt::Augmented)]
#[case::tuplehandler("(ValueError, TypeError)", Attempt::TupleHandler)]
fn failed_producers_keep_the_first_error_without_polling_or_emitting(
    #[case] source: &str,
    #[case] attempt: Attempt,
) {
    assert_eq!(
        observe_attempt(source, attempt, State::Failed),
        (0, true, 0, 0, 0, false)
    );
}

fn run_ast(
    source: &str,
    operators: &MutationOperatorSelection,
    payload: &PayloadBudget,
    cancelled: &dyn Fn() -> bool,
) -> Result<ProducerPrefix, AnalysisError> {
    let parsed = parse_module(source).unwrap();
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    let line_index = LineIndex::new(source);
    let request = AnalyzeRequest {
        payload: payload.clone(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let selection = CandidateSelection::new(&request, &cancelled)?;
    AstCandidateCollector::collect(
        parsed.syntax(),
        source,
        &line_index,
        &facts,
        &request,
        &selection,
        &cancelled,
        None,
    )
    .map(|(prefix, _, _)| prefix)
}

#[rstest::rstest]
#[case("value = 'first' 'second'", MutationOperator::StringSegmentEmpty, vec!["\"\""])]
#[case("1", MutationOperator::IntegerLiteralNeighbor, vec!["(0)", "(2)"])]
#[case("18446744073709551616", MutationOperator::IntegerLiteralNeighbor, vec![])]
#[case("-18446744073709551616", MutationOperator::IntegerLiteralNeighbor, vec![])]
#[case("def f():\n items.insert(0, (yield value))", MutationOperator::CollectionAppendInsert, vec![])]
#[case("if first and second:\n pass", MutationOperator::ConditionClauseDelete, vec!["((second))", "((first))"])]
#[case("if condition:\n pass", MutationOperator::ConditionConstant, vec!["(True)", "(False)"])]
#[case("any(items)", MutationOperator::CollectionAnyAll, vec!["all"])]
#[case("items.copy()", MutationOperator::MethodCallRemove, vec!["(items)"])]
#[case("from decimal import Decimal\nDecimal(value)", MutationOperator::StdlibConversionCallRemove, vec!["(value)"])]
#[case("int(value)", MutationOperator::ConversionCallRemove, vec!["(value)"])]
#[case("items.append(item)", MutationOperator::CollectionAppendInsert, vec!["items.insert(0, item)"])]
#[case("items.get(key)", MutationOperator::StructureMappingGetSubscript, vec!["items[key]"])]
#[case("items[key]", MutationOperator::StructureMappingGetSubscript, vec!["items.get(key)"])]
#[case("items[1]", MutationOperator::StructureIndexNeighbor, vec!["2", "0"])]
#[case("items[1:2]", MutationOperator::StructureSliceNeighbor, vec!["2", "0", "3", "1"])]
#[case("items[:2]", MutationOperator::StructureSliceBoundMove, vec!["2:"])]
#[case("[item]", MutationOperator::CollectionListTuple, vec!["(item,)"])]
#[case("(item,)", MutationOperator::CollectionListTuple, vec!["[item,]"])]
#[case("raise ValueError()", MutationOperator::ExceptionTypePair, vec!["TypeError"])]
#[case("try:\n pass\nexcept ValueError:\n pass", MutationOperator::ExceptionTypePair, vec!["TypeError"])]
#[case("try:\n pass\nexcept:\n pass", MutationOperator::ExceptionBareToException, vec!["except Exception"])]
#[case("def function() -> int:\n return item", MutationOperator::FunctionBodyReturnConstant, vec!["return 0", "return 1"])]
#[case("value += 1", MutationOperator::AugmentedToAssignment, vec!["="])]
#[case("import builtins\ntry:\n action()\nexcept (builtins.ValueError, TypeError):\n pass", MutationOperator::ExceptionTupleAddPair, vec![])]
#[case("from decimal import Decimal\nDecimal(item for item in items)", MutationOperator::StdlibConversionCallRemove, vec![])]
#[case("int((captured := value))", MutationOperator::ConversionCallRemove, vec![])]
#[case("if first and (captured := second):\n pass", MutationOperator::ConditionClauseDelete, vec![])]
#[case("if captured := condition:\n pass", MutationOperator::ConditionConstant, vec![])]
#[case("while captured := condition:\n pass", MutationOperator::WhileConditionFalse, vec![])]
fn active_producers_keep_their_mutation_contracts(
    #[case] source: &str,
    #[case] operator: MutationOperator,
    #[case]
    #[by_ref]
    expected: &[&str],
) {
    let operators = serde_json::from_value(serde_json::json!([operator.as_str()])).unwrap();
    let prefix = run_ast(source, &operators, &PayloadBudget::default(), &|| false).unwrap();
    assert_eq!(
        prefix
            .candidates
            .iter()
            .map(|candidate| candidate.replacement.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    assert!(!prefix.overflowed);
}

#[test]
fn cancelled_ast_collection_rejects_selection_before_producing_candidates() {
    let operators = MutationOperatorSelection::default();
    assert!(matches!(
        run_ast(
            "items.copy()",
            &operators,
            &PayloadBudget::default(),
            &|| true
        ),
        Err(AnalysisError::Cancelled)
    ));
}

#[rstest::rstest]
#[case(
    "for value in values:\n callback = lambda value=value: value",
    MutationOperator::LambdaCaptureLate
)]
#[case("1", MutationOperator::IntegerLiteralNeighbor)]
#[case("if condition:\n pass", MutationOperator::ConditionConstant)]
#[case("items.append(item)", MutationOperator::CollectionAppendInsert)]
#[case(
    "def function() -> int:\n return item",
    MutationOperator::FunctionBodyReturnConstant
)]
#[case(
    "from enum import Enum\nclass Choice(Enum):\n FIRST=1\n SECOND=2\nChoice.FIRST",
    MutationOperator::EnumMemberReplace
)]
#[case(
    "from dataclasses import dataclass, field\n@dataclass\nclass C:\n values: list = field(default_factory=list)",
    MutationOperator::DataclassFactoryShare
)]
#[case("value = 'first' 'second'", MutationOperator::StringSegmentEmpty)]
#[case("if first and second:\n pass", MutationOperator::ConditionClauseDelete)]
#[case("any(items)", MutationOperator::CollectionAnyAll)]
#[case("items.copy()", MutationOperator::MethodCallRemove)]
#[case(
    "from decimal import Decimal\nDecimal(value)",
    MutationOperator::StdlibConversionCallRemove
)]
#[case("int(value)", MutationOperator::ConversionCallRemove)]
#[case("items.get(key)", MutationOperator::StructureMappingGetSubscript)]
#[case("items[key]", MutationOperator::StructureMappingGetSubscript)]
#[case("items[1]", MutationOperator::StructureIndexNeighbor)]
#[case("items[1:2]", MutationOperator::StructureSliceNeighbor)]
#[case("[item]", MutationOperator::CollectionListTuple)]
#[case("(item,)", MutationOperator::CollectionListTuple)]
#[case("raise ValueError()", MutationOperator::ExceptionTypePair)]
#[case(
    "try:\n pass\nexcept ValueError:\n pass",
    MutationOperator::ExceptionTypePair
)]
#[case(
    "try:\n pass\nexcept:\n pass",
    MutationOperator::ExceptionBareToException
)]
#[case("value += 1", MutationOperator::AugmentedToAssignment)]
#[case(
    "if flag:\n pass\nelif first and second:\n pass",
    MutationOperator::ConditionClauseDelete
)]
#[case(
    "if True:\n pass\nelif flag:\n pass",
    MutationOperator::ConditionConstant
)]
#[case("while flag:\n pass", MutationOperator::WhileConditionFalse)]
#[case("for item in items:\n action()", MutationOperator::LoopBreakInsert)]
#[case("def f():\n action()", MutationOperator::FunctionBodyErase)]
#[case("def f():\n yield item", MutationOperator::GeneratorReturnInsert)]
#[case(
    "class C:\n def __next__(self):\n  return item",
    MutationOperator::IteratorStopInsert
)]
#[case(
    "class C:\n async def __anext__(self):\n  return item",
    MutationOperator::AsyncIteratorStopInsert
)]
#[case(
    "try:\n pass\nexcept Exception:\n pass",
    MutationOperator::ExceptionExceptionToBare
)]
#[case(
    "try:\n pass\nexcept BaseException:\n pass",
    MutationOperator::ExceptionBaseBoundary
)]
#[case(
    "try:\n pass\nexcept (ValueError,):\n pass",
    MutationOperator::ExceptionTupleAddPair
)]
#[case(
    "try:\n pass\nexcept (ValueError, TypeError):\n pass",
    MutationOperator::ExceptionTupleRemoveMember
)]
#[case("action()", MutationOperator::StatementDelete)]
#[case("items[1:2]", MutationOperator::StructureSliceBoundOmit)]
#[case("items[:2]", MutationOperator::StructureSliceBoundMove)]
fn producer_payload_failure_stops_after_its_first_generated_candidate(
    #[case] source: &str,
    #[case] operator: MutationOperator,
) {
    let operators = serde_json::from_value(serde_json::json!([operator.as_str()])).unwrap();
    let budget = PayloadBudget::new(0);
    assert!(matches!(
        run_ast(source, &operators, &budget, &|| false),
        Err(AnalysisError::PayloadLimit { limit: 0 })
    ));
    assert_eq!(budget.snapshot().generated, 1);
    assert_eq!(budget.snapshot().used, 0);
}

#[rstest::rstest]
#[case("items.copy()", Attempt::MethodRemove, State::Active, 1)]
#[case("items.copy()", Attempt::MethodRemove, State::Target, 0)]
#[case("items.copy()", Attempt::MethodRemove, State::Pattern, 0)]
#[case(
    "from decimal import Decimal\nDecimal(value)",
    Attempt::StdlibConversion,
    State::Active,
    1
)]
#[case(
    "from decimal import Decimal\nDecimal(value)",
    Attempt::StdlibConversion,
    State::Target,
    0
)]
#[case(
    "from decimal import Decimal\nDecimal(value)",
    Attempt::StdlibConversion,
    State::Pattern,
    0
)]
#[case("int(value)", Attempt::Conversion, State::Active, 1)]
#[case("int(value)", Attempt::Conversion, State::Target, 0)]
#[case("int(value)", Attempt::Conversion, State::Pattern, 0)]
#[case("1", Attempt::Integer, State::Active, 2)]
#[case("1", Attempt::Integer, State::Pattern, 0)]
#[case("1", Attempt::Integer, State::SuppressedInteger, 0)]
#[case("[first, second]", Attempt::List, State::Active, 1)]
#[case("[first, second]", Attempt::List, State::ExceptionType, 0)]
#[case("(first, second)", Attempt::Tuple, State::Active, 1)]
#[case("(first, second)", Attempt::Tuple, State::ExceptionType, 0)]
#[case("list(items)", Attempt::Builtin, State::Active, 1)]
#[case("list(items)", Attempt::Builtin, State::ExceptionType, 0)]
fn producer_context_guards_prevent_generation_with_a_healthy_payload_budget(
    #[case] source: &str,
    #[case] attempt: Attempt,
    #[case] state: State,
    #[case] count: usize,
) {
    let (retained, error_unchanged, polls, generated, used, cancelled_observed) =
        observe_attempt(source, attempt, state);
    assert_eq!(retained, count);
    assert!(error_unchanged);
    assert!(!cancelled_observed);
    assert_eq!(generated, count);
    assert_eq!(polls == 0, count == 0);
    assert_eq!(used == 0, count == 0);
}

fn run_public(
    source: &str,
    operators: &MutationOperatorSelection,
    payload: &PayloadBudget,
    cancelled: &dyn Fn() -> bool,
) -> Result<AnalyzerOutput, AnalysisError> {
    let request = AnalyzeRequest {
        payload: payload.clone(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    analyze_source_cancellable(&request, source, cancelled)
}

#[rstest::rstest]
fn analyzer_cancellation_releases_payload_across_index_and_class_emission_phases(
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70,
        71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93,
        94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112,
        113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127
    )]
    checkpoint: usize,
) {
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Record:\n values: list[int] = field(default_factory=list)\n";
    let operators = serde_json::from_value(serde_json::json!(["dataclass_factory_share"])).unwrap();
    let budget = PayloadBudget::default();
    let total = Cell::new(0);
    let complete = run_public(source, &operators, &budget, &|| {
        total.set(total.get() + 1);
        false
    })
    .unwrap();
    assert_eq!(complete.candidates.len(), 1);
    drop(complete);
    let polls = Cell::new(0);
    let result = run_public(source, &operators, &budget, &|| {
        let current = polls.get();
        polls.set(current + 1);
        current >= checkpoint
    });
    assert_eq!(
        result.as_ref().err().copied(),
        (checkpoint < total.get()).then_some(AnalysisError::Cancelled)
    );
    drop(result);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn analyzer_final_cancellation_boundary_releases_already_generated_payload() {
    let source = "from dataclasses import dataclass, field\n@dataclass\nclass Record:\n values: list[int] = field(default_factory=list)\n";
    let operators = serde_json::from_value(serde_json::json!(["dataclass_factory_share"])).unwrap();
    let budget = PayloadBudget::default();
    let total = Cell::new(0_usize);
    let complete = run_public(source, &operators, &budget, &|| {
        total.set(total.get() + 1);
        false
    })
    .unwrap();
    assert_eq!(complete.candidates.len(), 1);
    drop(complete);
    let checkpoint = total.get().checked_sub(1).unwrap();
    let polls = Cell::new(0);
    let result = run_public(source, &operators, &budget, &|| {
        let current = polls.get();
        polls.set(current + 1);
        current >= checkpoint
    });
    assert!(matches!(result, Err(AnalysisError::Cancelled)));
    assert_eq!(polls.get(), total.get());
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn condition_clause_deletion_generates_only_one_sentinel_beyond_the_public_limit() {
    let condition = (0..103)
        .map(|index| format!("flag{index}"))
        .collect::<Vec<_>>()
        .join(" and ");
    let source = format!("if {condition}:\n pass\n");
    let operators = serde_json::from_value(serde_json::json!(["condition_clause_delete"])).unwrap();
    let budget = PayloadBudget::default();
    let output = run_public(&source, &operators, &budget, &|| false).unwrap();
    assert_eq!(output.candidates.len(), 100);
    assert!(output.truncated);
    assert_eq!(budget.snapshot().generated, 101);
    drop(output);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn augmented_assignment_observes_cancellation_while_scanning_its_operator() {
    assert_eq!(
        observe_attempt("value += 1", Attempt::Augmented, State::Cancelled),
        (0, true, 1, 0, 0, true)
    );
}
