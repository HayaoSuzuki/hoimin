# Issue 357 Capability-Bound Worker Cleanup Implementation Plan

> **Execution:** Use `superpowers:executing-plans` task by task. This plan is
> intentionally single-agent because the active collaboration policy forbids
> delegated agents unless the user explicitly requests them.

**Goal:** Remove ambient pathname permission changes from ordinary worker
cleanup while retaining cleanup of read-only, inaccessible, deep, and
non-UTF-8 worker entries.

**Architecture:** Capture the temporary wrapper directory before mutant code
can run. Drain the worker through `WorkerRoot`'s handle-relative, no-follow
removal state machine, repair the wrapper through its retained handle, then
close capabilities and use `std::fs::remove_dir_all` only for final wrapper
removal. A small Lean interleaving model generates expected outcomes for public
and internal filesystem correspondence tests.

**Tech stack:** Rust 1.88+, Camino, cap-primitives, rustix/libc, tempfile,
Lean 4 with the repository resource guard, Serde JSONL oracle fixtures, Cargo
fmt/Clippy/test, repository Python checks.

**Spec:**
`docs/superpowers/specs/2026-09-04-issue-357-cleanup-capability-design.md`

## Global constraints

- Work only in `.worktrees/issue-357-cleanup-race` on
  `fix/issue-357-cleanup-race`.
- Follow red-green-refactor for the filesystem bug: add the exact paused race,
  observe the outside permission change on unmodified cleanup, implement the
  smallest safe behavior, and retain the witness.
- Never perform a cleanup permission change through an ambient pathname.
- Preserve the managed-worker cleanup protocol, public error codes, serialized
  schemas, non-UTF-8 support, and the 128-level worker-tree bound.
- Keep a failed cleanup retryable. Do not close ordinary-worker capabilities
  until handle-relative preparation succeeds.
- Use the repository's shared Cargo target. Do not create a worktree-local
  `target` directory.
- Run Lean commands serially with a 20-second wall deadline, 768 MiB aggregate
  RSS cap, one Lake job, and per-command stats in a dedicated `/private/tmp`
  directory. Remove that directory after recording results.
- Do not run cargo-mutants unless an implementation review finds an untested
  predicate that the Lean broken transition and deterministic race do not
  exercise. If required, limit it to one function, one job, and a disposable
  output directory, then delete the directory immediately.
- Automatic GitHub Actions remain Linux-only. Do not dispatch the manual
  Windows/macOS workflow and do not use it as a condition or merge gate.

---

## Task 1: Build the bounded Lean cleanup-capability audit

**Files:**

