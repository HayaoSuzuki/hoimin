"""Temporarily observe the actual token-view allocation; always restore source."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

mode = sys.argv[1]
assert mode in {"before", "after"}
source_path = Path("crates/hoimin-python/src/analyzer/rust.rs")
original = source_path.read_bytes()
source = (subprocess.check_output(["git", "show", f"11eb28d3:{source_path}"])
          if mode == "before" else original).decode()
needle = ("    let tokens: Vec<_> = parsed.tokens().iter().collect();" if mode == "before"
          else "    let tokens: &[ruff_python_ast::token::Token] = parsed.tokens();")
assert source.count(needle) == 1
source = source.replace(needle, "    #[cfg(test)]\n    issue_1086_token_alloc_probe::begin();\n" + needle +
                        "\n    #[cfg(test)]\n    issue_1086_token_alloc_probe::finish(tokens.len());", 1)
helper = Path("crates/hoimin-cli/tests/support/heap_tracking.rs").resolve().as_posix()
module = r'''
#[cfg(test)]
mod issue_1086_token_alloc_probe {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[allow(unsafe_op_in_unsafe_fn)]
    #[path = "__HELPER__"]
    mod heap;
    #[global_allocator]
    static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
    static BYTES: AtomicUsize = AtomicUsize::new(0);
    static TOKENS: AtomicUsize = AtomicUsize::new(0);
    pub(super) fn begin() { heap::begin(); }
    pub(super) fn finish(count: usize) {
        BYTES.store(heap::finish(), Ordering::Relaxed);
        TOKENS.store(count, Ordering::Relaxed);
    }
    #[test]
    fn actual_token_view_uses_no_owned_reference_buffer() {
        let unit = "def selected(left, right):\n    a = -1 + 2\n    b = not (left is not right or left not in right)\n    c = f'顔 {left}'\n    d = t'顔 {right}'\n    return a, b, c, d\n";
        let mut measured = Vec::new();
        for target in [1024, 4096, 16384] {
            let source = unit.repeat((target / unit.len()).max(1));
            let operators = MutationOperatorSelection::default();
            let request = AnalyzeRequest { payload: PayloadBudget::default(),
                path: Utf8Path::new("subject.py"), lines: &[], symbols: &[],
                operators: &operators, profile: MutationProfile::Full, max_candidates: 4 };
            let output = analyze_source_cancellable(&request, &source, || false).unwrap();
            let bytes = BYTES.load(Ordering::Relaxed);
            let tokens = TOKENS.load(Ordering::Relaxed);
            assert!(tokens > 0);
            assert!(!output.candidates.is_empty());
            let file_hash = blake3::hash(source.as_bytes()).to_hex().to_string();
            let candidates: Vec<_> = output.candidates.iter().map(|candidate| {
                let identity = hoimin_core::CandidateIdentity {
                    schema_version: hoimin_core::CANDIDATE_SCHEMA_VERSION,
                    file_hash: file_hash.clone(), path: candidate.path.clone(), span: candidate.span,
                    operator: candidate.operator.as_str().to_owned(), replacement: candidate.replacement.clone()
                };
                serde_json::json!({"id": hoimin_core::stable_mutant_id(&identity).as_str(),
                    "path": candidate.path, "span": candidate.span, "original": candidate.original,
                    "replacement": candidate.replacement, "operator": candidate.operator.as_str(),
                    "line": candidate.line, "column": candidate.column, "symbol": candidate.symbol})
            }).collect();
            let diagnostics: Vec<_> = output.diagnostics.iter().map(|item| serde_json::json!({
                "code": item.code.canonical_name(), "path": item.path, "line": item.line,
                "column": item.column, "message": item.message })).collect();
            eprintln!("TOKEN_VIEW_OBSERVATION {}", serde_json::json!({"source_bytes": source.len(),
                "tokens": tokens, "peak": bytes, "pointer_bytes": std::mem::size_of::<&ruff_python_ast::token::Token>(),
                "candidates": candidates, "diagnostics": diagnostics, "truncated": output.truncated}));
            measured.push(bytes);
            assert!(matches!(analyze_source_cancellable(&request, &source, || true), Err(AnalysisError::Cancelled)));
        }
        assert!(measured.iter().all(|bytes| *bytes == 0), "owning token view bytes: {measured:?}");
    }
}
'''.replace("__HELPER__", helper)
evidence = Path(__file__).resolve().parent
log = evidence / f"token-view-{mode}.log"
log.parent.mkdir(parents=True, exist_ok=True)
try:
    source_path.write_text(source + module)
    env = os.environ | {"CARGO_BUILD_JOBS": "2", "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_INCREMENTAL": "0"}
    with log.open("w") as stream:
        result = subprocess.run(["cargo", "test", "--offline", "--locked", "-p", "hoimin-python", "--lib",
                                 "issue_1086_token_alloc_probe", "--", "--nocapture"],
                                env=env, stdout=stream, stderr=subprocess.STDOUT, check=False)
finally:
    source_path.write_bytes(original)
    print("Restored exact original source bytes", flush=True)
rows = [json.loads(match.group(1)) for match in re.finditer(r'TOKEN_VIEW_OBSERVATION (\{[^\n]+\})', log.read_text())]
assert len(rows) == 3, log
for row in rows:
    assert row["peak"] == (row["tokens"] * row["pointer_bytes"] if mode == "before" else 0), row
    print({key: row[key] for key in ["source_bytes", "tokens", "peak"]})
assert result.returncode == (101 if mode == "before" else 0), result.returncode
(evidence / f"token-view-{mode}.json").write_text(json.dumps(rows, indent=2))
if mode == "after":
    before = json.loads((evidence / "token-view-before.json").read_text())
    assert [{k:v for k,v in row.items() if k != "peak"} for row in before] == [
        {k:v for k,v in row.items() if k != "peak"} for row in rows]
    print("Candidate IDs, descriptors, diagnostics and truncation match baseline exactly")
