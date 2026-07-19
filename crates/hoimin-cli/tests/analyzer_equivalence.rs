#![allow(dead_code)]

#[path = "../src/analyzer/protocol.rs"]
mod protocol;

pub use protocol::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};

mod analyzer {
    pub use crate::protocol::AnalyzerDiagnosticCode;
}

#[path = "../src/analyzer/rust.rs"]
mod rust;

use std::io::Write;
use std::process::{Command, Stdio};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::LineRange;
use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/analyzer_parity.py");

#[derive(Debug, Eq, PartialEq)]
struct ComparableCandidate {
    sequence: usize,
    span_start: u64,
    span_length: u64,
    original: String,
    replacement: String,
    operator: String,
    line: u32,
    column: u32,
    symbol: Option<String>,
}

#[derive(Deserialize)]
struct PythonRecord {
    kind: String,
    span: Option<PythonSpan>,
    original: Option<String>,
    replacement: Option<String>,
    operator: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
    #[serde(default)]
    symbol: serde_json::Value,
}

#[derive(Deserialize)]
struct PythonSpan {
    start: u64,
    length: u64,
}

#[test]
fn rust_and_python_helpers_emit_identical_candidates() {
    for path in ["pkg/sample.py", "pkg/sub/__init__.py"] {
        let rust_candidates = rust_candidates(path);
        let python_candidates = python_candidates(FIXTURE, path);
        assert_eq!(
            rust_candidates, python_candidates,
            "candidate mismatch for fixture {path}"
        );
    }
}

fn rust_candidates(path: &str) -> Vec<ComparableCandidate> {
    rust::analyze_source(
        &rust::AnalyzeRequest {
            path: Utf8Path::new(path),
            lines: &[] as &[LineRange],
            symbols: &[],
            max_candidates: 10_000,
        },
        FIXTURE,
    )
    .candidates
    .into_iter()
    .enumerate()
    .map(|(index, candidate)| ComparableCandidate {
        sequence: index + 1,
        span_start: candidate.span.start,
        span_length: candidate.span.length,
        original: candidate.original,
        replacement: candidate.replacement,
        operator: candidate.operator,
        line: candidate.line,
        column: candidate.column,
        symbol: candidate.symbol,
    })
    .collect()
}

fn python_candidates(source: &str, path: &str) -> Vec<ComparableCandidate> {
    let request = serde_json::json!({
        "effect_id": 1,
        "path": path,
        "module": source,
        "lines": [],
        "symbols": [],
        "max_candidates": 10_000,
    });
    let mut child = Command::new(python_executable())
        .arg(repo_root().join("python/hoimin_analyzer.py"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start Python analyzer helper");
    child
        .stdin
        .as_mut()
        .expect("Python helper stdin")
        .write_all(format!("{request}\n").as_bytes())
        .expect("send JSONL request to Python helper");
    let output = child
        .wait_with_output()
        .expect("read Python analyzer helper output");
    assert!(
        output.status.success(),
        "Python analyzer helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let record: PythonRecord = serde_json::from_slice(line).expect("valid Python JSONL");
            (record.kind == "candidate").then_some(record)
        })
        .enumerate()
        .map(|(index, record)| {
            let span = record.span.expect("candidate span");
            ComparableCandidate {
                sequence: index + 1,
                span_start: span.start,
                span_length: span.length,
                original: record.original.expect("candidate original"),
                replacement: record.replacement.expect("candidate replacement"),
                operator: record.operator.expect("candidate operator"),
                line: record.line.expect("candidate line"),
                column: record.column.expect("candidate column"),
                symbol: match record.symbol {
                    serde_json::Value::Null => None,
                    serde_json::Value::String(symbol) => Some(symbol),
                    value => panic!("candidate symbol must be a string or null: {value}"),
                },
            }
        })
        .collect()
}

fn python_executable() -> Utf8PathBuf {
    if let Some(path) = std::env::var_os("HOIMIN_TEST_PYTHON") {
        return Utf8PathBuf::from_path_buf(path.into()).expect("HOIMIN_TEST_PYTHON is UTF-8");
    }
    let root = repo_root();
    let windows = root.join(".venv/Scripts/python.exe");
    let unix = root.join(".venv/bin/python");
    [windows, unix]
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| {
            panic!(
                "set HOIMIN_TEST_PYTHON to a Python with LibCST and pytest: {}",
                root
            )
        })
}

fn repo_root() -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root")
            .to_owned(),
    )
    .expect("workspace root is UTF-8")
}
