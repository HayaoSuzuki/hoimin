use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use hoimin_core::{CandidateSpoolRef, ContractInvariant, MutationCandidate, contract_ensure};
use tempfile::NamedTempFile;
use thiserror::Error;

pub const MAX_SPOOL_RECORD_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("candidate limit {limit} exceeded")]
    LimitExceeded { limit: u64 },
    #[error("candidate sequence mismatch: expected {expected}, got {actual}")]
    InvalidSequence { expected: u64, actual: u64 },
    #[error("candidate spool record exceeds {limit} bytes")]
    RecordTooLarge { limit: u64 },
    #[error("invalid candidate spool offset {offset}")]
    InvalidOffset { offset: u64 },
    #[error(
        "candidate spool ended before all records: expected {expected_records}, got {actual_records}"
    )]
    UnexpectedEof {
        expected_records: u64,
        actual_records: u64,
    },
    #[error("candidate spool I/O failed: {0}")]
    Io(String),
    #[error("candidate spool record is corrupt: {0}")]
    CorruptRecord(String),
}

pub struct CandidateStore {
    file: NamedTempFile,
    count: u64,
    records_written: u64,
    max_candidates: u64,
}

impl CandidateStore {
    /// # Errors
    ///
    /// Returns an error when the candidate limit is zero or the temporary spool file cannot be
    /// created.
    pub fn new(max_candidates: u64) -> Result<Self, StoreError> {
        if max_candidates == 0 {
            return Err(StoreError::LimitExceeded { limit: 0 });
        }
        let file = NamedTempFile::new().map_err(|error| io_error(&error))?;
        Ok(Self::with_file(max_candidates, file))
    }

    /// Creates a candidate store inside a caller-owned directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the candidate limit is zero or the temporary spool file cannot be
    /// created in `directory`.
    pub fn new_in(max_candidates: u64, directory: impl AsRef<Path>) -> Result<Self, StoreError> {
        if max_candidates == 0 {
            return Err(StoreError::LimitExceeded { limit: 0 });
        }
        let file = NamedTempFile::new_in(directory).map_err(|error| io_error(&error))?;
        Ok(Self::with_file(max_candidates, file))
    }

