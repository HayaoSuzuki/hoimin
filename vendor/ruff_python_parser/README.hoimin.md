# Ruff parser backport for hoimin

This directory contains `littrs-ruff-python-parser 0.6.2` from the crates.io package, with the parser-stack fix from [Ruff PR #25464](https://github.com/astral-sh/ruff/pull/25464), merge commit `7a9aed24ffa150677657f9dde0eb252a8377c09d`, adapted to the pinned AST and Vec-based implementation.

The package's recorded VCS source is `chonkie-inc/littrs` commit `f57d08328da8f205d4377c4e8b5a628ab05a3ee8`, path `vendor/ruff_python_parser`. Its `src/`, `resources/`, and normalized `Cargo.toml` were copied from the published package. The upstream Ruff MIT license, including its third-party notices, is preserved in `LICENSE`; it was retrieved from the Ruff merge commit above because the published package omitted the license file.

## Local changes

- Add stacker (requirement 0.1.24, resolved to 0.1.25 in Hoimin's lockfile) and a `with_recursion` helper that checks available stack at every instrumented entry, with a 128 KiB red zone and 1 MiB growth segments.
- Guard initial module/expression parsing, the binary-expression entry, lambda bodies, conditional else arms, recursive format specifications, pattern LHS parsing, nested suites, and async error recovery. These are the final PR #25464 checkpoints.
- Do not copy upstream's 20-entry deferred-check optimization: the older parser's debug frame costs have not been shown to fit that unchecked budget. This changes stack probing, not syntax acceptance.
- Preserve parser API, AST types, token positions, and ordinary error recovery. AST/trivia/text-size dependencies remain at the pinned compatible version.
- Guard context assignment, assignment/delete target validation, pre-3.9 decorator traversal, and pattern-to-expression conversion with the same stack checks. These traverse already-built trees independently of the grammar checkpoints.
- Add `ast_cleanup` with iterative destruction shared with Hoimin's depth guard. Use it when discarding speculative with/match trees, invalid keyword patterns, and the subpattern of an unrepresentable `as` pattern during expression recovery. Never rely on one stack check around recursive derived Drop.
- Add a local workspace boundary for standalone formatting inside linked worktrees; ignore standalone build output and lockfiles. Hoimin builds and lint checks use the root lockfile.

Dynamic stack growth is limited by platform support and allocation availability. It is not a total memory bound. It does not automatically protect helper traversals or recursive destruction of an already-built tree. Hoimin retains its post-parse depth-128 rejection and iterative destruction, including partial syntax-error trees. See [issue #513 design](../../docs/superpowers/specs/2026-09-12-issue-513-parser-recursion-design.md) and its implementation plan for coverage and native-platform evidence.

The vendor is excluded from the workspace and selected by the workspace `[patch.crates-io]`. CI checks its formatting with `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check` and its library with `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`, in addition to workspace checks. The published package omits upstream test dependencies; Hoimin's `analysis_depth` integration tests execute the parser directly as well as public plan/run, and `rust_analyzer` checks repeated reclamation. Keep this patch narrow and replace the vendor with a compatible published release only after the same regression and candidate-compatibility checks pass.
