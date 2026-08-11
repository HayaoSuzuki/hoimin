# Lean Annotation-Scope Correspondence Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the previous binding-flow audit's correspondence gap by comparing Lean-owned expectations with exact annotation-site import facts and comprehension-internal name resolution from Hoimin's production analyzers.

**Architecture:** Add a small proof-oriented Lean module for directed scope writes and comprehension entry/exit, a separate fixed-case/corpus executable, and two crate-private Rust projections over `AnnotationCollector` and `NameResolutionIndex`. Keep public CLI correspondence separate, preserve infrastructure-error classification, and change production semantics only after a same-premise failing case exists.

**Tech Stack:** Lean 4 with `Std`, Rust 2024 workspace, Ruff Python AST/parser, Serde JSONL corpus, Cargo tests, Python resource guard.

## Global Constraints

- Run every Lean or Lake command alone with `-Kjobs=1` through `formal/HoiminOracle/tools/lean_resource_guard.py`.
- Use a 20-second deadline, 768 MiB root-plus-descendant RSS limit, and 250 ms sampling.
- Keep structured exploration at depth 2; do not run depth 3 or 4.
- Use `maxHeartbeats 100000` on every non-trivial new theorem; never use unlimited heartbeats.
- Do not add `sorry`, `admit`, custom axioms, or hand-edited generated corpus expectations.
- Use only `strict`, `internal-fixture`, `model-only`, and `infrastructure-error` correspondence modes.
- Treat parse, marker, timeout, RSS, panic, and malformed-corpus failures as infrastructure errors, not semantic mismatches.
- Preserve the public analyzer API; all new state projections are crate-private and `#[cfg(test)]`.
- Run Rust workspace tests with at most two build jobs on the local machine.

---

## File Map

- Create `formal/HoiminOracle/HoiminOracle/AnnotationScopeModel.lean`: directed-scope and comprehension semantics only.
- Create `formal/HoiminOracle/HoiminOracle/AnnotationScopeProofs.lean`: unbounded model theorems and fixed broken witnesses.
- Create `formal/HoiminOracle/HoiminOracle/AnnotationScopeCases.lean`: closed case set, schema values, sensitivity verdicts, and JSONL rendering.
- Create `formal/HoiminOracle/AnnotationScopeAuditMain.lean`: corpus generation, freshness, cases, sensitivity, and stats CLI.
- Create `formal/HoiminOracle/corpus/annotation-scope-correspondence.jsonl`: deterministic Lean-generated expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import only model and proof modules; do not import expensive cases.
- Modify `formal/HoiminOracle/lakefile.toml`: register the non-library `generate_annotation_scope` executable.
- Modify `crates/hoimin-cli/src/analyzer/rust.rs`: capture scope on `AnnotationSite` and add test-only marker projections.
- Modify `crates/hoimin-cli/src/analyzer/rust_tests.rs`: focused Red/Green projection and private correspondence tests.
- Create `crates/hoimin-cli/tests/lean_annotation_scope_oracle.rs`: corpus schema and public CLI correspondence tests.
- Create `docs/superpowers/reports/2026-08-11-lean-annotation-scope-correspondence-audit.md`: final evidence and resource ledger.

### Task 1: Formal directed-scope and comprehension model

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/AnnotationScopeModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/AnnotationScopeProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Consumes: `HoiminOracle.BindingFlow.{Name, Target, Fact, Env, ScopeKind}`.
- Produces: `DirectedState`, `writeDirected`, `ComprehensionObservation`, `observeComprehension`, `Resolution`, and preservation theorems imported by cases.

- [ ] **Step 1: Add a failing proof consumer**

Create `/tmp/hoimin-annotation-scope-proof-consumer.lean` with:

```lean
import HoiminOracle.AnnotationScopeProofs

open HoiminOracle BindingFlow AnnotationScope

example (outer : Env) (target : Name) :
    (observeComprehension outer target).after = outer :=
  comprehension_preserves_outer outer target
```

