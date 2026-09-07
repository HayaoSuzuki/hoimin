# Issue #335: owned report delivery

## Changes

The CLI run and verify paths move report operations to Tokio's blocking pool.
Each driver retains one pending operation, its report handler, and the delivery
root/child leases. A dropped waiter does not retry or acknowledge that operation.
Workers check the shared absolute deadline before starting queued output.

Final output, flush, metrics warnings, and terminal run diagnostics share the
shutdown budget. Once that budget expires, the CLI exits with code 2 without
requiring another stderr write. Caller-provided borrowed/non-`Send` writers keep
the existing synchronous API behavior.

Execution cleanup requires process/drain/monitor safety and returned workspace
ownership after resource close. Delivery cleanup also requires report quiescence
and spool release. A blocked JSON writer retains its delivery root, child, and
spool while the already-cleaned execution root stays absent.

## Reproduction and regressions

Before the fix, an unread full Unix stream kept JSONL CLI output blocked beyond
a six-second watchdog despite a one-second total timeout and two-second grace.
The regression now passes for JSON, JSONL, and human output. Separate scenarios
cover unread stderr and SIGINT after observing the start of JSON/JSONL output.

Controlled blocking-pool tests caught queued output starting after abandonment
and a writer panic reporting the wrong effect ID. Both failed before their fixes.
The final tests also cover deadline expiry with a live driver, physical-write
acknowledgement, flush ownership, immediate I/O failure, and actual JSON spool
retention across cancellation.

## Verification

macOS arm64, dedicated Cargo target directory `/private/tmp/hoimin-335-target`:

- `cargo test --workspace --all-features --quiet -- --test-threads=1`: passed.
  The CLI library reports 555 passed and 9 ignored; `run_e2e` reports 58 passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Independent review: no remaining important findings.

The full Rust suite includes the existing shutdown and disk-lifecycle oracle
checks. This change adds no Lean theorem, bounded search, or generated corpus.
The abstract model's report completion and transferred blocking ownership remain
the contract; Rust tests exercise the new delivery driver and OS transport.

| Mode | Premise and observation | Evidence boundary |
| --- | --- | --- |
| strict | Full external stdout/stderr, configured timeout, process exit; SIGINT after first output byte | Real macOS CLI using Unix streams |
| internal-fixture | Pending writer/flush, retained leases, queued start rejection, effect identity | Controlled writers and one-thread blocking pool |
| model-only | Abstract report completion and blocking ownership transfer | Existing `ShutdownModel.lean`; no claim that Lean proves OS writes |

Windows CI exercises the portable driver and shell tests. The Unix-stream and
SIGINT transport scenarios compile only on Unix. CI results belong to the pull
request's final pushed commit.
