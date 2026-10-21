# Performance fixes for issues #1039, #1040, #1041, #1042, #1044 and #1045

## Scope and invariants

Base: `a892eee0` (main). These six issues authorize internal optimizations with unchanged candidate identity, order, diagnostics, truncation, input validation, resource limits and cleanup ownership. The issue bodies are the acceptance specification:

- #1039: extract private visible-alias roots once per collector that visits classes; preserve mangling and exclusions.
- #1040: resolve qualified annotation spellings by their first identifier; preserve unqualified alias ordering and allowed-name checks.
- #1041: build preview payloads only for dry runs after the same validation; ordinary execution retains no display-only payload.
- #1042: reuse traversal depth when sorting worker entries; preserve depth/native-path order and no-follow restoration.
- #1044: intersect related exception identities with module visibility without visiting every invisible sibling at each site; preserve eligibility reasons and cancellation.
- #1045: skip ring-spool seeks only when the cursor is known to be aligned; preserve wrap, pending-write errors, bounded drain and first-failure ownership.

Issue URLs use `https://github.com/HayaoSuzuki/hoimin/issues/NUMBER`.

## Resource and measurement policy

One worktree and one build process at a time; reuse `/Users/hayao/RustroverProjects/hoimin/target`, set `CARGO_BUILD_JOBS=2`, and share the existing Python environment. Keep benchmark fixtures and logs under this plan's ignored scratch directory; retain only compact measurements and reproduction instructions in Git. Check disk use between build phases. Never clean another task's target or the shared environment.

Regression gates use real production operation counts and allocator observations, not millisecond thresholds. Compare release binaries built from the same locked dependencies; retain only the baseline executable rather than a second build tree. Measure small inputs and independently varied issue axes, validate output equality, and distinguish allocator heap from RSS, stage timings from whole CLI timings, and calculated counts from measured counts. No smaller candidate limits or reduced validation count as optimizations.

## Design

Use a lazy per-collector set of borrowed private alias roots for #1039. For #1040 use the first qualified identifier to select its single module binding. For #1044 keep one constructor-compatible count per existing child group and choose the smaller of visible identities and adjacent identities at each site; retain no all-pairs cache.

Separate execution preparation from preview preparation with a shared internal checked result for #1041. A normal `VerifiedPlan` carries only execution inputs; a dedicated preview preparation entry point constructs display data from the same checked selection. Update consumers directly, without a compatibility wrapper that recreates discarded payload.

Store the already computed depth in `WorkerEntry` for #1042. Keep the output ring algorithm for #1045, making its private file boundary measurable and retaining seeks for zero logical position, skipped offsets and within-chunk wraps.

## Evidence

### Deterministic regressions

| Issue | Observed failing baseline | Passing bound / behavior |
| --- | --- | --- |
| #1040 | 32 imports, one qualified request: 32 binding visits | At most one lookup per request; imports 1/32/256 × annotations 1/16/64 |
| #1039 | Eight classes, one public alias: 24 alias visits | At most one scan of module aliases; aliases 1/32/128 × classes 0/1/8/32 |
| #1044 | Eight sibling classes, one raise: nine related visits | At most two visible visits per site; siblings 8/64/256 × sites 1/8/32 |
| #1041 | 4 KiB candidate text, 16 selected: 139,286 bytes retained after preparation | Less than 64 KiB requested-live delta; text 4/128 KiB × selected 1/5/16; runtime warmed |
| #1042 | Restoring the old comparator: depth 1/files 1 visits three components | Zero sorting component visits; depths 1/8/32 × files 1/32/128 |
| #1045 | Sixteen aligned writes: sixteen seeks | One initial seek; real-file bytes checked for boundary, wrap, oversized, empty and zero capacity |

These are operation or allocator observations at the stated boundaries. They do
not establish a wall-time threshold, a whole-validation peak-heap bound, or a
process RSS bound. The normal-verify gate uses the real public checked API;
manifest/source parsing remains necessary. Preview JSON is still allocated on
request. Existing dry-run comparisons exercise the separately prepared preview.

The hierarchy reference test preserves exhaustive eligibility results, including
constructor policy, invisible relatives, alias ordering/offsets and exclusions.
The existing analyzer suite additionally covers private mangling, generic scopes,
shadowing and cancellation. Worker tests retain no-follow/depth/ownership/reset
coverage. The Lean-generated output-retention corpus remains unchanged.

### Retained state and review

Qualified lookup adds no cache. Private roots use a lazy collector-local borrowed
set, O(P) distinct private roots, with no scan if no eligible class is visited.
The related-class index adds one `usize` per existing child group, with no
class-pair cache. Worker entries add one `usize` for already validated depth
(8 bytes per field on this 64-bit target). Ring writes add no retained buffer or
cursor field. These are representation costs, not measured process RSS.

