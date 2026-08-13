# Lean Nested Match Exit Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove and execute-correspond the phase-2 nested-match categorized-exit contract from issue #300 without changing production behavior unless a same-premise regression fails.

**Architecture:** Add a focused Lean layer over the existing `BindingFlow.Exits` and `NestedTryFlow` APIs for match branch joins and loop-boundary consumption. Generate six closed JSONL rows from Lean, compare three post-try snapshots and one loop-head snapshot through owned test-only projections, and compare two complete candidate observations through the public CLI.

**Tech Stack:** Lean 4, Lake, `Std`, Rust 2024, Ruff Python AST, Serde JSONL, Tokio integration tests, Cargo, and the repository Lean resource guard.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-nested-match-exits` on branch `audit/lean-nested-match-exits`.
- Keep phase 3 multiple-handler selection, phase 4 compound-pattern binding, and phase 5 `except*` out of this PR.
- Use exactly `strict`, `internal-fixture`, `model-only`, and `infrastructure-error` as corpus modes; this phase emits three internal try-exit rows, one internal loop-head row, and two strict rows.
- Lean proves the reduced model only; Rust correspondence is a separate executable observation.
- Do not hand-edit `formal/HoiminOracle/corpus/nested-match-exits.jsonl`.
- Run every Lean/Lake command alone through `tools/lean_resource_guard.py` with `--timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250` and run Lake builds with `-Kjobs=1`.
- Use bounded `maxHeartbeats` on non-trivial declarations and never use `maxHeartbeats 0`.
- Generated depth, event alphabet, explored states, and transitions remain zero.
- Treat parse, marker, schema, panic, timeout, RSS, and child-process failures as infrastructure errors.
- Change production transfer semantics only after a focused failing same-premise regression. Test-only observation and mutation seams are allowed.
- Add only listed files to commits; `.venv` remains an untracked worktree symlink.

---

## File Map

- Create `formal/HoiminOracle/HoiminOracle/NestedMatchExitModel.lean`: `composeMatch`, natural loop entry, and loop-boundary consumption.
- Create `formal/HoiminOracle/HoiminOracle/NestedMatchExitProofs.lean`: categorized preservation, reachability, cleanup composition, and loop-consumption theorems.
- Create `formal/HoiminOracle/HoiminOracle/NestedMatchExitCases.lean`: six fixed rows and five broken variants.
- Create `formal/HoiminOracle/NestedMatchExitAuditMain.lean`: schema-1 JSONL renderer, freshness checker, statistics, and sensitivity output.
- Create `formal/HoiminOracle/corpus/nested-match-exits.jsonl`: generated corpus.
- Modify `formal/HoiminOracle/HoiminOracle.lean` and `formal/HoiminOracle/lakefile.toml`: register cheap modules and the non-imported generator.
- Create `crates/hoimin-cli/src/analyzer/nested_match_exit_oracle_tests.rs`: closed internal adapter and sensitivity tests.
- Modify `crates/hoimin-cli/src/analyzer/rust.rs`: register the test module and add marker-qualified loop-head/optional-mutation projections; semantic changes only on confirmed mismatch.
- Create `crates/hoimin-cli/tests/lean_nested_match_exit_oracle.rs`: complete strict public correspondence.
- Create `docs/superpowers/reports/2026-08-13-lean-nested-match-exits-audit.md`: self-contained audit report and measurement ledger.

### Task 1: Lean model and theorem surface

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/NestedMatchExitModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/NestedMatchExitProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-nested-match-proof-consumer.lean`

**Interfaces:**
- Consumes: `BindingFlow.{Env, Exits, ExitCategory, meetAll?}` and `NestedTryFlow.{cleanupExits, composeTry, outgoingEnv}`.
- Produces: `composeMatch : List Exits -> Option Env -> Exits`, `loopNaturalEntry : Env -> Exits -> Env`, `consumeLoop : Env -> Exits -> (Env -> Exits) -> Exits`, plus the named theorems in step 3.

- [ ] **Step 1: Write and run the RED proof consumer**

Create the temporary consumer with:

```lean
import HoiminOracle.NestedMatchExitProofs

open HoiminOracle.BindingFlow
open HoiminOracle.NestedMatchExit

#check compose_match_preserves_breaks
#check compose_match_preserves_continues
#check compose_match_preserves_terminates
#check nested_handler_cleanup_precedes_finally
#check consume_loop_uses_continue_back_edges
#check consume_loop_propagates_only_terminates
```

