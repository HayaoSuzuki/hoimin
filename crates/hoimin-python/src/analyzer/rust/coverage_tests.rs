use super::*;

#[rstest::rstest]
#[case::empty_marker(|| unique_marker_range("value", "")
            .unwrap_err()
            .contains("must not be empty"))]
#[case::duplicate_marker(|| unique_marker_range("value value", "value")
            .unwrap_err()
            .contains("2 times"))]
#[case::missing_annotation(|| annotation_site_test_snapshot("value = 1\n", "value")
            .unwrap_err()
            .contains("0 annotation sites"))]
#[case::class_annotation(|| annotation_site_test_snapshot("class Box:\n    value: int\n", "int")
            .is_ok_and(|snapshot| snapshot.scope == "class"))]
#[case::duplicate_tracked_name(|| name_resolution_test_snapshot("list + list\n", "list + list", "list")
            .unwrap_err()
            .contains("2 times inside marker"))]
#[case::assignment_is_not_load(|| name_resolution_test_snapshot("list = 1\n", "list", "list")
            .unwrap_err()
            .contains("tracked load name"))]
#[case::unparsed_name(|| name_resolution_test_snapshot("list(\n", "list", "list")
            .unwrap_err()
            .contains("did not parse"))]
#[case::missing_try(|| binding_flow_try_exit_snapshot_inner("value = 1\n", "value", None)
            .unwrap_err()
            .contains("0 try exits"))]
#[case::unparsed_try(|| binding_flow_try_exit_snapshot_inner("value = (\n", "value", None)
            .unwrap_err()
            .contains("did not parse"))]
#[case::unparsed_marker(|| binding_flow_marker_snapshot("value = (\n", "value")
            .unwrap_err()
            .contains("did not parse"))]
#[case::unknown_handler_exit(|| binding_flow_handler_exit_snapshot("value = 1\n", "value", "unknown")
            .unwrap_err()
            .contains("unsupported handler exit category"))]
#[case::unparsed_handler(|| binding_flow_handler_exit_snapshot("value = (\n", "value", "fallthrough")
            .unwrap_err()
            .contains("did not parse"))]
#[case::unreached_handler(|| binding_flow_handler_exit_snapshot(
            "raise RuntimeError\nvalue = 1\n",
            "value",
            "fallthrough"
        )
        .unwrap_err()
        .contains("was not reached"))]
#[case::missing_handler(|| binding_flow_handler_exit_snapshot("value = 1\n", "value", "fallthrough")
            .unwrap_err()
            .contains("0 handler exits"))]
#[case::missing_break_exit(|| binding_flow_handler_exit_snapshot(
            "try:\n    pass\nexcept Exception:\n    marker = 1\n",
            "marker",
            "break"
        )
        .unwrap_err()
        .contains("produced 0 exits"))]
#[case::unparsed_loop(|| binding_flow_loop_head_snapshot_at_marker("value = (\n", "value", None)
            .unwrap_err()
            .contains("did not parse"))]
#[case::missing_loop(|| binding_flow_loop_head_snapshot_at_marker("raise RuntimeError\nvalue = 1\n", "value", None)
            .unwrap_err()
            .contains("0 loop heads"))]
fn snapshot_adapters_reject_ambiguous_missing_and_invalid_source_markers(
    #[case] inspect: fn() -> bool,
) {
    let actual = inspect();

    assert!(actual);
}

#[test]
fn retained_candidate_order_uses_emission_sequence_for_ties() {
    let budget = payload::PayloadBudget::default();
    let candidate = || AnalyzerCandidate {
        path: "sample.py".into(),
        line: 1,
        column: 1,
        span: hoimin_core::ByteSpan {
            start: 0,
            length: 1,
        },
        operator: hoimin_core::MutationOperator::BooleanLiteral,
        original: "True".into(),
        replacement: "False".into(),
        symbol: None,
    };
    let first = RetainedCandidate {
        candidate: budget.retain(candidate()).unwrap(),
        emission_sequence: 0,
    };
    let same = RetainedCandidate {
        candidate: budget.retain(candidate()).unwrap(),
        emission_sequence: 0,
    };
    let later = RetainedCandidate {
        candidate: budget.retain(candidate()).unwrap(),
        emission_sequence: 1,
    };

    let actual = first.partial_cmp(&later);

    assert!(first == same);
    assert!(first != later);
    assert_eq!(actual, Some(std::cmp::Ordering::Less));
}