    fn with_file(max_candidates: u64, file: NamedTempFile) -> Self {
        Self {
            file,
            count: 0,
            records_written: 0,
            max_candidates,
        }
    }

    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// # Errors
    ///
    /// Returns an error when the store is full, the candidate sequence is invalid, serialization
    /// fails, the encoded record is too large, or writing the spool fails.
    pub fn push(&mut self, candidate: &MutationCandidate) -> Result<(), StoreError> {
        if self.count >= self.max_candidates {
            return Err(StoreError::LimitExceeded {
                limit: self.max_candidates,
            });
        }
        let expected = self.count.checked_add(1).ok_or(StoreError::LimitExceeded {
            limit: self.max_candidates,
        })?;
        if candidate.sequence != expected {
            return Err(StoreError::InvalidSequence {
                expected,
                actual: candidate.sequence,
            });
        }
        let mut counter = CountingWriter::default();
        serde_json::to_writer(&mut counter, candidate)
            .map_err(|error| StoreError::CorruptRecord(error.to_string()))?;
        if counter.bytes >= MAX_SPOOL_RECORD_BYTES {
            return Err(StoreError::RecordTooLarge {
                limit: MAX_SPOOL_RECORD_BYTES,
            });
        }
        serde_json::to_writer(self.file.as_file_mut(), candidate)
            .map_err(|error| StoreError::Io(error.to_string()))?;
        self.file
            .as_file_mut()
            .write_all(b"\n")
            .map_err(|error| io_error(&error))?;
        self.count = expected;
        self.records_written = expected;
        contract_ensure!(
            "candidate_store.count.invariant",
            self.invariant(),
            (&self.count, &self.records_written, &self.max_candidates),
        );
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error when flushing, syncing, or preserving the spool file fails.
    pub fn finish(mut self) -> Result<CandidateSpoolRef, StoreError> {
        self.file
            .as_file_mut()
            .flush()
            .map_err(|error| io_error(&error))?;
        self.file
            .as_file()
            .sync_all()
            .map_err(|error| io_error(&error))?;
        let records = self.count;
        let (_file, path) = self.file.keep().map_err(|error| io_error(&error.error))?;
        Ok(CandidateSpoolRef {
            token: path.to_string_lossy().into_owned(),
            records,
        })
    }

    /// # Errors
    ///
    /// Returns an error when the spool cannot be read, the offset or sequence is invalid, a
    /// record is oversized or corrupt, or the spool ends before its declared record count.
    pub fn replay_one(
        reference: &CandidateSpoolRef,
        offset: u64,
    ) -> Result<Option<(MutationCandidate, u64)>, StoreError> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&reference.token)
            .map_err(|error| io_error(&error))?;
        let length = file.metadata().map_err(|error| io_error(&error))?.len();
        if offset > length {
            return Err(StoreError::InvalidOffset { offset });
        }
        if offset != 0 {
            file.seek(SeekFrom::Start(offset - 1))
                .map_err(|error| io_error(&error))?;
            let mut previous = [0_u8; 1];
            file.read_exact(&mut previous)
                .map_err(|error| io_error(&error))?;
            if previous[0] != b'\n' {
                return Err(StoreError::InvalidOffset { offset });
            }
        }
        let expected_sequence = expected_sequence_at(&mut file, offset)?;
        if offset == length {
            let actual_records = expected_sequence.saturating_sub(1);
            return match actual_records.cmp(&reference.records) {
                std::cmp::Ordering::Less => Err(StoreError::UnexpectedEof {
                    expected_records: reference.records,
                    actual_records,
                }),
                std::cmp::Ordering::Equal => Ok(None),
                std::cmp::Ordering::Greater => Err(StoreError::InvalidSequence {
                    expected: reference.records,
                    actual: actual_records,
                }),
            };
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| io_error(&error))?;
        read_one_bounded(file, reference, offset, expected_sequence)
    }
}

impl ContractInvariant for CandidateStore {
    fn invariant(&self) -> bool {
        self.count == self.records_written && self.count <= self.max_candidates
    }
}

fn read_one_bounded(
    file: File,
    reference: &CandidateSpoolRef,
    offset: u64,
    expected_sequence: u64,
) -> Result<Option<(MutationCandidate, u64)>, StoreError> {
    let mut line = Vec::new();
    let mut reader = BufReader::new(file).take(MAX_SPOOL_RECORD_BYTES + 1);
    let read = reader
        .read_until(b'\n', &mut line)
        .map_err(|error| io_error(&error))?;
    if read == 0 {
        return Err(StoreError::UnexpectedEof {
            expected_records: reference.records,
            actual_records: expected_sequence.saturating_sub(1),
        });
    }
    if read as u64 > MAX_SPOOL_RECORD_BYTES {
        return Err(StoreError::RecordTooLarge {
            limit: MAX_SPOOL_RECORD_BYTES,
        });
    }
    if line.last() != Some(&b'\n') {
        return Err(StoreError::UnexpectedEof {
            expected_records: reference.records,
            actual_records: expected_sequence.saturating_sub(1),
        });
    }
    line.pop();
    let candidate: MutationCandidate = serde_json::from_slice(&line)
        .map_err(|error| StoreError::CorruptRecord(error.to_string()))?;
    if candidate.sequence != expected_sequence {
        return Err(StoreError::InvalidSequence {
            expected: expected_sequence,
            actual: candidate.sequence,
        });
    }
    if candidate.sequence > reference.records {
        return Err(StoreError::InvalidSequence {
            expected: reference.records,
            actual: candidate.sequence,
        });
    }
    let next = offset
        .checked_add(read as u64)
        .ok_or(StoreError::InvalidOffset { offset })?;
    Ok(Some((candidate, next)))
}

