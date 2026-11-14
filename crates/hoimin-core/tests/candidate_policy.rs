#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use camino::Utf8PathBuf;
use hoimin_core::{
    ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, CandidateIdentity,
    CandidateIdentityRef, CandidateValidationContext, CandidateValidationError, MutationCandidate,
    PythonSourceIndex, python_source_column, stable_mutant_id, validate_candidate,
    validate_candidate_with_context,
};
use proptest::prelude::*;

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

proptest! {
    #[test]
    fn mutation_candidate_validation_ignores_assigned_id_and_sequence(
        id in any::<String>(),
        sequence in any::<u64>(),
    ) {
        // Arrange: bookkeeping can vary without changing the source mutation.
        let source = b"x = 1\nvalue == 2\n";
        let expected_id = validate_candidate(source, &descriptor(source)).unwrap();
        let candidate = MutationCandidate {
            id,
            sequence,
            path: "pkg/calc.py".into(),
            span: ByteSpan { start: 12, length: 2 },
            original: "==".into(),
            replacement: "!=".into(),
            operator: "compare_eq_ne".into(),
            line: 2,
            column: 6,
            symbol: Some("compare".into()),
            file_hash: blake3::hash(source).to_hex().to_string(),
        };

        // Act: recompute identity through the descriptor consumed by validation.
        let actual_id = validate_candidate(source, &CandidateDescriptor::from(&candidate));

        // Assert: assigned identity and discovery order do not affect validation.
        prop_assert_eq!(actual_id, Ok(expected_id));
    }
}

#[test]
fn indexed_source_locations_preserve_repeated_queries_after_later_offsets() {
    let source = "\u{feff}aβ😀\r\nxy\u{301}z\r最後\n";
    let index = PythonSourceIndex::new(source).unwrap();
    let offsets = [3, 4, 6, 10, 14, 16, 18, 24, 4];

    let actual = offsets.map(|offset| index.line_and_column(offset));

    assert_eq!(
        actual,
        [
            Some((1, 0)),
            Some((1, 1)),
            Some((1, 2)),
            Some((1, 3)),
            Some((2, 2)),
            Some((2, 3)),
            Some((3, 0)),
            Some((3, 2)),
            Some((1, 1))
        ]
    );
}

#[rstest::rstest]
#[case::first_scalar(3, Some((1, 0)))]
#[case::second_scalar(4, Some((1, 1)))]
#[case::astral_scalar(6, Some((1, 2)))]
#[case::carriage_return(10, Some((1, 3)))]
#[case::combining_scalar(14, Some((2, 2)))]
#[case::second_line_end(16, Some((2, 3)))]
#[case::third_line_start(18, Some((3, 0)))]
#[case::third_line_end(24, Some((3, 2)))]
#[case::repeated_query(4, Some((1, 1)))]
#[case::inside_bom(1, None)]
#[case::inside_two_byte_scalar(5, None)]
#[case::inside_astral_scalar(7, None)]
#[case::inside_combining_scalar(15, None)]
#[case::past_end(26, None)]
fn indexed_source_locations_preserve_python_columns_for_arbitrary_queries(
    #[case] offset: usize,
    #[case] expected: Option<(u32, u32)>,
) {
    let source = "\u{feff}aβ😀\r\nxy\u{301}z\r最後\n";
    let index = PythonSourceIndex::new(source).unwrap();

    let actual = index.line_and_column(offset);

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case::value("")]
#[case::plain_ascii_next("plain ascii\nnext")]
#[case::u_feff_first_eta_end("\u{feff}first\r\nβeta\r😀 end\n")]
#[case::a_u_feff_b_u_feff_c("a\u{feff}b\n\u{feff}c")]
#[case::e_u_301_and("e\u{301} and 日本語")]
fn indexed_source_locations_match_the_single_query_contract(
    #[case] source: &str,
    #[values(0usize, usize::MAX)] query: usize,
) {
    let index = PythonSourceIndex::new(source).unwrap();
    let starts = hoimin_core::python_line_starts(source.as_bytes()).unwrap();
    let offset = query.min(source.len());
    let line_index = starts.partition_point(|start| *start as usize <= offset) - 1;
    let expected = python_source_column(source, starts[line_index] as usize, offset)
        .map(|column| (u32::try_from(line_index).unwrap() + 1, column));

    let actual = index.line_and_column(offset);

    assert_eq!(actual, expected, "{source:?} at {offset}");
}

