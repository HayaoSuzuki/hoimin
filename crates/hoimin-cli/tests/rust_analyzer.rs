#![allow(dead_code)]

#[path = "../src/analyzer/protocol.rs"]
mod protocol;

pub use protocol::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

mod analyzer {
    pub use crate::protocol::AnalyzerDiagnosticCode;
}

#[path = "../src/analyzer/rust.rs"]
mod rust;
