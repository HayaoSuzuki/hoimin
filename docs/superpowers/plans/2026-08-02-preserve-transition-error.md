# Preserve transition error during shutdown plan

1. Add failing unit tests for combining a transition failure with the optional
   shutdown drain failure.
2. Add the shared formatter and use it in all primary-error drain paths.
3. Change the transition-error branch to inspect the drain result instead of
   propagating it with `?`.
4. Run focused shell tests, formatting, lint, workspace tests, and diff checks.
5. Request independent review, commit, rebase onto the latest `origin/main`,
   push, create the PR, monitor CI, and squash merge it.