#[test]
fn candidate_prefix_preserves_first_payload_failure_for_subsequent_pushes() {
    let mut prefix = CandidatePrefix::new(2, payload::PayloadBudget::new(1));
    let candidate = || AnalyzerCandidate {
        path: "sample.py".into(),
        span: hoimin_core::ByteSpan {
            start: 0,
            length: 4,
        },
        operator: hoimin_core::MutationOperator::BooleanLiteral,
        original: "True".into(),
        replacement: "False".into(),
        line: 1,
        column: 1,
        symbol: None,
    };
    let error = prefix.push(candidate()).unwrap_err();

    let actual = prefix.push(candidate());
    let actual_2 = prefix.finish().is_err();

    assert_eq!(actual, Err(error));
    assert!(actual_2);
}

#[rstest::rstest]
fn known_imports_transfer_nested_assignment_targets_annotations_and_augments(
    #[values("a", "b", "rest", "attribute")] name: &str,
) {
    let parsed = parse_module("from typing import TypeVar\n[a, (b, *rest)] = TypeVar('T')\nattribute.value = TypeVar('U')\na: object = 1\nb += 1\ndel rest\n").unwrap();
    let mut imports = KnownImports::default();
    imports.transfer_statement(&parsed.syntax().body[0]);
    imports.transfer_statement(&parsed.syntax().body[1]);
    let before = imports.bindings.get(name).cloned();
    imports.transfer_statement(&parsed.syntax().body[2]);
    imports.transfer_statement(&parsed.syntax().body[3]);
    imports.transfer_statement(&parsed.syntax().body[4]);
    imports.transfer_statement(&parsed.syntax().body[5]);
    let after = imports.bindings.get(name);

    assert_eq!(
        before,
        (name != "attribute").then_some(KnownBinding::TypeVar)
    );
    assert_eq!(after, None);
}

#[rstest::rstest]
fn function_local_collection_tracks_binding_headers_but_not_deferred_bodies(
    #[values(
        "package",
        "decorator",
        "nested",
        "annotation",
        "default",
        "result",
        "class_decorator",
        "Nested",
        "a",
        "b",
        "rest",
        "Alias",
        "item",
        "mapping_rest",
        "head",
        "tail"
    )]
    name: &str,
) {
    let parsed = parse_module("import package.module\n@(decorator := identity)\ndef nested[T](arg: (annotation := int) = (default := 1)) -> (result := int):\n    deferred = 1\n@(class_decorator := identity)\nclass Nested(base):\n    class_body = 1\n[a, (b, *rest)] = values\na += 1\ndel b\ntype Alias = int\nmatch values:\n    case {'key': item, **mapping_rest}: pass\n    case [head, *tail]: pass\n").unwrap();
    let mut collector = FunctionLocalCollector::default();
    collector.visit_body(&parsed.syntax().body);
    let contains_name = collector.locals.contains(name);

    let actual = !collector.locals.contains("deferred");
    let actual_2 = !collector.locals.contains("class_body");

    assert!(contains_name, "missing {name}");
    assert!(actual);
    assert!(actual_2);
}

#[test]
fn stale_project_exception_index_is_rejected_before_analysis() {
    let path = Utf8Path::new("sample.py");
    let index = exception_hierarchy::ExceptionIndex::from_sources(
        &[(path.to_owned(), "class Root(Exception): pass\n".into())],
        &[],
        &|| false,
    )
    .unwrap();
    let operators = MutationOperatorSelection::default();
    let request = AnalyzeRequest {
        payload: payload::PayloadBudget::default(),
        path,
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };

    let actual = matches!(
        analyze_source_with_exceptions(&request, "changed = 1\n", &|| false, Some(&index)),
        Err(AnalysisError::HierarchySourceChanged)
    );

    assert!(actual);
}

