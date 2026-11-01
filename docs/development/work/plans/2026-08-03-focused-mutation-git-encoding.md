# Focused Mutation Git Encoding Plan

1. Add failing tests that inspect the probe and startup `rev-parse` subprocess decoding policy.
2. Pin both call sites to UTF-8 with surrogate escaping.
3. Run focused Python tests, lint/type checks, and full repository tests.
4. Run focused hoimin mutation testing for the changed decoding arguments.
5. Request independent review, create the issue PR, wait for all CI, squash merge, and remove the worktree.
