use super::*;

#[test]
fn snapshot_adapters_reject_ambiguous_missing_and_invalid_source_markers() {
    assert!(
        unique_marker_range("value", "")
            .unwrap_err()
            .contains("must not be empty")
    );
    assert!(
        unique_marker_range("value value", "value")
            .unwrap_err()
            .contains("2 times")
    );
    assert!(
        annotation_site_test_snapshot("value = 1\n", "value")
            .unwrap_err()
            .contains("0 annotation sites")
    );
    assert!(
        annotation_site_test_snapshot("class Box:\n    value: int\n", "int")
            .is_ok_and(|snapshot| snapshot.scope == "class")
    );
    assert!(
        name_resolution_test_snapshot("list + list\n", "list + list", "list")
            .unwrap_err()
            .contains("2 times inside marker")
    );
    assert!(
        name_resolution_test_snapshot("list = 1\n", "list", "list")
            .unwrap_err()
            .contains("tracked load name")
    );
    assert!(
        name_resolution_test_snapshot("list(\n", "list", "list")
            .unwrap_err()
            .contains("did not parse")
    );
    assert!(
        binding_flow_try_exit_snapshot_inner("value = 1\n", "value", None)
            .unwrap_err()
            .contains("0 try exits")
    );
    assert!(
        binding_flow_try_exit_snapshot_inner("value = (\n", "value", None)
            .unwrap_err()
            .contains("did not parse")
    );
    assert!(
        binding_flow_marker_snapshot("value = (\n", "value")
            .unwrap_err()
            .contains("did not parse")
    );
    assert!(
        binding_flow_handler_exit_snapshot("value = 1\n", "value", "unknown")
            .unwrap_err()
            .contains("unsupported handler exit category")
    );
    assert!(
        binding_flow_handler_exit_snapshot("value = (\n", "value", "fallthrough")
            .unwrap_err()
            .contains("did not parse")
    );
    assert!(
        binding_flow_handler_exit_snapshot(
            "raise RuntimeError\nvalue = 1\n",
            "value",
            "fallthrough"
        )
        .unwrap_err()
        .contains("was not reached")
    );
    assert!(
        binding_flow_handler_exit_snapshot("value = 1\n", "value", "fallthrough")
            .unwrap_err()
            .contains("0 handler exits")
    );
    assert!(
        binding_flow_handler_exit_snapshot(
            "try:\n    pass\nexcept Exception:\n    marker = 1\n",
            "marker",
            "break"
        )
        .unwrap_err()
        .contains("produced 0 exits")
    );
    assert!(
        binding_flow_loop_head_snapshot_at_marker("value = (\n", "value", None)
            .unwrap_err()
            .contains("did not parse")
    );
    assert!(
        binding_flow_loop_head_snapshot_at_marker("raise RuntimeError\nvalue = 1\n", "value", None)
            .unwrap_err()
            .contains("0 loop heads")
    );
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
    assert!(first == same);
    assert!(first != later);
    assert_eq!(first.partial_cmp(&later), Some(std::cmp::Ordering::Less));
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
    assert_eq!(prefix.push(candidate()), Err(error));
    assert!(prefix.finish().is_err());
}

#[test]
fn known_imports_transfer_nested_assignment_targets_annotations_and_augments() {
    let parsed = parse_module("from typing import TypeVar\n[a, (b, *rest)] = TypeVar('T')\nattribute.value = TypeVar('U')\na: object = 1\nb += 1\ndel rest\n").unwrap();
    let mut imports = KnownImports::default();
    imports.transfer_statement(&parsed.syntax().body[0]);
    imports.transfer_statement(&parsed.syntax().body[1]);
    for name in ["a", "b", "rest"] {
        assert_eq!(imports.bindings.get(name), Some(&KnownBinding::TypeVar));
    }
    imports.transfer_statement(&parsed.syntax().body[2]);
    for statement in &parsed.syntax().body[3..] {
        imports.transfer_statement(statement);
    }
    for name in ["a", "b", "rest", "attribute"] {
        assert!(!imports.bindings.contains_key(name));
    }
}

