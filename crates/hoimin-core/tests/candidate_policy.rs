use camino::Utf8PathBuf;
use hoimin_core::{
    ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, CandidateIdentity,
    CandidateValidationError, stable_mutant_id, validate_candidate,
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
fn path_policy_matches_task4_helper_for_colons_and_absolute_paths() {
    let source = b"x = 1\nvalue == 2\n";
    let mut nested_colon = descriptor(source);
    nested_colon.path = Utf8PathBuf::from("pkg/a:b.py");
    assert!(validate_candidate(source, &nested_colon).is_ok());

    for invalid in ["/pkg/calc.py", "C:/pkg/calc.py", "pkg/../calc.py"] {
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
