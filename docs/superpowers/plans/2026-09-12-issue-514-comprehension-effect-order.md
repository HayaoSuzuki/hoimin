# Issue 514 Comprehension Effect Order Implementation Plan

> **Execution:** root opened the serial implementation gate on 2026-09-13 after issue 513 completed. No subagents. Root coordinates Cargo, Git integration and publication.

**Goal:** restore valid builtin mutations in the eager first iterable while preserving later comprehension-write uncertainty.

**Architecture:** make each comprehension scope carry `outward_effect_after`, the entire expression's completion boundary. Rerouted named writes use the outermost skipped completion boundary for ordered facts while preserving immediate static/whole-scope/backedge updates.

**Tech Stack:** Rust/Ruff, existing public CLI tests, CPython 3.14.7, existing bounded Lean comprehension-binding generator.

**Spec:** [issue 514 design](../specs/2026-09-12-issue-514-comprehension-effect-order-design.md).

## Global constraints

- The documentation-only stage ended on 2026-09-13. Preserve its historical evidence below; implementation now runs in this issue worktree.
- Worktree `.worktrees/issue-514`, design base `61c654f`; preserve issue 515 when integrated, but do not require it for this fix.
- No changes to public schemas, candidate spans/IDs, static local semantics, loop summaries, directive routing, or type-position suppression.
- Root runs Cargo RED before production edits, then GREEN and broad checks in the shared lane.
- Lean runs serially under the repository guard: 30 s wall / 2048 MiB RSS / 250 ms sampling, `-j1`, `-DElab.async=false`. No manually edited generated expectations or new generator framework.

## Task 1: public RED witnesses

Files: `crates/hoimin-cli/tests/comprehension_named_bindings.rs`; compact resolution-state cases may be added in `crates/hoimin-cli/src/analyzer/rust_tests.rs` using the existing `name_resolution_test_snapshot` helper.

Interfaces: reuse `check(source, candidates)` for collection_any_all, its bounded `output` helper, and exact name/span assertions. Keep only one any call in each new positive source.

- [x] Add `first_iterable_precedes_body_named_bindings` covering these four expressions, replacing NAME with both any and all:

```python
[(NAME := lambda values: "custom", item)[1] for item in [any((0, 1))]]
{(NAME := lambda values: "custom", item)[1] for item in [any((0, 1))]}
{item: (NAME := lambda values: "custom") for item in [any((0, 1))]}
((NAME := lambda values: "custom", item)[1] for item in [any((0, 1))])
```

For each, source is `values = EXPR\nassert next(iter(values)) is True\n`. Expect one any-to-all candidate. Original CPython exits 0; a call-only mutation `any((0, 1)) -> all((0, 1))` exits nonzero on the assertion. Compare the candidate's byte span to that unique call's name, and derive its stable ID from the existing identity API.

- [x] Add separate unconsumed-generator and already-shadowed-module controls. Do not use a failing original as a positive candidate fixture.
- [x] Request root RED: `cargo test -p hoimin-cli --test comprehension_named_bindings first_iterable_precedes_body_named_bindings --all-features`. Expected failure is zero candidates versus one, not a fixture compilation or process error.

## Task 2: explicit outward completion boundary

File: `crates/hoimin-cli/src/analyzer/rust.rs` only.

Private interface:

```rust
NameScopeKind::Comprehension { outward_effect_after: usize }

fn visit_comprehension_expression(
    &mut self,
    generators: &[ruff_python_ast::Comprehension],
    outward_effect_after: usize,
    result: impl FnOnce(&mut Self),
)
```

- [x] Add the scope payload; adapt every Comprehension match/equality in the name resolver to a payload-aware pattern. Do not change other scope semantics.
- [x] Pass `usize::from(comprehension.range.end())` from list/set/dict/generator expression branches. Keep first.iter traversal before creating/entering the scope.
- [x] In `record_named_target`, start `effect_after` at the named-expression end and update it while skipping comprehension parents:

```rust
while let NameScopeKind::Comprehension { outward_effect_after } =
    self.index.scopes[destination.0].kind
{
    effect_after = effect_after.max(outward_effect_after);
    destination = self.index.scopes[destination.0]
        .parent.expect("comprehension has a containing scope");
}
```

