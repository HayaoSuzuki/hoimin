# Issue 516: Share active session ownership-tree identities

Status: implemented and verified on macOS; execution evidence is in the implementation plan. Issue: https://github.com/tokyogas-tech/hoimin/issues/516

Source review and public CLI observations used main `61c654fd87cd2536082a101052d68e97d69928ca` on macOS. With `calc.py` containing `value = True`, an existing `actual-locks` directory and `.session.db.hoimin-locks -> actual-locks`, a session run fails before baseline with `workspace.original.changed` naming `actual-locks/<hash>.lock`. The source is unchanged and the lock was created by hoimin itself. Jobs 1 and 2 reproduce; an ordinary ownership directory, an outside-root target, and explicit exclusion of `actual-locks/**` are positive controls. Native case-insensitive alias spelling reproduces the same defect. These observations establish the missing canonical ownership-tree exclusion.

Implementation used main `6e78871`, including issues 513, 514 and 515. The plan is [the implementation plan](../plans/2026-09-12-issue-516-session-ownership-identities-plan.md).

## Required result

A session using a preexisting ownership-directory alias must not mistake its own lock creation for an original project edit. Both workspace exclusion and metrics collision protection must consume the same active ownership-tree identities. Keep actual ownership locks, phase ordering, ordinary fixture/source integrity and safe metrics leaf replacement.

## Smallest concrete change

Add a fallible ownership-tree accessor beside `SessionArtifacts::lock_directory`, for example:

```rust
pub fn lock_trees(&self) -> std::io::Result<Vec<PathBuf>>
```

Return the constructed lexical `lock_directory()` first. Resolve that tree with `std::fs::canonicalize`; append the existing canonical target when distinct. Treat NotFound as an as-yet-uncreated tree (lexical entry remains sufficient), exactly as the existing metrics helper does. Propagate other canonicalization errors; do not invent an alias path, broaden to a filename prefix or swallow permission/identity failures.

Keep `SessionArtifacts::resolve` and `lock_directory()` behavior unchanged. A fallible accessor avoids introducing additional tree I/O into SessionHandler::open or context construction merely because it resolves a database. The accessor is called only in existing blocking preflight/inspection paths. Public addition is additive; existing constructor signatures and database/sidecar methods remain unchanged. If project convention favors crate-private exposure plus unit tests, that visibility is sufficient for the two production consumers; no broader abstraction is required.

Consumers:

1. `shell::prepare_session_artifacts` obtains the tree list before constructing literal exclusions. Chain each tree with `tree=true`, alongside the four existing exact DB/sidecar paths. Independently map each path against the canonical project root, so an outside-root lexical alias can still contribute an in-root canonical target. Preserve normalized root-relative validation and the existing `session.path` failure boundary. Do not move session opening/locking ahead of snapshot creation.
2. `metrics_destination::session_artifacts` replaces its private canonical-tree expansion with the shared accessor. Preserve its configured DB entry protection, Windows configured sidecar extras, confirmed-collision-first behavior and withheld/write-warning semantics for uncertainty. No raw output-path write fallback.
3. Leave RunOwnership acquisition, lock file names, lifecycle events, SQLite open and resume policy unchanged. It can continue to use the existing lexical lock directory; both aliases refer to the same actual ownership tree established before snapshotting.

No CopyOptions or generic walker-policy redesign is necessary. Existing Tree exclusions already prune the real directory and its descendants for inventory/copy/snapshot/rechecks and resist includes. A lexical symlink is already skipped by the walker (`manifest.rs:168–201,351`); it is not copied as an active file. Do not change ordinary symlink policy just for this fix.

## Error and boundary rulings

- New lock tree: absent canonical target is normal; do not create it during preflight.
- Existing directory symlink/case alias: include actual canonical spelling and target, plus lexical identity.
- Existing inaccessible tree: fail safe at existing blocking session-preflight error boundary when exclusions cannot be determined. Metrics inspection continues using its existing conservative withheld/error rules.
- Dangling tree symlink: preserve existing NotFound handling of the shared metrics algorithm; later ownership creation may reject it. Do not silently create a guessed referent or redirect ownership. If a stricter early rejection is desired, present it as a separate behavior decision with a test.
- Canonical ownership tree equal to project root/ancestor: do not exempt the entire project or hide selected sources. Keep root-relative validity and fingerprint invariants; reject an unrepresentable/unsafe exclusion explicitly. Add a narrow diagnostic test if touching this case, rather than waiving source protection.
- No arbitrary concurrent filesystem-replacement guarantee. Resolve preexisting aliases within the existing preflight phase; do not add locking of user directories, new core phases or automatic parent creation.
- Do not change all DB fixtures or similarly prefixed folders. Only the active lexical/canonical ownership trees and existing exact DB/sidecars are exempt.

