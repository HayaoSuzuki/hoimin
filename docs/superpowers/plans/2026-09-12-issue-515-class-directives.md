# Issue 515 Class Directive Lookup Implementation Plan

> **For agentic workers:** use superpowers:executing-plans for this approved branch. Root coordinates Cargo, Git integration and publication; no further subagents.

**Goal:** prevent class global directives from bypassing bindings captured by descendant functions/comprehensions.

**Architecture:** skip indirect Class frames before directive dispatch; preserve direct Class and Function directive semantics. Correct the matching bounded shared Lean lookup model and compare ordinary closure cases through the public adapter.

**Tech Stack:** Rust/Ruff, public plan/run tests, CPython 3.14.7, existing Lean BindingFlow generator.

**Spec:** [issue 515 design](../specs/2026-09-12-issue-515-class-directives-design.md).

## Global constraints

- Worktree `.worktrees/issue-515`, base `61c654f`; no issue 514 production changes.
- Both builtin endpoints must remain definite; no changes to static locals, ordered facts, loop summaries, type-position suppression or public schemas.
- Tests before production; root runs RED/GREEN and broader Cargo checks. No Git mutations by the implementing subagent.
- Lean: serial guard, 30 s / 2048 MiB / 250 ms, `-j1`, `-DElab.async=false`; no manual corpus edits or new generator framework.

## Task 1: public regression and minimal runtime fix

Files: `crates/hoimin-cli/src/analyzer/rust.rs`, `crates/hoimin-cli/tests/type_parameter_bindings.rs`, `crates/hoimin-cli/tests/fixtures/type_parameter_bindings.py`, `crates/hoimin-cli/tests/run_e2e.rs`.

Interface: keep `resolve_scope(ScopeId, &str, bool, usize) -> NameResolution` unchanged.

- [x] Add five descendant negatives plus direct class, method-owned global and outer module positives using existing `assert_plan` span/ID checks. Add an ordinary closure case and the actual false-kill source to the existing run test.
- [x] Execute the CPython identity fixture; observed exit 0 on Python 3.14.7.
- [x] Root runs `cargo test -p hoimin-cli --test type_parameter_bindings class_global_directives --all-features` and `cargo test -p hoimin-cli --test run_e2e generic_type_parameter_destinations_cannot_create_false_kills --all-features`; confirm expected wrong candidates/false kill.
- [x] Insert the complete indirect Class skip before global/nonlocal dispatch:

```rust
if scope.kind == NameScopeKind::Class && !direct {
    return self.resolve_parent(scope.parent, name, offset);
}
```

- [x] Root reruns the two targets, then the complete `type_parameter_bindings` suite and analyzer library regressions. Require source preservation, baseline success, zero false kills, and retained positive candidates.

## Task 2: bounded model correction and oracle correspondence

Files: `formal/HoiminOracle/HoiminOracle/BindingFlowModel.lean`, `BindingFlowProofs.lean`, `BindingFlowCases.lean`, `formal/HoiminOracle/BindingFlowAuditMain.lean`, generated `formal/HoiminOracle/corpus/binding-flow-joins.jsonl`, `crates/hoimin-cli/tests/lean_binding_flow_oracle.rs`.

Interface: preserve the current schema 1 corpus; add ordinary closure scenarios using existing builtin-pair fields. The shared model remains without TypeParameters; generic evidence is not mislabeled as a strict model case.

- [x] Add a failing theorem/witness for an indirect class-global frame before a shadowing ordinary function frame; capture Lean RED under the resource guard before correcting model semantics.
- [x] Move the `.class` direct test before directive handling. Prove `resolveFrom name (frame :: rest) false = resolveFrom name rest false` when `frame.kind = .class`.
- [x] Add model-derived negative closure/global-class, direct-class-global positive, and method-owned-global positive corpus cases with explicit source/site/symbol correspondence. Add directive-order sensitivity while preserving existing broken-family checks.
- [x] Guarded build/regenerate/check/sensitivity with `generate_binding_flow`; request root Cargo adapter run. Review generated diff and preserve every old case.

## Task 3: evidence, OKF and final checks

Files: this plan, the issue spec, `docs/knowledge/design/analyzer.md`; add an issue report if the model evidence needs a separate worksheet/result artifact.

