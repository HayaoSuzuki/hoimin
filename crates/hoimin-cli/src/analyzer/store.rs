use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};

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
    pub fn new(max_candidates: u64) -> Result<Self, StoreError> {
        if max_candidates == 0 {
            return Err(StoreError::LimitExceeded { limit: 0 });
        }
        Ok(Self {
            file: NamedTempFile::new().map_err(io_error)?,
            count: 0,
            records_written: 0,
            max_candidates,
        })
    }

    pub fn count(&self) -> u64 {
        self.count
    }

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
        self.file.as_file_mut().write_all(b"\n").map_err(io_error)?;
        self.count = expected;
        self.records_written = expected;
        contract_ensure!(
            "candidate_store.count.invariant",
            self.invariant(),
            (&self.count, &self.records_written, &self.max_candidates),
        );
        Ok(())
    }

    pub fn finish(mut self) -> Result<CandidateSpoolRef, StoreError> {
        self.file.as_file_mut().flush().map_err(io_error)?;
        self.file.as_file().sync_all().map_err(io_error)?;
        let records = self.count;
        let (_file, path) = self.file.keep().map_err(|error| io_error(error.error))?;
        Ok(CandidateSpoolRef {
            token: path.to_string_lossy().into_owned(),
            records,
        })
    }

    pub fn replay_one(
        reference: &CandidateSpoolRef,
        offset: u64,
    ) -> Result<Option<(MutationCandidate, u64)>, StoreError> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&reference.token)
            .map_err(io_error)?;
        let length = file.metadata().map_err(io_error)?.len();
        if offset > length {
            return Err(StoreError::InvalidOffset { offset });
        }
        if offset == length {
            return Ok(None);
        }
        if offset != 0 {
            file.seek(SeekFrom::Start(offset - 1)).map_err(io_error)?;
            let mut previous = [0_u8; 1];
            file.read_exact(&mut previous).map_err(io_error)?;
            if previous[0] != b'\n' {
                return Err(StoreError::InvalidOffset { offset });
            }
        }
        file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
        read_one_bounded(file, reference, offset)
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
) -> Result<Option<(MutationCandidate, u64)>, StoreError> {
    let mut line = Vec::new();
    let mut reader = BufReader::new(file).take(MAX_SPOOL_RECORD_BYTES + 1);
    let read = reader.read_until(b'\n', &mut line).map_err(io_error)?;
    if read == 0 {
        return Ok(None);
    }
    if read as u64 > MAX_SPOOL_RECORD_BYTES || line.last() != Some(&b'\n') {
        return Err(StoreError::RecordTooLarge {
            limit: MAX_SPOOL_RECORD_BYTES,
        });
    }
    line.pop();
    let candidate: MutationCandidate = serde_json::from_slice(&line)
        .map_err(|error| StoreError::CorruptRecord(error.to_string()))?;
    if candidate.sequence == 0 || candidate.sequence > reference.records {
        return Err(StoreError::CorruptRecord(
            "sequence outside spool reference".into(),
        ));
    }
    let next = offset
        .checked_add(read as u64)
        .ok_or(StoreError::InvalidOffset { offset })?;
    Ok(Some((candidate, next)))
}

fn io_error(error: std::io::Error) -> StoreError {
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
