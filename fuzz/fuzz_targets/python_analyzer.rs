#![no_main]

// These are the actual shipping modules, not copies. Keep their parser patch
// and dependency versions aligned with the root and CLI manifests.
#[allow(dead_code)]
#[path = "../../crates/hoimin-cli/src/analyzer/types.rs"]
mod types;
use types::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};
#[allow(dead_code)]
#[path = "../../crates/hoimin-cli/src/analyzer/rust.rs"]
mod analyzer;

use analyzer::{AnalysisError, AnalyzeRequest, analyze_source_cancellable};
use camino::Utf8Path;
use hoimin_core::{
    CandidateDescriptor, CandidateValidationContext, MutationOperatorSelection, MutationProfile,
    validate_candidate_with_context,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|source: &str| {
    if source.len() > 4096 {
        return;
    }
    let context = CandidateValidationContext::new(source.as_bytes()).unwrap();
    // The pure analyzer receives decoded text. This target uses UTF-8 inputs;
    // non-UTF-8 declarations belong to the source_encoding target.
    if !context
        .decoded_source()
        .is_ok_and(|decoded| decoded.text() == source)
    {
        return;
    }
    let mut operators = MutationOperatorSelection::all_legacy();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.include(operator);
        }
    }
    let request = AnalyzeRequest {
        payload: analyzer::payload::PayloadBudget::default(),
        path: Utf8Path::new("input.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: if source.len().is_multiple_of(2) {
            MutationProfile::Full
        } else {
            MutationProfile::Focused
        },
        max_candidates: 32,
    };
    assert!(matches!(
        analyze_source_cancellable(&request, source, || true),
        Err(AnalysisError::Cancelled)
    ));
    let output = match analyze_source_cancellable(&request, source, || false) {
        Ok(output) => output,
        Err(AnalysisError::DepthExceeded { .. }) => return,
        Err(error) => panic!("unexpected analysis error: {error}"),
    };
    assert!(output.candidates.len() <= request.max_candidates);
    let mut previous = None;
    for candidate in &output.candidates {
        let start = candidate.span.start;
        assert!(previous.is_none_or(|previous| previous <= start));
        previous = Some(start);
        let descriptor = CandidateDescriptor {
            schema_version: 1,
            path: candidate.path.clone(),
            span: candidate.span,
            original: candidate.original.clone(),
            replacement: candidate.replacement.clone(),
            operator: candidate.operator.as_str().to_owned(),
            line: candidate.line,
            column: candidate.column,
            symbol: candidate.symbol.clone(),
            file_hash: context.file_hash().into(),
        };
        validate_candidate_with_context(&context, &descriptor).unwrap();
    }
    // A smaller retention budget must retain the same ordered prefix.
    let smaller = AnalyzeRequest {
        max_candidates: 1,
        ..request
    };
    let prefix = analyze_source_cancellable(&smaller, source, || false).unwrap();
    assert_eq!(
        prefix
            .candidates
            .iter()
            .map(|candidate| &candidate.value)
            .collect::<Vec<_>>(),
        output
            .candidates
            .iter()
            .take(1)
            .map(|candidate| &candidate.value)
            .collect::<Vec<_>>()
    );
    if output.candidates.len() > 1 {
        assert!(prefix.truncated);
    }
});
