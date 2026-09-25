# Issue 628 explicit environment fingerprint review

## Contract and implementation

Repeatable `--fingerprint-env NAME` on run/plan captures selected inherited values before worker rewriting. Portable ASCII names normalize with Unix case sensitivity or Windows uppercase, then sort/deduplicate. A version/platform-tagged, length-framed BLAKE3 input preserves unset versus empty and native Unix bytes/Windows UTF-16 units. Only canonical names and an aggregate digest enter normalized config, reports, sessions, and plans; captured plaintext is not added. This is not a promise of secrecy for low-entropy values.

Run fingerprint schema11 includes the declared names and digest under field13. Plan schema5 validates names/digest consistency and compares the current snapshot before baseline; report schema stays additive and ranking version remains4. Missing optional config fields decode as untracked. SQLite schema4 and the Issue624 budget/selection policy are unchanged. Older incomplete fingerprint schemas retain the existing `session.resume.incompatible` error when no compatible run is found.

Design and plan were independently reviewed and committed before code in `90567af`; each records three substantive review passes.

## Implementation self-reviews

1. **Capture/encoding/privacy.** Traced raw CLI names through core normalization, the CLI snapshot reader, incremental native hashing, and fingerprint field13. Values are transient OS strings passed to the hasher, never stored in config or error messages. Presence tags distinguish absent/empty; platform tags and raw Unix bytes/UTF-16 units avoid lossy conversion. Added validation at the reader boundary so direct library calls cannot send invalid/NUL names to `var_os`. Portable ASCII names make supported Windows casing exact without claiming Unicode name support.
2. **Prepared run/plan/session boundaries.** Followed owned and borrowed run entry points through preparation and verify through `run_verified`: the latter retains the validated snapshot rather than refreshing it. Plan metadata validates before OS lookup; snapshot mismatch precedes project work and the baseline. Existing source snapshot checks still own their filesystem TOCTOU contract. Regenerated only the current SQLite-v4 golden using actual `SessionHandler::begin/persist`; removed the temporary regeneration hook and strengthened semantic regeneration to compare every logical row. Historical v1–v3 artifacts remain unchanged.
3. **Platform/version/model boundaries.** Reviewed serde defaults/omission, normalized-config's additive JSON schema, fingerprint11 and plan5 assertions, and old-session error behavior. Updated current-version fixtures/assertions and corrected stale documentation that implied every old fingerprint silently starts a new run. The Lean proof establishes injective abstract presence encoding and tracked fresh-equivalence over four values; it does not establish BLAKE3 injectivity or prove OS capture/serialization. Windows native runtime remains unverified on this macOS host; cfg-gated UTF-16 tests are retained for Windows execution.

## Test self-reviews

1. **RED and semantic controls.** The public strict1→weak0 regression failed on the missing CLI option before production code. After implementation it starts a new run and agrees with an independently fresh weak control. Equal tracked values reuse; an untracked change intentionally preserves old behavior. Separate controls cover absent/empty in both directions, name addition/removal/replacement, order/duplicates, and invalid native Unix bytes. The initially flattened Lean encoding failed the absent→empty proof before correction.
2. **Persistence and error coverage.** Public plan tests check equal-value execution, changed-value exit2 before a marker-writing baseline, missing/malformed/detached digest metadata, duplicate/unsorted/invalid names, and old plan schema rejection. A unique marker value is absent from report JSON, diagnostics, generated session artifacts, and plan manifests; test code/argv do not contain that value. Injected-reader tests reject invalid names before lookup and avoid unsafe global environment mutation. Fixture-version failures exposed the required plan5/current-DB11 assertion updates; those were corrected without altering historical fixtures.
3. **Oracle correspondence.** The schema1 corpus contains32 unique tracked/untracked × four before × four after cases. Typed parsing rejects unknown fields, missing nullable fields, bad domains, duplicate IDs/input tuples, and incomplete case products. A final review strengthened termination comparison from only the `Exit` key to the whole null-or-exit object, rejecting unexpected termination variants; the strict oracle was rerun successfully afterward. The adapter creates real isolated SQLite sessions and observes public process status, run-ID reuse, candidate identity, execution metrics, termination, baseline, completeness, and both mutant outcomes. Expected observations come from Lean; subprocess/timeout/unexpected-infrastructure exits are separate from semantic comparisons. The four-value model excludes hash collisions, arbitrary bytes, Windows API behavior, and concurrent in-process environment mutation.

