# Issue #989 — direct current default selection

Date: 2026-10-19 (local Asia/Tokyo). The parent implementation plan remains dated 2026-10-18.
Base: `2ec902ae2353c38f2da1a74cbf6552785387e42d`.

## Requirements and design reviews

1. **Observable policy:** The frozen issue reports no wrong defaults. `config.rs` currently constructs 43 plus 11, and enum `Ord` controls persisted order. Preserve the exact 54 canonical IDs in their present order; test a hand-written literal list rather than derive expectations from either production collection.
2. **API boundary:** `docs/compatibility.md:254` excludes internal Rust APIs from independent guarantees. Remove `all_legacy` without an adapter or manifest/version-floor change; root will record source migration and retained 0.12.0 floor in shared docs.
3. **Policy separation:** Catalog identity and selector-family membership differ from default membership. Keep both declarations unchanged; remove the stale historical-selection reference in their ownership comment. No registry or new constructor is required.
4. **Meaningful selection behavior:** Historical 43/50 cardinality tests preserve an obsolete grouping. Replace these with literal mixed explicit selections covering exclusion, serialized roundtrip, prepared verification, and actual verification. Keep individual opt-out candidate behavior for currently enabled operators.
5. **Analyzer scope:** Existing exact-record/string/profile tests call historical helpers to suppress unrelated new operators. Replace those helpers with explicit operator fixtures chosen by each scenario, retain descriptor/Lean expectations, and preserve #986 visitor integration shims.

## Plan reviews

1. **Order and independence:** Put the independent 54-name literal in core integration tests first; characterize names and serialized order on unchanged production. CLI continues testing consumer behavior. Accidental removal, opt-in addition, or enum-order drift must fail the literal assertion.
2. **Characterization exception:** The approved brief explicitly accepts unchanged-behavior characterization for this refactor. Run new tests before editing production; do not invent a red behavior change or source-text/compile-fail test solely to assert API removal.
3. **Fixture precision:** Use JSON-deserialized literal selections in analyzer test utilities rather than default-minus-eleven or a renamed 43-element constant. Each affected scenario supplies the operators it actually exercises; token-string tests reuse the existing token-operator fixture.
4. **Resources and ownership:** Sole Cargo slot is held by this task. All builds use the owned `/tmp/hoimin-issues-984-989/target`, jobs 2, no debug/incremental, offline, owned TMPDIR and two test threads. Shared `.venv` remains read-only; no installs.
5. **Integration and commit:** Run core operator selection, CLI default selection/direct-include analyzer, and library analyzer tests because its helper changes are separate compilation. Format and run normal serialized hooks. Stage only the five task code/test files and this record; preserve four existing shared documentation changes. Root owns final workspace and OKF checks.

Initial software-design score: 7.5/10. Failed diagnostics: implementation changes currently affect callers because the historical API leaks an obsolete policy; interface comments explain history rather than only present selection promises. Direct `Default` plus scenario-owned test fixtures removes both failures without adding interfaces.

## Implementation and verification

The current default now collects one explicit 54-variant array into the existing `BTreeSet`. The obsolete API and its historical documentation are removed. Catalog identity, variant declaration order, family definitions and serialization derives are unchanged. Fourteen analyzer scenarios now provide their own selected operator names; token/string cases reuse their existing explicit token-operator fixture. The malformed-syntax test still intentionally expands the complete catalog, starting from current Default; the #986 visitor shim is intact.

The tests were written and run before production edits as the brief's authorized unchanged-behavior characterization. Initial core test compilation failed with E0277 because pinned Serde supports fixed arrays only through 32 entries for this serialization path. The independent 54-name literal is now serialized through `as_slice()`; this is a fixture correction, not a behavioral RED. Corrected characterization passed 17 core tests, 3 CLI selection tests and 341 direct analyzer tests (3 existing ignored).

All Cargo commands used `TMPDIR=/tmp/hoimin-issues-984-989/tmp`, `CARGO_TARGET_DIR=/tmp/hoimin-issues-984-989/target`, `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=false`, `CARGO_NET_OFFLINE=true`. No simultaneous Cargo process was started. Commands ran from the isolated worktree.

| Command | Outcome | Log under `/tmp/hoimin-issues-984-989` |
| --- | --- | --- |
| `cargo test --locked -p hoimin-core --test operator_selection` before production edits, initial fixture | Exit 101; E0277 array Serialize trait bound, no tests executed | `task-3-characterization-core.log` |
| Same command after fixture-only correction, old production | Exit 0; 17 passed | `task-3-characterization-core-corrected.log` |
| `cargo test --locked -p hoimin-cli --test default_operator_selection --test rust_analyzer -- --test-threads=2` old production | Exit 0; 3 + 341 passed, 3 existing ignored | `task-3-characterization-cli.log` |
| `cargo fmt --all` | Exit 0 | Direct tool output |
| `cargo test --locked -p hoimin-core --test operator_selection` final production | Exit 0; 17 passed | `task-3-green-core.log` |
| `cargo test --locked -p hoimin-cli --test default_operator_selection --test rust_analyzer -- --test-threads=2` final production | Exit 0; 3 + 341 passed, 3 existing ignored | `task-3-green-cli.log` |
| `cargo test --locked -p hoimin-cli --lib analyzer:: -- --test-threads=2` | Exit 0; 379 passed, 13 existing ignored, 658 filtered out | `task-3-green-library-analyzer.log` |
| `git diff --check` | Exit 0 | Direct tool output |