fn analysis_checkpoint_observation(
    select: impl FnOnce(usize) -> usize,
) -> (usize, usize, usize, Result<usize, AnalysisError>, usize) {
    let source = "from dataclasses import dataclass, field\nfrom enum import Enum\nfrom typing import Optional, Sequence\nimport operator\nimport functools\n@dataclass\nclass Record:\n    values: list[int] = field(default_factory=list)\nclass Choice(Enum):\n    FIRST = 1\n    SECOND = 2\nclass Root(Exception): pass\nclass Child(Root): pass\ndef selected(value: Optional[int] = None):\n    values = [True, False]\n    values.append(1)\n    result = operator.add(1, 2)\n    for item in values:\n        if item and result or value:\n            break\n    try:\n        return sorted(values, reverse=True)\n    except Child:\n        raise Root()\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.include(operator);
        }
    }
    let payload = payload::PayloadBudget::default();
    let request = AnalyzeRequest {
        payload: payload.clone(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let total = Cell::new(0);
    let baseline = analyze_source_cancellable(&request, source, || {
        total.set(total.get() + 1);
        false
    })
    .unwrap();
    let baseline_count = baseline.candidates.len();
    drop(baseline);
    let checkpoint = select(total.get());
    let polls = Cell::new(0);
    let result = analyze_source_cancellable(&request, source, || {
        let current = polls.get();
        polls.set(current + 1);
        current >= checkpoint
    })
    .map(|output| output.candidates.len());
    (
        checkpoint,
        total.get(),
        baseline_count,
        result,
        payload.snapshot().used,
    )
}

#[rstest::rstest]
fn analysis_cancellation_preserves_start_final_and_completion_boundaries(
    #[values(0, 1, 2)] boundary: usize,
) {
    let (checkpoint, total, baseline_count, result, used) =
        analysis_checkpoint_observation(|total| [0, total - 1, total][boundary]);

    let actual = result.is_err();
    let actual_2 = result
        .as_ref()
        .err()
        .is_none_or(|error| *error == AnalysisError::Cancelled);
    let actual_3 = result
        .as_ref()
        .ok()
        .is_none_or(|count| *count == baseline_count);
    let actual_4 = result.is_err();

    assert!(baseline_count > 0);
    assert_eq!(actual, checkpoint < total);
    assert!(actual_2);
    assert!(actual_3);
    assert_eq!(used, 0);
    assert_eq!(actual_4, boundary < 2);
}

proptest::proptest! {
    #[test]
    fn cancellation_at_each_analysis_checkpoint_releases_retained_payload(checkpoint in 0usize..4096) {
        let (checkpoint, total, baseline_count, result, used) = analysis_checkpoint_observation(|_| checkpoint);

        let actual = result.is_err();
        let actual_2 = result.as_ref().err().is_none_or(|error| *error == AnalysisError::Cancelled);
        let actual_3 = result.as_ref().ok().is_none_or(|count| *count == baseline_count);

    assert!(baseline_count > 0);
    assert_eq!(actual, checkpoint < total);
    assert!(actual_2);
    assert!(actual_3);
    assert_eq!(used, 0);
    }
}

fn payload_limit_observation(limit: usize) -> (Result<usize, AnalysisError>, usize) {
    let source = "from typing import Optional\ndef selected(value: Optional[int]):\n    values = [True, False]\n    values.append(1)\n    values.extend([2])\n    result = {'key': 1}.get('key', 0)\n    return (values, result)\n";
    let operators = MutationOperatorSelection::default();
    let payload = payload::PayloadBudget::new(limit);
    let request = AnalyzeRequest {
        payload: payload.clone(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let result = analyze_source_cancellable(&request, source, || false)
        .map(|output| output.candidates.len());
    (result, payload.snapshot().used)
}

#[rstest::rstest]
fn payload_exhaustion_at_different_producer_boundaries_releases_all_charges(
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70,
        71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93,
        94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112,
        113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127
    )]
    step: usize,
) {
    let limit = step * 16;

    let (result, used) = payload_limit_observation(limit);

    let actual = result
        .as_ref()
        .err()
        .is_none_or(|error| *error == AnalysisError::PayloadLimit { limit });
    let actual_2 = result.as_ref().ok().is_none_or(|count| *count > 0);

    assert!(actual);
    assert!(actual_2);
    assert_eq!(used, 0, "leaked payload at limit {limit}");
}

#[rstest::rstest]
fn large_payload_limits_release_all_charges(#[values(4096, 8192, 16384, 32768)] limit: usize) {
    let (result, used) = payload_limit_observation(limit);

    let actual = result
        .as_ref()
        .err()
        .is_none_or(|error| *error == AnalysisError::PayloadLimit { limit });
    let actual_2 = result.as_ref().ok().is_none_or(|count| *count > 0);

    assert!(actual);
    assert!(actual_2);
    assert_eq!(used, 0, "leaked payload at limit {limit}");
}

#[test]
fn generous_payload_limit_preserves_nonempty_analysis() {
    let (result, used) = payload_limit_observation(32768);

    assert!(result.unwrap() > 0);
    assert_eq!(used, 0);
}

#[test]
fn zero_payload_limit_rejects_analysis() {
    let (result, used) = payload_limit_observation(0);

    assert_eq!(result, Err(AnalysisError::PayloadLimit { limit: 0 }));
    assert_eq!(used, 0);
}

#[test]
#[should_panic(expected = "binding-flow loop fixture has no while statement")]
fn loop_snapshot_adapter_rejects_fixture_without_loop() {
    binding_flow_loop_head_snapshot("raise RuntimeError\nwhile flag:\n    pass\n", true);
}

#[test]
fn annotation_visitor_statement_entry_transfers_imports_and_stops_at_returns() {
    let parsed =
        parse_module("from typing import Optional\nvalue: Optional[int]\nreturn\n").unwrap();
    let mut collector = AnnotationCollector::empty();
    collector.visit_stmt(&parsed.syntax().body[0]);
    collector.visit_stmt(&parsed.syntax().body[1]);
    let optional_binding_before_return = collector.imports.bindings.get("Optional").cloned();
    collector.visit_stmt(&parsed.syntax().body[2]);

    let actual = optional_binding_before_return.as_ref();

    assert_eq!(
        actual,
        Some(&KnownBinding::Direct("typing.Optional".into()))
    );
}

#[test]
fn named_binding_statement_visitor_invalidates_walrus_inside_statement() {
    let parsed = parse_module("result = (List := replacement)\n").unwrap();
    let mut imports = KnownImports::default();
    imports
        .bindings
        .insert("List".into(), KnownBinding::Direct("typing.List".into()));
    let mut invalidator = NamedBindingInvalidator {
        imports: &mut imports,
    };
    invalidator.visit_stmt(&parsed.syntax().body[0]);

    let actual = !imports.bindings.contains_key("List");

    assert!(actual);
}

#[test]
fn focused_profile_retains_unrepresentable_external_candidate_span() {
    let parsed = parse_module("value = True\n").unwrap();
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), "value = True\n");
    let candidate = AnalyzerCandidate {
        path: "sample.py".into(),
        span: ByteSpan {
            start: u64::MAX,
            length: 1,
        },
        original: "True".into(),
        replacement: "False".into(),
        operator: MutationOperator::BooleanLiteral,
        line: 1,
        column: 1,
        symbol: None,
    };

    let actual = retained_by_profile(&candidate, MutationProfile::Focused, &facts);

    assert!(actual);
}