Run from `formal/HoiminOracle` through the resource guard and require a nonzero Lean child exit because the module does not exist; a guard stop is not the expected RED.

- [ ] **Step 2: Implement the minimal compositional API**

Use these exact definitions as the initial implementation:

```lean
def composeMatch (branches : List Exits) (unmatched : Option Env) : Exits :=
  let unmatchedExit := unmatched.map Exits.fallthroughOnly |>.getD .empty
  branches.foldl Exits.merge unmatchedExit

def loopNaturalEntry (zeroIteration : Env) (body : Exits) : Env :=
  meetAll? ([zeroIteration] ++ body.fallthrough.toList ++ body.continues)
    |>.getD zeroIteration

def consumeLoop
    (zeroIteration : Env) (body : Exits) (orelse : Env -> Exits) : Exits :=
  let afterElse := orelse (loopNaturalEntry zeroIteration body)
  { fallthrough := meetAll? (body.breaks ++ afterElse.fallthrough.toList ++
        afterElse.breaks ++ afterElse.continues)
    terminates := body.terminates ++ afterElse.terminates }
```

- [ ] **Step 3: Prove the named obligations with bounded heartbeats**

Add kernel-checked theorems showing that `composeMatch` concatenates each abrupt category, excludes `none` unmatched input from fallthrough, that `composeTry .empty (composeMatch ...) (some name)` is cleanup-before-finally composition, and that `consumeLoop` includes continue inputs while returning empty outward break/continue lists. Use `set_option maxHeartbeats 100000 in`; no `sorry`, axioms, or `native_decide` in the proof module.

- [ ] **Step 4: Run the proof consumer GREEN and commit**

Run the same guarded consumer, then guarded focused builds for `HoiminOracle.NestedMatchExitModel` and `HoiminOracle.NestedMatchExitProofs`. Commit the model, proofs, import, design, and plan as `test(lean): model nested match exit composition`.

### Task 2: Fixed cases, broken variants, and generated corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/NestedMatchExitCases.lean`
- Create: `formal/HoiminOracle/NestedMatchExitAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/nested-match-exits.jsonl`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 1 model and theorem modules.
- Produces: `nestedMatchExitCases`, `sensitivityPasses`, `fixedCasesPass`, and executable `generate_nested_match_exits` supporting `--output`, `--check`, `--stats`, `--sensitivity`, and `--cases`.

- [ ] **Step 1: Add a RED generator contract**

Register `generate_nested_match_exits` in `lakefile.toml`, then run guarded `lake -Kjobs=1 build generate_nested_match_exits`. Require failure until the executable and case module exist.

- [ ] **Step 2: Encode the six closed worksheet rows**

Each case contains schema, ID, mode, observation kind, family, exact source, unique marker, expected exit/fact or public observation, and no unused public fields. Use `Sequence` and `Mapping` as the two representative facts. Keep return and raise as distinct source exits even though both normalize to `terminate`.

- [ ] **Step 3: Encode and detect five broken variants**

Add fixed witnesses for `brokenFlattenNestedCategory`, `brokenRetainUnreachable`, `brokenOmitAbrupt`, `brokenCleanupOutsideHandler`, and `brokenOmitContinueBackEdge`. Define `sensitivityPasses` as the conjunction of exact inequalities; prove `example : sensitivityPasses = true := by native_decide` only in the executable.

- [ ] **Step 4: Generate, validate, and commit the corpus**

Run guarded build, sensitivity, cases, stats, `--output`, and `--check` commands separately. Confirm stats report six fixed cases, two strict, four internal fixtures, zero model-only, five sensitivity families, and zero generated search metrics. Commit as `test(lean): generate nested match exit corpus`.

### Task 3: Internal production correspondence and mutation sensitivity

