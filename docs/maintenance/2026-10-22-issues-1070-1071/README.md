# Issues #1070–#1071 implementation and evidence

Implementation continues on `audit/issues-1070-1071`, the branch containing the
[post-1069 audit](../../knowledge/audits/design-2026-10-22-post1069.md), after
audit commit `d45174b7d5b337e9792e2f9559c4c4adf81fa9ab`. The audit's original
revision, source hashes and static findings remain historical evidence.

## Original manifest read budget (#1070)

`build_manifest` and `SnapshotWriter::write_stream` wrap the safely opened
original descriptor in the existing `finite_file::SnapshotReader`. This covers
the preflight recheck, subsequent original verification, initial snapshot
streaming and duplicate-entry hashing. Reads consume at most the observed
descriptor length plus one growth-probe byte, rejecting observed append or early
truncation rather than accepting a prefix or following a growing producer.

The existing per-chunk copy and owned-disk checks remain in the snapshot writer.
Native path admission, capability/no-follow checks and permissions retain their
owners. No new fixed cap or public option is introduced. Finite bytes do not
bound arbitrary filesystem I/O time, provide an atomic snapshot, or detect every
same-length rewrite.

Before the fix, three new production-path tests failed and a stable-file control
passed. After the fix, all four pass. Finite append/truncate hooks run after the
descriptor budget is captured; no sustained writer or large fixture is used.
Tests cover preflight pending-snapshot cleanup, no installed plan on failure,
rejected original verification, duplicate snapshot entries, and stable empty,
single-byte and 128-KiB inputs with duplicate includes and independent hashes.
The existing primitive test directly observes the initial-length-plus-one byte
bound; the workspace suite retains copy/owned-budget and permission checks.

## Completed review-store publication (#1071)

A new store is initialized inside a private `.hoimin-review-*` directory beside
its destination. That directory owns the temporary DB and SQLite journals.
Initialization commits and validates the current schema, closes SQLite, then
syncs a write-capable file handle before publishing with
`TempPath::persist_noclobber`. The initializer closes its private directory and
opens the public pathname, so later journals use the actual store location.
An initializer losing publication opens and validates the winner at that path.

The pinned tempfile implementation uses no-replace rename or a hard-link fallback
on Unix and `MoveFileExW` without `REPLACE_EXISTING` on Windows. Any retained
source link from the fallback remains inside the owned private directory.
Errors and ordinary panic unwinding clean that directory and its sidecars.
Forced process termination can leave a private directory; a later invocation
neither adopts nor deletes unknown remnants and can retry the public destination.
No automatic recovery of existing empty/corrupt/foreign stores is added. The
former schema-zero polling is removed because new initializers never expose an
incomplete destination.

The store retains schema 2, project identity, DELETE journaling, FULL synchronous
mode, 16-MiB page bounds, foreign-key/trusted-schema settings and expected-revision
conflict checks. Tests preserve bytes of existing invalid stores and files that
appear during publication. A deterministic rendezvous makes two independent
private initializers reach publication together; exactly one revision-zero
append succeeds. A separate populated-winner test proves the loser opens the
public winner rather than its discarded private database.

Before the fix, seven initial visibility/error/panic regressions failed. The
expanded store suite now passes all 26 tests, including nine initialization
tests. Error and panic injection cover file creation, schema transaction and
pre-publication boundaries. These are ordinary error/unwind experiments, not
SIGKILL, SQLite crash-recovery or power-loss durability proofs. Directory fsync
and universal power-loss persistence are not claimed.

## Verification and resources

Explicit local build/test commands run on macOS arm64 with locked dependencies,
Cargo jobs limited to two, development debug information disabled and incremental
compilation off. These commands are serialized in this worktree's target directory.
The shared root Python environment is reused without dependency changes.

- Manifest regression tests: 4 passed.
- Workspace unit tests: 251 passed, 7 ignored.
- Review-store unit tests: 26 passed.
- `cargo test --locked --workspace`: exit 0; 3,963 passed and 34 ignored
  across 189 successful libtest summaries in the captured log. No failures.
- After two test-only Clippy corrections, `cargo test --locked -p hoimin-cli
  --features contracts --lib` passed: 1,024 passed and 25 ignored. This rechecks
  the final initialization/finite-read tests with runtime contracts enabled.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
  passed. Workspace and vendored-parser formatting checks passed.
- Workflow lint passed with actionlint, ShellCheck and zizmor from the locked
  workflow dependency group in a disposable environment; the shared `.venv`
  was not changed. Initial workflow/CI-selection Python tests: 89 passed, 8 skipped.
- Knowledge checks parsed YAML/reserved-file structure for 54 Markdown pages,
  matched all five new source hashes and citations, and found zero missing
  local Markdown/source targets or unreachable pages. The allowlisted public
  documentation site generated successfully; that does not validate external
  URLs or every historical claim.
- Independent implementation and documentation review found no important
  issue. Review included the pinned tempfile no-clobber implementation for
  Unix and Windows, but static inspection is not native Windows execution.

The pre-commit hooks additionally passed both formatting checks and workspace /
vendored-parser Clippy. The resulting local `target` directory (about 5.6 GiB)
and generated site were removed after verification; test logs remain outside
version control in `.superpowers/implementation-1070-1071/`.

## Native execution and CI follow-up

The [manual non-Linux run](https://github.com/HayaoSuzuki/hoimin/actions/runs/39667336844)
checks implementation commit `8c5c49b75e7d1cef427c32736a90e96a7614d25e`.
`review::store_tests` passed on both
[macOS](https://github.com/HayaoSuzuki/hoimin/actions/runs/39667336844/job/119455738843)
and [Windows](https://github.com/HayaoSuzuki/hoimin/actions/runs/39667336844/job/119455738976)
before the broader workspace tests. This establishes native execution of the
initialization/publication regressions, not completion of every manual CI job.
The longer workspace/platform jobs were still running at this follow-up.

The [first PR CI](https://github.com/HayaoSuzuki/hoimin/actions/runs/39667302809)
passed Linux quality, workflow lint, best-effort resources, Lean, Creusot, core
dependency purity and cross-boundary contracts. Its wheel-smoke Python suite
failed because the manual-vs-automatic workflow contract still expected no extra
publication step. This was reproduced locally, then corrected to validate one
unconditional native publication step in the Rust job before workspace tests;
the remaining job-shape comparison stays strict. Final related Python tests:
267 passed, 8 skipped. Full Python suite: 1,231 passed, 11 skipped; the tightened
job-specific assertion was then rechecked in the final related suite. Ruff lint,
format and ty checks passed for the updated test. Independent review confirmed
the added exception is limited to the Rust job and preserves the other job
comparisons.

The focused manual step retains two build jobs and harness threads. It no longer
overrides incremental compilation for that step alone, allowing the subsequent
workspace command to reuse its library artifacts. Final workflow lint passed.
These workflow/test/document follow-ups do not change the Rust bytes checked
by the native run. New PR CI results remain separate from the first run's
failure and the local correction; pending jobs are not claimed to have passed.
