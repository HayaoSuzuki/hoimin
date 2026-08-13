# Lean Nested Try/Finally Flow Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the reachable categorized-exit contract for nested `try`/handler/`finally` composition in Lean, compare a Lean-owned corpus with the Rust analyzer, and retain any same-premise counterexample as a Rust regression before a minimal production fix.

**Architecture:** Extend the existing `HoiminOracle.BindingFlow` semantics with focused helpers for category projection, cleanup, and try composition; put general theorems in an imported proof module and fixed sensitivity cases in a cheap imported case module. A non-imported Lean executable owns JSONL generation and resource statistics. One focused Rust `#[cfg(test)]` adapter parses that corpus, compares public rows through `hoimin_cli::run_with_io`, and compares internal rows through a production-backed post-`finally` exit projection.

**Tech Stack:** Lean 4 with Lake and `Std`; Rust 2024 workspace with Ruff AST, Tokio, Serde, and Cargo tests; JSON Lines; the repository's Lean RSS/time guard.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-cost-effective-audit` on branch `lean-cost-effective-audit`.
- Every corpus row has exactly one mode from `strict`, `model-only`, `internal-fixture`, or `infrastructure-error`.
- Promote a row to `strict` only when the public CLI configures the same premise and exposes the complete observation.
- Lean establishes properties of the reduced model only; Rust correspondence remains a separate executable observation.
- Keep theorem/model modules cheap to import. Corpus rendering, freshness checking, statistics, and sensitivity execution stay in `NestedTryFlowAuditMain.lean`.
- Run one Lean/Lake command at a time through `tools/lean_resource_guard.py` with a 20-second deadline, 768 MiB root-plus-descendant RSS cap, 250 ms sampling, and `-Kjobs=1` for Lake builds.
- Use `set_option maxHeartbeats 100000 in` on non-trivial declarations; never use `maxHeartbeats 0`.
- Do not retry a timeout, RSS stop, monitor failure, or aggregate build with larger limits. Split the declaration or reduce computation instead.
- Do not add generated trace-depth exploration. The retained domain is fixed theorem premises plus literal sensitivity witnesses.
- Do not hand-edit `formal/HoiminOracle/corpus/nested-try-flow.jsonl`; generate it from Lean.
- Do not change production behavior without a focused same-premise Rust test that fails for the semantic mismatch.
- Keep nested `match`, multiple concrete-handler selection, compound patterns, and `except*` outside this plan.
- Add only explicitly listed files to commits; `.venv` is a local worktree symlink and must remain uncommitted.

---

## File Map

- Create `formal/HoiminOracle/HoiminOracle/NestedTryFlowModel.lean`: category observations, cleanup mapping, and compositional try/finally semantics over the existing binding-flow lattice.
- Create `formal/HoiminOracle/HoiminOracle/NestedTryFlowProofs.lean`: reachability, category routing, cleanup-once, and meet-soundness theorems.
- Create `formal/HoiminOracle/HoiminOracle/NestedTryFlowCases.lean`: fixed cases and deliberately broken semantic variants.
- Create `formal/HoiminOracle/NestedTryFlowAuditMain.lean`: deterministic JSONL rendering, corpus checks, sensitivity output, and fixed-domain statistics.
- Create `formal/HoiminOracle/corpus/nested-try-flow.jsonl`: Lean-generated expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import the three cheap modules.
- Modify `formal/HoiminOracle/lakefile.toml`: register `generate_nested_try_flow` without importing the executable from the library.
- Create `crates/hoimin-cli/src/analyzer/nested_try_oracle_tests.rs`: closed corpus parser, strict public adapter, internal production-backed adapter, and single-case filtering.
- Create `crates/hoimin-cli/tests/lean_nested_try_flow_oracle.rs`: strict public CLI correspondence in the normal integration-test crate context.
- Modify `crates/hoimin-cli/src/analyzer/rust.rs`: test-only post-try categorized-exit projection; production transfer behavior changes only if a confirmed mismatch requires it.
- Create `docs/superpowers/reports/2026-08-13-lean-nested-try-finally-flow-audit.md`: self-contained result, correspondence ledger, counterexamples, commands, and resource measurements.

