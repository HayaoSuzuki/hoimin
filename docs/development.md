# Development

Run the Rust quality gate locally with the same commands used in CI:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin build --release
uv run python tests/wheel_smoke.py
```

The standalone wheel smoke script builds a release wheel unless `HOIMIN_WHEEL`
names an existing wheel to test.

After changing the Rust analyzer or its tests, run mutation analysis with:

```console
uv run hoimin run --root . --source crates --file crates/hoimin-cli/src/analyzer/mod.rs --max-candidates 1000 --max-mutants 1000 --jobs 1 --total-timeout 10m --allow-best-effort-memory --format json -- cargo test --workspace
```