#[test]
fn tuple_replacement_sensitivity_probe_detects_unselected_eager_work() {
    let operators = serde_json::from_str("[\"boolean_literal\"]").unwrap();
    let request = AnalyzeRequest {
        payload: payload::PayloadBudget::default(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    COLLECTION_REPLACEMENT_BUILDS.set(0);
    let normal = analyze_source(&request, "value = (1, 2)\n");
    let normal_builds = COLLECTION_REPLACEMENT_BUILDS.get();
    BROKEN_EAGER_REPLACEMENT.set(true);
    let broken = analyze_source(&request, "value = (1, 2)\n");
    BROKEN_EAGER_REPLACEMENT.set(false);

    let actual = COLLECTION_REPLACEMENT_BUILDS.get() > normal_builds;

    assert_eq!(normal.candidates, broken.candidates);
    assert_eq!(normal_builds, 0);
    assert!(actual);
}

#[rstest::rstest]
#[case("match value:\n    case [*List]: pass\n", false)]
#[case("match value:\n    case [*_]: pass\n", true)]
fn star_pattern_flow_preserves_only_uncaptured_imports(
    #[case] source: &str,
    #[case] preserved: bool,
) {
    let parsed = parse_module(source).unwrap();
    let statement = parsed.syntax().body[0].as_match_stmt().unwrap();
    let mut imports = KnownImports::default();
    imports
        .bindings
        .insert("List".into(), KnownBinding::Direct("typing.List".into()));
    let result = pattern_binding_flow(&imports, &statement.cases[0].pattern);

    let actual = result.matched.unwrap().bindings.contains_key("List");
    let actual_2 = result.failed.unwrap();

    assert_eq!(actual, preserved);
    assert_eq!(actual_2, imports);
}

#[rstest::rstest]
#[case("obj.attribute: int = value\n")]
#[case("obj.attribute += value\n")]
#[case("del obj.attribute\n")]
fn implicit_exception_projection_includes_partial_attribute_stores(#[case] source: &str) {
    let parsed = parse_module(source).unwrap();
    let mut imports = KnownImports::default();
    imports
        .bindings
        .insert("List".into(), KnownBinding::Direct("typing.List".into()));
    let incoming =
        ImplicitExceptionEntry::for_statement(&imports, &parsed.syntax().body[0]).unwrap();

    assert_eq!(incoming, imports);
}

#[rstest::rstest]
#[case::module_alias("alias", true)]
#[case::missing_name("missing", false)]
fn module_identity_queries_reject_class_namespace_and_unrecorded_expression(
    #[case] name: &str,
    #[case] expected: bool,
) {
    let parsed =
        parse_module("import dependency as alias\nalias\nclass Box:\n    alias\n").unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["alias".to_owned()]),
    );
    let module_load = parsed.syntax().body[1].as_expr_stmt().unwrap();

    let resolved = index.resolves_unique_module(&module_load.value, name);

    assert_eq!(resolved, expected);
}

