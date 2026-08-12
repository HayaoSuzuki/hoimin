# Lean Progress Decision Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the pair and history decision semantics behind `hoimin progress`, replay a Lean-generated corpus through the public Rust CLI, and make the smallest test-first Rust correction only if strict correspondence exposes a mismatch.

**Architecture:** Three imported Lean modules own the pure pair/history model, kernel-checked theorems, and fixed cases. A non-imported audit executable performs bounded exploration, mandatory broken-variant sensitivity checks, and deterministic JSONL generation; a dedicated `hoimin-cli` integration test translates those rows into owned schema-v2 reports and observes public JSON output.

**Tech Stack:** Lean 4.32.2, Lake, Rust 2024 edition, Serde JSON, Tokio process tests, Cargo integration tests.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-progress-decision` on branch `audit/lean-progress-decision`.
- Keep the specification, plan, Lean files, generated corpus, Rust adapter, any regression repair, and audit report in that worktree.
- Use only `strict`, `model-only`, `internal-fixture`, and `infrastructure-error` correspondence classifications.
- Keep imported Lean modules free of exhaustive evaluation, filesystem I/O, and corpus serialization.
- Use local `maxHeartbeats 100000` limits for nontrivial proofs; never use unlimited heartbeats.
- Run one Lean command at a time with a 20-second wall-clock deadline, a 768 MiB process-tree RSS ceiling, 250 ms sampling, and one Lake job.
- Do not retry the aggregate Lean build with a larger cap; preserve its baseline RSS failure as `infrastructure-error` and use focused commands under the same cap.
- Generate expectations only in Lean. Rust may translate inputs and compare observations but must not independently recompute expected decisions.
- Add a Rust production change only after preserving the smallest strict Lean witness and observing the corresponding public Rust regression test fail.
- Do not weaken Lean semantics or remove a strict corpus case to match Rust.
- Exact score correspondence is limited to `null`, `0`, `0.5`, and `1`.

---

## File Structure

- Create `formal/HoiminOracle/HoiminOracle/ProgressDecisionModel.lean`: pure report, pairing, comparison, and history semantics.
- Create `formal/HoiminOracle/HoiminOracle/ProgressDecisionProofs.lean`: suffix, reset, precedence, correspondence-key, and inconclusive theorems.
- Create `formal/HoiminOracle/HoiminOracle/ProgressDecisionCases.lean`: strict/model-only fixed cases and mandatory sensitivity witnesses.
- Create `formal/HoiminOracle/ProgressDecisionAuditMain.lean`: bounded enumeration, shrinking, stats, JSONL serialization, generation, and freshness checking.
- Create `formal/HoiminOracle/corpus/progress-decision.jsonl`: deterministic Lean-generated expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import the three proof-oriented modules.
- Modify `formal/HoiminOracle/lakefile.toml`: register `generate_progress_decision`.
- Create `crates/hoimin-cli/tests/lean_progress_decision_oracle.rs`: strict corpus parser, schema-v2 report builder, public CLI runner, and field comparison.
- Modify `crates/hoimin-cli/tests/progress.rs` and `crates/hoimin-cli/src/progress/compare.rs` only if Task 4 confirms a strict mismatch.
- Create `docs/superpowers/reports/2026-08-12-lean-progress-decision-audit.md`: self-contained claims, evidence, resource measurements, and counterexample ledger.

### Task 1: Pure Lean pair/history model and proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ProgressDecisionModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/ProgressDecisionProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-progress-decision-proof-consumer.lean`

**Interfaces:**
- Consumes: Lean `Std` collections and arithmetic.
- Produces: `Status`, `Mutant`, `Report`, `Eligibility`, `JoinMode`, `PairState`, `LatestState`, `Counts`, `PairObservation`, `HistoryObservation`, `joinMode`, `transitionCounts`, `comparePair`, `compareHistory`, `trailingStalls`, and the named theorems below.

- [ ] **Step 1: Write the failing proof consumer**

Create `/tmp/hoimin-progress-decision-proof-consumer.lean` with:

```lean
import HoiminOracle.ProgressDecisionProofs

open HoiminOracle.ProgressDecision

example : classifyPair .matching 2 1 1 = .regressing := by decide
example : classifyPair .matching 1 0 1 = .improving := by decide
example : classifyPair .different 2 0 0 = .indeterminate := by decide

example (history : List PairStep) (patience : Nat) :
    (foldPairStepsWithPatience history patience).consecutiveStalls =
      trailingStalls history :=
  fold_consecutive_stalls_eq_trailing history patience

example (history : List PairStep) (patience : Nat) (positive : 0 < patience) :
    (foldPairStepsWithPatience history patience).latest = .saturated →
      patience ≤ trailingStalls history :=
  saturated_implies_patience_le_trailing history patience positive
```

