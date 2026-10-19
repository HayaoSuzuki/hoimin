# Task 1 (#1006) implementation report

## Requirements/design reviews against ca9afb9

1. Raw API semantics: rust.rs:820 accepts arbitrary inclusive u32 ranges; empty means all while reversed ranges cannot match. Prefix maxima use no end+1, preserving zero/MAX and nested unions.
2. Producer boundary: make_candidate is reached by token, AstCandidateCollector::add_candidate_in_scope, and annotation producer. expression_selected has a separate early builder gate. One prepared owner must reach all four boundaries; source-position LineIndex remains separate.
3. Error order: analyze_source_with_exceptions checks cancellation, hierarchy/source matching, hierarchy construction and parse diagnostics before candidate selection. Preparation goes after successful parsing/facts and existing cancellation check, preserving diagnostic priority.
4. Payload/order: candidate admission precedes original copying and payload.generated; selection storage must not consume candidate payload. CandidatePrefix merge/sort/truncation code stays untouched. Existing payload and whole-expression tests provide controls; output identity gets separate comparisons.
5. Complexity/evidence: instrument actual old predicate comparisons and query count, run each sparse N independently, then retain same bounded operation gate on indexed queries. Frozen release receipt identifies 34e33490. Time/heap/RSS are separate measurements; public CLI line and changed scopes need explicit end-to-end checks.

## Progress

Clean baseline library suite completed before instrumentation/product edits, with the required bounded Cargo environment.

## TDD evidence to date

- Clean baseline: `cargo test --locked -p hoimin-python --lib`, 423 passed/3 ignored (baseline-tests.log).
- RED: `cargo test --locked -p hoimin-python selection_ -- --nocapture`, 22 passed/4 expected sparse failures (red.log). Actual old comparisons at N=1000/2000/4000/8000: 1,000,000/4,000,000/16,000,000/64,000,000; actual queries equal N. Counter directly wrapped the production `.any` predicate. Instrumentation-only old source removed with replacement; no runtime fallback retained.
- Expanded preservation controls before production edit: `cargo test --locked -p hoimin-python rust::selection_tests -- --nocapture`, 24 passed/4 sparse failures (red-controls.log). All named boundaries, symbols, 192 arbitrary-u32 property cases, and three producer cases passed on old implementation. A transient unused test instrumentation helper warning is confined to RED; GREEN uses the helper.
- Initial GREEN: `cargo test --locked -p hoimin-python selection -- --nocapture`, 53 passed. Indexed membership comparisons 11,999/25,999/55,999/119,999, preparation 2,999/5,999/11,999/23,999. The bound allows Rust partition_point's extra terminating comparison plus predecessor end check: Q*(floor(log2 R)+3), with Q=N asserted. Original RED bound was one comparison tighter; all old sparse counts also exceed the final conservative bound by orders of magnitude. Preparation counts include actual copied ranges, sort comparator calls and prefix steps.

## Implementation self-reviews

1. Ownership/call sites: exact diff threads one prepared reference through token, AST and annotation admission, plus expression builder gates. No public request fields changed and old linear fallback removed.
2. Union correctness: sorted starts and prefix-maximum ends answer the union at predecessor; reversed ranges filtered but Restricted(empty) retained. No arithmetic on end+1; named zero/MAX/nested/gap cases and independent oracle cover the invariant.
3. Cancellation/error ordering: owner construction follows successful parse/facts and original cancellation check. Check entry, each copied interval, post-sort and each prefix element; no inconsistent interrupting comparator. Added cancellation tests for entry/copy/post-sort/prefix and syntax precedence.
4. Resource accounting: only nonempty raw selectors allocate, exactly one Vec capacity R; selected candidate leases/original copy boundary and CandidatePrefix code unchanged. Preparation storage is separate from logical payload; All fast path remains allocation-free.
5. Scope/maintainability: private selection module contains admission metadata and documented complexity; source-position index and symbols' matching algorithm preserved. Existing rust.rs is large (~8,800 lines), but change is narrowly threaded references and extracted predicate. Additional argument lints will document why metadata is independent.