macro_rules! indexed_source_query_property {
    ($name:ident, $source:expr) => {
        proptest! {
            #[test]
            fn $name(offset in 0usize..=($source).len()) {
                let source = $source;
                let index = PythonSourceIndex::new(source).unwrap();
                let starts = hoimin_core::python_line_starts(source.as_bytes()).unwrap();
                let line_index = starts.partition_point(|start| *start as usize <= offset) - 1;
                let expected = python_source_column(source, starts[line_index] as usize, offset)
                    .map(|column| (u32::try_from(line_index).unwrap() + 1, column));

                let actual = index.line_and_column(offset);

                prop_assert_eq!(actual, expected, "{} at {}", source, offset);
            }
        }
    };
}

indexed_source_query_property!(generated_query_value, "");
indexed_source_query_property!(generated_query_plain_ascii_next, "plain ascii\nnext");
indexed_source_query_property!(
    generated_query_u_feff_first_eta_end,
    "\u{feff}first\r\nβeta\r😀 end\n"
);
indexed_source_query_property!(generated_query_a_u_feff_b_u_feff_c, "a\u{feff}b\n\u{feff}c");
indexed_source_query_property!(generated_query_e_u_301_and, "e\u{301} and 日本語");

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

#[rstest::rstest]
#[case::pkg_calc_py("/pkg/calc.py")]
#[case::c_pkg_calc_py("C:/pkg/calc.py")]
#[case::pkg_calc_py_3("pkg/../calc.py")]
#[case::pkg_cache_calc_py("pkg:cache/calc.py")]
#[case::pkg_calc_py_stream("pkg/calc.py:stream")]
#[case::pkg_calc_py_6("pkg/calc\0.py")]
fn candidate_paths_reject_absolute_parent_and_colon_components(#[case] invalid: &str) {
    let source = b"x = 1\nvalue == 2\n";

    let mut candidate = descriptor(source);
    candidate.path = Utf8PathBuf::from(invalid);

    assert_eq!(
        validate_candidate(source, &candidate),
        Err(CandidateValidationError::InvalidPath),
        "path should be rejected: {invalid}"
    );
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

#[rstest::rstest]
#[case::file_bom_start("\u{feff}ab\n\u{feff}c\n", 0, 0, Some(0))]
#[case::after_file_bom("\u{feff}ab\n\u{feff}c\n", 0, 3, Some(0))]
#[case::first_line_end("\u{feff}ab\n\u{feff}c\n", 0, 5, Some(2))]
#[case::second_line_start("\u{feff}ab\n\u{feff}c\n", 6, 6, Some(0))]
#[case::second_line_bom_counts("\u{feff}ab\n\u{feff}c\n", 6, 9, Some(1))]
#[case::second_line_end("\u{feff}ab\n\u{feff}c\n", 6, 10, Some(2))]
#[case::ascii("plain", 0, 5, Some(5))]
#[case::unicode("日本x", 0, 6, Some(2))]
#[case::interior_bom("a\u{feff}b", 0, 4, Some(2))]
#[case::second_file_bom_counts("\u{feff}\u{feff}x", 0, 6, Some(1))]
fn python_columns_ignore_exactly_one_leading_file_bom(
    #[case] source: &str,
    #[case] line_start: usize,
    #[case] offset: usize,
    #[case] expected: Option<u32>,
) {
    let actual = python_source_column(source, line_start, offset);

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case::value_4(4, 3)]
#[case::value_0(0, 6)]
#[case::value_1(1, 3)]
#[case::value_0_4(0, 4)]
fn python_columns_reject_invalid_ranges_and_utf8_boundaries(
    #[case] line_start: usize,
    #[case] offset: usize,
) {
    let source = "\u{feff}β";

    assert_eq!(
        python_source_column(source, line_start, offset),
        None,
        "invalid byte range {line_start}..{offset}",
    );
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

#[rstest::rstest]
#[case::candidatedescriptor_schema_version_candidate_schema_version_1_pat_1(0)]
#[case::candidatedescriptor_path_invalid_py_into_file_hash_stale_into_bas_2(1)]
#[case::candidatedescriptor_file_hash_stale_into_operator_string_new_base_3(2)]
#[case::candidatedescriptor_operator_string_new_span_bytespan_start_99_le_4(3)]
#[case::candidatedescriptor_span_bytespan_start_99_length_1_original_wron_5(4)]
#[case::candidatedescriptor_original_z_into_base_clone_6(5)]
#[case::base_7(6)]
fn reusable_context_preserves_validation_error_precedence(#[case] scenario_index: usize) {
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

    let (candidate, expected) = cases.into_iter().nth(scenario_index).unwrap();

    assert_eq!(
        validate_candidate_with_context(&context, &candidate),
        Err(expected.clone())
    );
    assert_eq!(validate_candidate(source, &candidate), Err(expected));
}

#[rstest::rstest]
#[case::x_1_value_2("x = 1\rvalue == 2\r", 2, 6)]
#[case::x_1_value_2_2("x = 1\r\nvalue == 2\r\n", 2, 6)]
#[case::x_1_value_2_3("x = 1\nvalue == 2", 2, 6)]
#[case::header_next_2("# header\r\n\r# next\n値 == 2\r", 4, 2)]
#[case::u_feff_header_2("\u{feff}# header\r\r値 == 2", 3, 2)]
fn validates_python_physical_lines_without_normalizing_source_bytes(
    #[case] source: &str,
    #[case] line: u32,
    #[case] column: u32,
) {
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

#[rstest::rstest]
#[case::value("", &[0])]
#[case::value_2("\r", &[0, 1])]
#[case::value_3("\n", &[0, 1])]
#[case::value_4("\r\n", &[0, 2])]
#[case::value_5("\r\r\n\n", &[0, 1, 3, 4])]
#[case::a_b_c("a\r\nb\rc\n", &[0, 3, 5, 7])]
#[case::a_b("a\rb", &[0, 2])]
fn python_physical_line_starts_count_crlf_once_and_keep_empty_lines(
    #[case] source: &str,
    #[case] expected: &[u32],
) {
    assert_eq!(
        hoimin_core::python_line_starts(source.as_bytes()).unwrap(),
        expected,
        "{source:?}"
    );
}

#[test]
fn borrowed_candidate_identity_preserves_published_id_vector() {
    // Arrange: this checked-in ID predates the borrowed identity projection.
    let candidates: Vec<MutationCandidate> = serde_json::from_str(include_str!(
        "../../hoimin-cli/tests/fixtures/method-replacements/candidates.json"
    ))
    .unwrap();
    let candidate = &candidates[0];
    let expected = "m1_3faeba4770789066e291b3ecd51b9b44ba6301f757a68a2ff7e8bde823ca82ab";
    // Act
    let actual = CandidateIdentityRef::from(candidate).stable_mutant_id();
    // Assert
    assert_eq!(actual.as_str(), expected);
    assert!(CandidateIdentityRef::from(candidate).matches_id(expected));
}

#[test]
fn borrowed_descriptor_identity_preserves_canonical_path_and_hash_case() {
    // Arrange
    let candidates: Vec<MutationCandidate> = serde_json::from_str(include_str!(
        "../../hoimin-cli/tests/fixtures/method-replacements/candidates.json"
    ))
    .unwrap();
    let mut descriptor = CandidateDescriptor::from(&candidates[0]);
    descriptor.path = "./subject.py".into();
    descriptor.file_hash = descriptor.file_hash.to_ascii_uppercase();
    // Act
    let actual = CandidateIdentityRef::from(&descriptor).stable_mutant_id();
    // Assert
    assert_eq!(
        actual.as_str(),
        "m1_3faeba4770789066e291b3ecd51b9b44ba6301f757a68a2ff7e8bde823ca82ab"
    );
}

#[test]
fn borrowed_descriptor_identity_includes_candidate_schema() {
    // Arrange
    let candidates: Vec<MutationCandidate> = serde_json::from_str(include_str!(
        "../../hoimin-cli/tests/fixtures/method-replacements/candidates.json"
    ))
    .unwrap();
    let mut descriptor = CandidateDescriptor::from(&candidates[0]);
    descriptor.schema_version += 1;
    // Act
    let matches = CandidateIdentityRef::from(&descriptor).matches_id(&candidates[0].id);
    // Assert
    assert!(!matches);
}

proptest! {
    #[test]
    fn borrowed_projections_preserve_published_vectors_despite_display_metadata(
        mut candidate in proptest::sample::select(
            serde_json::from_str::<Vec<MutationCandidate>>(include_str!(
                "../../hoimin-cli/tests/fixtures/method-replacements/candidates.json"
            )).unwrap()
        ),
        original in any::<String>(),
        symbol in proptest::option::of(any::<String>()),
        line in any::<u32>(),
        column in any::<u32>(),
        sequence in any::<u64>(),
    ) {
        // Arrange: preserve the published ID and vary every excluded field.
        candidate.original = original;
        candidate.symbol = symbol;
        candidate.line = line;
        candidate.column = column;
        candidate.sequence = sequence;
        let descriptor = CandidateDescriptor::from(&candidate);
        // Act
        let mutation_id = CandidateIdentityRef::from(&candidate).stable_mutant_id();
        let descriptor_id = CandidateIdentityRef::from(&descriptor).stable_mutant_id();
        // Assert: expected IDs were recorded before this implementation.
        prop_assert_eq!(mutation_id.as_str(), candidate.id.as_str());
        prop_assert_eq!(descriptor_id.as_str(), candidate.id.as_str());
    }
}

#[test]
fn production_validation_context_remains_send_and_sync() {
    fn assert_traits<T: Send + Sync>() {}
    assert_traits::<CandidateValidationContext<'static>>();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]
    #[test]
    fn indexed_locations_match_an_independent_scalar_walk(
        chunks in proptest::collection::vec(prop::sample::select(vec!["a", "é", "\u{301}", "😀", "\n", "\r\n", "\r"]), 0..64),
        bom in any::<bool>(),
        query in any::<usize>(),
    ) {
        let source = format!("{}{}", if bom { "\u{feff}" } else { "" }, chunks.concat());
        let mut expected = vec![None; source.len() + 1];
        let (mut line, mut column) = (1, 0);
        let mut scalars = source.char_indices().peekable();
        while let Some((offset, scalar)) = scalars.next() {
            expected[offset] = Some((line, column));
            match scalar {
                '\n' => { line += 1; column = 0; }
                '\r' if !matches!(scalars.peek(), Some((_, '\n'))) => { line += 1; column = 0; }
                '\u{feff}' if offset == 0 => {}
                _ => column += 1,
            }
        }
        expected[source.len()] = Some((line, column));

        let offset = query % expected.len();
        let expected_position = expected[offset];
        let index = PythonSourceIndex::new(&source)?;

        let actual = index.line_and_column(offset);

        prop_assert_eq!(actual, expected_position);
    }
}

#[rstest::rstest]
#[case(b"# coding: ascii\nx = 1\n".as_slice(), "é")]
#[case(b"# coding: latin-1\nx = 1\n".as_slice(), "λ")]
fn unencodable_original_is_an_original_mismatch(#[case] source: &[u8], #[case] original: &str) {
    let mut candidate = descriptor(source);
    candidate.original = original.into();

    let actual = validate_candidate(source, &candidate);

    assert_eq!(actual, Err(CandidateValidationError::OriginalMismatch));
}

#[test]
fn validation_scratch_bound_counts_owned_temporary_text() {
    let source = b"x = 1\nvalue == 2\n";
    let candidate = descriptor(source);
    let expected = candidate.original.len()
        + 2 * candidate.replacement.len()
        + 3 * candidate.path.as_str().len()
        + candidate.operator.len()
        + 2 * candidate.file_hash.len()
        + 67;

    let actual = hoimin_core::candidate_validation_scratch_bytes(&candidate);

    assert_eq!(actual, Some(expected));
}

#[test]
fn displayed_mutant_id_preserves_its_stable_wire_identity() {
    let source = b"x = 1\nvalue == 2\n";
    let id = validate_candidate(source, &descriptor(source)).unwrap();

    let actual = id.to_string();

    assert_eq!(actual, id.as_str());
}

#[test]
fn reusable_context_exposes_decoded_source_without_rebuilding_it() {
    let context = CandidateValidationContext::new(b"# coding: latin-1\n\xe9").unwrap();
    let decoded = context.decoded_source().unwrap();

    let actual = decoded.text();
    let actual_2 = decoded.encoding();

    assert_eq!(actual, "# coding: latin-1\né");
    assert_eq!(actual_2, hoimin_core::PythonSourceEncoding::Latin1);
}
