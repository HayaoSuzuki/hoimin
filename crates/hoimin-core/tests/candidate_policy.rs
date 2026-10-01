use camino::Utf8PathBuf;
use hoimin_core::{
    ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, CandidateIdentity,
    CandidateValidationContext, CandidateValidationError, PythonSourceIndex, python_source_column,
    stable_mutant_id, validate_candidate, validate_candidate_with_context,
};

fn descriptor(source: &[u8]) -> CandidateDescriptor {
    CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: Utf8PathBuf::from("pkg/calc.py"),
        span: ByteSpan {
            start: 12,
            length: 2,
        },
        original: "==".into(),
        replacement: "!=".into(),
        operator: "compare_eq_ne".into(),
        line: 2,
        column: 6,
        symbol: Some("compare".into()),
        file_hash: blake3::hash(source).to_hex().to_string(),
    }
}

#[test]
fn indexed_source_locations_preserve_python_columns_for_arbitrary_queries() {
    let source = "\u{feff}aβ😀\r\nxy\u{301}z\r最後\n";
    let index = PythonSourceIndex::new(source).unwrap();

    for (offset, expected) in [
        (3, Some((1, 0))),
        (4, Some((1, 1))),
        (6, Some((1, 2))),
        (10, Some((1, 3))),
        (14, Some((2, 2))),
        (16, Some((2, 3))),
        (18, Some((3, 0))),
        (24, Some((3, 2))),
        (4, Some((1, 1))),
    ] {
        assert_eq!(index.line_and_column(offset), expected, "offset {offset}");
    }

    for offset in [1, 5, 7, 15, source.len() + 1] {
        assert_eq!(index.line_and_column(offset), None, "offset {offset}");
    }
}

#[test]
fn indexed_source_locations_match_the_single_query_contract() {
    for source in [
        "",
        "plain ascii\nnext",
        "\u{feff}first\r\nβeta\r😀 end\n",
        "a\u{feff}b\n\u{feff}c",
        "e\u{301} and 日本語",
    ] {
        let index = PythonSourceIndex::new(source).unwrap();
        let starts = hoimin_core::python_line_starts(source.as_bytes()).unwrap();
        for offset in 0..=source.len() {
            let line_index = starts.partition_point(|start| *start as usize <= offset) - 1;
            let expected = python_source_column(source, starts[line_index] as usize, offset)
                .map(|column| (u32::try_from(line_index).unwrap() + 1, column));
            assert_eq!(
                index.line_and_column(offset),
                expected,
                "{source:?} at {offset}"
            );
        }
    }
}

#[test]
fn candidate_id_is_stable_across_runs_and_root_locations() {
    let source = b"x = 1\nvalue == 2\n";
    let left = CandidateIdentity::from(&descriptor(source));
    let right = CandidateIdentity {
        path: Utf8PathBuf::from(r"pkg\calc.py"),
        ..left.clone()
    };
    assert_eq!(stable_mutant_id(&left), stable_mutant_id(&right));
    assert!(stable_mutant_id(&left).as_str().starts_with("m1_"));
}

#[test]
fn candidate_identity_has_unambiguous_field_framing() {
    let source = b"x = 1\nvalue == 2\n";
    let left = CandidateIdentity::from(&descriptor(source));
    let mut right = left.clone();
    right.operator = "compare_eq_n".into();
    right.replacement = "e!=".into();
    assert_ne!(stable_mutant_id(&left), stable_mutant_id(&right));
}

#[test]
fn validates_exact_hash_span_path_and_line_metadata() {
    let source = b"x = 1\nvalue == 2\n";
    let candidate = descriptor(source);
    assert_eq!(
        validate_candidate(source, &candidate).unwrap(),
        stable_mutant_id(&CandidateIdentity::from(&candidate))
    );
}

#[test]
fn rejects_span_original_mismatch() {
    let source = b"x = 1\nvalue == 2\n";
    let mut candidate = descriptor(source);
    candidate.original = ">=".into();
    assert_eq!(
        validate_candidate(source, &candidate),
        Err(CandidateValidationError::OriginalMismatch)
    );
}

#[test]
fn rejects_overflowing_span_without_panicking() {
    let source = b"x = 1\n";
    let mut candidate = descriptor(source);
    candidate.span.start = u64::MAX;
    candidate.file_hash = blake3::hash(source).to_hex().to_string();
    assert_eq!(
        validate_candidate(source, &candidate),
        Err(CandidateValidationError::SpanOutOfBounds)
    );
}

#[test]
fn rejects_wrong_line_metadata_and_non_normalized_path() {
    let source = b"x = 1\nvalue == 2\n";
    let mut line = descriptor(source);
    line.line = 1;
    assert_eq!(
        validate_candidate(source, &line),
        Err(CandidateValidationError::LocationMismatch)
    );
    let mut path = descriptor(source);
    path.path = Utf8PathBuf::from("pkg/../calc.py");
    assert_eq!(
        validate_candidate(source, &path),
        Err(CandidateValidationError::InvalidPath)
    );
}