- [x] Record exact RED/GREEN/model commands, outcomes, guard costs, scope limits and three actual self-reviews of implementation/tests.
- [x] Update the existing analyzer OKF concept with the class-directive contract, matching source metadata and footnote; retain unknown metadata.
- [x] Parse concept YAML with a safe YAML parser, review links/footnotes and claim boundaries separately, and record three actual OKF reviews.
- [x] Request root-owned fmt/Clippy/full-workspace checks after focused tests pass. Report findings without claiming unrun native-platform coverage.

## Plan self-reviews

1. Coverage: each runtime requirement maps to Task 1's public/CPython/run checks; Task 2 addresses the actual existing model discrepancy rather than adding unrelated PEP 695 machinery.
2. Sequencing: RED gates precede both runtime and model fixes; Cargo and Git remain root-controlled. The five descendant negatives have two contrasting scope-owner positives, preventing a blanket directive skip from passing.
3. Interfaces and evidence: existing resolver and corpus schemas remain unchanged. The test helper isolates name mutations from literal mutations, and formal/generic observations are labeled separately. All new knowledge claims cite the local issue spec and retain draft status until final verification.

## Execution evidence

- Initial CPython identity fixture: `.venv/bin/python crates/hoimin-cli/tests/fixtures/type_parameter_bindings.py`, exit 0, `header/body/lazy identities OK`.
- Public RED requested from root after regression edits; production untouched at request time.


## Runtime and model results

The root ran the public RED and GREEN commands in this worktree using its serialized Cargo lane. This subagent ran no Cargo command and performed no Git mutation.

| Check | Observation | Evidence |
| --- | --- | --- |
| public class-directive RED | both added tests fail on extra method candidates | `/private/tmp/hoimin-reaudit/515-red-bindings.log` |
| actual run RED | killed count 1, expected 0; baseline succeeds | `/private/tmp/hoimin-reaudit/515-red-run.log` |
| full type-parameter GREEN | 8 passed, 0 failed | `/private/tmp/hoimin-reaudit/515-green-bindings.log` |
| actual run GREEN | 1 passed, 0 failed; 66 filtered | `/private/tmp/hoimin-reaudit/515-green-run.log` |
| CPython identity fixture | exit 0, five descendant captures plus direct/own-global controls | `crates/hoimin-cli/tests/fixtures/type_parameter_bindings.py` |
| old-model fixed witness RED | `decide` proves expected shadowed result false, exit 1 | `/private/tmp/hoimin-reaudit/515-class-directive-red.lean`, `515-lean-red.json` |
| corrected general class-skip theorem | targeted model/proof build exit 0 | `515-lean-proof-green.json` |
| model corpus | 28 rows; all 25 prior rows preserved byte-for-byte; three new strict cases | `corpus/binding-flow-joins.jsonl` |
| corpus freshness | exit 0 | `515-lean-check.json` |
| sensitivity | all six checks true, including class directive order | `515-lean-sensitivity.json` |
| finite existing audit | depth 2, 130 states/programs, 612 transitions, ceiling 1024, 28 fixed cases | `515-lean-stats.json` |

Each short JSON evidence name above is under `/private/tmp/hoimin-reaudit/`. The source cases, theorem, model, generator and Rust adapter are retained in the branch; temporary logs are session evidence rather than repository inputs.

### Exact guarded commands