- [ ] **Step 2: Run the consumer and verify RED**

From `formal/HoiminOracle`, run under the resource guard:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-proof-red.json -- lake env lean /tmp/hoimin-progress-decision-proof-consumer.lean
```

Expected: FAIL because `HoiminOracle.ProgressDecisionProofs` does not exist; the guard itself must report normally.

- [ ] **Step 3: Implement the pure model**

Create `ProgressDecisionModel.lean` in namespace `HoiminOracle.ProgressDecision` with these public shapes:

```lean
inductive Status | killed | survived | inconclusive
  deriving Repr, DecidableEq, BEq, Inhabited

structure Mutant where
  candidateId : Nat
  contentKey : Nat
  status : Status
  deriving Repr, DecidableEq, BEq

inductive Report | unusable | usable (mutants : List Mutant)
  deriving Repr, DecidableEq, BEq

inductive Eligibility | matching | different | duplicate
  deriving Repr, DecidableEq, BEq

inductive JoinMode | candidateId | contentKey
  deriving Repr, DecidableEq, BEq

inductive PairState | improving | regressing | stalled | indeterminate
  deriving Repr, DecidableEq, BEq

inductive LatestState
  | improving | regressing | stalled | saturated | indeterminate
  deriving Repr, DecidableEq, BEq

structure Counts where
  common added removed ambiguous inconclusive : Nat
  improvements regressions carriedSurvivors : Nat
  previousKilled previousSurvived currentKilled currentSurvived : Nat
  deriving Repr, DecidableEq, BEq

structure PairObservation where
  eligibility : Eligibility
  counts : Counts
  state : PairState
  deriving Repr, DecidableEq, BEq

inductive PairStep | unusableAdjacency | compared (observation : PairObservation)
  deriving Repr, DecidableEq, BEq

structure HistoryObservation where
  comparisons : List PairObservation
  latest : LatestState
  consecutiveStalls : Nat
  deriving Repr, DecidableEq, BEq
```

Implement `candidateIds`, duplicate detection, `eligibility`, ID-key indexing for matching unique ID sets, content-key indexing with ambiguity removal otherwise, `comparePair`, and `compareHistory`. Define exact score components as numerator/denominator pairs rather than floating point. Keep the classifier separate and total:

```lean
def classifyPair
    (eligibility : Eligibility)
    (comparableCommon regressions improvements : Nat) : PairState :=
  if eligibility != .matching || comparableCommon = 0 then .indeterminate
  else if regressions > 0 then .regressing
  else if improvements > 0 then .improving
  else .stalled
```

Define `joinMode .matching = .candidateId` and use `.contentKey` for `.different` and `.duplicate`. Define `transitionCounts : Status → Status → Counts` as the single-key contribution used by `comparePair`, so inconclusive exclusion is stated and proved once.

`foldPairStepsWithPatience` must reset on `.unusableAdjacency`, `.indeterminate`, `.improving`, and `.regressing`; increment only on `.stalled`; and use `consecutiveStalls >= patience` for saturation. `compareHistory` derives adjacent `PairStep` values with `reports.zip reports.tail` semantics and never compares across an unusable gap.

- [ ] **Step 4: Implement the proof module**

Create `ProgressDecisionProofs.lean`, import the model, and establish these theorem signatures with explicit premises:

```lean
theorem fold_consecutive_stalls_eq_trailing
    (steps : List PairStep) (patience : Nat) :
  (foldPairStepsWithPatience steps patience).consecutiveStalls =
    trailingStalls steps

theorem unusable_resets (steps : List PairStep) (patience : Nat) :
  (foldPairStepsWithPatience
    (steps ++ [.unusableAdjacency]) patience).consecutiveStalls = 0

theorem nonstalled_resets
    (steps : List PairStep) (obs : PairObservation) (patience : Nat)
    (h : obs.state ≠ .stalled) :
  (foldPairStepsWithPatience
    (steps ++ [.compared obs]) patience).consecutiveStalls = 0

theorem saturated_implies_latest_stalled
    (steps : List PairStep) (patience : Nat) (positive : 0 < patience) :
  (foldPairStepsWithPatience steps patience).latest = .saturated →
    ∃ obs, steps.getLast? = some (.compared obs) ∧ obs.state = .stalled

