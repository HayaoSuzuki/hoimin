# Issue 352 POSIX Backslash Path Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject Unix filenames containing a literal backslash before hoimin can rewrite, collide, or read the wrong path, while preserving Windows native separators and existing glob escape syntax.

**Architecture:** Add one private `portable_path` boundary helper in `hoimin-cli`. Native filesystem inputs use host-aware conversion; decoded Git paths use a host-independent rejection rule because Git already emits `/` separators. Existing CLI, fingerprint, target, and workspace layers map the helper error into their established domain errors before inserting paths into maps.

**Tech Stack:** Rust 1.88+, Camino, ignore, Git fixtures, Tokio integration tests, cargo fmt/Clippy/test, repository Python checks.

**Spec:** `docs/superpowers/specs/2026-09-04-issue-352-posix-backslash-paths-design.md`

## Global constraints

- Work only in the issue worktree and branch `fix/issue-352-posix-backslash`.
- Follow red-green-refactor: add a focused failing behavior test, observe the intended failure, implement the smallest production change, then rerun it.
- Validate concrete paths before `BTreeMap::insert`, file reads, or target lookup.
- Keep `validate_pattern`'s original glob pattern and validation-only slash projection intact; `\` can escape a glob metacharacter.
- Do not change plan/report schemas, stable error codes, core candidate identity, or Windows native-path acceptance.
- Use the shared Cargo target directory; do not create a worktree-local build cache.
- Do not run cargo-mutants unless a later review identifies an untested predicate. If it becomes necessary, bound jobs/time and remove its temporary output immediately.
- Do not add Lean: real Unix filesystem and Git tests exercise the relevant host semantics directly.
- Automatic CI must remain Linux-only. Do not dispatch the manual Windows/macOS workflow during iterative development. Until PR #397 lands, open this as a stacked PR against `chore/manual-non-linux-ci` so non-Linux checks cannot run automatically or gate it.

---

## Task 1: Add the portable-path boundary helper

**Files:**

- Create: `crates/hoimin-cli/src/portable_path.rs`
- Modify: `crates/hoimin-cli/src/lib.rs`

- [x] Register `mod portable_path;`, create the module with its test skeleton, and add unit tests for `from_git`: `/` paths are borrowed unchanged, while a literal backslash is rejected with its original spelling on every host.
- [x] Add `#[cfg(not(windows))]` unit tests for `from_native`: ordinary paths are borrowed and literal backslashes are rejected.
- [x] Add a `#[cfg(windows)]` unit test for `from_native`: `pkg\file.py` becomes owned `pkg/file.py`.
- [x] Run `cargo test -p hoimin-cli portable_path::tests -- --nocapture` and record the initial compile/test failure before production code exists.
- [x] Implement `PortablePathError`, `from_native`, and `from_git` with `Cow<'_, str>`. Keep the rejected value available to callers and make the diagnostic name the original spelling.
- [x] Rerun the focused test and refactor only after it is green.
- [x] Commit the helper and tests.

## Task 2: Protect CLI and fingerprint boundaries

**Files:**

