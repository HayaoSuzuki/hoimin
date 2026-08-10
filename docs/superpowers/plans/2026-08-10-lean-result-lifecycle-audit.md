# Lean Result Lifecycle Consistency Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and run a Lean-owned audit that checks whether accepted mutant identities and statuses remain conserved across session persistence, report output, summary counts, stop handling, and metrics accounting.

**Architecture:** Add a focused `ResultLifecycle` model, proofs, deterministic cases, and bounded audit executable to the existing pinned `formal/HoiminOracle` project. Generate a versioned JSONL corpus and compare its stable strict projection with real CLI JSON/JSONL output, SQLite rows, metrics files, warnings, and exit codes through a dedicated Rust integration adapter. Keep production behavior unchanged; record every mismatch in a self-contained report and counterexample ledger.

**Tech Stack:** Lean 4 with the repository-pinned toolchain and `Std`; Rust 1.88+; Tokio integration tests; Serde JSON; Rusqlite; existing Hoimin CLI test fixtures.

## Global Constraints

- The work is audit-only: do not repair or refactor production behavior in this branch.
- Lean proofs establish model properties only; implementation correspondence is a separate adapter result.
- Use exactly `strict`, `internal-fixture`, `model-only`, and `infrastructure-error` for correspondence modes.
- Generate expectations only in Lean; never hand-edit corpus expectations or recompute them in Rust.
- Keep exhaustive exploration, shrinking, statistics, `native_decide`, and serialization out of imported proof modules.
- Enumerate traces shortest-first with stable event order and disclose the finite domain, depth, and reductions.
- Treat panics, setup failures, timeouts, malformed captures, and unavailable platform primitives as infrastructure errors.
- Retain fixed witnesses for atomicity, uniqueness, boundary/precedence, cross-surface consistency, and metrics conservation.
- Pure documentation commits include `[skip ci]`; commits containing Lean, Rust, or generated corpus changes do not.
- A confirmed bug receives a detailed Issue but no production fix in this worktree.

---

### Task 1: Define the result-lifecycle model and kernel-checked invariants

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ResultLifecycleModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/ResultLifecycleProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Consumes: the pinned Lean project and list-based model patterns used by `SessionModel.lean` and `ShutdownModel.lean`.
- Produces: `ResultLifecycle.Mutant`, `Status`, `Result`, `Setup`, `Event`, `Verdict`, `State`, `initial`, `step`, `run`, `Invariant`, `safe`, and deliberately broken transitions consumed by Tasks 2 and 3.

- [ ] **Step 1: Add imports that expose the missing model and proof modules**

```lean
import HoiminOracle.ResultLifecycleModel
import HoiminOracle.ResultLifecycleProofs
```

- [ ] **Step 2: Run the Lean build and record the expected Red result**

Run: `cd formal/HoiminOracle && lake build`

Expected: FAIL because `HoiminOracle.ResultLifecycleModel` and
`HoiminOracle.ResultLifecycleProofs` do not exist.

- [ ] **Step 3: Implement the minimal semantic types and correct transition**

Use two finite mutant roles and seven statuses:

```lean
inductive Mutant | m0 | m1
inductive Status
  | killed | survived | timeout | outOfMemory | processLimit | error | notRun

structure Result where
  mutant : Mutant
  status : Status
  executed : Bool

structure Setup where
  session : Bool
  metrics : Bool
  discovered : List Mutant
  seededDurable : List Result

inductive Event
  | discover (mutant : Mutant)
  | accept (mutant : Mutant) (status : Status)
  | persistOk (mutant : Mutant)
  | persistFailed (mutant : Mutant)
  | reportOk (mutant : Mutant)
  | reportFailed (mutant : Mutant)
  | stop
  | markNotRun (mutant : Mutant)
  | finishSession (complete : Bool)
  | finishMetrics
  | returnRun
```

`State` stores discovered identities, seeded durable results from a prior run,
current-run accepted real results, durable results,
persistence failures, reported results, summary statuses, metrics-executed
count, stop/final flags, diagnostics, and duplicate/overwrite instrumentation.
Reject invalid events without changing the state. `accept` rejects `notRun`,
increments accepted-process accounting exactly once, and preserves status.
`reportOk` requires a matching accepted result and, when sessions are enabled,
either a matching durable result or a recorded persistence failure.
`markNotRun` is permitted only for discovered identities without an accepted
real result.

