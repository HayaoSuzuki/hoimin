#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use hoimin_core::{
    ByteSpan, CandidateDescriptor, PythonSourceEncoding, decode_python_source, validate_candidate,
};

#[rstest::rstest]
#[case::value_1(1)]
#[case::value_2(2)]
#[case::value_17(17)]
fn latin1_mapping_matches_scalar_widths_for_every_byte_and_boundary(#[case] stride: usize) {
    let mut source = b"# coding: latin-1\n".to_vec();
    for byte in 0..=u8::MAX {
        source.extend(std::iter::repeat_n(b'a', stride - 1));
        source.push(byte);
    }

    let expected_text = source.iter().copied().map(char::from).collect::<String>();
    let raw_expected = expected_text
        .char_indices()
        .map(|(offset, _)| Some(offset))
        .chain(std::iter::once(Some(expected_text.len())))
        .collect::<Vec<_>>();
    let utf8_expected = (0..=expected_text.len())
        .map(|offset| {
            expected_text
                .is_char_boundary(offset)
                .then(|| expected_text[..offset].chars().count())
        })
        .collect::<Vec<_>>();

    let decoded = decode_python_source(&source).unwrap();
    let raw_actual = (0..=source.len())
        .map(|offset| decoded.raw_to_utf8(offset))
        .collect::<Vec<_>>();
    let utf8_actual = (0..=expected_text.len())
        .map(|offset| decoded.utf8_to_raw(offset))
        .collect::<Vec<_>>();
    let encoded = decoded.encoding().encode(decoded.text()).unwrap();
    let outside = (
        decoded.raw_to_utf8(usize::MAX),
        decoded.utf8_to_raw(usize::MAX),
    );

    assert_eq!(raw_actual, raw_expected);
    assert_eq!(utf8_actual, utf8_expected);
    assert_eq!(outside, (None, None));
    assert_eq!(encoded.as_ref(), source);
}

fn location_candidate(ending: &[u8], query: usize) -> (Vec<u8>, CandidateDescriptor) {
    use hoimin_core::{CandidateValidationContext, PythonSourceIndex};
    let mut source = b"# coding: latin-1\n".to_vec();
    for payload in [b"\xe9\xff\x85\xa0x".as_slice(), b"", b"a\xe9b\xff"] {
        source.extend_from_slice(payload);
        source.extend_from_slice(ending);
    }
    let context = CandidateValidationContext::new(&source).unwrap();
    let decoded = source.iter().copied().map(char::from).collect::<String>();
    let index = PythonSourceIndex::new(&decoded).unwrap();
    let raw = query % (source.len() + 1);
    let utf8 = source[..raw]
        .iter()
        .map(|byte| char::from(*byte).len_utf8())
        .sum();
    let (line, column) = index.line_and_column(utf8).unwrap();
    let candidate = CandidateDescriptor {
        schema_version: 1,
        path: "source.py".into(),
        span: ByteSpan {
            start: raw as u64,
            length: 0,
        },
        original: String::new(),
        replacement: "a".into(),
        operator: "test".into(),
        line,
        column,
        symbol: None,
        file_hash: context.file_hash().into(),
    };
    (source, candidate)
}

#[rstest::rstest]
fn latin1_validation_locations_match_decoded_unicode_index_at_every_boundary(
    #[values(b"\n".as_slice(), b"\r\n", b"\r", b"\r\n\r\n\n\r")] ending: &[u8],
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45
    )]
    query: usize,
) {
    use hoimin_core::{CandidateIdentity, stable_mutant_id};
    let (source, candidate) = location_candidate(ending, query);
    let expected = stable_mutant_id(&CandidateIdentity::from(&candidate));

    let actual = validate_candidate(&source, &candidate);

    assert_eq!(actual, Ok(expected));
}

#[rstest::rstest]
#[case::wrong_line(1, 0)]
#[case::wrong_column(0, 1)]
fn latin1_validation_rejects_incorrect_location(
    #[values(b"\n".as_slice(), b"\r\n", b"\r", b"\r\n\r\n\n\r")] ending: &[u8],
    #[values(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45
    )]
    query: usize,
    #[case] line_delta: u32,
    #[case] column_delta: u32,
) {
    use hoimin_core::CandidateValidationError;
    let (source, mut candidate) = location_candidate(ending, query);
    candidate.line += line_delta;
    candidate.column += column_delta;

    let actual = validate_candidate(&source, &candidate);

    assert_eq!(actual, Err(CandidateValidationError::LocationMismatch));
}