## Validation self-reviews (five distinct passes)

1. Boundary/preservation: final focused selection run59 passed, including independent192-case arbitrary-u32 membership property, hand-derived zero/MAX/reversed/nested/adjacent/gap cases, symbols and all three producers. Full final hoimin-python library457 passed/3 pre-existing ignored. No warnings in GREEN logs. Baseline423 passed/3 ignored;34 new unit tests account for the difference.
2. Budget/cancellation/identity: payload27 and cancellation30 focused tests passed. Exact candidate+identity charge boundary, boundary+one and zero-budget tests are separate from heap evidence. New public CLI3 passed; equivalent selector unions compare complete candidate objects (IDs/ranks/spans/text/order) with truncation=true. Direct raw API range equivalence asserts exact retained prefix lines1,2. Four preparation cancellation stages and syntax-before-preparation are covered. Initial CLI fixture incorrectly used --file with --changed; real CLI rejected it (`--changed requires --source`), corrected fixture to --source ., with no product change.
3. Real operation regression: four active performance registry gates invoke sparse N1000/2000/4000/8000 tests individually. Each asserts actual queries==N>0, nonzero membership/preparation, logarithmic membership and bounded preparation; removing selection calls or rebuilding per query cannot silently pass. Instrumented old O(N squared) failed all four; no elapsed-time assertions.
4. Matched release evidence: immutable baseline release/rlibs verified by SHA256, matched candidate probes built with explicit `-O -C lto=thin -C embed-bitcode=yes`. Fixture/request construction precedes API timer.96 API samples across16 fixed workloads and18 CLI samples across3 workloads, each revision has warmup and3 paired samples in alternating order. All API identity digests agree; all full CLI stdout digests agree exactly for sparse --line, selected overlapping --line (IDs/truncation), and noncontiguous --changed. Source, tool, binary, rlib and evidence-script hashes captured in persistent JSON. The initial `/usr/bin/time -l` attempt failed because sandbox denied sysctl kern.clockrate; zero usable samples from it. Fresh isolated Python wrappers use RUSAGE_CHILDREN after one bounded subprocess, so RSS is independently observed per invocation.
5. Limits/scope: API sparse workload improves while full CLI many-line median worsens in the measured3-pair sample. Requested heap/RSS/output-string bytes/logical payload and fixture/output disk bytes are distinguished explicitly. Raw samples and medians/min/max retained; no universal regression or speedup claim and no additional timing repetitions. Source-position index, public APIs, version0.14.0, schemas, formal proofs and Task2/3 product code untouched. Final fmt check and final library suite passed; normal commit hooks are the remaining lint verification before handoff.

## Release observations

Whole CLI many-line median **increased 9.257→11.657ms**; this is a fixed-workload observation from three pairs, alongside the API improvement, and must not be hidden in a no-regression claim. Full CLI selected-overlap6.093→6.379ms, changed39.886→39.299ms. Ranges and raw samples are in measurement.json. Every full stdout digest matches across revisions and all three samples for its workload.

|N|API sparse before ms (min/max)|after ms (min/max)|requested peak before/after bytes|actual membership old/new|
|---:|---:|---:|---:|---:|
|1000|0.803 (0.769/0.945)|0.476 (0.459/0.554)|456349/464349|1000000/11999|
|2000|2.624 (2.564/2.684)|1.031 (0.903/1.117)|912349/928349|4000000/25999|
|4000|7.742 (7.596/8.292)|2.023 (1.938/2.048)|1824349/1856349|16000000/55999|
|8000|26.038 (24.366/26.805)|4.062 (4.036/4.085)|3648349/3712349|64000000/119999|

