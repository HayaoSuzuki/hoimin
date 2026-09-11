# Issue 472 implementation plan

Spec: ../specs/2026-09-11-issue-472-session-artifacts-design.md

Global constraints: independent worktree from origin/main; no merges. Three self-reviews per stage. Controller owns design/plan/OKF; implementer owns Rust source/tests and relevant README behavior documentation. No cargo-mutants. Preserve lock enforcement, source integrity, current-directory path semantics and user copy-policy precedence.

## Plan self-review

1. Dependencies: resolve/names policy in the existing blocking preflight first, thread it into the stored copy options before the preflight snapshot, then verify public session integration. The session open path must consume the same resolved name used by exclusions.
2. Coverage: pair active-artifact exclusion with ordinary DB copying/change detection, include restoration and exact-name negatives; pair new-session success with resume/multiworker and live ownership refusal.
3. Scope and verification: one cohesive multi-file task avoids concurrent edits across session/shell/workspace. Focused tests cover path and actual lifecycle boundaries; full suite/Clippy follow focused green. No new ownership state-machine change is planned.

## OKF self-review

1. Read session/report ownership and runtime snapshot contracts against active session, ownership and workspace implementation; historical Lean evidence remains scoped to its original model.
2. Add an issue-specific source for exact active-artifact selection, with a captured source hash. Preserve unrelated source revisions and distinguish intended contract from new execution evidence.
3. Validate reserved files, YAML/source-footnote links, reachability, local paths, new source hashes and all design index entries/count after edits; append final implementation evidence only after checks run.

## Task 1: Integrate active session artifacts with workspace selection

Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-472`, base `4adf809`. Own session module/ownership, shell setup, workspace options/manifest and necessary option constructor callsites, focused integration tests, README session explanation. Controller owns docs/superpowers and docs/knowledge. No subagents.

- [x] Read spec and /private/tmp/issue472-preflight-notes.md; inspect current code, reproduce the actual in-root baseline failure in a public regression before production edits. Preserve useful raw RED evidence.
- [x] Implement one session-owned artifact path/naming description. Resolve existing DB or existing parent+new leaf without creating DB; preserve CWD-relative semantics and aliases. Make SQLite/ownership consume the same resolved database. Avoid unresolved/dangling aliases silently diverging from excluded paths.
- [x] Add small typed literal file/tree exclusion policy to CopyOptions; update bounded callsites with defaults. Validate normalized relative rules, share predicate across both walks, preserve normal includes/excludes/default policy. Store policy through all WorkspacePlan inventory/snapshot/recheck paths. Exact active files + lock tree only; no broad globs or recheck-only exception.
- [x] Direct tests: in/out root, new/existing, exact names versus similar prefixes/subdirectory matches, file/tree distinction, supported metacharacter names, include restoration, canonical root/parent/DB aliases, platform path behavior. Assert copied ordinary DB fixture bytes and omission of active artifacts; genuine source/ordinary fixture changes still fail.
- [x] Actual public CLI tests: absolute and CWD-relative session paths, new/existing DB, jobs2, naturally incomplete session then resume, baseline plus actual mutant execution. Reuse existing failure-safe subprocess helpers; do not mutate process-wide CWD/environment during parallel tests. Retain or extend real live ownership refusal coverage. Use bounded synchronization rather than fragile sleeps where concurrency is needed.
- [x] README documents exact active-artifact exclusions and ordinary DB behavior. Review against implementation; do not claim all-platform execution from local tests.
- [x] Three concrete implementation reviews and three test reviews, with findings and resolutions in task report. Run focused red/green, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`. Every Cargo uses `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`. `.venv` symlink provides CPython3.14.7; never commit it. No other implementer using Cargo.
- [x] Commit owned implementation/tests/README only, no push or PR. Write complete task report with exact command/result counts, raw logs `/private/tmp/issue472-*.log`, three reviews per owned stage, limitations, and commit hash. Controller handles review/publication. No extra full-suite reruns after green without a change/failure that requires one.

If code inspection reveals a material incompatibility in the proposed path policy, report the concrete issue and a small alternative while continuing independent work; do not silently alter lock semantics.

## Integration review adjustment

