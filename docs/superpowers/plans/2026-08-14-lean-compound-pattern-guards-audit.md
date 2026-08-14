# Lean Compound Pattern and Guard Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove and executable-correspond the issue #300 phase-4 compound-pattern and guard-failure contract, then make the smallest Rust repair only for a retained same-premise mismatch.

**Architecture:** Add a structural Lean pattern-attempt algebra over the existing two-name `BindingFlow.Env`. Lean computes eleven fixed observations and eight sensitivity witnesses into a closed JSONL corpus. Rust compares seven exact-site internal observations and three complete public CLI candidate records with those expectations; the unequal-arm OR witness remains model-only because Python rejects that source.

**Tech Stack:** Lean 4, Lake, `Std`, Rust 2024, Ruff Python AST, Serde JSONL, Tokio integration tests, Cargo, and the repository Lean resource guard.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-compound-pattern-guards` on branch `audit/lean-compound-pattern-guards`.
- Keep phase 5 `except*` and all phase-0 through phase-3 re-audits out of this PR.
- Use exactly `strict`, `internal-fixture`, `model-only`, and `infrastructure-error` as corpus modes. The planned corpus has three `strict`, seven `internal-fixture`, and one `model-only` row unless correspondence discovery proves a premise unobservable.
- Lean proves the reduced model only. Rust correspondence remains a separate executable observation.
- Do not hand-edit `formal/HoiminOracle/corpus/compound-pattern-guards.jsonl`.
- Run every Lean or Lake command alone through `tools/lean_resource_guard.py` with `--timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250`. Run Lake builds with `-Kjobs=1`.
- Use finite local `maxHeartbeats` on non-trivial Lean declarations. Do not use `maxHeartbeats 0`, `sorry`, `admit`, or custom axioms.
- Keep generated depth, event alphabet, explored states, and transitions at zero.
- Treat parse, marker, schema, panic, timeout, RSS, malformed output, and child-process failures as infrastructure errors.
- Change production match transfer only after a focused same-premise Rust test fails for its retained Lean row.
- Add only listed files to commits. Remove any temporary `.venv -> ../../.venv` symlink before status checks and commits.

---

## File Map

- Create `docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md`: pre-model worksheet, mismatch ledger, proof results, correspondence results, resource ledger, and exclusions.
- Create `formal/HoiminOracle/HoiminOracle/CompoundPatternGuardModel.lean`: `Attempt`, primitive tests, capture, sequence, AS, mapping, class, OR, and guard composition.
- Create `formal/HoiminOracle/HoiminOracle/CompoundPatternGuardProofs.lean`: ten proof obligations with explicit reachability premises.
- Create `formal/HoiminOracle/HoiminOracle/CompoundPatternGuardCases.lean`: eleven fixed cases, closed validation, and eight sensitivity families.
- Create `formal/HoiminOracle/CompoundPatternGuardAuditMain.lean`: deterministic JSONL rendering, generation, freshness, cases, sensitivity, and statistics.
- Create `formal/HoiminOracle/corpus/compound-pattern-guards.jsonl`: Lean-generated expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import the cheap model, proof, and fixed-case modules.
- Modify `formal/HoiminOracle/lakefile.toml`: register `generate_compound_pattern_guards` without importing the executable.
- Create `crates/hoimin-cli/src/analyzer/compound_pattern_guard_oracle_tests.rs`: closed corpus parsing, exact internal comparison, single-case filtering, and mutation checks.
- Modify `crates/hoimin-cli/src/analyzer/rust.rs`: register the internal test module, add test-only pattern mutations, and apply a production repair only after RED correspondence.
- Create `crates/hoimin-cli/tests/lean_compound_pattern_guard_oracle.rs`: three complete public CLI comparisons.

### Task 1: Freeze the correspondence worksheet

**Files:**
- Create: `docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md`

**Interfaces:**
- Consumes: issue #300, `AnnotationCollector::visit_match`, `invalidate_pattern_bindings`, `binding_flow_marker_snapshot`, the public `plan` path, and the approved design.
- Produces: one worksheet row per closed corpus identity with the exact source, marker, modeled route, observation, and initial mode.

- [ ] **Step 1: Record the seven internal identities**

Use these closed keys:

```text
or_success_meets_arms                 internal-fixture  case-entry
or_failure_meets_arms                 internal-fixture  next-case-entry
as_child_failure_precedes_alias       internal-fixture  next-case-entry
mapping_child_failure_precedes_rest   internal-fixture  next-case-entry
class_early_failure_precedes_capture  internal-fixture  next-case-entry
false_guard_uses_post_guard           internal-fixture  next-case-entry
compound_preserves_mapping            internal-fixture  next-case-entry
```

Each row calls `binding_flow_marker_snapshot(source, marker)` and compares the complete sorted known-import fact vector. The adapter reads the real `visit_match` state and does not reproduce pattern composition.

- [ ] **Step 2: Record the three strict and one model-only identities**

```text
as_failure_public_candidate       strict      public-candidate
mapping_failure_public_candidate  strict      public-candidate
class_failure_public_candidate    strict      public-candidate
unequal_or_capture_sets           model-only  model-witness
```

Each strict row runs only `type_list_sequence` and compares count, path, byte start, byte length, operator, original, replacement, and symbol. The model-only row has no production source because Python rejects OR arms that bind different names.

- [ ] **Step 3: Fix the exact production sources**

Record these source premises in the report and require Lean to emit the same bytes:

```python
from typing import Mapping, Sequence
match value:
    case [0, Sequence] | {"item": Sequence}:
        or_body_marker: list[str]
    case _:
        or_failure_marker: list[str]
