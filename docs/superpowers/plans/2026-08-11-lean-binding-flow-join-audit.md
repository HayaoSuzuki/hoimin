# Lean Binding-Flow Join Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Audit and, where same-premise evidence requires it, repair Hoimin's scope-dependent AST binding propagation across control-flow joins.

**Architecture:** Project typing-import snapshots and builtin/exception resolution into one Lean knowledge lattice. Prove conservative join, scope, exit-routing, and loop-fixed-point properties; generate a bounded JSONL corpus; then check strict cases through the public `hoimin plan` path and private state only through disclosed internal fixtures.

**Tech Stack:** Lean 4.32.2, Lake, Rust 1.88+, Ruff Python AST 0.6.2, serde/serde_json, Python 3 standard library for the local RSS guard.

## Global Constraints

- Run at most one Lean or Lake process at a time and always pass `-Kjobs=1` to Lake builds.
- Apply a 20-second wall-clock deadline and `maxHeartbeats 100000`; never use unlimited heartbeats.
- Sample combined parent/descendant RSS every 250 ms, terminate at 768 MiB, and do not increase exploration after a 512 MiB sample.
- Start structured enumeration at depth 2; advance only one level at a time through maximum depth 4.
- Stop expansion after timeout, RSS stop, severe slowdown, more than fourfold state growth, or the hard 1024-state ceiling.
- Keep expensive enumeration and JSON serialization outside imported Lean modules.
- Use exactly `strict`, `internal-fixture`, `model-only`, and `infrastructure-error` correspondence modes.
- Compare strict cases through public `hoimin plan`; direct analyzer and environment snapshots are internal fixtures only.
- Do not change production analyzer behavior without first retaining a same-premise failing test.
- Preserve root-checkout user files, mutation operators, ranking, candidate ordering, schemas, profiles, and limits.

---

### Task 1: Add a tested local Lean RSS and timeout guard

**Files:**

- Create: `formal/HoiminOracle/tools/lean_resource_guard.py`
- Create: `tests/test_lean_resource_guard.py`

**Interfaces:**

- Produces CLI:
  `lean_resource_guard.py --timeout-seconds FLOAT --rss-limit-mib INT --sample-ms INT --stats PATH -- COMMAND...`
- Produces JSON stats fields:
  `schema`, `reason`, `exit_code`, `elapsed_ms`, `peak_rss_kib`, `rss_limit_kib`, `timeout_ms`.
- Returns the child exit code normally, `124` for timeout, `125` for RSS termination, and `126` for setup/monitoring failure.

- [ ] **Step 1: Write failing behavior tests**

  Add `unittest` cases that launch the guard with the current Python
  interpreter and literal fixture commands:

  ```python
  ALLOCATING_CHILD = (
      "import os,time; "
      "payload=bytearray(32*1024*1024); "
      "open(marker,'w').write(str(os.getpid())); time.sleep(30)"
  )
  SLEEPING_CHILD = "import time; time.sleep(30)"
  ```

  The RSS case uses `--rss-limit-mib 8`, requires exit `125`, requires
  `reason == "rss_limit"`, and verifies the marker PID is no longer alive.
  The timeout case uses `--timeout-seconds 0.2`, requires exit `124`, and
  requires `reason == "timeout"`. A normal `sys.exit(7)` case requires exit 7
  and `reason == "child_exit"`.

- [ ] **Step 2: Run the tests and verify RED**

  Run:

  ```bash
  .venv/bin/python -m unittest tests.test_lean_resource_guard -v
  ```

  Expected: import or executable failure because
  `formal/HoiminOracle/tools/lean_resource_guard.py` does not exist.

- [ ] **Step 3: Implement the guard**

  Use `argparse`, `subprocess.Popen(..., start_new_session=True)`,
  `time.monotonic`, `subprocess.run(["ps", "-axo", "pid=,ppid=,rss="], ...)`,
  and `os.killpg`. Parse the process table into parent-to-child edges, compute
  the transitive descendants of the launched PID, and sum their RSS with the
  root RSS. On timeout or limit breach, send `SIGTERM` to the process group,
  wait up to one second, send `SIGKILL` if needed, reap the root, and write the
  stats file atomically with `Path.replace`.

  Reject non-positive timeout, RSS limit, or sample interval before spawning.
  Monitoring/`ps` failure terminates and reaps the process group and returns
  `126`; it never silently runs an unmonitored Lean command.

