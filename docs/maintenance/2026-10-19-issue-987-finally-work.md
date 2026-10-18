# Issue 987: share equivalent finally transfers

`apply_finally` now owns one reusable completed transfer. The finalbody, enclosing scope and qualname, implicit-exception tracking, and transfer-only observation mode remain fixed for that owner's lifetime. Exact typed `KnownImports` equality is its varying key. Sharing and same-category exact normalization are disabled when the enclosing collector has a `ClassContext` or a test projection/mutation hook. Class fallback semantics retain the existing traversal.

The separate joined-entry annotation traversal still records callbacks and candidates in their original order. A transfer hit restores residual `collector.imports`, including after abrupt completion, before returning its exit snapshots. A miss drops the previous slot before recursion and admits only a completed transfer. No annotations or callbacks are cached. Stable normalization keeps the last occurrence of each exact environment in its original relative order within each abrupt category; normal, break, continue, successful return, explicit raise and implicit exception remain separate and can coexist. Distinct explicit environments are not intersected.

Annotation collection now accepts a borrowed cancellation predicate and latches observation. Suites, statements, finally entries including cache hits, and loop fixed-point iterations poll; child returns also poll before subsequent work. Scope/context/recording/tracking restoration still runs while unwinding. The production boundary returns `AnalysisError::Cancelled`, never successful partial candidates. Already observed candidate-payload failure retains priority. Empty test adapters remain uncancellable.

## Work and retained state

Actual statement visits, transfer misses/hits and pre-normalization vector sizes were measured independently for the reported valid function family: each nested try has `if c: return`, its finally contains the next level, and the leaf is `x: int` or `pass`.

| Depth | Transfer-only statements (body) | Transfer misses/hits | Recording statements (includes def) | Recording misses/hits | Raw max abrupt vector |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 4 | 1/2 | 6 | 1/2 | 1 |
| 2 | 7 | 2/4 | 13 | 3/6 | 4 |
| 4 | 13 | 4/8 | 36 | 10/20 | 4 |
| 8 | 25 | 8/16 | 118 | 36/72 | 4 |
| 12 | 37 | 12/24 | 248 | 78/156 | 4 |
| 20 | 61 | 20/40 | 652 | 210/420 | 4 |

Transfer-only traversal visits the annotation leaf once and retains one normalized return environment at each tested depth. Fallthrough and implicit exception remain present. Recording visits the annotation leaf `depth + 1` times but records exactly one annotation. The pass variant has the same statement/transfer/vector measurements and zero annotation visits. Unselected type operators perform zero annotation-collector work.

These are linear transfer-only and quadratic recording work bounds for this homogeneous family. The cache has O(depth) slots; each contains source-dependent import and exit snapshots, so this is not a byte or RSS bound. Stable Vec normalization takes O(n*k) equality comparisons for n incoming states and k distinct states, quadratic for all-distinct input. Genuinely distinct states and class-context paths have no new universal bound. No exploration cap or candidate-budget accounting was added.

## Validation

Cargo used the shared owned target `/tmp/hoimin-issues-984-989/target`, jobs2, dev/test debug information disabled, incremental=false, offline=true and the task-owned TMPDIR. Cargo and formatting were serialized. Logs are in `/tmp/hoimin-issues-984-989/`.

- RED: `cargo test --locked -p hoimin-cli --lib finally_transfer_tests -- --test-threads=2 --nocapture` failed both new regressions on old production behavior: depth1 leaf visits3 rather than1, and cancellation after20 statement visits continued to902. Five existing tests passed. An initial wrong filter selected zero tests; it was corrected and is not verification evidence. Log: `987-red.log`.
- Final focused tests:23 passed after hook corrections. They include independent six-pending-category by five-finally-completion expectations; twenty distinct typed A/B pairs with A/B/A eviction; actual-hit callback/full-candidate differential comparisons; residual-import restoration; class global/nonlocal fallback and duplicate-state bypass; four observer/mutation-hook bypasses; no-annotation, transfer-only and hit cancellation; and payload-error priority. Logs: `987-focused-final.log` (22), `987-hook-corrections.log` (23). The final added case verifies class-context cancellation; match-guard cleanup also passed63 matching library tests (`987-match-correction.log`).
- `cargo test --locked -p hoimin-cli --lib analyzer:: -- --test-threads=2`:396 passed,13 existing ignored. Includes #546/#557 work gates and existing internal flow/scope oracles. Log: `987-analyzer-suite.log`.
- Existing Rust integrations `lean_binding_flow_oracle`, `lean_nested_try_flow_oracle`, `lean_annotation_scope_oracle`, `lean_implicit_finally_oracle`, `lean_except_star_flow_oracle`, `with_suppression`, `lean_prepared_annotation_import_oracle`, `lean_private_annotation_import_oracle`:24 passed across eight top-level suites, with two test threads. Log: `987-flow-oracles.log`. No Lean model changed or Lean rebuild was needed.
- `cargo fmt --all` and `git diff --check` passed. The first normal hook run passed both formatters and vendored-parser Clippy but found eight workspace Clippy style issues. They were corrected without suppressions; final normal hook outcomes are retained in the task report.
- CPython3.12.6 AST generation independently accepted all12 depth/leaf fixtures. Ordinary bytecode compilation succeeded for release depths12/13. A separate attempt to compile all depths to bytecode exited1 with `MemoryError` before its summary; no per-case progress or peak RSS was captured, and no explicit memory limit was set. Deep ordinary compilation was not retried. The depth20 claim is Rust work/state testing plus Python AST validity, not successful bytecode generation. Log: `987-python-fixtures.log` records the successful AST-only check.