- [ ] **Step 4: Define structural and conservation predicates**

```lean
def Invariant (state : State) : Prop :=
  state.accepted.Pairwise (fun left right => left.mutant != right.mutant) ∧
  state.durable.Pairwise (fun left right => left.mutant != right.mutant) ∧
  state.reported.Pairwise (fun left right => left.mutant != right.mutant) ∧
  (∀ result ∈ state.durable,
    result ∈ state.accepted ∨ result ∈ state.seededDurable) ∧
  (∀ result ∈ state.reported,
    result ∈ state.accepted ∨ result ∈ state.seededDurable ∨
      result.executed = false ∧ result.status = .notRun) ∧
  (∀ status, state.summary.count status =
    (state.reported.map Result.status).count status) ∧
  state.metricsExecuted = state.accepted.length
```

Add `safe : State -> Bool` for the executable checks, including exact status
agreement, stop preservation, complete-final coverage, no duplicate durable or
reported identity, and zero executed contribution from `notRun`.

- [ ] **Step 5: Prove the surviving contract and fixed named traces**

Provide theorem bodies without `sorry`, `admit`, or custom axioms:

```lean
theorem rejected_preserves_state ...
theorem step_preserves_invariant ...
theorem run_preserves_invariant ...
theorem accepted_status_is_stable ...
theorem stopped_not_run_does_not_increment_metrics ...
theorem complete_report_summary_corresponds ...
```

Define broken transitions for premature summary mutation, duplicate report,
stop overwrite to `notRun`, persisted/reported status divergence, and missing
or spurious metrics execution. Add fixed `example ... := by decide` witnesses
whose exact schedules fail `safe` for each broken family and pass for `step`.

- [ ] **Step 6: Run the Lean build and confirm Green**

Run: `cd formal/HoiminOracle && lake build`

Expected: PASS with all model theorems and fixed sensitivity examples checked.

- [ ] **Step 7: Commit the formal semantics**

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/HoiminOracle/ResultLifecycleModel.lean \
  formal/HoiminOracle/HoiminOracle/ResultLifecycleProofs.lean
git commit -m "test(lean): model result lifecycle conservation"
```

### Task 2: Add deterministic cases, bounded exploration, and corpus generation

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ResultLifecycleCases.lean`
- Create: `formal/HoiminOracle/ResultLifecycleAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/result-lifecycle.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: `ResultLifecycle.step`, `run`, `safe`, broken transitions, and stable observations from Task 1.
- Produces: `resultLifecycleCases`, JSONL schema version 1, `lake exe generate_result_lifecycle -- --output|--check|--stats|--sensitivity`, and the checked-in corpus consumed by Task 3.

- [ ] **Step 1: Register the missing executable and case import**

```toml
[[lean_exe]]
name = "generate_result_lifecycle"
root = "ResultLifecycleAuditMain"
```

Add `import HoiminOracle.ResultLifecycleCases` to `HoiminOracle.lean`.

- [ ] **Step 2: Run the new target and record the expected Red result**

Run: `cd formal/HoiminOracle && lake exe generate_result_lifecycle -- --stats`

Expected: FAIL because the case and executable modules do not exist.

- [ ] **Step 3: Define corpus records and stable observations**

```lean
structure ExpectedObservation where
  accepted : List Result
  durable : List Result
  reported : List Result
  summary : List Status
  metricsExecuted : Nat
  stopped : Bool
  sessionComplete : Bool
  runComplete : Bool
  returned : Bool

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  setup : Setup
  schedule : List Event
  expected : ExpectedObservation
