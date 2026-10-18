# Task 2 — #975 nonblocking regular review inputs

Base: `0202878e466ed86bdbc48ddb4c5267a858b04d6a`. Approved contract: task-2-brief.md and `/tmp/hoimin-design-audit-5d3aee95/impl-975.json`.

## Requirements/design self-review (five passes)

1. Reachability: `bounded_read` blocks before descriptor validation; `validate_destination_entry` independently opens plan/report/source/fingerprint protected paths when a store entry exists. Both need the common opener.
2. Ownership/policy: use `io::Result<Option<File>>` for the common regular descriptor boundary (the existing duplicate contract). Artifact callers retain Invalid diagnostics; duplicate confirmation remains optional; protected nonregular sources are skipped for identity comparison and remain Unavailable in source validation.
3. Race/security: Unix `O_NONBLOCK` must be applied to the actual open, followed by descriptor metadata; pre-open path metadata alone is insufficient. Keep store no-symlink validation separate and preserve protected regular-file identity checks.
4. Compatibility/bounds: follow accepted input symlinks, retain missing-input I/O behavior and 64 MiB/growth checks. Existing arbitrary-byte property already covers regular files, so do not duplicate it.
5. Test lifetime/scope: add independent FIFO plan/report/source missing-store/source existing-store/fingerprint existing-store scenarios with deadline and cleanup of a broken reader. Keep changes restricted to artifact/duplicate/review paths and focused tests; controller owns integrated full-workspace verification.

## RED

Command (each Cargo invocation has the resource environment below):

`env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --test review_cli review_fifo -- --nocapture`

- `/tmp/hoimin-design-audit-5d3aee95/task-2-red.log`: baseline 0 passed, 5 failed (2.03s test execution), exit 101; every failure is a FIFO deadline and broken processes are released/killed/reaped.
- Initial implementation run `/tmp/hoimin-design-audit-5d3aee95/task-2-green-fifo.log`: 2 passed, 3 failed. Investigation `/tmp/hoimin-design-audit-5d3aee95/task-2-debug-source.log` showed partially written successful JSON, diagnosing undrained subprocess pipe backpressure on macOS, rather than another input opener.
- Replaced pipe capture with file-backed capture. `/tmp/hoimin-design-audit-5d3aee95/task-2-green-fifo-capture.log`: 5 passed, exit 0 (0.41s).
- Reverted only the three production files to HEAD and re-ran the corrected helper: `/tmp/hoimin-design-audit-5d3aee95/task-2-red-corrected-capture.log`: 0 passed, 5 failed, exit 101 (2.04s). Restored implementation patch afterward. This confirms the final helper detects the production bug independently of output backpressure.

## Implementation self-review (five passes)

1. Call graph audit: no blocking `File::open` remains in review/input.rs; store descriptor and every protected plan/report/source/fingerprint identity open use the common opener. Duplicate implementation removed, leaving one definition.
2. Caller policy audit: `None` becomes the existing artifact Invalid message, optional duplicate confirmation stays false, and protected nonregular inputs are ignored only for alias checking; source state still uses bounded_read and becomes Unavailable. NotFound remains ignored for protected inputs; other I/O errors propagate.
3. Descriptor/race audit: common helper applies Unix O_NONBLOCK before open and checks file.metadata().is_file(). Store's symlink_metadata rejection remains a separate rule, with an additional regular descriptor check after opening.
4. Resource/test lifetime audit: the initial pipe-backed helper itself could block a successful response; changed to tempfile capture and repeated baseline RED. Timeout opens FIFO rescue, kills child, and waits; a Drop guard also kills/waits if an intermediate assertion panics. Guard remains attached until successful wait.
5. Compatibility/bounds audit: read limit, take(limit + 1), post-read length check, and existing arbitrary-byte property are preserved. Added regular-file symlink acceptance, source symlink with existing store, store symlink rejection, missing source with existing store, and symlinked protected fingerprint alias coverage. No generic reader application policy or blocking compatibility wrapper added.

## Verification commands and results

All commands ran serially, with existing worktree target and no dependency changes:

```sh
env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo fmt --all
env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --lib artifacts:: -- --nocapture
env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --lib review:: -- --nocapture
env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --lib progress::duplicate:: -- --nocapture
env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --test review_cli -- --nocapture
git diff --check
```

Logs in `/tmp/hoimin-design-audit-5d3aee95/`:

| Log | Result |
| --- | --- |
| task-2-fmt.log | exit 0 |
| task-2-artifacts.log | 12 passed, 0 failed/ignored; exit 0 |
| task-2-review.log | 78 passed, 0 failed/ignored; exit 0 |
| task-2-duplicate.log | 4 passed, 0 failed/ignored; exit 0 |
| task-2-review-cli.log | 17 passed, 0 failed/ignored; exit 0 |

