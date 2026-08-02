# Clap success output routing plan

1. Add failing `run_with_io` tests for root help and version stdout, plus a
   guard that invalid arguments remain on stderr.
2. Route zero-exit Clap displays to stdout and non-zero diagnostics to stderr.
3. Run focused CLI tests, formatting, lint, workspace tests, and diff checks.
4. Request independent review, commit, push, create the PR, monitor CI, and
   squash merge it.