**Files:**
- Create: `crates/hoimin-cli/src/analyzer/nested_match_exit_oracle_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

**Interfaces:**
- Consumes: generated JSONL, `binding_flow_try_exit_snapshot`, production `AnnotationCollector`, and `BindingFlowTestSnapshot`.
- Produces: closed schema parser, selected-case filter `HOIMIN_NESTED_MATCH_EXIT_CASE`, marker-qualified loop-head projection, and optional mutation projections.

- [ ] **Step 1: Write failing corpus and correspondence tests**

Tests must reject unknown fields, duplicate IDs, wrong schema/mode/observation kind, changed sources, duplicate markers, and crossed identities. Internal tests compare every field of complete try-exit snapshots and the full normalized loop-head fact set.

- [ ] **Step 2: Run focused tests RED**

Run `cargo test -p hoimin-cli --lib nested_match_exit_oracle_tests --no-fail-fast`. Require compilation/test failure because the module and observation seam are absent.

- [ ] **Step 3: Add the smallest owned observation seams**

Reuse the existing try-exit projection. Extend loop-head projection to select exactly one loop by a unique source marker and return `Result<Vec<String>, String>` with `infrastructure-error:` failures. Add test-only mutation support that either flattens nested match abrupt exits to fallthrough or omits continue back edges; do not duplicate the correct transfer in the adapter.

- [ ] **Step 4: Classify correspondence before semantic edits**

Run all four internal rows. For any difference, rerun exactly one row with `HOIMIN_NESTED_MATCH_EXIT_CASE=<id>` and record expected/actual fields. Classify model defect, specification ambiguity, infrastructure error, or confirmed production bug before editing semantic code.

- [ ] **Step 5: Retain a regression before any required Rust fix**

If and only if production mismatches, keep the corpus row as a failing regression and apply the smallest change inside `visit_match`, `visit_try`, or `visit_loop`. Otherwise make no production semantic change. Require mutation tests to show the fixtures fail under both flattening and omitted-continue variants.

- [ ] **Step 6: Run focused tests GREEN and commit**

Run the focused module, relevant existing binding-flow tests, and `cargo fmt --check`. Commit as `test(rust): correspond nested match exit fixtures`.

### Task 4: Strict public correspondence

**Files:**
- Create: `crates/hoimin-cli/tests/lean_nested_match_exit_oracle.rs`

**Interfaces:**
- Consumes: the two strict corpus rows and `hoimin_cli::run_with_io`.
- Produces: complete normalized candidate observations with count, path, operator, original, replacement, and symbol.

- [ ] **Step 1: Write the strict adapter and run RED**

Copy only CLI setup/manifest parsing infrastructure from `lean_nested_try_flow_oracle.rs`. Parse closed corpus identities, write each exact source to `target.py`, run public `plan` with the exact type operator, and select candidates overlapping the unique marker. Compare the entire normalized record, not presence alone.

- [ ] **Step 2: Resolve only same-premise failures**

Run `cargo test -p hoimin-cli --test lean_nested_match_exit_oracle --no-fail-fast`. A parse, CLI, marker, manifest, timeout, or unexpected-exit failure is infrastructure. For a semantic mismatch, use the row filter and follow Task 3's regression-before-fix rule.

- [ ] **Step 3: Run GREEN and commit**

Require both strict rows and existing nested-try public correspondence to pass. Commit as `test(cli): audit nested match exits publicly`.

### Task 5: Report, full verification, PR, and merge

**Files:**
- Create: `docs/superpowers/reports/2026-08-13-lean-nested-match-exits-audit.md`

**Interfaces:**
- Consumes: fresh command output and resource-guard JSON stats.
- Produces: a self-contained report suitable for issue #300 and PR review.

- [ ] **Step 1: Run fresh verification in required order**

Run guarded Lean proof build, sensitivity, corpus freshness, and stats separately; focused internal and strict tests; `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`; and `git diff --check`. Record elapsed time and peak RSS from each retained Lean stats file.

- [ ] **Step 2: Write and verify the report**

Record theorem premises, six-row mode counts, five sensitivity results, complete correspondence verdicts, every mismatch classification, production files changed or explicitly unchanged, exclusions, exact commands, and resource measurements. State that Lean proves the model, not Rust.

- [ ] **Step 3: Self-review and create the PR**

Use `superpowers:verification-before-completion` and `superpowers:requesting-code-review` as a local self-review because subagents are not authorized. Inspect the full diff, rerun changed-area checks, push `audit/lean-nested-match-exits`, and create a PR referencing `#300` without auto-closing the issue.

- [ ] **Step 4: Monitor CI, merge, and update issue #300**

Wait for every required check to succeed. Fix failures in the same worktree with fresh verification and push updates. Squash-merge the PR, confirm the merge commit is on `origin/main`, comment on issue #300 with the phase-2 report/PR and remaining phases 3–5, then remove only this phase's worktree after confirming it is clean.