The first full run stopped at the pre-existing constructor regression after 61 passing run_e2e tests. Keep that regression unchanged. Resolve the DB and literal exclusions within the existing blocking preflight operation, before `handle_preflight_validated`; return the resolved session path in its existing workspace completion and install it before the core receives `PreflightCompleted`. Keep cleanup/cancellation ownership and lazy session opening unchanged. Focused constructor/preflight checks and another complete workspace run must pass after this adjustment.

## Implementation and verification record

Controller independently read the final full-workspace log: 69 result groups, 1,616 passed, 0 failed, 13 ignored. Final fmt and all-target/all-feature Clippy passed. The initial failing run and superseded intermediate run are not final verification evidence.


Status: DONE. Commit `ced6fd3` — `fix: exclude active session artifacts from workspace snapshots`.

## Implemented

- Added session-owned `SessionArtifacts`: resolves an existing database or existing parent plus new leaf without creating SQLite files; rejects dangling final symlinks and directories. Database, WAL, SHM, rollback journal and ownership-directory names derive from the resolved path.
- Shell resolves active artifacts inside the existing blocking Preflight effect before snapshot creation and passes the same resolved database name to lazy session opening through the existing OwnedWorkspace completion. Constructor behavior is unchanged. Relative inputs remain relative to invoking CWD. Canonical root/path comparison reuses workspace component handling, including its Windows case fallback.
- Added typed literal file/tree exclusions to stored `CopyOptions`. Both normal and include-restoration walks use the same predicate; the lock directory is pruned. Existing normalized-relative-path validation is reused, and empty exclusions bypass per-entry path work. Normal selection/default exclusions are unchanged.
- SQLite opens the resolved database; lock naming moved behind the shared artifact object without changing lock acquisition, run eligibility, transactions or release behavior.
- README documents exact active artifacts, include precedence, ordinary fixture integrity, relative paths, aliases and retained ownership.

## TDD evidence

