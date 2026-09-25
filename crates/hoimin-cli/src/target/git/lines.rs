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

    #[test]
    fn every_small_byte_source_matches_independent_line_start_indexes() {
        for length in 0..=5 {
            for mut word in 0..4usize.pow(length) {
                let source = (0..length)
                    .map(|_| {
                        let byte = [b'x', b'\r', b'\n', 0xe9][word % 4];
                        word /= 4;
                        byte
                    })
                    .collect::<Vec<_>>();
                let python = hoimin_core::python_line_starts(&source).unwrap();
                let git = source
                    .split_inclusive(|byte| *byte == b'\n')
                    .scan(0usize, |offset, bytes| {
                        let start = *offset;
                        *offset += bytes.len();
                        Some(start..*offset)
                    })
                    .collect::<Vec<_>>();
                let expected_count = source.len().checked_sub(1).map_or(0, |last| {
                    python.partition_point(|start| *start as usize <= last)
                });
                assert_eq!(
                    physical_line_count(&source).unwrap() as usize,
                    expected_count
                );
                for first in 0..git.len() {
                    for last in first..git.len() {
                        let expected = LineRange {
                            start: u32::try_from(
                                python.partition_point(|start| *start as usize <= git[first].start),
                            )
                            .unwrap(),
                            end: u32::try_from(
                                python.partition_point(|start| (*start as usize) < git[last].end),
                            )
                            .unwrap(),
                        };
                        let range = LineRange {
                            start: u32::try_from(first + 1).unwrap(),
                            end: u32::try_from(last + 1).unwrap(),
                        };
                        assert_eq!(
                            translate(&source, &[range]).unwrap(),
                            [expected],
                            "{source:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn disjoint_git_intervals_skip_bytes_without_losing_python_rows() {
        let source = b"a\rb\nc\r\nd\re\nf";
        assert_eq!(
            translate(
                source,
                &[
                    LineRange { start: 1, end: 1 },
                    LineRange { start: 3, end: 4 }
                ]
            )
            .unwrap(),
            [
                LineRange { start: 1, end: 2 },
                LineRange { start: 4, end: 6 }
            ]
        );
    }

    #[test]
    fn empty_eof_rows_invalid_ranges_and_output_overflow_are_rejected() {
        for (source, range) in [
            (b"".as_slice(), LineRange { start: 1, end: 1 }),
            (b"a\n", LineRange { start: 2, end: 2 }),
            (b"a\n", LineRange { start: 1, end: 2 }),
            (b"a\r", LineRange { start: 1, end: 2 }),
            (b"a", LineRange { start: 0, end: 1 }),
            (b"a", LineRange { start: 2, end: 1 }),
        ] {
            assert!(translate(source, &[range]).is_err());
        }
        assert!(translate(b"", &[]).unwrap().is_empty());
        assert_eq!(row(u32::MAX as usize).unwrap(), u32::MAX);
        #[cfg(target_pointer_width = "64")]
        assert!(row(u32::MAX as usize + 1).is_err());
    }
}
