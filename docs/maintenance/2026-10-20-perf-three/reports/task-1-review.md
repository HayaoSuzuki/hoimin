### Spec Compliance

- ✅ Spec compliant for Task 1 at `4c906846e4515735d7e89c17a2b374483ba299ff`; no missing, extra, or misunderstood product changes found in `ca9afb90852c43d1422e543470940991ffaae7a2..4c906846e4515735d7e89c17a2b374483ba299ff`.
- `crates/hoimin-python/src/analyzer/rust/selection.rs:8` defines a private owner borrowing path/symbol metadata and owning one interval Vec. `selection.rs:28` preserves empty-as-All and nonempty-all-invalid-as-Restricted(empty); `selection.rs:46` sorts valid intervals and `selection.rs:54` prepares prefix maxima without successor arithmetic. `selection.rs:84` uses predecessor membership, preserving inclusive arbitrary-u32 semantics.
- `crates/hoimin-python/src/analyzer/rust.rs:454` constructs the owner after successful parsing/facts and the existing cancellation check. The changed make_candidate, collector, AST wrapper, and annotation signatures thread its reference through the producer paths; `rust.rs:3109` retains the unrestricted expression fast path. The old repeated linear predicate is removed, and the source-position LineIndex remains separate.
- `crates/hoimin-python/src/analyzer/rust/selection_tests.rs:1` adds actual query/membership/preparation instrumentation, four independently named sparse workloads, a one-range control, named raw-range/symbol/producer cases, bounded arbitrary-u32 oracle comparison, preparation-cancellation cases, and exact retained-prefix equivalence. `crates/hoimin-cli/tests/analyzer_line_selection.rs:1` covers public multiple --line, complete candidate-object equality with truncation, and separated --changed candidates. `docs/performance/shapes.json:1139` registers the four deterministic operation gates without wall-time assertions.
- `docs/maintenance/2026-10-20-issue-1006/probe.rs:39` performs API warmup before timing, with request/fixture construction outside timing; `measure.py:17` records three paired samples with alternating execution order. Raw API/CLI observations and distinct heap/RSS/output/disk definitions are retained in `measurement.json:1`; `verification.json:18` records tool/source/rlib provenance and focused-test results. The implementation report records five distinct reviews in each of the three required stages.
- ⚠️ Cannot independently establish from the final diff: removed RED instrumentation and its execution, previously existing exact payload-boundary contracts, raw test/hook execution, and frozen-baseline artifact provenance. The report supplies commands/results and `/tmp/hoimin-perf-three-task1` raw receipts; the controller should retain these for integration. No changed payload-accounting or CandidatePrefix logic appears in this task's diff. This is a scope/evidence limit, not an identified defect.

### Strengths

- `crates/hoimin-python/src/analyzer/rust/selection.rs:87` documents why the predecessor's prefix maximum answers nested/overlapping membership. Filtering only reversed intervals keeps the raw public request semantics while isolating preprocessing from callers.
- `crates/hoimin-python/src/analyzer/rust/selection.rs:25` checks cancellation before preparation; copying, post-sort, and prefix phases also check it. Sorting retains a consistent comparator, matching the explicitly cooperative cancellation design.
- `crates/hoimin-python/src/analyzer/rust/selection_tests.rs:1` counts work in the production operations under cfg(test), ties bounds to the actual expected positive query count, and bounds preparation as well as membership. The independent linear oracle and hand-derived boundary cases avoid using the index to grade itself.
- `crates/hoimin-cli/tests/analyzer_line_selection.rs:1` exercises the public interfaces with bounded children and compares complete candidate objects, preserving IDs/order/text/ranks rather than only counts. The changed-hunk fixture also expects specific candidate lines and originals.
- `docs/maintenance/2026-10-20-issue-1006/verification.json:88` explicitly discloses the whole-CLI many-line median increase (9.257 to 11.657 ms in three pairs). The API sparse N=8000 improvement (26.038 to 4.062 ms) and extra 64,000 requested peak bytes are scoped observations rather than a universal speedup claim. This measured tradeoff should remain visible in final integration reporting.

### Issues

#### Critical (Must Fix)

None found.

#### Important (Should Fix)

None found.

#### Minor (Nice to Have)

None identified that warrants a change in this task.

### Review Checks and Limits

- Read the task brief, line-design discovery, issue snapshot, implementer report, and provided review package. Reviewed the package in bounded chunks; recovered sections hidden by tool-output truncation. Did not regenerate the diff, read changed files separately, inspect unchanged product code, or mutate product/Git state.
- No concrete unresolved correctness doubt required an unchanged-code read or a focused execution. Did not rerun reported passing tests or benchmarks. Reported final GREEN evidence is selection 59, payload 27, cancellation 30, public CLI 3, and library 457 passed / 3 pre-existing ignored; normal final hooks are reported all Passed. Earlier RED-only unused-helper noise and initial hook lint failures are disclosed as resolved in the report, with no final warning reported.
- This gate covers Task 1 only; Task 2/3 implementation and final branch integration remain outside this review.

### Assessment

**Task quality:** Approved.

**Reasoning:** The private prefix-max representation is correct for the requested raw inclusive ranges, constructed once per successfully parsed file, and consistently reused across admission paths. Focused semantic/operation gates and matched, candidly scoped release evidence support this task; no blocking correctness or maintainability defect was found.
