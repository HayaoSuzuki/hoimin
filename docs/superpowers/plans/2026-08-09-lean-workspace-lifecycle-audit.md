# Lean workspace lifecycle audit implementation plan

> **Execution:** Follow this plan task by task in the dedicated audit worktree.
> Use test-first edits for every executable claim and preserve minimal broken
> witnesses before proving the correct transition.

**Goal:** Build a reproducible Lean audit of worker workspace ownership across
active handler state, owned blocking tasks, pending cleanup, and lifecycle
cleanup, then compare Lean-generated expectations with Rust under identical
premises.

**Architecture:** Extend the pinned `formal/HoiminOracle` project with an
independent finite model and executable explorer. Generate one versioned JSONL
corpus. A public integration adapter handles strict synchronous cases, while a
crate-local test adapter handles private task ownership boundaries. Production
behavior remains unchanged; a confirmed mismatch becomes a detailed report
and separate repair issue.

**Tech stack:** Lean 4, Lake, `Std`, `Lean.Data.Json`, Rust 2024, Cargo tests,
`tempfile`, existing `hoimin-core` request and reservation types.

## Global constraints

- Keep all design, plan, model, corpus, adapters, and report changes in this
  worktree and one PR.
- Do not change production semantics in this audit.
- Use exactly `strict`, `model-only`, `internal-fixture`, and
  `infrastructure-error` as correspondence modes.
- Model exactly two worker roles, two generation roles, and two task roles.
- Treat task/completion values as affine; do not model cloning a Rust value.
- Include a lifecycle epoch so stale completion rejection is an explicit
  contract rather than an accidental map collision.
- Default bounded search depth is at most eight and must be labeled bounded.
- Imported proof modules must not execute BFS or large `native_decide` checks.
- Lean owns expected observations; Rust adapters only translate and observe.
- Corpus parse/setup/panic failures are infrastructure errors.
- All three sensitivity families must produce a stable minimal witness.
- Do not alter existing Lean corpora or their adapter semantics.
- A confirmed mismatch remains reproducible and is not fixed in this branch.

## Task 1: Establish correspondence and finite state

**Files:**

- Create `formal/HoiminOracle/HoiminOracle/WorkspaceModel.lean`.
- Modify `formal/HoiminOracle/HoiminOracle.lean`.

- [ ] Define worker, generation, task, operation, outcome, location, handler
      epoch, event, rejection, verdict, and observation types.
- [ ] Represent active, task-owned/completed, and pending locations separately
      so uniqueness is a substantive invariant.
- [ ] Add stable renderers and deterministic event enumeration.
- [ ] Add an intentionally false example with one generation in two locations.
- [ ] Run `lake env lean HoiminOracle/WorkspaceModel.lean` and retain the RED
      result in the work log.
- [ ] Define the correct transition and remove the false example.
- [ ] Run the model file and `lake build` GREEN.
- [ ] Commit the finite model independently.

## Task 2: Refute broken lifecycle variants

**Files:**

- Modify `formal/HoiminOracle/HoiminOracle/WorkspaceModel.lean`.
- Create `formal/HoiminOracle/WorkspaceAuditMain.lean` initially as a witness
  runner.

- [ ] Implement broken atomicity, uniqueness, and stale-epoch transitions.
- [ ] Write fixed witness traces for each family.
- [ ] First assert one broken transition is safe and observe Lean RED.
- [ ] Replace the false assertion with checks that each witness violates its
      named property.
- [ ] Run the witness executable GREEN and record shortest traces.

## Task 3: Prove explicit-premise properties

**Files:**

- Create `formal/HoiminOracle/HoiminOracle/WorkspaceProofs.lean`.
- Modify `formal/HoiminOracle/HoiminOracle.lean`.

- [ ] Prove pairwise location disjointness from the invariant.
- [ ] Prove generation owner uniqueness.
- [ ] Prove exact ownership transfer on successful prepare.
- [ ] Prove prepare rejection preserves the entire state.
- [ ] Prove operation-specific completion postconditions.
- [ ] Prove rejected/stale completion preserves active and pending maps.
- [ ] Prove cleanup success empties handler ownership and advances the epoch.
- [ ] Prove cleanup failure retains retryable ownership.
- [ ] Prove transition-local invariant preservation and lift it over traces.
- [ ] Confirm no `sorry`, custom axiom, or explorer import exists in the proof
      module.