#[test]
fn latin1_candidate_uses_original_bytes_and_unicode_metadata() {
    let source = b"# coding: latin-1\nvalue = ['caf\xe9']\n";
    let candidate = CandidateDescriptor {
        schema_version: 1,
        path: "calc.py".into(),
        span: ByteSpan {
            start: 27,
            length: 6,
        },
        original: "'café'".into(),
        replacement: "'thé'".into(),
        operator: "test".into(),
        line: 2,
        column: 9,
        symbol: None,
        file_hash: blake3::hash(source).to_hex().to_string(),
    };
    assert_eq!(&source[27..33], b"'caf\xe9'");
    let actual = validate_candidate(source, &candidate);

    // pins: issue #480
    assert!(actual.is_ok());
}

#[test]
fn decoding_maps_every_latin1_boundary_and_reencodes_exactly() {
    let source = b"# coding: latin-1\r\n\xe9a\xff\r";
    let expected = source.iter().copied().map(char::from).collect::<String>();
    let raw_expected = expected
        .char_indices()
        .map(|(offset, _)| Some(offset))
        .chain(std::iter::once(Some(expected.len())))
        .collect::<Vec<_>>();
    let utf8_expected = (0..=expected.len())
        .map(|offset| {
            expected
                .is_char_boundary(offset)
                .then(|| expected[..offset].chars().count())
        })
        .collect::<Vec<_>>();

    let decoded = decode_python_source(source).unwrap();
    let raw_actual = (0..=source.len())
        .map(|offset| decoded.raw_to_utf8(offset))
        .collect::<Vec<_>>();
    let utf8_actual = (0..=expected.len())
        .map(|offset| decoded.utf8_to_raw(offset))
        .collect::<Vec<_>>();
    let encoded = decoded.encoding().encode(decoded.text()).unwrap();
    let outside = (
        decoded.raw_to_utf8(source.len() + 1),
        decoded.utf8_to_raw(expected.len() + 1),
    );

    assert_eq!(decoded.encoding(), PythonSourceEncoding::Latin1);
    assert_eq!(decoded.text(), expected);
    assert_eq!(encoded.as_ref(), source);
    assert_eq!(raw_actual, raw_expected);
    assert_eq!(utf8_actual, utf8_expected);
    assert_eq!(outside, (None, None));
}

#[test]
fn latin1_encoding_rejects_unrepresentable_text() {
    let text = "日本語";

    let encoded = PythonSourceEncoding::Latin1.encode(text);

    assert!(encoded.is_err());
}

#[rstest::rstest]
#[case::input_1("# coding: latin-1\n")]
#[case::input_2("#!/usr/bin/python\n# coding=LATIN_1\n")]
#[case::input_3("\n# coding: iso-8859-1\n")]
#[case::input_4("\t\x0c\r# coding: latin1\r")]
#[case::input_5("# explanation coding: latin-1\r\n")]
fn standalone_comment_cookie_selects_latin1(#[case] prefix: &str) {
    let source = [prefix.as_bytes(), b"name = 'caf\xe9'\n"].concat();

    let decoded = decode_python_source(&source).unwrap();

    assert_eq!(decoded.encoding(), PythonSourceEncoding::Latin1);
}

#[rstest::rstest]
#[case::input_1("text = 'coding: latin-1'\n")]
#[case::input_2("value = 1 # coding: latin-1\n")]
#[case::input_3("value = 1\n# coding: latin-1\n")]
#[case::input_4("# first\n# second\n# coding: latin-1\n")]
#[case::input_5("# coding : latin-1\n")]
#[case::input_6("# Coding: latin-1\n")]
fn embedded_or_late_cookie_keeps_utf8(#[case] source: &str) {
    let decoded = decode_python_source(source.as_bytes()).unwrap();

    assert_eq!(decoded.encoding(), PythonSourceEncoding::Utf8);
}

#[rstest::rstest]
#[case::input_1(b"# coding: cp1252\n".as_slice(),
            "cp1252",
            "unsupported or unknown")]
#[case::input_2(b"# coding: not-a-codec\n", "not-a-codec", "unsupported or unknown")]
#[case::input_3(b"# coding: ASCII\nvalue = '\xe9'\n", "ASCII", "non-ASCII")]
#[case::input_4(b"# coding: UTF_8\nvalue = '\xe9'\n", "UTF_8", "not valid UTF-8")]
#[case::input_5(b"value = '\xe9'\n", "utf-8 (default)", "not valid UTF-8")]
#[case::input_6(b"\xef\xbb\xbf# coding: latin-1\n", "latin-1", "BOM")]
#[case::input_7(b"\xef\xbb\xbf# coding: utf8\n", "utf8", "BOM")]
fn encoding_errors_retain_declarations_and_reasons(
    #[case] source: &[u8],
    #[case] name: &str,
    #[case] reason: &str,
) {
    let error = decode_python_source(source).unwrap_err().to_string();

    assert!(error.contains(name), "{error}");
    assert!(error.contains(reason), "{error}");
}

