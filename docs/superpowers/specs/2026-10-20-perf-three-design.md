# Selection and immutable metadata performance design

## Intent and authority

Implement #1006, #1007 and #1008 on main `34e3349099ce7fd61838629ee8c0d0b814e65a5d`, after merged PR #1023. The requested outcome is lower repeated selection work and immutable metadata/index retention, with observable behavior preserved. The linked issues are the acceptance authority; their historical measurements are not new evidence. User authorization includes implementation through a PR, no compatibility adapters, five self-review passes in each requirements/design, implementation and validation stage, AAA tests with one behavioral case per function/parameter instance, property testing, and bounded disk/memory usage. Existing untracked user files in the original checkout are outside this worktree.

The three changes share a validation/evidence plan but have independent implementation and review gates. This is an architectural review of existing boundaries, not a new subsystem or a reason to request approval again for already authorized routine choices. Any material unresolved user-facing contract would be raised separately.

## #1006: prepare per-file selection membership

The public AnalyzeRequest keeps accepting an arbitrary slice of LineRange. A private CandidateSelection is prepared once per successfully parsed file and reused by token, AST and annotation candidate producers. It owns an All/Restricted line index and borrows the existing path and symbol selectors. The source-position LineIndex remains independent.

All means the original slice was empty. A nonempty slice containing only reversed ranges means no selected lines. Valid ranges are sorted by (start,end); each stored end becomes the maximum end of its prefix. To query line q, binary-search the final start <= q and compare q with that prefix end. All qualifying starts are in the prefix, so its maximum end exactly characterizes the union. This handles unsorted, overlapping, nested, duplicate and adjacent inclusive ranges without caller preprocessing. No end+1 arithmetic is required at u32::MAX. Retention is O(R), preparation O(R log R), queries O(log R); duplicates are not compacted. All allocates no interval storage.

CandidateSelection owns current symbol admission as a conjunction, preserving module-qualified and nested symbols; it does not introduce a symbol-resolution index. Retain the unrestricted fast path for AST expression admission. No candidate order, original/replacement bytes, IDs, truncation, cancellation result, or candidate-payload accounting policy changes. Constructor checks cancellation before/during preparation and after the standard sort; sorting remains a synchronous cooperative-cancellation phase, not a new hard latency guarantee. Preserve initial cancellation and syntax/hierarchy error priority.

Alternatives considered: repeated scans retain the defect; the core path-keyed LineSelectionIndex has different empty semantics and adds path work; a public prepared-request API creates avoidable migration; an unrelated source-position index must not absorb selection state. A prefix-max representation avoids duplicating the separate target-normalization implementation.

## #1007: immutable manifest handles

WorkspaceManifest becomes a read-only value handle over private Arc<ManifestStorage>. Storage contains entries, directories and logical_bytes together and does not derive Clone. Existing public read-only access and structural equality remain. Whole-manifest clone shares metadata; explicit ManifestEntry clone keeps its ordinary owned-value semantics.

Plan and worker continue storing WorkspaceManifest values. Their ownership, worker IDs, capabilities, allowance, charge, cleanup state, disk snapshot and drop ordering stay independent. There is no Mutex, mutable cache, Arc back-reference, mutable accessor or filesystem cleanup in the metadata storage. Original-source verification still constructs and compares fresh content, never just pointer identity.

The required ordinary Rust source changes include removing const from logical_bytes() and WorkspacePlan::completed(). Do not duplicate the scalar to retain const or add a compatibility route. This is an internal crate API change; serialized formats and product version floor remain unchanged. Workers and detached/pending tasks keep metadata alive through owned handles, including after plan drop; final handle drop releases it.

Arc fields on both plan and worker were considered but spread clone policy across modules and leave standalone manifest cloning deep. Separate shared vectors add refcounts without an independent lifetime need. Borrowing from the plan constrains existing worker lifetime unnecessarily.

## #1008: lazy validation positions

Keep CandidateValidationContext as the immutable owner of raw-source borrow, decoded facts and raw hash. Materialize its private CandidateSourceIndex only when strict validation reaches the existing location check. Use OnceLock<Result<CandidateSourceIndex, CandidateValidationError>> to reuse the fallible factory without a new panic or unsafe source/index pairing. The initializer accepts no external source/index pair and derives only from private context facts.

Raw and decoded u32 size checks remain eager in new(), including Latin-1 decoded expansion. Factor the checked-length invariant into a private helper as needed. Keep decoding-error retention and the exact validator order: schema/path/hash/mutation/span/original/decoding/boundaries/replacement/location/ID. CLI still reports source decoding errors before analysis. Metadata and scratch budget admission stay before strict candidate validation.

Existing CLI plan/run finishes analysis and drops its local Unicode index before converting candidates. Lazy validation therefore removes their simultaneous duplicate index. A successful UTF-8 source still builds two indexes sequentially; no one-build or CPU speedup claim follows. Early rejection/no-candidate paths can avoid the validation index. Arbitrary external callers who validate before separately analyzing the same text can still overlap indexes; the guarantee is for the affected CLI ordering, not global deduplication.

