//! In-memory results shared by the Rust analyzer and its callers.

use camino::Utf8PathBuf;
use hoimin_core::ByteSpan;

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
