# Task 3 review — #1008

## Spec compliance

**⚠️ Not a full acceptance pass.** The functional implementation, simultaneous-index heap reduction, required measurements, and task process are compliant with the brief. The issue's small-ASCII “負担を増やさない” acceptance condition remains unresolved by the available timing evidence. This is an acceptance-evidence gap, not a demonstrated functional defect or a proven task-local performance regression.

Reviewed base `e5e33603a4ed6a269d4131fbb6dbe99a409de05c` through head `952ab52780b181ae8de316e4b1f45988b5d0d1b0`, including both listed commits. Used the supplied review package; no regenerated diff or Git state commands. Read the complete product/test diff, reviewed measurement/instrumentation scripts and patches, and parsed repeated raw JSON/command receipts rather than printing thousands of repeated records. No tests, benchmarks, builds, hooks, or product/Git mutations were performed. This report is the only written artifact.

## Strengths

- `crates/hoimin-core/src/candidate.rs:183`, `:191`, `:238`, `:613`: the private source-bound `OnceLock<Result<…>>` keeps raw borrowing/decoded ownership intact, preserves eager raw and decoded length checks and hashing, and initializes only at the existing final location check. Cached errors propagate without a new panic. Public signatures and encoding-specific coordinates are retained.
- `crates/hoimin-core/src/candidate_lazy_tests.rs:22`, `:31`, `:43`: constructor-boundary counters test real unused-context deferral, reuse across validations, and separate early-error precedence cases. The length-helper boundary tests avoid giant allocations. `crates/hoimin-core/tests/candidate_policy.rs:580` checks production Send/Sync; `:588` uses an independent bounded scalar-walk oracle over UTF-8 boundaries, BOM, and all three line endings.
- `crates/hoimin-cli/tests/source_index_heap.rs:34`, `:70`, `:93`: the isolated allocator test executes actual discovery with the source read inside the window, fixes one boolean candidate/max1, and requires an extra real retained index to exceed the same bound. The recorded old eager implementation also genuinely fails, rather than relying only on that proxy.
- `crates/hoimin-cli/tests/source_encoding_heap.rs:44` now forces actual location validation, keeping the Latin1 retention test sensitive after deferral. `crates/hoimin-cli/tests/source_index_encoding_flow.rs:24` covers ASCII/UTF8/BOM/Latin1 × LF/CRLF/CR through plan/run/verify, raw spans/hash, route-stable IDs, actual worker bytes, and preserved original workspace bytes. `:123` preserves decoding-before-syntax diagnostics.
- `crates/hoimin-python/src/analyzer/rust.rs:825` removes only the ignored source argument from the private lookup method and updates the affected production/test callers. No public source/index pair, Task1 selection redesign, manifest change, payload charge, or compatibility switch was introduced.
- `docs/maintenance/2026-10-20-issue-1008/README.md:15`, `:17`, `:19`, `:21`: the evidence distinguishes requested heap, process RSS, logical payload and disk; identifies cumulative Tasks1/2/3 timing; admits unchanged successful build counts and ASCII uncertainty; and preserves initial and confirmation samples.

## Issues

### Critical

None found.

### Important — acceptance evidence remains open

**I1. Small-ASCII no-burden acceptance cannot yet be marked satisfied.** `docs/maintenance/2026-10-20-issue-1008/README.md:21`, `verification.json:29`, `small_controls.py:7`.

The original three-pair ordinary small-plan medians are 5.244→8.726 ms and remain preserved. The one bounded 12-pair confirmation has aggregate medians 5.510→5.850 ms; reconstructing matched pairs gives an after-minus-before median of **+0.2365205 ms**, mean +0.245913 ms, with **8 positive and 4 negative** differences and range −0.634333…+1.049833 ms. These observations must not be called no regression. Conversely, twelve noisy observations and a cumulative original→Tasks1/2/3 comparison do not establish that #1008 causes a slowdown; treating 8/12 as decisive would overstate the evidence. The more local small API pipeline is nearly flat (aggregate 0.298958→0.299271 ms; paired median +0.003125 ms; six increases/six decreases), which helps isolate the affected flow but cannot discharge the ordinary CLI condition.

The design explicitly permits two sequential builds and requires measuring the lazy-cell overhead; it does not waive the issue's ASCII acceptance. The 160→168-byte context stack increase is real and should remain disclosed, but by itself does not establish higher overall memory burden: tiny context heap decreases 80→64 bytes, small context heap 576→64, API/discovery peaks decrease, and small plan requested peak is unchanged. Do not misclassify this fixed representation tradeoff as proof of a whole-flow memory regression.

**Required disposition:** keep the small-ASCII acceptance item open. Before calling #1008 fully accepted, the controller must resolve it with a focused task-local comparison under a defined practical tolerance, or an explicit acceptance/spec decision. Replaying the same suites or repeatedly sampling until favorable would not resolve it. This review does not request an automatic redesign of the otherwise sound lazy implementation.

