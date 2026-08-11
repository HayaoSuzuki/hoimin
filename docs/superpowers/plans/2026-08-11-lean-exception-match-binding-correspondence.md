# Lean Exception and Match Binding Correspondence Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish Lean-generated, exact-site correspondence for exception-handler target cleanup and structural-match failed-case binding propagation, fixing Hoimin only if a minimized same-premise mismatch is found.

**Architecture:** Reuse `BindingFlow.Env`, `Fact`, `ExitCategory`, and `Exits` in a focused Lean transition model rather than extending the generic statement evaluator. Generate a fixed JSONL corpus, then compare each row with narrow marker-addressed Rust observations from `NameResolutionIndex`, `AnnotationCollector`, or the public `hoimin plan` manifest.

**Tech Stack:** Lean 4 with `Std`, Rust 2024 workspace, Ruff Python AST/parser, Serde JSONL, Cargo tests, Python resource guard.

## Global Constraints

- Run every Lean or Lake command alone through `formal/HoiminOracle/tools/lean_resource_guard.py`.
- Use a 20-second deadline, 768 MiB root-plus-descendant RSS limit, and 250 ms sampling.
- Use `lake -Kjobs=1` for builds; never run concurrent Lean commands.
- Use `lake env lean --run` for executable cases; do not retry full-library or native-link commands that exceed the RSS limit.
- Keep all cases fixed and human-readable; do not increase structured exploration beyond depth 2.
- Use `set_option maxHeartbeats 100000 in` for non-trivial theorems; never use unlimited heartbeats.
- Do not add `sorry`, `admit`, custom axioms, or hand-edited generated expectations.
- Treat parse, marker, timeout, RSS, malformed corpus, panic, and nonzero public exits as infrastructure errors.
- Keep private projections under `#[cfg(test)]`; preserve the public analyzer API unless a failing same-premise case proves a defect.
- Run Rust workspace compilation with at most two jobs locally.

---

## File Map

- Create `formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingModel.lean`: focused handler and ordered-case transitions.
- Create `formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingProofs.lean`: quantified invariants and literal broken witnesses.
- Create `formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingCases.lean`: closed case schema, fixtures, validation, and JSON rendering.
- Create `formal/HoiminOracle/ExceptionMatchBindingAuditMain.lean`: deterministic cases, sensitivity, stats, output, and freshness CLI.
- Create `formal/HoiminOracle/corpus/exception-match-binding-correspondence.jsonl`: Lean-owned expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import model and proof modules only.
- Modify `formal/HoiminOracle/lakefile.toml`: register the focused executable without importing cases into the full library.
- Modify `crates/hoimin-cli/src/analyzer/rust.rs`: add only missing test-only exact-site projections.
- Modify `crates/hoimin-cli/src/analyzer/rust_tests.rs`: private corpus correspondence and sensitivity tests.
- Create `crates/hoimin-cli/tests/lean_exception_match_binding_oracle.rs`: strict schema and public candidate correspondence.
- Create `docs/superpowers/reports/2026-08-11-lean-exception-match-binding-correspondence-audit.md`: claims, exclusions, mismatches, sensitivity, and resource ledger.

### Task 1: Focused Lean transition model and proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Consumes: `HoiminOracle.BindingFlow.{Name, Fact, Env, ExitCategory, Exits}`.
- Produces: `deleteName`, `bindTarget`, `cleanupHandlerExits`, `HandlerObservation`, `observeHandler`, `PatternResult`, `CaseStep`, `advanceCase`, `finishMatch`, plus cleanup and propagation theorems.

- [ ] **Step 1: Write a proof consumer and verify Red**

Create `/tmp/hoimin-exception-match-proof-consumer.lean`:

```lean
import HoiminOracle.ExceptionMatchBindingProofs

open HoiminOracle BindingFlow ExceptionMatchBinding

example (name : Name) (environment : Env) :
    (cleanupHandlerExits name
      (.categoryOnly .terminate environment)).terminates =
        [deleteName environment name] :=
  handler_cleanup_terminate name environment
```

Run:

```bash
python3 tools/lean_resource_guard.py \
  --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 \
  --stats /tmp/hoimin-exception-match-red.json -- \
  lake env lean /tmp/hoimin-exception-match-proof-consumer.lean
```

Expected: exit 1 because the imported module does not exist.

- [ ] **Step 2: Implement the minimal handler transition**

In `ExceptionMatchBindingModel.lean`, define the handler order without parsing
or strings:

```lean
import HoiminOracle.BindingFlowModel

namespace HoiminOracle.ExceptionMatchBinding
open BindingFlow

def deleteName (environment : Env) (name : Name) : Env :=
  environment.set name .shadowed

def bindTarget (environment : Env) (name : Name) : Env :=
  environment.set name .shadowed

def mapExitEnvs (transform : Env → Env) (exits : Exits) : Exits where
  fallthrough := exits.fallthrough.map transform
  breaks := exits.breaks.map transform
  continues := exits.continues.map transform
  terminates := exits.terminates.map transform

def cleanupHandlerExits (name : Name) (body : Exits) : Exits :=
  mapExitEnvs (fun environment => deleteName environment name) body

structure HandlerObservation where
  typeEntry : Env
  bodyEntry : Env
  exits : Exits
  deriving Repr, DecidableEq, BEq

def observeHandler (incoming : Env) (target : Name)
    (body : Env → Exits) : HandlerObservation where
  typeEntry := incoming
  bodyEntry := bindTarget incoming target
  exits := cleanupHandlerExits target (body (bindTarget incoming target))
```

The production analyzer treats return and raise together as `terminate`; keep
distinct fixtures but do not invent separate Lean exit categories.

- [ ] **Step 3: Implement ordered match-case transitions**

Add these model shapes:

```lean
structure PatternResult where
  matched : Env
  failed : Option Env
  deriving Repr, DecidableEq, BEq

structure CaseStep where
  body : Option Env
  nextCase : Option Env
  deriving Repr, DecidableEq, BEq

def advanceCase (pattern : PatternResult) (guardPassed : Option Bool)
    (guardEnvironment : Env := pattern.matched) : CaseStep :=
  match guardPassed with
  | none => { body := some pattern.matched, nextCase := pattern.failed }
  | some true => { body := some guardEnvironment, nextCase := pattern.failed }
  | some false =>
      { body := none
        nextCase := meetOption pattern.failed (some guardEnvironment) }

def finishMatch (unmatched : Option Env)
    (completed : List Env) : Option Env :=
  meetAll? (completed ++ unmatched.toList)
```

An irrefutable pattern is represented with `failed := none`; a refutable
pattern supplies the conservative environment at its exact failure point.

- [ ] **Step 4: Prove the semantic invariants**

In `ExceptionMatchBindingProofs.lean`, add exact theorems for:

```lean
theorem handler_type_precedes_target
    (incoming : Env) (target : Name) (body : Env → Exits) :
    (observeHandler incoming target body).typeEntry = incoming := by rfl

theorem handler_body_has_target
    (incoming : Env) (target : Name) (body : Env → Exits) :
    (observeHandler incoming target body).bodyEntry.get target = .shadowed := by
  cases target <;> rfl

theorem handler_cleanup_fallthrough (target : Name) (environment : Env) :
    (cleanupHandlerExits target
      (.categoryOnly .fallthrough environment)).fallthrough =
        some (deleteName environment target) := by rfl

theorem handler_cleanup_break (target : Name) (environment : Env) :
    (cleanupHandlerExits target (.categoryOnly .break environment)).breaks =
      [deleteName environment target] := by rfl

theorem handler_cleanup_continue (target : Name) (environment : Env) :
    (cleanupHandlerExits target
      (.categoryOnly .continue environment)).continues =
        [deleteName environment target] := by rfl

theorem handler_cleanup_terminate (target : Name) (environment : Env) :
    (cleanupHandlerExits target
      (.categoryOnly .terminate environment)).terminates =
        [deleteName environment target] := by rfl

theorem handler_cleanup_preserves_other_name
    (environment : Env) (target observed : Name) (different : target ≠ observed) :
    (deleteName environment target).get observed = environment.get observed := by
  cases target <;> cases observed <;> simp_all [deleteName, Env.set, Env.get]

theorem handler_join_is_meet (left right : Env) :
    meetOption (some left) (some right) = some (left.meet right) := by rfl

theorem failed_pattern_reaches_next_case (pattern : PatternResult) :
    (advanceCase pattern none).nextCase = pattern.failed := by rfl

theorem false_guard_reaches_next_case
    (pattern : PatternResult) (afterGuard : Env) :
    (advanceCase pattern (some false) afterGuard).nextCase =
      meetOption pattern.failed (some afterGuard) := by rfl

theorem irrefutable_case_has_no_unmatched_path (matched : Env) :
    (advanceCase { matched, failed := none } none).nextCase = none := by rfl

theorem match_join_includes_refutable_unmatched
    (completed : List Env) (unmatched : Env) :
    finishMatch (some unmatched) completed =
      meetAll? (completed ++ [unmatched]) := by rfl
```

