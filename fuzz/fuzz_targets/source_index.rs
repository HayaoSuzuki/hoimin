#![no_main]

use hoimin_core::PythonSourceIndex;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|source: &str| {
    let index = PythonSourceIndex::new(source).unwrap();
    let mut expected = vec![None; source.len() + 1];
    let mut characters = source.char_indices().peekable();
    let (mut line, mut column) = (1u32, 0u32);
    while let Some((offset, character)) = characters.next() {
        expected[offset] = Some((line, column));
        match character {
            '\n' => {
                line += 1;
                column = 0;
            }
            '\r' if !matches!(characters.peek(), Some((_, '\n'))) => {
                line += 1;
                column = 0;
            }
            '\u{feff}' if offset == 0 => {}
            _ => column += 1,
        }
    }
    expected[source.len()] = Some((line, column));
    for (offset, position) in expected.into_iter().enumerate() {
        assert_eq!(index.line_and_column(offset), position);
    }
    assert_eq!(index.line_and_column(source.len() + 1), None);
    assert_eq!(index.line_and_column(usize::MAX), None);
});
