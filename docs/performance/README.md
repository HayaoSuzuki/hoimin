# Performance validation by input shape

[shapes.json](shapes.json) inventories inputs across eight dimensions and 29 shapes, N/2N/4N sizes, measurement subjects, and executable regression tests. `growth_model` describes input structure; `expected_after_fix` describes expectations after dependent issues are integrated. Read measurements as observations at their explicitly recorded revisions.

| Dimension | Release-mode inputs | Deterministic ordinary gates |
| --- | --- | --- |
| Target discovery | Independently increase non-target/target file counts and file/symbol/line selector declarations | Active traversal-count gate for #453, selector queries for #474, and line-range merging for #475 |
| Fingerprint | File counts, total bytes, duplicate globs, and mixed exact/glob inputs | Active walk-count gate for #457 |
| Source layout | Place identical bytes/candidates on one long line or many lines; compare Unicode columns | Active column-query gate for #470 and existing token-range queries |
| AST | Independently increase width, left depth, huge literals, and unselected collections | Active replacement-generation gate for #461 and existing prefix/overflow gates |
| Analysis state | Import width × annotation count; rebinding × call count; unselected/zero-candidate controls | Active operation-count gates for #479/#482 |
| Verify | One/many files, top1/topN, explicitly incomplete plans | Active preprocessing-count gate for #456 |
| Output | Independently increase mutant record count and length | Report/progress allocator peaks, #463 write counts, and progress gates with 500/1,000/2,000 entries per report × 2/4/8 history lengths |
| Workspace | Independently increase files, bytes, and workers | Separate post-creation retained-heap and preflight allocator-peak gates |

The registry declares the active gates. These do not cover every performance regression shape. Register pending fixes as pending and exclude them from successful counts. After integrating a fix, check actual test names and counter locations, run the specified `args`, then mark the gate active. Relabeling alone is not verification.

## Ordinary gates

The `analyzer-distinct-symbol-selection-exact-work` gate varies distinct symbol selectors and
lookup counts independently (32/128/512). It measures selector preparation and
actual set probes: a one-level miss performs one lookup per candidate, while
line-rejected candidates perform none. Nested names probe dot-boundary ancestors;
the prepared set retains O(S) borrowed entries. This gate does not count allocator
events, retained bytes or RSS. The existing `symbol-selectors` release shape
repeats the same declaration over one function, so it remains a normalization
measurement rather than evidence for the distinct-selector axis.