theorem saturated_implies_patience_le_trailing
    (steps : List PairStep) (patience : Nat) (positive : 0 < patience) :
  (foldPairStepsWithPatience steps patience).latest = .saturated →
    patience ≤ trailingStalls steps

theorem patience_le_trailing_implies_saturated
    (steps : List PairStep) (patience : Nat) (positive : 0 < patience)
    (enough : patience ≤ trailingStalls steps) :
  (foldPairStepsWithPatience steps patience).latest = .saturated

theorem regression_precedes_improvement
    (common regressions improvements : Nat) (hasRegression : 0 < regressions) :
  classifyPair .matching common regressions improvements =
    (if common = 0 then .indeterminate else .regressing)

theorem pair_never_saturated (pair : PairObservation) :
  pair.state = .improving ∨ pair.state = .regressing ∨
    pair.state = .stalled ∨ pair.state = .indeterminate

theorem matching_uses_candidate_id :
  joinMode .matching = .candidateId

theorem inconclusive_transition_has_no_directional_or_score_counts
    (before after : Status)
    (h : before = .inconclusive ∨ after = .inconclusive) :
  let counts := transitionCounts before after
  counts.improvements = 0 ∧ counts.regressions = 0 ∧
    counts.previousKilled + counts.previousSurvived = 0 ∧
    counts.currentKilled + counts.currentSurvived = 0
```

Use `matching_uses_candidate_id` together with a general indexing lemma showing that `.candidateId` keys do not inspect `contentKey`; retain the repeated-content fixed case in Task 2 as executable evidence. Use local `set_option maxHeartbeats 100000 in` only around proofs that need it.

- [ ] **Step 5: Import modules and verify GREEN**

Add imports for `ProgressDecisionModel` and `ProgressDecisionProofs` to `HoiminOracle.lean`. Run one guarded command at a time:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-model.json -- lake env lean HoiminOracle/ProgressDecisionModel.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-proofs.json -- lake env lean HoiminOracle/ProgressDecisionProofs.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-proof-consumer.json -- lake env lean /tmp/hoimin-progress-decision-proof-consumer.lean
```

Expected: all three exit zero within both limits.

- [ ] **Step 6: Commit model and proofs**

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/HoiminOracle/ProgressDecisionModel.lean \
  formal/HoiminOracle/HoiminOracle/ProgressDecisionProofs.lean
git commit -m "test(lean): prove progress decision semantics"
```

### Task 2: Fixed cases, bounded refutation, executable, and corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ProgressDecisionCases.lean`
- Create: `formal/HoiminOracle/ProgressDecisionAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/progress-decision.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 1 semantics and theorem-backed public definitions.
- Produces: `progressDecisionCases`, `sensitivityPasses`, `boundedCheck`, deterministic corpus schema 1, and executable `generate_progress_decision` supporting `--cases`, `--sensitivity`, `--stats`, `--output PATH`, and `--check PATH`.

- [ ] **Step 1: Register the absent executable and verify RED**

Append to `formal/HoiminOracle/lakefile.toml`:

```toml
[[lean_exe]]
name = "generate_progress_decision"
root = "ProgressDecisionAuditMain"
```

Run under the fixed guard:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-exe-red.json -- lake exe generate_progress_decision -- --sensitivity
```

Expected: FAIL because `ProgressDecisionAuditMain.lean` does not exist.

- [ ] **Step 2: Define fixed strict and model-only cases**

Create `ProgressDecisionCases.lean` with a corpus input type containing `id`, `mode`, positive `patience`, and `reports`; expected output contains every comparison count, pair state, rational score pair or null, latest state, `consecutiveStalls`, and saturated flag.

Define at least these unique cases, using candidate roles `0..1`, content roles `0..1`, and statuses from Task 1:

```text
patience_1_first_stall
patience_2_two_stalls
patience_3_short_suffix
improvement_resets_stalls
regression_resets_stalls
simultaneous_regression_wins
inconclusive_resets_stalls
unusable_gap_resets_stalls
matching_ids_ignore_duplicate_content
different_ids_are_indeterminate
different_ids_duplicate_content_is_ambiguous
added_and_removed_counts
empty_comparable_common
score_zero
score_half
score_one
duplicate_candidate_id_model_only
```