## Task 4: Add bounded exploration and deterministic cases

**Files:**

- Create `formal/HoiminOracle/HoiminOracle/WorkspaceCases.lean`.
- Complete `formal/HoiminOracle/WorkspaceAuditMain.lean`.
- Modify `formal/HoiminOracle/HoiminOracle.lean` and `lakefile.toml`.
- Generate `formal/HoiminOracle/corpus/workspace-lifecycle.jsonl`.

- [ ] Add named strict and internal-fixture schedules covering success,
      rejection, retry, reset-discard, duplicate slot, cleanup, and stale
      completion behavior.
- [ ] Add schema-versioned JSON encoding and stable ordering.
- [ ] Implement shortest-first BFS with state deduplication after outgoing
      transition checks.
- [ ] Add shrinking, reachable-state/transition statistics, and default depth
      eight.
- [ ] Make all sensitivity witnesses mandatory before corpus output.
- [ ] Register a dedicated Lake executable without importing it from the
      library root.
- [ ] Generate the corpus twice and verify byte-for-byte determinism.

## Task 5: Strict public Rust correspondence

**Files:**

- Create `crates/hoimin-cli/tests/lean_workspace_oracle.rs`.

- [ ] Add strict schema/mode/event/observation parsing with duplicate-ID
      rejection and optional single-case replay.
- [ ] Build isolated projects and valid preflight/reservation grants using
      public APIs.
- [ ] Replay synchronous create/apply/reset/cleanup and validation rejection
      schedules.
- [ ] Observe active count, pending count, file content/generation marker, and
      released reservation data without deriving expected semantics.
- [ ] First corrupt one expected observation and retain Rust RED evidence.
- [ ] Restore the Lean-generated corpus and run the adapter GREEN.

## Task 6: Internal task-boundary correspondence

**Files:**

- Create `crates/hoimin-cli/src/workspace/lean_oracle_tests.rs`.
- Modify `crates/hoimin-cli/src/workspace/mod.rs` only to include the
  `#[cfg(test)]` module.

- [ ] Replay only `internal-fixture` cases through prepare/execute/accept.
- [ ] Observe active, task-owned, pending, and physical generation identity.
- [ ] Cover apply success/failure, reset success/discard paths, pending create,
      duplicate active slot, and stale cleanup epoch.
- [ ] Ensure synthetic setup is named in failure output and never classified
      as strict.
- [ ] Run the internal adapter with randomized Rust test order.

## Task 7: Audit shell scheduling correspondence

**Files:**

- Modify existing shell tests only when a same-premise controlled hook already
  exists; otherwise document the row as unexercised.

- [ ] Trace core pending-effect rules that gate workspace operations.
- [ ] Verify whether cleanup can coexist with a workspace task completion.
- [ ] Exercise the real public shell path for any controllable schedule.
- [ ] Classify schedules prohibited by the core state machine as `model-only`,
      with source references and no mismatch claim.

## Task 8: Classify findings and write the report

**Files:**

- Create `docs/superpowers/reports/2026-08-09-lean-workspace-lifecycle-audit.md`.
- Conditionally create a counterexample ledger if any mismatch remains.

- [ ] Record owned claims, exclusions, correspondence table, commands, tool
      versions, depth, reduction rules, state/transition counts, corpus size,
      and elapsed costs.
- [ ] Record each broken-family witness and the property that detects it.
- [ ] Classify every Rust comparison as match, confirmed bug, ambiguity, model
      defect, or infrastructure error.
- [ ] For a confirmed bug, include the minimal schedule, implementation call
      path, expected/actual state, severity, likely repair boundary, and
      regression-test recipe.
- [ ] Open a separate GitHub issue for each confirmed production bug and link
      it from the report; do not implement the fix here.

## Task 9: Full verification and PR integration

- [ ] Run pinned Lean build, proofs, explorer, sensitivity, and corpus freshness.
- [ ] Run strict and internal correspondence adapters and focused existing
      workspace/shell tests.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace` and the skill tests.
- [ ] Run `git diff --check` and confirm only audit-scope files changed.
- [ ] Self-review claim/premise alignment and generated-file freshness.
- [ ] Push, create one PR containing docs and implementation, wait for required
      CI, squash merge, confirm closure/state, and remove the worktree and local
      branch.