Run from `formal/HoiminOracle`:

```bash
python3 tools/lean_resource_guard.py \
  --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 \
  --stats /tmp/hoimin-annotation-scope-red.json -- \
  lake env lean /tmp/hoimin-annotation-scope-proof-consumer.lean
```

Expected: exit 1 because `HoiminOracle.AnnotationScopeProofs` does not exist.

- [ ] **Step 2: Implement the minimal model**

Create `AnnotationScopeModel.lean` with these public shapes:

```lean
import HoiminOracle.BindingFlowModel

namespace HoiminOracle.AnnotationScope
open BindingFlow

inductive Resolution
  | definitelyBuiltin
  | shadowed
  | unknown
  deriving Repr, DecidableEq, BEq

structure DirectedState where
  moduleEnv : Env
  nearestFunctionEnv : Env
  currentEnv : Env
  deriving Repr, DecidableEq, BEq

def writeDirected (directive : Directive) (name : Name) (fact : Fact)
    (state : DirectedState) : DirectedState :=
  match directive with
  | .global => { state with moduleEnv := state.moduleEnv.set name fact }
  | .nonlocal =>
      { state with nearestFunctionEnv := state.nearestFunctionEnv.set name fact }
  | .normal => { state with currentEnv := state.currentEnv.set name fact }

def factResolution : Fact → Resolution
  | .known .builtin | .absent => .definitelyBuiltin
  | .shadowed | .known .typing => .shadowed
  | .unknown => .unknown

structure ComprehensionObservation where
  firstIterable : Resolution
  inside : Resolution
  after : Env
  deriving Repr, DecidableEq, BEq

def observeComprehension (outer : Env) (target : Name) :
    ComprehensionObservation where
  firstIterable := factResolution (outer.get target)
  inside := .shadowed
  after := outer

end HoiminOracle.AnnotationScope
```

Keep the model independent of strings, parser offsets, and JSON serialization.

- [ ] **Step 3: Prove target ownership and comprehension invariants**

Create `AnnotationScopeProofs.lean`. Include exact theorems for:

```lean
theorem comprehension_preserves_outer (outer : Env) (target : Name) :
    (observeComprehension outer target).after = outer := by rfl

theorem comprehension_target_is_shadowed (outer : Env) (target : Name) :
    (observeComprehension outer target).inside = .shadowed := by rfl

theorem global_write_preserves_nearest_function
    (state : DirectedState) (name : Name) (fact : Fact) :
    (writeDirected .global name fact state).nearestFunctionEnv =
      state.nearestFunctionEnv := by rfl

theorem global_write_updates_module
    (state : DirectedState) (name : Name) (fact : Fact) :
    (writeDirected .global name fact state).moduleEnv.get name = fact := by
  cases name <;> rfl

theorem nonlocal_write_preserves_module
    (state : DirectedState) (name : Name) (fact : Fact) :
    (writeDirected .nonlocal name fact state).moduleEnv = state.moduleEnv := by rfl

theorem nonlocal_write_updates_nearest_function
    (state : DirectedState) (name : Name) (fact : Fact) :
    (writeDirected .nonlocal name fact state).nearestFunctionEnv.get name = fact := by
  cases name <;> rfl

theorem normal_write_updates_only_current
    (state : DirectedState) (name : Name) (fact : Fact) :
    (writeDirected .normal name fact state).currentEnv.get name = fact := by
  cases name <;> rfl

theorem global_reimport_restores_typing (state : DirectedState) (name : Name) :
    (writeDirected .global name (.known .typing)
      (writeDirected .global name .shadowed state)).moduleEnv.get name =
        .known .typing := by
  cases name <;> rfl

theorem nonlocal_reimport_restores_typing (state : DirectedState) (name : Name) :
    (writeDirected .nonlocal name (.known .typing)
      (writeDirected .nonlocal name .shadowed state)).nearestFunctionEnv.get name =
        .known .typing := by
  cases name <;> rfl

theorem comprehension_preserves_name
    (outer : Env) (target observed : Name) :
    (observeComprehension outer target).after.get observed = outer.get observed := by
  rfl
```

