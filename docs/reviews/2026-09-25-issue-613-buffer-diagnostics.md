# Issue 613: stderr diagnostic buffering review

JSON/JSONL stderr diagnostics now serialize through an event-local 8-KiB buffer. The helper reuses the existing JSON Lines serializer, newline, and explicit flush. It dismantles the buffer with `into_parts` after the attempt, discarding any remainder so Drop cannot retry after an I/O error. Human output, normal JSONL stdout and the JSON document spool retain their existing paths.

The design and implementation plan were committed before code as `82dbb3e`, each with three review passes. No schema, baseline retention/chunking or decision rule changed.

## Implementation self-reviews

1. **Scope and bytes.** Checked the single callsite: only `OutputEvent::Diagnostic` with JSON/JSONL uses the new helper; ordinary stdout and human stderr keep `write_event`. Reusing the existing serializer preserves escaping/newline/flush and existing `serialization_failed` error mapping. The serialized event is borrowed and no document/event Vec is introduced.
2. **Errors and acknowledgement.** Traced errors at buffer spill, partial write, zero write and underlying flush. Returning the existing result preserves ReportIo and effect ID; no acknowledgement occurs before successful flush. `into_parts` consumes the buffer on every ordinary result path, preventing deferred Drop writes after an error. The helper does not rely on Drop for success. Panic/unwind semantics are outside this normal Result contract.
3. **Resource and ownership.** Capacity is explicitly 8 KiB and scope is one event; baseline diagnostics remain 16-KiB chunks. Large strings may bypass the buffer directly, which preserves bounded storage and reduces ordinary-string writes. No state survives to the next event, and no ReportHandler generic/interface or shutdown state changes. Per-event allocation is a deliberate small cost for an isolated error boundary.

## Test self-reviews

1. **Performance RED and byte sensitivity.** The new public ReportHandler counted-writer test failed on unchanged code: a 16-KiB plain payload made 54 writes against a 13-write bound. After buffering, the same event takes three writes. Tests cover both JSON formats, 16-KiB/1-MiB payloads, plain strings, newlines, quotes, backslashes, control characters and Unicode. They compare exact serde bytes plus newline, parse the message back, assert one flush before acknowledgement, and confirm stdout stays empty.
2. **Error sensitivity.** Always-failing writers cannot reveal a second attempt; the transient partial-error writer becomes writable after its error. Removing `into_parts` temporarily caused a third write where the test required two (RED). Restoring it passed. Zero-write, Interrupted-then-success, and underlying flush failures also exercise the real public handler. Repeated interrupted/normal records verify each acknowledged event is visible and flushed with no cross-event retention.
3. **Memory and end-to-end boundary.** A separate single-test allocator binary constructs the input event before measurement and writes to a nonretaining sink. Both formats use 8,192 peak additional bytes at 16-KiB and 1-MiB escaped messages. A temporary whole-event Vec serializer produced 2,097,152 bytes and failed the 32-KiB cap; restoring the fixed buffer passed. The public release run_with_io probe measures actual failed-baseline output with an unbuffered counted Stderr and reconstructs all retained payload bytes. Writer-call counts are not syscall-tracer measurements, and local timings include baseline execution and other development activity.

## Independent review

Root independently reviewed production and tests: no blockers. Confirmed `into_parts` suppresses failed-buffer retries, event flush reaches the underlying writer, diagnostic-only routing preserves document stdout, and partial/zero/interrupted/error and heap sensitivity controls are meaningful.

## Verification and performance

Focused `report_handler`, `diagnostic_heap`, `baseline_output`, and `report_heap` tests passed after both temporary sensitivity mutants were removed. The original helper was restored exactly before independent review and all final checks. Full workspace passed: 2327 passed / 22 ignored across 100 result groups (including subprocess groups). Both exact CI clippy commands, workspace/vendor fmt, and diff checks passed. Release measurements follow below.

Reproduction uses `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target/batch-progress`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=1`:

```sh
cargo test -p hoimin-cli --test report_handler --test diagnostic_heap --test baseline_output --test report_heap
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings
cargo fmt --all -- --check
cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check
```

Logs: `/tmp/hoimin-batch-604-632/613-*`. Release probe, build/measurement scripts and before/after JSON results: `/tmp/hoimin-batch-604-632/613-benchmark/`. No Python repository source changed and no shared environment was modified. No Lean process was launched: this change preserves the existing report effect/error contract and the workspace runs its existing oracle consumers.

## Public release baseline measurements

Two separately linked probes call the actual unchanged/optimized `hoimin_cli::run_with_io` from release libraries at the same base commit. Each runs the same 1-MiB retained baseline workload with an unbuffered counted `std::io::Stderr`, redirected to a regular file; each condition has three samples. No production parser or writer is copied into the probe. All 36 executions returned baseline-failure status 3, reconstructed the exact retained payload, and produced the same byte count within each workload/format.

| Payload (1 MiB) | Format | Writes before → after | Bytes (both) | Median seconds before → after |
|---|---|---:|---:|---:|
| plain | json | 3,638 → 193 | 1,064,569 | 0.109746 → 0.156982 |
| plain | jsonl | 3,638 → 193 | 1,064,569 | 0.104168 → 0.133513 |
| short-lines | json | 1,052,150 → 257 | 1,588,857 | 1.117288 → 0.107489 |
| short-lines | jsonl | 1,052,150 → 257 | 1,588,857 | 1.132068 → 0.091975 |
| failure-lines | json | 29,211 → 193 | 1,077,356 | 0.146582 → 0.101516 |
| failure-lines | jsonl | 29,211 → 193 | 1,077,356 | 0.195610 → 0.157036 |

The short-line case reduces writer calls by roughly 4,094× (1,052,150/257); this is the deterministic performance evidence. Plain-payload medians were slower after the change despite fewer writes. Timings include process/baseline work, use only three local samples, and run alongside other development; they are observed medians, not a general speed guarantee. Environment: macOS arm64, Rust 1.98.1, release thin LTO, one Cargo job. The probe/measurement scripts preserve arguments and expected payload checks in the artifact directory above.
