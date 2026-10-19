# Task 3 D1 acceptance addendum

**I1 — ADDRESSED under the explicit D1 user acceptance contract.** The controller reports the user's answer to the pending D1 question: 「+0.1 ms以内を許容して採用する（推奨）」. That question disclosed the paired median of approximately +0.021 ms, one-sided upper bound of approximately +0.068 ms, unchanged whole-CLI requested heap, and failure of the original +0.05 ms criterion. The user authorizes adopting the current implementation with a +0.1 ms practical tolerance.

Applying that explicit decision to the already verified evidence at `f72134d76d28d87cd728305ed224f298df41af4a`:

- The small fixture's one-sided 95% paired-median upper confidence bound is **+0.067791 ms**, within **+0.1 ms**; its paired median is +0.021354 ms.
- The tiny fixture's corresponding upper bound is **−0.019834 ms**, also within +0.1 ms.
- All three matched whole-plan requested peak and retained heap differences per fixture remain **0 B**, satisfying the unchanged no-increase heap condition. Context stack growth **160→168 B** remains disclosed separately.

Evidence: `docs/maintenance/2026-10-20-issue-1008/r6/README.md:9`, `:14`, `:21`; `r6/summary.json:1`; verified arithmetic/provenance in `task-3-fix-1-review.md`.

The original review and fix-round verdicts remain accurate under their original +0.05 ms contract and must be preserved. D1 is an explicit **post-evidence acceptance decision**, not a preregistered +0.1 ms experiment and not a retroactive success of the original R6 criterion. The evidence supports this finite-workload decision subject to its recorded independent-representative-pair assumption and per-fixture confidence; it does not guarantee universal timing equality, stack equality, or absence of regressions on other workloads.

**Remaining I1 gaps: none under D1.** The controller should archive the exact user decision alongside this addendum so the changed acceptance contract remains auditable. The task-local acceptance concern is closed; this addendum does not replace Task4's separate integration gate. No product/evidence changes, new measurements, tests, builds, Git commands, broad review, or subagents were used. This ignored addendum is the only written artifact.
