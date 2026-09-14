use hoimin_core::{ByteSpan, CandidateDescriptor, validate_candidate};

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
    // pins: issue #480
    assert!(validate_candidate(source, &candidate).is_ok());
}

#[test]
fn decoding_maps_every_latin1_boundary_and_reencodes_exactly() {
    use hoimin_core::{PythonSourceEncoding, decode_python_source};
    let source = b"# coding: latin-1\r\n\xe9a\xff\r";
    let decoded = decode_python_source(source).unwrap();
    assert_eq!(decoded.encoding(), PythonSourceEncoding::Latin1);
    assert_eq!(
        decoded.encoding().encode(decoded.text()).unwrap().as_ref(),
        source
    );
    let expected = source.iter().copied().map(char::from).collect::<String>();
    assert_eq!(decoded.text(), expected);
    for raw in 0..=source.len() {
        let utf8 = source[..raw]
            .iter()
            .map(|byte| char::from(*byte).len_utf8())
            .sum();
        assert_eq!(decoded.raw_to_utf8(raw), Some(utf8));
        assert_eq!(decoded.utf8_to_raw(utf8), Some(raw));
    }
    for offset in 0..=expected.len() {
        if !expected.is_char_boundary(offset) {
            assert_eq!(decoded.utf8_to_raw(offset), None);
        }
    }
    assert_eq!(decoded.raw_to_utf8(source.len() + 1), None);
    assert_eq!(decoded.utf8_to_raw(expected.len() + 1), None);
    assert!(decoded.encoding().encode("日本語").is_err());
}

#[test]
fn cookies_follow_physical_lines_and_standalone_comment_rules() {
    use hoimin_core::{PythonSourceEncoding, decode_python_source};
    for prefix in [
        "# coding: latin-1\n",
        "#!/usr/bin/python\n# coding=LATIN_1\n",
        "\n# coding: iso-8859-1\n",
        "\t\x0c\r# coding: latin1\r",
        "# explanation coding: latin-1\r\n",
    ] {
        let source = [prefix.as_bytes(), b"name = 'caf\xe9'\n"].concat();
        assert_eq!(
            decode_python_source(&source).unwrap().encoding(),
            PythonSourceEncoding::Latin1
        );
    }
    for source in [
        "text = 'coding: latin-1'\n",
        "value = 1 # coding: latin-1\n",
        "value = 1\n# coding: latin-1\n",
        "# first\n# second\n# coding: latin-1\n",
        "# coding : latin-1\n",
        "# Coding: latin-1\n",
    ] {
        assert_eq!(
            decode_python_source(source.as_bytes()).unwrap().encoding(),
            PythonSourceEncoding::Utf8
        );
    }
}

#[test]
fn errors_retain_declarations_and_bom_requires_tokenizer_utf8_name() {
    use hoimin_core::decode_python_source;
    for (source, name, reason) in [
        (
            b"# coding: cp1252\n".as_slice(),
            "cp1252",
            "unsupported or unknown",
        ),
        (
            b"# coding: not-a-codec\n",
            "not-a-codec",
            "unsupported or unknown",
        ),
        (b"# coding: ASCII\nvalue = '\xe9'\n", "ASCII", "non-ASCII"),
        (
            b"# coding: UTF_8\nvalue = '\xe9'\n",
            "UTF_8",
            "not valid UTF-8",
        ),
        (b"value = '\xe9'\n", "utf-8 (default)", "not valid UTF-8"),
        (b"\xef\xbb\xbf# coding: latin-1\n", "latin-1", "BOM"),
        (b"\xef\xbb\xbf# coding: utf8\n", "utf8", "BOM"),
    ] {
        let error = decode_python_source(source).unwrap_err().to_string();
        assert!(error.contains(name), "{error}");
        assert!(error.contains(reason), "{error}");
    }
    for source in [
        b"# coding: utf8\n".as_slice(),
        b"\xef\xbb\xbf# coding: UTF_8\n",
        b"# coding: ascii\n",
    ] {
        assert_eq!(
            decode_python_source(source).unwrap().text().as_bytes(),
            source
        );
    }
}

#[test]
fn utf8_offsets_and_encoding_remain_unchanged() {
    use hoimin_core::decode_python_source;
    let source = "\u{feff}name = '日本語'; value = True\n";
    let decoded = decode_python_source(source.as_bytes()).unwrap();
    for offset in 0..=source.len() {
        let expected = source.is_char_boundary(offset).then_some(offset);
        assert_eq!(decoded.utf8_to_raw(offset), expected);
        assert_eq!(decoded.raw_to_utf8(offset), expected);
    }
}

#[test]
fn candidate_validation_rejects_unencodable_replacements_and_decoded_offsets() {
    use hoimin_core::CandidateValidationError;
    let source = b"# coding: latin-1\nname = '\xe9'; value = True\n";
    let start = source
        .windows(4)
        .position(|bytes| bytes == b"True")
        .unwrap();
    let candidate = CandidateDescriptor {
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
    };
    assert!(validate_candidate(source, &candidate).is_ok());
    let invalid = CandidateDescriptor {
        replacement: "日本語".into(),
        ..candidate.clone()
    };
    assert!(matches!(
        validate_candidate(source, &invalid),
        Err(CandidateValidationError::InvalidEncoding(_))
    ));
    let decoded_span = CandidateDescriptor {
        span: ByteSpan {
            start: candidate.span.start + 1,
            ..candidate.span
        },
        ..candidate.clone()
    };
    assert_eq!(
        validate_candidate(source, &decoded_span),
        Err(CandidateValidationError::OriginalMismatch)
    );
    let wrong_column = CandidateDescriptor {
        column: candidate.column + 1,
        ..candidate
    };
    assert_eq!(
        validate_candidate(source, &wrong_column),
        Err(CandidateValidationError::LocationMismatch)
    );
}