- [ ] **Step 4: Verify GREEN and formatting**

  Run:

  ```bash
  .venv/bin/python -m unittest tests.test_lean_resource_guard -v
  uv run ruff check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
  uv run ruff format --check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
  git diff --check
  ```

  Expected: three guard behaviors pass, lint and formatting are clean, and no
  fixture process survives.

- [ ] **Step 5: Commit the safety harness**

  ```bash
  git add formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
  git commit -m "test: guard Lean audit resource usage"
  ```

---

### Task 2: Define the common binding-flow model and unbounded proofs

**Files:**

- Create: `formal/HoiminOracle/HoiminOracle/BindingFlowModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/BindingFlowProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-binding-flow-proof-consumer.lean` (ephemeral)

**Interfaces:**

- Produces `Name`, `Target`, `Fact`, `Env`, `ScopeKind`, `Directive`, `Frame`,
  `ExitCategory`, `Exits`, and structured `Stmt`.
- Produces `Fact.meet`, `Env.meet`, `meetAll?`, `resolve`, `allowsCandidate`,
  `routeFinally`, `loopIteration`, and `eval`.
- Produces public theorems `fact_meet_comm`, `fact_meet_assoc`,
  `fact_meet_idem`, `meetAll_retained_on_every_path`,
  `meet_does_not_invent_knowledge`, `allowed_candidate_is_sound`,
  `unrelated_sibling_isolated`, `method_skips_class_scope`,
  `fallthrough_finally_preserves_category`, `loop_iteration_descends`,
  `loop_iteration_stabilizes`, and `iterate_to_fixed_point_returns_stable`.

- [ ] **Step 1: Write the failing theorem consumer**

  Create `/tmp/hoimin-binding-flow-proof-consumer.lean` importing
  `HoiminOracle.BindingFlowProofs`. Instantiate the public theorems with literal
  facts and environments, including a source `.known .builtin`, destination
  `.known .builtin`, an intervening class binding, and a fallthrough finalizer.

- [ ] **Step 2: Run the consumer under the guard and verify RED**

  Run from `formal/HoiminOracle`:

  ```bash
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-proof-red.json -- lake env lean /tmp/hoimin-binding-flow-proof-consumer.lean
  ```

  Expected: child failure naming the absent `BindingFlowProofs` module, while
  the guard reports `reason=child_exit` rather than timeout or RSS termination.

- [ ] **Step 3: Implement the smallest semantic model**

  Define:

  ```lean
  inductive Name | source | destination
  inductive Target | builtin | typing
  inductive Fact | absent | known (target : Target) | shadowed | unknown
  structure Env where source : Fact; destination : Fact
  def Fact.meet (left right : Fact) : Fact :=
    if left == right then left else .unknown
  ```

  Add frames for module/function/class/comprehension and normal/global/nonlocal
  directives. Define categorized exits as one optional fallthrough plus break,
  continue, and terminate state lists. Define structured statements and a
  fuel-indexed evaluator whose fuel exhaustion yields no semantic success and
  is used only by executable exploration. Keep the theorem-facing transfer
  primitives total and independent of enumeration depth.

- [ ] **Step 4: Prove lattice, scope, exit, loop, and safety laws**

  Put `set_option maxHeartbeats 100000 in` on each non-trivial theorem. Prove
  meet laws by cases, path-retention by list induction, scope properties by
  explicit frame cases, finally category preservation for a falling-through
  finalizer, and loop descent/stabilization using a finite rank over the two
  facts in `Env`. Prove the candidate theorem from exact target equality; do
  not weaken `.unknown` into an allowed state.

- [ ] **Step 5: Verify GREEN under separate guarded commands**

  Run one at a time:

  ```bash
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-proofs.json -- lake env lean HoiminOracle/BindingFlowProofs.lean
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-consumer.json -- lake env lean /tmp/hoimin-binding-flow-proof-consumer.lean
  ```

  Expected: both pass; each stats file has peak RSS below 512 MiB before any
  bounded-depth increase is considered.

