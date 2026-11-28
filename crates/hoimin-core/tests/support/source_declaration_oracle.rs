use hoimin_core::{PythonSourceEncoding, SourceEncodingError, decode_python_source};
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reason {
    BomConflict,
    UnsupportedCodec,
    InvalidUtf8,
    NonAscii,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Decoded {
        codec: String,
        text: String,
    },
    Error {
        label: String,
        implicit: bool,
        reason: Reason,
        offset: Option<usize>,
    },
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: Vec<u8>,
    expected: Observation,
}

fn observe_error(error: SourceEncodingError, id: &str) -> Observation {
    let (label, detail, implicit) = match error {
        SourceEncodingError::ImplicitUtf8 { encoding, reason } => (encoding, reason, true),
        SourceEncodingError::Encoding { encoding, reason } => (encoding, reason, false),
    };
    let (reason, offset) = match detail.as_str() {
        "declaration conflicts with UTF-8 BOM" => (Reason::BomConflict, None),
        "unsupported or unknown codec; supported codecs are UTF-8, ASCII and Latin-1" => {
            (Reason::UnsupportedCodec, None)
        }
        detail if detail.starts_with("bytes are not valid UTF-8: ") => (Reason::InvalidUtf8, None),
        detail => {
            let index = detail
                .strip_prefix("non-ASCII byte at original offset ")
                .and_then(|offset| offset.parse().ok())
                .unwrap_or_else(|| panic!("{id}: unexpected encoding diagnostic: {detail}"));
            (Reason::NonAscii, Some(index))
        }
    };
    Observation::Error {
        label,
        implicit,
        reason,
        offset,
    }
}

#[test]
fn source_declaration_and_decoding_match_lean() {
    let corpus = include_str!("../../../../formal/HoiminOracle/corpus/source-declaration.jsonl");
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        let observed = match decode_python_source(&case.source) {
            Ok(decoded) => {
                let codec = match decoded.encoding() {
                    PythonSourceEncoding::Utf8 => "utf8",
                    PythonSourceEncoding::Ascii => "ascii",
                    PythonSourceEncoding::Latin1 => "latin1",
                };
                Observation::Decoded {
                    codec: codec.to_owned(),
                    text: decoded.text().to_owned(),
                }
            }
            Err(error) => observe_error(error, &case.id),
        };
        assert_eq!(observed, case.expected, "{}", case.id);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 360);
}
