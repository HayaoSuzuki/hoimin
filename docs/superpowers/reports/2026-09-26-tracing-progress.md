# Tracing and terminal progress review

## Agreed scope

Show execution stages, completed mutant counts, and elapsed time during interactive
`run`, `plan`, and `verify` commands. Enable detailed diagnostic logging through
`RUST_LOG`. Keep report stdout compatible. Work on `feat/tracing-progress` and
submit a pull request. Perform at least three self-review passes per stage.

## Design self-review

1. **User behavior:** existing human lifecycle messages do not show elapsed time
   while a test is running. Add periodic, plain progress lines on terminal stderr;
   count completed mutants without inventing a total for truncated or resumed runs.
   Help, completions, and historical `progress` comparisons stay free of live UI.
2. **Output compatibility:** enable progress only when stderr is a terminal.
   Default redirected stderr retains existing diagnostics. Explicit `RUST_LOG`
   enables additional tracing records: text on a terminal, JSON Lines otherwise.
   These debug records are distinct from the versioned report-event schema.
   Never write tracing or live progress to stdout. Keep library subscriber setup
   under the embedding application's control; initialize only in the binary.
3. **Runtime constraints:** publish small progress snapshots and bounded, lossy
   log messages to a dedicated output thread. Neither a slow sink nor log volume
   may block the Tokio executor. Bound shutdown flushing and release the guard
   before `process::exit`. Instrument async functions with async-aware spans and
   explicitly propagate spans into spawned process/blocking tasks. Avoid dumping
   source, configuration, environment, or test argv into tracing fields.

## Baseline

`cargo test -p hoimin-cli --test report_handler`: 29 passed.

## Implementation self-review

1. **Stages and counts:** reviewed state transitions, including persisted and
   reused results. Compare completed counts before and after each accepted state
   transition, including synthetic result acknowledgements. Counts exclude
   `not_run`. Keep elapsed time across stage changes. Help and completions do not
   start a progress snapshot.
2. **Concurrency and lifetime:** reviewed every added span and task boundary.
   Use `instrument` for async functions, `in_current_span` for spawned process
   futures, and scoped spans inside blocking closures. Tests confirm run ID and
   mutant ID survive both nested process tasks. A blocked-sink test confirms
   publishers and guard shutdown remain bounded; queue pressure and oversized
   records drop whole messages.
3. **Compatibility and diagnostics:** reviewed binary-only initialization,
   launcher bypass, terminal detection, filter fallback, and final guard release.
   Tested redirected JSON/JSONL stdout with logging enabled, default quiet stderr,
   invalid/disabled filters, and real PTY progress with `RUST_LOG=off`. Instrumented
   fields omit source contents, configuration, environment, and test argv.

## Focused validation

- Before implementation: two new behavioral tests failed because the binary
  emitted neither debug logs nor terminal progress; two compatibility tests passed.
- After implementation: three binary unit tests and five integration tests passed,
  including PTY output, process span correlation, and blocked-sink shutdown.

## Additional review and regression fixes

A separate read-only reviewer found two issues. Both regressions failed before
their fixes and passed afterward:

- Reused results update the summary when their output is acknowledged. Comparing
  summary counts captures this update before the next live mutant starts.
- Buffered JSON diagnostics can span multiple writes. Owned handlers detect
  standard stderr before type erasure and hold its reentrant lock for the complete
  diagnostic record. Custom library writers keep their synchronization policy.

The focused re-review approved both fixes with no remaining Critical or Important
finding. A further self-review caught a stage boundary: target resolution and
workspace preflight share a core phase. The CLI now restores the workspace stage
when target resolution completes; a regression failed before this correction.

## Validation self-review

1. **Behavioral evidence:** checked the red/green results and final focused runs:
   seven CLI integration tests, three binary unit tests, and the diagnostic
   serialization regression. The integration tests cover real PTY output, reused
   result timing, process span context, redirected records, filter fallback, and
   a saturated stderr transport while the execution deadline expires.
2. **Regression scope:** reviewed report goldens, owned-output failure handling,
   resumed sessions, and the workspace test results. Report tests retain 29 passing
   cases. The full workspace run and final toolchain checks are recorded below.
3. **Release and documentation:** checked binary-only subscriber initialization,
   launcher bypass, Cargo.lock changes, MSRV, lint output, and the README examples.
   Existing untracked IDE files are outside the change. Linux and Windows runtime
   behavior was not exercised locally; local execution used macOS arm64.

## Documentation self-review

1. Compared README and OKF claims to the terminal/filter branches and count logic.
   Explicitly distinguished tracing JSON records from versioned diagnostic events.
2. Checked source identifiers, relative paths, source hashes, and the new report's
   entry in the audit catalog. Retained historical source metadata and draft status.