Represent Rust timeout, OOM, process-limit, error, and not-run with five strict rows sharing the Lean `.inconclusive` expectation but carrying distinct adapter status tags. The duplicate candidate-ID row is `model-only`; all other listed rows are `strict`.

- [ ] **Step 3: Add mandatory broken variants and fixed witnesses**

In the cases module define seven separate broken semantics:

```lean
def brokenImprovementFirst ...
def brokenKeepIndeterminateSuffix ...
def brokenKeepUnusableSuffix ...
def brokenStrictPatience ...
def brokenJoinMatchingByContent ...
def brokenDuplicateContentUnique ...
def brokenCountInconclusive ...
```

For each, retain a named smallest fixed witness and a Boolean detector. Define `sensitivityPasses` as their conjunction. Corpus output and checking must abort before filesystem access unless every detector is true.

- [ ] **Step 4: Implement bounded enumeration and deterministic shrinking**

In `ProgressDecisionAuditMain.lean`, enumerate the exact declared finite domain: two candidate IDs, two content keys, three statuses, histories of length `0..4`, and patience `1..3`. Each usable report contains zero, one, or two mutants and each candidate-ID role occurs at most once; duplicate-ID behavior remains the fixed `model-only` witness. Traverse histories breadth-first as reachable `(lastReport, HistoryObservation)` states, deduplicating after every depth instead of materializing the full Cartesian product. Order transitions lexicographically by candidate/content/status role.

`boundedCheck` compares the primary model with separately written local specifications for classifier precedence and trailing suffix. On failure, shrink by removing reports from the right, removing mutants from the right, lowering patience, then lowering role/status ordinals; print the smallest witness and exit nonzero. `--stats` prints schema version, maximum history length, patience range, explored transitions, deduplicated states, pair count, fixed-case count, strict count, and model-only count. The resource-guard JSON supplies elapsed milliseconds and process-tree peak RSS separately.

- [ ] **Step 5: Implement stable JSONL and freshness commands**

Use `Lean.Data.Json` and emit one object per fixed case. Store score as either `null` or `{ "killed": N, "decidable": N }`; use explicit strings for states and modes. Ensure `--output` uses the same in-memory bytes as `--check`; `--check` performs byte-for-byte comparison and exits nonzero for stale or missing content.

Run separately under the guard:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-sensitivity.json -- lake exe generate_progress_decision -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-cases.json -- lake exe generate_progress_decision -- --cases
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-stats.json -- lake exe generate_progress_decision -- --stats
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-output.json -- lake exe generate_progress_decision -- --output corpus/progress-decision.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-fresh.json -- lake exe generate_progress_decision -- --check corpus/progress-decision.jsonl
```

Expected: all commands exit zero, seven sensitivity detectors pass, bounded checking completes within both limits, and freshness succeeds.

- [ ] **Step 6: Verify deterministic generation and commit**

Generate to `/tmp/progress-decision-second.jsonl` under the same guard and compare:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-second.json -- lake exe generate_progress_decision -- --output /tmp/progress-decision-second.jsonl
cmp corpus/progress-decision.jsonl /tmp/progress-decision-second.jsonl
git diff --check
```

Expected: `cmp` and `git diff --check` exit zero.

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/HoiminOracle/ProgressDecisionCases.lean \
  formal/HoiminOracle/ProgressDecisionAuditMain.lean \
  formal/HoiminOracle/corpus/progress-decision.jsonl \
  formal/HoiminOracle/lakefile.toml
git commit -m "test(lean): generate progress decision oracle"
```

### Task 3: Public Rust CLI correspondence adapter

**Files:**
- Create: `crates/hoimin-cli/tests/lean_progress_decision_oracle.rs`
- Test: `formal/HoiminOracle/corpus/progress-decision.jsonl`

**Interfaces:**
- Consumes: Task 2 schema-1 JSONL rows and the built `hoimin` binary.
- Produces: `lean_progress_decision_corpus_is_valid` and `public_progress_matches_every_strict_lean_case`; any semantic mismatch names the Lean case and field, while setup/process/parser failures are reported separately as infrastructure errors.

- [ ] **Step 1: Write the corpus schema validator**

Define all corpus structs with `#[serde(deny_unknown_fields)]`, include the corpus with:

```rust
const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/progress-decision.jsonl");
```

Reject an unknown schema, unknown mode/state/status, empty or duplicate IDs, zero patience, out-of-domain roles, malformed score fractions, a `saturated` expected pair state, and duplicate candidate IDs in a strict row. Assert every required fixed ID from Task 2 is present and at least one row exists for each concrete Rust inconclusive status.

