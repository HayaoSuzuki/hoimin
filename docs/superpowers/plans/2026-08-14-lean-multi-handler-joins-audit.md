# Lean Multiple Handler Join Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove and executable-correspond the issue #300 phase-3 ordered multiple-handler join contract, fixing Rust only if a retained same-premise regression demonstrates a production defect.

**Architecture:** Add a small Lean fold over existing `BindingFlow.Exits` and `NestedTryFlow.cleanupExits`, with explicit selected exits and non-selected remainders. Generate seven closed JSONL rows, compare five complete production-backed try-exit snapshots internally, and compare two complete candidate observations through the public CLI.

**Tech Stack:** Lean 4, Lake, `Std`, Rust 2024, Ruff Python AST, Serde JSONL, Tokio integration tests, Cargo, and the repository Lean resource guard.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-multi-handler-joins` on branch `audit/lean-multi-handler-joins`.
- Keep phase 4 compound patterns and phase 5 `except*` out of this PR.
- Use exactly `strict`, `internal-fixture`, `model-only`, and `infrastructure-error` as corpus modes; the planned final corpus has five `internal-fixture`, two `strict`, and zero `model-only` rows unless correspondence discovery proves an exact premise unobservable.
- Lean proves the reduced model only; Rust correspondence remains a separate executable observation.
- Do not hand-edit `formal/HoiminOracle/corpus/multiple-handler-joins.jsonl`.
- Run every Lean/Lake command alone through `tools/lean_resource_guard.py` with `--timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250`; run Lake builds with `-Kjobs=1`.
- Use finite local `maxHeartbeats` on non-trivial Lean declarations and never use `maxHeartbeats 0`.
- Generated depth, event alphabet, explored states, and transitions remain zero.
- Treat parse, marker, schema, panic, timeout, RSS, and child-process failures as infrastructure errors.
- Change production transfer semantics only after a focused failing same-premise regression. Test-only observation and mutation seams are allowed.
- Add only listed files to commits. A temporary `.venv -> ../../.venv` setup symlink must be removed before status checks and commits.

---

## File Map

- Create `docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md`: correspondence worksheet first, then final self-contained result and resource ledger.
- Create `formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinModel.lean`: `HandlerStep`, `HandlerRoute`, `routeHandler`, `routeHandlers`, and `finishHandlers`.
- Create `formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinProofs.lean`: reachability, cleanup, category, meet, and remainder theorems.
- Create `formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinCases.lean`: seven fixed cases and six sensitivity families.
- Create `formal/HoiminOracle/MultipleHandlerJoinAuditMain.lean`: deterministic JSONL renderer, checker, case/sensitivity output, and statistics.
- Create `formal/HoiminOracle/corpus/multiple-handler-joins.jsonl`: generated expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import the cheap model, proof, and fixed-case modules.
- Modify `formal/HoiminOracle/lakefile.toml`: register only the non-imported generator executable.
- Create `crates/hoimin-cli/src/analyzer/multiple_handler_join_oracle_tests.rs`: closed-schema internal adapter and mutation sensitivity tests.
- Modify `crates/hoimin-cli/src/analyzer/rust.rs`: register the test module and, only where needed, add `cfg(test)` mutation seams around the real `visit_try` transfer.
- Create `crates/hoimin-cli/tests/lean_multiple_handler_join_oracle.rs`: two-row complete public CLI correspondence.

### Task 1: Freeze the correspondence worksheet

**Files:**
- Create: `docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md`

**Interfaces:**
- Consumes: issue #300, `AnnotationCollector::visit_try`, `binding_flow_try_exit_snapshot`, and the approved design.
- Produces: one worksheet row per planned corpus identity with an exact Lean premise, source marker, production configuration, observation function, and initial mode.

- [ ] **Step 1: Write the five internal worksheet identities**

Record these exact IDs and observation boundaries before creating Lean declarations:

```text
two_handlers_disagree_fallthrough       internal-fixture  try_exit
three_handlers_preserve_mapping         internal-fixture  try_exit
per_handler_cleanup_categories          internal-fixture  try_exit
different_handler_break_continue        internal-fixture  try_exit
unhandled_remainder_terminates           internal-fixture  try_exit
```

Each row names `binding_flow_try_exit_snapshot(source, marker)` and requires the complete normalized `fallthrough`, `breaks`, `continues`, and `terminates` record.

- [ ] **Step 2: Write the two strict worksheet identities**

Record these exact IDs:

```text
all_handlers_preserve_public_candidate  strict  public_candidate
one_handler_shadows_public_candidate    strict  public_candidate
```

Both use the same type-list operator premise and compare candidate count, path, byte span, operator, original, replacement, and symbol through the public `plan` path.

- [ ] **Step 3: Record exclusions and ownership decisions**

State that runtime exception matching, handler-type side effects, `except*`, nested match/loop/finally re-auditing, and generated exploration are excluded. State that exact unhandled-remainder identity is inferred only in a fixture whose try body has one `raise`, whose handlers have no terminate exits, and whose full snapshot contains exactly one terminate state.

- [ ] **Step 4: Verify and commit the worksheet**

Run:

```bash
rg -n "strict|internal-fixture|model-only|infrastructure-error|two_handlers|unhandled_remainder" docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md
git diff --check
git add docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md
git commit -m "docs: map multiple handler correspondence"
```

Require all seven IDs, allowed modes, exact observation functions, and no placeholder text.

### Task 2: Add the handler-fold model and proof surface

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-multiple-handler-proof-consumer.lean`