#[test]
fn function_local_collection_tracks_binding_headers_but_not_deferred_bodies() {
    let parsed = parse_module("import package.module\n@(decorator := identity)\ndef nested[T](arg: (annotation := int) = (default := 1)) -> (result := int):\n    deferred = 1\n@(class_decorator := identity)\nclass Nested(base):\n    class_body = 1\n[a, (b, *rest)] = values\na += 1\ndel b\ntype Alias = int\nmatch values:\n    case {'key': item, **mapping_rest}: pass\n    case [head, *tail]: pass\n").unwrap();
    let mut collector = FunctionLocalCollector::default();
    collector.visit_body(&parsed.syntax().body);
    for name in [
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
        "tail",
    ] {
        assert!(collector.locals.contains(name), "missing {name}");
    }
    assert!(!collector.locals.contains("deferred"));
    assert!(!collector.locals.contains("class_body"));
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
    assert!(matches!(
        analyze_source_with_exceptions(&request, "changed = 1\n", &|| false, Some(&index)),
        Err(AnalysisError::HierarchySourceChanged)
    ));
}

#[test]
fn cancellation_at_each_analysis_checkpoint_releases_retained_payload() {
    let source = "from dataclasses import dataclass, field\nfrom enum import Enum\nfrom typing import Optional, Sequence\nimport operator\nimport functools\n@dataclass\nclass Record:\n    values: list[int] = field(default_factory=list)\nclass Choice(Enum):\n    FIRST = 1\n    SECOND = 2\nclass Root(Exception): pass\nclass Child(Root): pass\ndef selected(value: Optional[int] = None):\n    values = [True, False]\n    values.append(1)\n    result = operator.add(1, 2)\n    for item in values:\n        if item and result or value:\n            break\n    try:\n        return sorted(values, reverse=True)\n    except Child:\n        raise Root()\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.include(operator);
        }
    }
    let mut reached_completion = false;
    for checkpoint in 0..4096 {
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
        let polls = Cell::new(0usize);
        let result = analyze_source_cancellable(&request, source, || {
            let current = polls.get();
            polls.set(current + 1);
            current >= checkpoint
        });
        match result {
            Ok(output) => {
                assert_ne!(output.candidates.len(), 0);
                reached_completion = true;
            }
            Err(error) => assert_eq!(error, AnalysisError::Cancelled),
        }
        assert_eq!(payload.snapshot().used, 0);
        if reached_completion {
            break;
        }
    }
    assert!(
        reached_completion,
        "analysis must eventually complete without cancellation"
    );
}

#[test]
fn payload_exhaustion_at_different_producer_boundaries_releases_all_charges() {
    let source = "from typing import Optional\ndef selected(value: Optional[int]):\n    values = [True, False]\n    values.append(1)\n    values.extend([2])\n    result = {'key': 1}.get('key', 0)\n    return (values, result)\n";
    let operators = MutationOperatorSelection::default();
    let mut failures = 0;
    let mut successes = 0;
    for limit in (0..2048).step_by(16).chain([4096, 8192, 16384, 32768]) {
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
        match analyze_source_cancellable(&request, source, || false) {
            Ok(output) => {
                assert_ne!(output.candidates.len(), 0);
                successes += 1;
            }
            Err(error) => {
                assert_eq!(error, AnalysisError::PayloadLimit { limit });
                failures += 1;
            }
        }
        assert_eq!(
            payload.snapshot().used,
            0,
            "leaked payload at limit {limit}"
        );
    }
    assert!(failures > 0);
    assert!(successes > 0);
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
    assert_eq!(
        collector.imports.bindings.get("Optional"),
        Some(&KnownBinding::Direct("typing.Optional".into()))
    );
    collector.visit_stmt(&parsed.syntax().body[2]);
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
    assert!(!imports.bindings.contains_key("List"));
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
    assert!(retained_by_profile(
        &candidate,
        MutationProfile::Focused,
        &facts
    ));
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
    assert_eq!(normal.candidates, broken.candidates);
    assert_eq!(normal_builds, 0);
    assert!(COLLECTION_REPLACEMENT_BUILDS.get() > normal_builds);
}

