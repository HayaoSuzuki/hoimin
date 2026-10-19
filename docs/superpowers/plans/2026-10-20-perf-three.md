# Selection and immutable metadata performance implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve #1006/#1007/#1008 with measured reductions in repeated work/retention and unchanged product semantics, then create a reviewed PR.

**Architecture:** Private per-file CandidateSelection; immutable WorkspaceManifest backed by one private Arc storage; context-local lazy candidate location index. Each boundary owns its representation and preserves existing validation/lifecycle authority.

**Tech Stack:** Rust1.99.0, existing proptest and allocator harnesses, existing Python3.14 tooling, GitHub Actions. No new production dependency.

**Spec:** `docs/superpowers/specs/2026-10-20-perf-three-design.md` and archived issue snapshots.

## Global Constraints

- Implement only #1006/#1007/#1008; no compatibility adapters, feature switches, unrelated state-machine or formal-proof changes. Current schemas and version0.14.0 remain.
- Five distinct self-reviews in each requirements/design, implementation and validation stage for each task; report evidence, not a checked box repeated five times. Independent review is additional.
- AAA, one behavioral case per test function/parameter instance; parameter/property tests with independent expectations and bounded sizes. Real RED for inefficiency, preservation controls labeled honestly.
- One implementation/Cargo owner at a time. CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=/tmp/hoimin-perf-three-target, CARGO_INCREMENTAL=0, RUST_TEST_THREADS=2, PREK_NO_CONCURRENCY=1. Existing Python environment/cache under /tmp/hoimin-priority-six-{venv,uv-cache}; PATH starts with its bin.
- Preserve clean baseline release artifacts under /tmp/hoimin-perf-three-baseline until comparisons finish. Record actual checked source/tool/binary hashes; historical issue values are not fresh measurements.
- Measurements are fixed-workload evidence: requested live/peak heap, process RSS, elapsed time, payload and disk charges are distinct. No wall-time assertion in ordinary tests. Each measurement command has a bounded timeout; no concurrent benchmarks/builds.
- Product tests per task are focused; final workspace/contracts/Python/performance tests run once at integration, repeat only for changed code/failures. Mandatory commit hooks remain enabled.
- No subagents inside implementers/reviewers; controller owns review, PR push/creation, CI dispatch and final cleanup. Reports and raw receipts live in this plan's ignored SDD workspace until archived.

## Review Focus

1. Nonempty all-reversed line selections must select none, including arbitrary u32 edges; Task1 independent oracle and named cases.
2. Selection gates for AST expression builders and annotations must use the same prepared ranges as token candidates; Task1 producer/symbol/cancel/budget controls.
3. A surviving or pending worker must retain immutable metadata after other plan/worker handles drop without retaining their charge/cleanup authority; Task2 drop-order and failure/cancel cases.
4. Lazy indexing must not defer decoded-size failures or change original-mismatch/decoding/replacement/location precedence; Task3 exact early-error cases and length helper boundary tests.
5. UTF8/BOM/Latin1 with CRLF/lone CR must keep raw span, Unicode column, ID and writeback unchanged; Task3 independent coordinate property and public pipeline matrix.

---

### Task 1: Index analyzer line selection (#1006)

**Files:** Create `crates/hoimin-python/src/analyzer/rust/selection.rs` and focused test module(s); modify private call sites in `crates/hoimin-python/src/analyzer/rust.rs`; add public selection coverage in appropriate CLI tests; add bounded evidence under `docs/maintenance/2026-10-20-issue-1006/` and performance registry if suitable.

**Interfaces:** Consumes unchanged public AnalyzeRequest and LineRange. Produces private CandidateSelection::new(request, cancelled) -> Result<Self, AnalysisCancelled>, includes(line:u32,symbol:Option<&str>)->bool, is_unrestricted()->bool. It borrows request metadata and owns the prepared interval Vec; public analyzers stay unchanged. No dependency on later tasks. Source position LineIndex stays distinct.