**Interfaces:**
- Consumes: `BindingFlow.{Env, Exits, Name, ExitCategory, meetAll?}` and `NestedTryFlow.cleanupExits`.
- Produces: `HandlerStep`, `HandlerRoute`, `routeHandler`, `routeHandlers`, `finishHandlers`, and the named theorems below.

- [ ] **Step 1: Write the RED proof consumer**

Create `/tmp/hoimin-multiple-handler-proof-consumer.lean` with:

```lean
import HoiminOracle.MultipleHandlerJoinProofs

open HoiminOracle.BindingFlow
open HoiminOracle.MultipleHandlerJoin

#check route_handlers_after_remainder_exhausted
#check selected_target_cleaned_in_every_category
#check selected_unrelated_fact_preserved
#check reachable_fallthrough_participates_in_meet
#check selected_abrupt_categories_preserved
#check unhandled_remainder_terminates_once
```

Run guarded `lake env lean /tmp/hoimin-multiple-handler-proof-consumer.lean` and require a nonzero Lean child exit because the module is absent. A timeout, RSS stop, or monitor error is not the required RED.

- [ ] **Step 2: Implement the minimal handler-fold types**

Start `MultipleHandlerJoinModel.lean` with these interfaces:

```lean
structure HandlerStep where
  selected : Option Exits
  remainder : Option Env
  target : Option Name := none
  deriving Repr, DecidableEq, BEq

structure HandlerRoute where
  exits : Exits := .empty
  remainder : Option Env := none
  deriving Repr, DecidableEq, BEq

def cleanSelected (step : HandlerStep) : Exits :=
  match step.selected, step.target with
  | none, _ => .empty
  | some exits, none => exits
  | some exits, some name => cleanupExits name exits

def routeHandler (current : HandlerRoute) (step : HandlerStep) : HandlerRoute :=
  match current.remainder with
  | none => current
  | some _ =>
      { exits := current.exits.merge (cleanSelected step)
        remainder := step.remainder }

def routeHandlers (incoming : Option Env) (steps : List HandlerStep) : HandlerRoute :=
  steps.foldl routeHandler { remainder := incoming }

def finishHandlers (route : HandlerRoute) : Exits :=
  match route.remainder with
  | none => route.exits
  | some environment => route.exits.merge (.categoryOnly .terminate environment)
```

- [ ] **Step 3: Prove the named obligations**

