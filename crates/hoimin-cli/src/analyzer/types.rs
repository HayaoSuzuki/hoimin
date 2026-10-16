//! In-memory results shared by the Rust analyzer and its callers.

use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, MutationOperator};

/// Diagnostics emitted by the current analyzer; saved diagnostic codes remain strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalyzerDiagnosticCode {
    InvalidSyntax,
    UnsupportedExceptionHierarchy,
    UnsupportedEnumDefinition,
    CandidateLimitExceeded,
}

impl AnalyzerDiagnosticCode {
    /// Returns the stable external name without a caller-specific prefix.
    pub(crate) const fn canonical_name(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "invalid_syntax",
            Self::UnsupportedExceptionHierarchy => "exception_hierarchy_skipped",
            Self::UnsupportedEnumDefinition => "enum_definition_skipped",
            Self::CandidateLimitExceeded => "candidate_limit",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyzerCandidate {
    pub path: Utf8PathBuf,
    pub span: ByteSpan,
    pub original: String,
    pub replacement: String,
    /// Known identity; saved descriptors convert this to its canonical name.
    pub operator: MutationOperator,
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
