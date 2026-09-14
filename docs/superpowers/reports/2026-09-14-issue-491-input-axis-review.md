# Issue491 input-axis expansion review

## Design and plan reviews

Each has three concrete passes in `../plans/2026-09-14-issue-491-input-axis-expansion.md`. The existing11 shapes omitted independent axes;18 added controls cover target files, selector declarations, fingerprint files/bytes/exact+glob, Unicode layout, AST width/left depth/large unselected literal, active annotations, multifile/partial verify, record length and workspace bytes/workers. Original21 deterministic gates remain intact.

## Implementation reviews

1. Inspected fixture output and selector semantics. Initial real CLI smoke exposed missing --source for symbol selectors; added explicit source root and regression. Repeated selectors intentionally keep source/function/line cardinality fixed, measuring declaration normalization rather than implying distinct target growth.
2. Reviewed partial-plan behavior across prepare and execute. Only fixtures explicitly declaring truncation accept plan exit4 and incomplete execution; complete fixtures still reject incomplete output. Available plan count remains separate from top1 selection count. All child processes retain existing external guard/deadlines and unchanged8GiB workspace/10GiB reserve.
3. Reviewed metric meaning and independent scaling: actual Python source bytes include multifile sources, output document bytes are observed separately, RSS remains sampled process-tree RSS and cannot be renamed allocator peak. Added within-binary N/2N/4N growth ratios rather than only same-size baseline/candidate ratios. Null/unobserved RSS propagates to ratios.

## Test reviews

1. New fixture/partial-completeness tests failed before implementation (missing shapes and rejected incomplete fixture). After implementation, all18 added size2 shapes passed real CLI semantic checks against fresh release baseline. Compiled fixture bytes, actual file sizes, selector argv and paired Unicode byte totals are independently asserted.
2. Growth summary regression failed before implementation. Final focused Python suite16passes; it rejects missing N/2N/4N medians and distinguishes each binary's ratios and null RSS. Original empty/ignored/failed gate, missing-baseline manifest, resource error and artifact-preservation tests remain present.
3. Mutation tested actual production completeness predicate with operator identity: `is not`→`is`, ID `m1_415a87220dfadaad2d19bd4458b4fec84c37f7010c820432c01ef1985f2b9b11`, killed, verify exit0, execution cleanup clean. Source tools, explicit tools/performance_shapes.py, line183 at selection time; default profile/operators, jobs1/workspace8GiB/free10GiB, fingerprint registry and normal test module. An initial broad-source plan encountered an existing intentionally invalid vendor fixture; regenerated with explicit file/source selection. Temporary plan/report directory was trap-cleaned. Full Python suite first ran during Lean CI edits and under sandbox:2 in-flight registration failures plus4 process-monitor permission failures are not claimed passing; final owner runs the completed suite with required process observation permission.

## OKF reviews

1. Input dimension coverage is described as explicit bounded controls, not a Cartesian-product or universal asymptotic guarantee.
2. Source-index owners must include this report plus both design/plan and Rust/Lean evidence, preserving historical audits separately.
3. Fresh measurements must record actual binary hashes, environment, repeats, sample interval, semantics and growth; historical198executions are not counted as this change's validation.

## PR reviews

1. Scope remains performance validation infrastructure and test-only counters; no individual algorithm optimization is attributed to this PR.
2. Separate deterministic cost/allocator gates from measurement-only RSS/time observations; no speedup claim from noisy timings or test-instrumentation changes.
3. Parent reviewed integration with valid symbol validation (#476), partial-plan exit policy and existing registry. Publication owner verifies final tests, source hashes and remote head after all contributors finish.

## Fresh release measurement

Baseline commit8b33167, Rust1.98.0, dedicated release target; binary SHA256 `6ea07c0003b2334d875f96ed77110902db65bc4ba8d1426d481a61af30aefd51`. Candidate SHA256 `6c81c75781a55db2f285cd2efd3ca7e35147dce825bde116acee14ac6aa23df6`. All522executions passed (29shapes×3sizes×3repeats×2binaries),174medians,87cross-binary comparisons and116within-binary growth comparisons. The committed `docs/performance/2026-09-14-issue-491-expanded-measurement.json` records environment, hashes, allmedians/ratios and raw-result hash/path. Raw artifact remains `/private/tmp/issue-491-expanded-measurements/`. No speedup is claimed; local concurrent builds make time/RSS observations environment-specific.

Final shared gate run:all26active gates executed at least one test and passed (`/private/tmp/issue-491-final-gates/result.json`). Parent independently reviewed the Lean binary-search/tree-update model, actual Rust counters and broken scan/clone/eager-builder paths, representation checks, and preflight allocator peak versus eager-copy sensitivity; no blocking finding.