Call the existing `record_target(target, effect_after)` in the destination scope under the current conditional-depth discipline. Do not defer the call itself: locals, possible_bindings and loop summaries must update during index construction.

- [x] Request root GREEN for the focused test and then the complete `comprehension_named_bindings` target. Confirm all four forms and both endpoints execute, rather than stopping after the first table row.

## Task 3: scope/flow regressions and actual run

Files: `crates/hoimin-cli/tests/comprehension_named_bindings.rs`, `crates/hoimin-cli/src/analyzer/rust_tests.rs`.

- [x] Add or retain these exact control shapes:

```python
# Static local: no candidate; the test harness catches the expected error.
def f():
    return [(any := 0) for item in [any((0, 1))]]
try:
    f()
except UnboundLocalError:
    pass
else:
    raise AssertionError("expected static local")
```

```python
# A previous iteration can already have rebound the destination: no candidate.
for _ in range(2):
    values = [(all := lambda values: True, item)[1]
              for item in [any((0, 1))]]
```

```python
# A completed sibling expression has published its summary: no candidate.
values = ([(all := lambda values: True) for _ in range(1)], any((0, 1)))
```

```python
# A nested produced body must not contaminate the outer first iterable.
values = [[(all := lambda values: True, item)[1] for _ in range(1)]
          for item in [any((0, 1))]]
assert values == [[True]]
```

Also retain existing empty/lazy, delayed reset/import/delete, global/nonlocal, lambda-boundary, iteration-nonleakage and ordinary RHS tests; add consumed-generator outer-loop coverage. Representative list/tuple, min/max and sorted/reversed first-iterable cases use exact name filtering to exclude independent literal mutations.

- [x] Add `first_iterable_mutation_is_detected_by_real_run` using the first list witness and the existing public run setup: one job, bounded baseline/total execution, JSON result. Require baseline Exit(0), one any-to-all mutant at the first iterable, killed, complete report, and unchanged source bytes.
- [x] Root runs `cargo test -p hoimin-cli --test comprehension_named_bindings --test type_parameter_bindings --all-features` and `cargo test -p hoimin-cli --lib --all-features`. No code-level counter or candidate special case is accepted to make these tests pass.

## Task 4: bounded expression-summary oracle

Files: `formal/HoiminOracle/HoiminOracle/ComprehensionBindingModel.lean`, `formal/HoiminOracle/ComprehensionBindingAuditMain.lean`, generated `formal/HoiminOracle/corpus/comprehension-bindings.jsonl`, `crates/hoimin-cli/tests/comprehension_named_bindings.rs`.

Interfaces: preserve schema 1 and the existing generated fields. Each source row has exactly one candidate observation; keep all 11 prior rows. Add seven cases from the spec, yielding 18 rows / 8 positive cases.

- [x] Add a small expression-summary IR with observation, sequence and comprehension(firstIterable, outwardWrites). Use the existing routing functions to identify owners. Separate static/whole declaration facts from ordered-before facts.
- [x] Before correcting modeled ordering, run a fixed first-iterable witness with publication placed first; it must produce the wrong expected eligibility. Retain that behavior only as a broken sensitivity variant.
- [x] Implement interpretation as firstIterable before outward publication. Static function facts are established before either step; generator publication remains a possible write at creation.
- [x] Prove first-iterable observation independence from only the current body's summary, static-shadow preservation and no invented builtin knowledge after publication. Keep arbitrary abstract nesting model-only.
- [x] Add the seven explicit source rows and derive expected booleans through interpretation. Update the consumer's exact counts and validate original compilation, unique candidate site, source span, stable ID and retained old routing cases.
- [x] Under the resource guard, build only affected model/executable dependencies, run `generate_comprehension_bindings -- --output`, `--check`, `--sensitivity`, and `--stats`. Record domain size, elapsed/RSS, broken variants and any infrastructure failures without raising limits.
- [x] Request root public adapter checks plus existing `lean_binding_flow_oracle` regressions. No CI inventory change is needed while reusing the existing generator; if a new executable becomes necessary, stop for design review rather than silently expanding this task.

