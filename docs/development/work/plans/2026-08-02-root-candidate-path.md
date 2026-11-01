# Root-level mutation candidate path plan

1. Add a failing workflow regression for a root-level inventory candidate.
2. Add shared workspace-package extraction and skip invalid candidates as
   `not_run`.
3. Run focused Python tests, formatting/lint checks, and the normal repository
   test suite.
4. Use hoimin plan/verify against the changed production Python path to inspect
   relevant mutation candidates.
5. Request independent review, commit, rebase onto latest main, push, create the
   PR, monitor CI, and squash merge it.
