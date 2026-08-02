# Explicit-line ranking path normalization plan

1. Add ranking regression tests for dot-prefixed and absolute `--line` paths;
   run them to demonstrate the missing `explicit_line` reason.
2. Make the existing core logical-path normalization and equality helpers
   available to the CLI without duplicating platform rules.
3. Normalize selected line paths before ranking comparisons and rerun the
   focused tests.
4. Bump the ranking-rule version so older plans are rejected as unsupported
   instead of being misdiagnosed as tampered.
5. Run formatting, lint, workspace tests, diff checks, and independent review.
6. Commit, push the issue branch, and create a PR that closes #103.