Each write theorem must quantify over `state`, `name`, and `fact`, and compare
the exact `Env.get` field. Put `set_option maxHeartbeats 100000 in` around any
theorem not discharged by `rfl`/`simp` immediately.

- [ ] **Step 4: Add literal broken witnesses**

Define these non-exported broken functions in the proof module:

```lean
private def brokenComprehensionLeak (outer : Env) (target : Name) :
    ComprehensionObservation :=
  { observeComprehension outer target with after := outer.set target .shadowed }

private def brokenFirstIterableBinding (outer : Env) (target : Name) :
    ComprehensionObservation :=
  { observeComprehension outer target with firstIterable := .shadowed }

private def brokenGlobalCurrent (name : Name) (fact : Fact)
    (state : DirectedState) : DirectedState :=
  writeDirected .normal name fact state

private def brokenNonlocalModule (name : Name) (fact : Fact)
    (state : DirectedState) : DirectedState :=
  writeDirected .global name fact state
```

Add four literal `example` witnesses, each ending in `:= by decide`, showing the
broken result differs from the correct result for `target := .source`, plus a
fifth literal pair showing an annotation's entry environment differs from its
post-suite environment after `.destination` is shadowed.

- [ ] **Step 5: Run the guarded proof consumer and focused build**

Run the consumer command from Step 1, then:

```bash
python3 tools/lean_resource_guard.py \
  --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 \
  --stats /tmp/hoimin-annotation-scope-model.json -- \
  lake -Kjobs=1 build HoiminOracle.AnnotationScopeProofs
```

Expected: both exit 0; stats reason is `child_exit`; sampled RSS remains below
768 MiB. Stop without raising limits on exit 124, 125, or 126.

- [ ] **Step 6: Commit the formal model**

```bash
git add formal/HoiminOracle/HoiminOracle/AnnotationScopeModel.lean \
  formal/HoiminOracle/HoiminOracle/AnnotationScopeProofs.lean \
  formal/HoiminOracle/HoiminOracle.lean
git commit -m "test(lean): model annotation scope correspondence"
```

### Task 2: Lean-owned fixed cases and corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/AnnotationScopeCases.lean`
- Create: `formal/HoiminOracle/AnnotationScopeAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/annotation-scope-correspondence.jsonl`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 1 model and proof functions.
- Produces: schema-1 JSONL fields `id`, `mode`, `scenario`, `observation_kind`, `source`, `marker`, `expected_facts`, `expected_symbol`, `expected_scope`, `expected_resolution`, and `expected_present`.

- [ ] **Step 1: Register a missing executable and observe Red**

Add this target to `lakefile.toml` before its root exists:

```toml
[[lean_exe]]
name = "generate_annotation_scope"
root = "AnnotationScopeAuditMain"
```

Run it through the guard with `lake -Kjobs=1 build generate_annotation_scope`.
Expected: exit 1 naming the missing root module.

- [ ] **Step 2: Define a closed typed case schema**

In `AnnotationScopeCases.lean`, define:

```lean
inductive ObservationKind | annotation | resolution | publicCandidate
inductive Scenario
  | globalBefore | globalAfterWrite | globalRestored
  | nonlocalBefore | nonlocalAfterWrite | nonlocalRestored
  | classGlobal | classNonlocal
  | listFirstIterable | listBody | setBody | dictBody | generatorBody

structure Case where
  id : String
  mode : String
  scenario : Scenario
  observationKind : ObservationKind
  source : String
  marker : String
  expectedFacts : List String := []
  expectedSymbol : Option String := none
  expectedScope : Option String := none
  expectedResolution : Option String := none
  expectedOperator : Option String := none
  expectedOriginal : Option String := none
  expectedReplacement : Option String := none
  expectedPresent : Bool
```

