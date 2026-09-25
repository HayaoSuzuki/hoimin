#![no_main]

use hoimin_core::{PythonSourceEncoding, decode_python_source};
use libfuzzer_sys::fuzz_target;

fn check(source: &[u8], required_encoding: Option<PythonSourceEncoding>) {
    let decoded = match decode_python_source(source) {
        Ok(decoded) => decoded,
        Err(_) => {
            assert!(required_encoding.is_none());
            return;
        }
    };
    if let Some(encoding) = required_encoding {
        assert_eq!(decoded.encoding(), encoding);
    }

    // Derive text and coordinates with standard scalar iteration, independently
    // of the decoder's sparse expansion table and binary searches.
    let expected = match decoded.encoding() {
        PythonSourceEncoding::Latin1 => source.iter().copied().map(char::from).collect(),
        PythonSourceEncoding::Ascii => {
            assert!(source.is_ascii());
            std::str::from_utf8(source).unwrap().to_owned()
        }
        PythonSourceEncoding::Utf8 => std::str::from_utf8(source).unwrap().to_owned(),
    };
    assert_eq!(decoded.text(), expected);
    assert_eq!(
        decoded.encoding().encode(decoded.text()).unwrap().as_ref(),
        source
    );

    let mut raw_to_utf8 = vec![None; source.len() + 1];
    let mut utf8_to_raw = vec![None; expected.len() + 1];
    for (scalar, utf8) in expected
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(expected.len()))
        .enumerate()
    {
        let raw = if decoded.encoding() == PythonSourceEncoding::Latin1 {
            scalar
        } else {
            utf8
        };
        raw_to_utf8[raw] = Some(utf8);
        utf8_to_raw[utf8] = Some(raw);
    }
    for (raw, expected) in raw_to_utf8.into_iter().enumerate() {
        assert_eq!(decoded.raw_to_utf8(raw), expected);
    }
    for (utf8, expected) in utf8_to_raw.into_iter().enumerate() {
        assert_eq!(decoded.utf8_to_raw(utf8), expected);
    }
    for offset in [source.len() + 1, usize::MAX] {
        assert_eq!(decoded.raw_to_utf8(offset), None);
    }
    for offset in [expected.len() + 1, usize::MAX] {
        assert_eq!(decoded.utf8_to_raw(offset), None);
    }
}

fuzz_target!(|data: &[u8]| {
    // Raw input exercises cookies, BOM conflicts, and malformed encodings.
    check(data, None);

    // A valid Latin-1 declaration ensures arbitrary bytes also reach mapping
    // checks, and catches a decoder that incorrectly rejects every input.
    let mut latin1 = b"# coding: latin-1\n".to_vec();
    latin1.extend_from_slice(data);
    check(&latin1, Some(PythonSourceEncoding::Latin1));

    if std::str::from_utf8(data).is_ok() {
        let mut utf8 = b"# coding: utf-8\n".to_vec();
        utf8.extend_from_slice(data);
        check(&utf8, Some(PythonSourceEncoding::Utf8));
    }
});