- [ ] Read issue snapshot and controller line-design discovery; record five requirements/design reviews against exact current source. Baseline binary/rlibs are already preserved at34e33490; use them, not a newer accidental build, for before measurements.
- [ ] Add real cfg(test) old predicate comparison instrumentation and separate sparse-workload tests at N=1000,2000,4000,8000. Assert indexed-work bounds using actual query count (and a nonzero expected query count), not elapsed time. Run focused tests and capture old O(N squared) failures plus controls before changing production behavior.
- [ ] Add named parameter cases for empty/all-invalid/zero/MAX/unsorted/nested/overlap/duplicate/adjacent ranges, symbols and each producer path. Bounded proptest compares membership with `ranges.is_empty() || ranges.iter().any(|r| r.start<=q && q<=r.end)` using arbitrary u32s. Preserve raw public-request semantics and current error priority.
- [ ] Implement All/Restricted prefix-max index from the spec, one construction per parsed file, cancellation checks before/during copying/prefix preparation and after sorting. Thread its reference through make_candidate, AST collector and annotation producers, retaining unrestricted fast path. Remove old repeated scan rather than retaining fallback code.
- [ ] GREEN focused Python analyzer/payload/selection/cancellation tests and public CLI --line/--changed cases. Test candidate order/ID/truncation equivalence and exact payload boundaries separately. Suggested commands: `cargo test --locked -p hoimin-python selection`; `cargo test --locked -p hoimin-python payload`; focused CLI selection and changed tests. Final hook clippy covers private signature call sites.
- [ ] Measure release API sparse/control N matrix and real CLI multiple selectors/noncontiguous changed scope with before/after output digests. >=1 warmup and3 paired samples; raw counts/heap/time plus small ordinary controls. Keep fixture generation outside API timing; label full CLI timing separately. Add a deterministic operation-count performance gate, no timing gate.
- [ ] Run five implementation and five validation review passes, fmt/clippy and appropriate focused regressions; commit with ordinary bounded hooks. Report exact RED/GREEN commands/results, measurements/limits, source hashes and remaining concerns. Controller then reviews this task before Task2.

### Task 2: Share immutable manifest storage (#1007)

**Files:** Modify `crates/hoimin-cli/src/workspace/manifest.rs`, `copy.rs`; focused lifecycle tests in existing workspace modules; add isolated metadata heap test binary(s) alongside `tests/workspace_heap.rs`; evidence under `docs/maintenance/2026-10-20-issue-1007/`.

**Interfaces:** Consumes current WorkspaceManifest value API. Produces same methods/structural equality and cheap Clone via private Arc<ManifestStorage{entries,directories,logical_bytes}>. Remove const only where required (`logical_bytes`, `WorkspacePlan::completed`). Plan/worker field types and transfer signatures remain unchanged. No state shared with Task1's selection index.

- [ ] Read issue snapshot and manifest design report; inspect actual base afterTask1. Record five design reviews, particularly const callers, security capabilities, detached tasks and charge/cleanup separation. Baseline immutable manifest implementation is still34e33490; public probes can link preserved rlibs.
- [ ] Add allocator regression binaries with one case per process and a self-checked requested-live/peak meter. Capture actual RED for clone retention and real-worker metadata scaling before edits. Use preallocated output handles, exclude fixture/setup/printing from clone windows, retain handles during LIVE readings. Establish discrimination before selecting a conservative bound; do not confuse existing peak-only finish() with retained bytes.
- [ ] Parameterize handle content/equality/drop-order cases and bounded small filesystem properties against generated file data. Reuse/extend existing failed_snapshot_copy_rolls_back_only_the_slot_and_keeps_the_bound_reservation, reset_task_preserves_depth_error_and_pending_copy_accounting, shared_snapshot_restores_after_original_source_changes and MaterializationPause cancellation harnesses. Add separate sibling isolation, final metadata release and plan-drop cases where existing coverage does not address shared lifetime. No sleeps or platform-unsafe chmod assumptions.
- [ ] Implement private immutable Arc storage at the single manifest construction boundary. Keep structural comparisons, path/hash/size checks, fresh original verification, sorted directories and checked byte totals. No public Arc getter, mutable access, Weak instrumentation API, Mutex, cache, scalar duplicate, deep-copy fallback or lifecycle reordering.
- [ ] GREEN metadata heap and focused workspace reset/mutation/recovery/failure/cancellation tests. Suggested commands: `cargo test --locked -p hoimin-cli --test workspace_manifest_heap -- --test-threads=1`; focused workspace unit filters and existing workspace_heap/recovery cases. Native Windows-only cases are recorded for final CI, not claimed locally passed.
- [ ] Measure 1000/4000 files,1/4/16 clone handles and workers, empty/64-byte payloads, with sequential cleanup. Record pure-clone retention/time separately from actual materialization retention/time and optional process RSS. Copy filesystem operations still scale with files*workers; no generic materialization speedup claim. >=3 paired samples with bounded churn and retained raw observations/digests/charge checks. Add deterministic retained-memory gate.
- [ ] Run five implementation/five validation reviews, normal fmt/clippy/hooks and focused regressions, commit and report. Controller task review gates Task3.

### Task 3: Defer candidate validation location indexes (#1008)

**Files:** Modify `crates/hoimin-core/src/candidate.rs`; add focused core tests and CLI heap/pipeline tests (`source_encoding_heap.rs` plus isolated Unicode flow test); adjust only necessary private analyzer location method plumbing; evidence under `docs/maintenance/2026-10-20-issue-1008/`.