#[rstest::rstest]
#[case("match value:\n    case [*List]: pass\n", false)]
#[case("match value:\n    case [*_]: pass\n", true)]
fn star_pattern_flow_preserves_only_uncaptured_imports(
    #[case] source: &str,
    #[case] preserved: bool,
) {
    let parsed = parse_module(source).unwrap();
    let Stmt::Match(statement) = &parsed.syntax().body[0] else {
        // Each parameterized literal starts with a match statement.
        #[cfg(coverage_nightly)]
        {
            (#[coverage(off)]
            || panic!("expected match fixture"))()
        }
        #[cfg(not(coverage_nightly))]
        {
            panic!("expected match fixture")
        }
    };
    let mut imports = KnownImports::default();
    imports
        .bindings
        .insert("List".into(), KnownBinding::Direct("typing.List".into()));
    let result = pattern_binding_flow(&imports, &statement.cases[0].pattern);
    assert_eq!(
        result.matched.unwrap().bindings.contains_key("List"),
        preserved
    );
    assert_eq!(result.failed.unwrap(), imports);
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

#[test]
fn module_identity_queries_reject_class_namespace_and_unrecorded_expression() {
    let parsed =
        parse_module("import dependency as alias\nalias\nclass Box:\n    alias\n").unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["alias".to_owned()]),
    );
    let Stmt::Expr(module_load) = &parsed.syntax().body[1] else {
        // The selected fixed alias statement is an expression.
        #[cfg(coverage_nightly)]
        {
            (#[coverage(off)]
            || panic!("expected expression fixture"))()
        }
        #[cfg(not(coverage_nightly))]
        {
            panic!("expected expression fixture")
        }
    };
    assert!(index.resolves_unique_module(&module_load.value, "alias"));
    assert!(!index.resolves_unique_module(&module_load.value, "missing"));
    let Stmt::ClassDef(class) = &parsed.syntax().body[2] else {
        // The third fixed statement is the literal class definition.
        #[cfg(coverage_nightly)]
        {
            (#[coverage(off)]
            || panic!("expected class fixture"))()
        }
        #[cfg(not(coverage_nightly))]
        {
            panic!("expected class fixture")
        }
    };
    let Stmt::Expr(class_load) = &class.body[0] else {
        // The selected fixed alias statement is an expression.
        #[cfg(coverage_nightly)]
        {
            (#[coverage(off)]
            || panic!("expected expression fixture"))()
        }
        #[cfg(not(coverage_nightly))]
        {
            panic!("expected expression fixture")
        }
    };
    assert!(!index.resolves_unique_module(&class_load.value, "alias"));
    assert!(!index.annotation_import_stable(usize::MAX, "alias", true));
    let another = parse_module("alias = 1\n").unwrap();
    let Stmt::Assign(assignment) = &another.syntax().body[0] else {
        // The separate literal `alias = 1` is an assignment.
        #[cfg(coverage_nightly)]
        {
            (#[coverage(off)]
            || panic!("expected assignment fixture"))()
        }
        #[cfg(not(coverage_nightly))]
        {
            panic!("expected assignment fixture")
        }
    };
    assert!(!index.resolves_unique_module(&assignment.targets[0], "alias"));
}

#[test]
fn annotation_nonlocal_import_finds_owner_across_intermediate_function_scope() {
    let source = "def outer():\n    from typing import Optional as O\n    def middle():\n        def inner():\n            nonlocal O\n            value: O[int]\n";
    let parsed = parse_module(source).unwrap();
    let index = NameResolutionIndex::from_module_with_names(
        parsed.syntax(),
        HashSet::from(["O".to_owned()]),
    );
    let offset = source.find("O[int]").unwrap();
    assert!(index.annotation_import_stable(offset, "O", true));
    assert!(!index.annotation_import_stable(offset, "O", false));
}