```

Generate expected observations only with `observe (run step (initial setup)
schedule)`. Include strict cases for sessionless completion, session completion,
resume reuse, stop after accepted result, metrics write failure projection, and
session persistence failure. Include model-only cases for duplicate/stale
completion and the exact summary-to-report boundary. Include an
internal-fixture persist-stop race case.

Use these stable IDs: `sessionless_complete`, `session_complete`,
`resume_reuses_determinate`, `stop_preserves_accepted`,
`metrics_write_failure`, `session_persistence_failure`,
`stop_during_persist`, `duplicate_completion`, and
`stop_after_summary_before_report`.

- [ ] **Step 4: Implement breadth-first exploration and sensitivity commands**

Use a stable alphabet over two mutant roles and representative statuses. Search
shortest-first through depth nine, deduplicate exact states only after checking
all successors, minimize the first witness, and report:

Print one statistics line containing the numeric `depth`, `alphabet`,
`states`, and `transitions` fields, followed by one minimized trace line for
each of `atomicity`, `uniqueness`, `boundary`, `cross_surface`, and `metrics`.

The `--sensitivity` command exits nonzero unless all five broken transitions
have a retained witness. `--check` regenerates in memory and compares exact
bytes with `corpus/result-lifecycle.jsonl`.

- [ ] **Step 5: Generate the corpus and inspect the semantic diff**

Run:

```bash
cd formal/HoiminOracle
lake exe generate_result_lifecycle -- --output corpus/result-lifecycle.jsonl
lake exe generate_result_lifecycle -- --check corpus/result-lifecycle.jsonl
lake exe generate_result_lifecycle -- --stats
lake exe generate_result_lifecycle -- --sensitivity
```

Expected: all commands exit 0; the corpus contains unique IDs, exact approved
modes, stable schedules, and Lean-derived observations.

- [ ] **Step 6: Verify expensive evaluation stays outside the imported library**

Run: `cd formal/HoiminOracle && /usr/bin/time -p lake build`

Expected: PASS without executing the breadth-first search or rewriting corpus
files. Record the elapsed time for the final report.

- [ ] **Step 7: Commit cases and generated evidence**

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/lakefile.toml \
  formal/HoiminOracle/HoiminOracle/ResultLifecycleCases.lean \
  formal/HoiminOracle/ResultLifecycleAuditMain.lean \
  formal/HoiminOracle/corpus/result-lifecycle.jsonl
git commit -m "test(lean): generate result lifecycle oracle"
```

### Task 3: Build the Rust corpus validator and implementation adapter

**Files:**
- Create: `crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs`

**Interfaces:**
- Consumes: `corpus/result-lifecycle.jsonl` and the public `hoimin` binary.
- Produces: strict case execution, `HOIMIN_RESULT_LIFECYCLE_CASE` single-case replay, parser rejection tests, normalized observations, and match/mismatch/infrastructure-error classification.

- [ ] **Step 1: Add a parser test before the adapter implementation**

Define deny-unknown-fields Serde records matching the corpus and tests that
require schema 1, unique IDs, known modes, scenarios, events, mutants,
statuses, nonempty schedules, and consistent expected observations.

```rust
#[test]
fn result_lifecycle_corpus_is_typed_and_complete() {
    let cases = parse_corpus(corpus_text()).expect("valid Lean corpus");
    assert!(cases.iter().any(|case| case.mode == "strict"));
    assert!(cases.iter().any(|case| case.mode == "model-only"));
    assert!(cases.iter().any(|case| case.mode == "internal-fixture"));
}
```

- [ ] **Step 2: Run the focused test and record Red**

Run: `cargo test -p hoimin-cli --test lean_result_lifecycle_oracle -- --nocapture`

Expected: FAIL until parser helpers and scenario adapters are implemented.

- [ ] **Step 3: Implement stable observation and classification types**

```rust
enum CaseClass { Match, Mismatch, InfrastructureError }

struct ImplementationObservation {
    accepted: Vec<ResultObservation>,
    durable: Vec<ResultObservation>,
    reported: Vec<ResultObservation>,
    summary: Vec<StatusObservation>,
    metrics_executed: u64,
    stopped: bool,
    session_complete: bool,
    run_complete: bool,
    returned: bool,
}
```

Expected values come only from deserialized corpus fields. Normalization may
map generated UUIDs and candidate IDs to semantic `m0`/`m1` roles but must not
derive statuses, counts, or completeness.

- [ ] **Step 4: Implement isolated strict scenarios through the real CLI**

Create a temporary Python project per case and invoke `CARGO_BIN_EXE_hoimin`.
Implement:

- sessionless one/two-result completion;
- complete `--session` persistence;
- resume reuse with a metrics sidecar and no new mutant process;
- cancellation or total-timeout after one accepted result;
- unwritable metrics destination with unchanged report/exit result;
- SQLite-triggered persistence failure with no partial durable result.

Read JSON/JSONL reports directly, query SQLite independently, deserialize
`RunMetrics`, capture warnings and exit code, and compare the full observable
projection. Use existing test helpers only for fixture construction and process
control, not expected semantic calculation.

