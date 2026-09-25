# Issue 622 verification and review

The fingerprint now includes configured source-root order, separately from the set of source-content hashes. A reversed Python import search order starts a new run; identical configured order can still reuse conclusive results. Fingerprint schema 9 prevents old schema-8 sessions from matching without rewriting their database rows.

## Implementation self-review

1. Traced `RunConfig.selection.sources` into both the worker environment and `FingerprintInput::from_config`. The new field copies the same ordered configuration and does not sort it. Existing source-content and target set normalization is unchanged. Length/count framing under field 12 distinguishes roots and boundaries from import roots under field 9.
2. Reviewed schema and persistence. Bumped the explicit fingerprint schema to 9 and updated the pinned version test. Existing session lookup uses the new fingerprint; no SQL migration, deletion or result rewriting is introduced. Corrected documentation to explain that users may retain the same database while a new run executes. Jobs/max-output remain operational and all status reuse rules are unchanged.
3. Reviewed the complete diff and formal integration. The Lean import, executable, CI compilation/generator entries and Python closed registry agree. Generator order matches lakefile order. The public adapter consumes generated expectations rather than computing verdicts from source order. No new environment capture, path canonicalization or worker execution behavior was added.

## Test self-review

1. The new core regression failed before the change: AB and BA produced exactly the same digest. After the change all 22 resume-policy tests pass. Assertions also preserve same-order equality and distinguish roots with equal concatenated bytes but different boundaries. Existing property tests retain unordered-set invariants for file hashes and targets.
2. The eight-case corpus is a closed matrix: source/import-root kind × two initial orders × two resumed orders. Its adapter uses separate temporary roots and SQLite databases, actual public `run`, and real Python imports. It verifies baseline, exit, completion, both mutant statuses, executed count, termination metadata, stable candidate identity and same/different run ID. Changed-order cases are compared with fresh execution using a separate database.
3. Ran the complete Rust workspace and exact CI lint; no behavioral failure remains. Ran all 40 CI workflow contract tests to cover native generator registration as well as Rust consumption. Independent read-only review found no correctness blocker. The reviewer noted that the adapter uses synchronous subprocess output without a separate outer deadline; CLI runtime limits and CI job timeouts remain applicable. This is a test-harness limitation, not a model mismatch, and the adapter never treats an infrastructure failure as a semantic match.

## Evidence and reproduction

- Design and plan committed before code in `90e1201`, each with three substantive review passes.
- Core RED: `configured_source_root_order_changes_fingerprint` failed on digest inequality before the production change; GREEN: 22 tests passed.
- Public oracle: all eight cases match, plus four fresh-run comparisons. A generated case's expected values are never hand-edited.
- `cargo test --workspace`: exit 0, 2289 passed, 0 failed, 22 ignored, 96 suite summaries.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `.venv/bin/python -m unittest discover -s tests -p test_ci_workflow.py`: 40 passed.

Cargo commands used `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target/batch-resume`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=1`. Run the focused public adapter with `cargo test -p hoimin-cli --test lean_source_order_oracle`.

## Formal claim and limits

The promoted audit model proves compatible-order verdict preservation and fresh/resumed equivalence for two roots and two import orders. Its deliberately broken source-order-insensitive rule disagrees in both reversal directions. It does not prove Rust hashing, cryptographic collision resistance, arbitrary Python loaders, environment changes or concurrency. Actual implementation correspondence is limited to these public fixtures; there are no retained mismatches.

Run each command below from `formal/HoiminOracle`, sequentially and wrapped in `python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/source-order-stats.json --`:

```sh
lake build +HoiminOracle.SourceOrderModel:o
lake build +SourceOrderAuditMain:o
lake exe generate_source_order -- --check corpus/source-order.jsonl
lake exe generate_source_order -- --sensitivity
lake exe generate_source_order -- --stats
```

Generation uses `lake env lean -j1 -DElab.async=false --run SourceOrderAuditMain.lean --output corpus/source-order.jsonl`. All commands succeeded; each theorem retains 10000 heartbeats and all process guards remain 20 seconds / 2048 MiB. No timeout, memory breach or limit increase occurred. Resource measurements are recorded in the adjacent `2026-09-25-issue-622-source-order-resources.json`.