#[rstest::rstest]
#[case::input_1(b"# coding: utf8\n".as_slice())]
#[case::input_2(b"\xef\xbb\xbf# coding: UTF_8\n")]
#[case::input_3(b"# coding: ascii\n")]
fn compatible_cookie_and_bom_preserve_source_bytes(#[case] source: &[u8]) {
    let decoded = decode_python_source(source).unwrap();

    assert_eq!(decoded.text().as_bytes(), source);
}

#[test]
fn utf8_offsets_and_encoding_remain_unchanged() {
    let source = "\u{feff}name = '日本語'; value = True\n";
    let expected = (0..=source.len())
        .map(|offset| source.is_char_boundary(offset).then_some(offset))
        .collect::<Vec<_>>();

    let decoded = decode_python_source(source.as_bytes()).unwrap();
    let raw = (0..=source.len())
        .map(|offset| decoded.raw_to_utf8(offset))
        .collect::<Vec<_>>();
    let utf8 = (0..=source.len())
        .map(|offset| decoded.utf8_to_raw(offset))
        .collect::<Vec<_>>();

    assert_eq!(raw, expected);
    assert_eq!(utf8, expected);
}

const VALIDATION_SOURCE: &[u8] = b"# coding: latin-1\nname = '\xe9'; value = True\n";

fn validation_candidate() -> CandidateDescriptor {
    let source = VALIDATION_SOURCE;
    let start = source
        .windows(4)
        .position(|bytes| bytes == b"True")
        .unwrap();
    CandidateDescriptor {
        schema_version: 1,
        path: "calc.py".into(),
        span: ByteSpan {
            start: start as u64,
            length: 4,
        },
        original: "True".into(),
        replacement: "False".into(),
        operator: "boolean_literal".into(),
        line: 2,
        column: u32::try_from(start - 18).unwrap(),
        symbol: None,
        file_hash: blake3::hash(source).to_hex().to_string(),
    }
}

#[test]
fn candidate_validation_accepts_original_latin1_coordinates() {
    let candidate = validation_candidate();

    let actual = validate_candidate(VALIDATION_SOURCE, &candidate);

    assert!(actual.is_ok());
}

#[test]
fn candidate_validation_rejects_unencodable_replacement() {
    use hoimin_core::CandidateValidationError;
    let candidate = CandidateDescriptor {
        replacement: "日本語".into(),
        ..validation_candidate()
    };

    let actual = validate_candidate(VALIDATION_SOURCE, &candidate);

    assert!(matches!(
        actual,
        Err(CandidateValidationError::InvalidEncoding(_))
    ));
}

#[test]
fn candidate_validation_rejects_decoded_byte_offset() {
    use hoimin_core::CandidateValidationError;
    let candidate = validation_candidate();
    let invalid = CandidateDescriptor {
        span: ByteSpan {
            start: candidate.span.start + 1,
            ..candidate.span
        },
        ..candidate
    };

    let actual = validate_candidate(VALIDATION_SOURCE, &invalid);

    assert_eq!(actual, Err(CandidateValidationError::OriginalMismatch));
}

#[test]
fn candidate_validation_rejects_incorrect_column() {
    use hoimin_core::CandidateValidationError;
    let candidate = validation_candidate();
    let invalid = CandidateDescriptor {
        column: candidate.column + 1,
        ..candidate
    };

    let actual = validate_candidate(VALIDATION_SOURCE, &invalid);

    assert_eq!(actual, Err(CandidateValidationError::LocationMismatch));
}

#[test]
fn ascii_mutation_text_rejects_non_ascii_characters() {
    let error = PythonSourceEncoding::Ascii.encode("é").unwrap_err();

    let actual = error.to_string();

    assert_eq!(
        actual,
        "source encoding 'ascii': mutation text contains a non-ASCII character"
    );
}

#[test]
fn ascii_encoding_preserves_representable_text() {
    let encoded = PythonSourceEncoding::Ascii.encode("value = 1\r\n").unwrap();

    let actual = encoded.as_ref();

    assert_eq!(actual, b"value = 1\r\n");
}

#[test]
fn an_empty_encoding_label_does_not_hide_a_valid_second_line_cookie() {
    let decoded = decode_python_source(b"# coding: \n# coding: latin-1\n\xe9").unwrap();

    let actual = decoded.encoding();
    let actual_2 = decoded.text();

    assert_eq!(actual, PythonSourceEncoding::Latin1);
    assert_eq!(actual_2, "# coding: \n# coding: latin-1\né");
}
