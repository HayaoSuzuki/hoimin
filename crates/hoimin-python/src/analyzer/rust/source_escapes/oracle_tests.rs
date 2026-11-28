use super::{Decoded, decode};
use ruff_python_parser::{LexicalErrorType, ParseErrorType, parse_module};

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../formal/HoiminOracle/corpus/source-escape.jsonl"
));

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Rejected,
    Character,
    Preserved,
    Continuation,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Observation {
    kind: Kind,
    value: Option<char>,
    next: usize,
    lookup: Option<String>,
}

#[derive(serde::Deserialize)]
struct Name {
    name: String,
    value: char,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: String,
    start: usize,
    bytes: bool,
    provider: Option<Name>,
    expected: Observation,
    known_actual: Option<Observation>,
}

impl Fixture {
    fn observe(&self) -> Observation {
        let mut next = self.start;
        let mut lookup = None;
        let result = decode(&self.source, &mut next, self.bytes, |name| {
            lookup = Some(name.to_owned());
            self.provider
                .as_ref()
                .filter(|known| known.name == name)
                .map(|known| known.value)
        });
        let (kind, value) = match result {
            Some(Decoded::Character(value)) => (Kind::Character, Some(value)),
            Some(Decoded::Preserved(value)) => (Kind::Preserved, Some(value)),
            Some(Decoded::Continuation) => (Kind::Continuation, None),
            None => (Kind::Rejected, None),
        };
        Observation {
            kind,
            value,
            next,
            lookup,
        }
    }
}

#[test]
fn escape_observations_match_lean() {
    let mut ids = std::collections::BTreeSet::new();
    for row in CORPUS.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        if case.known_actual.is_none() {
            assert_eq!(case.observe(), case.expected, "{}", case.id);
            assert!(ids.insert(case.id));
        }
    }
    assert_eq!(ids.len(), 121);
}

#[test]
fn parser_rejects_the_signed_hex_spellings_accepted_by_the_internal_decoder() {
    let assert_invalid_escape = |source: String| {
        let error = parse_module(&source).expect_err(&source);
        assert_eq!(
            error.error,
            ParseErrorType::Lexical(LexicalErrorType::UnicodeError),
            "{source:?}"
        );
    };
    let mut ids = std::collections::BTreeSet::new();
    for row in CORPUS.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        if let Some(known_actual) = &case.known_actual {
            let observed = case.observe();
            assert_ne!(
                observed, case.expected,
                "{}: promote fixed fixture",
                case.id
            );
            assert_eq!(&observed, known_actual, "{}", case.id);
            let marker = if case.bytes { "b" } else { "" };
            assert_invalid_escape(format!("value = {marker}\"{}\"\n", case.source));
            if !case.bytes {
                assert_invalid_escape(format!("value = f\"{{1:{}}}\"\n", case.source));
            }
            assert!(ids.insert(case.id));
        }
    }
    assert_eq!(ids.len(), 4);
}
