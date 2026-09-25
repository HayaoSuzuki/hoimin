use std::io::{self, Read};

use camino::Utf8Path;

use super::WorkspaceError;

#[cfg(test)]
mod property_tests;

pub(super) const BUFFER_BYTES: usize = 64 * 1024;

pub(super) fn chunks(
    reader: &mut impl Read,
    path: &Utf8Path,
    operation: &'static str,
    mut consume: impl FnMut(&[u8]) -> Result<(), WorkspaceError>,
) -> Result<u64, WorkspaceError> {
    let mut buffer = vec![0; BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(WorkspaceError::io(operation, path, error)),
        };
        total = total
            .checked_add(count as u64)
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        consume(&buffer[..count])?;
    }
}

fn fill(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut count = 0;
    while count < buffer.len() {
        match reader.read(&mut buffer[count..]) {
            Ok(0) => break,
            Ok(read) => count += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(count)
}

pub(super) fn equal(
    left: &mut impl Read,
    right: &mut impl Read,
    mut observed: impl FnMut(usize, usize),
) -> io::Result<bool> {
    let mut left_buffer = vec![0; BUFFER_BYTES];
    let mut right_buffer = vec![0; BUFFER_BYTES];
    let mut equal = true;
    loop {
        let left_count = fill(left, &mut left_buffer)?;
        let right_count = fill(right, &mut right_buffer)?;
        observed(left_count, right_count);
        equal &= left_buffer[..left_count] == right_buffer[..right_count];
        if left_count == 0 && right_count == 0 {
            return Ok(equal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fragmented<'a> {
        remaining: &'a [u8],
        interrupted: bool,
        size: usize,
    }

    impl Read for Fragmented<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.interrupted {
                self.interrupted = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.interrupted = true;
            let count = buffer.len().min(self.remaining.len()).min(self.size);
            buffer[..count].copy_from_slice(&self.remaining[..count]);
            self.remaining = &self.remaining[count..];
            Ok(count)
        }
    }

    #[test]
    fn comparison_handles_different_short_reads_interrupts_and_eof() {
        for right in [
            b"abc\0def".as_slice(),
            b"abc\0deg",
            b"abc\0de",
            b"abc\0defg",
        ] {
            let mut left = Fragmented {
                remaining: b"abc\0def",
                interrupted: true,
                size: 2,
            };
            let mut right_reader = Fragmented {
                remaining: right,
                interrupted: false,
                size: 3,
            };
            let mut counts = (0, 0);
            assert_eq!(
                equal(&mut left, &mut right_reader, |l, r| {
                    counts.0 += l;
                    counts.1 += r;
                })
                .unwrap(),
                right == b"abc\0def"
            );
            assert_eq!(counts, (7, right.len()));
        }
    }

    #[test]
    fn chunk_copy_hashes_every_byte_despite_short_reads_and_interrupts() {
        let mut reader = Fragmented {
            remaining: b"abc\0def",
            interrupted: true,
            size: 2,
        };
        let mut written = Vec::new();
        let entry =
            super::super::manifest::hash_contents(Utf8Path::new("data"), &mut reader, |bytes| {
                written.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert_eq!(written, b"abc\0def");
        assert_eq!(entry.size, 7);
        assert_eq!(entry.blake3, blake3::hash(b"abc\0def"));
    }
}
