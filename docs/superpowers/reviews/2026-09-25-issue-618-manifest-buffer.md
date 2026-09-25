# Issue 618 review and verification record

Design and implementation plan were committed as `34d10f7` before code/test changes; each records three self-review passes. The user authorized work through publication. The coordinator owns CI polling, merge and cleanup.

## Implementation self-review

1. Ownership check: both Value and PlanManifest deserialize into owned data. The added `drop(bytes)` occurs after successful conversion and before validation/awaits; no later expression refers to the input buffer. Invocation paths remain separately owned for issue 606 protection.
2. Diagnostic check: compared the production diff with its parent. It is exactly one drop statement; schema precheck, typed unknown-field rejection, header/config checks, selection, source/fingerprint comparison and candidate validation keep their order and error mapping. Error paths still release bytes by ordinary scope cleanup.
3. Scope and performance check: no deserializer, schema, ranking or filesystem behavior changes. The claim is early release of the input buffer, not a whole-process RSS or prepare-peak reduction. Independent ranking changes cannot invalidate the lifetime test's observation boundary. No production changes followed independent review.

## Test self-review

1. Fixture and public behavior: the public CLI dispatch creates an unedited plan with 24 valid list mutations and 256-KiB literals, exceeding 12 MiB of JSON. Public preparation and subprocess dry-run select 1 and 5 candidates, compare complete preview JSON plus saved IDs/ranks/bodies, and require an external test marker to remain absent.
2. Observation and cleanup: one dedicated process-global allocator tracks the unique manifest-sized allocation, follows realloc identity and observes its deallocation. The sole blocking worker is held before polling public prepare; Pending plus the unique allocation check proves parsing occurred before the queued source read. RAII releases the worker on assertion panic, and channel/subprocess waits are bounded. RED completed without deadlock and failed specifically on zero releases versus one.
3. Sensitivity and maintainability: buffer tracking is armed only around the first public future poll; pointer identity and exactly-one-match assertions reject unrelated allocation matches. Checking release before source reading is stronger than checking release at API return and does not depend on subsequent ranking allocations. Clippy's function-length finding was resolved by extracting fixture creation without changing the observation/assertion sequence. Its path-formatting finding was resolved by quoting the marker's string view, following existing fixtures. No lint exemption was added.

## Verification evidence

- RED: `cargo test -p hoimin-cli --test manifest_buffer_lifetime -- --test-threads=1`: exit 101; one failure, unique buffer found but release count was 0 instead of 1 at the suspended source read. Test finished in 1.36 seconds.
- GREEN: `cargo test -p hoimin-cli --test manifest_buffer_lifetime --test plan -- --test-threads=1`: exit 0; lifetime regression 1 passed, plan suite 86 passed and 1 ignored. This includes existing malformed-header/schema, unknown-field, candidate tampering, source/fingerprint and preview validation coverage.
- `cargo test --workspace -- --test-threads=1` on base `c7b7a5b`: exit 0; 2367 passed, 0 failed, 22 ignored across 105 test/doc-test binaries, including the final fixture helper/string-formatting changes.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`: exit 0.
- `cargo fmt --all -- --check`, `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check` and `git diff --check`: exit 0.

Cargo uses the assigned `target/batch-verify` cache with one build job, no dev/test debug information and no incremental builds. Only core/CLI artifacts were cleaned when switching worktrees. No Lean or Python code/test changes were needed.

## Independent review

The coordinator independently reviewed the production drop and the allocator/public test and found no blockers. Review covered owned deserialization, allocation uniqueness, realloc identity, the blocking gate's release on RED failure, bounded waits, preview identity/body/rank correspondence and absent execution marker. This evidence establishes early buffer release; it does not measure RSS or overall prepare peak.

## Rebase verification

Rebased unpublished implementation `a5183a6` and design `34d10f7` onto main `d2277bf`, yielding `d1ad63e` and `22d371e`. Both range-diff entries are unchanged (`=`); no conflicts occurred.

After cleaning only core/CLI artifacts, fresh lifetime/plan tests passed 87 tests with 1 ignored (1 lifetime and 86 plan tests). Both exact CI clippy commands, both format checks and diff checks passed on this final base. The complete 2367-test workspace result above belongs to the pre-rebase implementation; final-base checks cover the input lifetime and existing public plan validation behavior. No source changes followed these checks.
