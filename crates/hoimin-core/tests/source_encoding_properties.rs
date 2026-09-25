use hoimin_core::{PythonSourceEncoding, decode_python_source};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        max_shrink_iters: 2_048,
        ..ProptestConfig::default()
    })]

    #[test]
    fn boundary_property_latin1_coordinates_preserve_arbitrary_bytes(
        payload in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let mut source = b"# coding: latin-1\n".to_vec();
        source.extend(payload);
        let expected: String = source.iter().copied().map(char::from).collect();
        let decoded = decode_python_source(&source)?;
        prop_assert_eq!(decoded.encoding(), PythonSourceEncoding::Latin1);
        prop_assert_eq!(decoded.text(), &expected);
        let encoded = decoded.encoding().encode(decoded.text())?;
        prop_assert_eq!(encoded.as_ref(), &source);

        // Enumerate the independently rendered scalar boundaries, including EOF.
        let boundaries: Vec<_> = expected.char_indices().map(|(offset, _)| offset)
            .chain(std::iter::once(expected.len())).collect();
        for (raw, &utf8) in boundaries.iter().enumerate() {
            prop_assert_eq!(decoded.raw_to_utf8(raw), Some(utf8));
            prop_assert_eq!(decoded.utf8_to_raw(utf8), Some(raw));
        }
        for offset in 0..expected.len() {
            if !expected.is_char_boundary(offset) {
                prop_assert_eq!(decoded.utf8_to_raw(offset), None);
            }
        }
        for offset in [source.len() + 1, usize::MAX] {
            prop_assert_eq!(decoded.raw_to_utf8(offset), None);
        }
        for offset in [expected.len() + 1, usize::MAX] {
            prop_assert_eq!(decoded.utf8_to_raw(offset), None);
        }
    }

    #[test]
    fn boundary_property_utf8_coordinates_reject_split_scalars(
        characters in proptest::collection::vec(any::<char>(), 0..256),
        bom in any::<bool>(),
    ) {
        // An explicit first-line cookie isolates coordinate mapping from cookie parsing.
        let mut source = if bom { "\u{feff}" } else { "" }.to_owned();
        source.push_str("# coding: utf-8\n");
        source.extend(characters);
        let decoded = decode_python_source(source.as_bytes())?;
        prop_assert_eq!(decoded.encoding(), PythonSourceEncoding::Utf8);
        prop_assert_eq!(decoded.text(), &source);
        let encoded = decoded.encoding().encode(decoded.text())?;
        prop_assert_eq!(encoded.as_ref(), source.as_bytes());
        for offset in 0..=source.len() + 1 {
            let expected = source.is_char_boundary(offset).then_some(offset);
            prop_assert_eq!(decoded.raw_to_utf8(offset), expected);
            prop_assert_eq!(decoded.utf8_to_raw(offset), expected);
        }
        prop_assert_eq!(decoded.raw_to_utf8(usize::MAX), None);
        prop_assert_eq!(decoded.utf8_to_raw(usize::MAX), None);
    }
}
