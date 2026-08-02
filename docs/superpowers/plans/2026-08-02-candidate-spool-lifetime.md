# Candidate spool lifetime plan

1. Add a failing store lifetime test that expects a finished spool to live in
   and be removed with a caller-owned temporary directory.
2. Add directory-aware `CandidateStore` construction and route the shell's
   analyzer through its existing run-scoped spool root.
3. Run focused analyzer/run tests, formatting, lint, workspace tests, and diff
   checks.
4. Request independent review, commit, push, and create a PR closing #106.
