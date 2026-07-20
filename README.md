# hoimin

hoimin is a bounded mutation-testing CLI for focused Python changes. Its Rust analyzer mutates copied source files, runs the test command once for the baseline and once per selected mutant, and emits versioned machine-readable results.

## Run it

`uvx` and `pipx run` install hoimin's native binary wheel in an isolated environment for a single invocation. Rust performs analysis in-process; the command after `--` is the test command for the project being checked.

```console
uvx hoimin run --root . --file src/calc.py --format json -- python -m pytest -q
```

Use a narrower selector to check one symbol:

```console
pipx run hoimin run --root . --source src --symbol calc:add --format jsonl -- python -m pytest -q
```

In a persistent installation, replace the launch prefix with `hoimin`. Everything after `--` is passed directly as native argv to the child process. hoimin does not join it into a command string, invoke a shell, expand globs or variables, or interpret shell quoting.

## Select mutation targets

At least one target selector is required:

- `--source DIR` selects Python files below a source root and may be repeated.
- `--file PATH` selects an entire Python file and may be repeated.
- `--line PATH:START-END` selects an inclusive line range and may be repeated.
- `--symbol MODULE:QUALNAME` selects a function, method, or class resolved below `--source` and may be repeated.
- `--changed` restricts selection to staged, unstaged, and untracked Git changes. It requires `--source`.
- `--diff-base REV` uses the merge base of `REV` and `HEAD` for `--changed`. It is invalid without `--changed`.

Explicit selectors form a union. For example, `--source src --file src/calc.py` selects `src/calc.py` and every Python file below `src`. Use `--file` alone for a run limited to named files.

`--root DIR` resolves relative paths and defaults to the current directory. Combining explicit selectors with `--changed` takes their intersection with changed lines. A `--symbol` requires `--source`; when `--source` is present, file and line paths must be inside a source root.

`--include GLOB` can restore files excluded by ignore rules or built-in copy exclusions. `--exclude GLOB` adds exclusions and wins when both match. Both options may be repeated.

## Limits and defaults

The defaults are:

| Option | Default | Scope |
| --- | ---: | --- |
| `--jobs` | `1` | concurrent workers; maximum 256 and never greater than `--max-processes` |
| `--max-mutants` | `100` | mutants executed |
| `--max-candidates` | `10000` | candidates discovered before execution |
| `--analyzer-timeout` | `30s` | each analyzer process |
| `--baseline-timeout` | `60s` | baseline process |
| `--mutant-timeout` | `auto` | each mutant; `max(5s, 2 × baseline elapsed + 1s)` |
| `--total-timeout` | `5m` | complete run |
| `--max-memory` | `1GiB` | run-wide descendant memory |
| `--max-output` | `1MiB` | combined retained stdout and stderr per process |
| `--max-copy-size` | `1GiB` | run-wide logical bytes copied across all workers |
| `--max-processes` | `64` | run-wide descendants |
| `--format` | `json` | `json`, `jsonl`, or `human` |

By default there are no include/exclude overrides or SQLite session, and `--changed`, `--resume`, and `--allow-best-effort-memory` are disabled.

Every numeric limit must be nonzero. Memory, process, copy, and total-timeout limits are run-wide and are not multiplied by `--jobs`. On Windows, Job Objects provide hard process and memory enforcement. On Linux, delegated cgroup v2 provides hard enforcement. When hard enforcement is unavailable, Unix uses best-effort process-group and rlimit controls; such a run is rejected unless `--allow-best-effort-memory` is explicit. Reports identify `hard` or `best_effort` resource mode.

hoimin copies regular files into isolated workers. It does not follow or copy symlinks; each skipped symlink produces a diagnostic. The original tree is checked for changes and workers are reset between mutants.

These controls reduce accidental resource exhaustion. hoimin executes user-selected Python and test programs and is **not a security boundary** for untrusted code.

## Mutation operators

The MVP operator set is:

- equality (`==` ↔ `!=`) and ordered comparisons (`<`, `<=`, `>`, `>=`);
- membership (`in` ↔ `not in`) and identity (`is` ↔ `is not`);
- boolean `and` ↔ `or`;
- binary and augmented `+` ↔ `-`;
- `*` ↔ `/` and `//` ↔ `%`;
- unary `+` ↔ `-`;
- removal of unary `not`;
- `True` ↔ `False`;
- `break` ↔ `continue`.

The Rust analyzer preserves the original source except for exactly one replacement per mutant.

## Results

`--format json` emits one document. `--format jsonl` emits flushed lifecycle events; diagnostics are JSON Lines on stderr. Public JSON contracts are versioned in [`run-result.schema.json`](docs/json-schema/run-result.schema.json) and [`run-event.schema.json`](docs/json-schema/run-event.schema.json). Event kinds are `run_started`, `baseline_finished`, `mutant_started`, `mutant_finished`, `diagnostic`, and `run_finished`. Parallel events are emitted in completion order; candidate sequence numbers allow stable reordering.

Mutant statuses are:

- `killed`: the mutant test process exited nonzero;
- `survived`: it exited zero;
- `timeout`: its deadline elapsed;
- `out_of_memory`: the run-wide memory limit stopped it;
- `process_limit`: the run-wide descendant limit stopped it;
- `error`: infrastructure could not produce a valid result;
- `not_run`: it was discovered but not executed, including cancellation or a run-wide limit.

The score is `killed / (killed + survived)`. Inconclusive statuses are excluded. If no killed or survived mutant exists, `score` is `null`.

Exit codes are:

| Code | Meaning |
| ---: | --- |
| `0` | complete, with no survivor |
| `1` | complete, with at least one survivor |
| `2` | CLI, configuration, analyzer, storage, workspace, or other infrastructure error |
| `3` | baseline failed |
| `4` | incomplete because of timeout, memory/process/candidate/mutant limits, or `not_run` |
| `130` | user cancellation |

Higher-priority conditions win in this order: cancellation, infrastructure error, baseline failure, incomplete result, survivor.

## Sessions and resume

No database is created by default. `--session PATH` stores a run in SQLite and commits each mutant result independently. `--resume` requires `--session` and looks up the newest compatible incomplete run. Compatibility includes source and configuration fingerprints, test argv, safety limits, and the operator set. Completed `killed` and `survived` results can be reused; `timeout`, `out_of_memory`, `process_limit`, `error`, and `not_run` are run again. An incompatible or already complete run is not silently mixed with new results.

## Build and verify a wheel

The package is a native binary wheel, not a Python extension module. Build and smoke-test the wheel locally with:

```console
uv run maturin build --release
uv run python tests/wheel_smoke.py
```

The smoke test installs the wheel into a new environment and runs the Rust-only CLI outside this checkout. For development verification, see [the development guide](docs/development.md).

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin build --release
uv run python tests/wheel_smoke.py
```

Windows Job Object tests run on Windows and Linux hard-limit tests require a delegated cgroup v2 runner. The ordinary Linux CI job verifies the explicit best-effort path separately.

Releases are built only from matching `v*` tags. Before enabling publication, protect the repository `pypi` environment with the required reviewers or rules and register PyPI Trusted Publishing for this workflow and environment; the release job uses that environment's OIDC token rather than a stored upload token.
