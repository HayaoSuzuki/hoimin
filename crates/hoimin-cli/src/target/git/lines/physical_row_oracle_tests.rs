use super::{physical_line_count, translate};
use hoimin_core::{LineRange, TargetError};
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Ok { ranges: Vec<LineRange> },
    Error { message: String },
}

#[derive(serde::Deserialize)]
struct Query {
    git_ranges: Vec<LineRange>,
    expected: Observation,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: Vec<u8>,
    physical_count: u32,
    queries: Vec<Query>,
}

#[test]
fn git_physical_row_translation_matches_lean() {
    let mut ids = BTreeSet::new();
    let mut query_count = 0;
    let corpus =
        include_str!("../../../../../../formal/HoiminOracle/corpus/git-physical-rows.jsonl");
    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        assert_eq!(
            physical_line_count(&fixture.source).unwrap(),
            fixture.physical_count,
            "{}",
            fixture.id
        );
        query_count += fixture.queries.len();
        for query in fixture.queries {
            let actual = match translate(&fixture.source, &query.git_ranges) {
                Ok(ranges) => Observation::Ok { ranges },
                Err(TargetError::GitFailed(message)) => Observation::Error { message },
                Err(error) => panic!("{}: unexpected error: {error}", fixture.id),
            };
            assert_eq!(
                actual, query.expected,
                "{}: {:?}",
                fixture.id, query.git_ranges
            );
        }
        assert!(ids.insert(fixture.id));
    }
    assert_eq!(ids.len(), 374);
    assert_eq!(query_count, 8139);
}
