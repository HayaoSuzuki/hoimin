use hoimin_core::{PythonSourceEncoding, decode_python_source};
use proptest::prelude::*;

fn latin1_source(payload: &[u8]) -> Vec<u8> {
    let mut source = b"# coding: latin-1\n".to_vec();
    source.extend_from_slice(payload);
    source
}

fn utf8_source(characters: &[char], bom: bool) -> String {
    let mut source = ["", "\u{feff}"][usize::from(bom)].to_owned();
    source.push_str("# coding: utf-8\n");
    source.extend(characters);
    source
}

proptest! {
    #![proptest_config(ProptestConfig {
        max_shrink_iters: 2_048,
        ..ProptestConfig::default()
    })]

    #[test]
    fn boundary_property_latin1_decoding_preserves_arbitrary_bytes(
        payload in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let source = latin1_source(&payload);
        let expected: String = source.iter().copied().map(char::from).collect();

        let decoded = decode_python_source(&source)?;

        prop_assert_eq!(decoded.encoding(), PythonSourceEncoding::Latin1);
        prop_assert_eq!(decoded.text(), expected);
    }

    #[test]
    fn boundary_property_latin1_encoding_roundtrips_arbitrary_bytes(
        payload in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let source = latin1_source(&payload);
        let decoded = decode_python_source(&source)?;

        let encoded = decoded.encoding().encode(decoded.text())?;

        prop_assert_eq!(encoded.as_ref(), &source);
    }

    #[test]
    fn boundary_property_latin1_raw_coordinate_matches_scalar_widths(
        payload in proptest::collection::vec(any::<u8>(), 0..512),
        query in 0usize..1024,
    ) {
        let source = latin1_source(&payload);
        let decoded = decode_python_source(&source)?;
        let expected: String = source.iter().copied().map(char::from).collect();
        let offset = query % (source.len() + 2);
        let expected = expected.char_indices().map(|(offset, _)| offset)
            .chain(std::iter::once(expected.len())).nth(offset);

        let actual = decoded.raw_to_utf8(offset);

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn boundary_property_latin1_utf8_coordinate_rejects_split_scalars(
        payload in proptest::collection::vec(any::<u8>(), 0..512),
        query in 0usize..2048,
    ) {
        let source = latin1_source(&payload);
        let decoded = decode_python_source(&source)?;
        let expected: String = source.iter().copied().map(char::from).collect();
        let offset = query % (expected.len() + 2);
        let expected = expected.char_indices().map(|(offset, _)| offset)
            .chain(std::iter::once(expected.len())).position(|boundary| boundary == offset);

        let actual = decoded.utf8_to_raw(offset);

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn boundary_property_utf8_decoding_preserves_characters(
        characters in proptest::collection::vec(any::<char>(), 0..256),
        bom in any::<bool>(),
    ) {
        let source = utf8_source(&characters, bom);

        let decoded = decode_python_source(source.as_bytes())?;

        prop_assert_eq!(decoded.encoding(), PythonSourceEncoding::Utf8);
        prop_assert_eq!(decoded.text(), &source);
    }

    #[test]
    fn boundary_property_utf8_encoding_roundtrips_characters(
        characters in proptest::collection::vec(any::<char>(), 0..256),
        bom in any::<bool>(),
    ) {
        let source = utf8_source(&characters, bom);
        let decoded = decode_python_source(source.as_bytes())?;

        let encoded = decoded.encoding().encode(decoded.text())?;

        prop_assert_eq!(encoded.as_ref(), source.as_bytes());
    }

    #[test]
    fn boundary_property_utf8_raw_coordinate_rejects_split_scalars(
        characters in proptest::collection::vec(any::<char>(), 0..256),
        bom in any::<bool>(),
        query in 0usize..2048,
    ) {
        let source = utf8_source(&characters, bom);
        let decoded = decode_python_source(source.as_bytes())?;
        let offset = query % (source.len() + 2);
        let expected = source.is_char_boundary(offset).then_some(offset);

        let actual = decoded.raw_to_utf8(offset);

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn boundary_property_utf8_coordinate_rejects_split_scalars(
        characters in proptest::collection::vec(any::<char>(), 0..256),
        bom in any::<bool>(),
        query in 0usize..2048,
    ) {
        let source = utf8_source(&characters, bom);
        let decoded = decode_python_source(source.as_bytes())?;
        let offset = query % (source.len() + 2);
        let expected = source.is_char_boundary(offset).then_some(offset);

        let actual = decoded.utf8_to_raw(offset);

        prop_assert_eq!(actual, expected);
    }
}

#[rstest::rstest]
#[case::latin1(b"# coding: latin-1\n\xe9".as_slice())]
#[case::utf8("# coding: utf-8\n日本語".as_bytes())]
fn maximum_coordinate_is_outside_source(#[case] source: &[u8]) {
    let decoded = decode_python_source(source).unwrap();

    let raw = decoded.raw_to_utf8(usize::MAX);
    let utf8 = decoded.utf8_to_raw(usize::MAX);

    assert_eq!((raw, utf8), (None, None));
}
