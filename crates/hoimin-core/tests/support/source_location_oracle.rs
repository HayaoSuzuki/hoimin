use hoimin_core::{PythonSourceIndex, python_line_starts};

#[derive(serde::Deserialize)]
struct Query {
    offset: usize,
    location: Option<(u32, u32)>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: String,
    line_starts: Vec<u32>,
    queries: Vec<Query>,
}

#[test]
fn physical_line_and_scalar_column_index_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/source-locations.jsonl"
    ));
    let mut ids = std::collections::BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        let index = PythonSourceIndex::new(&case.source).unwrap();
        assert_eq!(
            python_line_starts(case.source.as_bytes()).unwrap(),
            case.line_starts,
            "{}",
            case.id
        );
        assert_eq!(case.queries.len(), case.source.len() + 5, "{}", case.id);
        for query in case.queries {
            assert_eq!(
                index.line_and_column(query.offset),
                query.location,
                "{} at {}",
                case.id,
                query.offset,
            );
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 230);
}