- [ ] **Step 6: Commit the model and proofs**

  ```bash
  git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle/BindingFlowModel.lean formal/HoiminOracle/HoiminOracle/BindingFlowProofs.lean
  git commit -m "test(lean): model binding flow joins"
  ```

---

### Task 3: Add bounded structured programs, sensitivity, and corpus generation

**Files:**

- Create: `formal/HoiminOracle/HoiminOracle/BindingFlowCases.lean`
- Create: `formal/HoiminOracle/BindingFlowAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/binding-flow-joins.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**

- Produces executable `generate_binding_flow` commands:
  `--output PATH`, `--check PATH`, `--stats DEPTH`, `--sensitivity`, and
  `--cases`.
- Corpus schema 1 includes `id`, `mode`, `family`, `operator`, `source`,
  `site_marker`, `expected_present`, `expected_replacement`, and
  `expected_symbol`.

- [ ] **Step 1: Add failing fixed-case and sensitivity consumers**

  Extend the ephemeral consumer to require `fixedCasesPass = true` and
  `sensitivityPasses = true`. Run it under the guard and confirm RED because
  `BindingFlowCases` is absent.

- [ ] **Step 2: Define fixed semantic cases**

  Define Lean-owned cases for:

  - identical and disagreeing `if` branches;
  - loop zero-iteration, fallthrough back-edge, continue back-edge, and break;
  - try normal/handler exits and falling-through/abrupt `finally`;
  - unmatched, irrefutable, and conservative guard-binding match paths;
  - function whole-block local, closure, and class non-closure;
  - global/nonlocal uncertainty, wildcard import, and unconditional re-import;
  - builtin/exception source and destination gating.

  Every strict case carries literal Python source and a unique marker whose
  annotated expression, call, or exception occurrence is the observation site.
  Expected presence and target spelling are computed from `eval` and
  `allowsCandidate`, not duplicated as hand-written booleans.

- [ ] **Step 3: Add five broken variants and fixed witnesses**

  Implement `brokenUnionMeet`, `brokenResolveThroughClass`,
  `brokenRouteFinally`, `brokenLoopIteration`, and
  `brokenAllowsSourceOnly`. Require a literal minimal witness for each and make
  `sensitivityPasses` the conjunction of all five detections.

- [ ] **Step 4: Implement canonical bounded enumeration**

  Generate semantic statements by exact syntax depth over two names and a
  reduced event alphabet. Deduplicate by resulting `Exits` plus candidate
  decision before expanding the next layer. Report `programs`, `states`,
  `transitions`, `depth`, and `state_ceiling`; fail before corpus output if
  states exceed 1024, a fixed case fails, or sensitivity fails.

- [ ] **Step 5: Run depth 2 and evaluate safety gates**

  ```bash
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-depth2-resource.json -- lake exe generate_binding_flow -- --stats 2
  ```

  Record executable statistics and guard peak RSS. Continue only if exit is
  zero, peak RSS is below 512 MiB, and states do not exceed 1024.

- [ ] **Step 6: Run depth 3, then conditionally depth 4**

  Run depth 3 under an independent stats file. Run depth 4 only if depth 3 is
  below 512 MiB, below 1024 states, below 20 seconds, and no count is more than
  four times depth 2. Retain the deepest safe result; never retry a stopped
  depth with larger thresholds.

- [ ] **Step 7: Verify sensitivity, cases, and generated corpus**

  Run each in its own guarded process:

  ```bash
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-sensitivity.json -- lake exe generate_binding_flow -- --sensitivity
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-cases.json -- lake exe generate_binding_flow -- --cases
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-output.json -- lake exe generate_binding_flow -- --output corpus/binding-flow-joins.jsonl
  ../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/binding-check.json -- lake exe generate_binding_flow -- --check corpus/binding-flow-joins.jsonl
  ```

  Expected: five sensitivity flags and all fixed cases are true; freshness
  returns zero without rewriting the corpus.

- [ ] **Step 8: Commit executable evidence**

  ```bash
  git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle/BindingFlowCases.lean formal/HoiminOracle/BindingFlowAuditMain.lean formal/HoiminOracle/corpus/binding-flow-joins.jsonl formal/HoiminOracle/lakefile.toml
  git commit -m "test(lean): audit bounded binding flow programs"
  ```

---

### Task 4: Connect the Lean corpus to public and internal Rust observations

**Files:**

- Create: `crates/hoimin-cli/tests/lean_binding_flow_oracle.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs` only to add `#[cfg(test)]`
  projection helpers for internal-fixture cases
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**