Define one private case per observation scenario. For the nine same-premise
public observations, add a separate `publicCandidate` case so each record still
has exactly one mode. Use unique markers around the observed annotation or
identifier so source matching cannot select a different occurrence. All private
projection cases use `internal-fixture`; public candidate cases use `strict`
only when the same marker is observable in the manifest. The closed total is 29
cases: 20 private and 9 public. Four private cases observe the scope unaffected
by directed function/class writes. The post-comprehension private/public pair must
make the comprehension-leak sensitivity observable at the corpus boundary.

- [ ] **Step 3: Make the fixed cases self-validating**

Implement `Case.valid`, `fixedCasesPass`, and `sensitivityPasses`. Validation
must require the exact four modes, the scenario/observation-kind pairing, one
marker occurrence, sorted fact strings, and mutually exclusive annotation vs
resolution fields. Sensitivity covers all five design families, including a
site-entry vs suite-exit witness represented as two unequal literal states.

- [ ] **Step 4: Implement deterministic JSONL generation**

Create `AnnotationScopeAuditMain.lean` following `BindingFlowAuditMain.lean`.
Support exactly:

```text
--output <path>
--check <path>
--cases
--sensitivity
--stats
```

`--stats` prints fixed case count and sensitivity count; it does not enumerate
new structured programs. Serialization escapes strings through the existing
JSON helpers and emits one stable line per case.

- [ ] **Step 5: Build and generate through the resource guard**

Attempt the native target once through the guard. If linking reaches exit 125,
record that run and do not retry or raise the limit. Use the same Lean main via
`lake env lean --run AnnotationScopeAuditMain.lean --` for all operations. Run
one guarded command at a time:

```bash
lake -Kjobs=1 build HoiminOracle.AnnotationScopeCases
lake env lean --run AnnotationScopeAuditMain.lean -- --cases
lake env lean --run AnnotationScopeAuditMain.lean -- --sensitivity
lake env lean --run AnnotationScopeAuditMain.lean -- --stats
lake env lean --run AnnotationScopeAuditMain.lean -- --output corpus/annotation-scope-correspondence.jsonl
lake env lean --run AnnotationScopeAuditMain.lean -- --check corpus/annotation-scope-correspondence.jsonl
```

Wrap each command with the exact guard flags from Global Constraints and a
distinct `/tmp/hoimin-annotation-scope-*.json` stats file. Do not hand-edit the
generated corpus.

- [ ] **Step 6: Commit the corpus boundary**

```bash
git add formal/HoiminOracle/HoiminOracle/AnnotationScopeCases.lean \
  formal/HoiminOracle/AnnotationScopeAuditMain.lean \
  formal/HoiminOracle/corpus/annotation-scope-correspondence.jsonl \
  formal/HoiminOracle/lakefile.toml
git commit -m "test(lean): generate annotation scope expectations"
```

### Task 3: Marker-addressed Rust projections

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: production parser, `AnnotationCollector`, and `NameResolutionIndex`.
- Produces: `AnnotationSiteTestSnapshot`, `NameResolutionTestSnapshot`, `annotation_site_test_snapshot`, and `name_resolution_test_snapshot`, all `pub(super)` under `#[cfg(test)]`.

- [ ] **Step 1: Write failing fixture tests**

In `rust_tests.rs`, import the new corpus with `include_str!`. Add one focused
test that calls the not-yet-existing helpers and checks:

```rust
assert_eq!(site.scope, "function");
assert_eq!(site.symbol.as_deref(), Some("outer.inner"));
assert_eq!(site.facts, vec!["direct:Sequence=typing.Sequence"]);
assert_eq!(first_iterable.resolution, "definitely-builtin");
assert_eq!(body.resolution, "shadowed");
```