The [#1060 release samples](2026-10-22-issue-1060-release.json) compare one-symbol,
unrestricted and 512-distinct-selector plans. In five pairs per input on macOS
arm64, the many-selector median wall time is 20.002→12.913 ms and median maximum
process RSS is 10,469,376→10,616,832 bytes. Small/unrestricted observations are
mixed and no confidence interval or timing threshold was used. Read the
[audit and measurement conditions](../knowledge/audits/design-2026-10-22-post1059.md#follow-up-implementation-and-validation)
before inferring a general speed or memory improvement.

```sh
python tools/performance_shapes.py check
python tools/performance_shapes.py gate --output /tmp/hoimin-perf-gates-new
```

The runner executes specified Rust tests with `--exact`; zero matches, ignored-only results, and nonzero exits fail. Ordinary CI and manual non-Linux CI use the same small gates. Each cargo invocation has a 300-second external limit; the overall job retains its existing CI limit. These gates do not replace all-workspace tests.

## Release measurement lane

```sh
python tools/performance_shapes.py measure \
  --baseline /absolute/path/to/baseline/hoimin \
  --candidate /absolute/path/to/candidate/hoimin \
  --output /tmp/hoimin-perf-measurements-new
```

Filter with `--shape long-line --shape many-lines`. Each size defaults to three repetitions, with 3–10 allowed. Records include both executables' SHA-256, argv, OS/CPU/Python/Rust, input bytes/file counts, candidate counts, truncation, exit codes, raw time/RSS, and aggregates. A binary digest alone cannot reconstruct the source revision; also record compared commits in the execution report.

stdout/stderr go to files. The existing resource guard limits each execution to 30 seconds and the process tree to 2 GiB, sampling RSS every 25 ms. Unobserved short-lived processes have null RSS, not zero bytes. Elapsed time includes guard startup monitoring and exit confirmation. Monitoring overhead can dominate small fixtures; default sizes alone do not establish asymptotic time/RSS guarantees. Allocator peaks, retained heap, and RSS are not interchangeable.

Each execution creates its input project in a temporary directory and removes it on completion. Result directories must be new and are never overwritten. Failed executions still save stdout/stderr and verdicts; failure to create the output directory prevents artifact storage. Preliminary verify-plan generation is outside measurements, and failures are not counted as successful measurements.

The GitHub Actions `Performance measurements` workflow is manual-only. It release-builds the selected ref and `baseline_ref`, measures using the same inventory, and saves artifacts. Ordinary PR pass/fail does not gain time/RSS thresholds. POSIX process monitoring limits this lane to Linux/macOS; it does not establish Windows RSS measurements.

## Lean evidence

The inventory's `lean` entries link existing models, proofs, generated corpora, and Rust adapters. Build the generator, run `--sensitivity`, and run `--check` sequentially with resource guards, then compare actual target reads through the separate `lean-target-reads-complete` and `lean-target-reads-truncated` gates. Define cases in Lean rather than editing JSONL. Existing 0/1/limit/overflow cases check prefixes and order, without guaranteeing constant memory for the entire CLI or native-stack bounds. The additional 56-case `lean.cost` corpus compares binding indexes, whole-state clones inside annotation callbacks, and actual replacement-builder counts/bytes. It covers 0/1, 7/8/9, 8/16/32 and deliberately broken paths that perform full scans, clones, and builds before filtering. Counter observations require internal seams, so these are internal fixtures; details are in the [correspondence worksheet](../superpowers/reports/2026-09-14-issue-491-cost-correspondence-worksheet.md).

## Combined fix validation

The [2026-09-14 integration check](2026-09-14-integration-check.md) records the revision combining Rust changes from ten PRs, conflict resolutions, and all-workspace test results. Integration with dependent fixes ran 21 gates and promoted them to active. Individual PRs were to be merged sequentially after their CI passed.

## Additional execution evidence

The [expanded release measurements](2026-09-14-issue-491-expanded-measurement.json) record 522 runs: 29 shapes × 3 sizes × 3 repetitions × 2 executables. They save output-document bytes, within-binary N/2N/4N ratios, and ratios against baseline, retaining null for unobserved RSS. These runs are separate from the existing 198-run audit. See the [input-axis report](../superpowers/reports/2026-09-14-issue-491-input-axis-review.md) for environment, digests, and sensitivity checks.

Preflight, worker creation, and reset use fixed-length content buffers rather than retaining heap proportional to the largest file. Metadata such as manifest entries scales with file/directory counts. Dedicated allocator gates measure stages using single files of 1/8/32 MiB, with real whole-content reads as the control. This does not establish constant memory or RSS for the entire CLI.

Reset preserves full byte comparison for equal-sized files and does not recreate unchanged files. On an equal-size change, it rewinds the same snapshot handle to restore the file, so snapshot reads for that file can double. For size mismatches, restoration skips worker-content reads. Contract post-validation performs a separate comparison. Source reads for snapshot/hash creation and final preflight revalidation remain one each.

The progress allocator gate keeps its function name while independently increasing report contents (500/1,000/2,000 entries) and history length (2/4/8). It retains the original 2,000-entry × 16-history case. It records actual report bytes and caps additional peak over history length 2 at 512 KiB for each report size. It also checks all inputs are usable and candidate correspondence, scores, differences, and stall decisions are correct. A control retaining every parsed historical report exceeds the same bound. These additional ordinary-gate checks are not part of the 522 release measurements.

## Nested control-flow reanalysis (Issue #546)

The `control-flow-reanalysis-*` exact Rust gates measure a separate input axis:
loop nesting with empty imports, not expression depth or AST width. They count
actual `visit_statement_flow` calls and actual annotation-statement visits at
depths 1, 2, 4, 8, 16, and 20, for both for and while in module and class scope. With the type operator
selected, the bounds are `(d+1)(d+2)/2` total statements and `d+1` annotation
visits (add one total statement for the class definition). The unselected control must perform zero annotation-flow visits, with
zero candidates and no truncation. A second gate re-enables the old replay at
depth 8 and requires it to exceed the same bound without changing candidates.

Only the stable final transfer from the current fixed-point invocation is
reused; no map of cached import snapshots is retained. Annotation recording,
a changed class fallback, and test projections keep their full traversal.
A fallback snapshot is kept only during each active class-loop evaluation,
then discarded; retention depends on fallback size and active nesting depth. These gates
do not establish a quadratic bound for changing import states, class scopes,
or arbitrary finally paths; elapsed time and RSS are not pass/fail metrics.
See the [design](../superpowers/specs/2026-09-15-issue-546-loop-transfer-design.md)
and [review](../superpowers/reports/2026-09-15-issue-546-loop-transfer-review.md).

## Regression gates from audit inputs

`audit-promoted-546` uses for-loop depths 18/19/20 from the original audit; `audit-promoted-547` uses 512/1,024/2,048 imports and int annotations. Both check zero candidates and operation-count bounds with the type operator selected and unselected. They are active inventory gates run by ordinary CI.

Historical audit time/RSS measurements remain unchanged. These gates count visits, clones, and copied elements without time thresholds. See the [promotion verification report](../superpowers/reports/2026-09-15-audit-verification-promotion.md).

## Nested finally reanalysis (Issue #557)

The `nested-finally-reanalysis` exact gate counts actual statement-flow and
AnnAssign visits for nested `try: pass / finally` suites. It covers module,
class and function bodies at depths 1, 2, 4, 8 and 16 through 20. With the type
operator selected, one candidate remains and leaf visits are at most depth+1;
statement visits are at most (depth+1)^2+1, plus one for the class/function
wrapper. Without the type operator, both counters and the candidate count are
zero. The gate detects redundant recording walks in transfer-only finally
analysis without imposing a timing threshold.

Separate tests restore the old walk to demonstrate sensitivity, compare full
candidate descriptors/order, and preserve explicit/implicit exit routing.
These bounds apply to the single-entry fixture, not arbitrary multiple exits
or changing environments. Release observations and review results are in the
[review](../superpowers/reviews/2026-09-25-issue-557.md), with the
[design](../superpowers/specs/2026-09-25-issue-557-finally-traversal.md).

## Multiple exits through nested finally (Issue #987)

The follow-up fixture nests `try: if c: return / finally` inside a function, with
an annotation or `pass` at the deepest point. Equal environments arrive under
multiple completion categories. Its regression checks count actual transfer
work and retained exit states; elapsed time is measured separately rather than
used as a pass/fail threshold.

Local transfer reuse and exact same-category normalization bound transfer-only
statement visits linearly and recording-enabled visits quadratically for this
homogeneous input family. They preserve completion categories and annotation
recording. The optimization excludes enclosing class contexts and observable
test hooks; semantic controls also exercise real cache hits. Distinct import
states and other Python shapes do not inherit this work bound. The number of
cache slots is bounded by live nesting depth, but their retained bytes and whole
process RSS are not bounded by that count.

See the [implementation and measurements](../maintenance/2026-10-19-issue-987-finally-work.md)
for the exact operation/state-count cases, fresh before/after release samples,
cancellation checks and comparison scope. The existing single-entry #557 and
loop #546 gates remain separate regression controls.

The depth20 fixtures passed Python AST parsing and Rust operation/state gates.
A separate attempt at deep CPython bytecode compilation failed with `MemoryError`;
it was not repeated. Successful bytecode compilation is evidenced only for the
depth12/13 release comparison fixtures.


## Indexed admission and immutable metadata (Issues #1006–#1008)

The registry adds four sparse per-file selection comparison gates, separate
manifest-clone/actual-worker retained-heap gates, and an actual CLI discovery
source-index heap gate. They assert work or requested allocation, without timing
thresholds. The [integration archive](../maintenance/2026-10-20-perf-three/README.md)
links exact checked-source receipts, real RED/GREEN evidence and release samples.

Sparse API membership work and manifest metadata retention improve on their
fixed fixtures. Task1's three-pair many-line whole-CLI median increases
9.257→11.657 ms; Task2 filesystem timings are mixed. Task3 reduces simultaneous
location-index retention while retaining two sequential Unicode constructions.
Its [task-local ASCII experiment](../maintenance/2026-10-20-issue-1008/r6/README.md)
did not meet the initial +0.05 ms criterion for small input (+0.067791 ms upper
confidence bound). The user then explicitly accepted +0.1 ms under D1; both
fixtures meet that criterion and retain unchanged requested whole-plan heap.
The context stack grows by 8 bytes. Preserve the initial failure and this
post-observation acceptance decision; these data establish no universal speedup.