- [ ] **Step 5: Separate infrastructure failures and support one-case replay**

Wrap every case boundary so fixture setup, subprocess timeout, signal delivery,
JSON parsing, SQLite query, and panic failures return
`CaseClass::InfrastructureError`. Read `HOIMIN_RESULT_LIFECYCLE_CASE` to select
one exact corpus ID and fail if it does not exist.

- [ ] **Step 6: Run report mode and retain every raw mismatch**

Run:

```bash
HOIMIN_RESULT_LIFECYCLE_MODE=report \
  cargo test -p hoimin-cli --test lean_result_lifecycle_oracle \
  oracle_correspondence -- --exact --nocapture
```

Expected: the command prints each match, mismatch, or infrastructure error and
does not rewrite the corpus or expected values.

- [ ] **Step 7: Commit the adapter without production changes**

```bash
git add crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs
git commit -m "test(cli): compare result lifecycle Lean oracle"
```

### Task 4: Reconcile strict, internal-fixture, and model-only evidence

**Files:**
- Modify: `crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs`
- Test: `crates/hoimin-core/tests/machine.rs`
- Test: `crates/hoimin-cli/tests/run_e2e.rs`
- Test: `crates/hoimin-cli/tests/session_handler.rs`

**Interfaces:**
- Consumes: report-mode classifications and fixed Lean witnesses from Tasks 1-3.
- Produces: blocking strict matches, explicit unresolved mismatch records, focused internal-fixture evidence, and exact single-case reproduction commands.

- [ ] **Step 1: Reproduce every report-mode mismatch individually**

Run for each mismatch:

```bash
HOIMIN_RESULT_LIFECYCLE_MODE=report \
HOIMIN_RESULT_LIFECYCLE_CASE=session_persistence_failure \
  cargo test -p hoimin-cli --test lean_result_lifecycle_oracle \
  oracle_correspondence -- --exact --nocapture
```

Record expected and actual complete observations, differing fields, fixture
premises, and whether the comparison remains same-premise.

- [ ] **Step 2: Classify results without weakening the model**

Use only `confirmed bug`, `specification ambiguity`, `model defect`, or
`infrastructure error`. Correct model or adapter defects before continuing;
never alter an owned expected value solely to match Rust.

- [ ] **Step 3: Promote reviewed strict matches to the blocking gate**

The default `oracle_correspondence` test must fail on any new strict mismatch or
infrastructure error. If a confirmed bug remains, keep its exact case visible
in report mode and make the blocking policy explicit in the report rather than
silently allowlisting it.

- [ ] **Step 4: Run independent existing evidence**

Run focused tests covering:

```bash
cargo test -p hoimin-core --test machine cancellation_during_result_persistence_reports_the_classified_result -- --exact --nocapture
cargo test -p hoimin-core --test machine deadline_during_result_persistence_reports_the_classified_result -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e sqlite_save_failure_reports_the_classification_but_leaves_no_partial_database_result -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e metrics_write_failure_warns_without_changing_run_result -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e serial_output_that_requests_stop_is_accepted_before_cancellation -- --exact --nocapture
```

If exact test names differ, locate the current owned test with `rg` and record
the actual command in the report; do not substitute a different semantic
premise.

- [ ] **Step 5: Commit any adapter classification corrections**

```bash
git add crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs
git commit -m "test(cli): classify result lifecycle correspondence"
```

Skip this commit when reconciliation requires no file change.

### Task 5: Write the self-contained audit report and counterexample ledger

**Files:**
- Create: `docs/superpowers/reports/2026-08-10-lean-result-lifecycle-audit.md`
- Create when needed: `docs/superpowers/reports/2026-08-10-lean-result-lifecycle-counterexamples.md`

**Interfaces:**
- Consumes: theorem list, finite exploration statistics, sensitivity witnesses, per-mode correspondence results, timings, and exact reproduction commands.
- Produces: the complete audit handoff required by `lean-formal-audit` and an Issue-ready ledger for each confirmed mismatch.

- [ ] **Step 1: Record the report contract completely**

Include the durable claim and exclusions, declared versus implicit behavior,
full correspondence worksheet, model state/events, theorem premises, bounded
domain/depth/alphabet/reductions, computation placement and cost, all five
broken-family witnesses, corpus inventory, strict results, internal-fixture
evidence, model-only boundaries, infrastructure failures, and owner decisions.