```

```python
from typing import Sequence
match value:
    case [0] as Sequence:
        pass
    case _:
        as_failure_marker: list[int]
```

```python
from typing import Sequence
match value:
    case {"tag": 0, **Sequence}:
        pass
    case _:
        mapping_failure_marker: list[int]
```

```python
from typing import Sequence
match value:
    case Point(0, tail=Sequence):
        pass
    case _:
        class_failure_marker: list[int]
```

Use this false-guard source:

```python
from typing import Mapping, Sequence
match value:
    case [Mapping] if ((Sequence := local_sequence) and False):
        pass
    case _:
        false_guard_marker: list[str]
```

Use this unrelated-name source:

```python
from typing import Mapping, Sequence
match value:
    case [0] as Sequence:
        pass
    case _:
        preserved_mapping_marker: tuple[Mapping]
```

- [ ] **Step 4: Verify and commit the worksheet**

Run:

```bash
rg -n "or_success_meets_arms|as_child_failure|mapping_child_failure|class_early_failure|false_guard|unequal_or_capture_sets|strict|internal-fixture|model-only|infrastructure-error" docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md
git diff --check
git add docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md
git commit -m "docs: map compound pattern correspondence"
```

Require all eleven IDs, the four allowed modes, exact observation functions, source literals, exclusions, and no placeholder text.

### Task 2: Add the structural attempt model and proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/CompoundPatternGuardModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/CompoundPatternGuardProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-compound-pattern-proof-consumer.lean`

**Interfaces:**
- Consumes: `BindingFlow.{Name, Fact, Env, meetAll?, meetOption}`.
- Produces: `Attempt`, `pureTest`, `capture`, `thenAttempt`, `asPattern`, `mappingPattern`, `classPattern`, `orPattern`, `Attempt.summary`, `GuardResult`, and `applyGuard`.

- [ ] **Step 1: Write the RED proof consumer**

Create `/tmp/hoimin-compound-pattern-proof-consumer.lean` with:

```lean
import HoiminOracle.CompoundPatternGuardProofs

open HoiminOracle.BindingFlow
open HoiminOracle.CompoundPatternGuard

#check sequence_retains_prefix_failure
#check as_failure_precedes_alias
#check mapping_rest_requires_child_success
#check class_capture_requires_prefix_success
#check or_success_uses_every_reachable_arm
#check or_failure_uses_every_reachable_arm
#check or_retained_fact_occurs_in_each_success
#check false_guard_uses_post_guard_environment
#check capture_preserves_other_name
#check unreachable_outcome_does_not_join
```

Run guarded `lake env lean /tmp/hoimin-compound-pattern-proof-consumer.lean`. Require a nonzero Lean child exit because the module is absent. A timeout, RSS stop, or monitor failure does not satisfy RED.

