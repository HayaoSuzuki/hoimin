# Final whole-branch review — #1006 / #1007 / #1008

Reviewed range: `34e3349099ce7fd61838629ee8c0d0b814e65a5d` → `1dc37bbbfdf2e9b8c00db8fe51667fa0348e5a92`.

**Verdict: Approved for PR publication and the planned final CI. No code or documentation fixes are required by this review.** This is not a merge authorization or a claim that publication/native CI has already run.

## Scope and method

Read the final brief first, the code-reviewer template, issue snapshots, authoritative spec and plan, controller rulings R1–R7/D1, and Task4's completed review. Inspected the supplied full-range package in passes: product/test/current-documentation hunks, measurement helpers and probes, archival provenance and compactly parsed raw JSON/JSONL. Checked substantive self-review entries for all 60 indexed passes. Historical pending-I1 language remains historical; current acceptance is explicitly supplied by the D1 addendum.

Focused unchanged-source inspection addressed named cross-boundary risks: actual CLI analysis/conversion ordering and payload admission; strict validation error order; fresh original-source verification; worker materialization, cleanup, charge release and field drop order. No subagents, builds, tests, benchmarks, broad suite replay, product edits, index changes or HEAD changes were performed. Read-only evidence arithmetic/hash checks described below were performed. The checkout's tracked diff remained empty; this ignored report is the review deliverable.

## Strengths and technical assessment

### #1006: prepared selection

- `crates/hoimin-python/src/analyzer/rust/selection.rs:20` correctly distinguishes raw-empty `All` from nonempty input whose valid-range vector is empty. Sorting by start/end and prefix maxima make predecessor membership equivalent to the original inclusive union, including nested/duplicate/reversed/zero/MAX inputs, without endpoint increment overflow (`:44`, `:75`). Path-qualified and nested-symbol matching remains the existing conjunction (`:95`).
- One owner is constructed after the existing hierarchy/parse/facts/error boundary and passed to token, AST and annotation producers (`crates/hoimin-python/src/analyzer/rust.rs:447`, `:698`, `:3109`, `:7829`). Candidate admission still precedes original copying and payload generation; the AST unrestricted fast path and child traversal remain intact. Ordering, prefix merging, truncation and identity creation are not rewritten.
- Cancellation checks cover entry, copying, post-sort and prefix preparation. The O(R) allocation, retained duplicates and synchronous standard sort are disclosed costs, not hidden cancellation guarantees. Independent arbitrary-u32 union properties and named producer/symbol cases complement actual operation counters (`crates/hoimin-python/src/analyzer/rust/selection_tests.rs:58`, `:219`). Public CLI tests cover many `--line` selectors, exact candidate equality/truncation and noncontiguous changed hunks (`crates/hoimin-cli/tests/analyzer_line_selection.rs:44`).

### #1007: immutable manifest ownership

- `crates/hoimin-cli/src/workspace/manifest.rs:92` places all immutable metadata behind one private Arc. Storage itself is not Clone and exposes no mutable/Arc/Weak API. Derived equality remains structural; content comparison still checks directory/path/size/hash values (`:134`). The two documented const removals are necessary and introduce no duplicated scalar compatibility route (`:121`; `crates/hoimin-cli/src/workspace/copy.rs:512`).
- Fresh original verification still rebuilds the manifest (`crates/hoimin-cli/src/workspace/copy.rs:565`). Materialization shares only metadata while retaining independent root, snapshot, allowance, state, slot and charged-byte ownership (`:744`; `crates/hoimin-cli/src/workspace/mod.rs:463`, `:588`). Sharing adds no filesystem authority, cleanup callback, cycle or validation bypass.
- Tests cover independent equal/changed manifests, generated real file data, surviving handles, final metadata release, a cancelled waiter with detached blocking materialization, sibling failure/charge isolation, mutation isolation and pending-cleanup metadata (`crates/hoimin-cli/src/workspace/manifest.rs:470`, `:493`; `crates/hoimin-cli/src/workspace/copy.rs:1935`; `crates/hoimin-cli/src/workspace/mod.rs:1936`). Retained-heap probes keep owners live and exclude fixture/output capacities; meter self-checks distinguish live/peak/realloc/release (`docs/maintenance/2026-10-20-issue-1007/probe.rs:79`, `:97`).

### #1008: source-bound deferred locations

- The private OnceLock caches a fallible index derived exclusively from the context's borrowed raw source and owned/borrowed decoded facts (`crates/hoimin-core/src/candidate.rs:187`, `:238`). Raw and decoded length limits, decoding retention and raw hashing remain eager. No new independent source/index argument, unsafe product code, panic, source copy or global cache is introduced.
- The initializer is at the existing location check after schema/path/hash/mutation/span/original/decoding/boundary/replacement validation (`crates/hoimin-core/src/candidate.rs:559`, `:612`). Unicode uses decoded scalar coordinates and Latin1 uses raw byte coordinates (`:222`). OnceLock's synchronized immutable publication preserves production Send/Sync and cannot recurse through this initializer.
- Both CLI paths complete analysis before converting candidates, so the analyzer's local index has already dropped (`crates/hoimin-cli/src/analyzer/mod.rs:455`, `:467`, `:715`, `:725`). Conversion still admits metadata and scratch before strict validation (`:892`, `:944`). Existing cancellation checks remain at conversion boundaries; scans are still synchronous. No worker hash/ID/writeback validation is bypassed by shared manifests or lazy locations.
- Independent scalar-walk properties, early-error/materialization tests, production Send/Sync assertion, the 12-case encoding/newline plan-run-verify matrix and actual byte writeback cover meaningful behavior (`crates/hoimin-core/src/candidate_lazy_tests.rs:45`; `crates/hoimin-core/tests/candidate_policy.rs:573`; `crates/hoimin-cli/tests/source_index_encoding_flow.rs:10`). The Latin1 heap test now reaches validation, and the CLI heap gate requires an eager-retention sensitivity control to fail the same bound (`crates/hoimin-cli/tests/source_encoding_heap.rs:44`; `crates/hoimin-cli/tests/source_index_heap.rs:69`).

