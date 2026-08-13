# Result-lifecycle deterministic stop fixture design

## Context and diagnosis

PR #304 failed only in GitHub Actions job `Rust randomized order
(nightly-2026-07-27)`, at existing case
`lean_result_lifecycle_oracle::stop_preserves_accepted`. The report-sequence
oracle added by the PR passed. The failing observation exited with code 4 but
had no executed, durable, or reported mutants; the expected observation has
`m0` killed and durable before `m1` becomes `not_run`.

The fixture currently requests `--total-timeout 1s`. Under the shuffled CI
load that deadline expired before the first mutant completed. The exact shuffle
seed passes locally, so test order alone is not causal; elapsed scheduling and
process startup time determine whether the fixture establishes its required
premise.

## Decision

On Unix, replace the wall-clock semantic trigger for
`stop_preserves_accepted` with a condition-based trigger:

1. spawn the real CLI in its own process group without `--total-timeout`;
2. poll the fixture session database until exactly one killed result is
   durably present, while also detecting premature CLI exit;
3. send SIGINT to the CLI root process using the repository's existing Unix
   signal pattern;
4. wait for bounded graceful completion, drain stdout/stderr, and feed the
   unchanged public report/session/metrics observations to the Lean oracle;
5. on every failure path, terminate and reap the process tree and include
   readiness, cleanup, stdout, and stderr details in the infrastructure error.

The overall wait remains bounded; only the semantic trigger changes from
elapsed time to the condition owned by the case. Windows retains the current
total-timeout path because this repository has no corresponding safe console
interrupt fixture seam, and its CI job is green.

## Alternatives rejected

- Increasing the total timeout is smaller but preserves the race at a
  different load threshold.
- Serializing the shuffle workflow would slow unrelated tests and conceal the
  fixture defect.
- Waiting only for the execution marker is insufficient: it proves the test
  command ran, not that the accepted result was durably committed, which is
  the premise of `stop_preserves_accepted`.

## Code boundary

Change only
`crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs`. Production Rust,
Lean models/corpora, workflow configuration, and the new report-sequence
adapter remain unchanged.

The implementation may extract command construction and child-output
collection helpers only where needed to reuse the existing cleanup behavior.
It must not alter the expected observation or weaken mismatch classification.

## Regression and verification

RED is the existing strict oracle case under an intentional Unix-only delay
before the first mutated test completes; the current one-second trigger must
reliably produce the same empty observation seen in CI. GREEN replaces that
trigger with durable-result readiness, after which the unchanged strict case
must pass despite the delay.

Verification covers:

- the focused case with the recorded shuffle seed;
- the complete `lean_result_lifecycle_oracle` binary under that seed;
- the new `lean_report_sequence_oracle` suite;
- formatting and all-target/all-feature clippy;
- the full workspace test suite with the pinned nightly shuffle command.

The fixture continues to classify spawn, database-readiness, signal, timeout,
drain, and cleanup failures as infrastructure errors rather than semantic
mismatches.
