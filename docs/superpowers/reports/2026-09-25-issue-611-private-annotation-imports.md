# Issue 611 evidence and self-review

Base: `ba96e06` (#605 fix). Author self-reviews below are not independent approvals.

## Design reviews before code

1. Traced both endpoint routes into `annotation_import_stable` and reviewed Python's mangling rule. A private-name-only string filter would incorrectly reject trailing-dunder and underscore-only class positives; chose an inherited class prefix with explicit exceptions.
2. Reviewed reverse spelling and executed CPython private-global/nested-underscore probes. Found rejecting only raw private annotation roots leaves `_C__Alias` imports vulnerable to `__Alias` writes. Added conservative transformed-key write tracking and directive ownership, without claiming full flow normalization.
3. Parent challenged an early idea to reject canonical prefixes whenever a matching class existed anywhere. Agreed that unrelated classes must not taint imports. Restricted transformed-key taint to actual binding writes and their destination scope; preserve compiler context in lexical descendants and reset at nested classes.

## Plan reviews before code

1. Mapped issue acceptance cases and shared consumers. Added separate source/destination, reverse spelling and qualified-root regressions; all operators must traverse the common guard, not an operator-specific patch.
2. Compared the original 36-case safety oracle with the conservative design. Kept runtime permission and key/identity unchanged, added separate static count so clean private aliases may be suppressed without weakening soundness. Observer remains outside analyzed source to avoid invalidating imports.
3. Reviewed #605 integration results and CI executable registry. Included the Python registry from the outset, explicit baseline/public-run checks, finite closed coordinate validation, and bounded Lean commands. Parent owns independent review; preserve artifacts rather than skill cleanup.

## Execution ledger

Pre-flight: Task 2 consumes Task 1 only through public CLI; generated semantic expectations remain independent. No interface conflict.

Task 1 RED: all three initial regression tests failed on the unchanged #605 implementation. A second aggregate RED run enumerated all eight unsafe cases: original source/destination, reverse source/destination, both global spellings, nonlocal closure write and qualified private root. This avoids losing later witnesses behind the first failing assertion.

Task 2 RED: public plan emitted a clean private alias contrary to conservative eligibility; the original source overwrite public run reported killed=1 instead of zero. Independently replayed all 36 Python fixtures: every runtime dictionary key and annotation binding matched the unchanged Lean model.

Task 1 implementation ruling: the first guard only rejected raw private roots. A separate implementation review found that a private function parameter can shadow an explicitly mangled root, while `KnownImports` still retains the canonical import. Added a new regression and observed RED, then conservatively excluded both spellings under the actual inherited class prefix. This also prevents the unevaluated local-annotation fast path from certifying canonical private roots. A clean canonical alias inside that matching class context now has expected count zero; unrelated class prefixes and module scopes remain positive. This is a documented static precision decision; no runtime permission/key/identity expectation changed.

Task 1 GREEN: all five focused tests passed after this correction. Added header-context and unevaluated-local controls for the final workspace suite. A local CPython probe showed a generic class bound uses its enclosing private context, not the newly declared class prefix; kept header traversal unchanged and added the corresponding positive test instead of changing semantics on an assumption.

Task 2 GREEN: four public adapter tests pass. They cover 36 Lean cases (keys, binding, exact candidate metadata), six additional scope-routing controls and both original source/destination runs. Both runs execute an independent valid baseline, then report complete=true, killed=0 and no mutants. Scope-routing controls are ordinary public regression expectations, separate from the finite Lean model.

## Implementation self-reviews

1. **Compiler context and alternate spellings.** Traced scope creation through methods, nested functions, nested classes and generic headers. Found the private parameter edge above; fixed it with the stronger scoped conservative predicate after observing RED. Prefix inheritance intentionally differs from runtime class lookup; underscore-only nested classes reset the prefix. Header probe confirmed no new class prefix should be applied to its own bound.
2. **Write ownership and directives.** Read `tracks`, `record_import_write`, global/nonlocal handling and post-build nonlocal ownership resolution together. Raw private writes whose canonical key is imported are now tracked even when the raw spelling is absent from imports. Canonical writes use the existing permanent-instability sentinel and normalized directive spellings, so global writes reach the module and nonlocal writes reach the closure. Unrelated class-local writes do not affect a module import; a public positive pins this.
3. **Shared consumers and compatibility.** Re-read root resolution, destination spelling selection and the referenced-import rejection gate. Direct and qualified aliases use the shared gate; nullable add/remove, list/Sequence, set/AbstractSet, Mapping, and Iterable/Iterator depend on that route. Added shared-operator source tests. Retained #605 prepared namespace logic and existing raw tracking, documented conservative clean/private/canonical suppression rather than claiming full import-flow normalization.

## Test self-reviews

1. **Finite runtime independence.** Checked all 36 class/alias/endpoint/overwrite coordinates against independent CPython dictionary and annotation probes. Observation code remains outside the analyzed file. The original runtime model remains unchanged; static count is `allowed && key == alias`. Plain aliases, trailing-dunder aliases, underscore-only classes and absent-overwrite controls stay visible.
2. **Sensitivity and extra boundaries.** Confirmed three broken-model witnesses: distinct raw keys, class underscore stripping and trailing-dunder mangling. The retained corpus has four original unsafe raw-key witnesses. Added reverse/global/nonlocal/qualified/private-parameter/lexical controls and all-underscore nested reset, with public probes for six of these. Private-parameter regression was independently RED before the final guard, rather than inferred from a green suite.
3. **Adapter and maintenance gates.** Reviewed closed coordinate IDs, schema/mode/unknown-field/missing/duplicate rejection, byte spans, text/line/operator/symbol checks and explicit runtime permission for emitted pairs. Native generator freshness and sensitivity succeeded. CI module/executable/generator lists and Python registry include the new entry in lake order; 40 workflow contract tests pass. Public timeouts and launch/parse failures are infrastructure errors, not semantic mismatches.

## Formal result and limits

The issue's `mangle`, ordered `lookup`, `envFor`, runtime `allowed` and last-write theorem are retained. The new implication theorem proves conservative static eligibility entails runtime permission. This proves properties of the Lean model, not Rust or Python. The corpus is exactly 3 class names × 3 alias spellings × 2 endpoints × 2 overwrite settings = 36 strict cases, with maximum two writes, no transitions/concurrency/trace-depth search. Broken policies are model-only; atomicity and idempotency do not apply.

All Lean runs used the global slot, external 20 seconds / 2048 MiB and theorem-local heartbeat 10000. Successful model: 3.006 s / 685968 KiB peak RSS; generation: 3.031 s / 631168 KiB; native executable/freshness: 5.283 s / 802976 KiB; sensitivity: 0.573 s / 55792 KiB. Initial proof compilation used an unavailable lemma invocation; a first repair command had a wrong working-directory-relative path and re-ran that same failing source. Corrected the proof with `simp_all [eligible]`; no semantic change or bound increase. These are infrastructure/setup errors, not production counterexamples. No timeout or OOM occurred.

The runtime model excludes long identifiers, nested scopes, metaclasses, type parameters, string/future annotations and arbitrary dynamic writes; ordinary Rust/public controls cover selected scope cases beyond it. The implementation uses conservative exclusion rather than a complete Python mangling-normalized flow analysis. Clean private aliases and clean canonical aliases inside their matching compiler context may lose candidates; transformed-key writes may conservatively taint a later clean reimport. Unrelated class presence alone never invalidates another scope's import.

## Reproduction commands

From the repository root, using the assigned exclusive lane target:

```sh
export CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target/batch-analyzer
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1
cargo test -p hoimin-cli --lib private_annotation_import
cargo test -p hoimin-cli --test lean_private_annotation_import_oracle
cargo test --workspace
cargo fmt --all -- --check
cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings
.venv/bin/python -m unittest tests.test_ci_workflow -q
uv tool run --offline ruff format --check .
uv tool run --offline ruff check --no-fix .
```

From `formal/HoiminOracle`, reserve the global Lean slot and permit process-table RSS monitoring:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-611-lean-model-success.json -- lake build HoiminOracle.PrivateAnnotationImportModel
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-611-lean-generate.json -- lake env lean -j1 -DElab.async=false --run PrivateAnnotationImportAuditMain.lean --output corpus/private-annotation-import.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-611-lean-fresh.json -- lake exe generate_private_annotation_import -- --check corpus/private-annotation-import.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-611-lean-sensitive.json -- lake exe generate_private_annotation_import -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-611-lean-stats.json -- lake exe generate_private_annotation_import -- --stats
```

Minimal original source witness: save corpus row `C-__Alias-source-true`'s `source` as `subject.py` in an empty directory. Run `hoimin plan --root <directory> --file subject.py --operators type_sequence_iterable --allow-best-effort-memory --min-free-space 1B -- <absolute-python> -c 'import subject; assert subject.C.__annotations__["value"] == tuple[int]'`. Expected: no candidates. With `run --format json`, expected complete=true, killed=0, empty mutants. Destination witness is `C-__Alias-destination-true`, using a baseline assertion that value remains `typing.Sequence[int]`. Both are automated in the public adapter.

The corpus retains 18 runtime-allowed inputs, 14 statically emitted pairs and four original broken-raw-key witnesses. The four clean private cases are intentionally runtime-safe/static-suppressed. No Lean runtime expectation was changed to match production.

## Final gates

- Full workspace: exit 0, 97 reported result entries, 2318 passed / 0 failed / 22 ignored. This includes the six new unit regressions and four new public adapter tests.
- Exact workspace/all-targets/all-features Clippy: pass. It initially requested `clone_from` and flagged a 105-line adapter test; applied the equivalent clone spelling and extracted target-byte-offset calculation into a helper. After those lint changes, reran the complete library (695 passed, 12 ignored) and new public adapter (4 passed), both exit 0.
- Locked vendored parser Clippy, workspace/vendor fmt and `git diff --check`: pass.
- CI workflow contract: 40 passed. Full repository Ruff format/lint: pass (19 files), using the existing offline tool cache.
- Guarded model, generation, native freshness, sensitivity and stats: pass. Stats explicitly report 36 cases, 3 classes, 3 aliases, 2 endpoints, 2 overwrite choices, max_writes=2, transitions=0.

No unresolved semantic mismatch or deferred implementation finding remains in this scope. The explicit implementation decision is conservative exclusion of canonical private spellings under their matching active compiler context, supported by the private-parameter RED→GREEN regression. Parent owns independent review, publication and remote CI. Artifacts remain in `/tmp/hoimin-611-*` and the worktree formal build cache.
