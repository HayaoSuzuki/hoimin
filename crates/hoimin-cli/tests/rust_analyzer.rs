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