Run:

```bash
cargo test -p hoimin-cli --lib annotation_scope_marker_projections --no-fail-fast
```

Expected: compile failure because the helpers do not exist.

- [ ] **Step 2: Capture annotation scope at record time**

Add `scope_kind: ScopeKind` to private `AnnotationSite` and initialize it in
`AnnotationCollector::record`. Derive `Clone, Copy, Debug, Eq, PartialEq` on
`ScopeKind`. Do not change candidate generation reads or public structures.

- [ ] **Step 3: Implement the annotation-site projection**

Add test-only shapes:

```rust
#[cfg(test)]
pub(super) struct AnnotationSiteTestSnapshot {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) symbol: Option<String>,
    pub(super) scope: &'static str,
    pub(super) facts: Vec<String>,
}

#[cfg(test)]
pub(super) fn annotation_site_test_snapshot(
    source: &str,
    marker: &str,
) -> Result<AnnotationSiteTestSnapshot, String>;
```

Resolve the marker with `match_indices`, require exactly one marker and exactly
one containing annotation range, and return an `infrastructure-error:`-prefixed message for
parse, zero-match, duplicate-marker, or duplicate-site failures. Reuse
`normalize_binding_flow_imports`; do not duplicate normalization.

- [ ] **Step 4: Implement the name-resolution projection**

Add:

```rust
#[cfg(test)]
pub(super) struct NameResolutionTestSnapshot {
    pub(super) start: usize,
    pub(super) resolution: &'static str,
}

#[cfg(test)]
pub(super) fn name_resolution_test_snapshot(
    source: &str,
    marker: &str,
    name: &str,
) -> Result<NameResolutionTestSnapshot, String>;
```

Require the marker to contain exactly one identifier spelling equal to `name`,
build `NameResolutionIndex::from_module`, and call `resolution(start, name)`.
Map only to `definitely-builtin`, `shadowed`, or `unknown`.

- [ ] **Step 5: Verify setup errors stay infrastructural**

Add tests for malformed source, missing marker, duplicate marker, wrong name,
and a marker that contains two annotations/identifiers. Assert each helper
returns `Err` beginning with `infrastructure-error:`.

- [ ] **Step 6: Run focused analyzer tests and commit**

```bash
cargo test -p hoimin-cli --lib annotation_scope --no-fail-fast
cargo test -p hoimin-cli --lib typing_import_rebinding_scope --no-fail-fast
cargo test -p hoimin-cli --lib comprehension_exception_target_and_wildcard_boundaries_are_conservative --no-fail-fast
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "test(analyzer): project annotation scope facts"
```

### Task 4: Corpus adapters and mismatch decision

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Create: `crates/hoimin-cli/tests/lean_annotation_scope_oracle.rs`
- Modify only on confirmed mismatch: `crates/hoimin-cli/src/analyzer/rust.rs`

**Interfaces:**
- Consumes: Task 2 corpus and Task 3 projections.
- Produces: closed-schema validation, private same-premise comparisons, and public manifest comparisons.

- [ ] **Step 1: Write a failing corpus schema test**

In the integration test, deserialize with `#[serde(deny_unknown_fields)]`,
reject duplicate IDs, unknown modes, unknown scenarios, wrong observation-kind
fields, non-unique markers, and unsorted facts. Initially assert the exact case
count from Task 2 and run before wiring all validators.

Expected: Red because validation is incomplete or the adapter is absent.

- [ ] **Step 2: Add the private correspondence loop**

In `rust_tests.rs`, deserialize the same corpus. For each `internal-fixture`
case, dispatch solely by `observation_kind`:

- `annotation`: compare exact range containment, facts, symbol, and scope;
- `resolution`: compare exact marker start and resolution.

Any helper `Err` must panic with its `infrastructure-error` message before an
`assert_eq!` semantic comparison.

