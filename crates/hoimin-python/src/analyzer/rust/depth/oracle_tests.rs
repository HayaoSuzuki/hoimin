use super::{AnalysisError, CheckedParseError, dispose, parse_checked};
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResultKind {
    Accepted,
    Syntax,
    Cancelled,
    Depth,
}

#[derive(Debug, serde::Deserialize)]
struct Query {
    cancel_at: Option<usize>,
    result: ResultKind,
    polls: usize,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: String,
    queries: Vec<Query>,
}

#[test]
fn checked_ast_admission_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/ast-depth.jsonl"
    ));
    let mut ids = std::collections::BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(!case.queries.is_empty(), "{}", case.id);
        for query in &case.queries {
            let polls = Cell::new(0);
            let result = parse_checked(&case.source, &|| {
                let current = polls.get() + 1;
                polls.set(current);
                query.cancel_at == Some(current)
            });
            let observed = match result {
                Ok(parsed) => {
                    assert!(parsed.has_valid_syntax(), "{}", case.id);
                    dispose(parsed.into_syntax());
                    ResultKind::Accepted
                }
                Err(CheckedParseError::InvalidSyntax) => ResultKind::Syntax,
                Err(CheckedParseError::Analysis(AnalysisError::DepthExceeded { limit })) => {
                    assert_eq!(limit, 128, "{}", case.id);
                    ResultKind::Depth
                }
                Err(CheckedParseError::Analysis(AnalysisError::Cancelled)) => ResultKind::Cancelled,
                Err(error) => panic!("{}: {error:?}", case.id),
            };
            assert_eq!(
                (observed, polls.get()),
                (query.result, query.polls),
                "{}: {query:?}",
                case.id
            );
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 40);
}
