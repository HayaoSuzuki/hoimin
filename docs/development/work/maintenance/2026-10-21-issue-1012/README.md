# Issue #1012: report the resolved individual mutant timeout

Base: `9d457e32` (main, merged #1048). Scope follows the latest product triage
in [Issue #1012](https://github.com/HayaoSuzuki/hoimin/issues/1012), not its
historical multiplier proposal.

## Result and boundaries

The execution machine stores the duration resolved when it constructs its first
mutant process request. Every subsequent request and the final summary consume
that same value. The reporter never reimplements the auto-timeout formula.

Report/event schema 10 adds required nullable `resolved_mutant_timeout` to
`RunSummary`. JSON exposes it under `summary`; JSONL on `run_finished`; human
output includes a labeled summary line using precise duration formatting.
Resolved wire values use canonical integer `secs` and `nanos`. The existing
strict report-duration decoder is shared with mutation elapsed values: arrays,
unknown fields and nanos >=1,000,000,000 are rejected instead of normalized.
Missing metadata is rejected; explicit null is accepted.

Null means no mutant process request was constructed in this run. This includes
baseline failures, pre-dispatch stops, empty selections and reuse-only runs.
Even a fixed configured timeout is not called resolved without a request.
A non-null value does not certify that an OS process started or ran until this
cap: the run-wide deadline can interrupt earlier. Existing timeout/result
classification and exit codes remain authoritative and unchanged.

The auto/fixed calculation and `hoimin-budget-math`/formal proof sources are
unchanged. There is no multiplier, additional limit, configuration setting,
plan/session/fingerprint version change, history migration, or new persistence.
Readers move directly to report schema 10. Prior fixtures remain historical
artifacts, not a runtime compatibility path. Package floor stays 0.14.0.

## Verification approach

- Regression tests first observed missing report metadata against actual
  `RunMutant` requests for the auto floor, 2.900000001s -> 6.800000002s, fixed
  1.234567891s and no-request null. The literals check existing arithmetic; the
  reported value is compared to the actual request, not recomputed by a reporter.
- Removing the new human line made both precision and unresolved-display tests
  fail. Restoring it preserves nanoseconds and explicitly labels unresolved caps.
- The standard Duration decoder accepted arrays and noncanonical nanos; new
  wire regressions observed both failures before sharing the strict decoder.
- Public CLI tests cover auto JSON, fixed JSONL/human, failed baselines in both
  policies and verify's saved fixed cap. Baseline failure retains exit code 3.
- Current-schema goldens exercise typed producer regeneration and consumers;
  schema tests cover nullable/required duration shapes and invalid boundaries.
- The completed checks below are local macOS arm64 observations; no claim
  is made about unexecuted OS-specific checks or new formal proofs.

## Resource policy

The numbered branch/worktree is isolated; one Cargo process runs at a time with
`CARGO_BUILD_JOBS=2` and the existing shared `CARGO_TARGET_DIR`. The existing
Python environment is linked, not copied. There is one optional Duration per run
and one final report field, with no per-mutant cache, extra filesystem scan or
new retained output buffer. Disk use is monitored between build phases; test
fixtures use automatic temporary-directory cleanup.

## Completed validation

- `cargo test --locked --offline --workspace --all-features`: 3,888 passed,
  0 failed, 33 ignored across 186 suite/doc-test results.
- `cargo test --locked --offline -p hoimin-core --test report_policy`:
  32 passed with contracts disabled.
- `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings`:
  passed. Workspace and vendored-parser Clippy also run through commit hooks.
- `cargo fmt --all -- --check`, changed Python tests' Ruff check/format:
  passed.
- `.venv/bin/pytest -q`: 1,223 passed, 14 skipped.
- `.venv/bin/python -m tools.docs_site --output site`: strict build passed.
- OKF YAML/reserved-file parsing and entry-point reachability: 50 pages.
  Relative link targets in the four changed concepts, plus new source hashes
  and matching footnotes: passed. Historical source hashes were not refreshed.
  This does not assert that all historical links in the entire bundle still
  resolve against the current checkout.
- Independent static review: no Important or Critical findings.
- Final whitespace check: passed. Disk free space after full tests: about 144 GiB.

The first full run exposed a shared progress-test JSON input missing the new
required field. Updating that current-format fixture fixed all 101 dependent
failures; the focused 161-test progress suite and subsequent full workspace run
passed. Historical schema-9 rejection is now explicitly covered. No reader
fallback or permissive default was added to accommodate those inputs.

## PR #1049 fuzz build follow-up

[CI job 119081391328](https://github.com/HayaoSuzuki/hoimin/actions/runs/39557554706/job/119081391328?pr=1049)
failed before any fuzz target executed. Its `fuzz-report/build.log` reports
E0063 at `fuzz_targets/report_sequence.rs`: the synthetic `RunSummary` omitted
`resolved_mutant_timeout`. The initial full-workspace test and lint commands do
not include the independent `fuzz/` workspace; the initial validation missed
that separate build. `cargo check --locked --offline --manifest-path
fuzz/Cargo.toml --bins` reproduced the exact missing-field error locally.

The harness now supplies a subsecond cap for executed outcomes and explicit
None for reused/not-run outcomes, exercising both summary round-trip forms.
The independent formatter also reordered one existing import in
`python_analyzer.rs`. No shipping implementation or contract changed in this
follow-up; the four OKF concepts updated above remain applicable.

Follow-up checks, executed sequentially with the shared target and jobs=2:

- `cargo fmt --manifest-path fuzz/Cargo.toml -- --check`: passed.
- `cargo check --locked --offline --manifest-path fuzz/Cargo.toml --bins`: passed.
- `cargo clippy --locked --offline --manifest-path fuzz/Cargo.toml --bins -- -D warnings`:
  passed.
- `cargo +nightly-2026-10-08 fuzz build --target-dir <shared-target> --codegen-units 16`:
  all seven targets built with AddressSanitizer, matching CI's pinned toolchain.
- `cargo +nightly-2026-10-08 fuzz run report_sequence --target-dir <shared-target>
  --codegen-units 16 fuzz/corpus/report_sequence fuzz/seeds/report_sequence --
  -max_total_time=30 -max_len=4096 -timeout=5 -rss_limit_mb=1024`:
  exit 0, 98,270 executions in 31 seconds, seed 1049459887. Final reported
  RSS was 469 MB under the 1,024 MB limit. Disk free space remained about 143 GiB.

The bounded run is a macOS arm64 observation for this target and these inputs;
it does not replace the Linux CI campaign or establish exhaustive coverage.
The earlier Rust/Python suite counts describe the original implementation
validation; those entire suites were not repeated for this harness-only fix.
