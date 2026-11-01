# Task 1 report — #974

Scope: reject shared mutation descriptors before permission changes and writes; retain parent handles and file identity guards; preserve external readonly permissions through removal and cleanup. Base: `8b1f5fff`; branch: `issue-971-979-cli-analyzer-resource-safety`.

Skills read: systematic-debugging, test-driven-development and writing-good-tests reference, verification-before-completion. Approved design supplied by controller; no additional approval gate. Controller explicitly reserves full integrated suite until all nine tasks, so task verification is focused.

## Requirements/design: five completed self-review passes

1. Acceptance audit: read full authoritative `/tmp/hoimin-design-audit-5d3aee95/impl-974.json` and task brief. Direct application and application after reset must reject; automatic detachment is prohibited.
2. Write-boundary trace: `apply_mutation` validates bytes/candidate, then `MutationFile::into_writable` changes permissions and returns the descriptor later truncated/written. Shared-descriptor validation belongs inside this private boundary, before `make_file_writable`.
3. Handle/identity review: Unix retains parent only on permission-denied reopen and compares dev/ino after reopen; Windows retains parent and compares handle identity. Preserve those branches and validate final descriptor again after reopen.
4. Regression design review: original-source and independent sentinel tests cover direct/reset paths separately; readonly sentinel checks the permission side effect. All invoke real workspace/filesystem behavior and compare literal original bytes plus captured external permissions.
5. Cross-platform/property review: Unix uses descriptor metadata `nlink`; Windows must use `GetFileInformationByHandle`'s `nNumberOfLinks`. Named tests compile on Unix/Windows; bounded property cases add 1–8 additional aliases to detect an erroneous `links == 2` check. Existing independent/readonly/mismatch tests provide controls. Native Windows unavailable on this macOS host.

## TDD RED

Command (all Cargo commands use `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`):

`cargo test --offline --locked -p hoimin-cli --lib workspace::mutation::tests::shared_mutation -- --skip additional_aliases`

Result: exit 101; **0 passed, 5 failed**. Four original/sentinel direct/reset cases observed literal `mutated!\n` instead of `original\n`; readonly sentinel observed mode 33188 (0100644) instead of 33060 (0100444). Failures were behavior assertions, not build/setup errors. Tests were written before production changes. Bounded property shares the same behavioral helper; deliberately omitted from RED to avoid redundant shrinking/persistence after five independent regressions reproduced the defect.

## Implementation: five completed self-review passes

1. Ordering review: new descriptor link validation runs immediately before `make_file_writable`, so the RED readonly side effect is prevented. Validation remains after candidate/hash checks, preserving their diagnostic priority.
2. Unix handle review: `File::metadata().nlink()` queries the opened file; no path metadata is trusted. Retained parent, no-follow/nonblocking open, and dev/ino equality remain unchanged.
3. Windows handle review: `GetFileInformationByHandle` reads `BY_HANDLE_FILE_INFORMATION.nNumberOfLinks` from a live handle and propagates errors. Existing reparse checks, relative opens, share/access flags and identity comparison are untouched.
4. Final-descriptor review: link validation repeats after the existing reopen branches and before returning a writable handle; no truncate/write is reachable after a shared descriptor rejection. Normal independent files still follow the existing permission/write behavior. No reset detachment was introduced.
5. Error/test review: `MutationTargetShared { path }` carries the logical target and uses `workspace.mutation.shared`; conversion to `EffectFailure::InvalidMutation` preserves that path. Regression assertions now require this error, unchanged worker bytes, and unchanged external bytes/mode. Additional alias property covers counts greater than two. No dependency changes or unrelated refactoring.

## Cleanup review and controller ruling

After initial GREEN (six new regressions passed), Windows source review identified a downstream permission side effect during cleanup: both `windows::remove_file` and `remove_open_entry_io_with_guard` unconditionally cleared readonly. Controller explicitly included narrow protection in Task1: shared handles must be unlinked without chmod; return existing cleanup error if unsupported. Preserve single-link readonly cleanup behavior. Unix public `WorkerWorkspace::remove` had the same unconditional chmod; Unix recursive cleanup already unlinks without changing regular-file permissions.

Five additional requirements/design reviews performed for that ruling:

1. Traced ordinary drop through `try_cleanup` → `clear_for_cleanup` → `remove_non_directory` → Windows `remove_file`; managed removal uses the separate guarded handle function. Both Windows paths need protection.
2. Inspected Unix `remove_non_directory`: existing metadata/identity checks followed by unlink, with no file chmod. No change needed in recursive Unix cleanup.
3. Inspected Unix public `remove_file`: readonly shared regular files were chmodded before unlink, despite unlink requiring parent permissions. Narrowly skip existing chmod for nlink > 1.
4. Inspected Windows `mark_delete_by_handle`: existing extended disposition uses `IGNORE_READONLY_ATTRIBUTE`, followed by legacy retained-handle fallback. Neither fallback should be preceded by chmod for shared files; propagate failure instead.
5. Reviewed test observability: extend every sharing regression through explicit cleanup and drop, checking external bytes/mode afterwards. Add separate shared-readonly removal test, and Windows-only guarded-removal test. Windows tests allow safe unlink failure; require external readonly/bytes invariant regardless.

Second RED command:

`cargo test --offline --locked -p hoimin-cli --features contracts --lib workspace::mutation::tests::removing_shared_readonly_target_preserves_external_permissions`

Result: exit 101; **0 passed, 1 failed**. Observed permission 0100644 instead of 0100444 at the external sentinel. This test was added and executed before cleanup production changes. Log: `/tmp/hoimin-design-audit-5d3aee95/task-1-cleanup-red.log`. Windows-only test was written before its implementation but cannot be executed natively on this host.

Five additional implementation reviews performed:

1. Unix remove now reuses the opened file metadata for `is_file` and nlink, skipping chmod only for shared regular files; parent retention and no-follow open are unchanged.
2. Windows ordinary removal reads links from its live pinned handle before deciding whether to make writable; metadata failures propagate with logical path.
3. Windows guarded removal checks live handle link count only when readonly, skips chmod on shared handles, and preserves guard callbacks and unchanged extended/legacy deletion routines.
4. Shared link-count helper renamed `file_link_count` since mutation and deletion both use it; no new path-based inspection and no duplicate unsafe query implementation were introduced.
5. Single-link readonly chmod remains in both platforms' removal functions; no broad cleanup redesign, detachment or compatibility mechanism was introduced. Tests assert external state before and after cleanup/drop.

## Verification commands and evidence

All Cargo commands used the four resource environment variables listed above, offline/locked where supported, existing target directory, and ran one at a time.

- Initial GREEN: `cargo test --offline --locked -p hoimin-cli --lib workspace::mutation::tests::shared_mutation` — exit 0; **6 passed**. `/tmp/hoimin-design-audit-5d3aee95/task-1-green.log`.
- Initial contracts unit check: `cargo test --offline --locked -p hoimin-cli --features contracts --lib workspace::mutation::` — exit 0; **11 passed, 1 ignored**. `/tmp/hoimin-design-audit-5d3aee95/task-1-mutation-contracts.log`.
- Initial contracts integration check: `cargo test --offline --locked -p hoimin-cli --features contracts --test workspace_handler --test workspace_stream --test lean_workspace_oracle` — exit 0; **40 passed** (36 handler, 2 stream, 2 Lean lifecycle oracle). `/tmp/hoimin-design-audit-5d3aee95/task-1-workspace-contracts.log`.
- `cargo fmt --all` — exit 0; formatted only touched source.

Initial RED evidence is preserved at `/tmp/hoimin-design-audit-5d3aee95/task-1-red.log` (transcribed from tool output, duplicate assertion frames condensed); later logs are direct tee captures.

Final verification after cleanup expansion:

- `cargo test --offline --locked -p hoimin-cli --features contracts --lib workspace::mutation::` — **12 passed, 1 ignored**, exit 0. `/tmp/hoimin-design-audit-5d3aee95/task-1-mutation-contracts-final.log`. One repeat was justified after adding test-only restoration of setup permissions, to avoid Windows temporary-directory leaks after invariant assertions.
- `cargo test --offline --locked -p hoimin-cli --features contracts --test workspace_handler --test workspace_stream --test lean_workspace_oracle` — **40 passed**, exit 0. `/tmp/hoimin-design-audit-5d3aee95/task-1-workspace-contracts-final.log`. These integration binaries do not include the subsequently adjusted unit-test teardown helper; production code remained unchanged after this run.
- `cargo fmt --all -- --check` — exit 0.
- `git diff --check` — exit 0.

## Verification: five completed self-review passes

