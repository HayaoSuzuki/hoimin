# Development

Run the Rust quality gate locally with the same commands used in CI:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uv run maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

The standalone wheel smoke script builds a release wheel unless `HOIMIN_WHEEL`
names an existing wheel to test.

After changing the Rust analyzer or its tests, run mutation analysis with:

```console
uv run hoimin run --root . --file crates/hoimin-cli/src/analyzer/mod.rs --max-candidates 1000 --max-mutants 1000 --jobs 1 --total-timeout 10m --allow-best-effort-memory --format json -- cargo test --workspace
```

## Rust mutation testing

Install `cargo-mutants` locally, then use the full command to discover every
outcome. While adding tests, `--iterate` reuses previously caught and unviable
outcomes; do not use it for the required final check.

```console
cargo install --locked cargo-mutants

# Discover all remaining outcomes.
cargo mutants --workspace

# While adding tests, reuse caught and unviable outcomes from the prior run.
cargo mutants --workspace --iterate

# Required final check: do not use --iterate here.
cargo mutants --workspace
```

`mutants.out/missed.txt` requires a behavior test unless the exact mutant is
equivalent. Resolve `timeout.txt`, a failed baseline, and tool errors;
`unviable.txt` is inconclusive. Each allowed exception is an anchored
complete-name `exclude_re` with a TOML reason comment. This workflow is local
and does not run in CI.
