#![allow(dead_code)]

#[path = "../src/analyzer/protocol.rs"]
mod protocol;

pub use protocol::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

mod analyzer {
    pub use crate::protocol::AnalyzerDiagnosticCode;
}

#[path = "../src/analyzer/rust.rs"]
mod rust;

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

    let output = rust::analyze_source(
        &rust::AnalyzeRequest {
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
    if std::env::var_os("HOIMIN_DEPTH_CLEANUP_CHILD").is_none() {
        let log = tempfile::NamedTempFile::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "repeated_depth_rejections_reclaim_ast_allocations",
            ])
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
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let operators = hoimin_core::MutationOperatorSelection::default();
            let request = rust::AnalyzeRequest {
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
                assert!(rust::analyze_source_cancellable(&request, source, || false).is_err());
            }
            let baseline = heap::live();
            for _ in 0..20 {
                for source in &sources {
                    assert!(matches!(
                        rust::analyze_source_cancellable(&request, source, || false),
                        Err(rust::AnalysisError::DepthExceeded { limit: 128 })
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
                let _ = rust::analyze_source_cancellable(&request, source, || false);
            }
            let baseline = heap::live();
            for _ in 0..10 {
                for source in &recovered {
                    let _ = rust::analyze_source_cancellable(&request, source, || false);
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

/// Recovering from an unterminated nested string of the other interpolation
/// kind returns the parser to the enclosing format specification while the
/// lexer still emits the nested string's middle token. Both middle tokens are
/// in that specification's FIRST set, so the element list must accept either
/// one; treating the foreign token as unreachable aborted the whole process.
/// `cargo fuzz run python_analyzer` found the f-string case.
#[test]
fn foreign_middle_token_in_a_format_spec_is_invalid_syntax() {
    let mut operators = MutationOperatorSelection::all_legacy();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name)
            .expect("a public valid operator name parses")
        {
            operators.include(operator);
        }
    }
    for source in [
        r#"f"{:{t"{m""m"#,
        r#"t"{:{f"{m""m"#,
        "f\"{:{t\"{m\"\"m\n",
        "t\"{:{f\"{m\"\"m\n",
    ] {
        assert!(
            parse_module(source).is_err(),
            "{source:?} is invalid Python"
        );
        for profile in [MutationProfile::Full, MutationProfile::Focused] {
            let output = rust::analyze_source(
                &rust::AnalyzeRequest {
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
    }
}

#[test]
fn statement_deletion_obeys_selection_prefix_and_cancellation() {
    use hoimin_core::LineRange;
    let source = "def first():\n    a()\ndef second():\n    b()\n    c()\n";
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(MutationOperator::from_name("statement_delete").unwrap());
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let limited = rust::analyze_source(&request, source);
    assert!(limited.truncated);
    assert_eq!(limited.candidates.len(), 1);
    assert_eq!(limited.candidates[0].original, "a()");
    request.max_candidates = 10;
    request.lines = &[LineRange { start: 4, end: 4 }];
    let selected = rust::analyze_source(&request, source);
    assert_eq!(selected.candidates.len(), 1);
    assert_eq!(selected.candidates[0].original, "b()");
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 2);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
}

#[test]
fn integer_neighbors_remain_opt_in_and_keep_a_bounded_prefix() {
    let operator = MutationOperator::from_name("integer_literal_neighbor").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def f():\n    return -3\n";
    let request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
    assert_eq!(output.candidates[0].original, "-3");
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[test]
fn condition_constants_respect_focused_guards_and_limits() {
    let operator = MutationOperator::from_name("condition_constant").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "if __name__ == '__main__':\n    run()\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert_eq!(output.candidates.len(), 1);
    assert!(output.truncated);
    request.profile = MutationProfile::Focused;
    assert!(rust::analyze_source(&request, source).candidates.is_empty());
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
}

#[test]
fn body_erasure_anchors_first_erased_line_and_owning_function() {
    let operator = MutationOperator::from_name("function_body_erase").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def outer():\n    'doc'\n    def inner():\n        return 1\n    call()\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[hoimin_core::LineRange { start: 5, end: 5 }],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 10,
    };
    assert!(rust::analyze_source(&request, source).candidates.is_empty());
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    let output = rust::analyze_source(&request, source);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].symbol.as_deref(), Some("outer"));
    let symbols = ["outer.inner".to_owned()];
    request.symbols = &symbols;
    request.lines = &[];
    assert!(rust::analyze_source(&request, source).candidates.is_empty());
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
}

#[test]
fn enum_replacement_is_opt_in_bounded_selected_and_cancellable() {
    let operator = MutationOperator::from_name("enum_member_replace").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "from enum import Enum\nclass Status(Enum):\n    A=1; B=2; C=3\ndef choose():\n    return Status.A\nx=Status.B\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let limited = rust::analyze_source(&request, source);
    assert_eq!(limited.candidates.len(), 1);
    assert!(limited.truncated);
    assert_eq!(limited.candidates[0].replacement, "B");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 6, end: 6 }];
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 2);
    request.lines = &[];
    let symbols = ["choose".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 2);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[test]
fn augmented_assignment_is_opt_in_selected_bounded_and_cancellable() {
    let operator = MutationOperator::from_name("augmented_to_assignment").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(x):\n    x += 1\ndef second(x):\n    x *= 2\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "+=");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    let output = rust::analyze_source(&request, source);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "*=");
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 1);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[test]
fn return_tuple_swap_is_opt_in_selected_bounded_and_cancellable() {
    let operator = MutationOperator::from_name("return_tuple_swap").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first(): return 1, 2\ndef second(): return 3, 4\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].replacement, "2, 1");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 2, end: 2 }];
    assert_eq!(
        rust::analyze_source(&request, source).candidates[0].replacement,
        "4, 3"
    );
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 1);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[test]
fn string_empty_is_opt_in_selected_bounded_and_cancellable() {
    let operator = MutationOperator::from_name("string_literal_empty").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "'doc'\ndef first(): return 'first'\ndef second(): return 'second'\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "'first'");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 3, end: 3 }];
    assert_eq!(
        rust::analyze_source(&request, source).candidates[0].original,
        "'second'"
    );
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 1);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[test]
fn while_false_is_opt_in_selected_bounded_and_cancellable() {
    let operator = MutationOperator::from_name("while_condition_false").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    while first_ready: break\ndef second():\n    while second_ready: break\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "first_ready");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    assert_eq!(
        rust::analyze_source(&request, source).candidates[0].original,
        "second_ready"
    );
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 1);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}