- [ ] **Step 2: Record every counterexample in the required form**

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

For a confirmed bug, add expected versus actual fields, source locations,
impact, and recommended repair scope. If there are no unresolved witnesses,
state that explicitly and do not create an empty ledger.

- [ ] **Step 3: Self-review the report**

Run:

```bash
rg -n "[T]BD|[T]ODO|proved Hoimin|proved the implementation|report-only" \
  docs/superpowers/reports/2026-08-10-lean-result-lifecycle*.md
git diff --check
```

Expected: no placeholders, no overclaim that Lean proved Rust, exact approved
mode names, and no whitespace errors.

- [ ] **Step 4: Commit the audit report**

```bash
git add docs/superpowers/reports/2026-08-10-lean-result-lifecycle-audit.md
git add docs/superpowers/reports/2026-08-10-lean-result-lifecycle-counterexamples.md
git commit -m "docs: report result lifecycle Lean audit [skip ci]"
```

Omit the second `git add` when no ledger exists.

### Task 6: Verify, review, publish, and hand off

**Files:**
- Modify only if verification exposes an audit-artifact defect: files created in Tasks 1-5.

**Interfaces:**
- Consumes: all audit artifacts and report evidence.
- Produces: independently reviewed commits, a pull request, successful CI, squash merge, closed audit Issue when one exists, synchronized main, and removed audit worktree/branches.

- [ ] **Step 1: Run fresh formal verification**

```bash
cd formal/HoiminOracle
lake build
lake exe generate_result_lifecycle -- --check corpus/result-lifecycle.jsonl
lake exe generate_result_lifecycle -- --stats
lake exe generate_result_lifecycle -- --sensitivity
```

- [ ] **Step 2: Run fresh Rust verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p hoimin-cli --test lean_result_lifecycle_oracle -- --nocapture
cargo test --workspace
uv run --frozen python -m unittest tests/test_skills.py
git diff --check main...HEAD
git status -sb
```

- [ ] **Step 3: Perform sensitivity mutation checks on the adapter**

Temporarily break at least status comparison, duplicate identity detection, and
infrastructure-error classification. Confirm the focused adapter fails for
each, restore the exact committed code, and rerun the adapter. Do not commit
temporary mutations.

- [ ] **Step 4: Request an independent code review**

Review the complete `main...HEAD` diff for critical, important, and minor
findings. Require explicit checks for correspondence premise equality, corpus
ownership, mode vocabulary, exhaustive-work placement, broken-family
sensitivity, production-code absence, and report reproducibility. Resolve all
critical and important audit-artifact defects before publishing.

- [ ] **Step 5: Publish the pull request**

```bash
git push -u origin audit/lean-result-lifecycle
gh pr create --title "test: audit result lifecycle consistency with Lean" \
  --body-file /private/tmp/hoimin-lean-result-lifecycle-pr.md
```

The PR body summarizes the claim, finite boundary, theorem scope, adapter
results, mismatches, verification, and explicitly states that production code
was not changed.

- [ ] **Step 6: Monitor CI and squash merge**

Run `gh pr checks --watch` until all required checks succeed. Expected
self-hosted hard-cgroup skips remain disclosed. Then run:

```bash
gh pr merge --squash --delete-branch
```

Verify the PR is `MERGED` even if the CLI reports that `main` is already used
by the primary worktree.

- [ ] **Step 7: Create detailed Issues only for confirmed bugs**

Use the counterexample ledger verbatim enough to preserve the claim, minimal
trace, expected/actual fields, correspondence mode, source locations, impact,
and reproduction command. Do not create Issues for resolved model defects or
infrastructure errors.

- [ ] **Step 8: Synchronize main and remove audit isolation**

```bash
git fetch origin
git merge --ff-only origin/main
git worktree remove /Users/hayao/RustroverProjects/hoimin/.worktrees/lean-result-lifecycle-audit
git worktree prune
git push origin --delete audit/lean-result-lifecycle
git branch -D audit/lean-result-lifecycle
```

Preserve the primary checkout's existing untracked `.idea/`, `.serena/`, and
`tests/fixtures/projects/basic/uv.lock`. Verify `HEAD == origin/main`, the audit
worktree and branches are absent, the PR is merged, and every confirmed bug
Issue is open with the detailed handoff.