## Task 5: final evidence and integration

Files: this plan, issue spec, `docs/knowledge/design/analyzer.md`, `docs/knowledge/references/design-documents.md`.

- [x] Record runtime/model RED and GREEN, exact guarded commands and proof/corpus/CPython boundaries. Add three actual implementation and test self-reviews after execution.
- [x] Refresh both OKF source hashes if the spec changes. Parse all knowledge frontmatter with safe YAML, check new footnotes/links, and preserve unknown metadata/draft status.
- [x] Request root fmt, Clippy and full-workspace checks after focused tests pass. Root owns commit, PR and integration; after integration preserve #515's indirect-Class rule and re-run both issue witnesses.

## Plan self-reviews

1. Checked each accepted design boundary against an explicit task: Task 2 changes only ordered effect visibility; Task 3 protects static locals, siblings, nesting, delayed writes and backedges. All four syntax forms pass the full expression end into the same helper.
2. Checked RED/GREEN sequencing and exact fixtures against the current helper's successful-original requirement. The static-local fixture catches UnboundLocalError, the first-iterable fixture has one any call, and the rejected nested-walrus-in-iterable spelling is absent from the executable matrix.
3. Checked private signatures, model scope and consumer counts. Existing candidate identity APIs and schema 1 are retained, with seven specified additions rather than an unspecified corpus expansion. Root owns the serial gate; this document does not claim implementation/test/model execution.

## Documentation-stage evidence and OKF self-reviews

Only this plan, the spec and the two OKF concepts were edited during preparation. No production/tests, Cargo, Lean, Git or subagent operations occurred.

1. Format review: retain the existing OKF v0.2 bundle, safe-parse YAML, and preserve draft status and unknown metadata. No unobserved reviewer identity or verification timestamp is added.
2. Provenance review: both edited concepts include issue 514's source ID, matching footnote, actual relative spec link and SHA-256; the design-document table quotes the new spec's exact heading. The source is marked untracked pending root integration.
3. Claim review: the Japanese summary describes the selected design and retained uncertainty; it does not state the unimplemented fix has passed. The spec distinguishes base-revision audit evidence, future runtime verification and bounded formal claims.

Documentation validation used `.venv/bin/python` with `yaml.safe_load`: 16 knowledge Markdown files passed YAML/type/reserved-file checks. Both modified concepts' local Markdown links resolve; issue 514 source hashes and matching footnotes, the quoted spec heading, Markdown fence balance and trailing whitespace checks passed. These checks validate documentation structure/provenance, not runtime behavior or Lean semantics. No implementation tests were run.

## Execution evidence (2026-09-13)

Integrated main `820ff2a`, which includes issues 513 and 515. Autostash application conflicted only in the two OKF concepts; both source lists, text and footnotes were retained. The indirect-Class rule is unchanged. Commands below run from this worktree unless stated otherwise.

### Reproduction and public correspondence

Use `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target` before each Cargo command.

- RED before production edits: `cargo test --offline -p hoimin-cli --test comprehension_named_bindings first_iterable_precedes_body_named_bindings --all-features` failed with zero candidates versus one on the first valid list/source fixture.
- GREEN: the complete comprehension target passed all 15 tests, including all eight syntax/endpoint witnesses and a real run with baseline Exit(0), exactly one killed first-iterable mutation, complete report and unchanged source.
- `cargo test --offline -p hoimin-cli --test comprehension_named_bindings --test type_parameter_bindings --test lean_binding_flow_oracle --all-features`: 15 + 8 + 4 tests passed, including issue 515's class directives and the 18-row comprehension corpus (eight positives).
- A first version of the other-operator regression counted the independent tuple-literal mutation and failed 2 versus 1. The helper now selects exact original name candidates and validates their spans and IDs; it does not suppress literal candidates in production.

### Lean model and safety

