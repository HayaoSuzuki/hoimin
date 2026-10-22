//! Internal pure Python analysis shared by the CLI, corpus tests and fuzzing.
//!
//! Results retain their logical payload leases until conversion or terminal handoff.
//! Filesystem acquisition, project fingerprints and candidate spools belong to callers.

#[path = "analyzer/rust.rs"]
mod rust;
#[path = "analyzer/types.rs"]
mod types;
#[path = "analyzer/visitor.rs"]
mod visitor;

pub use rust::exception_hierarchy::{ExceptionIndex, ExceptionIndexBuilder, MAX_SUMMARY_ENTRIES};
pub use rust::{
    AnalysisError, AnalyzeRequest, AnalyzerOutput, analyze_source_cancellable,
    analyze_source_with_exceptions, definition_names, payload,
};
pub use types::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

#[cfg(test)]
#[path = "analyzer/payload_tests.rs"]
mod payload_tests;