Prove the six consumer names plus helper lemmas for empty merges and cleanup membership. `route_handlers_after_remainder_exhausted` must quantify over every suffix list. `reachable_fallthrough_participates_in_meet` must use `NestedTryFlow.outgoingEnv` or `meetAll?` and retain explicit membership premises rather than a fixed case.

- [ ] **Step 4: Run GREEN and focused builds**

Run serially under the fixed guard:

```bash
lake -Kjobs=1 build HoiminOracle.MultipleHandlerJoinModel
lake -Kjobs=1 build HoiminOracle.MultipleHandlerJoinProofs
lake env lean /tmp/hoimin-multiple-handler-proof-consumer.lean
```

Require child exit 0 for each command and record elapsed time and peak RSS in the report draft.

- [ ] **Step 5: Commit the model and proofs**

```bash
git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinModel.lean formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinProofs.lean docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md
git commit -m "test(lean): model multiple handler joins"
```

### Task 3: Generate the fixed corpus and sensitivity evidence

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinCases.lean`
- Create: `formal/HoiminOracle/MultipleHandlerJoinAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/multiple-handler-joins.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 2 fold and proofs.
- Produces: `multipleHandlerJoinCases`, `fixedCasesPass`, `sensitivityPasses`, and executable `generate_multiple_handler_joins` with `--output`, `--check`, `--stats`, `--sensitivity`, and `--cases`.

- [ ] **Step 1: Register and run the RED generator contract**

Add:

```toml
[[lean_exe]]
name = "generate_multiple_handler_joins"
root = "MultipleHandlerJoinAuditMain"
```

Run guarded `lake -Kjobs=1 build generate_multiple_handler_joins` and require a missing-root/module failure.

- [ ] **Step 2: Encode the seven closed cases once in Lean**

Use a schema-1 `OracleCase` with these fields:

```lean
structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  observationKind : ObservationKind
  family : String
  source : String
  marker : String
  expectedExits : Exits := .empty
  candidate : CandidateExpectation := .notObserved
  candidateCount : Nat := 0
  candidatePath : Option String := none
  candidateStart : Option Nat := none
  candidateLength : Option Nat := none
  candidateOperator : Option String := none
  candidateOriginal : Option String := none
  candidateReplacement : Option String := none
  candidateSymbol : Option String := none
```

Define the seven sources and identities listed in Task 1. Use only `Sequence` and `Mapping`. Ensure each marker occurs exactly once, every public field group is closed, and every source is emitted directly from Lean.

- [ ] **Step 3: Encode six broken variants**

Add exact inequalities for:

```text
keep_first_selected_detected
keep_last_selected_detected
unreachable_selected_detected
omitted_cleanup_detected
flattened_category_detected
unhandled_remainder_detected
```

`sensitivityPasses` is their conjunction. Put `example : sensitivityPasses = true := by native_decide` only in `MultipleHandlerJoinAuditMain.lean`, never in an imported module.

- [ ] **Step 4: Generate and freshness-check the corpus**

Run each command alone under the guard:

```bash
lake -Kjobs=1 build HoiminOracle.MultipleHandlerJoinCases
lake env lean --run MultipleHandlerJoinAuditMain.lean -- --sensitivity
lake env lean --run MultipleHandlerJoinAuditMain.lean -- --cases
lake env lean --run MultipleHandlerJoinAuditMain.lean -- --stats
lake env lean --run MultipleHandlerJoinAuditMain.lean -- --output corpus/multiple-handler-joins.jsonl
lake env lean --run MultipleHandlerJoinAuditMain.lean -- --check corpus/multiple-handler-joins.jsonl
```

Require stats `fixed_cases=7`, `strict_cases=2`, `internal_fixture_cases=5`, `model_only_cases=0`, `sensitivity_families=6`, and all generated-search counters zero.

- [ ] **Step 5: Commit generated evidence**

