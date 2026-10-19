# CI policy correction report

Base: `2712291daf85d91ddbff2b166b23ba16fbab6924` (clean at start). The dependency-audit workflow rejects the four shipping `hoimin-*` crates at package version 0.14.0 because their own-crate Elastic-2.0 exceptions still end before 0.14.0. Updated only those four bounds in `deny.toml` to `>=0.14.0, <0.15.0`. Names, `Elastic-2.0` scopes, fuzz exception, third-party exceptions, global license rules, and source rules are preserved.

## Requirements and design review

1. Compared the request to the observed CI error: it identifies the four shipping crates and no third-party package.
2. Confirmed the approved package floor is 0.14.0 and the bounded policy remains the 0.14 minor series.
3. Confirmed the workflow pins cargo-deny 0.20.2 and runs only `licenses sources` with warnings denied.
4. Confirmed `deny.toml` distinguishes own-crate policy from third-party exceptions; no global Elastic-2.0 allowance is needed.
5. Confirmed the fuzz package has its own exact `hoimin-fuzz` exception and must retain it unchanged.

## Implementation review

1. The diff has exactly four changed lines, one version bound for each shipping crate.
2. All four crate names remain unchanged.
3. Every changed exception still allows only `Elastic-2.0`.
4. Parsed TOML comparison after normalizing only those four version values is identical to the base policy; no third-party or global scope changed.
5. No workflow, lockfile, package manifest, dependency/tool version, or bypass was edited.

## Validation review

1. Reproduced RED for the root manifest on the base configuration: exit 4; four 0.14.0 own-crate license rejections; sources passed.
2. Reproduced RED for the fuzz manifest on the base configuration: exit 4; three shipping-crate license rejections; sources passed.
3. GREEN for the root manifest after the correction: exit 0, `licenses ok, sources ok`.
4. GREEN for the fuzz manifest after the correction: exit 0, `licenses ok, sources ok`.
5. Structural TOML scope comparison passed, and `git diff --check` passed. These checks establish the local pinned-tool result; they do not establish CI results on every runner platform.

## Reproduction and evidence

Tool: `/tmp/hoimin-cargo-deny-859/cargo-deny-0.20.2-aarch64-apple-darwin/cargo-deny`, reports `cargo-deny 0.20.2`; SHA-256 `5f65c07c459c9514f0c97cc2e2fb6b120daef2d95aee31062cab4816cf027eb1`.

All commands used `CARGO_BUILD_JOBS=2`, `CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`, and `RUST_TEST_THREADS=2`, with the pinned binary directory prepended to `PATH`:

- `cargo deny --manifest-path Cargo.toml --workspace --all-features --locked --config deny.toml check --deny warnings licenses sources` — RED on base, exit 4; GREEN after change, exit 0.
- `cargo deny --manifest-path fuzz/Cargo.toml --workspace --all-features --locked --config deny.toml check --deny warnings licenses sources` — RED on base, exit 4; GREEN after change, exit 0.

Logs are under `/tmp/hoimin-priority-six-ci-policy/`:

- Original CI failure log SHA-256: `c0f28bc03b05664dd3bce325aa5da82ae324898be1b325b277bde19a45362085`
- `red-root.log`: `1503baa5d8311e13aea2bc8a9c8c8c3cadb0b712da3a0bd2eaf22e5b44899190`
- `red-fuzz.log`: `eb260327fa36827e9fa8f1af01e2a547abd78b577921ef37c5c902490a74bddb`
- `green-root.log`: `a09bbb250feeb5c0d6ac91177e3a1730bf09901eefcec1269ea8d00949a8f1a4`
- `green-fuzz.log`: `a09bbb250feeb5c0d6ac91177e3a1730bf09901eefcec1269ea8d00949a8f1a4`

Source head before the change: `2712291daf85d91ddbff2b166b23ba16fbab6924`. `deny.toml` diff SHA-256: `04fea229e41353c1195bf4a2689f11be1fc1672990bd8eacff84b1b2f5b7b588`.

## Final commit and working-tree receipt

Final product commit: `060b69d7536d40c84b3396cad84e7d2e2ecc3894`; its only changed file is `deny.toml`. The amend completed with exit 0 and normal hooks enabled (`PREK_NO_CONCURRENCY=1`, bounded Cargo variables). Hook output: workspace fmt, vendored parser fmt, workspace clippy, and vendored parser clippy were all skipped because no matching files were present; no hook failed. `git diff HEAD^ HEAD --check` passed, and `git status --porcelain=v1` was empty after the amend. Commit log and machine-readable receipt are saved at `/tmp/hoimin-priority-six-ci-policy/final-commit.log` and `/tmp/hoimin-priority-six-ci-policy/final-commit-receipt.json`.
