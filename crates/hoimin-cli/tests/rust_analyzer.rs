#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use hoimin_python::AnalyzerDiagnosticCode;

use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};
use ruff_python_parser::parse_module;

#[test]
fn typing_import_rebinding_inventory_is_site_aware() {
    let source = "from typing import Sequence\nbefore_direct: list[str]\nSequence = object\nafter_direct: list[str]\nimport typing as t\nbefore_alias: list[str]\nt = object\nafter_alias: list[str]\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name)
            .expect("a public valid operator name parses")
        {
            operators.exclude(operator);
        }
    }
    operators.include(MutationOperator::TypeListSequence);
    assert_eq!(operators.names(), vec!["type_list_sequence"]);

    let output = analyze(
        &hoimin_python::AnalyzeRequest {
            payload: hoimin_python::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/typing_rebinding.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 10_000,
        },
        source,
    );

    // Python 3.14 defers these module annotations: both imported spellings
    // are rebound before their values need to be evaluated.
    assert_eq!(output.candidates, Vec::new());

    for candidate in &output.candidates {
        let start = usize::try_from(candidate.span.start).unwrap();
        let length = usize::try_from(candidate.span.length).unwrap();
        let mut mutated = source.to_owned();
        mutated.replace_range(start..start + length, &candidate.replacement);
        assert!(parse_module(&mutated).is_ok());
    }
}

#[allow(
    dead_code,
    reason = "shared allocator also supports peak measurements in other tests"
)]
mod heap {
    include!("support/heap_tracking.rs");
    pub fn live() -> usize {
        LIVE.load(Ordering::Relaxed)
    }
}
#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;

#[test]
fn repeated_depth_rejections_reclaim_ast_allocations() {
    // Isolate allocator accounting from the harness's parallel tests, and keep an
    // accidental recursive drop in a bounded child rather than the main runner.
    let log = tempfile::NamedTempFile::new().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "depth_cleanup_child_fixture"])
        .env("HOIMIN_DEPTH_CLEANUP_CHILD", "1")
        .env_remove("RUST_MIN_STACK")
        .stdout(log.reopen().unwrap())
        .stderr(log.reopen().unwrap())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "{}",
                std::fs::read_to_string(log.path()).unwrap()
            );
            return;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("cleanup timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn depth_cleanup_child_cases() -> Vec<String> {
    std::env::var("HOIMIN_DEPTH_CLEANUP_CHILD")
        .into_iter()
        .collect()
}

