# Type mapping operator serialization plan

1. Add failing plan-config tests for canonical serialization and historical
   manifest deserialization.
2. Add an explicit Serde rename and narrow legacy alias to `TypeMapping`.
3. Run focused core/plan tests, formatting, lint, workspace tests, and diff
   checks.
4. Request independent review, commit, rebase onto latest main, push, create the
   PR, monitor CI, and squash merge it.