### Minor

None requiring a code change. The two failed commit-hook attempts are historical corrected failures, not outstanding lint or compile findings: the changed formatter and private test calls are present in the final diff, their focused reruns pass, and the final hook receipt is clean (`logs/commit-success.log:1`).

## Evidence audit

- `logs/red-materialization.log:17`, `:23`: actual eager constructor counts fail both new materialization assertions. `logs/red-cli-heap.log:8` records discovery peaks 4,722,483 / 9,440,528 / 18,877,712 bytes, followed by the real bound failure. `logs/red-context-heap.log:6` is correctly labeled a later original-baseline sensitivity check, not pre-edit TDD.
- `logs/green-heap.log:8` covers all twelve Latin1 density/size measurements with forced validation. `:27` records Unicode unused context 64 bytes and actual discovery 2,625,315 / 5,246,208 / 10,489,088 bytes; extra-index controls 4,721,936 / 9,440,528 / 18,877,712 still exceed each corresponding `12N+262144` bound. These support reduced simultaneous retention, not total CLI heap halving.
- Parsed all 210 final probe measurements, 60 ordinary CLI samples, 96 small-confirmation observations, and 75 observations per instrumented variant. Fixed-workload candidate counts/digests agree across paired variants. Independently recomputed heap/time medians, paired ASCII differences, and each constructor/drop event sequence; all reported build/drop/max-live tuples agree. Successful pipeline/discovery/plan remain two builds with maximum live 2→1; run remains three builds with maximum live 2→1; unused context goes 1→0 builds.
- `instrument.py:19`, `:28`, `instrument-*.patch:17`, `:29`: private instrumentation logs real constructor completion and ownership Drop boundaries using global atomics, including CLI worker threads. Counts are not inferred phase increments. The eager source is explicitly task3-base with earlier tasks retained, not pristine original. `instrument_runs.py:6` restores source in `finally`. No permanent cross-crate public test bridge was added.
- `build_probe.py:4`, `measure.py:17`, `probe.rs:25`, `:46`: paired builds use matching optimized thin-LTO options and the same probe; runtime/caller setup precedes requested-heap windows; ordinary process timing uses separate uninstrumented executables. Checked all eight measurement provenance hashes against the named existing files/binaries, all four probe binaries against their build receipts, and all nine final source hashes against `verification.json`; all match.
- Parsed every `logs/*.command.json`: bounded Cargo/test/hook executions use the specified shared target, two build jobs, disabled incremental, two test threads, serialized pre-commit, and existing venv/cache. Saved passing receipts cover 304 distinct focused cases and 12 explicit manual/fixture ignores; the initially filtered-out integration run is honestly superseded by the dedicated unfiltered run. Final hooks pass without residual warnings/errors. Whole-workspace/contracts/Python/full-performance integration remains controller-owned.
- The report supplies five substantively different design, implementation, and validation reviews. Their ownership/error/encoding/resource/evidence concerns correspond to actual changes and receipts, rather than five repeated check marks.

## Focused unchanged-code reads and their reasons

1. **Risk: lazy initialization could reorder strict rejection checks.** The supplied validator hunk starts mid-function, so read its enclosing `candidate.rs:557` function once. Schema/path/hash/mutation/span/original/decode/mapping/replacement still precede location construction, and stable ID still follows location validation.
2. **Risk: the analyzer might retain its index through conversion, or lazy construction might precede admission/cancellation.** Read only the two affected CLI call-site ranges (`crates/hoimin-cli/src/analyzer/mod.rs:455`, `:715`) and the conversion/source-validation helpers (`:874`, `:890`). Analysis returns before conversion, output owns candidates, existing cancellation checks remain before conversion, and metadata/scratch admission still precedes the strict validator. The actual-flow heap gate and separate construction/drop traces answer the lifetime concern without broader code crawling.
3. **Risk: process time/RSS or timeout labels might overclaim the wrapper's behavior.** Read the previously established `docs/maintenance/2026-10-20-issue-1006/resource_wrapper.py:1` once. It runs one bounded 25-second child per fresh wrapper and reports elapsed plus `RUSAGE_CHILDREN`; the new documentation correctly avoids aggregate-process-tree or heap labels for RSS.

No unanswered functional doubt justified another test run. Existing strict plan/review/worker, payload-exact-limit, and cancellation receipts answer the preservation questions; final integration is outside this task gate.

## Assessment

**Task quality: Approved.** The localized lazy implementation is maintainable, preserves source/encoding/error contracts, and has meaningful behavioral and heap sensitivity tests with unusually explicit provenance. No product correctness or maintainability fix is identified.

**Task spec gate: remains open solely for I1.** Functional/heap work can proceed to controller integration review, but an unconditional spec-compliance verdict or closure of the small-ASCII acceptance item is not supported yet. Neither a proven task-local slowdown nor compliance should be inferred from the existing noisy, cumulative timing data.