#[test]
fn depth_cleanup_child_fixture() {
    for _ in depth_cleanup_child_cases() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let operators = hoimin_core::MutationOperatorSelection::default();
                let request = hoimin_python::AnalyzeRequest {
                    payload: hoimin_python::payload::PayloadBudget::default(),
                    path: camino::Utf8Path::new("calc.py"),
                    lines: &[],
                    symbols: &[],
                    operators: &operators,
                    profile: hoimin_core::MutationProfile::Full,
                    max_candidates: 1,
                };
                let sources = [
                    format!("value = {}\n", vec!["1"; 25_000].join("+")),
                    format!("value = {}1\n", "-".repeat(1_000)),
                    format!("value = {}\n", vec!["1"; 2_000].join("**")),
                    format!("value = {}1\n", "lambda: ".repeat(2_000)),
                    format!("value = {}1\n", "1 if x else ".repeat(2_000)),
                ];
                // Warm parser thread-local state before checking retained allocations.
                for source in &sources {
                    assert!(
                        hoimin_python::analyze_source_cancellable(&request, source, || false)
                            .is_err()
                    );
                }
                let baseline = heap::live();
                for _ in 0..20 {
                    for source in &sources {
                        assert!(matches!(
                            hoimin_python::analyze_source_cancellable(&request, source, || false),
                            Err(hoimin_python::AnalysisError::DepthExceeded { limit: 128 })
                        ));
                        assert!(
                            heap::live() <= baseline + 1024,
                            "rejection retained AST memory"
                        );
                    }
                }
                // Recovery discards trees before returning a module. Process-exit
                // reclamation cannot stand in for releasing those owned subtrees.
                let recovered = [
                    format!(
                        "with ({}) as context:\n pass\n",
                        vec!["1"; 25_000].join("+")
                    ),
                    format!("match({})\n", vec!["1"; 25_000].join("+")),
                    format!("match x:\n case C(a{}=other): pass\n", ".b".repeat(25_000)),
                    format!("match x:\n case (a{} as y)(): pass\n", ".b".repeat(25_000)),
                ];
                for source in &recovered {
                    let _ = hoimin_python::analyze_source_cancellable(&request, source, || false);
                }
                let baseline = heap::live();
                for _ in 0..10 {
                    for source in &recovered {
                        let _ =
                            hoimin_python::analyze_source_cancellable(&request, source, || false);
                        assert!(
                            heap::live() <= baseline + 1024,
                            "parser recovery retained AST memory"
                        );
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

/// Recovering from an unterminated nested string of the other interpolation
/// kind returns the parser to the enclosing format specification while the
/// lexer still emits the nested string's middle token. Both middle tokens are
/// in that specification's FIRST set, so the element list must accept either
/// one; treating the foreign token as unreachable aborted the whole process.
/// `cargo fuzz run python_analyzer` found the f-string case.
#[rstest::rstest]
#[case::nested_template_full(r#"f"{:{t"{m""m"#, MutationProfile::Full)]
#[case::nested_template_focused(r#"f"{:{t"{m""m"#, MutationProfile::Focused)]
#[case::nested_fstring_full(r#"t"{:{f"{m""m"#, MutationProfile::Full)]
#[case::nested_fstring_focused(r#"t"{:{f"{m""m"#, MutationProfile::Focused)]
#[case::nested_template_newline_full("f\"{:{t\"{m\"\"m\n", MutationProfile::Full)]
#[case::nested_template_newline_focused("f\"{:{t\"{m\"\"m\n", MutationProfile::Focused)]
#[case::nested_fstring_newline_full("t\"{:{f\"{m\"\"m\n", MutationProfile::Full)]
#[case::nested_fstring_newline_focused("t\"{:{f\"{m\"\"m\n", MutationProfile::Focused)]
fn foreign_middle_token_in_a_format_spec_is_invalid_syntax(
    #[case] source: &str,
    #[case] profile: MutationProfile,
) {
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name)
            .expect("a public valid operator name parses")
        {
            operators.include(operator);
        }
    }

    assert!(
        parse_module(source).is_err(),
        "{source:?} is invalid Python"
    );

    let output = analyze(
        &hoimin_python::AnalyzeRequest {
            payload: hoimin_python::payload::PayloadBudget::default(),
            path: Utf8Path::new("pkg/format_spec.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile,
            max_candidates: 10_000,
        },
        source,
    );
    assert!(output.candidates.is_empty(), "{source:?}");
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![AnalyzerDiagnosticCode::InvalidSyntax],
        "{source:?}"
    );
}

#[test]
fn statement_deletion_obeys_selection_prefix_and_cancellation_bounded_prefix_1() {
    let source = "def first():\n    a()\ndef second():\n    b()\n    c()\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(MutationOperator::from_name("statement_delete").unwrap());
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let limited = analyze(&request, source);

    assert!(limited.truncated);
    assert_eq!(limited.candidates.len(), 1);
    assert_eq!(limited.candidates[0].original, "a()");
}

#[test]
fn statement_deletion_obeys_selection_prefix_and_cancellation_line_selection_2() {
    use hoimin_core::LineRange;
    let source = "def first():\n    a()\ndef second():\n    b()\n    c()\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(MutationOperator::from_name("statement_delete").unwrap());
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[LineRange { start: 4, end: 4 }];

    let selected = analyze(&request, source);

    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(selected.candidates[0].original, "b()");
}

#[test]
fn statement_deletion_obeys_selection_prefix_and_cancellation_symbol_selection_3() {
    use hoimin_core::LineRange;
    let source = "def first():\n    a()\ndef second():\n    b()\n    c()\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(MutationOperator::from_name("statement_delete").unwrap());
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 2);
}