- Strict adapter consumes `binding-flow-joins.jsonl`, calls public
  `hoimin_cli::run_with_io` with parsed `plan` arguments, and reads the emitted
  `PlanManifest`.
- Internal fixture exposes exact normalized facts only as
  `BindingFlowTestSnapshot { fallthrough, breaks, continues, terminates }`
  under `#[cfg(test)]`.

- [ ] **Step 1: Write failing corpus parser tests**

  Add tests requiring schema 1, unique non-empty IDs, exact four mode strings,
  known families/operators, non-empty source and marker, exact nullable symbol
  fields, and scenario-to-premise validation. Mutate one valid JSON line with
  an unknown field, duplicate ID, wrong mode, and mismatched operator/family;
  each must be rejected.

- [ ] **Step 2: Run parser tests and verify RED**

  ```bash
  cargo test -p hoimin-cli --test lean_binding_flow_oracle corpus -- --nocapture
  ```

  Expected: test target or `parse_corpus` is missing.

- [ ] **Step 3: Implement the strict public-plan adapter**

  For each strict case, create a `tempfile` project with `src/case.py`, a
  minimal `pyproject.toml`, and the Lean-provided source. Parse plan arguments
  with the case's exact operator, invoke `hoimin_cli::run_with_io`, require exit
  zero and empty stderr, deserialize `PlanManifest`, normalize only candidates
  whose source span covers the literal marker site, and compare presence,
  replacement, operator, and symbol.

  Unexpected CLI exit, malformed manifest, missing marker, duplicate matching
  candidates, or timeout is an infrastructure error and fails with that label,
  not a semantic mismatch.

- [ ] **Step 4: Verify strict cases and retain every mismatch**

  ```bash
  cargo test -p hoimin-cli --test lean_binding_flow_oracle strict -- --nocapture
  ```

  Expected: either all same-premise cases match, or the test prints the single
  case ID, source, expected normalized observation, and actual observation.
  Do not edit Lean expectations to make a mismatch pass.

- [ ] **Step 5: Add internal-fixture tests for exits and fixed points**

  Add a test-only projection around `AnnotationCollector::visit_statement_flow`
  and `loop_head_fixed_point`. Return sorted normalized direct/module/type-var
  facts for each exit category. Compare the internal-fixture corpus cases to
  literal expected snapshots derived from Lean and assert a deliberately
  removed continue edge changes at least one test result.

- [ ] **Step 6: Run focused analyzer regression tests**

  ```bash
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_import_rebinding -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::sibling_bindings_do_not_suppress_builtin_or_exception_pairs -- --nocapture
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::visible_source_or_destination_binding_suppresses_builtin_pairs -- --nocapture
  ```

  Expected: existing exact candidate behavior remains green alongside the new
  corpus adapter.

- [ ] **Step 7: Commit correspondence tests**

  ```bash
  git add crates/hoimin-cli/tests/lean_binding_flow_oracle.rs crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
  git commit -m "test: check analyzer binding flow against Lean"
  ```

---

### Task 5: Reconcile and repair same-premise mismatches

**Files:**

- Modify if evidence requires: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Modify: `crates/hoimin-cli/tests/lean_binding_flow_oracle.rs`
- Modify: `formal/HoiminOracle/HoiminOracle/BindingFlowCases.lean` only when the
  original abstraction, rather than production, is demonstrably wrong

**Interfaces:**

- Consumes failing strict or internal-fixture case IDs from Task 4.
- Produces a retained minimal regression and a classification of `confirmed
  bug`, `specification ambiguity`, `model defect`, or `infrastructure error`.

- [ ] **Step 1: Minimize each mismatch without changing its premise**

  Remove unrelated statements from the Lean-owned Python source one at a time,
  regenerate the corpus, and rerun the single case until no further removal
  preserves the differing normalized observation. Record intermediate binding
  states and the exact exit category or scope boundary where results diverge.