All Cargo commands used this exact prefix from worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-472`:

`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`

1. RED before production edits: `cargo test -p hoimin-cli --test run_e2e active_session_artifacts_allow_real_cli_in_root -- --nocapture`, log `/private/tmp/issue472-red.log`: 0 passed, 1 failed, 59 filtered. Real child CLI reported `workspace.original.changed` for the newly created ownership lock; baseline was not reached. This was the intended defect.
2. First implementation run, same command, log `/private/tmp/issue472-focused.log`: baseline and killed mutant succeeded, but the original test's success-exit assertion failed because max-mutants 1 produces a naturally incomplete run (exit 4). Fixed the expectation and used that natural incompleteness to exercise resume.
3. Initial matrix execution also exposed a test-only scheduling assumption: jobs 2 can kill a candidate other than array index zero. Changed the assertion to identify the actually killed candidate and verify that exact candidate is reused with null termination on resume.
4. GREEN: `cargo test -p hoimin-cli --test run_e2e active_session_artifacts -- --nocapture`, log `/private/tmp/issue472-cli-green.log`: 3 passed, 0 failed, 59 filtered. Includes 8 combinations of new/existing, absolute/CWD-relative, in/out-root; each runs and naturally resumes with jobs 2. Other tests exercise root/parent/DB aliases and actual original source/fixture edits.
5. Direct initial GREEN: `cargo test -p hoimin-cli --test workspace_handler literal_ -- --nocapture`, log `/private/tmp/issue472-workspace.log`: 2 passed, 0 failed, 31 filtered.
6. Direct final GREEN: `cargo test -p hoimin-cli --test session_handler --test workspace_handler`, log `/private/tmp/issue472-direct-green.log`: session_handler 28 passed, 1 preexisting ignored subprocess fixture; workspace_handler 33 passed. No failures.
7. Ownership GREEN: `cargo test -p hoimin-cli --test run_e2e concurrent_real_cli_runs_refuse_live_session_ownership -- --nocapture`, log `/private/tmp/issue472-ownership-green.log`: 1 passed, 61 filtered. Existing failure-safe bounded readiness test now uses an in-root DB, so exclusions coexist with real live ownership refusal.
8. `cargo fmt --all -- --check`, log `/private/tmp/issue472-fmt.log`: exit 0; repeated after final plumbing in `/private/tmp/issue472-fmt-final.log`.
9. `cargo clippy --workspace --all-targets --all-features -- -D warnings`, log `/private/tmp/issue472-clippy.log`: exit 0, no warnings.
10. `cargo test --workspace --all-features`, log `/private/tmp/issue472-full.log`: first run stopped at run_e2e, 61 passed and 1 failed because constructor resolution rejected the preexisting missing-parent fixture. See fix review below. Final run log `/private/tmp/issue472-full-final.log`: exit 0, 1,616 passed, 0 failed, 13 ignored across 69 result groups, including doc-test groups; no warning/error lines.

## Three implementation self-reviews

1. Lifecycle/ownership review: traced blocking preflight path resolution into the completion-carried session path, lazy dispatcher open, SQLite configure, run lock acquisition and drop. Exclusion discovery performs no writes; all naming shares the resolved DB. No acquisition flags, hash algorithm, resume eligibility or transaction changes. Rejected directory paths before the lock helper's filename invariant could panic.
2. Copy-policy review: traced inventory, original snapshot, worker creation and verify_originals through stored options and walk_selected_files. Both traversals share selection_filter, user includes cannot restore literals, exact files do not prune matching directories, and tree rules do not hide an ordinary file at the same exact name. Adopted controller feedback to reuse normalized_relative_path and preserve the sessionless empty-policy fast path.
3. Paths/scope review: checked absent leaf versus dangling symlink, canonical parent/root/DB aliases, native filename construction with OsString, normalized native-to-portable conversion, Windows existing component fallback, and nested/similar names. Extracted bounded shell helper to keep preparation readable. No new dependencies or lifecycle model; existing large shell/test modules were changed locally without restructuring.

## Three test self-reviews

1. Regression validity: captured actual public CLI RED before implementation. Corrected the incomplete exit expectation and jobs-2 scheduling assumption based on observed reports. Matrix verifies baseline exit zero, actual killed result, natural incomplete status, same resumed run ID and actual result reuse, without SQL-forced incompleteness.
2. Precision/integrity: direct test checks copied fixture bytes, active-artifact absence, aggregate size, active-file updates and new lock creation between snapshot and copy, explicit include restoration, metacharacter and similar-prefix negatives, nested same names, file/tree distinction and invalid normalized paths. Source and ordinary fixture edits match OriginalChanged; a separate real CLI test checks the public workspace.original.changed diagnostic for both.
3. Process/platform review: subprocess-local current_dir preserves invoking-CWD semantics without changing process-wide CWD/environment. Child commands use kill_on_drop, and live ownership retains the existing readiness and teardown helper. Generic CLI matrix can compile on supported platforms; Unix-only symlink/metacharacter tests are explicitly gated. Local execution is macOS with the provided CPython 3.14.7 .venv symlink; Linux/Windows native execution was unavailable and is not claimed.

## Owned files

README.md; crates/hoimin-cli/src/{plan.rs,shell.rs,session/mod.rs,session/ownership.rs,workspace/mod.rs,workspace/manifest.rs,workspace/copy.rs}; crates/hoimin-cli/tests/{run_e2e.rs,session_handler.rs,workspace_handler.rs}.

Controller-owned docs/superpowers, docs/knowledge and untracked .venv are excluded from the implementation commit. No cargo-mutants, process-wide environment changes, new lock/lifecycle behavior, push or PR.


## Preflight placement fix and three additional self-checks

The first full suite exposed `shell_context_construction_performs_no_project_io`: missing-root/missing-session-parent configuration historically constructs a context successfully and creates neither path. Historical references are `docs/superpowers/specs/2026-08-24-rust-toolchain-reproducibility-design.md` (controller cited lines 266–269) and `docs/superpowers/plans/2026-08-24-rust-toolchain-reproducibility.md:655`. Controller briefly approved adapting that test to accept earlier failure, then reconsidered in favor of retaining its exact original behavior. An already-started intermediate pipeline using that adaptation completed; `/private/tmp/issue472-full-superseded.log` is explicitly superseded evidence, not verification of final code. The adaptation was reverted completely.

Final fix: resolve artifacts in `BlockingEffect::Preflight::execute`, set the workspace literal policy before `handle_preflight_validated` creates its plan, and attach the resolved session path to its existing OwnedWorkspace completion. Acceptance installs that path before returning PreflightCompleted to the core. Cleanup completions carry no session path. Discovery errors return EffectFailed(session.path) plus owned workspace, preserving cleanup. No new state-machine transitions or lock semantics were introduced; shell construction performs the same no-project-I/O behavior as before.

1. Constructor and read timing: compared constructor test to HEAD and retained it unchanged; moved the helper call out of setup and onto the existing blocking preflight thread. `cargo test -p hoimin-cli --all-features --test run_e2e shell_context_construction_performs_no_project_io -- --exact` passed 1/1, 62 filtered (`/private/tmp/issue472-constructor-final-green.log`).
2. Ownership handoff and failure cleanup: inspected both OwnedWorkspace constructors and sole acceptance site. Both success and failure restore workspace ownership, only successful preflight carries the canonical session path, cleanup leaves it alone, and path discovery creates no artifacts. New real CLI missing-parent test proves parseable failure, no baseline, incomplete report and absent parent. `cargo test -p hoimin-cli --all-features --test run_e2e active_session_artifacts -- --nocapture` passed 4/4, 59 filtered (`/private/tmp/issue472-cli-final-green.log`).
3. Existing asynchronous/recheck boundary: reused the current blocking-effect completion and cancellation handling; no extra task or event. `cargo test -p hoimin-cli --all-features --lib preflight -- --nocapture` passed 3/3, 598 filtered, covering preflight async-runtime isolation, source hashing and fingerprint ABA rejection (`/private/tmp/issue472-preflight-green.log`). Final fmt, Clippy and full-suite rerun are justified by this production change and the earlier failing suite; there is no redundant full rerun after final green.


## Final verification and commit

- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo fmt --all -- --check`: exit 0; `/private/tmp/issue472-fmt-final.log` is empty.
- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0, no warnings; `/private/tmp/issue472-clippy-final.log`.
- `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target cargo test --workspace --all-features`: exit 0, 1,616 passed, 0 failed, 13 ignored across 69 result groups; `/private/tmp/issue472-full-final.log`. The ignored cases are existing ignored tests/subprocess entrypoints, not newly skipped regressions.
- `git diff --check`: exit 0 before staging. Staged diff inspected: exactly the 11 owned files, 604 insertions and 33 deletions. Commit `ced6fd3` created; no push or PR.
- Implementation and tests are complete with no known outstanding correctness concerns. Platform limitation remains: all execution evidence is macOS native; no Linux/Windows native pass is claimed. No new Lean model was needed because run locking, lifecycle transitions, transactions and cancellation ownership were retained.

