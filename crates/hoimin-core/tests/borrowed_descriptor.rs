use hoimin_core::{
    ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, CandidateDescriptorRef,
    CandidateIdentityRef, CandidateValidationContext, CandidateValidationError, MutationCandidate,
    validate_candidate_with_context,
};

type DescriptorEdit = fn(&mut CandidateDescriptor);

fn descriptor() -> CandidateDescriptor {
    CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: "pkg/calc.py".into(),
        span: ByteSpan {
            start: 6,
            length: 2,
        },
        original: "==".into(),
        replacement: "!=".into(),
        operator: "compare_eq_ne".into(),
        line: 1,
        column: 6,
        symbol: Some("compare".into()),
        file_hash: blake3::hash(b"value == 2\n").to_hex().to_string(),
    }
}

fn mutation(descriptor: &CandidateDescriptor) -> MutationCandidate {
    MutationCandidate {
        id: "assigned bookkeeping".into(),
        sequence: 77,
        path: descriptor.path.clone(),
        span: descriptor.span,
        original: descriptor.original.clone(),
        replacement: descriptor.replacement.clone(),
        operator: descriptor.operator.clone(),
        line: descriptor.line,
        column: descriptor.column,
        symbol: descriptor.symbol.clone(),
        file_hash: descriptor.file_hash.clone(),
    }
}

#[rstest::rstest]
#[case::schema(|row: &mut CandidateDescriptor| row.schema_version += 1)]
#[case::path(|row: &mut CandidateDescriptor| row.path = "other.py".into())]
#[case::span_start(|row: &mut CandidateDescriptor| row.span.start += 1)]
#[case::span_length(|row: &mut CandidateDescriptor| row.span.length += 1)]
#[case::original(|row: &mut CandidateDescriptor| row.original.push('x'))]
#[case::replacement(|row: &mut CandidateDescriptor| row.replacement.push('x'))]
#[case::operator(|row: &mut CandidateDescriptor| row.operator.push('x'))]
#[case::line(|row: &mut CandidateDescriptor| row.line += 1)]
#[case::column(|row: &mut CandidateDescriptor| row.column += 1)]
#[case::absent_symbol(|row: &mut CandidateDescriptor| row.symbol = None)]
#[case::empty_symbol(|row: &mut CandidateDescriptor| row.symbol = Some(String::new()))]
#[case::hash_case(|row: &mut CandidateDescriptor| row.file_hash.make_ascii_uppercase())]
fn issue_1089_borrowed_equality_preserves_every_owned_descriptor_field(
    #[case] edit: DescriptorEdit,
) {
    let original = descriptor();
    let mut modified = original.clone();
    edit(&mut modified);

    let owned_equal = original == modified;
    let borrowed_equal =
        CandidateDescriptorRef::from(&original) == CandidateDescriptorRef::from(&modified);

    assert!(!owned_equal);
    assert_eq!(borrowed_equal, owned_equal);
}

#[rstest::rstest]
#[case::path(|row: &mut CandidateDescriptor| row.path = "other.py".into())]
#[case::span_start(|row: &mut CandidateDescriptor| row.span.start += 1)]
#[case::span_length(|row: &mut CandidateDescriptor| row.span.length += 1)]
#[case::original(|row: &mut CandidateDescriptor| row.original.push('x'))]
#[case::replacement(|row: &mut CandidateDescriptor| row.replacement.push('x'))]
#[case::operator(|row: &mut CandidateDescriptor| row.operator.push('x'))]
#[case::line(|row: &mut CandidateDescriptor| row.line += 1)]
#[case::column(|row: &mut CandidateDescriptor| row.column += 1)]
#[case::absent_symbol(|row: &mut CandidateDescriptor| row.symbol = None)]
#[case::empty_symbol(|row: &mut CandidateDescriptor| row.symbol = Some(String::new()))]
#[case::hash_case(|row: &mut CandidateDescriptor| row.file_hash.make_ascii_uppercase())]
fn changed_descriptor_matches_its_mutation_projection(#[case] edit: DescriptorEdit) {
    let mut modified = descriptor();
    edit(&mut modified);
    let candidate = mutation(&modified);

    let actual = CandidateDescriptorRef::from(&candidate);

    assert_eq!(actual, CandidateDescriptorRef::from(&modified));
}

#[rstest::rstest]
#[case::assigned("assigned bookkeeping", 77)]
#[case::empty_id_and_maximum_sequence("", u64::MAX)]
fn bookkeeping_is_excluded_from_borrowed_equality(#[case] id: &str, #[case] sequence: u64) {
    let original = descriptor();
    let mut candidate = mutation(&original);
    candidate.id = id.into();
    candidate.sequence = sequence;

    let actual = CandidateDescriptorRef::from(&candidate);

    assert_eq!(actual, CandidateDescriptorRef::from(&original));
}

proptest::proptest! {
    #[test]
    fn arbitrary_bookkeeping_is_excluded_from_borrowed_equality(id in ".{0,128}", sequence in proptest::prelude::any::<u64>()) {
        let original = descriptor();
        let mut candidate = mutation(&original);
        candidate.id = id;
        candidate.sequence = sequence;

        let actual = CandidateDescriptorRef::from(&candidate);

        proptest::prop_assert_eq!(actual, CandidateDescriptorRef::from(&original));
    }
}

