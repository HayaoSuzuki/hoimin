#![no_main]

use hoimin_core::{
    ByteSpan, CandidateDescriptor, CandidateValidationContext, CandidateValidationError as Error,
    validate_candidate, validate_candidate_with_context,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 || data.len() > 4096 {
        return;
    }
    let latin1 = data[0] & 1 != 0;
    let payload = &data[5..];
    let mut source = if latin1 {
        b"# coding: latin-1\n".to_vec()
    } else {
        b"# coding: utf-8\n".to_vec()
    };
    if latin1 {
        source.extend_from_slice(payload);
    } else {
        source.extend_from_slice(String::from_utf8_lossy(payload).as_bytes());
    }
    let text: String = if latin1 {
        source.iter().copied().map(char::from).collect()
    } else {
        std::str::from_utf8(&source).unwrap().to_owned()
    };
    let boundaries: Vec<_> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let a = usize::from(u16::from_le_bytes([data[1], data[2]])) % boundaries.len();
    let b = usize::from(u16::from_le_bytes([data[3], data[4]])) % boundaries.len();
    let (a, b) = (a.min(b), a.max(b));
    let (start, end) = (boundaries[a], boundaries[b]);
    let (raw_start, raw_end) = if latin1 { (a, b) } else { (start, end) };
    let (mut line, mut column) = (1u32, 0u32);
    let mut chars = text.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if offset == start {
            break;
        }
        if ch == '\n' || (ch == '\r' && !matches!(chars.peek(), Some((_, '\n')))) {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    let context = CandidateValidationContext::new(&source).unwrap();
    let candidate = CandidateDescriptor {
        schema_version: 1,
        path: "pkg/input.py".into(),
        span: ByteSpan {
            start: raw_start as u64,
            length: (raw_end - raw_start) as u64,
        },
        original: text[start..end].into(),
        replacement: format!("{}x", &text[start..end]),
        operator: "fuzz".into(),
        line,
        column,
        symbol: None,
        file_hash: context.file_hash().into(),
    };
    let id = validate_candidate_with_context(&context, &candidate).unwrap();
    assert_eq!(validate_candidate(&source, &candidate), Ok(id.clone()));
    let mut metadata = candidate.clone();
    metadata.symbol = Some("changed_metadata".into());
    assert_eq!(validate_candidate_with_context(&context, &metadata), Ok(id));

    // Break one field at a time: each expected rejection names a real contract.
    for fault in 0..9 {
        let mut invalid = candidate.clone();
        let expected = match fault {
            0 => {
                invalid.schema_version = 0;
                Error::UnsupportedSchema
            }
            1 => {
                invalid.path = "../input.py".into();
                Error::InvalidPath
            }
            2 => {
                invalid.file_hash = "0".repeat(63);
                Error::FileHashMismatch
            }
            3 => {
                invalid.span.start = u64::MAX;
                invalid.span.length = 1;
                Error::SpanOutOfBounds
            }
            4 => {
                invalid.original.push('y');
                Error::OriginalMismatch
            }
            5 => {
                invalid.line += 1;
                Error::LocationMismatch
            }
            6 => {
                invalid.column += 1;
                Error::LocationMismatch
            }
            7 => {
                invalid.operator.clear();
                Error::InvalidMutation
            }
            _ => {
                invalid.replacement.clone_from(&invalid.original);
                Error::InvalidMutation
            }
        };
        assert_eq!(
            validate_candidate_with_context(&context, &invalid),
            Err(expected)
        );
    }
    if latin1 {
        let mut invalid = candidate;
        invalid.replacement = "\u{100}".into();
        assert!(matches!(
            validate_candidate_with_context(&context, &invalid),
            Err(Error::InvalidEncoding(_))
        ));
    }
});