## Evidence checked independently

- Inspected actual RED logs: old selection comparisons are 1M/4M/16M/64M; old retained manifest/worker gates fail at 7,510,400/474,312 bytes; eager context construction and actual CLI discovery fail their new bounds. These are efficiency failures, distinct from preservation controls.
- Parsed all 84 JSON files in the four new evidence collections. Verified six matching before/after observations per workload and invariant output digests across 16 selection API groups (96 observations), 24 manifest groups (144 observations, also equal logical bytes/charges), and 35 source-index probe groups (210 observations). Whole-CLI observations are separately retained, not misattributed as isolated Task3 causality.
- Independently recomputed the 2048-row R6 pairing, alternating order, per-fixture stable output agreement and exact rank 276. Tiny's upper bound is −19,834 ns; small's is +67,791 ns, with paired median +21,354 ns. The original +50,000 ns criterion remains failed for small. The explicit post-observation D1 +100,000 ns acceptance is satisfied; all six matched peak/retained heap differences are zero. This does not change the experiment's initial criterion or its independent-representative-pair assumption (`docs/maintenance/2026-10-20-issue-1008/r6/summary.json:1`; `docs/superpowers/specs/2026-10-20-perf-three-design.md:60`).
- Checked 75 eager and 75 lazy constructor/drop records: successful pipeline/discovery/plan retain two sequential builds with maximum live 2→1; run retains three builds with maximum live 2→1; unused context builds 1→0. Matching drops are present. Nine recorded restored Task3 source hashes match final sources, and the available preserved baseline/ordinary candidate binaries match restoration receipts.
- All 113 archive members match recorded sizes/hashes and membership. All 15 integration receipts' raw log hashes match; independently recounted top-level Rust results match workspace 3430 passed/33 ignored, core contracts 705/2, CLI contracts 2228/28 and reference 7. The 47 gates report passed and registry identity matches. No protected product/test/Cargo/tool/workflow/proof change exists after frozen integration source `f72134d7`.
- Python evidence accurately preserves the original 1197 passed/11 skipped/four sandbox failures and scoped four-case successful retry, not a fictional all-green full run. Current documentation preserves mixed CLI/filesystem timings, context stack 160→168 bytes, the original R6 failure and D1 decision (`docs/maintenance/2026-10-20-perf-three/README.md:41`; `docs/performance/README.md:137`). Clone nanosecond windows can fall below clock resolution and are not persuasive standalone speed estimates; the manifest result is supported by deterministic retained-heap evidence, with that timing limitation disclosed.

## Issues

### Critical (Must Fix)

None.

### Important (Should Fix)

None.

### Minor (Nice to Have)

None.

No unresolved cross-task defect or deferred code finding was identified. No compatibility wrapper or unrelated product change is requested.

## Recommendations and remaining controller work

Archive this review, Task4's review, final hook/handoff receipts and controller rulings before publishing. Then run the planned automatic PR CI and final native Windows/macOS workflow against the actual publication head, recording the merge-ref versus branch-head distinction. Those executions, PR creation and task-owned cleanup remain pending controller work; they are not failures of the reviewed implementation. Do not merge or release under this review.

Keep the R1–R7 costs/D1, mixed timing, finite-workload measurement limits, two sequential Unicode builds, +8-byte context stack and two const removals visible in final communication. No new broad test or benchmark run is requested here.

## Declined to judge

1. Universal latency/RSS/physical-disk guarantees, strict zero-cost Arc cloning and hard cancellation deadlines: outside the approved contracts; existing evidence concerns specified operation/requested-heap workloads and cooperative cancellation. Observed timing increases and measurement limits were assessed and retained, not discarded.
2. Global source-index deduplication for arbitrary external validation-before-analysis call order or a one-construction guarantee: explicitly excluded by the source-lifetime design. Actual affected CLI ordering and observed construction/lifetime behavior were assessed above.
3. Repair of inherited missing historical OKF paths/commits/catalog entries, external URL/fragment validity, or unrelated issues/formal extensions: outside this change. The integration audit records existing deficiencies separately and reports no newly introduced local-reference findings; this review does not certify those historical targets.

Native platform behavior is not silently declined as out of scope: its planned final CI is still required controller work, and no pending native/automatic run is represented as completed.

## Assessment

**Ready to proceed to PR/CI: Yes.** The three implementations meet the accepted contracts and preserve their cross-boundary ownership, error, encoding and accounting behavior. Recorded integration and independently checked evidence support approval; final publication/native CI and archival remain the controller's explicit next steps.
