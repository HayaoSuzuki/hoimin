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

#[test]
fn issue_1089_borrowed_equality_preserves_every_owned_descriptor_field() {
    let original = descriptor();
    let edits: [DescriptorEdit; 12] = [
        |row| row.schema_version += 1,
        |row| row.path = "other.py".into(),
        |row| row.span.start += 1,
        |row| row.span.length += 1,
        |row| row.original.push('x'),
        |row| row.replacement.push('x'),
        |row| row.operator.push('x'),
        |row| row.line += 1,
        |row| row.column += 1,
        |row| row.symbol = None,
        |row| row.symbol = Some(String::new()),
        |row| row.file_hash.make_ascii_uppercase(),
    ];
    for edit in edits {
        let mut modified = original.clone();
        edit(&mut modified);
        assert_ne!(original, modified);
        assert_ne!(
            CandidateDescriptorRef::from(&original),
            CandidateDescriptorRef::from(&modified)
        );
        if modified.schema_version == CANDIDATE_SCHEMA_VERSION {
            assert_eq!(
                CandidateDescriptorRef::from(&modified),
                CandidateDescriptorRef::from(&mutation(&modified))
            );
        }
    }
    let mut candidate = mutation(&original);
    assert_eq!(
        CandidateDescriptorRef::from(&original),
        CandidateDescriptorRef::from(&candidate)
    );
    candidate.id.clear();
    candidate.sequence = u64::MAX;
    assert_eq!(
        CandidateDescriptorRef::from(&original),
        CandidateDescriptorRef::from(&candidate)
    );
    for path in [
        "pkg/./calc.py",
        "pkg//calc.py",
        "pkg/../calc.py",
        "Pkg/calc.py",
        "pkg\\calc.py",
    ] {
        let mut modified = original.clone();
        modified.path = path.into();
        assert_eq!(
            original == modified,
            CandidateDescriptorRef::from(&original) == CandidateDescriptorRef::from(&modified)
        );
    }
}

#[test]
fn issue_1089_borrowed_validation_keeps_errors_and_identity() {
    let context = CandidateValidationContext::new(b"value == 2\n").unwrap();
    let valid = descriptor();
    let expected = CandidateIdentityRef::from(&valid).stable_mutant_id();
    assert_eq!(
        validate_candidate_with_context(&context, &valid),
        Ok(expected.clone())
    );
    assert_eq!(
        validate_candidate_with_context(&context, &mutation(&valid)),
        Ok(expected)
    );
    let cases: [(DescriptorEdit, CandidateValidationError); 8] = [
        (
            |row| {
                row.schema_version += 1;
                row.path = "../bad".into();
            },
            CandidateValidationError::UnsupportedSchema,
        ),
        (
            |row| {
                row.path = "../bad".into();
                row.file_hash.clear();
            },
            CandidateValidationError::InvalidPath,
        ),
        (
            |row| {
                row.file_hash.clear();
                row.operator.clear();
            },
            CandidateValidationError::FileHashMismatch,
        ),
        (
            |row| {
                row.operator.clear();
                row.span.start = u64::MAX;
            },
            CandidateValidationError::InvalidMutation,
        ),
        (
            |row| {
                row.span.start = u64::MAX;
                row.original = "bad".into();
            },
            CandidateValidationError::SpanOutOfBounds,
        ),
        (
            |row| {
                row.original = "bad".into();
                row.line = 99;
            },
            CandidateValidationError::OriginalMismatch,
        ),
        (
            |row| row.line = 99,
            CandidateValidationError::LocationMismatch,
        ),
        (
            |row| row.column = 99,
            CandidateValidationError::LocationMismatch,
        ),
    ];
    for (edit, expected_error) in cases {
        let mut modified = valid.clone();
        edit(&mut modified);
        assert_eq!(
            validate_candidate_with_context(&context, &modified),
            Err(expected_error.clone())
        );
        if modified.schema_version == CANDIDATE_SCHEMA_VERSION {
            assert_eq!(
                validate_candidate_with_context(&context, &mutation(&modified)),
                Err(expected_error)
            );
        }
    }
}
