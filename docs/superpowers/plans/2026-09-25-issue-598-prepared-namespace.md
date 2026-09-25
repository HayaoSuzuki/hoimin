# Prepared namespace implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Prevent builtin/exception mutations through untrusted prepared class namespaces while preserving safe lookup contexts.

**Architecture:** Attach one conservative namespace flag to name scopes; check it in ordinary and annotation resolution after lookup bypass. A Lean-owned corpus drives public CLI and CPython correspondence tests.

**Tech Stack:** Rust, Ruff AST, Lean 4.32.2, CPython 3.14, existing Cargo/CI and OKF.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-598-prepared-namespace-design.md`

## Global constraints

- No CLI/schema/dependency changes; no Python helper implementation.
- All work remains in the issue-598 worktree; `CARGO_BUILD_JOBS=2`.
- One Lean process at a time, 20-second initial external deadline, bounded heartbeats; use existing resource guard.
- Keep runtime identity separate from conservative static eligibility.

## Review focus

- Inherited metaclasses and dynamic/keyword headers must be conservative (Task 1/2 fixtures).
- Explicit global directives bypass class lookup; nonlocal does not establish builtin identity (Task 1 controls).
- First comprehension iterable reads class scope while body skips it (Task 1/2 fixtures).
- Deferred annotations and generic declaration headers retain class visibility (Task 1 regressions).
- Both exception and callable pair endpoints need protection, even when only destination is injected (Task 1/2 fixtures).

### Task 1: Scope policy and regression

**Files:** Modify `crates/hoimin-cli/src/analyzer/rust.rs`; test `crates/hoimin-cli/src/analyzer/rust_tests.rs`; create `crates/hoimin-cli/tests/lean_prepared_namespace_oracle.rs` in Task 2.

**Interfaces:** `NameScope.may_have_prepared_namespace: bool`; existing `resolve_scope` / `resolve_annotation_scope` return `NameResolution::Unknown` for class-visible untrusted loads.

- [x] Add analyzer regressions using `analyze_source` and operator filtering for explicit/inherited metaclasses, builtin call and exception families using shared resolution, global/nonlocal, plain/empty-header class, closure/method, first iterable/body and annotations. Assert exact relevant candidate counts and spans for positive controls.
- [x] Run `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --lib prepared_namespace -- --nocapture`; observe unwanted candidates on the baseline before production edits.
- [x] Implement the scope flag with default `false`, and class construction rule:
  ```rust
  this.index.scopes[scope.0].may_have_prepared_namespace = definition
      .arguments.as_ref().is_some_and(|args| !args.args.is_empty() || !args.keywords.is_empty());
  ```
  In both resolution paths, after lexical class skip and global handling, return `Unknown` when the class flag is set. Preserve header traversal before class creation.
- [x] Run focused regressions plus all analyzer library tests. Resolve findings, record RED/GREEN, commit the implementation and regressions.

### Task 2: Lean-owned public correspondence

**Files:** Create `formal/HoiminOracle/HoiminOracle/PreparedNamespaceModel.lean`, `formal/HoiminOracle/PreparedNamespaceAuditMain.lean`, `formal/HoiminOracle/corpus/prepared-namespace.jsonl`, and `crates/hoimin-cli/tests/lean_prepared_namespace_oracle.rs`. Modify `HoiminOracle.lean`, `lakefile.toml`, and `.github/workflows/ci.yml`.

**Interfaces:** JSONL schema 1 carries unique id, strict mode, source, operator, original/replacement, runtime observation expression, expected endpoint identity pair and candidate count. Lean is sole producer of expected fields; Rust translates and executes without recomputing semantic expectations.

- [x] Define `runtimeBuiltin (classVisible injected : Bool)` and `eligible (classVisible ordinary : Bool)`; prove eligible endpoints builtin given injection only in nonordinary fixtures. Add deliberate ignored-preparation, source-only and wrong-method-capture witnesses. Observe a failing false theorem before correcting it.
- [x] Generate the 12 historical cases plus boundary controls, with expectations derived from the model. Implement `--output`, `--check`, `--sensitivity`, `--stats` in the generator. Run guarded model build, generator and freshness check; record resources.
- [x] Add Rust corpus parsing validation (schema, unique IDs, required fields), isolated CPython endpoint probes and public plan assertions on count/pair/span/symbol. Use timeouts and report setup/exit failures as infrastructure errors.
- [x] Add public run cases for source-only, destination-only and both injected endpoints: success, complete report, zero killed and empty mutant list. Exercise CPython baseline even when run has zero candidates.
- [x] Wire the model and executable into existing serial CI build lists, corpus freshness and sensitivity entries. Run `CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_prepared_namespace_oracle`.
- [x] Commit formal integration and correspondence tests.

### Task 3: Review, contracts and final verification

**Files:** Modify `README.md`, `docs/knowledge/design/analyzer.md`, `docs/knowledge/references/design-documents.md`, `docs/knowledge/references/audit-documents.md`; create `docs/superpowers/reports/2026-09-25-issue-598-prepared-namespace.md`.

**Interfaces:** The report records the model/runtime/static correspondence, finite bounds, resource results, commands and 12 review passes; OKF links cite actual reviewed revisions or content hashes.

- [x] Perform three implementation review passes: scope-control flow, AST header coverage, public integration/precision. Fix each finding with regressions.
- [x] Perform three test review passes: negative/positive sensitivity, independence of CPython evidence, infrastructure/schema/freshness. Fix discovered gaps and rerun impacted tests.
- [x] Document conservative precision change in README and analyzer contract; register design/report sources and references in OKF.
- [x] Execute `cargo fmt --all -- --check`, `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --all-features -- -D warnings`, `CARGO_BUILD_JOBS=2 cargo test --workspace`, guarded Lean freshness/sensitivity and YAML/link checks. Record executed versus reviewed checks separately.
- [x] Commit final docs and review fixes, report commits, tests and limitations to root for independent review and PR publication.

## Reviewed executable anchors

Task 1's first RED fixture:

```rust
#[test]
fn prepared_namespace_blocks_direct_builtin_pair() {
    let output = analyze("class C(metaclass=Meta):\n    result = any([])\n");
    assert!(output.candidates.iter().all(|c| c.operator != "collection_any_all"));
}
```

Task 2's initial resource command, from `formal/HoiminOracle`:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/issue-598-lean-model.json -- lake build HoiminOracle.PreparedNamespaceModel
```

Follow with the same guard for `lake env lean -j1 --run PreparedNamespaceAuditMain.lean --output corpus/prepared-namespace.jsonl`, `--check`, `--sensitivity` and `--stats`. CI uses its existing 30-second guard and serial module list. Initial build/corpus timings must justify any command deadline adjustment; never increase search bounds or disable heartbeat limits.

The three design and three plan self-reviews are recorded in `docs/superpowers/reports/2026-09-25-issue-598-prepared-namespace.md`.
