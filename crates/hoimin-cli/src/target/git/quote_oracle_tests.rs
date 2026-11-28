use super::decode_git_quoted;
use hoimin_core::TargetError;
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Decoded { value: String },
    Error { message: String },
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    input: String,
    expected: Observation,
}

#[test]
fn git_quoted_path_decoding_matches_lean() {
    let corpus = include_str!("../../../../../formal/HoiminOracle/corpus/git-quote.jsonl");
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);

        let actual = match decode_git_quoted(&fixture.input) {
            Ok(value) => Observation::Decoded { value },
            Err(TargetError::GitFailed(message)) => Observation::Error { message },
            Err(error) => panic!("{}: unexpected error: {error}", fixture.id),
        };
        assert_eq!(actual, fixture.expected, "{}", fixture.id);
        assert!(ids.insert(fixture.id));
    }
    assert_eq!(ids.len(), 547);
}