fn expected_sequence_at(file: &mut File, offset: u64) -> Result<u64, StoreError> {
    if offset == 0 {
        return Ok(1);
    }
    let window_length = offset.min(MAX_SPOOL_RECORD_BYTES + 1);
    let window_start = offset - window_length;
    file.seek(SeekFrom::Start(window_start))
        .map_err(|error| io_error(&error))?;
    let mut window = vec![0_u8; window_length as usize];
    file.read_exact(&mut window)
        .map_err(|error| io_error(&error))?;
    let previous_body = window
        .strip_suffix(b"\n")
        .ok_or(StoreError::InvalidOffset { offset })?;
    let record_start = previous_body
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    if window_start != 0 && record_start == 0 {
        return Err(StoreError::RecordTooLarge {
            limit: MAX_SPOOL_RECORD_BYTES,
        });
    }
    let previous: MutationCandidate = serde_json::from_slice(&previous_body[record_start..])
        .map_err(|error| StoreError::CorruptRecord(error.to_string()))?;
    previous
        .sequence
        .checked_add(1)
        .ok_or(StoreError::InvalidSequence {
            expected: previous.sequence,
            actual: previous.sequence,
        })
}

fn io_error(error: &std::io::Error) -> StoreError {
    StoreError::Io(error.to_string())
}

#[derive(Default)]
struct CountingWriter {
    bytes: u64,
}