UTF-8/BOM/ASCII columns use Unicode scalar positions; Latin-1 validation indexes raw bytes while analysis indexes decoded UTF-8. Preserve these distinct coordinate domains, raw/decoded spans, hashes, stable IDs and encoding-preserving writeback for LF/CRLF/CR. Keep production Send/Sync. Do not add a public testing API or a shared index argument, global cache, copied source, legacy implementation or feature flag.

Safe source-bound sharing could remove a scan but requires broader core/python/CLI/fuzz API migration. Moving the entire context after analysis risks second decoding and changed error ordering. Lazy context-local materialization satisfies the requested retention boundary with less interface cost. Measure its per-validation OnceLock cost on small ASCII input.

## Evidence and acceptance

Each issue owns meaningful pre-change RED evidence for the inefficiency, GREEN behavior and performance bounds, five reviews in each of three stages, and an independent spec/quality gate. Preservation cases that already pass are labeled controls. Tests use separate AAA cases, generated parameter functions and bounded independent proptest expectations; no wall-clock CI assertions or implementation-mirroring oracle.

- #1006: actual old/new comparison counters; sparse source `flag = True\n\n` repeated N=1000/2000/4000/8000 with all blank even rows selected, boolean_literal/max1, plus one out-of-range selector control. Independent raw-range oracle and boundary/nesting/invalid/symbol properties; actual public CLI many --line and noncontiguous --changed. Preserve ordering/IDs/truncation/cancel/payload. Release time and requested heap cover stress and small controls.
- #1007: actual retained allocation windows for public manifest clones and real workers on 1000/4000 files, 1/4/16 workers/handles, empty and small files. Show retention does not multiply metadata by worker count. Retain existing large-file buffer guard. Exercise structural equality, reset/mutation/original verification, plan/handle drop orders, creation failure, cancellation and pending cleanup with deterministic existing harnesses. Logical copy charge and cleanup stay independent.
- #1008: fixed one-candidate Unicode comments at N=262144/524288/1048576; context-only, actual analysis+validation, actual CLI discovery/run/plan heap/time and small ASCII controls. Count real builds/materializations and distinguish sequential build count from maximum live indexes. Functional encoding/newline matrix and independent coordinate oracle; preserve early errors, payload and cancellation. Existing Latin-1 heap coverage must force deferred validation so it remains sensitive.

Baseline release binary and rlibs were built from clean main before product edits, with locked dependencies, Rust1.99.0, two build jobs and incremental disabled. A clone copy is preserved at /tmp/hoimin-perf-three-baseline/release; its receipt records original SHA and binary/log hashes. Build identical probe sources against this copy and candidate libraries. Production release timing and allocator/count instrumentation are separate experiments with explicit source patches and hashes. Fixtures, warmup/sample counts, individual results, commands, revisions and output digests are retained. At least three paired measured samples after warmup; batch tiny operations enough to exceed timer granularity. Do not label requested Rust heap, sampled/process RSS, logical payload, or disk charge as one quantity.

One shared task target /tmp/hoimin-perf-three-target and one Cargo owner; two build jobs and bounded test threads. Reuse the locked Python validation environment. Keep measurements sequential, fixtures <=4000 files and <=16 workers, and probes time-bounded. Monitor disk, remove task build artifacts after final CI. Baseline builds remain available until all paired measurements/reviews finish.

## Integration and publication

Run focused suites per issue and one final workspace/contracts/Python/performance-gate integration pass. Required normal fmt/clippy hooks remain enabled. Run actual PR CI and one final manual native Windows/macOS workflow, since workspace lifetimes and encoding apply there. Preserve proof inputs/tool pins and unrelated issues, including #806 and held formal extensions. No merge or release is requested.

Update affected current OKF analyzer/workspace/selection/compatibility explanations and the design-source catalog for this spec. Preserve historical audits and itemize inherited unavailable references rather than rewriting their evidence. Archive task reports, reviews, raw decisive evidence and controller rulings before clearing the task scratch workspace. PR names all three issues and records exact checked commits and measurement limits.


## Recorded small-ASCII acceptance decision (D1, 2026-10-20)

The initial controller R6 protocol fixed a +0.05 ms tolerance before observing
512 alternating pairs for each tiny/small ASCII fixture. Tiny's one-sided 95%
paired-median upper bound was −0.019834 ms; small's was +0.067791 ms. The latter
did not meet the initial criterion. All requested whole-plan peak and retained
heap differences were zero. That result and earlier adverse observations remain
in the [R6 evidence](../../maintenance/2026-10-20-issue-1008/r6/README.md).

After seeing those results, the user explicitly chose to accept a practical
upper-bound tolerance of +0.1 ms with the same no-increase whole-plan heap
criterion (D1). The existing observations meet that accepted criterion on both
fixtures. This is a user acceptance decision after observation, not a
prespecified +0.1 ms experiment or universal non-regression guarantee. The
context stack increase from 160 to 168 bytes remains disclosed. The
[acceptance addendum](../../maintenance/2026-10-20-perf-three/reports/task-3-d1-review.md)
records I1's closure under D1; the original +0.05 ms verdict remains unchanged.