- Create: `formal/HoiminOracle/HoiminOracle/CleanupCapabilityModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/CleanupCapabilityProofs.lean`
- Create: `formal/HoiminOracle/HoiminOracle/CleanupCapabilityCases.lean`
- Create: `formal/HoiminOracle/CleanupCapabilityAuditMain.lean`
- Create (generated):
  `formal/HoiminOracle/corpus/cleanup-capability.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

- [ ] Model `inspect`, `bind`, `swap`, and `effect` events, plus retained and
  post-inspection capability strategies. Keep outside-object writability as the
  safety observation.
- [ ] Define the broken ambient transition so an effect resolves the current
  path after stale inspection.
- [ ] Enumerate traces shortest-first through depth 4 over the four-event
  alphabet. Record 341 raw traces per strategy and retain the first broken
  witness.
- [ ] Prove by transition induction that the correct model preserves
  `outsideWritable = false` for arbitrary traces with that initial premise.
- [ ] Add fixed cases for stable cleanup, a pre-existing outside link, an entry
  swap after inspection, an entry swap after binding, and a retained-wrapper
  swap.
- [ ] Mark each case exactly `strict`, `internal-fixture`, or `model-only` as
  specified in the design worksheet. Do not invent another mode.
- [ ] Add atomicity sensitivity witnesses for both retained-wrapper and
  post-inspection entry binding. Record idempotency as covered by the invariant
  and boundary/precedence as not applicable to this model.
- [ ] Add `--output`, `--check`, `--stats`, `--cases`, and `--sensitivity` to a
  dedicated executable. Keep bounded enumeration out of imported proof modules.
- [ ] Run model, proof, case, sensitivity, and stats commands one at a time
  through `lean_resource_guard.py`; review each stats file before the next
  command.
- [ ] Generate the corpus from Lean, review the exact diff, then run
  `--check` to prove freshness.
- [ ] Scan the new Lean files for `sorry`, `admit`, and `axiom`; expect no
  matches.
- [ ] Commit the formal model, proofs, generator, and generated corpus.

## Task 2: Add same-premise Rust oracle cases and observe Red

**Files:**

- Create:
  `crates/hoimin-cli/src/workspace/cleanup_capability_oracle_tests.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/root.rs`

- [ ] Register the test module and parse the generated corpus with
  `serde(deny_unknown_fields)`. Assert the exact ID/mode/scenario contract and
  reject unknown, crossed-mode, and duplicate rows.
- [ ] Exercise stable and pre-existing-link `strict` cases through public
  `WorkerWorkspace::try_cleanup`, observing the result and outside permission
  fingerprint.
- [ ] Expose the existing test-only race checkpoint to the workspace parent and
  call it in the current ambient cleanup walker immediately after
  `symlink_metadata` and before permission repair.
- [ ] Implement the `internal-fixture` entry-swap case: pause after inspection,
  replace the read-only worker file with an outside-pointing symlink, resume,
  and compare the complete result/fingerprint observation with the Lean row.
- [ ] Add the retained-wrapper fixture at the wrapper permission boundary. Its
  correspondence assertion covers outside permissions only; explicitly clean
  the moved original wrapper and do not claim deletion completeness.
- [ ] Run the focused oracle test against unmodified production behavior.
  Record the expected failure: ambient cleanup changes the outside permission
  fingerprint and disagrees with the generated case.
- [ ] Confirm stable and pre-existing-link cases already pass, so the red signal
  is specific to the inspection/replacement interleaving.
- [ ] Commit the failing tests only if repository convention permits a red
  intermediate commit; otherwise keep the observed output and commit with the
  first green implementation slice.

## Task 3: Retain the ordinary wrapper cleanup capability

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/copy.rs`
- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`

- [ ] Add an `OwnedWorkspaceDirectory` helper that opens the temporary wrapper
  and returns `Some(File)`; managed owners return `None`.
- [ ] Capture the handle immediately after `PendingOwnedWorkspace` is created,
  before worker-root creation and before any file-copy charge. Map acquisition
  failure to the existing workspace I/O family.
- [ ] Pass the optional handle into `WorkerWorkspace::from_materialized` and
  store it next to `WorkerRoot`.
- [ ] Audit declaration and explicit close order so a pending materialization
  error, successful cleanup, failed cleanup, and `Drop` all release the wrapper
  handle before `TempDir` removal can run on Windows.
- [ ] Replace wrapper pathname permission repair with a handle-only helper that
  preserves the existing Unix `0700` and Windows read-only rules.
- [ ] Add or extend tests for an inaccessible wrapper, explicit retry after a
  final-removal failure, and retained-handle permission identity.
- [ ] Run focused materialization, cleanup, and drop tests.

## Task 4: Drain ordinary worker contents through `WorkerRoot`

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`

- [ ] Add `WorkerRoot::clear_for_cleanup`. Make the root writable through its
  open handle, enumerate direct entries relative to that handle, and dispatch
  each entry to the existing no-follow removal state machine.
- [ ] Place the retained test checkpoint after cleanup metadata inspection and
  before the first open/remove/permission effect. The checkpoint is compiled
  only in tests.
- [ ] Move directory-handle permission repair before enumeration and child
  removal so a `0500` directory remains cleanable.
- [ ] For an unopenable Unix directory, add only the capability-confined repair
  path described by the design. Re-stat no-follow, verify type and device/inode
  identity, then open and continue. Do not fall back to ambient `chmod`.
- [ ] Preserve nonblocking handling of Unix special entries, Windows
  reparse-point handling, non-UTF-8 native paths, and the 128-level bound.
- [ ] Change ordinary `WorkerWorkspace::try_cleanup` to clear through
  `WorkerRoot`, repair the wrapper handle, close both capabilities, and invoke
  `remove_dir_all` only for final removal. Delete `make_tree_writable` and both
  ambient `make_cleanup_entry_accessible` variants.
- [ ] Make the optional wrapper handle the cleanup-phase marker. On retry after
  final removal fails, skip the already-consumed capability phase and retry the
  final owner removal without reopening ambient paths.
- [ ] Reorder `Drop`: attempt ordinary cleanup while capabilities are live,
  then close any remnants before owned fields drop; keep managed-child
  quiescence ordering intact.
- [ ] Add a Unix nested mode-`0000` directory/file test and rerun the existing
  read-only non-UTF-8, wrapper-access, depth-error, reset, and drop tests.
- [ ] Run the generated Rust oracle again and observe Green for every
  `strict` and `internal-fixture` row.
- [ ] Refactor only after the focused suite is green.
- [ ] Commit the implementation and tests.