#[test]
fn statement_deletion_obeys_selection_prefix_and_cancellation_cancelled_4() {
    use hoimin_core::LineRange;
    let source = "def first():\n    a()\ndef second():\n    b()\n    c()\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(MutationOperator::from_name("statement_delete").unwrap());
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn integer_neighbors_are_default_and_keep_a_bounded_prefix_operator_policy_1() {
    let operator = MutationOperator::from_name("integer_literal_neighbor").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn integer_neighbors_are_default_and_keep_a_bounded_prefix_bounded_prefix_2() {
    let operator = MutationOperator::from_name("integer_literal_neighbor").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def f():\n    return -3\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
    assert_eq!(output.candidates[0].original, "-3");
}

#[test]
fn integer_neighbors_are_default_and_keep_a_bounded_prefix_cancelled_3() {
    let operator = MutationOperator::from_name("integer_literal_neighbor").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def f():\n    return -3\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn integer_neighbors_are_default_and_keep_a_bounded_prefix_operator_policy_4() {
    let operator = MutationOperator::from_name("integer_literal_neighbor").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn condition_constants_respect_focused_guards_and_limits_operator_policy_1() {
    let operator = MutationOperator::from_name("condition_constant").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn condition_constants_respect_focused_guards_and_limits_bounded_prefix_2() {
    let operator = MutationOperator::from_name("condition_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "if __name__ == '__main__':\n    run()\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
}

#[test]
fn condition_constants_respect_focused_guards_and_limits_focused_profile_3() {
    let operator = MutationOperator::from_name("condition_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "if __name__ == '__main__':\n    run()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.profile = MutationProfile::Focused;

    let actual = analyze(&request, source).candidates;

    assert_eq!(actual, Vec::new());
}

#[test]
fn condition_constants_respect_focused_guards_and_limits_cancelled_4() {
    let operator = MutationOperator::from_name("condition_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "if __name__ == '__main__':\n    run()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.profile = MutationProfile::Focused;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn body_erasure_anchors_first_erased_line_and_owning_function_operator_policy_1() {
    let operator = MutationOperator::from_name("function_body_erase").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn body_erasure_anchors_first_erased_line_and_owning_function_bounded_prefix_2() {
    let operator = MutationOperator::from_name("function_body_erase").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer():\n    'doc'\n    def inner():\n        return 1\n    call()\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[hoimin_core::LineRange { start: 5, end: 5 }],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };

    let actual = analyze(&request, source).candidates;

    assert_eq!(actual, Vec::new());
}

#[test]
fn body_erasure_anchors_first_erased_line_and_owning_function_line_selection_3() {
    let operator = MutationOperator::from_name("function_body_erase").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer():\n    'doc'\n    def inner():\n        return 1\n    call()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[hoimin_core::LineRange { start: 5, end: 5 }],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];

    let output = analyze(&request, source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("outer"));
}

#[test]
fn body_erasure_anchors_first_erased_line_and_owning_function_symbol_selection_4() {
    let operator = MutationOperator::from_name("function_body_erase").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer():\n    'doc'\n    def inner():\n        return 1\n    call()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[hoimin_core::LineRange { start: 5, end: 5 }],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    let symbols = ["outer.inner".to_owned()];
    request.symbols = &symbols;
    request.lines = &[];

    let actual = analyze(&request, source).candidates;

    assert_eq!(actual, Vec::new());
}

#[test]
fn body_erasure_anchors_first_erased_line_and_owning_function_cancelled_5() {
    let operator = MutationOperator::from_name("function_body_erase").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer():\n    'doc'\n    def inner():\n        return 1\n    call()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[hoimin_core::LineRange { start: 5, end: 5 }],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    let symbols = ["outer.inner".to_owned()];
    request.symbols = &symbols;
    request.lines = &[];

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn enum_replacement_is_opt_in_bounded_selected_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("enum_member_replace").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn enum_replacement_is_opt_in_bounded_selected_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("enum_member_replace").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "from enum import Enum\nclass Status(Enum):\n    A=1; B=2; C=3\ndef choose():\n    return Status.A\nx=Status.B\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let limited = analyze(&request, source);

    assert_eq!(limited.candidates.len(), 1);
    assert!(limited.truncated);
    assert_eq!(limited.candidates[0].replacement, "B");
}

#[test]
fn enum_replacement_is_opt_in_bounded_selected_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("enum_member_replace").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "from enum import Enum\nclass Status(Enum):\n    A=1; B=2; C=3\ndef choose():\n    return Status.A\nx=Status.B\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 6, end: 6 }];

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 2);
}

#[test]
fn enum_replacement_is_opt_in_bounded_selected_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("enum_member_replace").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "from enum import Enum\nclass Status(Enum):\n    A=1; B=2; C=3\ndef choose():\n    return Status.A\nx=Status.B\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 6, end: 6 }];
    request.lines = &[];
    let symbols = ["choose".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 2);
}

#[test]
fn enum_replacement_is_opt_in_bounded_selected_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("enum_member_replace").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "from enum import Enum\nclass Status(Enum):\n    A=1; B=2; C=3\ndef choose():\n    return Status.A\nx=Status.B\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 6, end: 6 }];
    request.lines = &[];
    let symbols = ["choose".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn enum_replacement_is_opt_in_bounded_selected_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("enum_member_replace").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn augmented_assignment_is_default_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("augmented_to_assignment").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn augmented_assignment_is_default_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("augmented_to_assignment").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(x):\n    x += 1\ndef second(x):\n    x *= 2\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "+=");
}

#[test]
fn augmented_assignment_is_default_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("augmented_to_assignment").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(x):\n    x += 1\ndef second(x):\n    x *= 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];

    let output = analyze(&request, source);

    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "*=");
}

#[test]
fn augmented_assignment_is_default_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("augmented_to_assignment").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(x):\n    x += 1\ndef second(x):\n    x *= 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn augmented_assignment_is_default_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("augmented_to_assignment").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(x):\n    x += 1\ndef second(x):\n    x *= 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn augmented_assignment_is_default_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("augmented_to_assignment").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn return_tuple_swap_is_default_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("return_tuple_swap").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn return_tuple_swap_is_default_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("return_tuple_swap").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(): return 1, 2\ndef second(): return 3, 4\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].replacement, "2, 1");
}