- [ ] **Step 3: Add public strict correspondence**

In `lean_annotation_scope_oracle.rs`, reuse the public `hoimin_cli::run_with_io`
pattern from `lean_binding_flow_oracle.rs`. For each `strict` case, invoke
`hoimin plan` with one operator and compare candidate presence, operator,
original, replacement, symbol, and marker-contained span. Use a 10-second
per-case timeout and reject any stderr or nonzero exit as infrastructure error.

- [ ] **Step 4: Classify every mismatch before changing semantics**

Run:

```bash
cargo test -p hoimin-cli --lib annotation_scope_private_correspondence -- --nocapture
cargo test -p hoimin-cli --test lean_annotation_scope_oracle -- --nocapture
```

If all cases match, make no production semantic change. If a case mismatches,
record its minimal source, expected/actual fields, and mode; retain it as a
fixed failing case; then apply only the smallest implementation repair required
for that same premise. Do not weaken Lean or change the expected corpus to make
current Rust pass.

- [ ] **Step 5: Verify sensitivity of the Rust correspondence**

Add test-only broken projections for site-exit substitution or bypass the
correct comprehension scope in the test module, and assert the fixed corpus
detects the wrong result. Keep broken behavior out of production builds.

- [ ] **Step 6: Commit the correspondence layer**

```bash
git add crates/hoimin-cli/src/analyzer/rust_tests.rs \
  crates/hoimin-cli/tests/lean_annotation_scope_oracle.rs \
  crates/hoimin-cli/src/analyzer/rust.rs
git commit -m "test: check annotation scope against Lean corpus"
```

### Task 5: Audit report and complete verification

**Files:**
- Create: `docs/superpowers/reports/2026-08-11-lean-annotation-scope-correspondence-audit.md`

**Interfaces:**
- Consumes: fresh command output and resource stats only.
- Produces: a self-contained report separating model proof, bounded/fixed evaluation, implementation observation, and remaining limitations.

- [ ] **Step 1: Run fresh guarded Lean verification**

Run separately through the guard:

```bash
lake env lean /tmp/hoimin-annotation-scope-proof-consumer.lean
lake -Kjobs=1 build HoiminOracle.AnnotationScopeProofs generate_annotation_scope
.lake/build/bin/generate_annotation_scope --stats
.lake/build/bin/generate_annotation_scope --sensitivity
.lake/build/bin/generate_annotation_scope --cases
.lake/build/bin/generate_annotation_scope --check corpus/annotation-scope-correspondence.jsonl
```

Retain every stats JSON. Stop on 124/125/126 and report infrastructure failure;
do not increase any limit.

- [ ] **Step 2: Run fresh Rust and formatting verification**

```bash
cargo test -p hoimin-cli --test lean_annotation_scope_oracle
cargo test --workspace --all-features -j 2
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
uvx ruff check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
uvx ruff format --check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
python3 -m unittest tests.test_lean_resource_guard -v
git diff --check
```

- [ ] **Step 3: Write the audit report**

Include the durable claim and exclusions, correspondence worksheet, every fixed
case and mode, theorem premises, all five new and five retained sensitivity
families, exact command lines, elapsed/RSS ledger, mismatch classifications,
owner decisions, and next independent audit target. State explicitly that Lean
proved the model rather than production Rust.

- [ ] **Step 4: Commit the report**

```bash
git add docs/superpowers/reports/2026-08-11-lean-annotation-scope-correspondence-audit.md
git commit -m "docs: report annotation scope Lean audit"
```

- [ ] **Step 5: Review and integration gate**

Use `superpowers:requesting-code-review`, resolve all findings with
`superpowers:receiving-code-review`, rerun affected checks, then use
`superpowers:verification-before-completion`. Confirm a clean worktree before
push/PR. Create a PR, wait for every required CI job, squash merge only when the
merge state is clean, and remove only this audit's branch/worktree afterward.