- [ ] **Step 2: Implement the minimal attempt algebra**

Use this public surface in `CompoundPatternGuardModel.lean`:

```lean
structure Attempt where
  matched : List Env := []
  failed : List Env := []
  deriving Repr, DecidableEq, BEq

structure AttemptSummary where
  matched : Option Env
  failed : Option Env
  deriving Repr, DecidableEq, BEq

def Attempt.summary (attempt : Attempt) : AttemptSummary where
  matched := meetAll? attempt.matched
  failed := meetAll? attempt.failed

def pureTest (incoming : Env) (canSucceed canFail : Bool) : Attempt :=
  { matched := if canSucceed then [incoming] else []
    failed := if canFail then [incoming] else [] }

def capture (name : Name) (incoming : Env) : Attempt :=
  { matched := [incoming.set name .shadowed] }

def thenAttempt (first : Attempt) (next : Env -> Attempt) : Attempt :=
  { matched := first.matched.flatMap fun environment => (next environment).matched
    failed := first.failed ++ first.matched.flatMap fun environment => (next environment).failed }

def asPattern (child : Attempt) (name : Name) : Attempt :=
  thenAttempt child (capture name)

def mappingPattern (structural : Attempt) (children : List (Env -> Attempt))
    (rest : Option Name) : Attempt :=
  let completed := children.foldl (fun current child => thenAttempt current child) structural
  match rest with
  | none => completed
  | some name => thenAttempt completed (capture name)

def classPattern (structural : Attempt) (children : List (Env -> Attempt)) : Attempt :=
  children.foldl (fun current child => thenAttempt current child) structural

def orPattern (arms : List Attempt) : Attempt :=
  { matched := arms.flatMap Attempt.matched
    failed := arms.flatMap Attempt.failed }

structure GuardResult where
  body : Option Env
  nextCase : Option Env
  deriving Repr, DecidableEq, BEq

def applyGuard (attempt : Attempt) (guard : Option (Env -> Env × Bool)) : GuardResult :=
  let summary := attempt.summary
  match summary.matched, guard with
  | none, _ => { body := none, nextCase := summary.failed }
  | some matched, none => { body := some matched, nextCase := summary.failed }
  | some matched, some evaluate =>
      let result := evaluate matched
      if result.2 then { body := some result.1, nextCase := summary.failed }
      else { body := none, nextCase := meetOption summary.failed (some result.1) }
```

- [ ] **Step 3: Prove the ten named obligations**

Prove each consumer name over arbitrary environments and explicit list-membership or non-empty premises. The AS, mapping, and class theorems must show a prefix failure occurs unchanged in `failed`. The OR theorems must connect `meetAll?` with all reachable arm outcomes. `capture_preserves_other_name` assumes distinct names. `unreachable_outcome_does_not_join` proves an empty success or failure list contributes no fact.

Put `set_option maxHeartbeats 100000 in` around a declaration only when the unbounded default does not close promptly. Keep all premises visible in theorem statements.

- [ ] **Step 4: Run GREEN and focused builds**

Run each command alone under the resource guard:

```bash
lake -Kjobs=1 build HoiminOracle.CompoundPatternGuardModel
lake -Kjobs=1 build HoiminOracle.CompoundPatternGuardProofs
lake env lean /tmp/hoimin-compound-pattern-proof-consumer.lean
```

Require child exit 0 and record elapsed milliseconds plus peak root-and-descendant RSS KiB in the report.

- [ ] **Step 5: Commit the model and proofs**

```bash
git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle/CompoundPatternGuardModel.lean formal/HoiminOracle/HoiminOracle/CompoundPatternGuardProofs.lean docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md
git commit -m "test(lean): model compound pattern guards"
```

### Task 3: Generate the fixed corpus and sensitivity evidence

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/CompoundPatternGuardCases.lean`
- Create: `formal/HoiminOracle/CompoundPatternGuardAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/compound-pattern-guards.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 2 attempt algebra and proofs.
- Produces: `compoundPatternGuardCases`, `fixedCasesPass`, `sensitivityPasses`, and executable `generate_compound_pattern_guards` with `--output`, `--check`, `--stats`, `--sensitivity`, and `--cases`.