Each cleanup theorem must quantify over arbitrary environments and verify
`Env.get target = .shadowed`; the preservation theorem must assume the observed
name differs from the target. Do not settle only for fixed literal examples.

- [ ] **Step 5: Add literal broken witnesses**

Define private broken variants and decidable witnesses for:

```lean
private def brokenBindBeforeType (incoming : Env) (target : Name)
    (body : Env → Exits) : HandlerObservation :=
  { observeHandler incoming target body with
      typeEntry := bindTarget incoming target }

private def brokenCleanupFallthroughOnly (target : Name) (body : Exits) : Exits :=
  { body with fallthrough := body.fallthrough.map
      (fun environment => deleteName environment target) }

private def brokenHandlerJoin (left _right : Env) : Env := left

private def brokenDiscardPatternFailure (pattern : PatternResult) : CaseStep :=
  { body := some pattern.matched, nextCase := none }

private def brokenDiscardGuardFailure
    (pattern : PatternResult) (_afterGuard : Env) : CaseStep :=
  { body := none, nextCase := pattern.failed }

private def brokenRetainIrrefutableUnmatched (incoming : Env) : Option Env :=
  some incoming
```

Each witness uses concrete `.source` / `.destination` environments and proves
the broken observation differs from the correct one with `by decide`.

- [ ] **Step 6: Run guarded Green checks and commit**

Run the consumer from Step 1 and:

```bash
python3 tools/lean_resource_guard.py \
  --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 \
  --stats /tmp/hoimin-exception-match-proofs.json -- \
  lake -Kjobs=1 build HoiminOracle.ExceptionMatchBindingProofs
```

Expected: both exit 0 with RSS below 768 MiB. On guard exit 124, 125, or 126,
stop that command and diagnose without raising limits. Then commit:

```bash
git add formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingModel.lean \
  formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingProofs.lean \
  formal/HoiminOracle/HoiminOracle.lean
git commit -m "test(lean): model exception and match binding flow"
```

### Task 2: Lean-owned fixed corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingCases.lean`
- Create: `formal/HoiminOracle/ExceptionMatchBindingAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/exception-match-binding-correspondence.jsonl`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 1 transitions and proof-backed expected values.
- Produces: schema-1 JSONL records with `id`, `mode`, `family`, `observation_kind`, `source`, `marker`, `name`, `expected_facts`, `expected_resolution`, `expected_exit_category`, `expected_present`, `operator`, `original`, `replacement`, and `symbol`.

- [ ] **Step 1: Register the missing executable and verify Red**

Add:

```toml
[[lean_exe]]
name = "generate_exception_match_binding"
root = "ExceptionMatchBindingAuditMain"
```

Run a guarded `lake -Kjobs=1 build generate_exception_match_binding` once.
Expected: exit 1 naming the missing root. This is the only native-target build;
after the root exists, use the interpreter path and do not attempt native link.

- [ ] **Step 2: Define a closed, validated case schema**

In `ExceptionMatchBindingCases.lean`, define:

```lean
inductive Family | handler | matchCase
inductive ObservationKind | resolution | annotation | exits | publicCandidate

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  family : Family
  observationKind : ObservationKind
  source : String
  marker : String := ""
  name : String := "Sequence"
  expectedFacts : List String := []
  expectedResolution : Option String := none
  expectedExitCategory : Option String := none
  expectedPresent : Bool := false
  operator : Option String := none
  original : Option String := none
  replacement : Option String := none
  symbol : Option String := none
```

`OracleCase.valid` must enforce schema 1, unique IDs, exactly one marker for
marker-based observations, allowed modes (`internal-fixture`, `strict`,
`model-only`, `infrastructure-error`), sorted fact arrays, valid family/kind
pairs, and mutually exclusive result fields.

- [ ] **Step 3: Add the fixed handler cases**

Create cases for:

- handler type load before target binding;
- body load after target binding;
- cleanup after fallthrough;
- cleanup on return and raise, each as its own terminate fixture;
- cleanup on break and continue inside enclosing loops;
- meet with the non-selected-handler path; and
- preservation of an unrelated direct typing import.

Use `from typing import Sequence` and `except Sequence as Sequence` only where
the fixture is valid Python. Where the exception expression must be a class,
use a separately imported `TypeAlias`/tracked name or a neutral `Error`, while
keeping `Sequence` as the handler target. Each source contains a unique comment
or expression marker that selects the exact observation.

- [ ] **Step 4: Add the fixed match cases**

Create cases for capture visibility in the successful body, partial capture
failure reaching the next case, false-guard propagation, a refutable unmatched
path in the final join, irrefutable exhaustion, and unrelated-name preservation.
Use Ruff-supported patterns such as:

```python
from typing import Sequence
match value:
    case [Sequence, 0]:
        pass
    case _:
        next_case: list[str]
```

and guarded cases that assign or shadow the audited name before returning
false. Keep internal observations separate from public candidate rows built
from the same source premise.

- [ ] **Step 5: Add sensitivity and deterministic CLI output**

Expose `fixedCasesPass`, `sensitivityPasses`, and JSON rendering. In
`ExceptionMatchBindingAuditMain.lean`, support exactly:

```text
--cases
--sensitivity
--stats
--output <path>
--check <path>
```

`--stats` reports fixed counts and the six broken families without generating
deeper programs. `--check` compares bytes with the checked-in corpus.

- [ ] **Step 6: Generate and check the corpus under the guard**

Run these one at a time with the Global Constraints and distinct stats files:

```bash
lake -Kjobs=1 build HoiminOracle.ExceptionMatchBindingCases
lake env lean --run ExceptionMatchBindingAuditMain.lean -- --cases
lake env lean --run ExceptionMatchBindingAuditMain.lean -- --sensitivity
lake env lean --run ExceptionMatchBindingAuditMain.lean -- --stats
lake env lean --run ExceptionMatchBindingAuditMain.lean -- --output corpus/exception-match-binding-correspondence.jsonl
lake env lean --run ExceptionMatchBindingAuditMain.lean -- --check corpus/exception-match-binding-correspondence.jsonl
```

Expected: exit 0 for each, RSS below 768 MiB, deterministic freshness check.
Do not hand-edit the JSONL. Commit:

```bash
git add formal/HoiminOracle/HoiminOracle/ExceptionMatchBindingCases.lean \
  formal/HoiminOracle/ExceptionMatchBindingAuditMain.lean \
  formal/HoiminOracle/corpus/exception-match-binding-correspondence.jsonl \
  formal/HoiminOracle/lakefile.toml
git commit -m "test(lean): generate exception match expectations"
```

### Task 3: Private exact-site Rust correspondence

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: Task 2 corpus, existing `unique_marker_range`, `name_resolution_test_snapshot`, `annotation_site_test_snapshot`, `binding_flow_test_snapshot`, and `ControlFlowExits`.
- Produces: only the smallest missing `#[cfg(test)]` marker projection needed to select an exit category or case-entry state.

- [ ] **Step 1: Add corpus deserialization and failing private correspondence**

In `rust_tests.rs`, define a `#[serde(deny_unknown_fields)]` row matching the
Task 2 schema. Load the corpus with:

```rust
const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../formal/HoiminOracle/corpus/exception-match-binding-correspondence.jsonl"
));
```

Dispatch `internal-fixture` rows to the existing resolution, annotation, or
exit snapshot and compare exact normalized facts/results. Run:

```bash
cargo test -p hoimin-cli --lib exception_match_binding_internal_corpus -- --exact
```

Expected: fail only for observations the existing helpers cannot address or
for a genuine semantic mismatch. Preserve the smallest failing fixture.

- [ ] **Step 2: Add the narrow missing projection**

If required, add:

```rust
#[cfg(test)]
pub(super) fn binding_flow_marker_snapshot(
    source: &str,
    marker: &str,
) -> Result<Vec<String>, String>
```

It must parse once, require a unique marker, run the production collector up to
the AST node containing that marker, and return
`normalize_binding_flow_imports` at that exact entry. Do not clone or reimplement
handler/match transfer logic. If existing helpers cover every row, omit this
function and record that no new projection was necessary.

- [ ] **Step 3: Diagnose every mismatch before production edits**

For each Red case, classify it as model, fixture/parser, adapter, wrong-site
projection, or production behavior. Use a one-case Rust regression and the
corresponding Lean literal expectation. Only for a production mismatch, change
the smallest relevant block in `AnnotationCollector::visit_try`,
`AnnotationCollector::visit_match`, or `NameResolutionBuilder::visit_except_handler`.

- [ ] **Step 4: Add observational sensitivity tests**

Add tests proving that moving the handler-type observation after target binding,
omitting cleanup for one exit category, dropping the failed-pattern state, or
dropping the false-guard state changes the projected result. These tests must
compare observable snapshots, not private model fields alone.

- [ ] **Step 5: Run Green and commit**

Run:

```bash
cargo test -p hoimin-cli --lib exception_match_binding -- --nocapture
cargo test -p hoimin-cli --lib typing_import_rebinding_control_flow -- --nocapture
cargo test -p hoimin-cli --lib typing_import_rebinding_match_propagates_failed_case_bindings -- --exact
```

