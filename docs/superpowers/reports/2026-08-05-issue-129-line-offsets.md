# Issue 129: Analyzer line-offset performance report

## Scope and revisions

Issue #129 precomputes line-start byte offsets once for an analyzed Python
source, then shares that index between token and type-annotation candidate
position lookups. The baseline harness was committed as
`e9578a1 test(analyzer): benchmark candidate position lookup`; the optimized
revision measured here is `caed967 perf(analyzer): precompute line offsets`.

The benchmark fixture is deterministic Python consisting of 7,680 fixed-width
assignment lines:

```text
result_{index:05} = left_{index:05} + right_{index:05}\n
```

It is exactly 307,200 bytes (300 KiB) and must produce exactly 7,680 `+`
candidates. The ignored benchmark times one real `analyze` call, sends its
output through `black_box`, asserts that count, and reports the source size,
candidate count, and elapsed time.

## Measurements

All runs used the single-target command below so the shared test module is not
also executed via `tests/rust_analyzer.rs`:

```console
cargo test --release -p hoimin-cli --lib benchmark_candidate_line_positions -- --ignored --nocapture
```

The five baseline runs at `e9578a1` were supplied by Task 1. The five optimized
runs below were captured at `caed967`; every benchmark invocation exited zero
and printed `source_bytes=307200 candidates=7680`.

| Run | Baseline (ms) | Optimized (ms) |
| --- | ---: | ---: |
| 1 | 177.892959 | 5.408000 |
| 2 | 183.387125 | 3.701584 |
| 3 | 179.590125 | 4.661875 |
| 4 | 178.706000 | 3.793500 |
| 5 | 180.916167 | 4.734083 |
| Median | 179.590125 | 4.661875 |

The optimized median is 97.404% lower than the baseline median:
`(1 - 4.661875 / 179.590125) * 100`. Equivalently, the measured sequential
workload is 38.52x faster (`179.590125 / 4.661875`).

These are sequential elapsed-time measurements. They support the performance
claim, but are not a CI threshold: host load, compiler version, and scheduling
can affect elapsed time. The ignored benchmark and its deterministic candidate
count provide reproducible supporting evidence; correctness remains guarded by
ordinary tests rather than a wall-clock limit.

## Correctness evidence

Fresh focused checks after the benchmark-fixture repair passed:

```console
cargo test -p hoimin-cli analyzer::rust::rust_tests --lib
# 33 passed; 0 failed; 1 ignored

cargo test -p hoimin-cli --test rust_analyzer
# 33 passed; 0 failed; 1 ignored
```

This covers the new line-index cases (one-based lines, Unicode-scalar columns,
CRLF, later lines, and token/type-annotation candidate tuples) in both test
targets. The release benchmark additionally asserted the unchanged 7,680
candidate count on every timing run.

## Full verification results

The requested commands were run fresh after temporarily linking this isolated
worktree's `.venv` entry to the existing managed project environment. The link
was removed after testing; no tracked source or test file was changed for setup.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed (exit 0). |
| `cargo test --workspace` | Passed (exit 0; all workspace tests, including 46/46 `run_e2e`, passed). |
| `cargo test --workspace --all-features` | Passed (exit 0; all workspace tests, including 46/46 `run_e2e`, passed). |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed (exit 0). The fixture now uses `String::with_capacity` and `writeln!` before timing, while asserting its exact 307,200-byte size. |
| `git diff --check origin/main...HEAD` | Passed (exit 0) after removing only the reported terminal blank lines from the Issue 129 plan and design. |

The earlier recovery attempt observed two transient macOS signal E2E failures
while the worktree lacked its controlled Python environment. After temporarily
linking the existing managed `.venv` for the final run, both signal scenarios
and the complete workspace suites passed. The link was removed after testing.

## Fixture and documentation repair

The initial Task 3 clippy run rejected the benchmark's iterator
`format!(...).collect()` construction in both compiled targets. The repair
preallocates the exact source size and appends each unchanged line via
`writeln!` before the timer starts; the benchmark now asserts that the fixture
remains 307,200 bytes and the fresh release invocation reported 7,680
candidates. This changes no production analyzer behavior.

The initial range diff check also found one extra terminal blank line in each
of the Issue 129 design and plan files. Removing only those two blank lines
made the required range check clean.
