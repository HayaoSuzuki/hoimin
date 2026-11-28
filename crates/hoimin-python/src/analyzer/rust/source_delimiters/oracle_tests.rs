use super::DelimiterDepth;
use ruff_python_parser::parse_module;
use ruff_text_size::{TextRange, TextSize};

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: String,
    bucket_counts: [usize; 8],
    queries: Vec<Query>,
}

#[derive(Debug, serde::Deserialize)]
struct Query {
    added: usize,
    start: u32,
    end: u32,
    can_enclose: bool,
    room_before: bool,
}

#[test]
fn sparse_delimiter_admission_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/delimiter-depth.jsonl"
    ));
    let mut ids = std::collections::BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        let parsed =
            parse_module(&case.source).unwrap_or_else(|error| panic!("{}: {error}", case.id));
        let depth = DelimiterDepth::build(parsed.tokens(), &|| false).unwrap();
        assert_eq!(
            depth.forbidden.each_ref().map(Vec::len),
            case.bucket_counts,
            "{}",
            case.id
        );
        assert!(!case.queries.is_empty(), "{}", case.id);
        for query in case.queries {
            let start = TextSize::new(query.start);
            let end = TextSize::new(query.end);
            assert_eq!(
                (
                    depth.can_enclose(TextRange::new(start, end), query.added),
                    depth.room_before(start, query.added)
                ),
                (query.can_enclose, query.room_before),
                "{}: {query:?}",
                case.id,
            );
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 15);
}