## PR self-review

1. Scope and behavior: inspected the production diff from `4adf809`, exact DB/sidecar/tree names, both traversal modes, empty-policy behavior and the canonical session-path handoff before PreflightCompleted. Original constructor behavior and lock calls are preserved; no general integrity bypass was added.
2. Evidence and limits: checked the intended public RED, direct and public GREEN, failed first full run, final placement correction, and fresh complete final suite. Native evidence is macOS/CPython3.14.7; generic code/tests and existing Windows path helper are not described as Windows native validation. No new Lean model or lifecycle proof is claimed.
3. Reviewability: checked issue link, README contract, design/plan/OKF links, captured source hash, 161-source index and counts, whitespace, PR template and exact owned file list. Environment symlink is excluded. Task and final branch review results are recorded separately from self-reviews.

## Controller decisions

The first full run revealed a constructor compatibility issue. The controller initially accepted a no-write test adjustment because the historical specification states that construction creates neither root nor DB. Its cost would have been earlier failure for a missing session parent. Before committing that adjustment, a bounded preflight integration was identified and selected instead: preserve the constructor test and perform path resolution before the preflight snapshot, carrying the result through the existing workspace completion. The additional handoff required focused preflight/failure checks and a fresh full suite, which passed. The intermediate test-adjusted run is superseded evidence.

## Independent review

Task reviewer `review472_task` approved base `4adf809` through implementation `ced6fd3`, with no critical, important or minor findings. Focused cross-module checks traced the lazy open path, stored copy policy and existing Windows component comparison. Native Linux/Windows limits remain explicit, and unchanged lifecycle behavior is covered by the existing final suite. Final whole-branch review is recorded in the issue tracker after completion.