#[test]
fn module_identity_query_rejects_the_class_namespace() {
    let parsed =
        parse_module("import dependency as alias\nalias\nclass Box:\n    alias\n").unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["alias".to_owned()]),
    );
    let class = parsed.syntax().body[2].as_class_def_stmt().unwrap();
    let class_load = class.body[0].as_expr_stmt().unwrap();

    let resolved = index.resolves_unique_module(&class_load.value, "alias");

    assert!(!resolved);
}

#[test]
fn annotation_import_query_rejects_an_unrecorded_offset() {
    let parsed = parse_module("import dependency as alias\nalias\n").unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["alias".to_owned()]),
    );

    let stable = index.annotation_import_stable(usize::MAX, "alias", true);

    assert!(!stable);
}

#[test]
fn module_identity_query_rejects_an_expression_outside_the_indexed_module() {
    let parsed = parse_module("import dependency as alias\nalias\n").unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["alias".to_owned()]),
    );
    let another = parse_module("alias = 1\n").unwrap();
    let assignment = another.syntax().body[0].as_assign_stmt().unwrap();

    let resolved = index.resolves_unique_module(&assignment.targets[0], "alias");

    assert!(!resolved);
}

#[rstest::rstest]
#[case::known_source_order(true, true)]
#[case::unknown_source_order(false, false)]
fn annotation_nonlocal_import_finds_owner_across_intermediate_function_scope(
    #[case] source_order_known: bool,
    #[case] expected: bool,
) {
    let source = "def outer():\n    from typing import Optional as O\n    def middle():\n        def inner():\n            nonlocal O\n            value: O[int]\n";
    let parsed = parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["O".to_owned()]),
    );
    let offset = source.find("O[int]").unwrap();

    let actual = index.annotation_import_stable(offset, "O", source_order_known);

    assert_eq!(actual, expected);
}

#[test]
fn name_resolution_snapshot_rejects_an_untracked_name() {
    let actual = name_resolution_test_snapshot("custom", "custom", "custom").unwrap_err();

    assert_eq!(
        actual,
        "infrastructure-error: name is not tracked for resolution"
    );
}

#[test]
fn prefix_eviction_preserves_other_replacements_with_the_same_identity_key() {
    let mut prefix = CandidatePrefix::new(1, payload::PayloadBudget::default());
    let candidate = |start, replacement: &str| AnalyzerCandidate {
        path: "sample.py".into(),
        span: ByteSpan { start, length: 1 },
        original: "1".into(),
        replacement: replacement.into(),
        operator: MutationOperator::IntegerLiteralNeighbor,
        line: 1,
        column: 1,
        symbol: None,
    };
    prefix.push(candidate(10, "2")).unwrap();
    prefix.push(candidate(10, "3")).unwrap();
    prefix.push(candidate(0, "4")).unwrap();

    let actual = prefix
        .identities
        .get(&(10, MutationOperator::IntegerLiteralNeighbor))
        .unwrap()
        .contains_key("2");
    let actual_2 = !prefix
        .identities
        .get(&(10, MutationOperator::IntegerLiteralNeighbor))
        .unwrap()
        .contains_key("3");
    let actual_3 = prefix.entries.len();

    assert!(actual);
    assert!(actual_2);
    assert_eq!(actual_3, 2);
}

fn module_identity_for_last_root_load(source: &str) -> bool {
    struct LastRoot<'ast>(Option<&'ast Expr>);
    impl<'ast> Visitor<'ast> for LastRoot<'ast> {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(expression, Expr::Name(name) if name.ctx == ExprContext::Load && name.id.as_str() == "root")
            {
                self.0 = Some(expression);
            }
            visitor::walk_expr(self, expression);
        }
    }
    let parsed = parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["root".to_owned()]),
    );
    let mut root = LastRoot(None);
    root.visit_body(&parsed.syntax().body);
    index.resolves_unique_module(root.0.unwrap(), "root")
}

