use std::fmt;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ByteSpan;

pub const CANDIDATE_SCHEMA_VERSION: u32 = 1;
const MUTANT_ID_DOMAIN: &[u8] = b"hoimin.mutant-id.v1\0";

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MutantId(String);

impl MutantId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MutantId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateIdentity {
    pub schema_version: u32,
    pub file_hash: String,
    pub path: Utf8PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub replacement: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateDescriptor {
    pub schema_version: u32,
    pub path: Utf8PathBuf,
    pub span: ByteSpan,
    pub original: String,
    pub replacement: String,
    pub operator: String,
    pub line: u32,
    pub column: u32,
    pub symbol: Option<String>,
    pub file_hash: String,
}

impl From<&CandidateDescriptor> for CandidateIdentity {
    fn from(candidate: &CandidateDescriptor) -> Self {
        Self {
            schema_version: candidate.schema_version,
            file_hash: candidate.file_hash.clone(),
            path: candidate.path.clone(),
            span: candidate.span,
            operator: candidate.operator.clone(),
            replacement: candidate.replacement.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CandidateValidationError {
    #[error("candidate source exceeds the maximum supported length of 4294967295 bytes")]
    SourceTooLarge,
    #[error("unsupported candidate schema version")]
    UnsupportedSchema,
    #[error("candidate path is not a normalized relative path")]
    InvalidPath,
    #[error("candidate file hash does not match the source")]
    FileHashMismatch,
    #[error("candidate span is outside the source")]
    SpanOutOfBounds,
    #[error("candidate original text does not match the source span")]
    OriginalMismatch,
    #[error("candidate line or column does not match its byte span")]
    LocationMismatch,
    #[error("candidate source is not valid UTF-8")]
    InvalidUtf8,
    #[error("candidate operator is empty or replacement is unchanged")]
    InvalidMutation,
}

/// Source-derived facts reused while validating candidates from one immutable file.
#[derive(Debug)]
pub struct CandidateValidationContext<'source> {
    source: &'source [u8],
    text: Result<&'source str, std::str::Utf8Error>,
    file_hash: String,
    source_index: Option<PythonSourceIndex>,
}

impl<'source> CandidateValidationContext<'source> {
    /// # Errors
    ///
    /// Returns [`CandidateValidationError::SourceTooLarge`] when the source exceeds `u32::MAX` bytes.
    pub fn new(source: &'source [u8]) -> Result<Self, CandidateValidationError> {
        let line_starts = python_line_starts(source)?;
        let text = std::str::from_utf8(source);
        let source_index = text
            .as_ref()
            .ok()
            .map(|text| PythonSourceIndex::from_parts(text, line_starts));
        Ok(Self {
            source,
            text,
            file_hash: blake3::hash(source).to_hex().to_string(),
            source_index,
        })
    }

    #[must_use]
    pub fn file_hash(&self) -> &str {
        &self.file_hash
    }
}

/// Reusable physical-line and Unicode-column facts for valid UTF-8 Python source.
#[derive(Debug)]
pub struct PythonSourceIndex {
    source_len: u32,
    line_starts: Vec<u32>,
    leading_bom: bool,
    // Each entry records the cumulative number of bytes beyond one byte per
    // Unicode scalar after the scalar ending at the given byte offset.
    unicode_excess: Vec<(u32, u32)>,
}

impl PythonSourceIndex {
    /// Builds an index without changing the source bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CandidateValidationError::SourceTooLarge`] when the source
    /// exceeds `u32::MAX` bytes.
    pub fn new(source: &str) -> Result<Self, CandidateValidationError> {
        let line_starts = python_line_starts(source.as_bytes())?;
        Ok(Self::from_parts(source, line_starts))
    }

    fn from_parts(source: &str, line_starts: Vec<u32>) -> Self {
        let mut cumulative = 0u32;
        let mut unicode_excess = Vec::new();
        for (start, character) in source.char_indices() {
            let length =
                u32::try_from(character.len_utf8()).expect("UTF-8 scalar uses at most 4 bytes");
            if length > 1 {
                cumulative += length - 1;
                unicode_excess.push((
                    u32::try_from(start + character.len_utf8())
                        .expect("source length was checked before index construction"),
                    cumulative,
                ));
            }
        }
        Self {
            source_len: u32::try_from(source.len())
                .expect("source length was checked before index construction"),
            line_starts,
            leading_bom: source.starts_with('\u{feff}'),
            unicode_excess,
        }
    }

    /// Returns the one-based physical line and zero-based Python source column.
    #[must_use]
    pub fn line_and_column(&self, offset: usize) -> Option<(u32, u32)> {
        let offset = u32::try_from(offset).ok()?;
        if offset > self.source_len || !self.is_character_boundary(offset) {
            return None;
        }
        let line_index = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts[line_index];
        let byte_column = offset - line_start;
        let unicode_excess = self.excess_at(offset) - self.excess_at(line_start);
        let bom_column = u32::from(line_start == 0 && offset >= 3 && self.leading_bom);
        Some((
            u32::try_from(line_index).ok()? + 1,
            byte_column - unicode_excess - bom_column,
        ))
    }

    fn excess_at(&self, offset: u32) -> u32 {
        let index = self
            .unicode_excess
            .partition_point(|(end, _)| *end <= offset);
        index
            .checked_sub(1)
            .map_or(0, |index| self.unicode_excess[index].1)
    }

    fn is_character_boundary(&self, offset: u32) -> bool {
        let index = self
            .unicode_excess
            .partition_point(|(end, _)| *end <= offset);
        self.unicode_excess
            .get(index)
            .is_none_or(|(end, cumulative)| {
                let previous = index
                    .checked_sub(1)
                    .map_or(0, |previous| self.unicode_excess[previous].1);
                let scalar_start = end - (cumulative - previous + 1);
                offset <= scalar_start
            })
    }
}

/// Returns physical line starts for Python's LF, CRLF and lone-CR line endings.
/// Offsets refer to the unchanged input bytes, including any leading UTF-8 BOM.
///
/// # Errors
///
/// Returns [`CandidateValidationError::SourceTooLarge`] before scanning when the
/// source exceeds `u32::MAX` bytes.
pub fn python_line_starts(source: &[u8]) -> Result<Vec<u32>, CandidateValidationError> {
    source_line_starts(
        source.len(),
        source.iter().enumerate().filter_map(|(offset, byte)| {
            (*byte == b'\n' || (*byte == b'\r' && source.get(offset + 1) != Some(&b'\n')))
                .then_some(offset + 1)
        }),
    )
}

fn source_line_starts(
    source_len: usize,
    newline_ends: impl Iterator<Item = usize>,
) -> Result<Vec<u32>, CandidateValidationError> {
    u32::try_from(source_len).map_err(|_| CandidateValidationError::SourceTooLarge)?;
    let mut line_starts = vec![0];
    for offset in newline_ends {
        line_starts
            .push(u32::try_from(offset).map_err(|_| CandidateValidationError::SourceTooLarge)?);
    }
    Ok(line_starts)
}

/// Returns the zero-based Python source column for a checked byte range.
///
/// Columns count Unicode scalar values. A single UTF-8 BOM is excluded only
/// when it begins the file; byte offsets and the source itself remain unchanged.
/// `None` is returned when either bound is outside `source`, the range is
/// reversed, a bound is not a UTF-8 character boundary, or the column cannot
/// be represented as `u32`.
#[must_use]
pub fn python_source_column(source: &str, line_start: usize, offset: usize) -> Option<u32> {
    let prefix = source.get(line_start..offset)?;
    let visible_prefix = if line_start == 0 {
        prefix.strip_prefix('\u{feff}').unwrap_or(prefix)
    } else {
        prefix
    };
    u32::try_from(visible_prefix.chars().count()).ok()
}

#[must_use]
pub fn stable_mutant_id(identity: &CandidateIdentity) -> MutantId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(MUTANT_ID_DOMAIN);
    framed(
        &mut hasher,
        b"schema",
        &identity.schema_version.to_le_bytes(),
    );
    framed(
        &mut hasher,
        b"source-blake3",
        identity.file_hash.to_ascii_lowercase().as_bytes(),
    );
    let path = canonical_identity_path(identity.path.as_str());
    framed(&mut hasher, b"path", path.as_bytes());
    framed(
        &mut hasher,
        b"span-start",
        &identity.span.start.to_le_bytes(),
    );
    framed(
        &mut hasher,
        b"span-length",
        &identity.span.length.to_le_bytes(),
    );
    framed(&mut hasher, b"operator", identity.operator.as_bytes());
    framed(&mut hasher, b"replacement", identity.replacement.as_bytes());
    MutantId(format!("m1_{}", hasher.finalize().to_hex()))
}

fn framed(hasher: &mut blake3::Hasher, tag: &[u8], value: &[u8]) {
    hasher.update(&(tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

/// # Errors
///
/// Returns [`CandidateValidationError`] when the candidate metadata, path, hash, span, source text, or reported location is invalid.
///
pub fn validate_candidate(
    source: &[u8],
    candidate: &CandidateDescriptor,
) -> Result<MutantId, CandidateValidationError> {
    validate_candidate_with_context(&CandidateValidationContext::new(source)?, candidate)
}

/// Validates a candidate using source facts shared by a batch from the same file.
///
/// # Errors
///
/// Returns [`CandidateValidationError`] when the candidate metadata, path, hash, span, source text, or reported location is invalid.
pub fn validate_candidate_with_context(
    context: &CandidateValidationContext<'_>,
    candidate: &CandidateDescriptor,
) -> Result<MutantId, CandidateValidationError> {
    if candidate.schema_version != CANDIDATE_SCHEMA_VERSION {
        return Err(CandidateValidationError::UnsupportedSchema);
    }
    if !normalized_relative_path(candidate.path.as_str()) {
        return Err(CandidateValidationError::InvalidPath);
    }
    if candidate.file_hash.len() != 64 || candidate.file_hash != context.file_hash {
        return Err(CandidateValidationError::FileHashMismatch);
    }
    if candidate.operator.is_empty() || candidate.original == candidate.replacement {
        return Err(CandidateValidationError::InvalidMutation);
    }

    let start = usize::try_from(candidate.span.start)
        .map_err(|_| CandidateValidationError::SpanOutOfBounds)?;
    let length = usize::try_from(candidate.span.length)
        .map_err(|_| CandidateValidationError::SpanOutOfBounds)?;
    let end = start
        .checked_add(length)
        .filter(|end| *end <= context.source.len())
        .ok_or(CandidateValidationError::SpanOutOfBounds)?;
    if context.source.get(start..end) != Some(candidate.original.as_bytes()) {
        return Err(CandidateValidationError::OriginalMismatch);
    }

    let text = context
        .text
        .map_err(|_| CandidateValidationError::InvalidUtf8)?;
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return Err(CandidateValidationError::OriginalMismatch);
    }
    let (line, column) = context
        .source_index
        .as_ref()
        .and_then(|index| index.line_and_column(start))
        .ok_or(CandidateValidationError::LocationMismatch)?;
    if candidate.line != line || candidate.column != column {
        return Err(CandidateValidationError::LocationMismatch);
    }
    Ok(stable_mutant_id(&CandidateIdentity::from(candidate)))
}

pub fn normalized_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return false;
    }
    path.split('/').all(valid_path_part)
}

fn valid_path_part(part: &str) -> bool {
    !part.is_empty() && part != "." && part != ".." && !part.contains(':')
}

fn canonical_identity_path(path: &str) -> String {
    let slash_path = path.replace('\\', "/");
    let mut normalized = String::with_capacity(slash_path.len());
    if slash_path.starts_with('/') {
        normalized.push('/');
    }
    for part in slash_path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
    {
        if !normalized.is_empty() && !normalized.ends_with('/') {
            normalized.push('/');
        }
        normalized.push_str(part);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{CandidateValidationError, PythonSourceIndex, source_line_starts};

    #[test]
    fn ascii_source_uses_no_unicode_correction_entries() {
        let source = "True,".repeat(64_000);
        let index = PythonSourceIndex::new(&source).unwrap();

        assert!(index.unicode_excess.is_empty());
        for offset in [0, 4, source.len() / 2, source.len()] {
            assert_eq!(
                index.line_and_column(offset),
                Some((1, u32::try_from(offset).unwrap()))
            );
        }
    }

    #[test]
    fn line_index_preserves_empty_and_trailing_newline_sources() {
        assert_eq!(source_line_starts(0, std::iter::empty()).unwrap(), [0]);
        assert_eq!(
            source_line_starts(4, [2, 4].into_iter()).unwrap(),
            [0, 2, 4]
        );
    }

    #[test]
    fn line_index_accepts_the_largest_representable_offset() {
        let max = usize::try_from(u32::MAX).unwrap();
        assert_eq!(
            source_line_starts(max, [max].into_iter()).unwrap(),
            [0, u32::MAX]
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn oversized_source_is_rejected_before_scanning_newlines() {
        let too_large = usize::try_from(u32::MAX).unwrap() + 1;
        let newline_ends = std::iter::from_fn(|| -> Option<usize> {
            panic!("oversized sources must be rejected before scanning")
        });
        assert_eq!(
            source_line_starts(too_large, newline_ends),
            Err(CandidateValidationError::SourceTooLarge)
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn oversized_source_without_newlines_is_rejected() {
        let too_large = usize::try_from(u32::MAX).unwrap() + 1;
        assert_eq!(
            source_line_starts(too_large, std::iter::empty()),
            Err(CandidateValidationError::SourceTooLarge)
        );
    }
}