**Interfaces:** Consumes existing immutable CandidateValidationContext and strict validator. Produces private OnceLock<Result<CandidateSourceIndex,CandidateValidationError>> with eager raw/decoded size checks, existing public constructor/validator signatures and unchanged Send/Sync. No public separate source/index argument. Task1's private CandidateSelection remains independent and unchanged.

- [ ] Read issue and source-index discovery; inspect exact validator order and both CLI callers. Record five design reviews, with source ownership, eager errors, UTF8/Latin1 coordinate separation, cancellation and small-ASCII cost. Two sequential builds on successful UTF8 are allowed and must be reported honestly.
- [ ] Add real RED context-only allocation/materialization cases and an isolated actual CLI discovery/run flow heap gate using fixed candidate count and Unicode size growth. Old eager path must violate the bound; record eager sensitivity control. New counters are private real construction boundaries, not assumed phase counts; no public test-only bridge across crate cfg(test).
- [ ] Add separate early-error/size-helper boundary cases; property coordinate oracle uses independent scalar walks. Parameterize ASCII/UTF8/BOM/Latin1 × LF/CRLF/CR with real raw-span/column/hash/ID/writeback behavior, separating test behaviors. Preserve existing strict external plan/review/workspace validation. Force actual validation in Latin1 heap test so delayed allocations cannot hide regressions. Integration trait assertion checks production Send/Sync without cfg(test) lookup Cells.
- [ ] Implement lazy context-local index with get_or_init at the existing final location check; retain decoding/hash/eager raw+decoded bounds. Reuse fallible factory and propagate cached error without introducing expect/panic. Factor checked-length logic rather than silently deferring errors. Do not move metadata/scratch admission, add index charges to payload, or change source mapping.
- [ ] GREEN core candidate/source-encoding/property tests and CLI encoding, plan/review strict checks, payload exact limits, cancellation and heap tests. Suggested commands: `cargo test --locked -p hoimin-core --test candidate_policy`; `cargo test --locked -p hoimin-core --test source_encoding`; relevant CLI source_encoding/heap and analyzer payload filters.
- [ ] Measure N262144/524288/1048576 comments with one boolean candidate/max1; public API context-only and full analysis+validation, actual CLI plan/run/discovery requested heap and ordinary release timing. Also measure tiny and realistic smallASCII controls, real index build/live counts and lookup timing. Use temporary documented instrumentation for cross-crate counts rather than public production hooks; keep uninstrumented release timing distinct. >=3 paired samples; same source/probe/command hashes and output digests. No total-CLI-half-memory or universal no-regression claim without data.
- [ ] Run five implementation/five validation reviews, focused suites and normal hooks; commit/report. Controller review gates integration.

### Task 4: Integrate evidence, documents and validation

**Files:** Update relevant `docs/knowledge/design/analyzer.md`, workspace/selection concepts found through catalog, `docs/knowledge/references/design-documents.md` for the new spec, development/performance references as needed; add this plan's execution/evidence archive. No unsolicited product edit; report integration failures to controller for scoped resolution.

**Interfaces:** Consumes three reviewed commits/reports and preserved measurement artifacts. Produces truthful current design/limits, >=60 indexed task self-reviews, full report/review/ruling provenance and PR-ready validation. Earlier task evidence stays tied to exact checked commits.

- [ ] Record five design/coverage passes against all three issues and interactions. Check cancellation, payload, worker charge/cleanup, encoding, release fixture limits, unchanged schemas/proof inputs and operation-vs-time-vs-heap claims. Read OKF overview/index/workflow and impacted source claims; inherited historical evidence remains historical.
- [ ] Run once: workspace fmt, vendor fmt, workspace all-target/all-feature clippy, vendor clippy; `cargo test --locked --workspace`; core and CLI `--features contracts`; CLI reference example and `--check`; frozen Python format/lint/ty/full pytest; registered `tools/performance_shapes.py gate`. Use shared target/twojobs, no concurrency. Capture exit, exactsource head/diff, top-levelcounts (exclude nested helper summaries), skippedcases and any warnings.
- [ ] Reconcile design docs and performance registry. Archive task reports, independent reviews/fixes, measurements, source hashes, concise raw receipts and all controller rulings. Verify archive membership/hashes, new/changed OKF references and catalog reachability; separately itemize inherited missing paths or unavailable commits. Do not claim URL/fragment checks unless run.
- [ ] Record five implementation/five validation reviews, commit documentation/evidence using normal hooks. Controller performs task review and one whole-branch review; at most one final combined fix wave and scoped re-review.
- [ ] Controller pushes issue-named branch, opens PR closing3issues, runs automatic CI and one final manual native Windows/macOS CI, fixes observed CI defects through scoped review, updates PR with exact heads/results, cleans only task-owned build/scratch after evidence preservation. No merge/release.