Expected: all pass. Commit only the actual files changed:

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs \
  crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "test: compare exception match binding sites"
```

### Task 4: Public correspondence and audit report

**Files:**
- Create: `crates/hoimin-cli/tests/lean_exception_match_binding_oracle.rs`
- Create: `docs/superpowers/reports/2026-08-11-lean-exception-match-binding-correspondence-audit.md`

**Interfaces:**
- Consumes: Task 2 strict corpus rows and the built `hoimin` binary.
- Produces: strict schema rejection tests, public manifest comparisons, final claims and resource ledger.

- [ ] **Step 1: Write public schema tests and verify Red**

Copy only the small JSONL parsing/test harness pattern from
`lean_annotation_scope_oracle.rs`. Use `#[serde(deny_unknown_fields)]`, reject
duplicate IDs, unknown mode/family/kind, invalid field combinations, duplicate
markers, malformed manifests, and nonzero CLI exits. Run:

```bash
cargo test -p hoimin-cli --test lean_exception_match_binding_oracle
```

Expected: fail until strict row dispatch and manifest normalization exist.

- [ ] **Step 2: Implement strict public candidate comparison**

For each `strict` row, write its source into a temporary fixture project, run
the existing test harness for `hoimin plan`, retain candidates whose byte span
contains the unique marker, and compare operator, original, replacement,
symbol, and presence. Multiple overlapping candidates are a semantic mismatch;
command, JSON, or marker failures remain infrastructure errors.

- [ ] **Step 3: Run public Green checks**

Run:

```bash
cargo test -p hoimin-cli --test lean_exception_match_binding_oracle
cargo test -p hoimin-cli --test lean_annotation_scope_oracle
cargo test -p hoimin-cli --test lean_binding_flow_oracle
```

Expected: all pass, preserving earlier audit correspondence.

- [ ] **Step 4: Write the audit report from fresh evidence**

The report must state: result, exact audited surface, exclusions, quantified
Lean theorems, fixed-case/internal/public counts, every sensitivity family,
any mismatch and production correction, corpus freshness, and a table of each
guarded Lean command's elapsed time, highest sampled RSS, and exit reason. It
must explicitly say Lean proves the reduced model rather than Rust/Python.

- [ ] **Step 5: Commit public evidence**

```bash
git add crates/hoimin-cli/tests/lean_exception_match_binding_oracle.rs \
  docs/superpowers/reports/2026-08-11-lean-exception-match-binding-correspondence-audit.md
git commit -m "test: audit exception match binding correspondence"
```

### Task 5: Verification, review, PR, and merge

**Files:**
- Modify: only files required by verified review feedback.

**Interfaces:**
- Consumes: Tasks 1-4 and all committed evidence.
- Produces: clean branch, reviewed PR, passing CI, squash merge, and synchronized `main`.

- [ ] **Step 1: Re-run final guarded Lean evidence serially**

Run the proof build, cases build, consumer, `--cases`, `--sensitivity`,
`--stats`, `--output` to a temporary file, and checked-in `--check`, one command
at a time through the exact Global Constraints. Compare the temporary output to
the checked-in corpus with `cmp`. Confirm no Lean/Lake process remains.

- [ ] **Step 2: Run full local verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -j 2 -- -D warnings
cargo test --workspace --all-features -j 2
git diff --check main...HEAD
git status --short
```

Expected: zero failures/warnings, no whitespace errors, and only intentional
tracked changes.

- [ ] **Step 3: Request code and lifecycle review**

Use `superpowers:requesting-code-review`. Review the same-premise mapping,
theorem strength, resource safety, schema rejection, exact marker projection,
and whether any production change is justified. Apply feedback through
`superpowers:receiving-code-review`, rerun affected focused tests, and commit.

- [ ] **Step 4: Push and create the PR**

```bash
git push -u origin audit/lean-exception-match-binding-correspondence
gh pr create --base main \
  --head audit/lean-exception-match-binding-correspondence \
  --title "Audit exception and match binding flow with Lean" \
  --body-file /tmp/hoimin-exception-match-pr.md
```

The body summarizes formal claims, correspondence counts, any production fix,
resource limits, sensitivity, and verification commands.

- [ ] **Step 5: Watch CI and merge**

Use `gh pr checks --watch` or bounded polling that still permits progress
updates. Diagnose any failure before editing. When required checks pass and the
PR is mergeable:

```bash
gh pr merge --squash --delete-branch
```

Then fast-forward the parent `main`, confirm the merge commit, remove this
worktree, and report the PR URL, merge commit, audit outcome, test evidence,
and whether production behavior changed.
