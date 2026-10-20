# Optional keyword admission (#1030)

Base: `1f7feae33fa310aae87fbadb390105f6d770439d`. Worktree:
`.worktrees/issues-1029-1030`; branch: `perf/issues-1029-1030`.

The old `collect_call` invoked `FunctionIndex::optional_keywords`, enumerated
keywords, and built each whole-call replacement before `make_candidate` checked
the start line and scope. An excluded call therefore consumed work outside the
retained candidate payload budget. The guard belongs on the optional-keyword
branch of `collect_call`; returning from the expression visitor would discard
selected child expressions.

## Design reviews

1. Compared the guard with `make_candidate`: both use the range start's line and
   `facts.scope_at(start)`, followed by `CandidateSelection::includes`. Reuse the
   existing `expression_selected` helper, accepting `impl Ranged` so `ExprCall`
   does not require a cloned `Expr`.
2. Traced `visit_expr` after `collect_call`: it walks arguments independently.
   Restrict the new guard to the optional-keyword condition; later call producers
   and argument traversal remain reachable.
3. Checked multiline and symbol selection. Interior lines do not select the
   enclosing call; nested calls and Boolean literals on selected lines must
   remain eligible. Include reversed-only ranges and symbol-only exclusion.
4. Checked limit and payload boundaries. The guard must precede signature
   lookup even for zero payload and limits 0, 1, and 10,000. Keep the existing
   selected builder limit, candidate admission, ranking and payload code.
5. Checked measurement scope and cancellation. Test counters observe lookup,
   real keyword visits, completed replacement strings and their byte lengths.
   Release probes use unchanged public APIs and separate requested allocation,
   retained/peak requested heap and elapsed time; no metric is RSS.

## TDD evidence

Parent's clean baseline: `cargo test -p hoimin-python --lib`, 457 passed,
3 ignored. The task's first compilation exposed two incorrect error variant
names in new tests; corrected to the existing `PayloadLimit` and `Cancelled`
variants before evaluating RED.

Command (shared target directory):

```sh
CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target \
  cargo test -p hoimin-python --lib optional_keyword_selection_tests
```

On the old product behavior, 12 tests failed and 2 passed. The failures measured
real work before candidate admission:

| Excluded 1,000-keyword call | Lookup | Keyword visits | Replacements | Replacement bytes |
| --- | ---: | ---: | ---: | ---: |
| limit 0, payload 0 | 1 | 1,000 | 1 | 8,888 |
| limit 1, payload 0 | 1 | 1,000 | 2 | 17,776 |
| limit 10,000, payload 0 | 1 | 1,000 | 1,000 | 8,886,110 |
| limit 10,000, unrestricted payload | 1 | 1,000 | 1,000 | 8,886,110 |

The bounded property shrank to one keyword, candidate limit 0, payload 0:
one lookup, one keyword visit, one replacement, five replacement bytes.
The generated regression seed is retained. Selected payload exhaustion and
cancellation after the first constructed replacement already passed.
The first RED run also exposed an incorrect test assumption that truncation
has no diagnostic; corrected to the existing `CandidateLimitExceeded` contract.
Detailed logs live under `.superpowers/issues-1029-1030/optional-*`.

Corrected RED again failed 12 tests solely on the excess work observations.
After adding the guard, all 14 tests passed, including 24 generated property
cases and the retained regression seed.

## Implementation reviews

1. Inspected the complete production diff: one existing helper parameter changes
   from `&Expr` to `&impl Ranged`, and one short-circuit predicate precedes
   `optional_keywords`. The helper body, `emit` behavior, and candidate admission
   code are unchanged.
2. Checked every caller of `expression_selected`: condition and container
   producers still pass the same `Expr`; the call producer passes its original
   `ExprCall`. No clone, allocation, range conversion or public API is added.
3. Checked exclusion scope: the predicate resides inside the optional branch.
   `collect_stdlib_conversion_call_remove`, conversion/method producers and
   `visitor::walk_expr` remain outside it. Final review identified a method-name
   replacement whose line differs from the call start; the additional test below
   directly protects this path.