#[rstest::rstest]
#[case("import package as root\ndef function():\n return root.member", true)]
#[case("import package as root\nclass C:\n value = root.member", false)]
#[case(
    "import package as root\ndef function():\n try:\n  pass\n except Exception as root:\n  root.member",
    false
)]
#[case(
    "import package as root\ndef outer():\n root = other\n def inner():\n  return root.member",
    false
)]
#[case(
    "import package as root\ndef outer():\n root = other\n def inner():\n  nonlocal root\n  return root.member",
    false
)]
#[case(
    "import package as root\nclass C:\n def method(self):\n  return root.member",
    true
)]
fn module_identity_respects_temporary_lexical_and_class_namespaces(
    #[case] source: &str,
    #[case] expected: bool,
) {
    let actual = module_identity_for_last_root_load(source);

    assert_eq!(actual, expected);
}

#[test]
fn every_nested_name_resolution_scope_has_an_enclosing_scope() {
    let parsed = parse_module("import package as root\nclass C[T]:\n def method[U](self, arg: list[U]):\n  return [root.member for item in values]\n").unwrap();
    let index = NameResolutionIndex::from_module(parsed.syntax());

    let actual = index.scopes.len() > 4;
    let actual_2 = index
        .scopes
        .iter()
        .enumerate()
        .skip(1)
        .all(|(id, scope)| scope.parent.is_some_and(|parent| parent.0 < id));

    assert!(actual);
    assert!(actual_2);
}

#[rstest::rstest]
#[case(0..2, 1, None)]
#[case(0..0, 1, None)]
#[case(0..99, 1, None)]
#[case(0..4, 2, None)]
#[case(0..4, 1, Some("'é'"))]
fn candidate_admission_validates_byte_ends_source_bounds_and_selected_lines(
    #[case] range: Range<usize>,
    #[case] selected_line: u32,
    #[case] expected: Option<&str>,
) {
    let source = "'é'";
    let operators = serde_json::from_value(serde_json::json!(["string_literal_empty"])).unwrap();
    let lines = [LineRange {
        start: selected_line,
        end: selected_line,
    }];
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("sample.py"),
        lines: &lines,
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let selection = CandidateSelection::new(&request, &|| false).unwrap();

    let actual = make_candidate(
        &selection,
        &request,
        source,
        &LineIndex::new(source),
        range,
        "''".to_owned(),
        MutationOperator::StringLiteralEmpty,
        None,
    )
    .map(|candidate| candidate.original);

    assert_eq!(actual.as_deref(), expected);
}

#[rstest::rstest]
#[case("", "list[[int, str]]", Some("list[[int, str]] | None"))]
#[case("int = custom", "list[[int, str]]", None)]
#[case("", "dict[*(str, int)]", Some("dict[*(str, int)] | None"))]
#[case("int = custom", "dict[*(str, int)]", None)]
#[case("", "list[int | str]", Some("list[int | str] | None"))]
#[case("str = custom", "list[int | str]", None)]
fn nullable_add_checks_builtin_provenance_inside_nested_annotation_forms(
    #[case] prefix: &str,
    #[case] annotation: &str,
    #[case] expected: Option<&str>,
) {
    let source = format!("{prefix}\nvalue: {annotation}\n");
    let operators = serde_json::from_value(serde_json::json!(["type_nullable_add"])).unwrap();
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let output = analyze_source(&request, &source);

    let actual = output
        .candidates
        .iter()
        .map(|candidate| candidate.replacement.as_str())
        .collect::<Vec<_>>();

    assert_eq!(actual, expected.into_iter().collect::<Vec<_>>());
}

#[rstest::rstest]
#[case("[list() for [list] in rows]", NameResolution::Shadowed)]
#[case("[list() for (head, list) in rows]", NameResolution::Shadowed)]
#[case("[list() for [head, *list] in rows]", NameResolution::Shadowed)]
#[case(
    "[list() for first in rows for list in others if list]",
    NameResolution::Shadowed
)]
#[case("list = custom\ndel [list]\nlist()", NameResolution::Unknown)]
#[case(
    "def f():\n from elsewhere import *\n return list()",
    NameResolution::Unknown
)]
#[case("def f():\n nonlocal list\n return list()", NameResolution::Unknown)]
#[case(
    "for list in rows:\n def f():\n  class C:\n   value = list()",
    NameResolution::Unknown
)]
fn destructuring_targets_and_deletions_preserve_builtin_lookup_rules(
    #[case] source: &str,
    #[case] expected: NameResolution,
) {
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module(parsed.syntax());

    let actual = index.resolution(source.find("list()").unwrap(), "list");

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case("(first, second)", 0, false, Some("( second)"))]
#[case("(first, second)", 1, false, Some("(first )"))]
#[case("(first,)", 0, false, Some("()"))]
#[case("()", 0, false, None)]
#[case("(first, second)", 2, false, None)]
#[case("(first, second)", 0, true, None)]
fn tuple_member_removal_requires_a_present_element_and_complete_source(
    #[case] source: &str,
    #[case] index: usize,
    #[case] truncate_source: bool,
    #[case] expected: Option<&str>,
) {
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let tuple = parsed.syntax().body[0]
        .as_expr_stmt()
        .unwrap()
        .value
        .as_tuple_expr()
        .unwrap();
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    let available = &source[..source.len() * usize::from(!truncate_source)];

    let actual = tuple_remove_replacement(available, tuple, &facts, index);

    assert_eq!(actual.as_deref(), expected);
}