## Task 5: Reconcile the formal and implementation claims

**Files:**

- Create:
  `docs/superpowers/reports/2026-09-04-issue-357-cleanup-capability-audit.md`
- Modify as needed: formal/Rust files above

- [ ] Write the claim and boundary, declared versus implicit behavior, the
  correspondence worksheet, theorem premises, bounded-search parameters,
  minimal broken witnesses, and sensitivity results.
- [ ] For each generated case, record mode, expected observation, actual
  observation, and `match`/`mismatch`/`infrastructure error`. Do not compare the
  `model-only` row as production correspondence.
- [ ] State explicitly that Lean proves only the abstract model. Tie the
  internal filesystem race to source locations and exact reproduction command.
- [ ] Record elapsed time and peak RSS from every retained Lean command, plus
  the temporary stats directory and its deletion.
- [ ] Resolve every counterexample-ledger item or leave it visible with an owner
  decision; do not weaken the model to match implementation.
- [ ] Rerun guarded Lean proofs, broken witnesses, corpus generation/freshness,
  and the Rust adapter in the required order.
- [ ] Commit the self-contained audit report and any correspondence fixes.

## Task 6: Verify the full change and create the PR

**Files:**

- Modify: `README.md` or `docs/development.md` only if the cleanup contract needs
  a user-facing note after implementation review
- Modify as needed: files above

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run focused cleanup/oracle tests, then `cargo test --workspace` using the
  shared target directory.
- [ ] Run the workspace Clippy command with warnings denied.
- [ ] Run the contracts-feature workspace tests and the repository Python suite
  that checks docs, workflows, and formal-oracle contracts.
- [ ] Run an MSRV `cargo check --workspace --all-targets --locked` if the new
  Rust syntax or APIs are not already exercised by an equivalent local gate.
- [ ] Run `git diff --check` and scan for ambient cleanup permission changes:
  `rg -n "make_tree_writable|make_cleanup_entry_accessible|set_permissions" crates/hoimin-cli/src/workspace`.
  Classify every remaining match; do not turn source spelling into a unit test.
- [ ] Perform three implementation self-review passes: capability/TOCTOU,
  lifecycle/retry/drop ordering, and test/formal/operational evidence. Record
  concrete findings and fixes in this plan.
- [ ] Confirm no worktree-local `target`, mutation output, Lean build leak, test
  wrapper, symlink, process, or guarded-command stats remain. Remove only
  artifacts created for this issue.
- [ ] Inspect the full branch diff and commits against `origin/main`.
- [ ] Push `fix/issue-357-cleanup-race` and create a PR to `main` referencing
  issue #357. State that automatic CI is Linux-only and no manual
  Windows/macOS workflow was dispatched.
- [ ] Monitor the Linux-only checks. Fix any failure on the same branch; do not
  dispatch or condition on the manual non-Linux workflow.

## Plan self-review record

### Round 1: task dependency and red/green evidence

The first review ordered the Lean source of truth before the Rust adapter and
placed the deterministic pause in the current vulnerable path so the test must
fail for the reported reason. It separated already-green stable/link cases from
the red inspection/swap case and requires the outside permission fingerprint,
not only a cleanup error, as the safety observation.

### Round 2: capability lifetime and failure paths

The second review enumerated construction failure before copying, failure while
copying, worker cleanup before final removal, final-removal failure, retry, and
`Drop`. It found that capturing a wrapper handle after the pending owner would
make Rust's reverse local-drop order safe, while storing it without an explicit
close would still conflict with Windows deletion. Tasks 3 and 4 now require
both declaration-order and explicit-close audits and use handle presence as the
retry phase marker.

### Round 3: filesystem compatibility and bounded resources

The third review checked read-only regular files, `0500` and `0000`
directories, links/reparse points, FIFOs, non-UTF-8 names, and the shared depth
limit. It added pre-enumeration directory repair and the inaccessible-directory
regression instead of assuming the existing remover covered them. It also caps
Lean at depth 4/341 traces per strategy, keeps commands serial, rejects default
mutation testing, and makes Linux-only automatic CI/non-Linux non-dispatch an
explicit delivery gate.

### Round 4: model correspondence and claim discipline

The fourth review split public, private-seam, and model-only cases before
implementation. It narrowed the wrapper-race observation to permission
confinement because an open handle cannot reveal an attacker's unknown rename
destination. The plan requires a separate audit report and forbids presenting
the Lean theorem, bounded search, or syscall behavior as interchangeable
evidence.

## Implementation self-review record

To be completed after implementation.