- [ ] **Step 2: Build owned schema-v2 reports without encoding expected decisions**

Define the fixture source exactly as:

```rust
fn base_report() -> serde_json::Value {
    serde_json::from_str(include_str!("golden/reports/schema-v2-current.json"))
        .expect("owned schema-v2 golden report must remain valid JSON")
}
```

For every Lean report:

- `.usable`: create one mutant event per input mutant, assign unique event/candidate sequence numbers, use the corpus candidate ID and content roles to populate `candidate.id`, path, original, replacement, operator, and symbol, map the adapter status tag to the corresponding public status string, and recompute only report summary counts required for parser validity;
- `.unusable`: create a valid report then apply the row's reason (`baseline = null`, failed baseline termination, or `summary.complete = false`);
- preserve oldest-to-newest path order and write each report into one isolated `tempfile::TempDir`.

This translation may satisfy schema mechanics but must not calculate comparison state, suffix length, counts, or scores.

- [ ] **Step 3: Invoke the public CLI and compare the complete observation**

For each strict case execute:

```rust
let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
command
    .args(["progress", "--format", "json", "--patience"])
    .arg(case.patience.to_string())
    .args(&report_paths);
```

Bound execution at 30 seconds. Require exit code zero and valid JSON. Compare top-level patience, latest state, latest/top-level consecutive stalls, saturated flag, comparison length, all eight integer count fields, pair states, and score fields. Convert Lean fractions only at the final observation boundary and permit only `0/1`, `1/2`, and `1/1`; compare those exact `f64` values.

Any duplicate-ID `model-only` row is schema-validated but not sent to the public parser. A process timeout, spawn failure, nonzero parser exit, or malformed stdout is an infrastructure failure and must not be printed as a semantic field mismatch.

- [ ] **Step 4: Run the adapter and classify the result**

```bash
cargo test -p hoimin-cli --test lean_progress_decision_oracle -- --nocapture
```

Expected outcome A: PASS, proving no strict mismatch and making Task 4's production branch unnecessary. Expected outcome B: FAIL with one or more case/field differences while corpus validation and CLI infrastructure pass; preserve the smallest failed corpus row and continue to Task 4.

- [ ] **Step 5: Commit the adapter evidence**

```bash
git add crates/hoimin-cli/tests/lean_progress_decision_oracle.rs
git commit -m "test: compare progress decisions with Lean oracle"
```

If the strict adapter is RED, this commit intentionally records the reproducible mismatch before production repair. If it is GREEN, record an empty mismatch ledger in Task 5.

### Task 4: Conditional TDD repair of a confirmed strict mismatch

**Files:**
- Modify only if needed: `crates/hoimin-cli/tests/progress.rs`
- Modify only if needed: `crates/hoimin-cli/src/progress/compare.rs`
- Test: `crates/hoimin-cli/tests/lean_progress_decision_oracle.rs`

**Interfaces:**
- Consumes: the smallest strict failing row from Task 3.
- Produces: one named public regression test and the smallest `compare.rs` correction that makes the row match without changing public types or weakening the corpus.

- [ ] **Step 1: Minimize and record the mismatch**

Use the Lean executable's shrink order and the adapter's case filter to reduce to one case. Record its exact reports, patience, expected observation, actual observation, and differing fields in `/tmp/hoimin-progress-mismatch.md`. Confirm all premises are `strict`; otherwise reclassify as `model-only` or `infrastructure-error` and make no Rust production change.

- [ ] **Step 2: Add the public Rust regression and verify RED**

Add one `#[tokio::test]` named `lean_witness_strict_progress_decision_mismatch` to `crates/hoimin-cli/tests/progress.rs` using `write_json`, owned schema-v2 documents, and `run_progress`. Assert only the complete minimal public observation from the witness.

Run exactly that test:

```bash
cargo test -p hoimin-cli --test progress lean_witness_strict_progress_decision_mismatch -- --exact --nocapture
```

Expected: FAIL with the same semantic difference as Task 3; do not proceed on a parser/setup failure.

- [ ] **Step 3: Make the smallest production correction**

Change only the responsible branch in `compare_reports`, `comparison_state`, `candidate_set_eligibility`, or `index_mutants`. Preserve `ProgressResult`, `Comparison`, `ProgressState`, JSON schema, CLI arguments, ordering, and unrelated count behavior. Do not refactor neighboring logic in this step.

- [ ] **Step 4: Verify GREEN and no collateral mismatch**