#[test]
fn return_tuple_swap_is_default_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("return_tuple_swap").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(): return 1, 2\ndef second(): return 3, 4\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 2, end: 2 }];

    let actual = analyze(&request, source).candidates[0].replacement.clone();

    assert_eq!(actual, "4, 3");
}

#[test]
fn return_tuple_swap_is_default_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("return_tuple_swap").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(): return 1, 2\ndef second(): return 3, 4\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 2, end: 2 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn return_tuple_swap_is_default_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("return_tuple_swap").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(): return 1, 2\ndef second(): return 3, 4\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 2, end: 2 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn return_tuple_swap_is_default_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("return_tuple_swap").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn string_empty_is_default_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("string_literal_empty").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn string_empty_is_default_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("string_literal_empty").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "'doc'\ndef first(): return 'first'\ndef second(): return 'second'\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "'first'");
}

#[test]
fn string_empty_is_default_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("string_literal_empty").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "'doc'\ndef first(): return 'first'\ndef second(): return 'second'\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];

    let actual = analyze(&request, source).candidates[0].original.clone();

    assert_eq!(actual, "'second'");
}

#[test]
fn string_empty_is_default_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("string_literal_empty").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "'doc'\ndef first(): return 'first'\ndef second(): return 'second'\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn string_empty_is_default_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("string_literal_empty").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "'doc'\ndef first(): return 'first'\ndef second(): return 'second'\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn string_empty_is_default_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("string_literal_empty").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn while_false_is_default_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("while_condition_false").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn while_false_is_default_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("while_condition_false").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    while first_ready: break\ndef second():\n    while second_ready: break\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "first_ready");
}

