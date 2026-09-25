# Issue 605 evidence and review

Base: `43989c2`. Worktree: issue-605. These are author self-reviews, not independent approvals.

## Design reviews before code

1. Read `AnnotationImports::resolved_name`, `spelling_for` and `annotation_import_stable`. Found both source and target already converge on the same checker, including attribute roots. Chose one guard there instead of separate operator-specific checks.
2. Compared builtin resolver precedence and #598's runtime nonlocal finding. Found putting the guard after nonlocal or explicit imports would incorrectly certify custom mapping reads. Placed it after lexical class skip/global redirect and before both paths; added these exclusions to policy.
3. Compared the issue's 16 runtime cases with static conservatism. Found exact candidate equality with the original runtime permission would force emission in empty custom mappings. Retained the original runtime formula unchanged and introduced a separate static eligibility projection with an implication proof; both values remain in corpus.

## Plan reviews before code

1. Mapped every acceptance item to tasks. Found the original integer destination is stronger than Shadow-only identity probes for false-kill regression; added that precise baseline/public-run case.
2. Read existing public adapter and guarded CI lists. Found a generator alone would not maintain correspondence. Added model import, executable, freshness/sensitivity, closed-ID validation and exact span/line/symbol checks.
3. Inspected local unevaluated annotation early return and class scope setup. The early return applies only to function-local declarations; preserved it. Added generic method and lexical positives to protect scope propagation, and required independent CPython observations rather than deriving expectations in Rust.

## Execution ledger

Pre-flight: Task 2 consumes Task 1's shared predicate only through public CLI; corpus expectations are independent. No interface conflict. Parent instruction supersedes skill cleanup/reviewer delegation: artifacts stay and root owns independent review.

Task 1 RED: `cargo test -p hoimin-cli --lib prepared_annotation_import` failed both direct-name and qualified-alias regressions: prepared class emitted one candidate, expected zero. Task 1 GREEN passed both tests after the five-line shared guard. All 12 scope/directive rows and two alias rows are exercised.

Task 2 RED: new public adapter failed the class-visible static policy and reproduced the original integer-destination `killed=1` false count. Separately, all 26 CPython identity probes already matched Lean. After the guard, public run passed; the first plan replay found an adapter metadata error: nested function symbol is `Subject.factory.inner`, not `Subject.factory`. Corrected the generator metadata, regenerated the corpus and verified freshness without changing identities, runtime permission or static eligibility. Final public adapter: 3 tests pass, including 26 isolated plans/identity probes and original public run (complete=true, killed=0, empty mutants).

Task 2 ruling: CI maintains a second executable registry in `tests/test_ci_workflow.py`, omitted by the initial plan. Root identified the same omission on prior branches; running unittest reproduced one failure in 40 tests. Added the one registry entry in lake order, then 40/40 passed. This is a test registry change only; no Python production logic or mutation campaign is involved. `pytest` is absent, so used the repository's unittest runner. No specification change.

## Implementation self-reviews

1. **Lookup precedence.** Re-read the complete stability loop and compare both builtin resolvers. The guard is after lexical class skipping and global redirect, before nonlocal and local/import ownership. That preserves per-name globals and prevents nonlocal or explicit imports from claiming arbitrary mapping trust. Function-local unevaluated annotations return before the loop exactly as before; those are not evaluated class annotations. No further production defect found.
2. **Shared consumers and scope construction.** Traced `resolved_name`, `spelling_for`, `contains_unstable_reference`, method annotations and type-parameter scopes. Both endpoints and qualified attribute roots use the shared check; TypeParameters preserve direct class visibility, while function scopes clear it. Public replay caught the nested-function symbol metadata defect above; corrected only that metadata. Unit controls pin generic method, lexical function and qualified-global behavior.
3. **Contract and integration.** Compared README against the implementation and issue acceptance conditions. Removed the old statement excluding import-alias analysis from preparation policy and documented explicit imports/nonlocal conservatism. Found the generator registry omission through root feedback and confirmed it locally with RED/GREEN. Model module, root import, executable, CI module/executable/freshness/sensitivity lists and Python registry now correspond.

## Test self-reviews