#[rstest::rstest]
#[case(vec![TokenKind::Lsqb], false, None)]
#[case(vec![TokenKind::Rsqb], false, None)]
#[case(vec![], true, None)]
#[case(vec![], false, Some("int"))]
fn optional_removal_requires_both_brackets_and_available_source(
    #[case]
    #[by_ref]
    omitted: &[TokenKind],
    #[case] truncate_source: bool,
    #[case] expected: Option<&str>,
) {
    let source = "Optional[int]";
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let optional = parsed.syntax().body[0]
        .as_expr_stmt()
        .unwrap()
        .value
        .as_subscript_expr()
        .unwrap();
    let tokens = ruff_python_ast::token::Tokens::new(
        parsed
            .tokens()
            .iter()
            .filter(|token| !omitted.contains(&token.kind()))
            .copied()
            .collect(),
    );
    let facts = AstFacts {
        tokens: Some(&tokens),
        source,
        ..AstFacts::default()
    };
    let available = &source[..source.len() * usize::from(!truncate_source)];

    let actual = nullable_optional_inner_source(optional, available, &facts);

    assert_eq!(actual.as_deref(), expected);
}

#[rstest::rstest]
#[case(false, Some("(list[\n int\n])"))]
#[case(true, None)]
fn nullable_union_removal_preserves_retained_multiline_grouping(
    #[case] truncate_source: bool,
    #[case] expected: Option<&str>,
) {
    let source = "(list[\n int\n] | None)";
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let binary = parsed.syntax().body[0]
        .as_expr_stmt()
        .unwrap()
        .value
        .as_bin_op_expr()
        .unwrap();
    let facts = AstFacts::from_module(parsed.syntax(), parsed.tokens(), source);
    let available = &source[..source.len() * usize::from(!truncate_source)];

    let actual = nullable_union_operand_source(&binary.left, binary, available, &facts);

    assert_eq!(actual.as_deref(), expected);
}

#[test]
fn wildcard_function_scope_cannot_prove_annotation_builtins() {
    let source = "def f():\n from elsewhere import *\n value: list\n";
    let parsed = ruff_python_parser::parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module(parsed.syntax());

    let actual = index.annotation_resolution(source.find("list").unwrap(), "list");

    assert_eq!(actual, NameResolution::Unknown);
}

#[test]
fn type_parameter_bounds_preserve_uncertain_module_loop_bindings() {
    let source = "for list in items:\n def f[T: list](): pass\n";
    let parsed = parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module(parsed.syntax());

    let actual = index.resolution(source.rfind("list").unwrap(), "list");

    assert_eq!(actual, NameResolution::Unknown);
}

#[test]
fn empty_resolution_index_cannot_prove_a_module_builtin() {
    let actual = NameResolutionIndex::default().resolve_module("list");

    assert_eq!(actual, NameResolution::Unknown);
}

#[rstest::rstest]
#[case(
    "class Outer:\n class Inner:\n  value = list()",
    NameResolution::DefinitelyBuiltin
)]
#[case(
    "def outer():\n list = custom\n class Inner:\n  value = list()",
    NameResolution::Shadowed
)]
#[case(
    "class Outer[list]:\n class Inner:\n  value = list()",
    NameResolution::Shadowed
)]
#[case(
    "class Outer[T]:\n class Inner:\n  value = list()",
    NameResolution::DefinitelyBuiltin
)]
fn nested_class_ancestors_resolve_module_function_and_type_parameter_bindings(
    #[case] source: &str,
    #[case] expected: NameResolution,
) {
    let parsed = parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module(parsed.syntax());

    let actual = index.resolution(source.rfind("list()").unwrap(), "list");

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case("del *list")]
#[case("del [head, (*list,)]")]
fn starred_delete_targets_are_parse_errors_not_admitted_semantic_errors(#[case] source: &str) {
    let error = parse_module(source).err().unwrap();

    let actual = error.error();

    assert_eq!(
        actual,
        ruff_python_parser::ParseErrorType::InvalidDeleteTarget
    );
}