## Regression matrix (RED first, then focused GREEN)

Public CLI cases, CPython with bounded baseline/total watchdogs and temp roots:

1. Preexisting `.session.db.hoimin-locks -> actual-locks` inside root, empty actual directory; run jobs=1 and jobs=2. Current RED: baseline null, workspace.original.changed actual-locks/<hash>.lock. Fixed: baseline Exit(0), complete, expected candidate results, unchanged source.
2. Same alias with include `**` and ordinary fixture DB/similarly named directory present. Assert actual lock tree absent from worker copy, unrelated fixture readable and selected candidates retained.
3. Case-insensitive native filesystem: preexisting `.SESSION.DB.HOIMIN-LOCKS`, requested session.db. Use a capability check that distinguishes unsupported/case-sensitive hosts; do not count a skipped platform case as native success. Keep the exact original native RED diagnostics.
4. Lexical tree outside root, canonical target inside root; and lexical tree inside root, canonical target outside. Map each identity independently. Preserve outside-root directory contents except actual owned locks.
5. New ordinary tree, normal existing tree, DB-parent/root alias, literal metacharacter DB name; these remain positive controls.
6. Natural incomplete/resume using alias tree: reusable killed result and retried timeout; ownership contention test remains effective. Reuse existing bounded session helpers; do not add sleeps without a deadline.
7. Controlled original source and ordinary fixture edits still trigger workspace.original.changed. No blanket exemption of sibling fixtures.
8. Metrics output under lexical and canonical lock trees still rejects before baseline and never writes. Ordinary output and distinct safe leaf links remain successful.

Accessor/direct policy tests should verify no artifacts are created, stable lexical-first order/dedup, missing tree result, existing canonical/case aliases as supported, and propagation of real non-NotFound errors using existing portable fault fixtures when available. Avoid tests that simply duplicate the implementation's returned expression without observing filesystem identity.

Focused checks: affected session_handler/run_e2e/metrics_destinations tests, shell no-project-I/O constructor regression, workspace literal exclusion tests, existing ownership tests. Then controller-scheduled serial `cargo test --workspace --all-features`, fmt and strict all-target/all-feature Clippy using the approved target directory. No Cargo from this read-only design turn. No new Lean/CI generator needed: no new state transition or classifier; filesystem identity must be checked with real paths and CLI evidence.

## Three design review passes

1. Ownership/phase review: accessor placement shares session naming without changing when DB creation or locks occur. Existing constructor no-I/O guarantee remains. Canonical target is determined before snapshot; actual ownership still uses the same DB-derived namespace.
2. Alias/scope review: both lexical and canonical trees are retained, mapped independently to root, and consumed by both features. Case-alias and outside-to-inside cases reject a fix that only handles lexical in-root symlinks. Existing Tree matching and symlink skipping are adequate; no unrelated copy-policy changes.
3. Failure/verification review: preserve missing-tree allowance and non-NotFound error propagation; never authorize uncertain metrics output or exclude the whole project. Public RED/GREEN, ordinary-source/fixture controls, resume/ownership and metrics collision regressions establish correspondence. Native case behavior is reported honestly, without treating a skipped Windows/Linux check as proof.

## Implementation correspondence and limits

`SessionArtifacts::lock_trees` now returns lexical-first, deduplicated existing canonical identities without creating directories. The two consumers are the existing blocking session preflight and metrics inspection. Session opening, ownership acquisition, lock naming and constructors retain their previous behavior.

The public regression failed before the fix for jobs 1 and 2 with no baseline and `workspace.original.changed actual-locks/<hash>.lock`. After the fix, both runs have a successful baseline and one killed mutation. The expanded matrix checks include overrides, a real SQLite fixture, a similarly named directory, both root-boundary alias directions, native case aliasing, a natural incomplete/resumed session, and source/fixture edits. Metrics rejects both lexical and canonical destinations inside the active tree before baseline, preserving the existing file bytes.

A tree resolving to the project root is rejected by the existing `workspace.path.invalid` boundary for the empty relative tree path. A self-referential alias fails at `session.path`. Both failures occur before baseline and without creating the session DB. The implementation does not change these phases merely to make their diagnostic codes identical.

macOS native case-insensitive alias capability was confirmed and the case ran successfully. Case-sensitive hosts explicitly report that this one native case is unavailable. Linux and Windows native execution was not performed locally; portable/cfg-gated tests are retained for their existing CI lanes. This patch does not guarantee identity stability against arbitrary concurrent filesystem replacement. No Lean model was added because the change shares existing filesystem identity resolution without adding state transitions.
