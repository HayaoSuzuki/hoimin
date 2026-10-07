# Issue 709 investigation plan

1. Read the original paper's methods, tables and threats, and the small replication README; distinguish per-test and command-level observations.
2. Build the unchanged CLI in the issue worktree; execute fixed controlled and installed-package probes via saved plans, saving exact statuses and timing.
3. Prove suite-result masking and finite-prefix insufficiency in Lean; keep model-only evidence separate from the CLI experiments.
4. Compare benefit, cost and implementation surface; record an explicit go/defer decision with reopening criteria and independent review.
5. Commit the report/probe/model/catalog update, open a research PR, clean cargo and task scratch files before issue 710.

## Plan self-reviews

1. Evidence: primary paper plus artifact README, no secondary summary used as proof.
2. Controls: passing, failing, alternating, masked and baseline-failure cases distinguish observation mechanisms.
3. Reproduction: keep the bounded probe with results, hashes and installed versions; reruns may change timings but controlled assertions must hold.
4. Proof boundaries: no statistical confidence or Rust correctness inferred from abstract Boolean theorems.
5. Delivery: do not label this as implementing repetitions or auto-close issue 709; explain the decision in the PR and catalog.
