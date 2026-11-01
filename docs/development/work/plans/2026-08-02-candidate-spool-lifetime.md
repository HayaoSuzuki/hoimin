# Candidate spool lifetime plan

1. Add a failing store lifetime test that expects a finished spool to live in
   and be removed with a caller-owned temporary directory.
2. Add directory-aware `CandidateStore` construction and route the shell's
   analyzer through its existing run-scoped spool root. Retain shared ownership
   in detached blocking analysis during prompt cancellation.
3. Extend cancellation coverage to prove eventual cleanup after the detached
   task exits.
4. Run focused analyzer/run tests, formatting, lint, workspace tests, and diff
   checks.
5. Request independent review, commit, push, and create a PR closing #106.
