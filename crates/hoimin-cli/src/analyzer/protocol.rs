use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, EffectId, MutationOperator, normalized_relative_path};
use serde::Deserialize;
use thiserror::Error;

pub const DEFAULT_MAX_ANALYZER_LINE_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_MAX_ANALYZER_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalyzerDiagnosticCode {
    InvalidSyntax,
    UnsupportedExceptionHierarchy,
    UnsupportedEnumDefinition,
    UnreconstructableSpan,
    UnparseableReplacement,
    CandidateLimitExceeded,
    InvalidRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyzerCandidate {
    pub path: Utf8PathBuf,
    pub span: ByteSpan,
    pub original: String,
    pub replacement: String,
    pub operator: String,
    pub line: u32,
    pub column: u32,
    pub symbol: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyzerDiagnostic {
    pub code: AnalyzerDiagnosticCode,
    pub path: Option<Utf8PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisStatus {
    Complete,
    AnalysisLimitReached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyzerSummary {
    pub candidate_count: u64,
    pub diagnostic_count: u64,
    pub truncated: bool,
}

impl AnalyzerSummary {
    #[must_use]
    pub fn status(&self) -> AnalysisStatus {
        if self.truncated {
            AnalysisStatus::AnalysisLimitReached
        } else {
            AnalysisStatus::Complete
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalyzerRecord {
    Candidate(AnalyzerCandidate),
    Diagnostic(AnalyzerDiagnostic),
    Summary(AnalyzerSummary),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProtocolError {
    #[error("analyzer JSONL line exceeds {limit} bytes")]
    LineTooLarge { limit: usize },
    #[error("analyzer output exceeds {limit} bytes")]
    OutputTooLarge { limit: usize },
    #[error("malformed analyzer JSON: {0}")]
    MalformedJson(String),
    #[error("invalid analyzer record: {0}")]
    InvalidRecord(&'static str),
    #[error("analyzer effect ID mismatch: expected {expected:?}, got {actual:?}")]
    EffectIdMismatch {
        expected: EffectId,
        actual: EffectId,
    },
    #[error("analyzer candidate order is not stable")]
    CandidateOrder,
    #[error("analyzer summary counts do not match received records")]
    CountMismatch {
        expected_candidates: u64,
        actual_candidates: u64,
        expected_diagnostics: u64,
        actual_diagnostics: u64,
    },
    #[error("analyzer record received after summary")]
    RecordAfterSummary,
    #[error("analyzer output ended without a summary")]
    MissingSummary,
}

#[derive(Debug)]
pub struct AnalyzerProtocol {
    expected_effect_id: EffectId,
    max_line_bytes: usize,
    max_output_bytes: usize,
    observed_output_bytes: usize,
    candidate_count: u64,
    diagnostic_count: u64,
    last_candidate_start: Option<u64>,
    limit_diagnostic_seen: bool,
    summary: Option<AnalyzerSummary>,
}

impl AnalyzerProtocol {
    #[must_use]
    pub fn new(expected_effect_id: EffectId) -> Self {
        Self::with_limits(
            expected_effect_id,
            DEFAULT_MAX_ANALYZER_LINE_BYTES,
            DEFAULT_MAX_ANALYZER_OUTPUT_BYTES,
        )
    }

    #[must_use]
    pub fn with_limits(
        expected_effect_id: EffectId,
        max_line_bytes: usize,
        max_output_bytes: usize,
    ) -> Self {
        Self {
            expected_effect_id,
            max_line_bytes,
            max_output_bytes,
            observed_output_bytes: 0,
            candidate_count: 0,
            diagnostic_count: 0,
            last_candidate_start: None,
            limit_diagnostic_seen: false,
            summary: None,
        }
    }

    /// # Errors
    ///
    /// Returns an error when the line or cumulative output is too large, malformed, addressed
    /// to another effect, or violates the analyzer record protocol.
    pub fn receive_line(&mut self, line: &[u8]) -> Result<Option<AnalyzerRecord>, ProtocolError> {
        if self.summary.is_some() {
            return Err(ProtocolError::RecordAfterSummary);
        }
        if line.len() > self.max_line_bytes {
            return Err(ProtocolError::LineTooLarge {
                limit: self.max_line_bytes,
            });
        }
        self.observed_output_bytes = self.observed_output_bytes.checked_add(line.len()).ok_or(
            ProtocolError::OutputTooLarge {
                limit: self.max_output_bytes,
            },
        )?;
        if self.observed_output_bytes > self.max_output_bytes {
            return Err(ProtocolError::OutputTooLarge {
                limit: self.max_output_bytes,
            });
        }
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() || line.contains(&b'\n') || line.contains(&b'\r') {
            return Err(ProtocolError::InvalidRecord(
                "expected one complete non-empty line",
            ));
        }
        let raw: RawRecord = serde_json::from_slice(line)
            .map_err(|error| ProtocolError::MalformedJson(error.to_string()))?;
        if raw.effect_id != self.expected_effect_id {
            return Err(ProtocolError::EffectIdMismatch {
                expected: self.expected_effect_id,
                actual: raw.effect_id,
            });
        }
        match raw.kind.as_str() {
            "candidate" => self.receive_candidate(raw).map(Some),
            "diagnostic" => self.receive_diagnostic(raw).map(Some),
            "summary" => self.receive_summary(raw).map(Some),
            _ => Err(ProtocolError::InvalidRecord("unknown kind")),
        }
    }

    /// # Errors
    ///
    /// Returns an error when no summary record was received.
    pub fn finish(self) -> Result<AnalyzerSummary, ProtocolError> {
        self.summary.ok_or(ProtocolError::MissingSummary)
    }

    fn receive_candidate(&mut self, raw: RawRecord) -> Result<AnalyzerRecord, ProtocolError> {
        if self.limit_diagnostic_seen {
            return Err(ProtocolError::InvalidRecord(
                "record after candidate limit diagnostic",
            ));
        }
        if raw.code.is_present()
            || raw.message.is_present()
            || raw.candidate_count.is_present()
            || raw.diagnostic_count.is_present()
            || raw.truncated.is_present()
        {
            return Err(ProtocolError::InvalidRecord(
                "candidate contains fields for another record kind",
            ));
        }
        let path = raw.path.required("candidate path is required")?;
        let span: ByteSpan = raw.span.required("candidate span is required")?.into();
        let original = raw.original.required("candidate original is required")?;
        let replacement = raw
            .replacement
            .required("candidate replacement is required")?;
        let operator = raw.operator.required("candidate operator is required")?;
        let line = raw.line.required("candidate line is required")?;
        let column = raw.column.required("candidate column is required")?;
        let symbol = raw
            .symbol
            .nullable_required("candidate symbol is required")?;
        if !normalized_relative_path(path.as_str())
            || line == 0
            || original.len() as u64 != span.length
            || span.start.checked_add(span.length).is_none()
            || original == replacement
            || !known_operator(&operator)
        {
            return Err(ProtocolError::InvalidRecord("invalid candidate fields"));
        }
        if self
            .last_candidate_start
            .is_some_and(|start| span.start < start)
        {
            return Err(ProtocolError::CandidateOrder);
        }
        self.last_candidate_start = Some(span.start);
        self.candidate_count = self
            .candidate_count
            .checked_add(1)
            .ok_or(ProtocolError::InvalidRecord("candidate count overflow"))?;
        Ok(AnalyzerRecord::Candidate(AnalyzerCandidate {
            path,
            span,
            original,
            replacement,
            operator,
            line,
            column,
            symbol,
        }))
    }

    fn receive_diagnostic(&mut self, raw: RawRecord) -> Result<AnalyzerRecord, ProtocolError> {
        if raw.span.is_present()
            || raw.original.is_present()
            || raw.replacement.is_present()
            || raw.operator.is_present()
            || raw.symbol.is_present()
            || raw.candidate_count.is_present()
            || raw.diagnostic_count.is_present()
            || raw.truncated.is_present()
        {
            return Err(ProtocolError::InvalidRecord(
                "diagnostic contains fields for another record kind",
            ));
        }
        let code = match raw.code.value().map(String::as_str) {
            Some("unsupported_exception_hierarchy") => {
                AnalyzerDiagnosticCode::UnsupportedExceptionHierarchy
            }
            Some("unsupported_enum_definition") => {
                AnalyzerDiagnosticCode::UnsupportedEnumDefinition
            }
            Some("invalid_syntax") => AnalyzerDiagnosticCode::InvalidSyntax,
            Some("unreconstructable_span") => AnalyzerDiagnosticCode::UnreconstructableSpan,
            Some("unparseable_replacement") => AnalyzerDiagnosticCode::UnparseableReplacement,
            Some("candidate_limit_exceeded") => AnalyzerDiagnosticCode::CandidateLimitExceeded,
            Some("invalid_request") => AnalyzerDiagnosticCode::InvalidRequest,
            _ => return Err(ProtocolError::InvalidRecord("unknown diagnostic code")),
        };
        let has_location = raw.line.is_present() || raw.column.is_present();
        let fields_match = match code {
            AnalyzerDiagnosticCode::UnsupportedEnumDefinition => {
                raw.path.value().is_some()
                    && raw.line.value().is_some()
                    && raw.column.value().is_some()
                    && raw.message.value().is_some()
            }
            AnalyzerDiagnosticCode::UnsupportedExceptionHierarchy => {
                raw.path.value().is_some() && !has_location && raw.message.value().is_some()
            }
            AnalyzerDiagnosticCode::InvalidRequest => {
                !raw.path.is_present() && !has_location && raw.message.value().is_some()
            }
            AnalyzerDiagnosticCode::UnreconstructableSpan
            | AnalyzerDiagnosticCode::UnparseableReplacement => {
                raw.path.value().is_some()
                    && raw.line.value().is_some()
                    && raw.column.value().is_some()
                    && !raw.message.is_present()
            }
            AnalyzerDiagnosticCode::InvalidSyntax
            | AnalyzerDiagnosticCode::CandidateLimitExceeded => {
                raw.path.value().is_some() && !has_location && !raw.message.is_present()
            }
        };
        if !fields_match || raw.line.value() == Some(&0) {
            return Err(ProtocolError::InvalidRecord("invalid diagnostic fields"));
        }
        if raw
            .path
            .value()
            .is_some_and(|path| !normalized_relative_path(path.as_str()))
        {
            return Err(ProtocolError::InvalidRecord("invalid diagnostic path"));
        }
        if code == AnalyzerDiagnosticCode::CandidateLimitExceeded {
            if self.limit_diagnostic_seen {
                return Err(ProtocolError::InvalidRecord(
                    "duplicate candidate limit diagnostic",
                ));
            }
            self.limit_diagnostic_seen = true;
        } else if self.limit_diagnostic_seen {
            return Err(ProtocolError::InvalidRecord(
                "record after candidate limit diagnostic",
            ));
        }
        self.diagnostic_count = self
            .diagnostic_count
            .checked_add(1)
            .ok_or(ProtocolError::InvalidRecord("diagnostic count overflow"))?;
        Ok(AnalyzerRecord::Diagnostic(AnalyzerDiagnostic {
            code,
            path: raw.path.into_value(),
            line: raw.line.into_value(),
            column: raw.column.into_value(),
            message: raw.message.into_value(),
        }))
    }

    fn receive_summary(&mut self, raw: RawRecord) -> Result<AnalyzerRecord, ProtocolError> {
        if raw.path.is_present()
            || raw.span.is_present()
            || raw.original.is_present()
            || raw.replacement.is_present()
            || raw.operator.is_present()
            || raw.line.is_present()
            || raw.column.is_present()
            || raw.symbol.is_present()
            || raw.code.is_present()
            || raw.message.is_present()
        {
            return Err(ProtocolError::InvalidRecord(
                "summary contains fields for another record kind",
            ));
        }
        let candidate_count = raw
            .candidate_count
            .required("summary candidate_count is required")?;
        let diagnostic_count = raw
            .diagnostic_count
            .required("summary diagnostic_count is required")?;
        let truncated = raw.truncated.required("summary truncated is required")?;
        if candidate_count != self.candidate_count || diagnostic_count != self.diagnostic_count {
            return Err(ProtocolError::CountMismatch {
                expected_candidates: self.candidate_count,
                actual_candidates: candidate_count,
                expected_diagnostics: self.diagnostic_count,
                actual_diagnostics: diagnostic_count,
            });
        }
        if truncated != self.limit_diagnostic_seen {
            return Err(ProtocolError::InvalidRecord(
                "truncation status does not match diagnostics",
            ));
        }
        let summary = AnalyzerSummary {
            candidate_count,
            diagnostic_count,
            truncated,
        };
        self.summary = Some(summary.clone());
        Ok(AnalyzerRecord::Summary(summary))
    }
}

fn known_operator(operator: &str) -> bool {
    MutationOperator::from_name(operator).is_some_and(|parsed| parsed.as_str() == operator)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRecord {
    kind: String,
    effect_id: EffectId,
    #[serde(default)]
    path: RawField<Utf8PathBuf>,
    #[serde(default)]
    span: RawField<RawSpan>,
    #[serde(default)]
    original: RawField<String>,
    #[serde(default)]
    replacement: RawField<String>,
    #[serde(default)]
    operator: RawField<String>,
    #[serde(default)]
    line: RawField<u32>,
    #[serde(default)]
    column: RawField<u32>,
    #[serde(default)]
    symbol: RawField<String>,
    #[serde(default)]
    code: RawField<String>,
    #[serde(default)]
    message: RawField<String>,
    #[serde(default)]
    candidate_count: RawField<u64>,
    #[serde(default)]
    diagnostic_count: RawField<u64>,
    #[serde(default)]
    truncated: RawField<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpan {
    start: u64,
    length: u64,
}

impl From<RawSpan> for ByteSpan {
    fn from(span: RawSpan) -> Self {
        Self {
            start: span.start,
            length: span.length,
        }
    }
}

#[derive(Debug, Default)]
enum RawField<T> {
    #[default]
    Missing,
    Present(Option<T>),
}

impl<T> RawField<T> {
    fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }

    fn value(&self) -> Option<&T> {
        match self {
            Self::Present(Some(value)) => Some(value),
            Self::Missing | Self::Present(None) => None,
        }
    }

    fn into_value(self) -> Option<T> {
        match self {
            Self::Present(value) => value,
            Self::Missing => None,
        }
    }

    fn required(self, message: &'static str) -> Result<T, ProtocolError> {
        self.into_value()
            .ok_or(ProtocolError::InvalidRecord(message))
    }

    fn nullable_required(self, message: &'static str) -> Result<Option<T>, ProtocolError> {
        match self {
            Self::Present(value) => Ok(value),
            Self::Missing => Err(ProtocolError::InvalidRecord(message)),
        }
    }
}

impl<'de, T> Deserialize<'de> for RawField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Self::Present)
    }
}
