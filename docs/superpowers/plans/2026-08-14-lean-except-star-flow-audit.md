# Lean `except*` Binding-Flow Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove and executable-correspond the issue #300 phase-5 `except*` must-known summary, fixing Rust only if a retained same-premise collapsed-summary regression demonstrates a production defect.

**Architecture:** Lean models ordered subgroup routes and collapses them into Hoimin's two-name must-known lattice. Exact runtime split rows remain model-only because Rust does not retain exception-group identities. Internal and public adapters compare only the conservative summary that production can configure and observe.

**Tech Stack:** Lean 4.32.2, Rust 1.88+, Ruff Python AST, Serde JSONL, Cargo, GitHub CLI.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-except-star-flow` on `audit/lean-except-star-flow`.
- Keep phases 1 through 4, runtime exception matching, exception values, traceback shape, and parser correctness out of this PR.
- Use only `strict`, `internal-fixture`, `model-only`, and `infrastructure-error` corpus modes.
- Run Lean commands serially with a 20-second deadline, 786,432 KiB RSS cap, 250 ms sampling, and `lake -Kjobs=1` for builds.
- Keep expensive evaluation in `ExceptStarFlowAuditMain.lean`.
- Do not change Rust production semantics without a focused same-premise RED regression.
- Do not treat exact sibling or remainder identity as production correspondence.
- Preserve the primary checkout's user-owned untracked files.

---

### Task 1: Establish the report and correspondence ledger

**Files:**
- Create: `docs/superpowers/reports/2026-08-14-lean-except-star-flow-audit.md`
- Reference: `docs/superpowers/specs/2026-08-14-lean-except-star-flow-design.md`

**Interfaces:**
- Consumes: issue #300, Python's `except*` contract, Ruff `StmtTry::is_star`, `AnnotationCollector::visit_try`, and the approved design.
- Produces: one self-contained evidence ledger updated after each task.

- [ ] **Step 1: Write the report boundary**

State the claim, included fallthrough and terminate behavior, forbidden abrupt statements, and the distinction between exact subgroup routes and the production-observable collapsed summary. Copy the correspondence worksheet without changing row modes.

- [ ] **Step 2: Record implementation facts**

Record `StmtTry::is_star`, the shared current `visit_try` transfer, the complete internal try-exit projection, the public candidate observation, and the absence of exception-group identity in production.

- [ ] **Step 3: Add the classification ledger**

Use columns `case`, `mode`, `expected`, `actual`, `classification`, and `decision`. Set unexecuted result cells to `not run`; do not predict a verdict.

- [ ] **Step 4: Check and commit**

Run `git diff --check`, scan for placeholders and Lean-to-Rust proof claims, then:

```bash
git add docs/superpowers/reports/2026-08-14-lean-except-star-flow-audit.md
git commit -m "docs: map except-star correspondence"
```

### Task 2: Add the route model, collapse, and proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ExceptStarFlowModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/ExceptStarFlowProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-except-star-proof-consumer.lean`

**Interfaces:**
- Consumes: `BindingFlow.{Name, Fact, Env, meetAll?}` and `NestedTryFlow.{Exits, cleanupExits}`.
- Produces: `FactAction`, `Split`, `StarHandler`, `StarRoute`, `routeStarHandler`, `routeStarHandlers`, `finishStarRoute`, `exactSummary`, and `conservativeSummary`.

- [ ] **Step 1: Write the RED proof consumer**

Create a consumer that imports `ExceptStarFlowProofs` and checks:

```lean
#check later_sibling_runs_after_raised_handler
#check handler_runs_at_most_once
#check only_remainder_advances
#check target_cleanup_reaches_fallthrough
#check target_cleanup_reaches_delayed_terminate
#check final_remainder_terminates
#check retained_fact_occurs_in_every_route
#check conservative_summary_matches_exact_subset_meet
```

Run it under the resource guard. Require a missing-module Lean child failure; resource or monitor failure does not satisfy RED.

- [ ] **Step 2: Implement the route algebra**

Define factwise `keep`, `invalidate`, and `restore Fact` actions. Define a split with `matched` and `remainder` booleans. Define a route with `Env`, active-remainder flag, pending-raised flag, and visit count.

`routeStarHandler` leaves inactive routes unchanged. For an active route, it skips the body when unmatched. A match applies each fact action once, applies target cleanup, records a pending raise, copies the remainder flag, and increments the visit count. `finishStarRoute` terminates when a remainder or raised exception remains. Summary functions meet routes by exit category.

- [ ] **Step 3: Prove eight named obligations**

