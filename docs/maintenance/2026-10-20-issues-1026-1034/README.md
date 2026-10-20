# Issues 1026, 1027, 1032, and 1034

Base: `05c909c54a3bdf3485db2c833456fabe83c7dfaf`.
Work branch: `fix/issues-1026-1034`.

The [implementation plan](../../superpowers/plans/2026-10-20-issues-1026-1034.md)
links the four issue specifications and records the shared boundaries.
Domain review records cover [Git](git.md), [reports](report.md), and
[preparation](runtime.md). Each record distinguishes design, implementation,
and verification reviews.

## Environment

- macOS on Apple Silicon
- Rust/Cargo 1.99.0
- Python 3.14.7
- Git 2.56.0
- Shared Cargo target directory and existing project Python environment

## Checks completed during implementation

| Check | Result |
| --- | --- |
| Original core library baseline, `cargo test -p hoimin-core --lib` | 58 passed, 2 ignored |
| Final workspace suite, `cargo test --workspace --no-fail-fast` | 3,764 passed, 33 ignored; 182 Cargo test harnesses; exit 0 |
| Core contract suite, `cargo test -p hoimin-core --features contracts` | 845 passed, 2 ignored |
| CLI contract suite, `cargo test -p hoimin-cli --features contracts` | 2,422 passed, 28 ignored; 135 Cargo test harnesses; exit 0 |
| Six Python schema suites | 70 passed |
| Python full suite, `.venv/bin/python -m pytest tests` | 1,198 passed, 14 skipped |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed after final Rust changes |
| Generated CLI reference, `generate_cli_reference --check` | Current |
| CLI reference example tests | 7 passed |
| `cargo fmt --all -- --check` and `git diff --check` | Passed |

Workspace totals use the final summary from each Cargo-launched harness. Two
successful embedded helper-process summaries are excluded to avoid counting the
same subprocess cases twice.

The first Python suite attempt ran inside the command sandbox and failed six
tests: four `test_lean_resource_guard` process-inspection tests and two
`test_release` subprocess tests that needed a writable uv cache. A rerun with
process inspection permitted and `UV_CACHE_DIR` under `/private/tmp` passed.
The stale memory recommendation to use unittest discovered no tests; the
repository config uses pytest. Current `docs/development.md` prohibits Rust
cargo-mutants checks, so those historical memory instructions do not apply.

All planned integration checks completed successfully on 2026-10-20.
This host does not establish Windows or Linux-specific lifecycle behavior.

## Knowledge maintenance

Consulted the catalog index, overview and OKF update procedure. Updated the
existing [selection](../../knowledge/design/selection-plan-verify.md),
[runtime](../../knowledge/design/runtime-lifecycle.md), and
[session/report](../../knowledge/design/session-report.md) concepts with the
changed contracts, evidence boundaries, and nine source-content hashes. Existing
historical source metadata remains unchanged.

PyYAML 6.0.3 parsed and validated the reserved-file/frontmatter structure of all
50 knowledge Markdown files. The three updated concepts passed source-ID,
footnote, new-source-hash and index-reachability checks; 263 local link targets
exist. One pre-existing historical link in the selection concept points to the
removed `crates/hoimin-cli/src/analyzer/rust.rs`. Its #476 modified-source record
is outside these issues and was retained, rather than relabeled as current
implementation evidence. New links have existing targets. This was a local
structure/reference check, not an official OKF validator or external URL check.

## Integration review findings

1. The report event decoder must keep nested `RawValue` support. A buffered
   internally tagged enum would break existing disk integer payloads.
2. Schema-open normalized configuration still contains schema-closed children.
   The reader must check recognized closed children while retaining the existing
   opaque configuration layout contract.
3. Git shutdown must retain its owner after reap failure and preserve both kill
   and reap diagnostics. An empty active-task count alone cannot prove success.
4. Janitor selection and deletion both consume preparation time; limiting only
   the deletion loop leaves the selection scan outside the deadline.
5. Preparation diagnostics must preserve write/flush failures, and a late stop
   must not hide an earlier infrastructure failure.

## Independent review

The independent reviewer inspected the full diff against the base commit,
including the final preparation/monitor ownership, cancellation handoff,
janitor deletion checks, and raw report envelope. No Critical, Important, or
Minor findings remain. The coordinator and workers added regressions for the
findings before applying fixes; the reviewer inspected the corrections and
did not claim to rerun their tests independently.

The initial full Rust run found a plan test that decoded ranked rows as bare
mutation candidates. The test now uses the existing `PlanManifest` boundary;
the focused comparison passed. A subsequent full run compiled the intentional
late-cancellation failing regression before its correction and confirmed the
one-second grace extension. That run also exposed a timeout integration test
whose tiny budget could now expire during preparation; the test now synchronizes
on baseline start before advancing the clock, retaining its report assertions.
Both focused regressions passed. The final integration run uses the frozen
source after those corrections.

The reviewer left the following questions outside this change. These boundaries
match the issue specifications; they do not establish additional guarantees:

- No legacy-report compatibility or general proof of every Schema constraint;
  open outer configuration objects retain the existing opaque-input policy.
- No change to standalone plan, verify prevalidation/reanalysis, or dry-run.
- No arbitrary descendant containment, sandbox, or new gc command. The Git
  runner owns the direct child and its local pipe readers.
- No immediate interruption of an active OS operation or cleanup guarantee
  after the caller destroys its Tokio runtime. Live blocking owners and dropped
  Git waiter cleanup retain the documented lifetime requirements.
- Injected termination/reap failures establish sequencing evidence; this macOS
  run does not validate actual Linux/Windows backend failures.
- No new formal proof or performance claim. Current development instructions
  prohibit cargo-mutants, and the issues do not require a new benchmark.