```bash
git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/lakefile.toml formal/HoiminOracle/HoiminOracle/MultipleHandlerJoinCases.lean formal/HoiminOracle/MultipleHandlerJoinAuditMain.lean formal/HoiminOracle/corpus/multiple-handler-joins.jsonl docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md
git commit -m "test(lean): generate multiple handler corpus"
```

### Task 4: Add internal Rust correspondence in TDD order

**Files:**
- Create: `crates/hoimin-cli/src/analyzer/multiple_handler_join_oracle_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

**Interfaces:**
- Consumes: the generated corpus, existing `binding_flow_try_exit_snapshot`, and `BindingFlowTestSnapshot`.
- Produces: a closed corpus parser, optional `HOIMIN_MULTIPLE_HANDLER_CASE` filter, five internal comparisons, and deliberate mutation checks.

- [ ] **Step 1: Write the failing closed-schema tests**

Deserialize with `#[serde(deny_unknown_fields)]`. Reject unknown fields, duplicate IDs, unknown modes/kinds, changed source text, missing/duplicate markers, invalid public field groups, crossed IDs, and any internal row that does not request a full try-exit observation.

- [ ] **Step 2: Write the five failing correspondence tests**

For every internal row call:

```rust
let actual = binding_flow_try_exit_snapshot(&item.source, &item.marker)
    .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
assert_eq!(actual, expected_snapshot(item), "same-premise mismatch for {}", item.id);
```

Compare the full ordered vectors in all four categories. Run:

```bash
cargo test -p hoimin-cli --lib multiple_handler_join_oracle_tests --no-fail-fast
```

Require RED because the test module/corpus support is not yet registered; do not accept an infrastructure failure as semantic RED.

- [ ] **Step 3: Register the test module and reuse the production snapshot**

Add `#[cfg(test)] mod multiple_handler_join_oracle_tests;` beside existing oracle modules. Do not add a new observation function if `binding_flow_try_exit_snapshot` exposes all four complete categories for every row.

- [ ] **Step 4: Classify every initial difference**

For any mismatch rerun exactly one identity:

```bash
HOIMIN_MULTIPLE_HANDLER_CASE=two_handlers_disagree_fallthrough cargo test -p hoimin-cli --lib multiple_handler_join_oracle_tests -- --nocapture
```

Record expected/actual categories and classify the result in the report as confirmed bug, specification ambiguity, model defect, or infrastructure error before changing Rust transfer code.

- [ ] **Step 5: Add only required test mutations**

Reuse existing handler-cleanup mutations. Add narrowly scoped `cfg(test)` variants only when needed to show that internal fixtures detect keeping one handler, flattening handler abrupt exits, and dropping body terminate exits. Mutations must branch inside the real `visit_try`; the adapter must not implement a second correct transfer.

- [ ] **Step 6: Retain regression before a production fix**

If a confirmed mismatch exists, preserve the failing corpus row/test, then make the smallest production change in `visit_try`. If all five rows match, leave production semantics unchanged and state that only test registration/mutation seams changed.

- [ ] **Step 7: Run GREEN and commit**

```bash
cargo test -p hoimin-cli --lib multiple_handler_join_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --lib exception_match_binding --no-fail-fast
cargo test -p hoimin-cli --lib nested_try_oracle_tests --no-fail-fast
cargo fmt --all -- --check
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/multiple_handler_join_oracle_tests.rs docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md
git commit -m "test(rust): correspond multiple handler joins"
```

### Task 5: Add strict public correspondence

**Files:**
- Create: `crates/hoimin-cli/tests/lean_multiple_handler_join_oracle.rs`

**Interfaces:**
- Consumes: the two strict corpus rows and `hoimin_cli::run_with_io` or the established public-plan helper pattern.
- Produces: complete normalized candidate observations, including byte span.

- [ ] **Step 1: Write the public adapter and closed identity checks**

Copy only process/fixture/manifest infrastructure from `lean_nested_match_exit_oracle.rs`. Select candidates whose byte span overlaps the unique marker and normalize:

```rust
struct PublicObservation {
    count: usize,
    path: Option<String>,
    start: Option<u64>,
    length: Option<u64>,
    operator: Option<String>,
    original: Option<String>,
    replacement: Option<String>,
    symbol: Option<String>,
}
```

Zero or one candidate is valid according to the Lean row; two overlapping candidates are a same-premise mismatch.

- [ ] **Step 2: Run RED and resolve only same-premise failures**

```bash
cargo test -p hoimin-cli --test lean_multiple_handler_join_oracle --no-fail-fast
```

The first run must fail before the file/corpus is complete. Parse, filesystem, marker, manifest, timeout, nonzero exit, signal, or stderr-on-success failures remain infrastructure errors.

- [ ] **Step 3: Run GREEN with existing public audits**

```bash
cargo test -p hoimin-cli --test lean_multiple_handler_join_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_exception_match_binding_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_nested_try_flow_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_nested_match_exit_oracle --no-fail-fast
```

- [ ] **Step 4: Commit strict correspondence**

```bash
git add crates/hoimin-cli/tests/lean_multiple_handler_join_oracle.rs docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md
git commit -m "test(cli): audit multiple handler joins publicly"
```

### Task 6: Final report, verification, PR, CI, and merge

**Files:**
- Modify: `docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md`

**Interfaces:**
- Consumes: fresh guarded command results, Rust test output, mismatch ledger, and final diff.
- Produces: one self-contained report, merged PR, issue #300 phase update, and cleaned phase worktree.

- [ ] **Step 1: Run fresh Lean verification in required order**

Run serially with the fixed guard: focused proof build, proof consumer, cases, sensitivity, temporary corpus output, byte comparison, freshness check, and stats. Record each child exit, stop reason, elapsed milliseconds, and peak RSS KiB. Do not retry a resource stop with larger limits.

- [ ] **Step 2: Run fresh Rust and repository verification**

Create the temporary `.venv -> ../../.venv` symlink only if required, then run:

```bash
cargo test -p hoimin-cli --lib multiple_handler_join_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --test lean_multiple_handler_join_oracle --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -j 2 -- -D warnings
cargo test --workspace --all-features -j 2
git diff --check main...HEAD
```

Remove the symlink immediately after workspace tests and require a clean tracked status.

- [ ] **Step 3: Complete and self-review the report**

Record the exact claim, correspondence worksheet, seven-row mode counts, six sensitivity results, theorem premises, implementation verdicts, every mismatch classification, production semantic changes or explicit non-change, exclusions, commands, elapsed times, peak RSS, and generated-search zeros. State explicitly that Lean proves only the reduced model.

- [ ] **Step 4: Apply completion and review skills**

Invoke `superpowers:verification-before-completion` and `superpowers:requesting-code-review`. Because subagents are not authorized, perform the requesting-code-review fallback as a fresh local full-diff review. Repair findings, rerun affected checks, and commit the final report as:

```bash
git add docs/superpowers/reports/2026-08-14-lean-multi-handler-joins-audit.md
git commit -m "docs: report multiple handler join audit"
```

- [ ] **Step 5: Push and create the PR**

Push `audit/lean-multi-handler-joins`, create a PR referencing issue #300 without closing it, and include the claim, Lean/Rust distinction, corpus counts, sensitivity results, mismatch classification, exact verification summary, and whether production semantics changed.

- [ ] **Step 6: Require all CI, merge, and update issue #300**

Monitor every required and cross-platform check. Diagnose failures using `superpowers:systematic-debugging`, make fixes only in this worktree, rerun relevant local verification, and push. After all required checks pass, squash-merge, fetch and confirm the merge commit is on `origin/main`, then comment on issue #300 with phase-3 evidence and phases 4–5 remaining.

- [ ] **Step 7: Clean only this phase worktree**

Confirm no tracked or untracked phase files remain, remove `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-multi-handler-joins`, prune worktrees, and fast-forward the primary `main` without touching its pre-existing `.idea/`, `.serena/`, or `tests/fixtures/projects/basic/uv.lock` files.