- [ ] **Step 1: Register and run the RED generator contract**

Add to `lakefile.toml`:

```toml
[[lean_exe]]
name = "generate_compound_pattern_guards"
root = "CompoundPatternGuardAuditMain"
```

Run guarded `lake -Kjobs=1 build generate_compound_pattern_guards` and require a missing-root or missing-module child failure.

- [ ] **Step 2: Encode all eleven cases once in Lean**

Use this closed schema:

```lean
inductive ObservationKind
  | caseEntry
  | nextCaseEntry
  | publicCandidate
  | modelWitness
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  observationKind : ObservationKind
  family : String
  source : String
  marker : String
  expectedFacts : List String := []
  candidateCount : Nat := 0
  candidatePath : Option String := none
  candidateStart : Option Nat := none
  candidateLength : Option Nat := none
  candidateOperator : Option String := none
  candidateOriginal : Option String := none
  candidateReplacement : Option String := none
  candidateSymbol : Option String := none
```

Define the exact IDs and sources from Task 1. Emit strict candidate spans from the Lean-owned source strings. Use `type_list_sequence`, original `list[int]`, replacement `Sequence[int]`, and `target.py`. Keep the model-only row's source and marker empty and give it only the unequal-success-meet facts.

- [ ] **Step 3: Encode eight broken variants**

Add fixed inequalities for:

```text
pre_pattern_failure_detected
late_capture_on_failure_detected
or_keep_first_success_detected
or_keep_last_success_detected
or_drop_failure_detected
pre_guard_failure_detected
unreachable_outcome_detected
overbroad_cleanup_detected
```

`sensitivityPasses` is their conjunction. Place `example : sensitivityPasses = true := by native_decide` only in `CompoundPatternGuardAuditMain.lean`, not an imported module.

- [ ] **Step 4: Generate and freshness-check the corpus**

Run serially under the fixed resource guard:

```bash
lake -Kjobs=1 build HoiminOracle.CompoundPatternGuardCases
lake env lean --run CompoundPatternGuardAuditMain.lean -- --sensitivity
lake env lean --run CompoundPatternGuardAuditMain.lean -- --cases
lake env lean --run CompoundPatternGuardAuditMain.lean -- --stats
lake env lean --run CompoundPatternGuardAuditMain.lean -- --output corpus/compound-pattern-guards.jsonl
lake env lean --run CompoundPatternGuardAuditMain.lean -- --check corpus/compound-pattern-guards.jsonl
```

Require `fixed_cases=11`, `strict_cases=3`, `internal_fixture_cases=7`, `model_only_cases=1`, `sensitivity_families=8`, and zeros for all generated-search counters.

- [ ] **Step 5: Commit generated evidence**

```bash
git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/lakefile.toml formal/HoiminOracle/HoiminOracle/CompoundPatternGuardCases.lean formal/HoiminOracle/CompoundPatternGuardAuditMain.lean formal/HoiminOracle/corpus/compound-pattern-guards.jsonl docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md
git commit -m "test(lean): generate compound pattern corpus"
```

### Task 4: Add internal Rust correspondence and retain any counterexample

**Files:**
- Create: `crates/hoimin-cli/src/analyzer/compound_pattern_guard_oracle_tests.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify only after confirmed RED: `crates/hoimin-cli/src/analyzer/rust.rs` production `visit_match` helpers.

**Interfaces:**
- Consumes: the generated corpus and existing `binding_flow_marker_snapshot`.
- Produces: a closed schema parser, `HOIMIN_COMPOUND_PATTERN_CASE` filter, seven internal comparisons, test-only sensitivity branches, and any minimal production repair supported by a retained failing row.

- [ ] **Step 1: Write closed-schema and identity tests**

Deserialize with `#[serde(deny_unknown_fields)]`. Compare the exact field set and eleven `(id, mode, observation_kind, family)` keys. Reject duplicate IDs, unknown modes or observation kinds, changed source literals, missing or duplicate markers, unsorted facts, public field-group inconsistencies, and a production observation attached to `unequal_or_capture_sets`.

Each test must name the production break it catches: accepting corpus drift could silently compare a different premise; accepting incomplete public fields could hide a candidate mismatch.

