# CI report_sequence fuzz-oracle correction

Base: `7c100e67086b5d8683f2959db40d876e662dce4c`. Scope: fuzz target and exact regression seed only. Original Linux CI run `39243267907`, job `118029625012`, rejected a correct Error-status round trip because the supposedly malformed wire changed Error to Error. Native replay platform: aarch64 macOS; no Linux rerun claim.

## Five requirements/design reviews

1. Failure provenance: original CI report_sequence.log identifies the rejection assertion, with bytes `[123,93,9]`; summary has 206.5965 seconds remaining and four preceding completed targets. This is an oracle failure, not timeout. Original crash SHA256 is `626c48424fa4331745a35d1cfe93fce1f85b4f6299422c9a6ddf68dcbc5e960c`. Original artifact files are retained unchanged; original-hashes.json records them.
2. Boundary contract: MutantFinished serializer emits outcome.status(), and its reader rejects contradictory valid statuses. With selector9 the generated output is CloseTimedOut and its canonical status is Error; writing error is a no-op. Production readers must remain unchanged.
3. Minimal correction: use the existing computed status; choose Killed for Error and Error for every other status. Both are valid enum values and always differ from the current status. This adds no classifier and preserves rejection rather than skipping Error outcomes.
4. Regression design: retain the exact original bytes as a named seed; reproduce RED before editing, then replay the identical bytes GREEN. Bounded independent byte fixtures cover all seven current statuses plus reused and close-timeout combinations; no permanent literal-mirror unit test or extra target is needed.
5. Operational scope: existing cargo-fuzz0.13.2 and nightly-2026-10-08 are available. Explicit --target-dir is necessary because cargo-fuzz has its own default; the active build uses the shared target and fuzz/target is absent. Sole Cargo owner, jobs2, tests2, per-run timeout/RSS limits, native-only evidence, mandatory hooks, no push/CI/cleanup.

## Five implementation reviews

1. Diff scope: only report_sequence imports/malformed-status mutation and the named seed changed. Lifecycle construction, round_trip, decoded records, sequence validation, and the parse rejection assertion remain intact; production readers are untouched.
2. Non-no-op invariant: the existing canonical status equals Error or does not; selecting Killed in the first branch and Error in the second contradicts every current enum variant. Using serialized enum values preserves valid wire spelling without copying classification rules.
3. Regression preservation: the named seed `error-status-close-timeout-regression` is copied byte-for-byte from the original artifact; size3 and SHA256 match the original. No input was suppressed or removed.
4. Domain fixture independence: temporary cases exercise first-byte parity, all six termination selectors, and all four outcome selectors (48 independent inputs). Inspection caught and corrected the temporary label for completed Cancelled from Error to NotRun before validation; close timeout still projects Error. These are hand-derived evidence fixtures, not new permanent tests or duplicated production classifiers.
5. Formatting/resource scope: own target imports were formatted. The manifest-wide formatter additionally reports an untouched pre-existing python_analyzer import-order failure; controller was informed before expanding scope. Explicit cargo-fuzz target choice worked (binary under shared target, no fuzz/target); no tool installation, extra target, budget change, product edit, subagent or CI dispatch occurred.

## Replay evidence

Original bytes reproduce the same rejection assertion before editing: cargo exit1/libFuzzer77; 61.83 seconds including ASan compilation, red-replay.log SHA256 `3a618e9404a4827883b743b8210abca11f66c551c65c94f727e9c864b834a5cd`. The identical input after correction completes with cargo exit0, green-replay.log SHA256 `42e63d6ce84e8b4a91306b4f4c8ffadfefb44346d1a54c44b1c5368e8c692fee`. No Linux execution is claimed. Exact argv, environment, timeout, elapsed time and log hashes are recorded in each named JSON receipt under `/tmp/hoimin-priority-six-ci-fuzz`.