4. Checked cancellation and failures: the existing collector failure check and
   visitor cancellation checks remain; selected `emit` still stops on
   cancellation or payload failure. Skipping work may complete before a
   wall-clock deadline that previously cancelled; callback counts are not an
   observable compatibility contract.
5. Checked instrumentation: counters and helper functions compile only under
   `cfg(test)` and use thread-local state. The release probe measures the public
   analyzer using its own allocator, without a product test-support feature.

## Verification reviews

1. RED tests fail on actual lookup, keyword enumeration and replacement work,
   including a budget of zero; they do not infer work from candidate count.
   The one-keyword shrunk counterexample also covers limit zero.
2. Selected multiline tests compare full candidate objects with unrestricted
   analysis and use literal replacement expectations. Separate tests cover
   truncation diagnostics, zero-budget failure and in-builder cancellation.
3. Nested optional-call and Boolean-child cases prove the visitor still reaches
   selected interior expressions. Symbol-only and reversed-range exclusions
   exercise the same prepared selection contract.
4. `cargo test -p hoimin-python --lib` passed 494 tests, with the existing
   3 ignored tests, after both issue implementations were present.
   `cargo test -p hoimin-cli --test optional_keyword_delete` passed all 6 tests,
   including generated-candidate CPython compilation, saved-plan execution,
   binding hazards, Unicode spelling and argument evaluation order.
5. The 19 release workloads each ran five before/after pairs, in alternating
   order and without competing builds or tests. All 190 invocations agreed on
   full result/ID/error and fixture digests. Reviewed the meter's full-size
   reallocation accounting, signed retained delta, setup exclusion and small
   input batching. The parent owns final workspace, Clippy and Python-support
   verification.

## Release probe method

`build_probe.py TARGET_DIR SOURCE_RS OUTPUT_BINARY` links the same probe against
one release artifact set. It rejects ambiguous rlibs and writes a sidecar with
the exact rustc command, compiler version and source, rlib and binary SHA-256.
`optional_probe.rs` accepts `KEYWORDS MODE MAX_CANDIDATES PAYLOAD_BYTES ITERATIONS`.
Modes include `excluded`, `selected`, `all`, `reversed`, `symbol`,
`optional-child` and `boolean-child`.

Source generation, selector/request construction and one warmup are outside the
measurement. A batch measures analysis and the drop of prior batch outputs,
retaining the final output for the heap snapshot. Requested-byte live totals
count successful allocations globally; cumulative bytes count successful
allocation and zeroed-allocation sizes plus the full new size of each successful
reallocation. Retained delta is signed, and peak is measured relative to live
bytes immediately before analysis. These metrics exclude allocator metadata,
usable-size rounding and RSS. Time is from the allocator-instrumented release
binary and is not a CI threshold.

After the snapshot, the probe hashes the full candidate fields, stable mutant
IDs, diagnostics and truncation. This catches result differences even when
candidate counts agree. Fixture hashes and callback counts are also reported;
the unit counters, rather than callback count, are the deterministic work gate.
The shared meter checks allocation, growth, shrink and release against known
16/32/8-byte requests before every warmup.

## Release results

macOS 15.7.7, arm64, rustc 1.99.0 (`b940084d7`), optimized release rlibs, probe
`rustc -O -C lto=thin`. The base release was built from the detached clean
`1f7feae33fa310aae87fbadb390105f6d770439d` checkout. The after release includes
both #1030 and #1029 changes. The optional-only requests below do not request
the exception operator.

The [raw results](optional-measurement.json) include all samples, exact probe
commands and SHA-256 hashes of binaries, probe sources and linked rlibs.
[Production source hashes](production-source-manifest.json) identify the three
modified production files in the before and after builds. Final test-only
additions and documentation changes are listed there; they do not change the
measured release behavior.

For an excluded call, payload 0, candidate limit 10,000, medians were:

| Keywords | Time before → after | Cumulative requested bytes before → after | Peak requested bytes before → after |
| ---: | --- | --- | --- |
| 1,000 | 14.641 ms → 0.556 ms | 34,082,971 → 1,236,211 | 624,465 → 597,325 |
| 2,000 | 57.981 ms → 0.939 ms | 133,695,211 → 2,465,531 | 1,242,833 → 1,191,309 |
| 4,000 | 215.429 ms → 1.830 ms | 529,527,683 → 4,924,163 | 2,479,569 → 2,379,277 |
| 10,000 | 1,318.210 ms → 4.867 ms | 2,639,116,603 → 16,396,043 | 7,404,241 → 6,854,541 |

Retained requested-byte delta was 0 for every excluded result, before and after.
These calls still incur parsing, signature indexing and child traversal costs.
The large cumulative reduction is not a comparable reduction in peak memory.
At candidate limit 1, the 1,000-keyword median was 0.483 → 0.446 ms,
1,384,499 → 1,236,211 cumulative bytes; the previous builder had already bounded
the number of replacements, so the improvement is smaller.

For one-keyword inputs, each sample batched 300 analyses:

| Selection | Time before → after per analysis | Cumulative requested bytes before → after |
| --- | --- | --- |
| Unrestricted | 5.116 µs → 5.502 µs | 13,135 → 13,135 |
| Selected call line | 4.678 µs → 4.584 µs | 13,143 → 13,143 |
| Excluded call line | 3.884 µs → 3.675 µs | 10,548 → 10,375 |

The unrestricted small-input median increased by 0.386 µs. Observed ranges
overlap (before 4.984–5.418 µs; after 4.873–6.397 µs); this sample does not
establish a universal small-input speedup or zero cost. Selected small-input
allocation, retained bytes (605) and peak bytes (8,519 for line-selected) were
unchanged.

Selected 64-keyword cases cover limits 0, 1 and 10,000 and zero/default payload;
symbol selection covers both complete and truncated results. Multiline optional
and Boolean child cases and reversed-only selection also matched the base's
complete output digests. No time threshold was introduced.

Reproduction after building the before and after release artifact directories:

```sh
python3 docs/maintenance/2026-10-20-issues-1029-1030/build_probe.py \
  target/issues-1029-1030-base \
  docs/maintenance/2026-10-20-issues-1029-1030/optional_probe.rs \
  /private/tmp/optional-before
python3 docs/maintenance/2026-10-20-issues-1029-1030/build_probe.py \
  target \
  docs/maintenance/2026-10-20-issues-1029-1030/optional_probe.rs \
  /private/tmp/optional-after
python3 docs/maintenance/2026-10-20-issues-1029-1030/measure_optional.py \
  /private/tmp/optional-before /private/tmp/optional-after \
  /private/tmp/optional-measurement.json
```

The recorded commands use absolute shared target paths; the example assumes
execution from the checkout that owns those target directories. The two measured
binaries are preserved in `.superpowers/issues-1029-1030/optional-{before,after}`.
These are synthetic local measurements, not native Linux/Windows results or a
new formal proof.

## Final selected-method regression

Review found that `text` and `.startswith(...)` can occupy different source
lines. Selecting only the method line must still yield the `startswith` →
`endswith` candidate when optional-keyword deletion is enabled and the call
start is excluded.

Added `selected_method_name_survives_excluded_call_start`. A temporary bad
implementation returning from all of `collect_call` when its start is excluded
failed this test with 0 candidates instead of 1
(`optional-method-bad-guard.log`). Removed the mutation, then checked that all
three production source hashes again exactly match the measured after build.
No cargo-mutants command was used. This is a test-only addition after release
measurement; it does not require a new performance run.

Final `cargo test -p hoimin-python --lib` passed **495 tests, 0 failed, 3 ignored**
(`optional-final-python-lib.log`). This includes all 15 optional-selection tests
and the other worker's exception regressions. `git diff --check` was clean.