- [ ] **Step 2: Write and run the seven RED internal comparisons**

For each internal row call real production code:

```rust
let actual = binding_flow_marker_snapshot(&item.source, &item.marker)
    .unwrap_or_else(|error| panic!("{error}\ncase={}\nsource:\n{}", item.id, item.source));
assert_eq!(actual, item.expected_facts, "same-premise mismatch for {}", item.id);
```

Run:

```bash
cargo test -p hoimin-cli --lib compound_pattern_guard_oracle_tests --no-fail-fast
```

Require semantic RED for every model/implementation difference. Reject parse, marker, panic, and setup errors as infrastructure rather than semantic RED.

- [ ] **Step 3: Classify each initial mismatch**

Rerun one row at a time:

```bash
HOIMIN_COMPOUND_PATTERN_CASE=as_child_failure_precedes_alias cargo test -p hoimin-cli --lib compound_pattern_guard_oracle_tests -- --nocapture
HOIMIN_COMPOUND_PATTERN_CASE=mapping_child_failure_precedes_rest cargo test -p hoimin-cli --lib compound_pattern_guard_oracle_tests -- --nocapture
HOIMIN_COMPOUND_PATTERN_CASE=class_early_failure_precedes_capture cargo test -p hoimin-cli --lib compound_pattern_guard_oracle_tests -- --nocapture
```

Record source, expected facts, actual facts, differing names, observation site, and classification as confirmed bug, specification ambiguity, model defect, or infrastructure error before editing production semantics.

- [ ] **Step 4: Add test-only sensitivity mutations around the real transfer**

Extend `BindingFlowTestMutation` only with variants needed for the eight phase-4 families. Branch inside real pattern transfer and `visit_match`, never inside the adapter. Reuse `UsePrePatternFailureEnvironment` and `UsePreGuardFailureEnvironment` where they represent the named broken rule. Verify each applicable internal row agrees with the Lean expectation and differs under its mutation.

- [ ] **Step 5: Make the smallest production repair after confirmed RED**

If AS, mapping-rest, or class-last-capture rows fail because `invalidate_pattern_bindings` applies later captures to earlier failures, replace that one-shot failure state with a structural helper shaped as:

```rust
struct PatternBindingFlow {
    matched: Vec<KnownImports>,
    failed: Vec<KnownImports>,
}

fn pattern_binding_flow(imports: &KnownImports, pattern: &Pattern) -> PatternBindingFlow;
```

The helper must follow AST child order, carry prefix failures unchanged, apply AS and mapping-rest capture after child success, start each OR arm from the same input, and meet reachable success/failure vectors at the `visit_match` boundary. Keep `invalidate_pattern_bindings` only if other callers still need whole-pattern capture collection. Do not change the public analyzer API or unrelated statement flow.

If the internal rows match existing behavior, omit this production helper and document that no Rust semantic fix was required.

- [ ] **Step 6: Run GREEN and adjacent regressions**

```bash
cargo test -p hoimin-cli --lib compound_pattern_guard_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --lib exception_match_binding --no-fail-fast
cargo test -p hoimin-cli --lib nested_match_exit_oracle_tests --no-fail-fast
cargo fmt --all -- --check
```

For a production repair, verify Red-Green by temporarily reverting only the fix, rerunning the retained regression to observe failure, restoring the fix, and rerunning Green.

