# Issue #561 Method Replacement Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task. The user authorized inline execution and three self-reviews per artifact; root handles final publication.

**Goal:** Remove full-expression replacement work for unselected method operators without changing candidates.

**Architecture:** Guard existing producer branches with their exact operator. Keep syntax helpers unchanged except cfg(test) instrumentation at actual entrances and allocation boundaries. Preserve visitor traversal and subscript neighbor collection.

**Tech Stack:** Rust 2024, Cargo, existing Ruff parser and standalone Rust allocation probe.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-561-method-replacements.md`

## Global Constraints

- Rust 2024, minimum Rust 1.88; no new dependencies or public API changes.
- Keep all changes in the issue-561 worktree and use `/private/tmp/hoimin-issue-561-target`.
- Commit design and plan before implementation; three substantive self-reviews each of design, plan, implementation and tests are recorded in the review log.
- Root agent owns the knowledge catalog update and PR publication.

All Cargo commands use `CARGO_TARGET_DIR=/private/tmp/hoimin-issue-561-target CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2`.

## Review Focus

1. Selecting append→insert must not build append→extend, or vice versa: independent-family counter case.
2. Skipping a parent helper must retain a nested selected method or token mutation: nested append/addition and selected method in unselected receiver cases.
3. Subscript selection must not suppress index/slice neighbors: select StructureIndexNeighbor independently.
4. Positive counters must observe real helper work: direct helper call and all selected directions.
5. Limit zero, exact count and overflow must retain the same IDs/order/truncation: public-discovery baseline prefixes.

### Task 1: Reproduce and pin method work and candidate compatibility

**Files:** Modify `crates/hoimin-cli/src/analyzer/rust.rs`; create `crates/hoimin-cli/src/analyzer/rust/method_replacement_tests.rs`; create `crates/hoimin-cli/tests/method_replacement_selection.rs` and fixture data under `crates/hoimin-cli/tests/fixtures/`; store allocation measurements under `docs/audits/2026-09-15-evaluation-order/`.

**Interfaces:** Tests invoke existing `analyze_source(&AnalyzeRequest, source)` and public `discover_targets`; new test-only counters are `METHOD_REPLACEMENT_HELPER_ENTRIES: Cell<[usize; 7]>` and `METHOD_REPLACEMENT_ALLOCATIONS: Cell<usize>`. Index order is append_insert, insert_append, append_extend, extend_append, get_subscript, rename, subscript_get.

- [ ] Build base debug library and run the existing allocator probe against its target directory; preserve baseline JSON.
- [ ] Add counters at seven helper entries and at `replace_within_call`'s call copy / mapping format expressions. Counters never depend on candidate emission.
- [ ] Write table-driven tests with sources `obj.append(1 + 2)`, `obj.insert(0, 1 + 2)`, `obj.extend([1 + 2])`, `obj.get(1 + 2)`, `(1 + 2).sort()`, `(1 + 2).reverse()`, `obj[1 + 2]`. Selecting only BinaryAddSub must yield one `+`→`-` and `(entries, allocations) == ([0; 7], 0)`; selected family must yield the hand-written replacement and one corresponding entry/allocation.
- [ ] Write independent append-family cases: CollectionAppendInsert yields `obj.insert(0, item)` and entries `[1,0,0,0,0,0,0]`; StructureAppendExtend yields `obj.extend([item])` and `[0,0,1,0,0,0,0]`.
- [ ] Add direct-call positive controls using parsed ExprCall/ExprSubscript and AstFacts; assert exact replacement and counter change. Add nested append depths `[1, 8, 16, 32]`, limit 1, only BinaryAddSub, zero method work.
- [ ] Capture full public candidate descriptors/IDs before guards and commit literal expected data. Assert each limit's candidates equal that baseline prefix and `truncated == (limit < baseline.len())`; include exact and above count, and zero.
- [ ] Run `cargo test --offline -p hoimin-cli --lib method_replacement_tests` and observe failures at nonzero helper counters; compatibility tests must already pass on base behavior.

### Task 2: Gate producers and verify behavior

**Files:** Modify `crates/hoimin-cli/src/analyzer/rust.rs`.

**Interfaces:** Uses existing `MutationOperatorSelection::contains(MutationOperator)`; consumes Task 1 counter tests and fixture.

- [ ] Add guards before helpers, e.g. `"append" if self.request.operators.contains(MutationOperator::CollectionAppendInsert) && has_supported_append_insert_arguments(call) => { ... }`. Apply corresponding operators from the spec table to every direction.
- [ ] Add `!self.request.operators.contains(MutationOperator::StructureMappingGetSubscript)` to subscript mapping eligibility after index/slice neighbor generation.
- [ ] Run Task 1 tests and `cargo test --offline -p hoimin-cli --test method_replacement_selection`; expect all pass.
- [ ] Review implementation three times: gate/operator mapping; traversal/eligibility/order; scope and production overhead. Review tests three times: negative/positive instrumentation; compatibility fixture/prefix; practical edge coverage. Fix findings and record each pass.
- [ ] Run `cargo fmt --all -- --check`, `cargo test --offline --workspace`, and `cargo clippy --offline --workspace --all-targets -- -D warnings`; report all failures if any.

### Task 3: Measure and finalize

**Files:** Allocation JSON and result notes in `docs/audits/2026-09-15-evaluation-order/`; review log and this plan.

**Interfaces:** Uses unchanged allocation probe with issue target supplied by environment or an explicit rustc invocation; no Python script changes required.

- [ ] Build updated library and repeat the same probe twice, asserting all eight candidate checks in each run. Save raw JSON; compare cumulative request totals with base and ignore control.
- [ ] Record environment, exact command and interpretation, explicitly distinguishing requested bytes from live bytes/RSS/timing.
- [ ] Mark plan steps and review log complete, commit implementation/tests/results, and send root commit IDs, checks and limitations. Do not push or create PR.
