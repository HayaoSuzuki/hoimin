# Issue 616 implementation plan

1. Add public baseline and WorkspacePlan create/reset regressions for nested empty directories, no files, ignored/explicitly included/excluded paths, and type replacements. Observe failure before production edits.
2. Extend manifest inventory with sorted selected directories plus ancestors. Reuse existing walking policy and require positive include matches for directories in the include pass. Preserve file selection/accounting tests.
3. Materialize directories in snapshot/workers and implement retained-root directory restoration. Compare directory sets in integrity/reset checks and exercise symlink/no-follow controls.
4. Integrate a finite Lean directory-selection/reset model and executable public adapter, with 10k-heartbeat proofs and 20s/2GiB guard; acquire the shared Lean slot before execution. Register CI/corpus consistently.
5. Run focused workspace/public regressions, meaningful sensitivity controls, whole workspace, exact CI Clippy/fmt and workflow tests. Record three separate implementation/test self-reviews and independent review. Commit, submit independent PR through gh stack, wait for CI, merge and remove worktree.

## Plan self-review

1. RED will use public APIs and actual CLI baseline observations, not a newly missing directory getter, so it exposes the user-visible omission. Start with portable file/directory cases; platform link cases supplement them.
2. Test ignore policy separately from reset semantics, including traversal-only include parents and original-directory deletion. Cover zero-file manifests and logical-byte invariance to prevent counting directories as fake files.
3. Avoid duplicated large build outputs by sharing the root Cargo lane after624 finishes. This branch is independent from resume work and will not form a giant stack. Lean limits and correspondence must remain explicit; published branch integration uses merge, not force push.
