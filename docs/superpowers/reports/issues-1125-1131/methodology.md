# Fixed bounded protocol

Evaluate independently written ASCII examples, two per issue: one API requirement
that can distinguish the proposed change, and one explicit-domain/no-requirement
or interface control. No book text or code is reproduced. The research adapter
reuses the previous slice/division experiment's subprocess, plan and edit helpers;
it is a fixed-corpus research tool, not a product API or project analyzer.

For each example, first require the original to pass both weak and stronger
checks. Obtain candidates from the base binary's public `plan` command for
(1) the nearest existing family and (2) the full existing default set. Obtain a
third pool with the experimental Python AST prototype. This prototype matches
only the fixed corpus and does not implement product import/scope eligibility.
It never imports the target to find candidates. Test execution deliberately
imports the selected original/mutant in a fresh process.

For each pool execute at most its first two candidates, using the same weak and
strong checks, with a fresh interpreter each time. Record all descriptors,
pool size, executed and unexecuted counts. Empty pools consume no mutant budget.
Existing candidates use public plan order; prototype candidates use AST walk
order. This is a deterministic prefix comparison, not exhaustive/default-ranking
performance or an unbiased operator survey. Static overlap analysis supplements
these bounded observations; do not generalize from unexecuted candidates.

Classify passed, assertion_failed and runtime error separately. A useful lead
requires weak survival and an assertion failure under an independently stated
requirement. Passing both checks is only observational equivalence for that
fixture/domain, never proof of universal equivalence. A runtime exception is
not credited as a useful missing-assertion lead. Different mutants exposing one
requirement are not independent test defects.

The adapter records generation wall time (public CLI planning versus a much
smaller AST prototype; these are not comparable production benchmarks), each
check's wall time, total time, Python/platform, base Git revision and binary
SHA-256. No peak-memory or realistic project throughput claim is made.

Each Python process has a five-second timeout; plan has a 45-second outer limit.
Recorded plan limits are one job, 1,000 candidates, 100 mutants, 10-second test
limits, 60-second total runtime, an 8-GiB workspace limit, and a 10-GiB free-disk
reserve. Direct probes operate on the fixed tiny sources rather than under the
product resource manager; do not infer hard process-memory containment from
them. Temporary directories are removed and cleanup is recorded. Product
behavior is verified separately through actual `plan`/`verify` integration tests.

Run with a binary built from the recorded base revision to reproduce the old
full-default pool. Building this branch changes that pool because three IDs are
now defaults. Recorded base observations are not overwritten with product runs.