- [ ] **Step 2: Add a focused failing Rust regression**

  Add one literal source test per confirmed implementation bug. Name the wrong
  production branch it catches, assert the exact candidate vector or internal
  exit snapshot, run it alone, and confirm failure is the observed semantic
  mismatch rather than parsing or setup.

- [ ] **Step 3: Apply the smallest production correction**

  Change only the responsible transfer boundary: `KnownImports::intersection`,
  the relevant `visit_if`/`visit_loop`/`visit_try`/`apply_finally`/`visit_match`
  path, function/class fallback, comprehension scope handling, or
  source/destination gate. Preserve conservative uncertainty and do not add a
  source-wide fallback.

- [ ] **Step 4: Verify the focused RED became GREEN**

  Rerun the single regression, the single corpus case, all strict/internal
  cases, and the existing binding tests. If evidence is a model defect instead,
  retain the Rust observation, correct the Lean premise, regenerate, and state
  why the original abstraction was invalid.

- [ ] **Step 5: Commit each independent repair**

  ```bash
  git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs crates/hoimin-cli/tests/lean_binding_flow_oracle.rs formal/HoiminOracle/HoiminOracle/BindingFlowCases.lean formal/HoiminOracle/corpus/binding-flow-joins.jsonl
  git commit -m "fix: preserve binding facts across control flow"
  ```

  If no mismatch exists, skip this commit and record “no implementation
  mismatch” in the report; do not make speculative production changes.

---

### Task 6: Report, verify, review, and integrate

**Files:**

- Create: `docs/superpowers/reports/2026-08-11-lean-binding-flow-join-audit.md`

**Interfaces:**

- Produces the durable audit report, exact resource ledger, correspondence
  results, minimized counterexamples, repairs, limitations, and next target.

- [ ] **Step 1: Run fresh guarded Lean verification**

  Run the theorem consumer, `lake -Kjobs=1 build`, retained-depth stats, five
  sensitivity witnesses, fixed cases, and corpus freshness as separate guard
  invocations. Record every stats JSON result. Scan the new Lean files for
  `sorry`, `admit`, and custom `axiom`; require no matches.

- [ ] **Step 2: Run fresh Rust and Python verification**

  ```bash
  .venv/bin/python -m unittest tests.test_lean_resource_guard -v
  cargo test -p hoimin-cli --test lean_binding_flow_oracle
  cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::typing_import_rebinding
  cargo test --workspace --all-features
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo fmt --all -- --check
  uv run ruff check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
  uv run ruff format --check formal/HoiminOracle/tools/lean_resource_guard.py tests/test_lean_resource_guard.py
  git diff --check
  ```

  Expected: all commands succeed with no warnings or stale corpus.

- [ ] **Step 3: Write the self-contained report**

  Include the durable claim, exclusions, correspondence worksheet, retained
  depth and abandoned attempts, programs/states/transitions, elapsed time,
  peak RSS, the RSS watcher's non-hard-limit caveat, theorem premises, five
  sensitivity witnesses, per-mode implementation result, every mismatch record,
  repairs, and exact reproduction commands.

- [ ] **Step 4: Self-review the complete branch**

  Inspect `git diff origin/main...HEAD`, confirm the root checkout was untouched,
  confirm only the worktree `.venv` is ignored, scan for placeholders and
  accidental unlimited Lean settings, and commit the report.

  ```bash
  git add docs/superpowers/reports/2026-08-11-lean-binding-flow-join-audit.md
  git commit -m "docs: report Lean binding-flow join audit"
  ```

- [ ] **Step 5: Push and create the pull request**

  Push `audit/lean-binding-flow`, create a PR summarizing theorem scope,
  bounded-search/resource statistics, implementation correspondence, and all
  verification commands. Request no claim that Lean proves the Rust walker.

- [ ] **Step 6: Wait for all applicable CI and squash merge**

  Monitor every PR check to completion. Diagnose any failure before changing
  code. When all applicable checks pass, squash merge, fetch `origin/main`, and
  verify the reported merge commit is an ancestor of `origin/main`.