## Independent review

The parent agent reviewed production code and the model with no blockers: canonical host-case names, presence/native framing, names+digest field13, metadata validation before lookup, pre-baseline verify comparison, and the finite model boundary were consistent. The reviewer requested explicit reporting of unverified Windows runtime and the absence of a hash-injectivity proof; those limits are stated above.

## Verification evidence

Commands use `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target/batch-progress`, debug0, incremental0, and one Cargo job. Logs are under `/tmp/hoimin-batch-604-632/628-*`.

- Public focused tests:51 passed/1 ignored (environment6, plan environment3, import roots5, Lean adapter2 covering32 rows, session35). Existing plan suite:86 passed/1 ignored. Core lib/resume:51 passed/2 ignored. Native capture unit tests:3 passed.
- Workflow registry:40 Python unittests passed. Cached offline Ruff lint/format checks cover the changed registry constant without modifying the shared project environment. No production Python code changed; mutation-testing test modules would not be an appropriate target.
- Lean model/native entry/generation/freshness/sensitivity/stats passed under the exclusive20-second/2-GiB guard. All new proofs use10,000 heartbeats. Retained domain:4 values,2 tracking modes,2 invocations,32 cases. Maximum successful command:7.974 seconds; maximum sampled RSS:1,285,504 KiB. No bound increase occurred.
- Full workspace:2,409 passed/22 ignored across111 test groups, exit0. After the assertion-only termination strengthening, both oracle tests/all32 public cases passed again. Both exact CI formatting commands and the whitespace check passed. Both exact CI clippy commands passed. A clippy-driven extraction of the environment validator preserved its pre-baseline position; test-only field-order/empty-string style fixes followed. The final public environment/plan/oracle and existing plan suites passed97 tests/1 ignored after those changes.

## Reproduction

From `formal/HoiminOracle`, serialize each Lean command through `python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 250 --stats /tmp/environment-<name>.json -- ...`: model target `lake build +HoiminOracle.EnvironmentFingerprintModel:o`, entry `lake build +EnvironmentFingerprintAuditMain:o`, then `lake exe generate_environment_fingerprint --check corpus/environment-fingerprint.jsonl`, `--sensitivity`, and `--stats`. The corpus is generated only with the executable's `--output` option.

Run `cargo test -p hoimin-cli --test fingerprint_env --test fingerprint_env_plan --test lean_environment_fingerprint_oracle` for strict public behavior and correspondence. The implementation does not include the issue's temporary fingerprint-file workaround.


## Final main integration

After Issue624 merged, rebased only628's commits with `git rebase --onto origin/main 19ffb88`, onto main `2498cc1`. Range-diff confirmed identical design/implementation patches (`c8de9c6`/`c6c7ab1`). Integration tests passed141 cases/2 ignored, including environment, plan, import roots, real budget/session behavior and the32-case oracle; the merged CI registry passed40 tests.

Main's earlier manifest-buffer drop added a line to the verification function and triggered the100-line clippy cap. Consolidated existing config validation and the new environment comparison in `validate_current_plan_config`, preserving their order after manifest-header validation and before selection/project work. After that extraction, all98 environment/plan/oracle tests passed (1 ignored), both exact CI clippy commands and both format checks passed, and the diff whitespace check passed. The full-workspace result above predates this patch-identical rebase and semantics-preserving extraction; the focused final integration checks address the changed boundary. Lean model/corpus contents did not change during integration.