Initial `cargo fmt --manifest-path fuzz/Cargo.toml -- --check` exited1 with own import wrapping plus the untouched pre-existing python_analyzer import ordering; own file was corrected using rustfmt. Controller instructed retaining the untouched python_analyzer file; final manifest-wide fmt still exits1 only for that file, proven byte-identical to base. Changed-target rustfmt check exits0. Bounded checks below are complete; normal commit hook results are recorded below. A process-inspection diagnostic (`ps`) was sandbox-denied; it is not used to claim process/memory measurements.


## Five validation reviews

1. Reproduction specificity: actual ASan fuzz execution on original artifact bytes fails before the edit at the same rejection assertion as Linux CI (cargo1/libFuzzer77) and succeeds after the edit (cargo0). The fixed replay runs the real target, not a status-literal mirror.
2. Deterministic domain and boundaries: actual target replay of 57 independent inputs passes (48 selector combinations, six named seeds including original regression, empty/max4096/oversized4097 boundaries). Hand-derived labels cover Killed, Survived, Timeout, OutOfMemory, ProcessLimit, Error and NotRun; each input executes independently. This demonstrates the supplied paths run without oracle crashes; the temporary expected labels are explanatory and are not asserted as a second classifier. The existing 119 report_status tests independently verify serializer/reader classifications and contradictions; all pass.
3. Build/static checks: nightly ASan target compilation, focused locked report_sequence clippy with warnings denied, changed-target rustfmt, and diff whitespace checks pass. Manifest-wide formatting fails solely for unchanged python_analyzer import ordering after own formatting correction; original/source snapshots and scope-baseline.json prove this predates the change. Controller explicitly retained it outside scope.
4. Bounded exploration/resource evidence: smoke ends after 10,000 runs in 3 seconds, exit0, with reported RSS 450 MB; limits are 10 seconds, 10,000 runs, 5 seconds/input, 4096-byte max and 1024 MB. Native atos symbolization emitted warnings but no target panic or sanitizer failure. Outer process-group timeouts and exact argv/env/log hashes are retained; no full local fuzz campaign was run.
5. Scope/platform assessment: changed tracked files are confined to target and exact seed; no product reader or workflow changes. Root native Rust jobs use the four-crate root workspace; report_sequence belongs to its separate fuzz workspace, and those root native jobs neither compile this target nor read its regression seed. Local native aarch64 macOS ASan execution is direct evidence for the fuzz change; it does not rerun Linux CI or establish new Windows evidence. CI all-target rerun and independent scoped review remain controller-owned. Normal commit hooks were enabled: all four hooks skipped by configured fuzz exclusion; no bypass occurred. Actual focused target checks above ran successfully, apart from the separately identified baseline-wide formatting issue.

## Commands and limits

Each command has a `<label>.json` receipt (complete argv, cwd, environment, timeout, elapsed, return code and log SHA256), and `<label>.log`, under `/tmp/hoimin-priority-six-ci-fuzz`. Relevant common environment: `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target RUST_TEST_THREADS=2 PREK_NO_CONCURRENCY=1 UV_PROJECT_ENVIRONMENT=/tmp/hoimin-priority-six-venv UV_CACHE_DIR=/tmp/hoimin-priority-six-uv-cache`, with the task venv bin prepended to PATH. All Cargo operations were sequential. Existing cargo-fuzz0.13.2, pinned nightly2026-10-08, native target aarch64-apple-darwin, default AddressSanitizer/debug assertions/overflow checks, codegen-units16. Explicit `--target-dir` confirmed execution under the shared target; repository `target` and `fuzz/target` remain absent.

