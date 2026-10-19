# Selection and immutable metadata integration

Frozen integration source: `f72134d76d28d87cd728305ed224f298df41af4a`.
This archive covers #1006, #1007 and #1008; the controller owns final branch
review, PR publication, automatic/native CI and task-owned scratch cleanup.

The user explicitly accepted a +0.1 ms practical small-ASCII upper-bound
tolerance under D1 after seeing R6's results. Tiny's −0.019834 ms and small's
+0.067791 ms bounds meet D1; requested whole-plan peak/retained heap is unchanged.
The [D1 addendum](reports/task-3-d1-review.md) resolves I1. Preserve the original
+0.05 ms criterion and its inconclusive small-input result in the
[R6 archive](../2026-10-20-issue-1008/r6/README.md). No universal non-regression
claim follows from this post-observation acceptance decision.

| Change | Original evidence | Main limitation |
| --- | --- | --- |
| #1006 prepared line membership | [Source/tool/measurement receipt](../2026-10-20-issue-1006/verification.json), [raw samples](../2026-10-20-issue-1006/measurement.json), [report](reports/task-1-report.md) | Sparse API improves; three-pair many-line whole-CLI median increases 9.257→11.657 ms; index adds 8 bytes per raw range. |
| #1007 shared immutable manifests | [Measurements and reproduction](../2026-10-20-issue-1007/README.md), [report](reports/task-2-report.md) | Requested metadata retention shrinks; filesystem times remain mixed, copying and logical disk charges unchanged; RSS unmeasured. |
| #1008 lazy validation positions | [Measurements and reproduction](../2026-10-20-issue-1008/README.md), [report](reports/task-3-report.md), [R6](../2026-10-20-issue-1008/r6/README.md) | Two Unicode builds remain sequential; max-live 2→1; measured context stack 160→168 bytes; D1 acceptance is finite-workload evidence. |

[Controller ledger](controller-ledger.md) preserves R1–R7 with costs and the D1
decision. [Reports](reports/) retain original task reports and independent
reviews, including the initial I1 finding and unsuccessful +0.05 ms fix round.
The [self-review index](self-review-index.md) identifies all 60 distinct passes.
The [task briefs, issue snapshots and discovery notes](records/) retain the
original instructions and read-only investigations, including their historical
pending-D1 state. The per-issue collections above retain raw measurements, source/binary/tool
hashes and scripts without duplication here.

The [integration summary](receipts/integration-summary.json) preserves each
command, timeout, exact head/diff hash, resource environment, exit and log hash.
It counts the final Rust harness summary per top-level binary/doc-test section,
excluding nested helper summaries. The full transient logs remain under
`/tmp/hoimin-perf-three-integration` until controller cleanup; concise receipts
retain all summary/ignore/warning lines and hashes. The
[protected-source receipt](receipts/protected-source.json) establishes an empty
diff for 839 product/test/Cargo/tool/workflow/proof inputs against the frozen
head. [Historical source checks](receipts/historical-source-hashes.json) verify
5/5 Task1, 12/12 Task2 and 9/9 Task3 recorded hashes at their actual task commits.
No measurement helper formatting or source provenance changed in integration.

Validation completed on the frozen source with no product changes:

| Check | Result |
| --- | --- |
| Workspace/vendor fmt and clippy | All pass, no compiler/linter warnings |
| Locked workspace tests | 3430 passed / 33 ignored; two nested helper summaries excluded |
| Core / CLI contracts | 705 passed / 2 ignored; 2228 passed / 28 ignored |
| CLI reference example / generated check | 7 passed / check exit0 |
| Frozen Ruff format/lint and ty | All pass |
| Frozen Python suite + environment retry | Original full run:1197 passed/11 skipped/4 sandbox failures; scoped unsandboxed retry:4 passed. Resolved aggregate1201 passed/11 skipped, not one all-green full run. |
| Registered performance gates | 47 passed, zero pending |

The Python guard's unchanged process-table observation raises PermissionError
in the sandbox. The controller authorized a four-case retry outside it;
[resolution](receipts/python-resolution.json), [original failed full log](receipts/python-tests-sandbox.log)
and [retry log](receipts/python-resource-guard-retry.log) preserve that distinction.
Three skips require Linux Creusot signal/session behavior; eight require missing
real actionlint/shellcheck/zizmor executables. The archive itemizes their source
guards and confirms executable absence without changing the environment.

The [current OKF audit](receipts/okf-references.json) passes structure/reachability
for all50 pages and verifies11 new source hashes. The [baseline delta](receipts/okf-reference-delta.json)
contains no new findings. It separately lists52 inherited missing-path
occurrences across12 target paths,67 unavailable local commit references across
24 commits,14 footnote findings and current catalog omissions for31 historical
designs/104 reports. This change adds the combined spec's catalog entry.
External URL/fragment checks and historical content-hash refresh are excluded. Earlier task counts are historical and overlap;
they are not added to final integration counts. Native Windows/Linux results
are not established by these macOS checks. URL and fragment checks are outside
the local reference audit.