## Fresh same-host release comparison

The actual task base was `d0d2472ed6797fcc07b286af3056e19dccbedc98`. Its release executable was built before production edits and copied alone into owned scratch; SHA256 was `00433c4570d2e13353b32d3052c99c9614c4422c1edd0591ee3e6f9708c609fd`. The changed release executable SHA256 was `96e36ffc0dc37b8be77b1b79af8f6d06655d1d480ad3fdd3a2ef172cc8894739`.

Each isolated project ran `plan --file input.py --operators type_nullable_add --allow-best-effort-memory -- python3 -c pass`, three samples including the first invocation. Seconds below are observations, not acceptance thresholds.

| Depth / bytes | Before samples | After samples | Before / after median |
| --- | --- | --- | --- |
| 12 / 1784 | 0.506595, 0.246887, 0.246516 | 0.238719, 0.006590, 0.005863 | 0.246887 / 0.006590 |
| 13 / 2035 | 0.735140, 0.731694, 0.744611 | 0.009753, 0.008217, 0.009426 | 0.735140 / 0.009426 |

The after measurements above use the rebuilt source after the residual-order review correction. Both earlier after snapshots remain archived separately. Every before/after sample produced one candidate and no diagnostics or truncation. Full candidate descriptors were equal, including ID, sequence, path, span, replacement, symbol, source hash and ranking. Depth12 retained ID `m1_0b7a9527ec12ada338f955482bb563fc581675f77d2b4620656645149f9bbaf5`, span1780+3; depth13 retained ID `m1_5f5d853a25355ee38d6572b6d2dc173e3e1031b182a8970398f39666af7496b8`, span2031+3. Both replace `int` with `int | None` in symbol `f`.

The owned baseline executable was deleted after saving its hash, all JSON outputs and metrics. Logs: `987-baseline-build.log`, `987-fix1-release-build.log`, `987-before.log`, `987-after-fix1.log`, `987-fix1-release-comparison.log`; raw artifacts: `987-measure/`. Historical issue-audit timings were not used as the fresh comparator. Target usage grew approximately5.7→6.0GiB; shared Python environment and main Lean artifacts were untouched.

## Five-pass review record

Requirements/design:

1. Inspected measured normal/return/implicit recurrence; per-category dedup alone cannot remove cross-category repeated traversal, so local transfer reuse is also required.
2. Inspected suite residual state and scope epilogues; added residual-import replay to the cached-result contract.
3. Inspected class external projection; gated both reuse and deletion of duplicate states, since equal imports can observe different fallback states.
4. Inspected projection/mutation observations; retained hook bypass and separate callback-recording traversal.
5. Inspected callback error ownership and loop convergence; chose latched internal cancellation, restored lexical state and preserved payload-error priority.

Plan:

1. Required an actual-base release build before production edits, executable-only copy and checksum rather than historical timings.
2. Put actual work and no-annotation cancellation RED first; restricted disabled reference to depths1/2/4.
3. Required direct return/raise category assertions and production-shaped real-hit comparisons, beyond test projection adapters.
4. Required independent pre-normalization state-size counters and one-slot eviction before recursion; no byte-bound claim.
5. Kept #986 visitor and #989 defaults unchanged; task sources/maintenance alone staged, relevant suites serialized, canonical contracts and independent review left with the coordinator.

Implementation:

1. Inspected slot ownership and restored flags; corrected cached-result normalization as well as final output normalization.
2. Inspected cancellation after child work; added try tracking restoration before early return and with/loop post-child checks before subsequent scanning.
3. Inspected category routing and class normalization; added independent 6×5 expectations and all-category actual-hit preservation.
4. Inspected test independence; canonicalized only semantic state-set comparisons while retaining raw vector gates, and required actual hits for full candidates/callbacks.
5. Inspected complexity; replaced repeated Vec removal with stable in-place unique-prefix compaction to avoid shifting duplicate suffixes repeatedly.

Verification:

1. Audited actual selected counts and corrected the zero-test filter; focused23, analyzer396/13ignored, oracle24 and final match63 results are distinct evidence; the wider suites precede the final narrow correction.
2. Matched independent work/hit/miss/vector measurements and narrow-family limits; no universal class/distinct-state/RSS conclusion.
3. Checked baseline RED and low-depth disabled-reference sensitivity without deep exponential reference execution or elapsed-time assertions.
4. Recorded macOS/CPython scope and the ordinary Python compiler MemoryError; retained AST-only depth20 validation and did not retry deep bytecode compilation.
5. Compared all release descriptors and recorded exact-base hashes and all samples; removed owned comparison binary and preserved the coordinator's12 unstaged docs.


## Review correction: preserve residual order

Independent task review found a semantic defect in the original first-occurrence rule: pending returns `[A, B, A]` became `[A, B]`. When no later explicit/implicit category runs, the collector keeps B's residual imports instead of A's; unreachable annotations then lose provenance and candidates. The exact valid fixture is now covered directly:

```python
def f():
    from typing import Sequence
    try:
        return
    except ():
        Sequence = 0
        return
    except ():
        return
    finally:
        pass
    after: Sequence[int]
```

On commit `8fabc15a2cdfd813043b7c9da830c7385bc17a1a`, three new tests failed: direct A/B/A residual imports were empty, the callback lacked `direct:Sequence=typing.Sequence`, and `type_list_sequence` produced zero candidates instead of the reference's one `Sequence[int]`→`list[int]` candidate at span183+13 in symbol `f`. Log: `987-fix1-red-behavior.log`. An earlier test compilation error comparing usize to u64 was corrected before this behavioral RED; it is not failure-sensitivity evidence.

Normalization now removes earlier occurrences, preserving the last occurrences in their original relative order. Reverse, existing in-place unique-prefix compaction, then reverse achieves this without extra snapshots or transfers. This intentionally revises the internal first-occurrence ordering choice. Public candidate/callback order, exact typed state sets and residual imports remain the requirements. Keeping only the original overall tail is insufficient in general: other output categories may receive their final contribution from a different entry. Deleting earlier copies of an equivalent transfer preserves the last contributing entry for every output category. The fixed-context/no-ClassContext/no-observer proof obligations remain unchanged.

Five correction-review passes:

1. Reproduced the reviewer fixture without implicit-exception entries that could mask the wrong tail. Verified direct residual, callback provenance and full candidate loss separately before production edits.
2. Reviewed the ordering law and rejected preserving only the overall tail. Last-occurrence ordering preserves each category's final contributor under equivalent deterministic transfers; the coordinator explicitly approved replacing the internal first-occurrence choice.
3. Reviewed ownership, exact typed equality and cost: two in-place reversals add linear traversal without additional caches/clones; all gates, cancellation and category distinctions remain. O(n*k) equality comparisons and homogeneous work/state bounds are unchanged.
4. Reviewed test independence: an index-based last-occurrence oracle covers364 words over three typed states in all four abrupt vectors, with all eight contributor subsets. Another242 valid generated handler programs, with and without outer finally, compare callbacks/full candidates against sharing disabled and independently assert the final Sequence/Mapping facts. Updated the existing A/B/A internal ordering expectation rather than treating it as a public contract.
5. Audited execution and artifacts: focused28, loop-neighbor6 and existing oracle24 pass; their exact logs and exit codes are in the task report. Python3.12.6 ordinary compilation accepts the exact fixture plus242 small programs. No depth20 bytecode retry or broad workspace repetition. Final release measurements and hooks are recorded below.

Correction checks use the same owned Cargo environment and sequential execution. `cargo test --locked -p hoimin-cli --lib finally_transfer_tests -- --test-threads=2 --nocapture` passed28 (`987-fix1-focused.log`), including all original operation/state/cancellation gates. `cargo test --locked -p hoimin-cli --lib nested_loop_transfer_tests -- --test-threads=2` passed6 (`987-fix1-loop-neighbors.log`). The same eight oracle/with integration binaries listed above passed24 (`987-fix1-oracles.log`). `cargo fmt --all` and `git diff --check` passed. Python fixture evidence: `987-fix1-python.log`.


The review correction's release rebuild passed (`987-fix1-release-build.log`). Three refreshed samples at each depth retain the initial sample and match every saved baseline candidate field, including ID/span/replacement/order. The main comparison table above now uses `after-fix1`; previous `after` and `after-final` artifacts remain intact. `987-fix1-release-comparison.log` records medians0.006590s/0.009426s at depths12/13, compared with saved baseline0.246887s/0.735140s. Normal correction-commit hook results are recorded in the task report.


The correction's first normal hook run found only a test-generator `format_collect` style issue; both formatters and vendor Clippy passed. Replacing that generator expression with a writeln loop changed no production code. All five residual-order tests passed again (`987-fix1-hook-correction.log`); the recorded release binary still matches the final production source.