#[test]
fn while_false_is_default_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("while_condition_false").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    while first_ready: break\ndef second():\n    while second_ready: break\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];

    let actual = analyze(&request, source).candidates[0].original.clone();

    assert_eq!(actual, "second_ready");
}

#[test]
fn while_false_is_default_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("while_condition_false").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    while first_ready: break\ndef second():\n    while second_ready: break\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn while_false_is_default_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("while_condition_false").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    while first_ready: break\ndef second():\n    while second_ready: break\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn while_false_is_default_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("while_condition_false").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn clause_delete_is_opt_in_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("condition_clause_delete").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn clause_delete_is_opt_in_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("condition_clause_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    if first_ready and allowed: pass\ndef second():\n    if second_ready or allowed: pass\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "first_ready and allowed");
}

#[test]
fn clause_delete_is_opt_in_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("condition_clause_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    if first_ready and allowed: pass\ndef second():\n    if second_ready or allowed: pass\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];

    let actual = analyze(&request, source).candidates[0].original.clone();

    assert_eq!(actual, "second_ready or allowed");
}

#[test]
fn clause_delete_is_opt_in_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("condition_clause_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    if first_ready and allowed: pass\ndef second():\n    if second_ready or allowed: pass\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 2);
}

#[test]
fn clause_delete_is_opt_in_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("condition_clause_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    if first_ready and allowed: pass\ndef second():\n    if second_ready or allowed: pass\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn clause_delete_is_opt_in_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("condition_clause_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn repeated_clauses_are_deduplicated_before_bounded_generation() {
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(MutationOperator::from_name("condition_clause_delete").unwrap());
    let source = format!("if {}: pass\n", vec!["a"; 4096].join(" and "));
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = analyze(&request, &source);
    assert_eq!(output.candidates.len(), 1);
    assert!(!output.truncated);
    assert_eq!(
        output.candidates[0].replacement.matches("(a)").count(),
        4095
    );
}

#[test]
fn container_delete_is_opt_in_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("container_element_delete").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn container_delete_is_opt_in_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("container_element_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return [1]\ndef second():\n    return [2]\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "[1]");
}

#[test]
fn container_delete_is_opt_in_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("container_element_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return [1]\ndef second():\n    return [2]\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];

    let actual = analyze(&request, source).candidates[0].original.clone();

    assert_eq!(actual, "[2]");
}

#[test]
fn container_delete_is_opt_in_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("container_element_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return [1]\ndef second():\n    return [2]\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn container_delete_is_opt_in_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("container_element_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return [1]\ndef second():\n    return [2]\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn container_delete_is_opt_in_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("container_element_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn repeated_container_elements_are_deduplicated_before_bounded_generation() {
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(MutationOperator::from_name("container_element_delete").unwrap());
    let source = format!("value = [{}]\n", vec!["a"; 4096].join(","));
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = analyze(&request, &source);
    assert_eq!(output.candidates.len(), 1);
    assert!(!output.truncated);
    assert_eq!(
        output.candidates[0].replacement.matches("(a)").count(),
        4095
    );
}

#[test]
fn conversion_remove_is_default_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("conversion_call_remove").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn conversion_remove_is_default_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("conversion_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return int(1)\ndef second():\n    return int(2)\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "int(1)");
}

#[test]
fn conversion_remove_is_default_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("conversion_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return int(1)\ndef second():\n    return int(2)\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];

    let actual = analyze(&request, source).candidates[0].original.clone();

    assert_eq!(actual, "int(2)");
}

#[test]
fn conversion_remove_is_default_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("conversion_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return int(1)\ndef second():\n    return int(2)\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn conversion_remove_is_default_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("conversion_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return int(1)\ndef second():\n    return int(2)\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn conversion_remove_is_default_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("conversion_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn optional_keyword_is_opt_in_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("optional_keyword_delete").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn optional_keyword_is_opt_in_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("optional_keyword_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def target(*, flag=True): return flag\ndef first(): return target(flag=False)\ndef second(): return target(flag=0)\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "target(flag=False)");
}

#[test]
fn optional_keyword_is_opt_in_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("optional_keyword_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def target(*, flag=True): return flag\ndef first(): return target(flag=False)\ndef second(): return target(flag=0)\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];

    let actual = analyze(&request, source).candidates[0].original.clone();

    assert_eq!(actual, "target(flag=0)");
}

#[test]
fn optional_keyword_is_opt_in_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("optional_keyword_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def target(*, flag=True): return flag\ndef first(): return target(flag=False)\ndef second(): return target(flag=0)\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn optional_keyword_is_opt_in_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("optional_keyword_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def target(*, flag=True): return flag\ndef first(): return target(flag=False)\ndef second(): return target(flag=0)\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn optional_keyword_is_opt_in_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("optional_keyword_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[rstest::rstest]
#[case::mutationprofile_full_true_1(MutationProfile::Full, true)]
#[case::mutationprofile_focused_false_2(MutationProfile::Focused, false)]
fn promoted_defaults_keep_profile_filtering_before_bounded_prefix_selection_full_generation(
    #[case] profile: MutationProfile,
    #[case] includes_first_line: bool,
) {
    let operators = MutationOperatorSelection::default();
    let source = "print('skip', 7)\ndef f(x, y):\n    x += 3\n    while x < y:\n        x += 1\n    z = int(x)\n    return x, y\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile,
        max_candidates: 1000,
    };

    let full = analyze(&request, source);

    assert!(!full.truncated);
    assert!(full.candidates.len() > 3);
    assert_eq!(
        full.candidates.iter().any(|candidate| candidate.line == 1),
        includes_first_line
    );
    assert_eq!(
        full.candidates
            .iter()
            .any(|candidate| candidate.line == 1
                && candidate.operator.as_str() == "statement_delete"),
        includes_first_line
    );
}

#[rstest::rstest]
#[case::mutationprofile_full_true_1(MutationProfile::Full)]
#[case::mutationprofile_focused_false_2(MutationProfile::Focused)]
fn promoted_defaults_keep_profile_filtering_before_bounded_prefix_selection_bounded_prefix_2(
    #[case] profile: MutationProfile,
) {
    let operators = MutationOperatorSelection::default();
    let source = "print('skip', 7)\ndef f(x, y):\n    x += 3\n    while x < y:\n        x += 1\n    z = int(x)\n    return x, y\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile,
        max_candidates: 1000,
    };
    let full = analyze(&request, source);
    request.max_candidates = 3;

    let bounded = analyze(&request, source);

    assert_eq!(bounded.candidates, full.candidates[..3]);
    assert!(bounded.truncated);
}

#[rstest::rstest]
#[case::mutationprofile_full_true_1(MutationProfile::Full)]
#[case::mutationprofile_focused_false_2(MutationProfile::Focused)]
fn promoted_defaults_keep_profile_filtering_before_bounded_prefix_selection_cancelled_3(
    #[case] profile: MutationProfile,
) {
    let operators = MutationOperatorSelection::default();
    let source = "print('skip', 7)\ndef f(x, y):\n    x += 3\n    while x < y:\n        x += 1\n    z = int(x)\n    return x, y\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile,
        max_candidates: 1000,
    };
    request.max_candidates = 3;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn method_remove_is_opt_out_selected_bounded_and_cancellable_operator_policy_1() {
    let operator = MutationOperator::from_name("method_call_remove").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn method_remove_is_opt_out_selected_bounded_and_cancellable_bounded_prefix_2() {
    let operator = MutationOperator::from_name("method_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return first_value.clean()\ndef second():\n    return second_value.clean()\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };

    let output = analyze(&request, source);

    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "first_value.clean()");
}

#[test]
fn method_remove_is_opt_out_selected_bounded_and_cancellable_line_selection_3() {
    let operator = MutationOperator::from_name("method_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return first_value.clean()\ndef second():\n    return second_value.clean()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];

    let actual = analyze(&request, source).candidates[0].original.clone();

    assert_eq!(actual, "second_value.clean()");
}

#[test]
fn method_remove_is_opt_out_selected_bounded_and_cancellable_symbol_selection_4() {
    let operator = MutationOperator::from_name("method_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return first_value.clean()\ndef second():\n    return second_value.clean()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 1);
}

#[test]
fn method_remove_is_opt_out_selected_bounded_and_cancellable_cancelled_5() {
    let operator = MutationOperator::from_name("method_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return first_value.clean()\ndef second():\n    return second_value.clean()\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn method_remove_is_opt_out_selected_bounded_and_cancellable_operator_policy_6() {
    let operator = MutationOperator::from_name("method_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_operator_policy_1() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let operators = MutationOperatorSelection::default();

    let actual = operators.contains(operator);

    assert!(actual);
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_bounded_prefix_2() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer() -> int:\n    'doc'\n    def inner(): return 1\n    return 2\n";
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };

    let full = analyze(&request, source);

    assert_eq!(full.candidates.len(), 2);
    assert!(
        full.candidates
            .iter()
            .all(|c| c.line == 3 && c.symbol.as_deref() == Some("outer"))
    );
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_bounded_prefix_3() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer() -> int:\n    'doc'\n    def inner(): return 1\n    return 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    let full = analyze(&request, source);
    request.max_candidates = 1;

    let bounded = analyze(&request, source);

    assert!(bounded.truncated);
    assert_eq!(bounded.candidates, full.candidates[..1]);
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_line_selection_4() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer() -> int:\n    'doc'\n    def inner(): return 1\n    return 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.max_candidates = 1;
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];

    let actual = analyze(&request, source).candidates;

    assert_eq!(actual, Vec::new());
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_line_selection_5() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer() -> int:\n    'doc'\n    def inner(): return 1\n    return 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.max_candidates = 1;
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];

    let actual = analyze(&request, source).candidates.len();

    assert_eq!(actual, 2);
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_symbol_selection_6() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer() -> int:\n    'doc'\n    def inner(): return 1\n    return 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.max_candidates = 1;
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["outer.inner".to_owned()];
    request.symbols = &symbols;

    let actual = analyze(&request, source).candidates;

    assert_eq!(actual, Vec::new());
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_symbol_selection_7() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.max_candidates = 1;
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["outer.inner".to_owned()];
    request.symbols = &symbols;
    request.symbols = &[];
    let guarded = "if __name__ == '__main__':\n    def f(x) -> int: return x\n";

    let actual = analyze(&request, guarded).candidates.len();

    assert_eq!(actual, 2);
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_symbol_selection_8() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.max_candidates = 1;
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["outer.inner".to_owned()];
    request.symbols = &symbols;
    request.symbols = &[];
    let guarded = "if __name__ == '__main__':\n    def f(x) -> int: return x\n";
    request.profile = MutationProfile::Focused;

    let actual = analyze(&request, guarded).candidates;

    assert_eq!(actual, Vec::new());
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_cancelled_9() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer() -> int:\n    'doc'\n    def inner(): return 1\n    return 2\n";
    let mut request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    request.max_candidates = 1;
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    request.lines = &[];
    let symbols = ["outer.inner".to_owned()];
    request.symbols = &symbols;
    request.symbols = &[];
    request.profile = MutationProfile::Focused;

    let actual = hoimin_python::analyze_source_cancellable(&request, source, || true);

    assert!(actual.is_err());
}

#[test]
fn return_constant_preserves_scope_selection_profile_prefix_and_cancellation_operator_policy_10() {
    let operator = MutationOperator::from_name("function_body_return_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    operators.exclude(operator);

    let actual = !operators.contains(operator);

    assert!(actual);
}

fn analyze(
    request: &hoimin_python::AnalyzeRequest<'_>,
    source: &str,
) -> hoimin_python::AnalyzerOutput {
    hoimin_python::analyze_source_cancellable(request, source, || false).unwrap()
}
