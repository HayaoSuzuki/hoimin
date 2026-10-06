# Enum implementation plan

Parent 10a0021; #699 cargo clean removed 4.0 GiB before this worktree was created.

- Add public CLI RED cases for three members, aliases/auto, shadowing and diagnostics.
- Implement cached enum definitions and conservative identity checks; register opt-in
  ID, ranking, diagnostics and inventories without altering default IDs.
- Check exact candidate spans, CPython member identities, saved-plan weak/strong probes,
  limits/selectors/cancellation, Lean, full workspace, clippy and package trials.
- Record five implementation/test reviews, independent review, commit docs and code,
  create/link PR, then remove scratch and cargo clean before next issue.

## Plan self-review

1. Require expected member pairs independent of the production collection algorithm.
2. Exercise standard module/import aliases plus reassignment and local shadowing.
3. Validate aliases by executing fixture-only Python enum definitions, never user code
   from inside the analyzer.
4. Show type-only assertions survive and identity assertions kill all two alternatives.
5. Include unsupported-body diagnostics and defaults/limit checks, not only happy paths.