1. RED/GREEN evidence audit: five initial named regressions failed for actual original/sentinel byte corruption or readonly permission changes. The additional removal regression independently failed for mode 0444→0644 before the cleanup fix. Final tests require the explicit target/path diagnostic and check external bytes/mode through cleanup/drop.
2. Control audit: final contracts integration run includes `mutation_checks_hash_exact_original_and_span`, `mutation_updates_a_read_only_regular_file`, `rejected_mutation_does_not_make_the_target_writable`, `mutation_rejects_linked_target_with_matching_bytes`, `resets_read_only_file`, `dropping_a_worker_removes_read_only_files`, and `file_apis_create_nested_replace_read_only_and_remove_files`; all passed. This verifies normal writes, readonly handling, mismatch diagnostics, symlink refusal and ordinary cleanup remain supported.
3. Handle/race audit: `parent_replacement_mutation_uses_the_opened_parent` passed with contracts. Source diff retains Unix dev/ino checks and both retained-parent branches; Windows relative opens, identity comparison, pinned handle access and sharing flags are unchanged. Final link validation precedes writable handle return.
4. Snapshot/stream audit: both `workspace_stream` tests and both `lean_workspace_oracle` cases passed. Same-content hardlinks remain rejectable after reset; differing-content external hardlinks are restored without writing their referent. No reset policy or snapshot format changed.
5. Platform/property audit: property uses 16 bounded cases with 1–8 extra links plus named two-link cases. Native Windows test and common tests cover ordinary/guarded cleanup in source, but were **not executed on Windows**. Installed targets include Windows GNU, but `x86_64-w64-mingw32-gcc` is unavailable; no cross-build/native claim is made. New Windows struct field/API spelling was checked against the locally cached windows-sys 0.61.2 declaration (`nNumberOfLinks: u32`).

## Completion preparation: five completed self-review passes

1. Authoritative acceptance checklist re-read: baseline-equivalent and reset original/sentinel routes have distinct named tests; each compares bytes and platform permission fingerprint, then verifies unchanged external state after cleanup/drop. Readonly external target has its own case. Shared mutation targets reject rather than detach.
2. Scope/diff audit: only four allowed workspace source files changed: `mod.rs` error/mapping, `mutation.rs` tests, `root.rs` private guard and shared removal, `root/windows.rs` descriptor query/removal guard/test. No plan, ledger, dependency, snapshot or broad documentation changes. This requested report remains at the ignored `.superpowers` location.
3. Failure-path audit: link-query errors propagate; shared readonly deletion never makes an external referent writable. Windows uses the retained handle and existing ignore-readonly extended disposition; a platform that rejects safe unlink reports existing deletion/cleanup failure. Single-link readonly cleanup retains existing chmod behavior.
4. Test quality audit: real workspace and hardlinks, literal bytes and captured permission fingerprints; no mocks or source-text assertions. Each finite route is a named test; bounded property varies alias count independently. Tests restore their own initial sentinel permission setup only **after** final cleanup/drop invariants, allowing Windows TempDir cleanup without hiding production side effects.
5. Handoff audit: full integrated suite remains reserved for controller after all nine tasks (reported baseline 979 passed, 20 ignored). All executed checks and both expected RED failures are named here, no unexpected failures omitted. Remaining concern is Windows runtime verification; no architectural ambiguity or pending user approval remains. Commit will contain only the four source files, with the requested resource environment set for hooks.

## Outcome and limitations

Shared mutation targets now report `MutationTargetShared` / `workspace.mutation.shared` before chmod or mutation write, and external readonly targets remain protected during removal/cleanup. Focused macOS contracts tests passed. Windows implementation and regression tests are provided but require native CI execution; safe readonly shared unlink may return existing cleanup errors on unsupported Windows filesystems rather than changing external permissions.

Commit: `0202878e466ed86bdbc48ddb4c5267a858b04d6a` — `fix(workspace): reject shared mutation targets before writes`.

Commit preparation initially hit sandbox `Operation not permitted` creating `.git/worktrees/.../index.lock`; the authorized escalated commit succeeded. Resource environment variables were set for the commit and inherited by hooks. No approval review rejection occurred.

Repository `prek.toml` hooks all passed:

- `cargo fmt --all -- --check`
- `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`

Post-commit `git rev-parse HEAD`, `git show --stat HEAD`, `git status --short` and `git diff HEAD --check` verified the intended four-file commit and a clean tracked working tree. The report is intentionally left at the requested ignored path; all test logs are retained under `/tmp/hoimin-design-audit-5d3aee95/task-1-*.log`.

Handoff status: **DONE_WITH_CONCERNS** solely because native Windows runtime verification (including `removing_shared_readonly_entry_preserves_external_attributes`) was unavailable. This is an explicitly reported platform limitation, not a pending implementation task. All requested macOS checks and self-review passes are complete.