Keep split premises explicit. Prove the first seven statements over arbitrary environments. Prove the collapse theorem for two handlers, two names, and the finite action domain. Compare summaries rather than exception identity. Use a bounded per-declaration heartbeat only if needed.

- [ ] **Step 4: Run focused GREEN**

```bash
lake -Kjobs=1 build HoiminOracle.ExceptStarFlowModel
lake -Kjobs=1 build HoiminOracle.ExceptStarFlowProofs
lake env lean /tmp/hoimin-except-star-proof-consumer.lean
```

Run each command alone through `tools/lean_resource_guard.py`. Record elapsed milliseconds and peak RSS KiB.

- [ ] **Step 5: Commit model and proofs**

```bash
git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle/ExceptStarFlowModel.lean formal/HoiminOracle/HoiminOracle/ExceptStarFlowProofs.lean docs/superpowers/reports/2026-08-14-lean-except-star-flow-audit.md
git commit -m "test(lean): model except-star flow"
```

### Task 3: Generate the corpus and sensitivity evidence

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ExceptStarFlowCases.lean`
- Create: `formal/HoiminOracle/ExceptStarFlowAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/except-star-flow.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 2 route and summary functions.
- Produces: `exceptStarFlowCases`, `fixedCasesPass`, `sensitivityPasses`, and `generate_except_star_flow` with `--output`, `--check`, `--stats`, `--sensitivity`, and `--cases`.

- [ ] **Step 1: Register and observe RED**

Register a `generate_except_star_flow` executable rooted at `ExceptStarFlowAuditMain`. Require its first guarded build to fail because the root is absent.

- [ ] **Step 2: Define eight Lean-owned rows**

```text
starred_summary_preserves_common       internal-fixture try_exit
starred_summary_meets_disagreement     internal-fixture try_exit
starred_target_cleanup                 internal-fixture try_exit
starred_unhandled_remainder            internal-fixture try_exit
two_matching_siblings_exact_route      model-only       model_witness
raised_handler_allows_later_sibling    model-only       model_witness
starred_public_candidate_present       strict           public_candidate
starred_public_candidate_absent        strict           public_candidate
```

Internal rows use literal `except*` and unique markers. Model-only rows have empty source and marker. Strict rows own the exact `target.py` candidate fields for `type_list_sequence`.

- [ ] **Step 3: Encode seven broken variants**

Detect stop-after-first-match, sibling loss after raise, lost remainder, duplicated remainder, eager raise propagation, omitted target cleanup, and exclusive-handler collapse. Record idempotency as inapplicable because production stores no durable event identity.

- [ ] **Step 4: Build and freshness-check**

```bash
lake -Kjobs=1 build HoiminOracle.ExceptStarFlowCases
lake env lean --run ExceptStarFlowAuditMain.lean -- --cases
lake env lean --run ExceptStarFlowAuditMain.lean -- --sensitivity
lake env lean --run ExceptStarFlowAuditMain.lean -- --stats
lake env lean --run ExceptStarFlowAuditMain.lean -- --output corpus/except-star-flow.jsonl
lake env lean --run ExceptStarFlowAuditMain.lean -- --check corpus/except-star-flow.jsonl
```

Require eight rows, four internal, two strict, two model-only, seven sensitivity families, and zero generated-search counters.

- [ ] **Step 5: Commit generated evidence**

```bash
git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/lakefile.toml formal/HoiminOracle/HoiminOracle/ExceptStarFlowCases.lean formal/HoiminOracle/ExceptStarFlowAuditMain.lean formal/HoiminOracle/corpus/except-star-flow.jsonl docs/superpowers/reports/2026-08-14-lean-except-star-flow-audit.md
git commit -m "test(lean): generate except-star corpus"
```

### Task 4: Add internal Rust correspondence and classify RED

**Files:**
- Create: `crates/hoimin-cli/src/analyzer/except_star_flow_oracle_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

**Interfaces:**
- Consumes: Lean corpus and `binding_flow_try_exit_snapshot`.
- Produces: closed validation, four comparisons, `HOIMIN_EXCEPT_STAR_CASE`, and production-local sensitivity mutations.

- [ ] **Step 1: Add closed validation**

Use `#[serde(deny_unknown_fields)]`, compare exact JSON fields and identities, and reject duplicate IDs, unknown enums, changed sources, non-unique markers, crossed families, incomplete public fields, and production observations on model-only rows.

- [ ] **Step 2: Add four internal comparisons**

Call the real production projection and compare the complete normalized exit snapshot. Run:

```bash
cargo test -p hoimin-cli --lib except_star_flow_oracle_tests --no-fail-fast
```