- Modify: `crates/hoimin-cli/src/cli.rs`
- Modify: `crates/hoimin-cli/src/fingerprint_inputs.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `crates/hoimin-cli/tests/fingerprint_inputs.rs`

- [x] Add a Unix CLI test showing `--line 'pkg\calc.py:4-7'` returns `CliError::InvalidValue` containing the full original argument.
- [x] Add a Unix exact-fingerprint test showing `literal\settings.toml` returns `fingerprint.file.invalid_path` with the original spelling.
- [x] Add a Unix glob-walk test whose real filename contains `\`; assert `fingerprint.include.unsupported_file` includes that spelling.
- [x] Add a compatibility test for an escaped glob metacharacter and assert it still selects the intended real file. This guards the deliberate `validate_pattern` exception.
- [x] Run the two focused integration-test binaries and observe that the new rejection cases fail under the current unconditional rewrites.
- [x] Change `parse_line_selection` and `normalize_exact_path` to use `from_native`, preserving their current public error families.
- [x] Change `resolve_one` to validate the concrete relative filesystem path through `from_native` before inspecting or recording it.
- [x] Do not route `validate_pattern` through `from_native`; retain the original string passed to `OverrideBuilder`.
- [x] Rerun the focused tests and commit.

## Task 3: Reject unsupported filesystem and workspace paths before collection

**Files:**

- Modify: `crates/hoimin-cli/src/target/fs.rs`
- Modify: `crates/hoimin-cli/src/workspace/manifest.rs`
- Modify: `crates/hoimin-cli/tests/target_handler.rs`
- Modify: `crates/hoimin-cli/tests/workspace_recovery.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`

- [x] Add Unix target-discovery tests for (a) one `foo\bar.py` and (b) both `foo\bar.py` and `foo/bar.py`. Both must reject the original literal name instead of returning a rewritten/collided map.
- [x] Add a public `plan` regression test for a literal-backslash source. Assert exit 2, empty stdout, the original spelling in stderr, and absence of the misleading rewritten `foo/bar.py: No such file` error.
- [x] Add a Unix workspace preflight test asserting `WorkspaceError::InvalidPath` includes the literal filename.
- [x] Run the focused tests and observe the current late read/collision behavior.
- [x] Add a typed `FsTargetError` variant for an unrepresentable portable path and use `from_native` before inserting into the discovery map.
- [x] Use `from_native` in workspace `relative_utf8` and map failure to the existing `WorkspaceError::InvalidPath` code before manifest insertion.
- [x] Rerun focused tests and commit.

## Task 4: Treat decoded Git backslashes as literal names

**Files:**

- Modify: `crates/hoimin-cli/src/target/git.rs`
- Modify: `crates/hoimin-cli/tests/target_handler.rs`

- [x] Add unit tests for a binary numstat NUL record and a C-quoted patch header whose decoded path contains a literal backslash; both must return `TargetError::GitFailed` with that spelling.
- [x] Add a unit test for a raw current-worktree NUL path containing a backslash.
- [x] Add a Unix real-Git integration test with a changed or untracked `literal\calc.py`, proving the public Git handler rejects it rather than reporting `literal/calc.py`.
- [x] Run the focused tests and observe the existing rewrite behavior.
- [x] Apply `from_git` after UTF-8/C-quote decoding in `insert_binary_numstat_path`, `parse_patch_path`, and `collect_current_worktree_paths`.
- [x] Preserve `/dev/null`, `a/`/`b/` prefix handling, non-UTF-8 errors, and existing rename/binary parsing behavior.
- [x] Rerun Git unit/integration/property tests and commit.

## Task 5: Document and verify the complete contract

**Files:**

- Modify: `README.md`
- Modify as needed: files above

- [x] Document that persisted logical paths use `/`, Windows native separators are accepted, and literal backslashes encountered in Unix filenames are rejected for targets, fingerprints, and workspace copies.
- [x] Run `rg -n "replace\('\\\\', \"/\"\)" crates/hoimin-cli/src` and classify every remaining match. Permit only Windows-native conversion and the glob validator's non-stored security projection; do not turn this spelling check into a unit test.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run focused `hoimin-cli` test binaries, then `cargo test --workspace` with the shared target directory.
- [x] Run workspace Clippy with warnings denied using the repository's standard command.
- [x] Run the repository Python test suite that validates docs/workflows/contracts.
- [x] Run `git diff --check`, inspect the complete diff, and perform three implementation self-review passes: contract/compatibility, ingress/error ordering, and test/operational risk.
- [x] Confirm no worktree-local `target`, mutation output, or other large temporary artifacts exist. Remove only artifacts created by this work.
- [x] Reproduce the original command against the fixed binary and confirm it fails early with the literal spelling and never emits plan JSON.
- [x] Commit documentation or review fixes, push the branch, and create a stacked PR against `chore/manual-non-linux-ci` referencing issue #352. State explicitly that automatic CI is Linux-only and no manual Windows/macOS run was dispatched.

## Plan self-review record

### Round 1: coverage and ordering

Traced every production `.replace('\\', "/")` in `hoimin-cli` to its input source and downstream map/read. The plan covers all native filesystem and decoded Git outputs and requires validation before map insertion. It excludes core canonicalization, which receives already-validated logical paths, and separates Git output from Windows-native paths.

### Round 2: compatibility and error surface

Checked accepted Windows paths, stable error families, schemas, and glob semantics. The review found that glob backslashes can be escapes, so the plan now preserves `validate_pattern` and tests escaped metacharacters while validating only its concrete walk results. No new public error code or serialized field is required.

### Round 3: evidence and operations

Checked each stated failure against a public or same-premise fixture: raw Unix filesystem, public CLI, workspace preflight, raw/C-quoted Git, and real Git. The review removed a brittle source-text unit test in favor of a final classification audit. It also makes the shared Cargo target, no-default mutation run, no Lean artifact, stacked Linux-only CI base, and manual non-Linux non-dispatch explicit.

## Implementation self-review record

### Round 1: contract and compatibility

Reviewed every production call to the new helper against its path source. Native CLI/filesystem inputs use host-aware conversion, decoded Git output always rejects a backslash, and glob patterns retain their escape grammar. Existing error families, exit codes, plan schemas, and accepted `/` paths remain unchanged; the Windows branch preserves native separator conversion.

### Round 2: ingress ordering and collision evidence

Traced each concrete path to its next map insertion or read. All filesystem, fingerprint, Git, and manifest paths are now validated before those operations. This review added a direct workspace colliding-pair regression in addition to target discovery, then reclassified the only remaining backslash replacements as the Windows-native helper and the glob validator's non-stored safety projection.

### Round 3: tests and operational risk

Confirmed red failures for the original rewrite at CLI, fingerprint, filesystem, workspace, public plan, raw Git, C-quoted Git, and real-Git boundaries. The full Rust workspace, Clippy with warnings denied, formatting, and 845 Python tests pass; the Python process-group cases were rerun outside the restricted sandbox after the sandbox returned exit 126. A final Rust rerun initially exposed that the test fixture's temporary `.venv` link had been removed too early; restoring it made the isolated failure and the full workspace pass, confirming a fixture-setup cause rather than a product regression. No mutation run or new Lean artifact was warranted. The accidental worktree-local Cargo cache, reproduction directory, and temporary virtual-environment link were removed; no Windows/macOS Action was dispatched.
