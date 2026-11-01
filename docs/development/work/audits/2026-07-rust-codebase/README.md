# Rust Codebase Audit — July 2026

Branch: `audit/rust-codebase-2026-07`
Base commit: `4a93b2720ee4be3ef9c0664b4c8b6116776eabc9`

## Status

`complete`

## Evidence conventions

- Command evidence records the command, host platform, exit code, and output path.
- Code evidence names exact files and line numbers at the base commit.
- `confirmed bug` requires a reproduction or a violated executable contract.
- `high-risk design` requires a concrete failure path and an unenforced invariant.
- `maintainability` requires a proposed boundary and characterization strategy.
- Platform behavior not executed locally is marked `limited`, never `pass`.

## Executable quality-gate baseline

Recorded on `2026-07-23` on macOS `15.7.7` (`Darwin 24.6.0`, `arm64`).
Paths below are local, ignored evidence under
`.audit/rust-codebase/quality-gates/`.

| Command | Date | Host OS / architecture | Exit status | Local log path |
| --- | --- | --- | ---: | --- |
| `git check-ignore .audit/rust-codebase/quality-gates` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/check-ignore.log` |
| `cargo fmt --all -- --check` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/fmt.log` |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/clippy.log` |
| `cargo test --workspace` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/workspace-tests.log` |
| `cargo test -p hoimin-core --features contracts` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/core-contracts.log` |
| `cargo test -p hoimin-cli --features contracts` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/cli-contracts.log` |
| `uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v` (sandboxed attempt) | 2026-07-23 | macOS 15.7.7 / arm64 | 2 | `.audit/rust-codebase/quality-gates/uv-sandbox-failure.log` |
| `UV_CACHE_DIR=.audit/rust-codebase/uv-cache uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v` (workspace-local cache rerun) | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/python-tests.log` |
| `cargo build --workspace --release` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/release-build.log` |
| `uv run --frozen maturin build --release` (sandboxed attempt) | 2026-07-23 | macOS 15.7.7 / arm64 | 2 | `.audit/rust-codebase/quality-gates/uv-sandbox-failure.log` |
| `UV_CACHE_DIR=.audit/rust-codebase/uv-cache uv run --frozen maturin build --release` (workspace-local cache rerun) | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/wheel-build.log` |
| `wheel_path=$(find target/wheels -type f -name '*.whl' -print \| sort \| tail -1); HOIMIN_WHEEL="$wheel_path" uv run --frozen python tests/wheel_smoke.py` (sandboxed attempt) | 2026-07-23 | macOS 15.7.7 / arm64 | 1 | `.audit/rust-codebase/quality-gates/wheel-smoke-sandbox-failure.log` |
| `wheel_path=$(find target/wheels -type f -name '*.whl' -print \| sort \| tail -1); UV_CACHE_DIR=.audit/rust-codebase/uv-cache HOIMIN_WHEEL="$wheel_path" uv run --frozen python tests/wheel_smoke.py` (unrestricted rerun) | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/wheel-smoke.log` |
| `cargo tree -p hoimin-core --edges normal --prefix none` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/core-tree.log` |
| `! rg '(^\| )(tokio\|rusqlite\|tempfile\|windows-sys\|libc\|hoimin-cli)( \|$)' .audit/rust-codebase/quality-gates/core-tree.log` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/core-purity.log` |
| `cargo tree --workspace --duplicates` | 2026-07-23 | macOS 15.7.7 / arm64 | 0 | `.audit/rust-codebase/quality-gates/duplicate-dependencies.log` |

The first sandboxed Python-test and wheel-build attempts exited `2` because
`~/.cache/uv` was not writable; their exact diagnostics are retained in
`.audit/rust-codebase/quality-gates/uv-sandbox-failure.log`. The first wheel
smoke attempt exited `1` because its nested `uvx` could not write
`~/.local/share/uv/tools`; that diagnostic is retained in
`.audit/rust-codebase/quality-gates/wheel-smoke-sandbox-failure.log`. These
are execution-environment limitations, not product-gate failures. The wheel
used by the successful smoke run is recorded in
`.audit/rust-codebase/quality-gates/wheel-path.log`.

The duplicate dependency report contains multiple `getrandom`, `rand`,
`rand_core`, `rand_chacha`, `phf_shared`, `ppv-lite86`, and `smallvec`
instances. No concrete compatibility, binary-size, or security burden was
established at baseline, so these remain maintenance observations rather than
findings.

## Finding identifiers

Use `RUST-AUDIT-NNN`, assigned monotonically when a lead first enters the findings ledger.
Rejected leads keep their identifiers so later rows and issue links never shift.

## Artifacts

- [Coverage matrix](coverage.md)
- [Findings ledger](findings.md)
- [Issue map](issues.md)
- [Final report](report.md)