### Task 1: Lean compositional model and unbounded theorems

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/NestedTryFlowModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/NestedTryFlowProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-nested-try-proof-consumer.lean`

**Interfaces:**
- Consumes: `HoiminOracle.BindingFlow.{Env, Name, Fact, ExitCategory, Exits, meetAll?, routeCategory, routeFinally}`.
- Produces: `Exits.categoryStates`, `mapExits`, `cleanupName`, `cleanupExits`, `composeTry`, `allReachableStates`, and the theorem names listed below.

- [x] **Step 1: Write the failing theorem consumer before the model exists**

Create `/tmp/hoimin-nested-try-proof-consumer.lean` with:

```lean
import HoiminOracle.NestedTryFlowProofs

open HoiminOracle.BindingFlow
open HoiminOracle.NestedTryFlow

#check sequential_abrupt_excludes_next
#check singleton_finally_routes_exactly
#check falling_finally_preserves_category
#check abrupt_finally_replaces_category
#check cleanup_exits_idempotent
#check reachable_meet_retains_only_common_knowledge
```

- [x] **Step 2: Run the consumer and observe RED**

Run from `formal/HoiminOracle`:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-proof-red.json -- lake env lean /tmp/hoimin-nested-try-proof-consumer.lean
```

Expected: nonzero child exit because `HoiminOracle.NestedTryFlowProofs` does not exist. A guard timeout/RSS/monitor result is infrastructure failure, not the expected RED.

- [x] **Step 3: Add the smallest model API**

Create `NestedTryFlowModel.lean` with these definitions, preserving the exact signatures:

```lean
import HoiminOracle.BindingFlowModel

namespace HoiminOracle.NestedTryFlow

open HoiminOracle.BindingFlow

def Exits.categoryStates (exits : Exits) : ExitCategory → List Env
  | .fallthrough => exits.fallthrough.toList
  | .break => exits.breaks
  | .continue => exits.continues
  | .terminate => exits.terminates

def mapExits (transfer : Env → Env) (exits : Exits) : Exits where
  fallthrough := exits.fallthrough.map transfer
  breaks := exits.breaks.map transfer
  continues := exits.continues.map transfer
  terminates := exits.terminates.map transfer

def cleanupName (name : Name) (environment : Env) : Env :=
  environment.set name .absent

def cleanupExits (name : Name) (exits : Exits) : Exits :=
  mapExits (cleanupName name) exits

def composeTry
    (body handler : Exits)
    (handlerTarget : Option Name)
    (orelse finalizer : Env → Exits) : Exits :=
  let normal := body.andThen orelse
  let cleanedHandler := match handlerTarget with
    | none => handler
    | some name => cleanupExits name handler
  routeFinally (normal.merge cleanedHandler) finalizer

def allReachableStates (exits : Exits) : List Env :=
  exits.states

end HoiminOracle.NestedTryFlow
```

- [x] **Step 4: Add the proof module with explicit premises**

Create `NestedTryFlowProofs.lean`. Prove these contracts without axioms, `sorry`, or unbounded heartbeats:

```lean
import HoiminOracle.NestedTryFlowModel
import HoiminOracle.BindingFlowProofs

namespace HoiminOracle.NestedTryFlow

open HoiminOracle.BindingFlow

set_option maxHeartbeats 100000 in
theorem sequential_abrupt_excludes_next
    (first : Exits)
    (next : Env → Exits)
    (unreachable : first.fallthrough = none) :
    first.andThen next = first.withoutFallthrough := by
  simp [Exits.andThen, unreachable]

set_option maxHeartbeats 100000 in
theorem singleton_finally_routes_exactly
    (category : ExitCategory)
    (environment : Env)
    (finalizer : Env → Exits) :
    routeFinally (Exits.categoryOnly category environment) finalizer =
      routeCategory category (finalizer environment) := by
  cases category <;> simp [routeFinally, routeMany, Exits.categoryOnly, Exits.merge,
    Exits.empty, meetOption]

set_option maxHeartbeats 100000 in
theorem falling_finally_preserves_category
    (category : ExitCategory)
    (environment after : Env)
    (finalizer : Env → Exits)
    (falls : finalizer environment = Exits.fallthroughOnly after) :
    routeFinally (Exits.categoryOnly category environment) finalizer =
      Exits.categoryOnly category after := by
  rw [singleton_finally_routes_exactly, falls]
  exact HoiminOracle.BindingFlow.fallthrough_finally_preserves_category category after

set_option maxHeartbeats 100000 in
theorem abrupt_finally_replaces_category
    (incoming outgoing : ExitCategory)
    (environment after : Env)
    (finalizer : Env → Exits)
    (abrupt : finalizer environment = Exits.categoryOnly outgoing after)
    (notFalls : outgoing ≠ .fallthrough) :
    routeFinally (Exits.categoryOnly incoming environment) finalizer =
      Exits.categoryOnly outgoing after := by
  rw [singleton_finally_routes_exactly, abrupt]
  cases outgoing <;> simp_all [routeCategory, Exits.categoryOnly, Exits.withoutFallthrough]

set_option maxHeartbeats 100000 in
theorem cleanup_exits_idempotent (name : Name) (exits : Exits) :
    cleanupExits name (cleanupExits name exits) = cleanupExits name exits := by
  cases name <;> cases exits <;> simp [cleanupExits, mapExits, cleanupName, Env.set]

set_option maxHeartbeats 100000 in
theorem reachable_meet_retains_only_common_knowledge
    (exits : Exits)
    (joined : Env)
    (name : Name)
    (target : Target)
    (joinedPaths : meetAll? (allReachableStates exits) = some joined)
    (retained : joined.get name = .known target) :
    ∀ environment ∈ allReachableStates exits,
      environment.get name = .known target := by
  exact HoiminOracle.BindingFlow.meetAll_retained_on_every_path
    (allReachableStates exits) joined name target joinedPaths retained

end HoiminOracle.NestedTryFlow
```

If a displayed `simp` proof needs decomposition, add named private lemmas for `routeMany` and `Exits.merge`; do not weaken any theorem statement.

- [x] **Step 5: Import only cheap modules from the Lean library root**

Append to `HoiminOracle.lean`:

```lean
import HoiminOracle.NestedTryFlowModel
import HoiminOracle.NestedTryFlowProofs
```

Do not import `NestedTryFlowAuditMain`.

- [x] **Step 6: Run focused GREEN verification serially**

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-model-build.json -- lake -Kjobs=1 build HoiminOracle.NestedTryFlowModel
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-proofs-build.json -- lake -Kjobs=1 build HoiminOracle.NestedTryFlowProofs
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-consumer.json -- lake env lean /tmp/hoimin-nested-try-proof-consumer.lean
```

Expected: three child exits 0. Record each stats JSON for the final report.

- [x] **Step 7: Commit the model and proofs**

```bash
git add formal/HoiminOracle/HoiminOracle/NestedTryFlowModel.lean formal/HoiminOracle/HoiminOracle/NestedTryFlowProofs.lean formal/HoiminOracle/HoiminOracle.lean
git commit -m "test(lean): prove nested try exit routing"
```

### Task 2: Fixed witnesses, executable, and Lean-owned corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/NestedTryFlowCases.lean`
- Create: `formal/HoiminOracle/NestedTryFlowAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/nested-try-flow.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: `composeTry`, `cleanupExits`, `routeFinally`, and the existing `BindingFlow.Env` fact lattice.
- Produces: `nestedTryFlowCases : List OracleCase`, `fixedCasesPass`, `sensitivityPasses`, and executable modes `--cases`, `--sensitivity`, `--stats`, `--output PATH`, and `--check PATH`.

- [x] **Step 1: Add a failing executable invocation**

Run:

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-exe-red.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --sensitivity
```

Expected: nonzero child exit because the executable does not exist.

- [x] **Step 2: Define the fixed corpus schema and cases**

Create `NestedTryFlowCases.lean` with these public types:

```lean
import HoiminOracle.NestedTryFlowProofs

namespace HoiminOracle.NestedTryFlow

open HoiminOracle.BindingFlow

structure ExpectedExits where
  fallthrough : List (List String) := []
  breaks : List (List String) := []
  continues : List (List String) := []
  terminates : List (List String) := []
  deriving Repr, DecidableEq, BEq

inductive CandidateExpectation
  | notObserved
  | present
  | absent
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  family : String
  source : String
  marker : String
  entryCategory : ExitCategory
  expected : ExpectedExits
  candidate : CandidateExpectation := .notObserved
  deriving Repr, DecidableEq, BEq
```