1. **Runtime premises and positive controls.** Replayed all 26 generated Python sources independently and compared identities only to Lean values. The original 16 cases remain intact, with module/lexical positives, all three injections at class/method sites, and empty prepared mapping runtime-positive/static-negative. Additional fixtures cover per-name globals, nonlocal, explicit imports, generic method, qualified aliases and integer destination. No model expectations were weakened.
2. **Adapter boundaries and sensitivity.** Reviewed schema, closed IDs, duplicates, missing rows, unknown fields, mode and marker checks; rejection test exercises schema/mode/IDs/unknown fields/missing/duplicate cases. Public plan restricts the target line, compares exact count/operator/text/span/line/symbol, and requires runtime permission for emitted pairs. Baseline is explicitly run even when public run has no candidates. Fixed witnesses detect source-only checks, destination-only checks and false lexical capture; atomicity/idempotency do not apply to this pure predicate.
3. **Freshness and evidence.** Checked generated corpus values against the generator, and ran the exact registered executable after correcting symbol metadata. The observed original run RED proves this is false-kill coverage, rather than only a synthetic static-count test. CPython crashes/timeouts and invalid public output have infrastructure-error diagnostics. Initial symbol mismatch is classified as adapter metadata error, absent pytest/ruff as tool setup limitations, never as semantic success.

## Formal claim and scope

The imported model proves `eligible` implies `allowed` under the explicit ordinary-namespace/no-injection premise, and `allowed` implies both runtime identities. It does not prove Rust, CPython, parsing or arbitrary metaclass implementations. Source/destination visibility and injection cover 32 Boolean valuations for the static implication and 26 public fixtures, including the historical 16. There is no transition alphabet, trace depth or state exploration. Fixtures are all strict; deliberately broken alternatives are model-only. No infrastructure error remains in the final public replay.

Limits were retained throughout: one Lean command globally, 20 seconds / 2048 MiB external guard, theorem-local heartbeat 10000. Initial model: 3.273 s / 679792 KiB peak RSS. Initial generation: 3.329 s / 631216 KiB. Sensitivity: 4.232 s / 685920 KiB. Final executable freshness after metadata correction: 6.083 s / 786752 KiB. All completed below bounds, no timeout/OOM/limit increase. Stats: fixed_cases=26, historical_cases=16, transitions=0, search_depth=0, sensitivity_families=3.

Arbitrary dynamic module mutation, future-string annotation evaluation, and full Python compiler semantics remain outside this formal model. Clean prepared mappings and explicit class imports are intentionally suppressed when visible; completeness is not promised. Runtime evidence is CPython 3.14 on macOS; remote CI belongs to parent publication.

## Reproduction commands

From the repository root (the target path is the assigned shared-run cache, exclusive to this lane):

```sh
export CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target/batch-analyzer
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1
cargo test -p hoimin-cli --lib prepared_annotation_import
cargo test -p hoimin-cli --test lean_prepared_annotation_import_oracle
cargo test --workspace
cargo fmt --all -- --check
cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings
.venv/bin/python -m unittest tests.test_ci_workflow -q
uv tool run --offline ruff format --check .
uv tool run --offline ruff check --no-fix .
```

From `formal/HoiminOracle`, with the global Lean slot held and process-table access available:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-605-lean-model.json -- lake build HoiminOracle.PreparedAnnotationImportModel
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-605-lean-generate-final.json -- lake env lean -j1 -DElab.async=false --run PreparedAnnotationImportAuditMain.lean --output corpus/prepared-annotation-import.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-605-lean-fresh-final.json -- lake exe generate_prepared_annotation_import -- --check corpus/prepared-annotation-import.jsonl
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-605-lean-sensitive.json -- lake env lean -j1 -DElab.async=false --run PreparedAnnotationImportAuditMain.lean --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/hoimin-605-lean-executable.json -- lake exe generate_prepared_annotation_import -- --stats
```

The smallest reproducer is corpus row `integer-destination`: write `source` to `subject.py` in an empty directory. `hoimin plan --root <directory> --file subject.py --operators type_sequence_iterable --allow-best-effort-memory --min-free-space 1B -- <absolute-python> -c 'import subject; assert subject.result'` must have no candidates. Switching to `run --format json` must yield complete=true, killed=0 and no mutants. The public adapter performs both operations plus an independent baseline.

## Final gates

- `cargo test --workspace`: exit 0; all nonignored tests pass, including both new unit tests and all three public oracle tests.
- Exact CI Clippy commands for workspace/all-targets/all-features and locked vendored parser: both exit 0, no warnings.
- Workspace and vendored parser rustfmt checks: exit 0.
- CI workflow unittest contract: 40 passed. Full repository Ruff formatting (19 files) and lint: pass using the existing offline uv tool cache; the shared `.venv` has no standalone Ruff installation.
- Guarded Lean model, executable generation, freshness and sensitivity: pass at the retained bounds above.
- `git diff --check`: pass.

No unresolved semantic mismatch or deferred implementation finding remains in this scope. The only plan adjustment was the additional existing CI registry, validated RED→GREEN. Root owns independent code review, PR publication and remote CI; those are not claimed by this author report.