impl Write for CountingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| std::io::Error::other("serialized record size overflow"))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use hoimin_core::ByteSpan;
    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    struct CandidateSeed {
        path_segment: String,
        original: String,
        replacement: String,
        symbol: Option<String>,
        operator: &'static str,
        span_start: u64,
        span_length: u64,
        line: u32,
        column: u32,
    }

    struct FinishedSpool(CandidateSpoolRef);

    impl Drop for FinishedSpool {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0.token);
        }
    }

    fn unicode_text(length: std::ops::RangeInclusive<usize>) -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec![
                'a', 'Z', '0', 'é', '雪', '中', '🧪', 'λ', '"', '\\', '\n', '\t',
            ]),
            length,
        )
        .prop_map(|characters| characters.into_iter().collect())
    }

    fn path_segment() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec!['a', 'Z', '0', 'é', '雪', '中', '🧪', 'λ', '_', '-']),
            1..=12,
        )
        .prop_map(|characters| characters.into_iter().collect())
    }

    fn candidate_seed() -> impl Strategy<Value = CandidateSeed> {
        (
            path_segment(),
            unicode_text(1..=20),
            unicode_text(1..=20),
            prop::option::of(unicode_text(1..=16)),
            prop::sample::select(vec![
                "binary_add_sub",
                "compare_eq_ne",
                "type_nullable_remove",
            ]),
            0_u64..4_096,
            0_u64..128,
            1_u32..1_024,
            0_u32..256,
        )
            .prop_map(
                |(
                    path_segment,
                    original,
                    replacement,
                    symbol,
                    operator,
                    span_start,
                    span_length,
                    line,
                    column,
                )| CandidateSeed {
                    path_segment,
                    original,
                    replacement: format!("替{replacement}"),
                    symbol,
                    operator,
                    span_start,
                    span_length,
                    line,
                    column,
                },
            )
    }

    fn ordered_candidates() -> impl Strategy<Value = Vec<MutationCandidate>> {
        prop::collection::vec(candidate_seed(), 1..=16).prop_map(|seeds| {
            seeds
                .into_iter()
                .enumerate()
                .map(|(index, seed)| {
                    let sequence = u64::try_from(index + 1).expect("fixture length fits u64");
                    MutationCandidate {
                        id: format!("候補-{sequence}-🧪"),
                        sequence,
                        path: Utf8PathBuf::from(format!("src/{}-{sequence}.py", seed.path_segment)),
                        span: ByteSpan {
                            start: seed.span_start,
                            length: seed.span_length,
                        },
                        original: seed.original,
                        replacement: seed.replacement,
                        operator: seed.operator.to_owned(),
                        line: seed.line,
                        column: seed.column,
                        symbol: seed.symbol,
                        file_hash: format!("{sequence:064x}"),
                    }
                })
                .collect()
        })
    }

    fn finish_candidates(candidates: &[MutationCandidate]) -> FinishedSpool {
        let mut store = CandidateStore::new(
            u64::try_from(candidates.len()).expect("property vector length fits u64"),
        )
        .unwrap();
        for candidate in candidates {
            store.push(candidate).unwrap();
        }
        FinishedSpool(store.finish().unwrap())
    }

    fn replay_from(reference: &CandidateSpoolRef, mut offset: u64) -> Vec<MutationCandidate> {
        let mut replayed = Vec::new();
        while let Some((candidate, next_offset)) =
            CandidateStore::replay_one(reference, offset).unwrap()
        {
            replayed.push(candidate);
            offset = next_offset;
        }
        replayed
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn ordered_unicode_candidates_round_trip_from_every_record_offset(
            candidates in ordered_candidates()
        ) {
            let spool = finish_candidates(&candidates);
            let mut offset = 0;
            let mut replayed = Vec::new();
            let mut observed_offsets = Vec::new();

            while let Some((candidate, next_offset)) =
                CandidateStore::replay_one(&spool.0, offset).unwrap()
            {
                replayed.push(candidate);
                offset = next_offset;
                if replayed.len() < candidates.len() {
                    observed_offsets.push((replayed.len(), next_offset));
                }
            }

            prop_assert_eq!(&replayed, &candidates);
            prop_assert_eq!(CandidateStore::replay_one(&spool.0, offset).unwrap(), None);
            for (index, observed_offset) in observed_offsets {
                prop_assert_eq!(
                    replay_from(&spool.0, observed_offset),
                    candidates[index..].to_vec()
                );
            }
        }
    }

    #[test]
    fn serialized_payload_boundaries_include_the_newline_exactly_once() {
        for payload_length in [MAX_SPOOL_RECORD_BYTES - 2, MAX_SPOOL_RECORD_BYTES - 1] {
            let candidate = candidate_with_payload_len(payload_length);
            assert_eq!(serialized_len(&candidate), payload_length);
            let mut store = CandidateStore::new(1).unwrap();

            store.push(&candidate).unwrap();
            let spool = FinishedSpool(store.finish().unwrap());

            assert_eq!(
                std::fs::metadata(&spool.0.token).unwrap().len(),
                payload_length + 1
            );
        }

        let exact_limit = candidate_with_payload_len(MAX_SPOOL_RECORD_BYTES);
        assert_eq!(serialized_len(&exact_limit), MAX_SPOOL_RECORD_BYTES);
        let mut store = CandidateStore::new(1).unwrap();
        assert!(matches!(
            store.push(&exact_limit),
            Err(StoreError::RecordTooLarge {
                limit: MAX_SPOOL_RECORD_BYTES
            })
        ));
        assert_eq!(store.count(), 0);
        assert_eq!(store.records_written, 0);
        assert_eq!(store.file.as_file().metadata().unwrap().len(), 0);
    }

    fn candidate_with_payload_len(target: u64) -> MutationCandidate {
        let mut candidate = MutationCandidate {
            id: "boundary".to_owned(),
            sequence: 1,
            path: "src/boundary.py".into(),
            span: hoimin_core::ByteSpan {
                start: 0,
                length: 1,
            },
            original: String::new(),
            replacement: "-".to_owned(),
            operator: "binary_add_sub".to_owned(),
            line: 1,
            column: 0,
            symbol: Some("boundary".to_owned()),
            file_hash: "0".repeat(64),
        };
        let base = serialized_len(&candidate);
        assert!(target >= base, "target must fit the fixed record fields");
        let padding = usize::try_from(target - base).expect("spool limit fits usize");
        assert!(
            padding <= usize::try_from(MAX_SPOOL_RECORD_BYTES).unwrap(),
            "padding remains bounded by the production record limit"
        );
        candidate.original = "x".repeat(padding);
        assert_eq!(serialized_len(&candidate), target);
        candidate
    }

    fn serialized_len(candidate: &MutationCandidate) -> u64 {
        u64::try_from(serde_json::to_vec(candidate).unwrap().len())
            .expect("serialized boundary fixture length fits u64")
    }
}
