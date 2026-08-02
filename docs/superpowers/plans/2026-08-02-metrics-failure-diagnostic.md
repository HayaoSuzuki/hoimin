# Metrics failure diagnostic plan

1. Add failing tests for failed-run metrics finalization and warning retention.
2. Preserve the latest accepted diagnostic run identifier outside the fallible
   run future, including session/resume run ID adoption.
3. Extract metrics finalization, add the failed-run diagnostic, and always emit
   queued warnings when metrics were requested.
4. Run focused metrics tests, formatting, lint, workspace tests, and diff checks.
5. Request independent review, commit, push, create the PR, monitor CI, and
   squash merge it.
