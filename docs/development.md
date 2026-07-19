# Development

Run the Python quality gate locally with the same commands used in CI:

```console
uv sync --frozen
uv run ruff check python tests
uv run ruff format --check python tests
uv run ty check
uv run pytest
```

`pytest` builds one release wheel per session for `tests/wheel_smoke.py` unless
`HOIMIN_WHEEL` names an existing wheel to test.

After changing the analyzer or its tests, run mutation analysis with:

```console
uv run maturin develop
uv run hoimin run --root . --source python --file python/hoimin_analyzer.py --python python --max-candidates 1000 --max-mutants 1000 --jobs 1 --total-timeout 10m --allow-best-effort-memory --format json -- python -m pytest python/tests -q
```