```bash
cargo test -p hoimin-cli --test progress lean_witness_strict_progress_decision_mismatch -- --exact --nocapture
cargo test -p hoimin-cli --test lean_progress_decision_oracle -- --nocapture
cargo test -p hoimin-cli --test progress
cargo fmt --all -- --check
```

Expected: all commands exit zero and the strict mismatch set is empty.

- [ ] **Step 5: Commit the minimal repair**

```bash
git add crates/hoimin-cli/tests/progress.rs crates/hoimin-cli/src/progress/compare.rs
git commit -m "fix: align progress decision with Lean witness"
```

If Task 3 was GREEN, skip all Task 4 edits and commits explicitly; no production diff is a valid audit result.

### Task 5: Audit report and complete verification

**Files:**
- Create: `docs/superpowers/reports/2026-08-12-lean-progress-decision-audit.md`
- Modify only if evidence requires: files from Tasks 1-4.

**Interfaces:**
- Consumes: theorem, bounded-refutation, sensitivity, corpus freshness, public adapter, optional repair, and resource evidence.
- Produces: a self-contained report whose claims distinguish Lean model proofs, bounded evidence, strict Rust correspondence, model-only cases, and infrastructure limits.

- [ ] **Step 1: Run final focused Lean evidence one command at a time**

Use the fixed guard for each command and distinct `/tmp` stats files:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-final-model.json -- lake env lean HoiminOracle/ProgressDecisionModel.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-final-proofs.json -- lake env lean HoiminOracle/ProgressDecisionProofs.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-final-consumer.json -- lake env lean /tmp/hoimin-progress-decision-proof-consumer.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-final-sensitivity.json -- lake exe generate_progress_decision -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-final-cases.json -- lake exe generate_progress_decision -- --cases
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-final-stats.json -- lake exe generate_progress_decision -- --stats
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-progress-final-fresh.json -- lake exe generate_progress_decision -- --check corpus/progress-decision.jsonl
```

Do not run these in parallel. Expected: each focused command exits zero; otherwise report the exact guard reason without claiming semantic success.

- [ ] **Step 2: Run Rust and repository verification**

```bash
cargo test -p hoimin-cli --test lean_progress_decision_oracle -- --nocapture
cargo test -p hoimin-cli --test progress
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
git status --short
```

Expected: tests, clippy, formatting, and diff checks exit zero. `git status --short` may show only the intentionally ignored setup `.venv` link before final documentation is committed; no generated or source file may be unstaged.

- [ ] **Step 3: Write the audit report**

The report must include:

- the durable progress decision claim and exact exclusions;
- the correspondence worksheet with every case classified;
- theorem names, premises, and a warning that Lean proves the model rather than Rust directly;
- finite domain, enumeration order, explored/deduplicated counts, shrink order, and boundedness caveat;
- all seven broken variants and their smallest witnesses;
- why duplicate-key idempotency/uniqueness is covered, while atomicity and transactionality do not apply to this pure computation;
- corpus schema, deterministic/freshness evidence, exact score boundary, and public adapter mechanics;
- a counterexample ledger containing either every strict mismatch and repair commit or the explicit entry `strict mismatches: none`;
- the duplicate-candidate-ID `model-only` boundary and five concrete inconclusive status mappings;
- the aggregate Lean baseline RSS failure at 950,352 KiB and focused command elapsed/RSS measurements;
- exact Rust test, clippy, formatting, and Git verification results.

- [ ] **Step 4: Self-review claims against evidence**

Search the report and changed files:

```bash
rg -n 'TB[D]|TO[D]O|FIX[M]E|unbounded proof of Rust|all histories|no mismatch' docs/superpowers/reports/2026-08-12-lean-progress-decision-audit.md
git diff --check
git diff --stat main...HEAD
```

Expected: no placeholders; any `no mismatch` wording is explicitly scoped to strict generated cases; the diff contains only the design, plan, progress Lean modules/executable/corpus, Rust adapter, optional two Rust repair files, and report.

- [ ] **Step 5: Commit the report and final evidence**

```bash
git add docs/superpowers/reports/2026-08-12-lean-progress-decision-audit.md
git commit -m "docs: report Lean progress decision audit"
```

- [ ] **Step 6: Perform the final clean-tree check**

```bash
git status -sb
git log --oneline main..HEAD
```

Expected: the branch contains the design, plan, model/proofs, oracle/corpus, adapter, optional repair, and report commits; no tracked changes remain.