#[test]
fn candidate_paths_reject_absolute_parent_and_colon_components() {
    let source = b"x = 1\nvalue == 2\n";
    for invalid in [
        "/pkg/calc.py",
        "C:/pkg/calc.py",
        "pkg/../calc.py",
        "pkg:cache/calc.py",
        "pkg/calc.py:stream",
        "pkg/calc\0.py",
    ] {
        let mut candidate = descriptor(source);
        candidate.path = Utf8PathBuf::from(invalid);
        assert_eq!(
            validate_candidate(source, &candidate),
            Err(CandidateValidationError::InvalidPath),
            "path should be rejected: {invalid}"
        );
    }
}

#[test]
fn stable_id_normalizes_only_harmless_path_variants() {
    let source = b"x = 1\nvalue == 2\n";
    let canonical = CandidateIdentity::from(&descriptor(source));
    let duplicate = CandidateIdentity {
        path: Utf8PathBuf::from("pkg//calc.py"),
        ..canonical.clone()
    };
    let dotted = CandidateIdentity {
        path: Utf8PathBuf::from("./pkg/calc.py"),
        ..canonical.clone()
    };
    let parent = CandidateIdentity {
        path: Utf8PathBuf::from("pkg/../calc.py"),
        ..canonical.clone()
    };
    let absolute = CandidateIdentity {
        path: Utf8PathBuf::from("/pkg/calc.py"),
        ..canonical.clone()
    };

    assert_eq!(stable_mutant_id(&canonical), stable_mutant_id(&duplicate));
    assert_eq!(stable_mutant_id(&canonical), stable_mutant_id(&dotted));
    assert_ne!(stable_mutant_id(&canonical), stable_mutant_id(&parent));
    assert_ne!(stable_mutant_id(&canonical), stable_mutant_id(&absolute));
}

#[test]
fn reusable_context_matches_strict_validation_for_multiple_locations() {
    let source = b"x = 1\nvalue == 2\nother != 3\n";
    let context = CandidateValidationContext::new(source).unwrap();
    assert_eq!(context.file_hash(), blake3::hash(source).to_hex().as_str());

    let first = descriptor(source);
    let second = CandidateDescriptor {
        span: ByteSpan {
            start: 23,
            length: 2,
        },
        original: "!=".into(),
        replacement: "==".into(),
        line: 3,
        column: 6,
        ..descriptor(source)
    };

    for candidate in [&first, &second] {
        assert_eq!(
            validate_candidate_with_context(&context, candidate),
            validate_candidate(source, candidate)
        );
    }
}

#[test]
fn reusable_context_preserves_unicode_and_crlf_location_semantics() {
    let source = "head\r\nβeta == 2\n".as_bytes();
    let candidate = CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: "pkg/unicode.py".into(),
        span: ByteSpan {
            start: 12,
            length: 2,
        },
        original: "==".into(),
        replacement: "!=".into(),
        operator: "compare_eq_ne".into(),
        line: 2,
        column: 5,
        symbol: None,
        file_hash: blake3::hash(source).to_hex().to_string(),
    };
    let context = CandidateValidationContext::new(source).unwrap();

    assert_eq!(
        validate_candidate_with_context(&context, &candidate),
        validate_candidate(source, &candidate)
    );

    let mut stale_location = candidate;
    stale_location.column = 6;
    assert_eq!(
        validate_candidate_with_context(&context, &stale_location),
        Err(CandidateValidationError::LocationMismatch)
    );
}

#[test]
fn python_columns_ignore_exactly_one_leading_file_bom() {
    let source = "\u{feff}ab\n\u{feff}c\n";

    for (line_start, offset, expected) in [
        (0, 0, Some(0)),
        (0, 3, Some(0)),
        (0, 5, Some(2)),
        (6, 6, Some(0)),
        (6, 9, Some(1)),
        (6, 10, Some(2)),
    ] {
        assert_eq!(
            python_source_column(source, line_start, offset),
            expected,
            "column for byte range {line_start}..{offset}",
        );
    }

    assert_eq!(python_source_column("plain", 0, 5), Some(5));
    assert_eq!(python_source_column("日本x", 0, 6), Some(2));
    assert_eq!(python_source_column("a\u{feff}b", 0, 4), Some(2));
    assert_eq!(python_source_column("\u{feff}\u{feff}x", 0, 6), Some(1));
}

#[test]
fn python_columns_reject_invalid_ranges_and_utf8_boundaries() {
    let source = "\u{feff}β";

    for (line_start, offset) in [(4, 3), (0, 6), (1, 3), (0, 4)] {
        assert_eq!(
            python_source_column(source, line_start, offset),
            None,
            "invalid byte range {line_start}..{offset}",
        );
    }
}

