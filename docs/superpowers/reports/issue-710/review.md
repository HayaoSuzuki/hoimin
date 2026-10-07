# Issue 710 review and validation

Design and implementation plan each record five self-review passes in their own
files. This change uses a separate worktree based on the #709 investigation, whose
cargo artifacts were cleaned before #710 began.

## Implementation self-reviews

1. Group identity: inspect `(path, Option<start-line>)`; file diversity uses None, line diversity uses Some. Equal line numbers across paths stay distinct and multi-line text only contributes its start line.
2. Stable ordering: the HashMap only finds vector indices; first saved appearance creates groups and queues preserve saved rank. Higher score tiers open only after all active queues drain.
3. Paging and allocation: skip/take consume the full borrowed iterator before ID cloning. Only selected IDs are owned; exhausted groups are removed. Extended allocator guard covers unique-line groups and detects eager/streamed discarded-ID clones.
4. Compatibility: CLI opt-in plus shared resolver leave strict default, explicit IDs, file diversity, plan ranks, fingerprints and limits unchanged. Added report enum/human/current schemas; historical run-event-v3 stays unchanged. Older enum readers need updating to read new-policy reports.
5. Integration: preview and execution share ordering and selection metadata. Public CLI coverage checks budgets and CLI conflicts; existing runtime/preview boundary coverage includes truncated plans. README separates retained-subset scores, line reach and defect-detection claims.

## Test self-reviews

1. Red/green: initial public CLI test failed with unknown `line-diverse`, exit 2; after implementation the exact eight-candidate order and concatenated pages passed.
2. Edges: ten mutants on one start line plus two other groups, same line across files, multi-line original, higher/lower score tiers, drained groups, empty input, oversized counts, out-of-range and huge offsets. Deterministic repeat and complete output check no omissions/duplicates.
3. Independent expectations: Lean sorts by tier/occurrence/group-first-rank while Rust rotates queues. The public adapter maps exact path/line/column/operator/score observations to roles and checks all 93 generated cases on this 64-bit host. Parser rejects absent/duplicate/malformed/unknown-mode cases.
4. Sensitivity and scope: five broken models distinguish file-only grouping, cross-path line collision, ignored tiers, offset-before-grouping and take-before-offset. Generic Lean theorems prove permutation, unique-ID preservation and paging equivalence for the model; fairness/tier witnesses are finite. This is not an all-input proof of Rust correctness or process execution.
5. Trial integrity: fixed 3-mutant budgets, only normal killed/survived outcomes, successful baselines, complete reports, source preservation and stable IDs/statuses. Strong assertions kill all selected mutants. Timings are under concurrent load and fixtures are authored; no representative effectiveness or speedup claim.

## Corrections found during validation

- First workspace compile exposed an exhaustive policy match in the allocator test. Extended both its policy matrix and expectation to include line diversity (all its start lines are unique).
- First trial invocation rejected the probe's incorrect assumed baseline JSON shape. The actual contract is `{"Exit": 0}`; corrected only the probe before collecting final data. Failed attempts are excluded from observations.

- Schema validation through actual public output exposed the existing preview schema's stale plan version 4 (the producer and accepted plans already use 5). Updated that current-schema constant to 5; no runtime schema version changed.
- CI contract tests exposed missing guarded model/executable build entries and the fixed corpus mapping. Added all three; the relevant 152 Python tests pass.
- Full tests also caught a README exact-string assertion; retained its documented file-policy sentence while adding the new line-policy description.

## Formal evidence

Files: `HoiminOracle/LineDiverseModel.lean`, `LineDiverseAuditMain.lean` and
`corpus/line-diverse.jsonl` under `formal/HoiminOracle`.

```sh
lake env lean -j1 -DElab.async=false HoiminOracle/LineDiverseModel.lean
lake exe generate_line_diverse -- --check corpus/line-diverse.jsonl
lake exe generate_line_diverse -- --sensitivity
lake exe generate_line_diverse -- --stats
```

All Lean commands had an external 20-second process-group deadline. Kernel check
2.196s, artifact build 2.588s, executable+generation 5.188s, freshness 0.394s,
sensitivity 0.238s, stats 0.232s. Peak child RSS about 715 MiB. The repository guard
could not inspect `ps` in the sandbox (`monitor_error`); the fallback retained the
deadline and measured child RSS. No increased bounds, `sorry` or `native_decide`.
CI now checks the generated corpus and sensitivity controls.

## Independent review

A read-only reviewer checked CLI/defaults, ordering, schema compatibility, unit/CLI/allocator coverage and Lean/trial claims in five passes. No actionable correctness finding. The suggested direct schema check was added and caught the pre-existing plan-version constant described above; after correction, public JSON/JSONL/preview and human-policy checks pass. A second review covered those additions and progress input. No builds were delegated to that reviewer.

Root repeated the Lean kernel/freshness/sensitivity/stats commands: 2.999s / 0.269s / 0.176s / 0.172s, all successful under the same 20-second deadline. All 93 strict oracle cases ran on the 64-bit host; five sensitivity families were true.

## Final validation

- `cargo test --offline --workspace`: exit 0, 2,688 passed, 0 failed, 22 ignored across 144 test/doc-test groups. macOS/aarch64, repository Rust 1.98.1 and Python 3.14.7. This includes new public schema/progress tests, the 93-case oracle and extended allocation guard.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo fmt --all --check` and `git diff --check`: pass.
- `python -m pytest -q tests/test_ranked_plan_docs.py tests/test_ci_workflow.py`: 152 passed. Changed CI mapping passes repository Ruff check/format.
- Research trial passes the scoped Ruff E4/E7/E9/F/I rules and formatting; the reports directory is normally excluded from repository Python lint. Both complete trial scenarios and all six strengthened-policy executions passed.
- All 29 OKF Markdown files pass YAML/reserved-file structure checks. New source hashes, footnotes and report-relative links validated. Historical source hashes are preserved as historical evidence, not asserted current.

Rust checks used `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_INCREMENTAL=0`. Full remote CI, other platforms, real-project effectiveness
and statistical performance validation were not run locally. PR creation is the
integration endpoint; cargo and temporary build artifacts are cleaned afterwards.
