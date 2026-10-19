use super::*;

fn descriptor(source: &[u8]) -> CandidateDescriptor {
    CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: "source.py".into(),
        span: ByteSpan {
            start: u64::try_from(source.len() - 5).unwrap(),
            length: 4,
        },
        original: "True".into(),
        replacement: "False".into(),
        operator: "boolean_literal".into(),
        line: 2,
        column: 7,
        symbol: None,
        file_hash: blake3::hash(source).to_hex().to_string(),
    }
}

#[test]
fn unused_context_does_not_materialize_locations() {
    let source = "#é\nflag = True\n";
    VALIDATION_INDEX_BUILDS.with(|builds| builds.set(0));
    let context = CandidateValidationContext::new(source.as_bytes()).unwrap();
    std::hint::black_box(&context);
    assert_eq!(VALIDATION_INDEX_BUILDS.with(Cell::get), 0);
}

#[test]
fn repeated_validation_reuses_one_materialization() {
    let source = "#é\nflag = True\n";
    let candidate = descriptor(source.as_bytes());
    let context = CandidateValidationContext::new(source.as_bytes()).unwrap();
    VALIDATION_INDEX_BUILDS.with(|builds| builds.set(0));
    let first = validate_candidate_with_context(&context, &candidate).unwrap();
    let second = validate_candidate_with_context(&context, &candidate).unwrap();
    assert_eq!(first, second);
    assert_eq!(VALIDATION_INDEX_BUILDS.with(Cell::get), 1);
}

// Each row has a lower-priority invalid location; earlier errors must win and
// must not pay for an index. These exercise the real constructor boundary.
macro_rules! early_error {
    ($name:ident, $edit:expr, $error:expr) => {
        #[test]
        fn $name() {
            let source = "#é\nflag = True\n";
            let mut candidate = descriptor(source.as_bytes());
            candidate.line = 999;
            ($edit)(&mut candidate);
            VALIDATION_INDEX_BUILDS.with(|builds| builds.set(0));
            let context = CandidateValidationContext::new(source.as_bytes()).unwrap();

            let result = validate_candidate_with_context(&context, &candidate);

            assert_eq!(result, Err($error));
            assert_eq!(VALIDATION_INDEX_BUILDS.with(Cell::get), 0);
        }
    };
}
early_error!(
    schema_precedes_location_without_building,
    |c: &mut CandidateDescriptor| {
        c.schema_version = 0;
        c.path = "/absolute".into();
    },
    CandidateValidationError::UnsupportedSchema
);
early_error!(
    path_precedes_hash_without_building,
    |c: &mut CandidateDescriptor| {
        c.path = "/absolute".into();
        c.file_hash.clear();
    },
    CandidateValidationError::InvalidPath
);
early_error!(
    hash_precedes_span_without_building,
    |c: &mut CandidateDescriptor| {
        c.file_hash.clear();
        c.span.start = u64::MAX;
    },
    CandidateValidationError::FileHashMismatch
);
early_error!(
    mutation_precedes_span_without_building,
    |c: &mut CandidateDescriptor| {
        c.operator.clear();
        c.span.start = u64::MAX;
    },
    CandidateValidationError::InvalidMutation
);
early_error!(
    span_precedes_original_without_building,
    |c: &mut CandidateDescriptor| {
        c.span.start = u64::MAX;
        c.original = "wrong".into();
    },
    CandidateValidationError::SpanOutOfBounds
);
early_error!(
    original_precedes_location_without_building,
    |c: &mut CandidateDescriptor| {
        c.original = "wrong".into();
    },
    CandidateValidationError::OriginalMismatch
);

#[test]
fn replacement_encoding_precedes_location_without_building() {
    let source = b"# coding: latin-1\nflag = True\n";
    let mut candidate = descriptor(source);
    candidate.replacement = "😀".into();
    candidate.line = 999;
    VALIDATION_INDEX_BUILDS.with(|builds| builds.set(0));
    let context = CandidateValidationContext::new(source).unwrap();

    let result = validate_candidate_with_context(&context, &candidate);

    assert!(matches!(
        result,
        Err(CandidateValidationError::InvalidEncoding(_))
    ));
    assert_eq!(VALIDATION_INDEX_BUILDS.with(Cell::get), 0);
}

#[test]
fn invalid_decoding_is_retained_without_building() {
    let source = b"#\xff\nflag = True\n";
    let candidate = descriptor(source);
    VALIDATION_INDEX_BUILDS.with(|builds| builds.set(0));
    let context = CandidateValidationContext::new(source).unwrap();

    let result = validate_candidate_with_context(&context, &candidate);

    assert_eq!(result, Err(CandidateValidationError::InvalidUtf8));
    assert_eq!(VALIDATION_INDEX_BUILDS.with(Cell::get), 0);
}

#[test]
fn original_mismatch_precedes_invalid_decoding_without_building() {
    let source = b"#\xff\nflag = True\n";
    let mut candidate = descriptor(source);
    candidate.original = "wrong".into();
    VALIDATION_INDEX_BUILDS.with(|builds| builds.set(0));
    let context = CandidateValidationContext::new(source).unwrap();

    let result = validate_candidate_with_context(&context, &candidate);

    assert_eq!(result, Err(CandidateValidationError::OriginalMismatch));
    assert_eq!(VALIDATION_INDEX_BUILDS.with(Cell::get), 0);
}

#[test]
fn length_check_accepts_the_largest_raw_or_decoded_source() {
    assert_eq!(check_source_length(u32::MAX as usize), Ok(()));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn length_check_rejects_decoded_expansion_beyond_the_raw_limit() {
    let raw_length = u32::MAX as usize / 2 + 1;
    assert_eq!(check_source_length(raw_length), Ok(()));
    assert_eq!(
        check_source_length(raw_length * 2),
        Err(CandidateValidationError::SourceTooLarge)
    );
}
