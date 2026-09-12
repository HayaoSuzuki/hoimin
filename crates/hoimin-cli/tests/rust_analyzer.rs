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
use hoimin_core::{ByteSpan, MutationOperator, MutationOperatorSelection, MutationProfile};
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

    assert_eq!(
        output.candidates,
        vec![
            AnalyzerCandidate {
                path: Utf8Path::new("pkg/typing_rebinding.py").to_path_buf(),
                span: ByteSpan {
                    start: 43,
                    length: 9,
                },
                original: "list[str]".to_owned(),
                replacement: "Sequence[str]".to_owned(),
                operator: "type_list_sequence".to_owned(),
                line: 2,
                column: 15,
                symbol: None,
            },
            AnalyzerCandidate {
                path: Utf8Path::new("pkg/typing_rebinding.py").to_path_buf(),
                span: ByteSpan {
                    start: 128,
                    length: 9,
                },
                original: "list[str]".to_owned(),
                replacement: "t.Sequence[str]".to_owned(),
                operator: "type_list_sequence".to_owned(),
                line: 6,
                column: 14,
                symbol: None,
            },
        ]
    );

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
