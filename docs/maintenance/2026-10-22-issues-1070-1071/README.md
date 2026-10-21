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

Local checks run on macOS arm64 with locked dependencies, Cargo jobs limited to
two, development debug information disabled and incremental compilation off.
Only one Cargo command runs at a time, using this worktree's target directory.
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
  was not changed. Workflow/CI-selection Python tests: 89 passed, 8 skipped.
- Knowledge checks parsed YAML/reserved-file structure for 54 Markdown pages,
  matched all five new source hashes and citations, and found zero missing
  local Markdown/source targets or unreachable pages. The allowlisted public
  documentation site generated successfully; that does not validate external
  URLs or every historical claim.
- Independent implementation and documentation review found no important
  issue. Review included the pinned tempfile no-clobber implementation for
  Unix and Windows, but static inspection is not native Windows execution.

The manual non-Linux workflow now runs `review::store_tests` on both native
platforms before the longer workspace suite. Native Windows runtime verification
is pending; local macOS success does not establish it. Further results are
recorded below when available.
