use std::io::{self, Read};

use camino::Utf8Path;
use proptest::prelude::*;

use super::{BUFFER_BYTES, WorkspaceError, chunks, equal};

fn contents() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        3 => proptest::collection::vec(any::<u8>(), 0..1_024),
        1 => (
            proptest::collection::vec(any::<u8>(), 1..32),
            prop::sample::select(vec![BUFFER_BYTES - 1, BUFFER_BYTES, BUFFER_BYTES + 1,
                2 * BUFFER_BYTES - 1, 2 * BUFFER_BYTES, 2 * BUFFER_BYTES + 1]),
        ).prop_map(|(pattern, length)| pattern.into_iter().cycle().take(length).collect()),
    ]
}

/// Every read makes progress or interrupts at most once before making progress.
/// Independent fragment sizes expose assumptions about equal read boundaries.
struct Reader<'a> {
    bytes: &'a [u8],
    fragment: usize,
    interrupt_next: bool,
    interrupts: bool,
    terminal_error: Option<io::ErrorKind>,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], fragment: usize, interrupts: bool) -> Self {
        Self {
            bytes,
            fragment,
            interrupt_next: interrupts,
            interrupts,
            terminal_error: None,
        }
    }
}

impl Read for Reader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.interrupt_next {
            self.interrupt_next = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.bytes.is_empty()
            && let Some(error) = self.terminal_error
        {
            return Err(error.into());
        }
        let count = buffer.len().min(self.fragment).min(self.bytes.len());
        buffer[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        self.interrupt_next = self.interrupts;
        Ok(count)
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        max_shrink_iters: 2_048,
        ..ProptestConfig::default()
    })]

    #[test]
    fn boundary_property_stream_equality_and_counts_ignore_read_partitioning(
        left in contents(),
        independent in proptest::collection::vec(any::<u8>(), 0..1_024),
        mode in 0u8..4,
        at in any::<usize>(),
        left_fragment in 1usize..257,
        right_fragment in 1usize..257,
        left_interrupts in any::<bool>(),
        right_interrupts in any::<bool>(),
    ) {
        let mut right = left.clone();
        match mode {
            0 => {}, // Equal content must remain equal under different short reads.
            1 if !right.is_empty() => { let index = at % right.len(); right[index] ^= 1; },
            1 => right.push(0),
            2 => right.truncate(at % (right.len() + 1)),
            _ => right = independent,
        }
        let expected = left == right;
        let mut left_reader = Reader::new(&left, left_fragment, left_interrupts);
        let mut right_reader = Reader::new(&right, right_fragment, right_interrupts);
        let mut observed = (0, 0);
        let actual = equal(&mut left_reader, &mut right_reader, |l, r| {
            observed.0 += l;
            observed.1 += r;
        })?;
        prop_assert_eq!(actual, expected);
        prop_assert_eq!(observed, (left.len(), right.len()));
        prop_assert!(left_reader.bytes.is_empty());
        prop_assert!(right_reader.bytes.is_empty());
    }

    #[test]
    fn boundary_property_streamed_copy_and_hash_preserve_all_bytes(
        bytes in contents(),
        fragment in prop_oneof![1usize..513, Just(BUFFER_BYTES), Just(BUFFER_BYTES + 1)],
        interrupts in any::<bool>(),
    ) {
        let mut reader = Reader::new(&bytes, fragment, interrupts);
        let mut copied = Vec::new();
        let entry = super::super::manifest::hash_contents(
            Utf8Path::new("generated.bin"), &mut reader, |part| {
                copied.extend_from_slice(part);
                Ok(())
            },
        )?;
        prop_assert_eq!(entry.size, u64::try_from(bytes.len()).unwrap());
        prop_assert_eq!(entry.blake3, blake3::hash(&bytes));
        prop_assert_eq!(&copied, &bytes);
        prop_assert!(reader.bytes.is_empty());
    }

    #[test]
    fn boundary_property_streams_propagate_terminal_read_errors(
        bytes in contents(),
        fragment in 1usize..257,
        interrupts in any::<bool>(),
        fail_left in any::<bool>(),
    ) {
        let mut reader = Reader::new(&bytes, fragment, interrupts);
        reader.terminal_error = Some(io::ErrorKind::PermissionDenied);
        let mut copied = Vec::new();
        let result = chunks(&mut reader, Utf8Path::new("generated.bin"), "read", |part| {
            copied.extend_from_slice(part);
            Ok(())
        });
        let error = result.unwrap_err();
        let WorkspaceError::Io { operation, path, message } = error else {
            return Err(TestCaseError::fail("terminal read error lost its I/O context"));
        };
        prop_assert_eq!(operation, "read");
        prop_assert_eq!(path, Utf8Path::new("generated.bin"));
        prop_assert_eq!(message, io::Error::from(io::ErrorKind::PermissionDenied).to_string());
        prop_assert_eq!(&copied, &bytes);

        let mut left = Reader::new(&bytes, fragment, interrupts);
        let mut right = Reader::new(&bytes, fragment + 1, !interrupts);
        left.terminal_error = fail_left.then_some(io::ErrorKind::PermissionDenied);
        right.terminal_error = (!fail_left).then_some(io::ErrorKind::PermissionDenied);
        let error = equal(&mut left, &mut right, |_, _| {}).unwrap_err();
        prop_assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn boundary_property_consumer_error_stops_the_stream(
        bytes in proptest::collection::vec(any::<u8>(), 1..1_024),
        fragment in 1usize..65,
        at in any::<usize>(),
        interrupts in any::<bool>(),
    ) {
        let mut reader = Reader::new(&bytes, fragment, interrupts);
        let fail_call = at % bytes.len().div_ceil(fragment) + 1;
        let mut calls = 0;
        let mut delivered = Vec::new();
        let result = chunks(&mut reader, Utf8Path::new("generated.bin"), "read", |part| {
            calls += 1;
            delivered.extend_from_slice(part);
            if calls == fail_call { Err(WorkspaceError::CopySizeOverflow) } else { Ok(()) }
        });
        prop_assert!(matches!(result, Err(WorkspaceError::CopySizeOverflow)));
        prop_assert_eq!(calls, fail_call);
        let length = (fail_call * fragment).min(bytes.len());
        prop_assert_eq!(delivered, &bytes[..length]);
        prop_assert_eq!(reader.bytes, &bytes[length..]);
    }
}
