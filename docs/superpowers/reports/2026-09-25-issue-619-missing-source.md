# Issue 619 verification and reviews

Design and implementation plan were committed as `1bd3324` before production edits. Each records three self-review passes.

## Implementation self-reviews

1. Ordering and public paths: shared TargetHandler entry validates sources before filesystem discovery and Git intersection. Run reports target.resolve; plan/verify preserve their contextual error wrappers. All report original quoted source. No baseline can run after this error.
2. Compatibility: existing lexical normalizer is reused; relative, absolute/root-equal, lexical `absent/../src`, regular files and empty directories have direct controls. No schema or selection union/intersection changes. Link metadata does not enable traversal; dangling links fail. Validation is diagnostic and does not claim to prevent later filesystem races.
3. Error handling and scope: metadata distinguishes NotFound from other OS failures rather than masking errors with exists(). Original input is debug-quoted to escape control characters. Missing-path precedence intentionally precedes invalid Git repository diagnosis. Existing Git-error fixtures now create their declared source so they continue to test Git behavior, rather than failing on an unrelated typo.

## Test self-reviews

1. Public regressions: both run and plan exercise missing-only, mixed valid/missing, and changed-clean missing sources. Exit 2, meaningful path diagnostic, and external baseline marker absence are independently asserted. Saved-plan regression deletes an empty declared source while retaining candidate-bearing src, so old source-record validation cannot accidentally satisfy the assertion.
2. Positive discrimination: existing-empty, excluded and changed-clean selections still succeed. Run must execute baseline and produce zero mutants; plan must produce zero candidates. Filesystem-backed TargetHandler tests preserve normalized/root-equal/regular-file source behavior, escape diagnostics, and Unix dangling/valid-link policy.
3. Isolation and portability: subprocesses have unique child-only temp roots and a 30-second kill-on-drop deadline. Python uses repository's controlled interpreter on both Windows and Unix. No global environment mutation. Native Windows tests await CI; Unix links are cfg-gated. Permission-denied errors are preserved in code but not forced with root-sensitive chmod tests.

## Evidence

- Initial public RED: missing-source and saved-plan tests failed against old implementation (exit 0 and exit 1, baseline ran); legitimate-empty control passed.
- First related suite exposed a missing worktree `.venv` symlink; fixed environment by linking the existing shared interpreter, without dependency changes.
- Next related suite: plan 87 passed/1 ignored; target 45 passed/2 failed because Git-error fixtures declared absent pkg. Corrected those fixtures as explained above.
- Final focused run: missing_source 3 passed; plan 87 passed/1 ignored; target_handler 47 passed.
- Both exact CI Clippy commands passed. Full workspace: 2393 passed, zero failed, 22 ignored across 110 groups.

Lean not added: this changes an OS existence observation and error ordering, not a pure transition policy. Filesystem and public-process tests directly exercise the claim.