Define exactly these ten closed identities, with literal Python source and normalized facts derived by evaluating the correct Lean model:

```text
finally_annotation_meets_normal_and_raise          strict
post_finally_uses_only_fallthrough                 strict
falling_finally_preserves_break                    internal-fixture
falling_finally_preserves_continue                 internal-fixture
falling_finally_preserves_return_terminate         internal-fixture
falling_finally_preserves_raise_terminate          internal-fixture
abrupt_finally_replaces_fallthrough                internal-fixture
abrupt_finally_replaces_break                      internal-fixture
unreachable_post_return_excluded                   internal-fixture
nonselected_handler_meet                           model-only
```

The strict source for `finally_annotation_meets_normal_and_raise` must place the unique marker on `value: Sequence[int]` inside `finally`, with one reachable path shadowing `Sequence` and raising and one normal path retaining it; expected candidate is `absent`. The strict source for `post_finally_uses_only_fallthrough` must put the unique marker after the `try/finally`; the raising shadowed path cannot reach it, and expected candidate is `present`.

- [x] **Step 3: Add literal broken variants and fixed witnesses**

Implement these Boolean checks in `NestedTryFlowCases.lean`:

```lean
def cleanupBoundarySensitivity : Bool
def finalizerCoverageSensitivity : Bool
def categoryPreservationSensitivity : Bool
def abruptReplacementSensitivity : Bool
def unreachableJoinSensitivity : Bool
def omittedAbruptSensitivity : Bool
def cleanupIdempotencySensitivity : Bool

def sensitivityPasses : Bool :=
  cleanupBoundarySensitivity && finalizerCoverageSensitivity &&
  categoryPreservationSensitivity && abruptReplacementSensitivity &&
  unreachableJoinSensitivity && omittedAbruptSensitivity &&
  cleanupIdempotencySensitivity

def fixedCasesPass : Bool :=
  nestedTryFlowCases.all fun item =>
    item.schema == 1 &&
    (item.mode == "strict" || item.mode == "internal-fixture" ||
      item.mode == "model-only") &&
    !item.id.isEmpty && !item.family.isEmpty && !item.source.isEmpty &&
    !item.marker.isEmpty

example : sensitivityPasses = true := by native_decide
example : fixedCasesPass = true := by native_decide
```

Each sensitivity must compare the correct result with a separately named broken definition. Fixed witnesses stay present even if later case pruning occurs.

- [x] **Step 4: Add the non-imported executable and Lake target**

Create `NestedTryFlowAuditMain.lean` following the repository executable pattern. `renderCorpus` must serialize every field with `Lean.Json.mkObj`, normalize all fact/state arrays deterministically, append exactly one newline per row, and never parse Rust output. `--stats` prints:

```text
schema=1
generated_depth=0
event_alphabet=0
fixed_cases=10
strict_cases=2
internal_fixture_cases=7
model_only_cases=1
sensitivity_families=7
```

Add to `lakefile.toml`:

```toml
[[lean_exe]]
name = "generate_nested_try_flow"
root = "NestedTryFlowAuditMain"
```

Append only the cheap case import to `HoiminOracle.lean`:

```lean
import HoiminOracle.NestedTryFlowCases
```

- [x] **Step 5: Build and run sensitivity before generating the corpus**

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-cases-target.json -- lake -Kjobs=1 build HoiminOracle.NestedTryFlowCases
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-sensitivity.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-cases.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --cases
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-stats.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --stats
```

Expected: all child exits 0 and all seven sensitivity lines report `true`.

- [x] **Step 6: Generate and freshness-check the corpus**

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-output.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --output corpus/nested-try-flow.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-try-fresh.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --check corpus/nested-try-flow.jsonl
```

Expected: generated file exists, contains ten newline-terminated JSON objects, and freshness exits 0.

- [x] **Step 7: Commit fixed cases and generated corpus**