#[rstest::rstest]
#[case::dot("pkg/./calc.py")]
#[case::repeated_separator("pkg//calc.py")]
#[case::parent("pkg/../calc.py")]
#[case::case("Pkg/calc.py")]
#[case::backslash("pkg\\calc.py")]
fn path_equality_matches_the_owned_descriptor(#[case] path: &str) {
    let original = descriptor();
    let mut modified = original.clone();
    modified.path = path.into();
    let expected = original == modified;

    let actual = CandidateDescriptorRef::from(&original) == CandidateDescriptorRef::from(&modified);

    assert_eq!(actual, expected);
}

#[test]
fn issue_1089_borrowed_validation_keeps_valid_identity() {
    let context = CandidateValidationContext::new(b"value == 2\n").unwrap();
    let valid = descriptor();
    let expected = CandidateIdentityRef::from(&valid).stable_mutant_id();

    let actual = validate_candidate_with_context(&context, &valid);

    assert_eq!(actual, Ok(expected));
}

#[test]
fn issue_1089_borrowed_mutation_validation_keeps_valid_identity() {
    let context = CandidateValidationContext::new(b"value == 2\n").unwrap();
    let valid = descriptor();
    let candidate = mutation(&valid);
    let expected = CandidateIdentityRef::from(&valid).stable_mutant_id();

    let actual = validate_candidate_with_context(&context, &candidate);

    assert_eq!(actual, Ok(expected));
}

#[rstest::rstest]
#[case::schema_before_path(|row: &mut CandidateDescriptor| {
                row.schema_version += 1;
                row.path = "../bad".into();
            }, CandidateValidationError::UnsupportedSchema)]
#[case::path_before_hash(|row: &mut CandidateDescriptor| {
                row.path = "../bad".into();
                row.file_hash.clear();
            }, CandidateValidationError::InvalidPath)]
#[case::hash_before_operator(|row: &mut CandidateDescriptor| {
                row.file_hash.clear();
                row.operator.clear();
            }, CandidateValidationError::FileHashMismatch)]
#[case::operator_before_span(|row: &mut CandidateDescriptor| {
                row.operator.clear();
                row.span.start = u64::MAX;
            }, CandidateValidationError::InvalidMutation)]
#[case::span_before_original(|row: &mut CandidateDescriptor| {
                row.span.start = u64::MAX;
                row.original = "bad".into();
            }, CandidateValidationError::SpanOutOfBounds)]
#[case::original_before_location(|row: &mut CandidateDescriptor| {
                row.original = "bad".into();
                row.line = 99;
            }, CandidateValidationError::OriginalMismatch)]
#[case::line(|row: &mut CandidateDescriptor| row.line = 99, CandidateValidationError::LocationMismatch)]
#[case::column(|row: &mut CandidateDescriptor| row.column = 99, CandidateValidationError::LocationMismatch)]
fn issue_1089_borrowed_descriptor_validation_keeps_error_precedence(
    #[case] edit: DescriptorEdit,
    #[case] expected: CandidateValidationError,
) {
    let context = CandidateValidationContext::new(b"value == 2\n").unwrap();
    let mut modified = descriptor();
    edit(&mut modified);

    let actual = validate_candidate_with_context(&context, &modified);

    assert_eq!(actual, Err(expected));
}

#[rstest::rstest]
#[case::path_before_hash(|row: &mut CandidateDescriptor| {
                row.path = "../bad".into();
                row.file_hash.clear();
            }, CandidateValidationError::InvalidPath)]
#[case::hash_before_operator(|row: &mut CandidateDescriptor| {
                row.file_hash.clear();
                row.operator.clear();
            }, CandidateValidationError::FileHashMismatch)]
#[case::operator_before_span(|row: &mut CandidateDescriptor| {
                row.operator.clear();
                row.span.start = u64::MAX;
            }, CandidateValidationError::InvalidMutation)]
#[case::span_before_original(|row: &mut CandidateDescriptor| {
                row.span.start = u64::MAX;
                row.original = "bad".into();
            }, CandidateValidationError::SpanOutOfBounds)]
#[case::original_before_location(|row: &mut CandidateDescriptor| {
                row.original = "bad".into();
                row.line = 99;
            }, CandidateValidationError::OriginalMismatch)]
#[case::line(|row: &mut CandidateDescriptor| row.line = 99, CandidateValidationError::LocationMismatch)]
#[case::column(|row: &mut CandidateDescriptor| row.column = 99, CandidateValidationError::LocationMismatch)]
fn issue_1089_borrowed_mutation_validation_keeps_error_precedence(
    #[case] edit: DescriptorEdit,
    #[case] expected: CandidateValidationError,
) {
    let context = CandidateValidationContext::new(b"value == 2\n").unwrap();
    let mut modified = descriptor();
    edit(&mut modified);
    let candidate = mutation(&modified);

    let actual = validate_candidate_with_context(&context, &candidate);

    assert_eq!(actual, Err(expected));
}
