//! Convert Git LF rows to Python physical rows by visiting the same raw bytes.
use hoimin_core::{LineRange, TargetError};

#[cfg(test)]
mod property_tests;

struct LineCursor<'a> {
    source: &'a [u8],
    offset: usize,
    git_row: usize,
    python_row: usize,
}

impl<'a> LineCursor<'a> {
    fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            offset: 0,
            git_row: 1,
            python_row: 1,
        }
    }

    fn advance(&mut self) -> Option<usize> {
        let byte = *self.source.get(self.offset)?;
        let row = self.python_row;
        self.git_row += usize::from(byte == b'\n');
        self.python_row += usize::from(
            byte == b'\n' || (byte == b'\r' && self.source.get(self.offset + 1) != Some(&b'\n')),
        );
        self.offset += 1;
        Some(row)
    }
}

fn row(value: usize) -> Result<u32, TargetError> {
    u32::try_from(value)
        .map_err(|_| TargetError::GitFailed("Python file has too many lines".into()))
}

pub(super) fn physical_line_count(source: &[u8]) -> Result<u32, TargetError> {
    let mut cursor = LineCursor::new(source);
    let mut last = 0;
    while let Some(line) = cursor.advance() {
        last = line;
    }
    row(last)
}

/// The caller supplies normalized, sorted Git intervals. No per-line index or
/// source normalization is needed; the cursor only advances across intervals.
pub(super) fn translate(
    source: &[u8],
    ranges: &[LineRange],
) -> Result<Vec<LineRange>, TargetError> {
    let mut cursor = LineCursor::new(source);
    let mut translated = Vec::with_capacity(ranges.len());
    for range in ranges {
        if range.start == 0 || range.end < range.start {
            return Err(TargetError::GitFailed("invalid Git line range".into()));
        }
        let start = range.start as usize;
        let end = range.end as usize;
        while cursor.git_row < start && cursor.advance().is_some() {}
        if cursor.git_row != start || cursor.offset == source.len() {
            return Err(outside_source());
        }
        let first = cursor.python_row;
        let mut last = first;
        let mut last_git = 0;
        while cursor.git_row <= end {
            let git_row = cursor.git_row;
            let Some(line) = cursor.advance() else { break };
            last = line;
            last_git = git_row;
        }
        if last_git != end {
            return Err(outside_source());
        }
        translated.push(LineRange {
            start: row(first)?,
            end: row(last)?,
        });
    }
    Ok(translated)
}

fn outside_source() -> TargetError {
    TargetError::GitFailed("Git changed-line range does not match current source bytes".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest::rstest]
    #[case::empty(b"".as_slice(), 0)]
    #[case::ascii(b"x".as_slice(), 1)]
    #[case::carriage_return(b"\r".as_slice(), 1)]
    #[case::line_feed(b"\n".as_slice(), 1)]
    #[case::crlf(b"\r\n".as_slice(), 1)]
    #[case::latin1(b"\xe9".as_slice(), 1)]
    #[case::maximum_original_length(b"x\r\nx\n".as_slice(), 2)]
    fn physical_line_count_preserves_byte_boundaries(#[case] source: &[u8], #[case] expected: u32) {
        let actual = physical_line_count(source).unwrap();

        assert_eq!(actual, expected);
    }

    proptest::proptest! {
        #[test]
        fn generated_byte_source_counts_match_independent_line_start_indexes(
            source in proptest::collection::vec(proptest::sample::select(vec![b'x', b'\r', b'\n', 0xe9]), 0..128),
        ) {
            let python = hoimin_core::python_line_starts(&source).unwrap();
            let expected = source.len().checked_sub(1).map_or(0, |last| python.partition_point(|start| *start as usize <= last));

            let actual = physical_line_count(&source).unwrap() as usize;

            proptest::prop_assert_eq!(actual, expected);
        }

        #[test]
        fn generated_git_intervals_translate_to_independent_python_line_indexes(
            source in proptest::collection::vec(proptest::sample::select(vec![b'x', b'\r', b'\n', 0xe9]), 1..128),
            first_query in proptest::prelude::any::<usize>(),
            last_query in proptest::prelude::any::<usize>(),
        ) {
            let python = hoimin_core::python_line_starts(&source).unwrap();
            let git = source.split_inclusive(|byte| *byte == b'\n').scan(0usize, |offset, bytes| {
                let start = *offset;
                *offset += bytes.len();
                Some(start..*offset)
            }).collect::<Vec<_>>();
            let first = first_query % git.len();
            let last = first + last_query % (git.len() - first);
            let expected = LineRange {
                start: u32::try_from(python.partition_point(|start| *start as usize <= git[first].start)).unwrap(),
                end: u32::try_from(python.partition_point(|start| (*start as usize) < git[last].end)).unwrap(),
            };
            let range = LineRange { start: u32::try_from(first + 1).unwrap(), end: u32::try_from(last + 1).unwrap() };

            let actual = translate(&source, &[range]).unwrap();

            proptest::prop_assert_eq!(actual, [expected]);
        }
    }

    #[test]
    fn disjoint_git_intervals_skip_bytes_without_losing_python_rows() {
        let source = b"a\rb\nc\r\nd\re\nf";
        let ranges = [
            LineRange { start: 1, end: 1 },
            LineRange { start: 3, end: 4 },
        ];

        let actual = translate(source, &ranges).unwrap();

        assert_eq!(
            actual,
            [
                LineRange { start: 1, end: 2 },
                LineRange { start: 4, end: 6 }
            ]
        );
    }

    #[rstest::rstest]
    #[case::b_as_slice_1(0)]
    #[case::b_a_2(1)]
    #[case::b_a_3(2)]
    #[case::b_a_4(3)]
    #[case::b_a_5(4)]
    #[case::b_a_6(5)]
    fn invalid_or_out_of_bounds_git_ranges_are_rejected(#[case] scenario_index: usize) {
        let (source, range) = [
            (b"".as_slice(), LineRange { start: 1, end: 1 }),
            (b"a\n", LineRange { start: 2, end: 2 }),
            (b"a\n", LineRange { start: 1, end: 2 }),
            (b"a\r", LineRange { start: 1, end: 2 }),
            (b"a", LineRange { start: 0, end: 1 }),
            (b"a", LineRange { start: 2, end: 1 }),
        ]
        .into_iter()
        .nth(scenario_index)
        .unwrap();

        let actual = translate(source, &[range]);

        assert!(actual.is_err(), "{source:?}: {range:?}");
    }

    #[rstest::rstest]
    #[case::b_as_slice(b"".as_slice())]
    #[case::b_a_b_c(b"a\r\nb\rc\n")]
    fn empty_ranges_select_no_rows(#[case] source: &[u8]) {
        let actual = translate(source, &[]).unwrap();

        assert_eq!(actual, Vec::<LineRange>::new());
    }

    #[test]
    fn maximum_representable_row_is_accepted() {
        let actual = row(u32::MAX as usize).unwrap();

        assert_eq!(actual, u32::MAX);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn unrepresentable_row_is_rejected() {
        let actual = row(u32::MAX as usize + 1);

        assert!(actual.is_err());
    }
}