- RED/GREEN: `cargo +nightly-2026-10-08 fuzz run --target-dir /tmp/hoimin-priority-six-target --codegen-units 16 report_sequence <original-artifact> -- -runs=1 -seed=588562252 -max_len=4096 -timeout=5 -rss_limit_mb=1024 -artifact_prefix=/tmp/hoimin-priority-six-ci-fuzz/` (outer480/180seconds).
- Deterministic cases: same command, with57 explicit input files instead of the artifact; exact paths in deterministic-cases.json (outer 180 seconds).
- Smoke: same target/toolchain/options, first corpus `/tmp/hoimin-priority-six-ci-fuzz/smoke-corpus`, then committed seeds and temporary case directory; `-max_total_time=10 -runs=10000` (outer90seconds). Only the first temporary corpus receives discovered files.
- Focused fuzz check: `cargo clippy --locked --manifest-path fuzz/Cargo.toml --bin report_sequence -- -D warnings` (outer300seconds).
- Existing boundary suite: `cargo test --locked -p hoimin-core --test report_status -- --test-threads=2` (outer180seconds;119/119passed).
- Format: `rustfmt --edition 2024 --check fuzz/fuzz_targets/report_sequence.rs` passes. `cargo fmt --manifest-path fuzz/Cargo.toml -- --check` fails only at the unchanged baseline python_analyzer file (outer60seconds).

No subagents, product changes, new tools/targets, broad workspace test rerun, push, CI dispatch or cleanup were performed. Temporary cases, source snapshots, original CI artifact and smoke corpus are retained.

| Check | Exit | Seconds | Log SHA256 |
| --- | --- | --- | --- |
| red-replay | 1 | 61.83 | `3a618e9404a4827883b743b8210abca11f66c551c65c94f727e9c864b834a5cd` |
| green-replay | 0 | 7.29 | `42e63d6ce84e8b4a91306b4f4c8ffadfefb44346d1a54c44b1c5368e8c692fee` |
| fuzz-fmt | 1 | 0.27 | `a672065c46ce4ec2bc126f705ea38119e7d60178ec39ff15f5f3901609ebd0ea` |
| format-target | 0 | 0.05 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| fuzz-clippy | 0 | 18.91 | `fa1f409dfbdd345fd1ffec58237fca0dbaf0da38d3ec67889ef0ba398b086fe4` |
| deterministic-cases | 0 | 7.48 | `7ffe2b8894cf6cf75b51fca84130a2188638b5330241c162a5c4649f259de919` |
| smoke | 0 | 3.68 | `0cb7035f48e487b24e79b524fff18f8162670779236795d09f80e1085a9b7b7b` |
| report-status | 0 | 0.43 | `507bc538ebd96ce6706d2beb5234df1cf96ddb23affc1f601a0ff151864c19c8` |
| target-fmt-check | 0 | 0.08 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| final-fuzz-fmt | 1 | 0.26 | `20ece4b4ddec150f0c4cff75ce70ef9a2f68337b951a5f40707f9bdbe09666ee` |

## Final commit receipt

Commit `be67c007d539842ad0e796e26eb2f54df18cab4e` (parent `7c100e67086b5d8683f2959db40d876e662dce4c`) contains exactly `fuzz/fuzz_targets/report_sequence.rs` and the 3-byte named regression seed. Normal pre-commit invocation completed with exit0; all four configured root format/clippy hooks reported Skipped (no files to check) because fuzz files are intentionally excluded. `PREK_NO_CONCURRENCY=1` and the bounded common Cargo/task-venv environment were present; hooks were not bypassed. Commit log SHA256 `dc05d30ebd3a1c009cc7e1a4a59c0d0b1c2523e3bbbff24efc0a8dc2a24c8b04`. Final `git diff HEAD^ HEAD --check` passes and `git status --porcelain=v1` is empty. The ignored SDD report remains outside the commit as requested.

Final target SHA256 `fa3a0770fef601e569748c676d1c4d7e22efd6cf502a62765d70adb620c6184f`; seed SHA256 `626c48424fa4331745a35d1cfe93fce1f85b4f6299422c9a6ddf68dcbc5e960c`; original artifacts remain unchanged. Final receipt, source/toolchain snapshots and check/log hash index are retained under `/tmp/hoimin-priority-six-ci-fuzz`. All 15 self-review entries are above. Independent scoped review, evidence archive and any eventual single push belong to the controller. No new native macOS/Windows workflow rerun is justified by the root-workspace input scope; this report claims only the actual local fuzz and focused status-suite checks.