Semantic differences count as RED. Parser, marker, panic, and setup failures count as infrastructure errors.

- [ ] **Step 3: Classify mismatches**

Rerun each difference with `HOIMIN_EXCEPT_STAR_CASE=<id>`. Record expected and actual states, source, observation site, and one allowed classification before editing production.

- [ ] **Step 4: Add real-transfer sensitivity seams**

Reuse target-cleanup mutations. Add starred-only variants only for collapsed-summary rules that production represents. Do not invent runtime subgroup identity in a mutation.

- [ ] **Step 5: Apply a conditional minimal repair**

If a collapsed-summary row confirms a bug, retain it and branch on `statement.is_star` inside `visit_try`. Extract only shared handler-body code required to avoid duplication. If all rows match, add only test registration and the smallest sensitivity seam.

- [ ] **Step 6: Run GREEN and adjacent tests**

```bash
cargo test -p hoimin-cli --lib except_star_flow_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --lib multiple_handler_join_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --lib nested_try_oracle_tests --no-fail-fast
cargo fmt --all -- --check
```

- [ ] **Step 7: Commit**

Use `fix(analyzer): preserve except-star sibling flow` after a confirmed production change. Otherwise use `test(rust): correspond except-star flow`.

### Task 5: Add strict public correspondence

**Files:**
- Create: `crates/hoimin-cli/tests/lean_except_star_flow_oracle.rs`

**Interfaces:**
- Consumes: two strict rows and `hoimin_cli::run_with_io`.
- Produces: full public candidate observations.

- [ ] **Step 1: Implement the adapter**

Validate the same closed identities. Create one isolated project per row, select only `type_list_sequence`, and normalize the unique overlapping candidate into count, path, start, length, operator, original, replacement, and symbol.

- [ ] **Step 2: Run RED or establish correspondence**

Run `cargo test -p hoimin-cli --test lean_except_star_flow_oracle --no-fail-fast`. Candidate differences are semantic. Launch, timeout, exit, stderr, manifest, and span failures are infrastructure errors.

- [ ] **Step 3: Run GREEN with adjacent audits**

```bash
cargo test -p hoimin-cli --test lean_except_star_flow_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_multiple_handler_join_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_nested_try_flow_oracle --no-fail-fast
```

- [ ] **Step 4: Commit**

```bash
git add crates/hoimin-cli/tests/lean_except_star_flow_oracle.rs docs/superpowers/reports/2026-08-14-lean-except-star-flow-audit.md
git commit -m "test(cli): audit except-star flow"
```

### Task 6: Verify, review, merge, close issue, and clean up

**Files:**
- Modify: `docs/superpowers/reports/2026-08-14-lean-except-star-flow-audit.md`

**Interfaces:**
- Consumes: final Lean and Rust output, ledger, diff, and CI.
- Produces: final report, merged PR, issue #300 decision, and phase-worktree cleanup.

- [ ] **Step 1: Run fresh Lean verification**

Run focused builds, proof consumer, cases, sensitivity, stats, temporary output, byte comparison, and freshness check under the fixed guard. Record exit, reason, time, and peak RSS.

- [ ] **Step 2: Run fresh repository verification**

```bash
cargo test -p hoimin-cli --lib except_star_flow_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --test lean_except_star_flow_oracle --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -j 2 -- -D warnings
cargo test --workspace --all-features -j 2
git diff --check origin/main...HEAD
```

Create `.venv -> ../../.venv` only if an existing test requires it, then remove it.

- [ ] **Step 3: Complete the report**

Record theorem premises, exact-versus-collapsed boundary, mode counts, sensitivity results, classifications, Rust change or non-change, exclusions, commands, resource measurements, search zeros, and freshness. State that Lean proves the reduced model.

- [ ] **Step 4: Review and commit**

Invoke `superpowers:verification-before-completion` and `superpowers:requesting-code-review`. Governing instructions prohibit subagent dispatch, so review `origin/main...HEAD` locally, map each requirement to evidence, repair findings, and rerun affected checks.

- [ ] **Step 5: Create and merge the PR**

Push `audit/lean-except-star-flow`, create a PR referencing #300, and report the formal boundary, counts, correspondence, and production decision. Wait for all CI. Diagnose failures with `superpowers:systematic-debugging`. Squash-merge after CI succeeds.

- [ ] **Step 6: Update issue and clean this phase**

Confirm the merge commit on `origin/main`. Comment with phase-5 evidence. Close issue #300 only if all five phases and parent acceptance criteria pass. Fast-forward primary `main`, remove this worktree, prune, and delete only this phase branch locally and remotely.