#[test]
fn validates_bom_adjusted_first_line_without_changing_identity_inputs() {
    let source = "\u{feff}value == 2\n".as_bytes();
    let candidate = CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: "pkg/bom.py".into(),
        span: ByteSpan {
            start: 9,
            length: 2,
        },
        original: "==".into(),
        replacement: "!=".into(),
        operator: "compare_eq_ne".into(),
        line: 1,
        column: 6,
        symbol: None,
        file_hash: blake3::hash(source).to_hex().to_string(),
    };
    let expected_identity = CandidateIdentity::from(&candidate);

    assert_eq!(
        validate_candidate(source, &candidate).unwrap(),
        stable_mutant_id(&expected_identity),
    );
    assert_eq!(candidate.span, expected_identity.span);
    assert_eq!(candidate.file_hash, expected_identity.file_hash);

    let mut bom_counted = candidate;
    bom_counted.column = 7;
    assert_eq!(
        validate_candidate(source, &bom_counted),
        Err(CandidateValidationError::LocationMismatch),
    );
}

#[test]
fn reusable_context_preserves_validation_error_precedence() {
    let source = b"x\xff";
    let valid_hash = blake3::hash(source).to_hex().to_string();
    let base = CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: "pkg/invalid.py".into(),
        span: ByteSpan {
            start: 0,
            length: 1,
        },
        original: "x".into(),
        replacement: "y".into(),
        operator: "replace".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: valid_hash.clone(),
    };
    let context = CandidateValidationContext::new(source).unwrap();

    let cases = [
        (
            CandidateDescriptor {
                schema_version: CANDIDATE_SCHEMA_VERSION + 1,
                path: "../invalid.py".into(),
                file_hash: "stale".into(),
                ..base.clone()
            },
            CandidateValidationError::UnsupportedSchema,
        ),
        (
            CandidateDescriptor {
                path: "../invalid.py".into(),
                file_hash: "stale".into(),
                ..base.clone()
            },
            CandidateValidationError::InvalidPath,
        ),
        (
            CandidateDescriptor {
                file_hash: "stale".into(),
                operator: String::new(),
                ..base.clone()
            },
            CandidateValidationError::FileHashMismatch,
        ),
        (
            CandidateDescriptor {
                operator: String::new(),
                span: ByteSpan {
                    start: 99,
                    length: 1,
                },
                ..base.clone()
            },
            CandidateValidationError::InvalidMutation,
        ),
        (
            CandidateDescriptor {
                span: ByteSpan {
                    start: 99,
                    length: 1,
                },
                original: "wrong".into(),
                ..base.clone()
            },
            CandidateValidationError::SpanOutOfBounds,
        ),
        (
            CandidateDescriptor {
                original: "z".into(),
                ..base.clone()
            },
            CandidateValidationError::OriginalMismatch,
        ),
        (base, CandidateValidationError::InvalidUtf8),
    ];

    for (candidate, expected) in cases {
        assert_eq!(
            validate_candidate_with_context(&context, &candidate),
            Err(expected.clone())
        );
        assert_eq!(validate_candidate(source, &candidate), Err(expected));
    }
}

#[test]
fn validates_python_physical_lines_without_normalizing_source_bytes() {
    for (source, line, column) in [
        ("x = 1\rvalue == 2\r", 2, 6),
        ("x = 1\r\nvalue == 2\r\n", 2, 6),
        ("x = 1\nvalue == 2", 2, 6),
        ("# header\r\n\r# next\n値 == 2\r", 4, 2),
        ("\u{feff}# header\r\r値 == 2", 3, 2),
    ] {
        let mut candidate = descriptor(source.as_bytes());
        candidate.span.start = u64::try_from(source.find("==").unwrap()).unwrap();
        candidate.line = line;
        candidate.column = column;
        let expected_id = stable_mutant_id(&CandidateIdentity::from(&candidate));
        // pins: issue #455 — a lone CR is a physical newline, not a column character.
        assert_eq!(
            validate_candidate(source.as_bytes(), &candidate),
            Ok(expected_id.clone()),
            "{source:?}"
        );
        candidate.line = 1;
        assert_eq!(
            stable_mutant_id(&CandidateIdentity::from(&candidate)),
            expected_id
        );
        assert_eq!(
            validate_candidate(source.as_bytes(), &candidate),
            Err(CandidateValidationError::LocationMismatch)
        );
    }
}

#[test]
fn python_physical_line_starts_count_crlf_once_and_keep_empty_lines() {
    for (source, expected) in [
        ("", vec![0]),
        ("\r", vec![0, 1]),
        ("\n", vec![0, 1]),
        ("\r\n", vec![0, 2]),
        ("\r\r\n\n", vec![0, 1, 3, 4]),
        ("a\r\nb\rc\n", vec![0, 3, 5, 7]),
        ("a\rb", vec![0, 2]),
    ] {
        assert_eq!(
            hoimin_core::python_line_starts(source.as_bytes()).unwrap(),
            expected,
            "{source:?}"
        );
    }
}