- [ ] **Step 7: Commit internal correspondence and any minimal repair**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/compound_pattern_guard_oracle_tests.rs docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md
git commit -m "fix(analyzer): preserve compound pattern failure bindings"
```

Use `test(rust): correspond compound pattern guards` instead when production semantics do not change.

### Task 5: Add strict public CLI correspondence

**Files:**
- Create: `crates/hoimin-cli/tests/lean_compound_pattern_guard_oracle.rs`

**Interfaces:**
- Consumes: the three strict corpus rows and `hoimin_cli::run_with_io`.
- Produces: complete normalized candidate observations for AS, mapping-rest, and class early failure.

- [ ] **Step 1: Write the public adapter and exact identity checks**

Reuse only fixture, process, and manifest normalization infrastructure from `lean_multiple_handler_join_oracle.rs`. Normalize the candidate overlapping the unique `list[int]` marker:

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

Zero or one overlap follows the Lean expectation. More than one overlap is a same-premise mismatch. Process launch, timeout, signal, nonzero exit, stderr on success, malformed manifest, or span/source disagreement is `infrastructure-error`.

- [ ] **Step 2: Run public RED before relying on the repair**

Run the public test against the retained corpus before applying a production repair, or temporarily revert the repair if Task 4 established the bug first:

```bash
cargo test -p hoimin-cli --test lean_compound_pattern_guard_oracle --no-fail-fast
```

Require the AS, mapping, and class rows to fail by missing complete candidates under the broken implementation. Restore the minimal fix afterward.

- [ ] **Step 3: Run GREEN with adjacent public audits**

```bash
cargo test -p hoimin-cli --test lean_compound_pattern_guard_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_exception_match_binding_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_nested_match_exit_oracle --no-fail-fast
cargo test -p hoimin-cli --test lean_multiple_handler_join_oracle --no-fail-fast
```

- [ ] **Step 4: Commit strict correspondence**

```bash
git add crates/hoimin-cli/tests/lean_compound_pattern_guard_oracle.rs docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md
git commit -m "test(cli): audit compound pattern failures"
```

### Task 6: Final report, verification, PR, CI, and merge

**Files:**
- Modify: `docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md`

**Interfaces:**
- Consumes: fresh guarded command results, Rust outputs, mismatch ledger, final diff, and CI results.
- Produces: a self-contained phase-4 report, merged PR, issue #300 phase update, and removal of only this phase worktree.

- [ ] **Step 1: Run fresh Lean verification in required order**

Run the focused proof build and proof consumer, cases, sensitivity, temporary corpus generation plus byte comparison, freshness check, and stats. Run every command alone with the fixed guard. Record child exit, stop reason, elapsed milliseconds, and peak RSS KiB. Do not retry a resource stop with larger limits.

- [ ] **Step 2: Run fresh Rust and repository verification**

Create `.venv -> ../../.venv` only if the public tests require it, then run:

```bash
cargo test -p hoimin-cli --lib compound_pattern_guard_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --test lean_compound_pattern_guard_oracle --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -j 2 -- -D warnings
cargo test --workspace --all-features -j 2
git diff --check origin/main...HEAD
```

Remove the temporary symlink after tests. Require no untracked phase artifacts.

- [ ] **Step 3: Complete and self-review the report**

Record the audited claim, correspondence worksheet, eleven-row mode counts, eight sensitivity results, theorem premises, production observations, every mismatch classification, retained counterexamples, exact Rust change or explicit non-change, exclusions, commands, elapsed times, peak RSS, and generated-search zeros. State that Lean proves only the reduced model.

- [ ] **Step 4: Review the complete diff and commit the report**

Invoke `superpowers:verification-before-completion` and `superpowers:requesting-code-review`. The current session cannot dispatch review subagents under its governing instruction, so inspect `origin/main...HEAD` as a fresh full diff, check every design requirement against a file and command, repair findings, and rerun affected checks.

```bash
git add docs/superpowers/reports/2026-08-14-lean-compound-pattern-guards-audit.md
git commit -m "docs: report compound pattern guard audit"
```

- [ ] **Step 5: Push and create the PR**

Push `audit/lean-compound-pattern-guards`. Create a PR against `main` that references issue #300 without closing it and states the Lean/Rust boundary, corpus counts, sensitivity results, mismatch decisions, production change, and verification summary.

- [ ] **Step 6: Require CI, merge, and update issue #300**

Monitor all required and cross-platform checks. On a failure, invoke `superpowers:systematic-debugging`, fix only in this worktree, rerun the affected local checks, and push. After all required checks pass, squash-merge, fetch, confirm the merge commit belongs to `origin/main`, and comment on issue #300 with phase-4 evidence and phase 5 remaining.

- [ ] **Step 7: Clean only this phase worktree and stop**

Confirm no tracked or untracked phase files remain. Remove `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-compound-pattern-guards`, prune worktrees, and fast-forward the primary `main` without touching its existing `.idea/`, `.serena/`, or `tests/fixtures/projects/basic/uv.lock` files. Stop after phase 4 as requested.