```bash
git add formal/HoiminOracle/HoiminOracle/NestedTryFlowCases.lean formal/HoiminOracle/NestedTryFlowAuditMain.lean formal/HoiminOracle/corpus/nested-try-flow.jsonl formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/lakefile.toml
git commit -m "test(lean): generate nested try flow oracle"
```

### Task 3: Rust corpus contract and production-backed correspondence

**Files:**
- Create: `crates/hoimin-cli/src/analyzer/nested_try_oracle_tests.rs`
- Create: `crates/hoimin-cli/tests/lean_nested_try_flow_oracle.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests -- --nocapture`

**Interfaces:**
- Consumes: `formal/HoiminOracle/corpus/nested-try-flow.jsonl`, `AnnotationCollector::visit_try`, and `hoimin_cli::run_with_io`.
- Produces: `binding_flow_try_exit_snapshot(source, marker) -> Result<BindingFlowTestSnapshot, String>`, three internal/schema tests, and one strict public integration test.

- [x] **Step 1: Write the adapter test first and name the break**

The production mutation this test must catch is: `visit_try` returns post-`finally` exits in the wrong category or admits a state from an unreachable statement. Create `nested_try_oracle_tests.rs`, include the corpus, define Serde structs with `#[serde(deny_unknown_fields)]`, and add:

```rust
#[test]
fn nested_try_internal_rows_match_post_finally_production_exits() {
    let cases = parse_corpus(CORPUS).expect("Lean corpus must satisfy the adapter schema");
    for item in cases.iter().filter(|item| item.mode == "internal-fixture") {
        let actual = binding_flow_try_exit_snapshot(&item.source, &item.marker)
            .unwrap_or_else(|error| panic!("{}: {error}", item.id));
        assert_eq!(actual, item.expected, "case={}", item.id);
    }
}
```

The adapter's Rust `ExpectedExits` representation must convert directly into `BindingFlowTestSnapshot`; it must not compute transfer expectations.

- [x] **Step 2: Run RED and verify the reason**

```bash
cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests::nested_try_internal_rows_match_post_finally_production_exits -- --exact --nocapture
```

Expected: compile failure because `binding_flow_try_exit_snapshot` and the module registration do not exist. Fix parser or fixture typos until the only failure is the missing production-backed projection.

- [x] **Step 3: Add the narrow test-only projection to `rust.rs`**

Add a `#[cfg(test)]` field to `AnnotationCollector`:

```rust
try_exit_projection: Option<BindingFlowTryExitProjection>,
```

Define the projection with the same marker-selection rules as existing binding-flow projections:

```rust
#[cfg(test)]
struct BindingFlowTryExitProjection {
    source: String,
    marker: Range<usize>,
    matching_tries: usize,
    exits: Option<BindingFlowTestSnapshot>,
}
```

Initialize it to `None` in `AnnotationCollector::empty`. At the end of `visit_try`, bind the result once, capture it, and return it:

```rust
let exits = self.apply_finally(joined, &statement.finalbody);
#[cfg(test)]
self.capture_try_exit(statement.range, &exits);
exits
```

`capture_try_exit` accepts only a `try` range containing the unique marker, increments `matching_tries`, and stores `BindingFlowTestSnapshot` using `normalize_binding_flow_states`. Add:

```rust
#[cfg(test)]
fn binding_flow_try_exit_snapshot(
    source: &str,
    marker: &str,
) -> Result<BindingFlowTestSnapshot, String>
```

It parses the real Ruff module, runs the real collector once, and returns `infrastructure-error:` for empty/non-unique markers, parse failure, zero/multiple matching tries, or missing capture.

Register the focused test module beside `rust_tests`:

```rust
#[cfg(test)]
#[path = "nested_try_oracle_tests.rs"]
mod nested_try_oracle_tests;
```

- [x] **Step 4: Verify internal GREEN**

```bash
cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests::nested_try_internal_rows_match_post_finally_production_exits -- --exact --nocapture
```

Expected: either PASS, or a semantic expected/actual difference with a concrete case ID. A parser/setup/panic result remains infrastructure failure and must be repaired before comparison.

- [x] **Step 5: Add closed-schema and adversarial validation tests**

Add tests that reject:

- duplicate IDs;
- unknown fields and schema versions;
- modes outside the four exact values;
- an ID/family/source/marker tuple that does not match the closed fixture map;
- `strict` without candidate expectation;
- `internal-fixture` without complete categorized exits;
- `model-only` accidentally passed to a production adapter;
- empty or duplicate markers.

Use literal malformed JSONL rows and assert the stable validation category, not an entire Serde error string.

- [x] **Step 6: Add strict public CLI correspondence**

For each `strict` row, write its source into an isolated temporary project, call `crate::run_with_io` with `plan --root PROJECT --file target.py --operators type_list_sequence --jobs 1 --allow-best-effort-memory -- PYTHON -c pass`, enforce a 10-second Tokio deadline, and parse the complete plan manifest. Match only candidates overlapping the unique marker and assert:

```rust
assert_eq!(candidate.path.as_str(), "target.py");
assert_eq!(candidate.operator, "type_list_sequence");
assert_eq!(candidate.original, "Sequence[int]");
assert_eq!(candidate.replacement, "list[int]");
assert_eq!(candidate.symbol.as_deref(), expected_symbol);
```

For `absent`, assert no overlapping candidate of any identity exists. Nonzero exit, stderr on success, timeout, malformed manifest, invalid span, missing marker, or duplicate marker is `infrastructure-error`, not a semantic mismatch.

- [x] **Step 7: Run the full focused adapter**

```bash
cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests -- --nocapture
```

Expected: schema, adversarial, internal, and strict tests pass, unless Task 4 records a confirmed semantic mismatch.

- [x] **Step 8: Commit correspondence infrastructure if there is no production change yet**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/nested_try_oracle_tests.rs crates/hoimin-cli/tests/lean_nested_try_flow_oracle.rs
git commit -m "test: compare nested try flow with Lean"
```

### Task 4: Counterexample classification and conditional minimal Rust repair

**Files:**
- Modify if and only if confirmed: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/nested_try_oracle_tests.rs`
- Preserve: `formal/HoiminOracle/corpus/nested-try-flow.jsonl`
- Update later: `docs/superpowers/reports/2026-08-13-lean-nested-try-finally-flow-audit.md`

**Interfaces:**
- Consumes: the first failing same-premise corpus row and its complete Rust observation.
- Produces: a counterexample record and, only for `confirmed bug`, a focused failing regression followed by the smallest production transfer correction.

- [x] **Step 1: Reproduce one failing row in isolation**

Support `HOIMIN_NESTED_TRY_CASE` in the test adapter and run every
implementation-facing identity separately so the output identifies the first
semantic failure without changing premises:

```bash
for case_id in \
  finally_annotation_meets_normal_and_raise \
  post_finally_uses_only_fallthrough \
  falling_finally_preserves_break \
  falling_finally_preserves_continue \
  falling_finally_preserves_return_terminate \
  falling_finally_preserves_raise_terminate \
  abrupt_finally_replaces_fallthrough \
  abrupt_finally_replaces_break \
  unreachable_post_return_excluded
do
  HOIMIN_NESTED_TRY_CASE="$case_id" \
    cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests -- --nocapture
done
```

Record the first failing identity, source, entry category, finalizer outcome,
intermediate exit snapshot, expected value, actual value, and exact command. If
every row matches, record “no correspondence counterexample” and skip Steps
2–5.

- [x] **Step 2: Classify before changing code**

Use exactly one classification:

```text
confirmed bug: same owned premise and complete observation disagree
specification ambiguity: repository intent does not own the disputed rule
model defect: Lean omitted behavior relevant to the claim
infrastructure error: setup or observation failed
```

For `model defect`, correct the Lean model first, regenerate the corpus, show the diff, and rerun sensitivity. For `infrastructure error`, repair only setup/observation. For `specification ambiguity`, preserve the row as `model-only` and state the owner decision needed. Do not change production Rust for these three classifications.

- [x] **Step 3: Confirm the Lean-owned row is a focused RED regression (skipped: no implementation mismatch)**

For a confirmed bug, do not duplicate the expectation in another fixture. The
generated corpus row is the retained regression. Use the adapter filter with
the failing literal identity and confirm that the focused corpus test fails on
the expected categorized field rather than setup. For example, if the failing
identity is `falling_finally_preserves_continue`, run:

```bash
HOIMIN_NESTED_TRY_CASE=falling_finally_preserves_continue \
  cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests -- --nocapture
```

The filter must select exactly one row or fail as infrastructure setup; it must
never silently fall back to all rows.

- [x] **Step 4: Apply the smallest production correction (skipped: no confirmed bug)**

Change only the branch in `visit_try`, `apply_finally`, or `route_finally_entry` demonstrated by the regression. Preserve these required rules:

```text
falling-through finalizer -> restore incoming category
abrupt finalizer -> emit abrupt finalizer category only
unreachable suite tail -> never re-enter reachable exits
handler target cleanup -> apply to every actual handler exit before finalizer routing
```

Do not refactor unrelated collector traversal, marker projection, match handling, or loop fixed points.

- [x] **Step 5: Verify GREEN and commit the repair separately (skipped: no production repair)**

```bash
cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests -- --nocapture
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/nested_try_oracle_tests.rs formal/HoiminOracle/corpus/nested-try-flow.jsonl
git commit -m "fix: preserve nested try exit semantics"
```

The corpus remains generated and unchanged for a production-only correction.

### Task 5: Final audit report, freshness, and quality gates

**Files:**
- Create: `docs/superpowers/reports/2026-08-13-lean-nested-try-finally-flow-audit.md`
- Modify if the public contract needs a command documented: `docs/development.md`

**Interfaces:**
- Consumes: all proof statements, fixed witnesses, corpus modes, Rust observations, counterexample classifications, resource stats JSON, and exact commands.
- Produces: one self-contained handoff report.

- [x] **Step 1: Re-run retained Lean checks serially with fresh stats paths**

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-final-model.json -- lake env lean HoiminOracle/NestedTryFlowModel.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-final-proofs.json -- lake env lean HoiminOracle/NestedTryFlowProofs.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-final-consumer.json -- lake env lean /tmp/hoimin-nested-try-proof-consumer.lean
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-final-sensitivity.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-final-cases.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --cases
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-final-stats.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --stats
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/hoimin-nested-final-fresh.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --check corpus/nested-try-flow.jsonl
```

- [x] **Step 2: Run focused and workspace Rust quality gates**

```bash
cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 -m unittest tests.test_lean_resource_guard -v
git diff --check
```

If a gate fails, use `superpowers:systematic-debugging`, establish whether the failure is new or baseline/infrastructure, and do not report completion until the relevant new failure is resolved.

- [x] **Step 3: Write the self-contained audit report**

The report must include:

- durable claim, included behavior, and exclusions;
- declared versus implicit Rust behavior;
- full correspondence worksheet with exact mode counts;
- Lean theorem premises and explicit statement that they apply only to the model;
- fixed cases and all seven broken-variant results;
- `generated_depth=0`, `event_alphabet=0`, and fixed-case counts;
- per-command elapsed time, peak sampled RSS, exit/reason, and the baseline aggregate `monitor_error` from `/tmp/hoimin-lean-cost-effective-baseline.json`;
- every minimal witness with intermediate categorized exits;
- correspondence results for strict, internal-fixture, model-only, and infrastructure-error rows;
- classification and minimal Rust correction for every mismatch, or an explicit statement that no same-premise mismatch was found;
- unresolved owner questions;
- exact reproduction commands and the final commit list.

Use this counterexample record format for each mismatch:

```text
claim:
model boundary:
finite domain or theorem premises:
minimal input or trace:
intermediate states:
classification:
implementation correspondence:
owner question:
reproduction command:
```

- [x] **Step 4: Commit the report and any deliberate documentation update**

```bash
git add docs/superpowers/reports/2026-08-13-lean-nested-try-finally-flow-audit.md
git add docs/development.md
git commit -m "docs: report nested try flow Lean audit"
```

Omit `git add docs/development.md` when no public development command changed.

- [ ] **Step 5: Inspect final branch state**

```bash
git status -sb
git log --oneline --decorate -8
git diff origin/main...HEAD --check
git diff --stat origin/main...HEAD
```

Expected: only the intentional untracked `.venv` worktree symlink remains; every repository deliverable is committed.