#[rstest::rstest]
#[case("del list", NameResolution::Unknown)]
#[case("del [list]", NameResolution::Unknown)]
#[case("del (head, list)", NameResolution::Unknown)]
#[case("del obj.list", NameResolution::Shadowed)]
#[case("del obj[list]", NameResolution::Shadowed)]
fn deletion_preserves_lookup_effects_of_all_admitted_target_shapes(
    #[case] deletion: &str,
    #[case] expected: NameResolution,
) {
    let source = format!("list = custom\n{deletion}\nlist()\n");
    let parsed = parse_module(&source).unwrap();
    let index = NameResolutionIndex::from_module(parsed.syntax());

    let actual = index.resolution(source.rfind("list()").unwrap(), "list");

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case(
    "value: list[factory()]",
    MutationOperator::TypeNullableAdd,
    Some("list[factory()] | None")
)]
#[case(
    "flag = True\nvalue: int if flag else str",
    MutationOperator::TypeNullableAdd,
    None
)]
#[case(
    "from typing import AbstractSet\nvalue: AbstractSet[int]",
    MutationOperator::TypeSetAbstractSet,
    Some("set[int]")
)]
#[case("value: factory().List[int]", MutationOperator::TypeListSequence, None)]
#[case(
    "from typing import Sequence\ntype Alias[T: int] = list[int]",
    MutationOperator::TypeListSequence,
    Some("Sequence[int]")
)]
#[case(
    "from typing import Sequence\ntype Alias[list] = list[int]",
    MutationOperator::TypeListSequence,
    None
)]
fn annotation_rewrites_require_statically_resolved_supported_shapes(
    #[case] source: &str,
    #[case] operator: MutationOperator,
    #[case] expected: Option<&str>,
) {
    let operators = serde_json::from_value(serde_json::json!([operator.as_str()])).unwrap();
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("sample.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    let output = analyze_source(&request, source);

    let actual = output
        .candidates
        .iter()
        .map(|candidate| candidate.replacement.as_str())
        .collect::<Vec<_>>();

    assert_eq!(output.diagnostics, Vec::new());
    assert_eq!(actual, expected.into_iter().collect::<Vec<_>>());
}

#[test]
fn nonlocal_import_write_invalidates_the_owner_across_an_intermediate_scope() {
    let source = "def outer():\n from typing import Optional as O\n def middle():\n  def inner():\n   nonlocal O\n   O = custom\n def target(value: O[int]): pass\n";
    let parsed = parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["O".to_owned()]),
    );
    let offset = source.find("O[int]").unwrap();

    let actual = index.imported_names.contains("O");
    let actual_2 = index.occurrences.contains_key(&offset);
    let actual_3 = !index.annotation_import_stable(offset, "O", true);

    assert!(actual);
    assert!(actual_2);
    assert!(actual_3);
}

#[test]
fn deleting_a_function_local_does_not_restore_builtin_provenance() {
    let source = "def f():\n del list\n return list()\n";
    let parsed = parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module(parsed.syntax());

    let actual = index.resolution(source.rfind("list()").unwrap(), "list");

    assert_eq!(actual, NameResolution::Shadowed);
}

#[test]
fn handler_raise_projection_ignores_other_handlers_and_return_bindings() {
    let source = "def f():\n try:\n  action()\n except ValueError:\n  from typing import Sequence as ignored\n except TypeError:\n  if flag:\n   from typing import Sequence as returning\n   return\n  from typing import Iterable as raising\n  raise Error\n";
    let parsed = parse_module(source).unwrap();
    let marker = unique_marker_range(source, "raising").unwrap();
    let line_start = source[..marker.start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let mut collector = AnnotationCollector::empty();
    collector.record_annotations = false;
    collector.handler_exit_projection = Some(BindingFlowHandlerExitProjection {
        source: source.to_owned(),
        marker,
        line_start,
        category: ExitCategory::Raise,
        matching_handlers: 0,
        matched_handler: None,
        states: Vec::new(),
    });
    let _ = collector.visit_suite_flow(&parsed.syntax().body);
    let projection = collector.handler_exit_projection.unwrap();

    let actual = normalize_binding_flow_states(&projection.states);

    assert_eq!(projection.matching_handlers, 1);
    assert_eq!(actual, [vec!["direct:raising=typing.Iterable".to_owned()]]);
}
