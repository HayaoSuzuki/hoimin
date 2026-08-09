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
    line_starts: Vec<usize>,
}

impl<'source> CandidateValidationContext<'source> {
    #[must_use]
    pub fn new(source: &'source [u8]) -> Self {
        let mut line_starts = Vec::with_capacity(
            source
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                .saturating_add(1),
        );
        line_starts.push(0);
        line_starts.extend(
            source
                .iter()
                .enumerate()
                .filter_map(|(offset, byte)| (*byte == b'\n').then_some(offset + 1)),
        );
        Self {
            source,
            text: std::str::from_utf8(source),
            file_hash: blake3::hash(source).to_hex().to_string(),
            line_starts,
        }
    }

    #[must_use]
    pub fn file_hash(&self) -> &str {
        &self.file_hash
    }
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
    validate_candidate_with_context(&CandidateValidationContext::new(source), candidate)
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
    let line_index = context
        .line_starts
        .partition_point(|line_start| *line_start <= start)
        .saturating_sub(1);
    let line =
        u32::try_from(line_index + 1).map_err(|_| CandidateValidationError::LocationMismatch)?;
    let line_start = context.line_starts[line_index];
    let column = u32::try_from(text[line_start..start].chars().count())
        .map_err(|_| CandidateValidationError::LocationMismatch)?;
    if candidate.line != line || candidate.column != column {
        return Err(CandidateValidationError::LocationMismatch);
    }
    Ok(stable_mutant_id(&CandidateIdentity::from(candidate)))
}

pub fn normalized_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return false;
    }
    let mut parts = path.split('/');
    let Some(first) = parts.next() else {
        return false;
    };
    !first.contains(':') && valid_path_part(first) && parts.all(valid_path_part)
}

fn valid_path_part(part: &str) -> bool {
    !part.is_empty() && part != "." && part != ".."
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