3. Reviewed Japanese paragraphs and evidence limits. Historical `progress`
   comparisons, live execution display, and diagnostic logging remain distinct.

## Final checks (macOS arm64, 2026-09-26)

| Check | Result |
| --- | --- |
| `cargo test --workspace` | Passed, no failures |
| Final `cargo test -p hoimin-cli --test telemetry --lib --bin hoimin` | 739 library tests, 3 binary tests, 7 telemetry integration tests passed; 12 existing ignored library tests |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo +1.88 check --workspace --all-targets --all-features --locked --target-dir /tmp/hoimin-tracing-msrv` | Passed |
| `cargo fmt --all -- --check` and `git diff --check` | Passed |
| OKF YAML/reserved files, local links, reachability | 29 pages passed |
| Added OKF source hashes and footnote correspondence | 3 sources matched |

The final focused run covers the target-resolution stage correction made during
the workspace run. No Python implementation, executable schema, or Lean model
changed. Cross-platform runtime behavior remains subject to platform testing.

## Publication self-review

1. Confirmed the topic branch forked from `main` at `56c51a8`; fetched `origin/main`
   still points to that commit. Reviewed the change scope and excluded pre-existing
   `.DS_Store`, `.idea/`, and `.serena/` files.
2. Checked that the README, updated knowledge pages, and this review record belong
   to the branch, with source links and hashes for the documented implementation.
3. Reviewed the PR description against the observed checks and stated macOS-only
   runtime validation. The PR targets `main` and leaves merging to the maintainer.

## CI follow-up: lease-only staging fixture

[The randomized-order job](https://github.com/tokyogas-tech/hoimin/actions/runs/36173808132/job/108200081395)
failed in `lease_only_staging_root_is_reclaimed_after_a_day` with zero reclaimed
roots, one preserved root, and no diagnostic detail. Its shuffle seed was
`1790361566959896000`. The other non-skipped jobs on that revision passed.

### Investigation self-review

1. Traced the empty-detail preservation paths. A busy lease takes this path;
   failing the stale-age check would instead append a diagnostic detail.
2. Compared the fixture with `unlocked_lease_with_a_heartbeat_at_least_twenty_four_hours_old_is_reclaimed`.
   That existing test explicitly unlocks its lease because a parallel fork can
   retain the shared file description until exec. Closing only the owner's
   descriptor does not establish an unlocked fixture in that interval.
3. Kept a Unix `try_clone()` alive through reclamation to model this condition.
   Before adding the explicit unlock, the targeted test failed locally with the
   same zero-reclaimed, one-preserved, empty-detail report as CI.

### Fix self-review

1. The fixture now explicitly unlocks the lease before dropping its owner. This
   establishes the abandoned-root premise without changing production locking,
   cleanup deadlines, or the requirement to preserve genuinely locked roots.
2. The cloned descriptor remains alive until after the reclamation assertions,
   so removing the explicit unlock deterministically reproduces the failure on
   Unix. The assertions still exercise actual directory reclamation.
3. Checked platform boundaries: the clone is Unix-only, and the cross-platform
   unlock follows the existing stale-heartbeat fixture. This test checks cleanup
   of an unlocked abandoned root, not automatic lock release after a real crash.

A separate read-only review approved the fix with no findings. The targeted test
passed after the fix on macOS arm64.

### Validation self-review

1. Compared the targeted red/green output: the retained-descriptor case failed
   before explicit unlock and passed afterward, without weakening the reclamation
   assertion or introducing retries.
2. Ran `cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle-seed 1790361566959896000`
   on macOS arm64. The whole workspace passed, including 739 library tests and
   seven telemetry integration tests; 12 existing library tests were ignored.
   The same seed does not imply identical Linux/macOS ordering because the
   platform-specific test sets differ. Nightly emitted three deprecation warnings
   for existing `fetch_update` calls outside this change.
3. Confirmed `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo fmt --all -- --check`, and `git diff --check` passed. Linux confirmation
   for the follow-up remains the responsibility of the new PR workflow run.

### Documentation and publication self-review

1. Checked the explanation against the retained-descriptor reproduction and the
   existing unlocked-heartbeat fixture. The CI interleaving was inferred from the
   failure and deterministic reproduction, not captured in a process trace.
2. Reviewed `docs/knowledge/design/runtime-lifecycle.md`: its production cleanup
   contract is unchanged. Updated this report and its audit-catalog source hash;
   retained unrelated historical evidence. Checked OKF structure, links, and hashes.
3. Reviewed the staged scope and PR explanation. This follow-up contains only the
   test fixture and review documentation, on the existing topic branch. Local
   results are distinguished from the pending Linux CI rerun.
