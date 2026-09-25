# Issue 608 review and verification record

Design and implementation plan were committed as `d987ef0` before code/test changes. Both documents record three self-review passes. Work follows the user's authorization through publication; CI polling and merge belong to the coordinating agent.

## Implementation self-review

1. Compared each extracted scoring branch with its predecessor: explicit-line normalization, resolved-symbol union and ancestor matching, changed-line reason, operator category and reason order remain identical. Creation still owns candidates, uses the original comparator and assigns one-based ranks after stable sorting. The reason score sum remains bounded by the same fixed categories.
2. Checked the replacement validator's fixed-point argument. Correct per-row reasons/score and position rank plus adjacent order are equivalent to unchanged stable re-ranking under this comparator. Previous rows have already been checked before comparison. Equal comparator keys ignore body/sequence and keep their input order, as before. Empty input and the private function's unknown-operator behavior remain unchanged; structural validation is still separate.
3. Traced shared verify preparation and checked ownership. Every retained candidate is visited before preview/test execution, regardless of selected count. Only selectors and a small current reason vector allocate; no original/replacement, ID, hash or candidate clone remains in revalidation. Generic error text and schema/ranking versions stay unchanged. During documentation review, corrected “before selecting” to “before running or previewing” because ID selection already occurs earlier. Clippy identified only fixture `format!` collection; changed setup to `writeln!` without a lint exemption.

## Test self-review

1. Reviewed allocator isolation and sensitivity. One dedicated test binary compiles the actual private production ranking module, creates 64 valid candidates through the public plan API, and performs conversion/target resolution before measuring. Three repeats at 32 B, 32 KiB and 256 KiB per literal use the same candidate shape/count. The largest eager-clone control must exceed 32 MiB. Peak measurement catches cloning the full set or one large candidate at a time; claims are limited to this ranking call, not process RSS or elapsed time.
2. Reviewed semantic oracles. Added comparison against the previous clone/re-rank equality algorithm for rank, score, reason scores, duplicate/coherently wrong reasons, all ordering dimensions and empty input. Adjacent swaps are renumbered so rejection cannot rely merely on stale ranks. Equal-key candidates have distinct bodies/sequence and are accepted in either stable input order. Existing literal reason/category/selector assertions independently protect shared scoring logic. Replaced a pointer-identity fixture choice with explicit context/target tuples for clarity.
3. Reviewed the public boundary. Real CLI dispatch creates an unedited 64-candidate plan; subprocess dry-runs selecting 1 and 7 require the saved IDs/ranks/bodies. The external marker remains absent. The final unselected candidate is then changed to a structurally coherent lower arithmetic score/reason; top-1 dry-run must reject it with the exact semantic-ranking diagnostic, status 2 and empty stdout. These behavior tests passed before the optimization as preservation controls.

## Verification evidence

- Allocator RED: `cargo test -p hoimin-cli --test ranking_heap -- --test-threads=1 --nocapture`: exit 101. Extra peak bytes were 47,744 / 4,237,952 / 33,598,080 for 32 B / 32 KiB / 256 KiB payloads, identical across three repeats. Failure was the 64-KiB bound, not invalid candidate setup.
- Pre-change semantic baseline: `cargo test -p hoimin-cli --lib ranking_semantic_validation -- --test-threads=1`: 3 passed, exit 0.
- Pre-change public baseline: `cargo test -p hoimin-cli --test plan verify_ranking_large_bodies -- --test-threads=1`: 1 passed, exit 0.
- Allocator GREEN: same allocator command: 1 passed, exit 0; all nine production measurements were 32 bytes, and the eager-clone sensitivity assertion passed.
- `cargo test -p hoimin-cli --lib ranking_ -- --test-threads=1`: 13 passed, exit 0.
- Public GREEN: same public command: 1 passed, exit 0.
- `cargo test --workspace -- --test-threads=1` on base `bf09c91`: exit 0; 2321 passed, 0 failed, 22 ignored across 100 test/doc-test binaries, including the final fixture construction.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`: exit 0.
- `cargo fmt --all -- --check`, `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check` and `git diff --check`: exit 0.

The assigned `target/batch-verify` cache uses one build job, disabled dev/test debug information and disabled incremental builds. Only core/CLI artifacts were cleaned on switching into this worktree. No Python source/test change or Lean run was needed.

## Independent review

The coordinating agent reviewed the production diff and new tests, including the untracked allocator binary, and reported no blockers. Review confirmed shared reason generation, all-row metadata checks before ordering, stable equal-key behavior, preserved private unknown-operator behavior, all comparator dimensions, the valid public-plan allocator fixture and clone sensitivity control, and rejection of an unselected tampered row. No production source changes followed that review; only test fixture string construction was adjusted for clippy.

## Rebase verification

After the full suite, implementation commit `b41c4f4` and its precommitted design were rebased onto main `35bd5cf`, yielding implementation `c9b7488` and design `6143f4b`. Rebase had no conflicts and both range-diff entries are unchanged (`=`).

- Fresh ranking unit tests: 13 passed, exit 0.
- Fresh `cargo test -p hoimin-cli --test ranking_heap --test plan -- --test-threads=1`: 85 passed, 0 failed, 1 ignored across two binaries, exit 0. This includes all existing plan validation/tampering/preview tests and the allocator regression.
- Both exact CI clippy commands, both fmt checks and `git diff --check` passed on the final base.

The complete workspace result above belongs to the pre-rebase implementation; final-base checks cover ranking, memory and public saved-plan behavior. No source changes followed these checks.

## Published-branch merge verification

Merged main `d2277bf` into the published branch without rewriting history. The only conflict was an additive test insertion in `tests/plan.rs`; retained both the ranking/public-preview test and the stale-plan helper/tests. Production ranking merged automatically, retaining the changed-context documentation and unchanged borrowed validation.

After cleaning only core/CLI artifacts in the assigned cache, ranking unit tests passed 13/13 and the plan/heap group passed 88 tests with 1 ignored. Both exact CI clippy commands, both fmt checks and diff checks passed. The merge paused at the user's request before conflict resolution, then resumed with explicit authorization; no checks were running during the pause.
