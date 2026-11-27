use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationProfile};
use hoimin_python::{
    AnalyzeRequest, analyze_source_cancellable, definition_names, payload::PayloadBudget,
};

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Candidate {
    start: usize,
    original: String,
    replacement: String,
    symbol: Option<String>,
}

#[derive(serde::Deserialize)]
struct Query {
    selectors: Vec<String>,
    candidates: Vec<Candidate>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    path: String,
    source: String,
    definitions: Vec<String>,
    queries: Vec<Query>,
}

#[test]
fn lexical_definitions_and_candidate_symbols_match_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/symbol-catalog.jsonl"
    ));
    let operator = MutationOperator::BooleanLiteral;
    let operators = serde_json::from_value(serde_json::json!([operator.as_str()])).unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        let names: Vec<_> = definition_names(&case.source)
            .unwrap_or_else(|error| panic!("{}: {error}", case.id))
            .into_iter()
            .collect();
        assert_eq!(names, case.definitions, "{}", case.id);
        assert!(!case.queries.is_empty(), "{}", case.id);
        for query in &case.queries {
            let request = AnalyzeRequest {
                payload: PayloadBudget::default(),
                path: Utf8Path::new(&case.path),
                lines: &[],
                symbols: &query.selectors,
                operators: &operators,
                profile: MutationProfile::Full,
                max_candidates: 100,
            };
            let output = analyze_source_cancellable(&request, &case.source, || false).unwrap();
            assert!(!output.truncated, "{}", case.id);
            assert!(
                output.diagnostics.is_empty(),
                "{}: {:?}",
                case.id,
                output.diagnostics
            );
            let actual: Vec<_> = output
                .candidates
                .iter()
                .map(|candidate| {
                    assert_eq!(candidate.operator, operator, "{}", case.id);
                    let start = usize::try_from(candidate.span.start).unwrap();
                    let length = usize::try_from(candidate.span.length).unwrap();
                    assert_eq!(
                        case.source.get(start..start + length),
                        Some(candidate.original.as_str())
                    );
                    Candidate {
                        start,
                        original: candidate.original.clone(),
                        replacement: candidate.replacement.clone(),
                        symbol: candidate.symbol.clone(),
                    }
                })
                .collect();
            assert_eq!(
                actual, query.candidates,
                "{}: {:?}",
                case.id, query.selectors
            );
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 21);
}