### Verification self-review (five passes)

1. FIFO behavior: all five independent CLI scenarios pass with a 2s subprocess deadline; the same final helper failed all five against baseline. Final CLI suite completes in 0.49s.
2. Review semantics: 78 review unit tests pass, including source freshness/unavailability, oversized plan/source rejection, hard-link protected inputs, fingerprint inputs, journal aliases, and newly added symlinked fingerprint alias detection.
3. Bytes and bounds: 12 artifact tests pass, including the existing 32-case arbitrary-byte exact-size property, oversized-file rejection, missing-I/O categorization and a new three-byte binary symlink fixture. Production initial-size and take(limit + 1)/post-read growth bounds are unchanged.
4. Duplicate compatibility: all four duplicate tests pass, covering optional missing-file handling, FIFO rejection/worker release, digest collisions, late differing bytes and lengths. They now exercise the shared opener.
5. Verification scope and diagnostics: all 111 focused tests passed with no warnings. `git diff --check` passes. No production behavior changed after those runs. Mandatory Clippy subsequently required three new empty-output assertions to use assert_eq! with an empty byte array; the CLI suite was rerun after that test-only adjustment. Full-workspace integration is controller Task 10, as explicitly delegated; it is not claimed here.

## Completion preparation self-review (five passes)

1. Scope audit: the commit contains five Rust files only; no Task 1 files, shared plan, brief or controller ledger edits. This report is retained locally under ignored .superpowers/sdd and is not committed. The initial commit mistakenly included this report; the controller authorized a packaging-only amend removing it from the index while preserving the local file. Maintained evidence is archived by controller Task 10.
2. Acceptance audit: plan/report FIFO rejection, missing/existing-store FIFO source unavailability, fingerprint identity nonblocking opens, regular symlink input acceptance, store symlink rejection, alias protections and missing-source behavior all have focused coverage.
3. Architecture audit: exactly one shared `open_regular_file` definition remains. `review/input.rs` has no `File::open`; no blocking wrapper or path-metadata-only replacement was introduced. Caller diagnostic/optional-confirmation/source-state policy remains outside the helper.
4. Resource/claim audit: Unix O_NONBLOCK addresses waiting for a FIFO writer; this is not a general filesystem I/O latency guarantee. Deterministic concurrent growth injection and Windows runtime behavior were not separately exercised; the growth bound code is preserved and Windows does not compile the Unix flags.
5. Commit preparation audit: inspected `prek.toml`: mandatory workspace/vendor fmt and workspace all-target/all-feature/vendor Clippy hooks. Commit command supplies all four Cargo resource variables so hooks inherit them; hooks are not skipped. The staged diff check found an extra trailing report blank line; removed it before commit. The first commit attempt passed both fmt hooks and vendor Clippy, but workspace Clippy rejected three new assert!(out.is_empty()) assertions. Replaced them with assert_eq!(out, [] as [u8; 0]) and reran CLI tests. First-attempt evidence is task-2-commit-first.log; final commit and mandatory hook details are captured in task-2-commit.log.

## Files

- `crates/hoimin-cli/src/artifacts/mod.rs`: shared regular descriptor opener, bounded reader reuse, symlink bytes regression.
- `crates/hoimin-cli/src/progress/duplicate.rs`: use shared opener; retain caller confirmation and existing tests.
- `crates/hoimin-cli/src/review/input.rs`: use shared opener for store/protected descriptor identity; skip nonregular protected inputs for alias comparison.
- `crates/hoimin-cli/src/review/tests.rs`: symlinked fingerprint identity remains protected.
- `crates/hoimin-cli/tests/review_cli.rs`: five FIFO regressions with finite cleanup, and four compatibility scenarios.
Local evidence outside the commit: `.superpowers/sdd/2026-10-18-issues-971-979/task-2-report.md` (this ignored report).

## Commit

```sh
env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 git commit -m 'fix(review): open regular inputs without FIFO waits'
```

`/tmp/hoimin-design-audit-5d3aee95/task-2-commit.log` records mandatory hooks and commit output. The controller receives the resulting commit SHA in the handoff; it is supplied in the controller handoff. Packaging-only amend uses the same resource environment with `git commit --amend --no-edit`; hooks remain enabled and log to task-2-amend.log. No tests are rerun for removal of the transient report from the commit.

Packaging amendment completed: `1fb0409bf34fa2ebce9053c4111552116178e7e1`.
`git diff e9acb80706043fe256c662fcbf7fead90a123373 HEAD --name-status` lists only removal of this transient report; all five Rust files are unchanged. Tracked status is clean, the local report exists and is ignored, and no `.superpowers` file is included in Task 2's amended commit. Hooks stayed enabled; they skipped automatically because the amend staged only the Markdown removal (see task-2-amend.log). The preceding source commit's four hooks had passed.