#[test]
fn clause_delete_is_opt_in_selected_bounded_and_cancellable() {
    let operator = MutationOperator::from_name("condition_clause_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    if first_ready and allowed: pass\ndef second():\n    if second_ready or allowed: pass\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "first_ready and allowed");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    assert_eq!(
        rust::analyze_source(&request, source).candidates[0].original,
        "second_ready or allowed"
    );
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 2);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
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
    let request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, &source);
    assert_eq!(output.candidates.len(), 1);
    assert!(!output.truncated);
    assert_eq!(
        output.candidates[0].replacement.matches("(a)").count(),
        4095
    );
}

#[test]
fn container_delete_is_opt_in_selected_bounded_and_cancellable() {
    let operator = MutationOperator::from_name("container_element_delete").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return [1]\ndef second():\n    return [2]\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "[1]");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    assert_eq!(
        rust::analyze_source(&request, source).candidates[0].original,
        "[2]"
    );
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 1);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
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
    let request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, &source);
    assert_eq!(output.candidates.len(), 1);
    assert!(!output.truncated);
    assert_eq!(
        output.candidates[0].replacement.matches("(a)").count(),
        4095
    );
}

#[test]
fn conversion_remove_is_opt_in_selected_bounded_and_cancellable() {
    let operator = MutationOperator::from_name("conversion_call_remove").unwrap();
    let mut operators = MutationOperatorSelection::default();
    assert!(!operators.contains(operator));
    for name in MutationOperatorSelection::valid_names() {
        for op in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(op);
        }
    }
    operators.include(operator);
    let source = "def first():\n    return int(1)\ndef second():\n    return int(2)\n";
    let mut request = rust::AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 1,
    };
    let output = rust::analyze_source(&request, source);
    assert!(output.truncated);
    assert_eq!(output.candidates.len(), 1);
    assert_eq!(output.candidates[0].original, "int(1)");
    request.max_candidates = 10;
    request.lines = &[hoimin_core::LineRange { start: 4, end: 4 }];
    assert_eq!(
        rust::analyze_source(&request, source).candidates[0].original,
        "int(2)"
    );
    request.lines = &[];
    let symbols = ["second".to_owned()];
    request.symbols = &symbols;
    assert_eq!(rust::analyze_source(&request, source).candidates.len(), 1);
    assert!(rust::analyze_source_cancellable(&request, source, || true).is_err());
    operators.exclude(operator);
    assert!(!operators.contains(operator));
}