The index adds exactly8*R requested bytes and one allocation for nonempty raw R. Sparse/control live delta after analysis is0 bytes; N8000 adds64000 peak requested bytes. One-range controls add8 requested bytes; All controls retain the same peak bytes. Small N8/128 all/overlap/sparse/control observations are included; some small/control medians increase, so no general speedup is inferred. RSS includes runtime/warmup, not an API-only peak or simultaneous summed tree peak. API measured output_string_bytes is not a full candidate logical charge; sparse output has no candidate payload, and exact logical charges remain covered separately by payload tests. Full CLI input/output document byte counts are in raw samples; no disk-charge optimization is claimed.

## Changed files and receipts

- `crates/hoimin-python/src/analyzer/rust/selection.rs`: private All/Restricted prefix-max owner, unchanged symbol admission logic, cancellation and cfg(test) actual counters.
- `crates/hoimin-python/src/analyzer/rust.rs`: one preparation after parsing, reference threaded through private producers, old scan removed.
- `crates/hoimin-python/src/analyzer/rust/selection_tests.rs`:34 focused unit cases/property/operation/cancellation tests.
- `crates/hoimin-cli/tests/analyzer_line_selection.rs`:3 public CLI tests with bounded20s child commands.
- `docs/performance/shapes.json`:4 deterministic operations gates, no timing gate.
- `docs/maintenance/2026-10-20-issue-1006/`: bounded raw/summary measurements, probe/build/measurement/resource-wrapper sources and verification/source/tool/rlib hash receipt.
- Detailed logs in `/tmp/hoimin-perf-three-task1`; all build/test/commit environment uses required bounded Cargo settings, Python environment on PATH. No simultaneous benchmarks/builds.

Final commit/hook result to be appended after normal hooks complete.

## Hook refinement

Initial ordinary commit attempt ran all configured hooks: workspace/vendor fmt passed and vendor clippy passed; workspace clippy rejected four new unit-test assertions under assert_is_empty. Replaced empty/nonempty Vec assertions with assert_eq!/assert_ne! so failure diagnostics show values, and made the public CLI empty JSON-array expectation explicit. No production source changed, so release measurements remain valid without repetition. Receipt retained as commit-hooks-first.log. Focused selection and CLI tests rerun after the assertion-only changes; normal commit hooks rerun on the final staged code.

The second ordinary hook attempt reached the new CLI test and rejected format_collect in its twelve-line fixture generator. Replaced the generated fixture with12 explicit literals, preserving exact source bytes and observable changes at lines2/11. Focused CLI3 rerun passed. Retained commit-hooks-second.log; no benchmark rerun because this changes only test fixture construction.

## Final handoff

Status: DONE. Commit `4c906846e4515735d7e89c17a2b374483ba299ff` — `perf(python): index candidate line selection (#1006)`.

Third ordinary commit attempt succeeded with all four mandatory hooks enabled: `cargo fmt --all -- --check`; vendored parser fmt; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`. Required CARGO_BUILD_JOBS=2 / shared target / incremental0 / test threads2 / PREK_NO_CONCURRENCY=1 and Python PATH prefix were applied; commit subprocess bounded600s. Final hook log `/tmp/hoimin-perf-three-task1/commit-hooks.log` is all Passed.

Fresh final checked HEAD is `4c906846e4515735d7e89c17a2b374483ba299ff`. Tracked tree is clean; `git diff --check` passes, and all five changed production/test/registry source hashes exactly match committed verification.json. Final library457 passed/3 pre-existing ignored; after hook-only test assertion/fixture refinements, selection59 and CLI3 passed again. Payload27/cancellation30 had already passed with unchanged production code. No production code changed after the measured release build, so no time/heap repetition was necessary.

All15 requested self-review passes are recorded above (requirements/design5, implementation5, validation5). Independent review/push/PR/CI/integration/cleanup remain controller-owned. Remaining material limitation is the measured full-CLI many-line slowdown (9.257→11.657ms median,3 paired samples) alongside the scoped sparse API improvement; sorting is cooperatively cancellable only at documented phase boundaries. No correctness uncertainty is known.