A fresh static review found two Important issues, both resolved. A transparent
test-only native-path wrapper now observes actual component iteration, with a
positive control; restoring only the old comparator fails the gate. Skipping a
Tokio seek also skipped pending-operation error normalization. The aligned branch
now polls completion without starting a seek. A real read-only file test first
failed with OS code `Some(9)` versus the baseline's `None`, then passed comparing
kind, OS code and display text. Existing first-error/EOF-drain/drop/finalize tests
remain in the process suite. The reviewer confirmed both fixes and reported no
unresolved Important or Critical issues.

### Release reproduction

The [paired probe](measure.py), [216 raw observations and 36-case summary](measurements.json),
[retained heap observations](heap-observations.json) and [validation record](validation.json)
are recorded alongside this document. Timing is an observation, not an acceptance gate. Release builds use
the same Cargo.lock and shared target; each command runs under a 30-second,
2 GiB process-tree guard with 25 ms RSS sampling. An unobserved RSS sample is
null. Small and independently scaled inputs use three alternating pairs.

The final release comparison ran with no Cargo build alongside it. All 36 cases
passed positive candidate-count/completeness controls and output comparison.
Plan JSON matched exactly. Run JSON retained candidate descriptors, selections,
termination/status, observed/retained output counts and final counts; UUIDs,
elapsed values and resource observations were normalized. Each fixture used
three alternating baseline/candidate pairs.

| Representative case | Baseline median ms | Candidate median ms |
| --- | ---: | ---: |
| qualified-8-8 | 57 | 57 |
| qualified-2000-2000 | 57 | 58 |
| private-8-8 | 57 | 57 |
| private-2000-2000 | 56 | 54 |
| related-1000-1000 | 387 | 55 |
| related-2000-2000 | 1646 | 56 |
| verify-131072-16 | 475 | 473 |
| reset-1-8 | 183 | 179 |
| reset-32-256 | 2183 | 2188 |
| output-0 | 99 | 99 |
| output-16777216 | 223 | 177 |
| output-67108864 | 475 | 315 |

Times are guard-observed whole-command elapsed values, including process
monitoring; the approximately 55–60 ms floor obscures qualified/private lookup
changes even at 2,000 × 2,000 inputs. Their operation gates establish the work
reduction. Measurements include increases, such as deep reset 2,183→2,188 ms;
there is no universal timing or RSS non-regression claim. The raw file contains
all small and independently scaled cases, not just this table.

RSS is the maximum sampled process-tree value from each run, not an allocator
measurement or a continuous native peak. Short commands can finish between
25 ms samples, so lower analysis RSS samples do not establish a smaller actual
peak. The guard remains 2 GiB per process tree. Normal preparation retained
1,188/1,820/4,214 requested bytes for 1/5/16 selections at both 4 KiB and 128 KiB
text sizes; validation's temporary peak was not separately bounded by that gate.

Run from the repository root after building the two locked release executables:

```sh
.venv/bin/python docs/maintenance/2026-10-21-issues-1039-1040-1041-1042-1044-1045/measure.py BASELINE_BINARY CANDIDATE_BINARY NEW_OUTPUT_DIRECTORY
```

The output directory must not exist. Fixture generation and complete output
checks are part of the probe; scratch projects are automatically removed. Only
compact observations and the probe are tracked, not binaries or fixture trees.
Binary hashes, platform, Python/Rust versions, Cargo.lock hash and code revision
are in the measurement file. Baseline is a892eee0; implementation is 4e2b4417.

### Final checks and limits

- `cargo test --locked --offline --workspace --all-features`: 185 target/doc-test
  result blocks, 3,867 passed, zero failures, 33 ignored. These are this change's
  macOS arm64 results, not historical or cross-platform attestations.
- Workspace/all-target/all-feature and vendored parser Clippy with `-D warnings`;
  workspace/vendor fmt; six new active performance inventory entries; strict docs
  build; source/footnote checks for four updated OKF pages and YAML/reserved-file
  structure plus entry-point reachability for all 50 pages.
- Two independent-review findings fixed and statically reconfirmed. Native
  Linux/Windows execution and a whole-validation allocator peak measurement
  remain unverified. The existing formal output corpus adapters passed as part
  of the workspace tests; Lean model sources and proofs were not changed.
- Shared target, jobs=2, no concurrent Cargo builds. The final disk check showed approximately
  150 GiB free. An accidentally generated
  603 MiB standalone hook cache was checked for distinct realpath/inode and
  removed; no shared cache was cleaned. The selected worktree and branch retain
  all six issue numbers.
