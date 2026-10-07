# Default promotion implementation plan

Base8437243; separate recent-analyzer-defaults worktree, added above PR733 in stack734.

- [x] Run adjacent core configuration baseline, update exact membership tests and observe RED (50 vs52).
- [x] Add the two defaults, update CLI/default inventories and assert exclusions/explicit overrides/saved previous50 plans through public CLI.
- [x] Recheck the existing generic Lean selection proof and add incremental recovery/sensitivity claims with explicit model-only scope under20s/2GiB.
- [x] Run full Rust tests without early exit, classify any fixture assumptions, complete quality and applicable document checks; five implementation/test reviews and independent final review.
- [x] Commit implementation/docs, create and link PR, cargo clean and remove task artifacts.

## Plan self-reviews

1. RED crosses the configuration normalization boundary before editing Default; independently enumerate the expected promoted set.
2. Public tests exercise actual candidates from both new operators, individual opt-outs and exact previous50 recovery rather than just checking an operator count.
3. Saved-plan verification uses a serialized previous50 selection, preventing a default change from altering rediscovery.
4. Full-suite failures must retain each test's purpose; pin an explicit selection only for tests about an unrelated fixed mutation set, not to hide default regressions.
5. Reuse the proved selection algebra and bound Lean resource use. Keep all evidence committed and clean generated binaries after the new PR is reviewable.

Validation is recorded in [review.md](review.md); delivery/cleanup follow the verified commit.