## Implementation reviews

1. **Ownership and API:** Reviewed the complete config diff and searched crate sources: no `all_legacy`/`analyze_legacy` references remain. All default membership lives in `Default`; no compatibility wrapper, registry, adapter or new production API was introduced.
2. **Edge semantics and order:** The collection remains `BTreeSet<MutationOperator>`, and declaration-order `Ord`, include/exclude methods and Serde derives are unchanged. The independent literal checks names and serialized array order; before/after 17-test runs confirm exact membership and order. The source-array grouping cannot accidentally change persistence order.
3. **Regression scope:** Reviewed all fourteen replaced analyzer call-site fixtures against their sources and expected descriptors. Exact token records, focused-profile filtering, exception-pair overflow/selection, operator import references and Lean projections retain their expectations. String fixtures select the existing complete token list; boolean-pattern fixtures also select collection/bitwise operators relevant to the source. #986 direct-include shim is unchanged.
4. **Test independence:** Replaced 43/50 cardinality and default-minus-history expectations with fixed 54 names and literal current subsets. Core serialization includes one default operator, one opt-in type operator and one method operator, with literal canonical order; CLI preparation and execution retain exactly two selected operators. Found and corrected the Serde fixture-size issue before production edits.
5. **Complexity:** Removed obsolete historical construction instead of renaming it. Scenario-specific test selections replace the shared historical grouping without production selection helpers. Reflowed the family ownership comment after removing its history clause. Final software-design score: 10/10; all eight diagnostics pass within this scoped module change. The previously failing interface/leakage diagnostics are resolved by removing the obsolete API and documenting only independent present policies.

## Verification reviews

1. **Actual execution:** Read each exit code and summary. Characterization and post-change core/CLI runs are green, and library analyzer neighbors are green. The initial E0277 is recorded as a fixture compile failure, never a claimed production RED. Unchanged-behavior characterization follows the approved brief.
2. **Contract coverage:** Fixed literals check the observable default names and saved order. Separate scenarios check omitted raw selection, default exclusions, explicit inclusion/exclusion, mixed opt-in serialized roundtrip, and CLI prepare/verify execution. The latter checks baseline, survivor count, completion and restored source bytes.
3. **Sensitivity and limits:** Removing a default, adding an opt-in, or changing enum order would mismatch the literal names/JSON array. Expanding an explicit subset would mismatch literal core/CLI name arrays; widening exclusions would mismatch candidate descriptors. This is test-design review, not a claim of an additional executed mutation campaign; no artificial source-text/API-presence test was added.
4. **Platform and broader scope:** Tests ran on the current macOS workspace only. Existing ignored tests (3 direct analyzer, 13 library analyzer) were not run. Final workspace, platform/contract/release and OKF integration checks remain root-owned; no cross-platform or full-workspace success is claimed here. No `.venv` writes or dependency installation occurred.
5. **Artifact accuracy and staging:** Actual work date is 2026-10-19; plan remains dated 2026-10-18. The four pre-existing shared-doc changes are excluded from staging. Only five task source/test files plus this maintenance record belong in the task commit. Normal hooks use owned Cargo environment plus `PREK_NO_CONCURRENCY=1` and `PREK_HOME=/tmp/hoimin-issues-984-989/prek`; commit/hook outcome is recorded in the coordinator task report after execution.

## Shared contract/OKF proposal

Root owns shared documentation updates and validation. Suggested selection contract text:

> Current runtime defaults are defined directly as one explicit 54-operator selection, independent of the complete operator catalog and selector-family membership. Omitted `--operators` uses that selection; explicit operator lists remain exact after exclusions and keep their saved selection when prepared for verification. Default names and saved order are checked against a fixed literal expectation; scenario-specific analyzer fixtures do not depend on a historical default grouping.

Suggested compatibility notice:

> The internal Rust API `MutationOperatorSelection::all_legacy` has been removed. Internal callers should use `Default` for the current defaults or construct the explicit selection required by their use case. Current default54, CLI selection behavior and saved formats are unchanged, so the version floor remains 0.12.0 under the internal Rust API policy.

Cite `crates/hoimin-core/src/config.rs`, core `tests/operator_selection.rs`, CLI `tests/default_operator_selection.rs`, analyzer `rust_tests.rs` and this maintenance record in the selection OKF evidence. Existing historical release notices may remain historical; do not describe this refactor as default expansion.

## Commit-hook correction

The initial sandboxed commit could not create `.git/worktrees/.../index.lock`; the authorized commit was retried with normal sandbox escalation. The first executed hook set passed workspace/vendor formatting and vendor Clippy, but workspace Clippy rejected the expanded exception-pair test at 106 lines. Reusing that test's already literal selected-operator fixture through `analyze_selected` removed the duplicated `AnalyzeRequest` setup; expected outputs and production code were unchanged. The first staged whitespace check also found a trailing blank line in this new record, which was removed. Failed hook evidence is retained in `task-3-commit-hooks.log`; this is a test-fixture cleanup, not a skipped check. The corrected exception-pair scenario passed both compilation paths (1 library + 1 direct analyzer test), using `cargo test --locked -p hoimin-cli --lib --test rust_analyzer except_star_exception_type_pairs_keep_shared_selection_and_profile_behavior -- --test-threads=2` with the owned environment; output is retained in `task-3-green-exception-fixture.log`. `cargo fmt --all` and `git diff --check` also completed with exit 0 after the correction. The normal hook retry output belongs in `task-3-commit-final.log`.
