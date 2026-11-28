use hoimin_core::{PythonSourceEncoding, decode_python_source};
use std::collections::BTreeSet;

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: Vec<u8>,
    codec: String,
    text: String,
    raw_to_utf8: Vec<Option<usize>>,
    utf8_to_raw: Vec<Option<usize>>,
}

#[test]
fn source_coordinate_boundaries_match_lean() {
    let corpus = include_str!("../../../../formal/HoiminOracle/corpus/source-coordinates.jsonl");
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);

        let decoded = decode_python_source(&case.source)
            .unwrap_or_else(|error| panic!("{}: {error}", case.id));
        let codec = match decoded.encoding() {
            PythonSourceEncoding::Latin1 => "latin1",
            PythonSourceEncoding::Utf8 => "utf8",
            PythonSourceEncoding::Ascii => "ascii",
        };
        assert_eq!(codec, case.codec, "{}", case.id);
        assert_eq!(decoded.text(), case.text, "{}", case.id);
        assert_eq!(
            decoded.encoding().encode(decoded.text()).unwrap().as_ref(),
            case.source,
            "{}",
            case.id,
        );

        assert_eq!(case.raw_to_utf8.len(), case.source.len() + 2, "{}", case.id);
        for (offset, expected) in case.raw_to_utf8.into_iter().enumerate() {
            assert_eq!(
                decoded.raw_to_utf8(offset),
                expected,
                "{} raw offset {offset}",
                case.id
            );
        }
        assert_eq!(case.utf8_to_raw.len(), case.text.len() + 2, "{}", case.id);
        for (offset, expected) in case.utf8_to_raw.into_iter().enumerate() {
            assert_eq!(
                decoded.utf8_to_raw(offset),
                expected,
                "{} UTF-8 offset {offset}",
                case.id
            );
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 154);
}
