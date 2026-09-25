# Issue 623 implementation plan

Dedicated worktree, shared root build cache, no worktree-local target. Commit this design/plan before implementation.

1. Add a public resolver allocation regression for 1/16/64-MiB binary inputs in exact/glob/overlap modes; measure outside fixture setup and assert complete records equal independent in-memory digest controls. Observe size-proportional RED.
2. Factor WorkerRoot read consumption while preserving platform checks. Add root-relative digest helper with identical missing classification. Change only auxiliary fingerprint reads to consume the digest.
3. Exercise binary boundaries, existing exact/glob error ordering and duplicate behavior. Add a hash parent-replacement test and non-file controls; existing byte-read race tests must continue passing.
4. Run focused fingerprint/heap/workspace/plan tests, workspace suite, exact CI Clippy and format checks. Obtain independent review. Record three implementation/test review passes and measured allocation, commit, publish with gh-stack. Root monitors CI in batches and merges/removes worktree after success.

## Plan review passes

1. Regression uses an existing public resolve API and measures all allocations during the call, so it catches transient whole-file buffers. Fixture writing and expected digest computation stay outside the measurement window.
2. Preserve failure behavior by running the existing ordering/unsupported/missing suite and capability races, not only successful hash equality. Hash-specific parent replacement catches accidental path reopening.
3. The new helper is internal; core fingerprint schema and caller output need no migration. Use the locked library implementation for interrupted reads, add empty/chunk-tail binary cases, and document that speed is not an acceptance claim.