Run from `formal/HoiminOracle`; each invocation used this guard prefix, with a distinct stats filename:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/hoimin-reaudit/515-lean-proof-green.json -- lake build +HoiminOracle.BindingFlowProofs
```

The commands after `--` were, in order: `lake build +HoiminOracle.BindingFlowModel` (base), `lake env lean -j1 -DElab.async=false /private/tmp/hoimin-reaudit/515-class-directive-red.lean` (expected RED), `lake build +HoiminOracle.BindingFlowProofs`, `lake build +HoiminOracle.BindingFlowCases`, `lake build generate_binding_flow`, `lake exe generate_binding_flow -- --output corpus/binding-flow-joins.jsonl`, `lake exe generate_binding_flow -- --check corpus/binding-flow-joins.jsonl`, `lake exe generate_binding_flow -- --sensitivity`, and `lake exe generate_binding_flow -- --stats 2`. `lakefile.toml` retains `-j1` and `-DElab.async=false`.

Initial sandboxed baseline guard returned exit 126, `monitor_error`, after 39 ms with no measured RSS. It was an infrastructure error from restricted process monitoring; the same command ran with ps access and unchanged resource limits. No automatic approval rejection occurred. Successful checks peaked at 783824 KiB RSS (generator build); longest elapsed time was 4932 ms. The model RED took 2719 ms / 590512 KiB and is a semantic counterexample, distinct from that monitor error. No timeout, memory-limit failure, raised bound, or aggregate all-model rebuild occurred.

## Implementation self-reviews

1. Traced the actual changed resolver branch after RED: indirect Class exits before both global and nonlocal; direct Class still uses ordered resolution and `resolve_class_parent`; Function and TypeParameters lookup are unchanged. No candidate-specific or name-specific conditions were introduced.
2. Reviewed the full runtime diff against identity observations: five descendant shapes capture the expected TypeVar; ordinary closure capture works; class body and method-owned global still resolve builtin. Static binding collection, comprehension traversal, loop backedges, annotation suppression and public schemas are untouched.
3. Reviewed model/production correspondence and final generated diff: model changes only indirect Class ordering, the theorem quantifies that exact rule, three ordinary-closure rows derive their eligibility from the model, and all old rows remain unchanged. Generic capture is still separate CPython evidence; no full Python semantics proof is claimed.

## Test self-reviews

1. RED witnesses exercise public candidate generation and actual run output rather than private implementation shape. Root observed the expected extra candidate and false kill before production edits; the CPython fixture independently identifies the bindings.
2. Checked positive controls against an overbroad fix: ignoring every global would suppress valid direct-class and method-owned-global candidates, while ignoring all generic scopes would incorrectly restore the negative cases. Literal collection candidates are filtered out by the established exact-name helper rather than suppressed in production.
3. Checked every new oracle row's Python compilation, unique site marker and expected qualified symbol; generated expectations remain solely Lean-owned. The existing adapter compares actual public candidates including operator/original/replacement/symbol, and setup failures retain infrastructure-error treatment. Corpus strict adapter execution is root-coordinated, not inferred from model compilation.

## OKF self-reviews

1. Parsed frontmatter with `yaml.safe_load`: all 16 knowledge Markdown files satisfy the repository's YAML/type and reserved-file checks. Kept existing draft status and unknown metadata; no human verification or timestamp was invented.
2. Checked the new spec path, matching source ID/footnote and SHA-256 in both analyzer concept and design-document index. The source is included in this branch as an untracked document pending root staging, not represented as an existing committed revision.
3. Read the added Japanese paragraph against the design and runtime diff: direct versus indirect scope and function-owned declarations remain explicit; ordinary closure model correspondence is distinguished from generic runtime observation. The index quotes the actual spec heading and the existing catalog entry point reaches both edited concepts.

## Root final verification and PR preparation

- `cargo test --workspace --all-features`: exit 0, 1746 passed / 13 ignored, 76 test-result groups. Includes the updated strict BindingFlow and comprehension adapters. Evidence: `/private/tmp/hoimin-reaudit/515-workspace.log`.
- `cargo fmt --all -- --check` and `git diff --check`: exit 0. Root ran strict all-target/all-feature Clippy; final adapter edits are included in the final check before commit.
- Root OKF validation: 16 pages, 356 source-footnote pairs, 676 local links; all concepts reachable, every design indexed, new source hashes match.
- Independent reviewer read runtime and final formal diff in three passes; independently executed CPython identities and all three new corpus originals/variants. Confirmed all 25 prior corpus rows unchanged and no blocking findings.

PR self-review 1: title/body reproduce the actual class-global trigger and explain the resulting binding behavior, with `Closes #515` and no unrelated issue changes.
PR self-review 2: validation distinguishes runtime, CPython, Lean model, finite cases and native-platform limits; no unexecuted CI result is called successful.
PR self-review 3: reviewed the complete branch diff and source/OKF references against the issue, checked the template fields and test logs, and retained exact-head CI confirmation as the merge prerequisite.
