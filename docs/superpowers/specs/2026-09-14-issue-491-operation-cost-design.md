# Issue #491: Operation costs and missing input dimensions

The existing performance-shape infrastructure already contains 21 active gates. This extension adds independent Lean-generated cost expectations and missing bounded release fixtures; historical results remain historical.

## Cost contract

Measure a named vector, not total runtime complexity. OrderedBindingHistory build_updates counts each activated segment-tree leaf and ancestor assignment: B × (ceil(log2 B) + 1), with zero at B=0. query_comparisons counts binary-search offset comparisons, bounded by R × (floor(log2 U) + 1) for U distinct offsets (zero when U=0). Sorting comparisons, allocation bytes, tree composition internals and native stack are outside those counters. The semantic reference folds eligible events in insertion order, including duplicate and nonmonotone activation offsets.

KnownImports actual Clone instrumentation counts calls and entries copied. The adapter isolates the annotation-record callback interval so legitimate control-flow snapshots outside it do not become false positives. Replacement instrumentation records successful String builds and bytes at the actual list/tuple builder boundary. Unselected operators must cause no replacement builds even with max-candidates=1; selected flat literals provide positive semantic and byte-count controls. Actual broken code paths perform full scans, full-state clones and eager replacement builds and fail the same observed predicates.

Lean owns every case and expected bound. Cases cover 0/1, 7/8/9 tree boundaries and N/2N/4N=8/16/32, with small finite semantic events and query offsets. The adapter parses a strict schema, checks Nat-to-usize and intermediate power/multiplication representability before execution, observes real counters, and does not reimplement the optimized cost formula. BoundedCandidateDiscovery proofs remain the foundation for candidate retention; they establish no whole-CLI constant-memory claim.

## Measurement contract

Extend the existing Python fixture runner and shape registry for target-file and selector counts, fingerprint files/bytes/exact+glob, paired Unicode layout, AST width/depth and large unselected literals, active imports, multifile/partial verify, record length and workspace bytes/workers. Preserve independent expected semantic results and finite N/2N/4N sizes. Release baseline/candidate comparisons use isolated monitored subprocesses and repeated observations with environment/binary digests. Time and sampled RSS remain observations without deterministic performance thresholds.

A separate allocator test observes workspace preflight peak and post-preflight retained bytes. These are distinct from post-worker retained bytes and sampled whole-process RSS. Expected input-proportional metadata and the existing largest-single-file fingerprint buffer are allowed; retaining all file contents multiplied by workers exceeds that model. Timeout/stack fixtures remain behind process deadlines and finite size caps.

## Review decisions

1. Reconciled the merged design/report with current code: 21 gates are already active, but extra Lean costs and several independently varying workload dimensions remain absent. Keep those gates and add fresh evidence.
2. Traced counters to exact production steps. Existing annotation snapshot and collection caller counters could miss work moved to another helper; measure actual Clone/build boundaries, and distinguish callback clones from necessary flow-state copies.
3. Checked numeric and semantic scope. The model's cost vector excludes sort/allocation overhead; generated native-size preconditions and event-order semantic checks prevent treating an arithmetic formula or a fabricated counter as production evidence.