From `formal/HoiminOracle`, every command used this prefix:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/issue514-lean-CHECK.json --
```

Lake retains `-j1 -DElab.async=false`. Executed suffixes, serially:

```sh
lake build generate_comprehension_bindings
lake exe generate_comprehension_bindings -- --output corpus/comprehension-bindings.jsonl
lake exe generate_comprehension_bindings -- --check corpus/comprehension-bindings.jsonl
lake exe generate_comprehension_bindings -- --sensitivity
lake exe generate_comprehension_bindings -- --stats
```

All passed. The finite domain has 18 explicit source rows, maximum four frames, eight eligible cases, and exactly one observation per row. It does not enumerate execution traces or loop transitions. The three summary operations are observation, sequence and comprehension, with an empty identity node. Existing routing sensitivity remains, supplemented by early publication, omitted static declaration, omitted sibling publication, and omitted zero/lazy writes.

A separate guarded `lake env lean -j1 -DElab.async=false /private/tmp/Issue514Red.lean` asserted:

```lean
import HoiminOracle.ComprehensionBindingModel
open HoiminOracle.BindingFlow HoiminOracle.ComprehensionBinding
example : (interpret (namedWrite .destination [comp, module]) (.observe 1)).1 = [true] := by decide
```

It failed because `decide` proved the proposition false. The committed `early_publication_detected` theorem proves the correct first-iterable result true and the prematurely published result false. This negative check was run after building the corrected model, rather than changing the production interpreter back to the broken ordering.

The first sandboxed guard invocation was an infrastructure failure (`monitor_error`, exit 126, RSS unobserved), because process monitoring was denied. Re-ran outside the sandbox with the same limits. Initial model builds exposed syntax and incomplete simplification errors; they were corrected without raising resource limits. No timeouts or memory-limit failures occurred. Final successful build took 4.966 s, peak aggregate RSS 790,320 KiB; generation 1.169 s / 57,088 KiB; freshness 1.046 s / 87,264 KiB. The deliberate RED check took 2.758 s / 603,616 KiB. Sensitivity and stats timings are retained in the local guard records.

### Implementation self-reviews

1. Traced all four visitors and every payload-aware scope match. Only the outward ordered boundary changes; static locals, possible bindings and backedges still update immediately. Nested scopes take the maximum boundary and stop at functions/lambdas. Preserved the integrated class-directive fix.
2. Reviewed the tests against actual Python validity and operator output. Fixed independent literal counting, required exact first-iterable spans, retained source/destination and empty/lazy/reset/backedge controls, and checked a real mutation rather than only plan output.
3. Reviewed model-to-source correspondence: retained the old routing rows, generated all new expectations from interpretation, required one observed call per row, and preserved the distinction between fixed-declaration proofs and CPython/Rust observations. The model does not claim callback, general-loop or generator-scheduling proofs.

### Final checks

- `cargo test --offline --workspace --all-features`: 1,756 passed, zero failed, 13 ignored across 75 result groups. This includes the library and integrated issue 513/515 regression suites.
- After strengthening exact name/span checks and adding global/nonlocal, parameter and lambda-default fixtures, re-ran `cargo test --offline -p hoimin-cli --test comprehension_named_bindings --all-features`: all 15 passed.
- `cargo fmt --all -- --check` and `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: passed.
- `.venv/bin/python -m unittest discover -s tests -p 'test_*.py' -v`: all 66 passed in 9.085 s. No production Python was modified, so no Python mutation run was applicable.
- Lean sensitivity: 0.573 s / 56,400 KiB; stats: 0.561 s / 54,448 KiB. All guarded successful runs remained below the unchanged limits.
- OKF review 1: all 16 knowledge Markdown documents passed safe-YAML and required type/version checks; draft status and unknown metadata were preserved.
- OKF review 2: both issue 514 sources have the actual spec SHA-256, matching footnotes and resolving local links; every spec remains in the design index. Historical source read-state metadata was retained.
- OKF review 3: the analyzer claim distinguishes the completed Rust behavior, finite CPython/CLI checks and model-only proofs. No claim of a general Rust/Python proof or new platform testing was added.

The earlier documentation-only restrictions and evidence remain historical. This execution used no subagents, retained the requested serial Lean guard, and completed three implementation self-reviews above. No CI inventory, public schema or generator framework change was necessary.
