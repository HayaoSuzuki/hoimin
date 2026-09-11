# Issue 472: Exclude the active session artifacts from workspace snapshots

Issue: https://github.com/tokyogas-tech/hoimin/issues/472

## Problem and contract

Workspace preflight snapshots the project before beginning a session. Opening SQLite and acquiring run ownership then creates or changes the active DB, its sidecars, and `.<database-name>.hoimin-locks/<run-hash>.lock`. The subsequent original-workspace comparison treats these tool-owned changes as user edits and fails before baseline. The current shell passes only user include/exclude globs to workspace selection.

During the existing blocking preflight effect, before snapshot creation, determine the files and directory owned by the configured session. Exclude exactly that DB, its `-wal`, `-shm`, and rollback `-journal` sidecars, and its ownership-lock directory subtree from the copy manifest, size accounting, snapshot and every original-integrity comparison. Apply the same policy to the normal and include-restoration walks. Explicit includes cannot reintroduce active session artifacts. Other DB fixtures, similarly prefixed files and same-name files in other directories retain ordinary copy and integrity behavior.

## Path and ownership design

Keep artifact naming in the session module beside the ownership implementation. Resolve the active database path before creating any artifact. Keep `ShellContext` construction independent of project paths: perform resolution inside `BlockingEffect::Preflight::execute`, install the literal policy before the workspace plan is created, and return the resolved session path through the existing owned-workspace completion. Install that path in the shell before delivering `PreflightCompleted`, so later session effects open the same DB. Failure and cancellation retain the existing owned-workspace cleanup path. For an existing DB, use its canonical path. For a new DB, resolve the existing parent and retain the final filename. Relative session paths remain relative to the invoking current directory, as before; they are not reinterpreted relative to `--root`. Existing root/parent/database symlink aliases must resolve to the same effective DB. A dangling final symlink must be resolved deliberately or rejected before side effects, rather than treated as an ordinary absent leaf with guessed sidecars.

Use that resolved DB path for the later SQLite open and the same naming rule for ownership. Opening or locking still occurs at the existing session lifecycle boundary; path discovery must not create the DB early. The workspace root is canonicalized by existing preflight. Map actual in-root artifact paths to literal root-relative exclusions; an outside-root session contributes no in-root exclusion. Preserve platform path-component/case behavior, including Windows canonical path prefixes. Do not compare textual filename prefixes or use broad `*.db`/`db*` matching.

Carry typed literal exclusions in `CopyOptions` so `WorkspacePlan` stores the exact policy used for all inventory, snapshot and recheck paths. Distinguish exact files from a directory subtree using a small enum or record. The existing constructor sites can be updated with empty defaults; this bounded API adjustment is preferable to generating glob strings that reinterpret metacharacters. Validate normalized relative paths at the workspace boundary. The normal and include-restoration traversal share one artifact predicate, with directory pruning for the lock tree. Default virtual-environment and user ignore/include/exclude behavior stays intact.

Do not weaken or remove `RunOwnership`, change SQLite transaction semantics, or suppress `workspace.original.changed` generally. Source and ordinary fixture edits remain errors. This changes workspace selection policy, not run ownership or resume eligibility. Existing constructor behavior with a missing project/session parent remains intact; actual project validation belongs to preflight.

## Verification

Capture the original in-root failure with an actual public run before changing production code. Add direct policy tests for new and existing DB artifact paths, in-root/outside-root paths, exact-file versus lock-tree matching, similar prefixes, nested same-name fixtures, and supported filename metacharacters. Exercise includes that would otherwise restore ignored paths. Assert that active artifacts are absent from worker manifests/copies while ordinary DB fixtures remain present and inspected.

Use real CLI/session integration for absolute and relative session paths, new and existing DBs, multiple workers and resume of a naturally incomplete run. Cover canonical parent/root aliases and an existing DB symlink on supported platforms. Preserve live-session ownership refusal with a real ownership test. A controlled original source or ordinary fixture edit must still produce `workspace.original.changed`.

Run focused red/green tests, formatting, all-target/all-feature Clippy with warnings denied, and one full all-feature workspace suite after implementation. Existing session ownership tests are correspondence evidence for unchanged locking; this patch does not require a new Lean concurrency model. If implementation changes lifecycle state transitions or lock acquisition, revisit that decision before claiming completion. Linux/Windows native behavior must be distinguished from local macOS execution; lack of native capability is not a passing test.

## Design self-review

1. Cause and ordering: traced shell setup, workspace preflight, lazy session open, lock naming and manifest rechecks. Moving DB creation before the snapshot alone would leave changing sidecars and locks in copied state; shared artifact selection addresses all those paths.
2. Precision and paths: compared generated glob rules with typed paths; selected literal rules to preserve metacharacter fixtures and avoid broad exclusions. Checked current-directory semantics, canonical root handling, existing versus absent database leaves, aliases and Windows component handling.
3. Safety and scope: traced ownership acquisition and resume tests; exclusion must not alter ownership or exempt unrelated edits. The design includes real copy/verification and lifecycle regressions rather than only testing a helper. No new abstract concurrency claim is needed for an unchanged lifecycle.

## Integration review adjustment

The first implementation resolved session paths during blocking shell setup. The full suite exposed the existing `shell_context_construction_performs_no_project_io` regression: constructing a context with a missing root and session parent previously succeeded without creating either. The historical toolchain spec (2026-08-24, constructor-regression section) states the no-creation guarantee. A no-write test adaptation was considered, with earlier invalid-parent rejection as its compatibility cost. A bounded alternative was then identified and selected: resolve inside the existing blocking preflight effect and return the path with its existing workspace ownership completion. The original constructor regression remains unchanged, and artifact discovery stays before snapshot creation. No new ownership state transition is introduced.
