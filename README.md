# hoimin

hoimin is a bounded mutation-testing CLI for focused Python changes. Its Rust analyzer mutates copied source files, runs the test command once for the baseline and once per selected mutant, and emits versioned machine-readable results.

## Run it

`uvx` and `pipx run` install hoimin's native binary wheel in an isolated environment for a single invocation. Rust performs analysis in-process; the command after `--` is the test command for the project being checked.

```console
uvx hoimin run --root . --file src/calc.py --format json -- python -m pytest -q
```

### Plan and verify with an agent

Create a plan before improving tests, then verify the candidate IDs selected from that plan:

```console
hoimin plan --root . --source src --profile focused \
  --fingerprint-include pyproject.toml -- python -m pytest -q > PLAN.json
# improve tests, then replace <ID_FROM_PLAN_JSON> with an exact candidates[].id value
hoimin verify PLAN.json --candidate '<ID_FROM_PLAN_JSON>' --format json
```

`plan` discovers candidates but does not run a baseline or test command, copy a worker, or create or reuse a session. A manifest with `truncated` set to `true` contains only a partial candidate set, and `plan` exits 4; it cannot establish full coverage of the selected targets. `verify` rejects a changed target or fingerprint input before its baseline runs. Each `verify` command runs a fresh baseline and does not use a session. Plan manifests are trusted local invocation data, not a security boundary.

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

`--fingerprint-include GLOB` records matching files as explicit session-fingerprint inputs. It only invalidates compatible-run reuse: it does not control worker copying, select mutation targets, or implicitly watch files. Use `--include GLOB` independently when a fingerprinted input must also be copied into each worker; no files are implicitly watched.

For example, when a test needs an ignored fixture input copied into its worker:

```console
hoimin run --root . --source src --fingerprint-include tests/fixtures/settings.toml --include tests/fixtures/settings.toml -- python -m pytest -q
```

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
| `--profile full|focused` | `full` | candidate-selection profile |
| `--fingerprint-include GLOB` | none | invalidates compatible session reuse; does not copy worker files |

By default there are no include/exclude overrides or SQLite session, and `--changed`, `--resume`, and `--allow-best-effort-memory` are disabled.

Every numeric limit must be nonzero. Memory, process, copy, and total-timeout limits are run-wide and are not multiplied by `--jobs`. On Windows, Job Objects provide hard process and memory enforcement. On Linux, delegated cgroup v2 provides hard enforcement. When hard enforcement is unavailable, Unix uses best-effort process-group and rlimit controls; such a run is rejected unless `--allow-best-effort-memory` is explicit. Reports identify `hard` or `best_effort` resource mode.

### Tuning parallel runs

Start with `--jobs 1` and a focused test command. Record the baseline elapsed time and
estimate one test worker's memory use before increasing concurrency gradually. A small
target can often keep `--jobs 1 --max-memory 1GiB --mutant-timeout auto`.

`--max-memory` is one run-wide limit shared by the analyzer, baseline, and all concurrent
workers; it is not multiplied by `--jobs`. When increasing `--jobs`, set an explicit
`--mutant-timeout` with headroom for contention instead of assuming that the `auto` value
derived from a single baseline will remain sufficient. For example, a focused baseline
that takes about 14 seconds can be tried with the following measured settings:

```console
hoimin run --root . --source src --profile focused --jobs 4 --max-memory 4GiB --mutant-timeout 2m -- python -m pytest -q
```

Measure your own suite rather than treating these values as a sizing formula. If results
contain `out_of_memory`, lower `--jobs` or raise `--max-memory`. If they contain `timeout`,
lower `--jobs` or raise `--mutant-timeout`.

hoimin copies regular files into isolated workers. It does not follow or copy symlinks; each skipped symlink produces a diagnostic. The original tree is checked for changes and workers are reset between mutants.

These controls reduce accidental resource exhaustion. hoimin executes user-selected Python and test programs and is **not a security boundary** for untrusted code.

## Mutation operators

The default runtime operator set is:

- equality (`==` ↔ `!=`) and ordered comparisons (`<`, `<=`, `>`, `>=`);
- membership (`in` ↔ `not in`) and identity (`is` ↔ `is not`);
- boolean `and` ↔ `or`;
- binary and augmented `+` ↔ `-`;
- `*` ↔ `/` and `//` ↔ `%`;
- unary `+` ↔ `-`;
- removal of unary `not`;
- `True` ↔ `False`;
- `break` ↔ `continue`.

Without `--operators`, a run selects all 13 runtime operators and does not mutate type annotations. Supplying `--operators` (comma-separated) selects an explicit operator set instead; `--exclude-operators` then removes individual operators or selector families from that set.

For example, run a type checker against nullable and collection annotation mutations:

```console
hoimin run --root . --source src --operators type_nullable,type_collections --format json -- uv run ty check
```

The type selector families are `type_nullable`, `type_collections`, and `type_iterables`; individual IDs are `type_nullable_remove`, `type_nullable_add`, `type_list_sequence`, `type_set_abstract_set`, `type_dict_mapping`, `type_iterable_iterator`, and `type_sequence_iterable`. For example, append `--exclude-operators type_nullable_add` to retain nullable-removal mutations only. As with every command after `--`, `uv`, `run`, `ty`, and `check` are direct argv elements on both Windows and Unix.

A type checker that exits nonzero for a mutated annotation kills that mutant.

The Rust analyzer preserves the original source except for exactly one replacement per mutant.

## Mutation profiles

`--profile full` is the default and considers every candidate produced by the selected
operators. `--profile focused` suppresses candidates in Python `__main__` guards, bare
`print(...)` calls, `assert` statements, and function default expressions. It is intended
to reduce low-value survivors in focused change checks, but may omit useful mutants; use
`full` when measuring the complete selected target.

```console
hoimin run --profile focused --root . --source src -- python -m pytest -q
```

## Results

### Mutation-progress reports

Compare ordered run reports to track mutation-testing progress. Inputs are oldest-to-newest, and `--patience` defaults to three consecutive comparable stalls.

```console
hoimin progress --patience 3 reports/before.json reports/after.json reports/latest.json
hoimin progress --format json reports/*.json
```

Comparisons require adjacent reports that are both complete and have successful baselines. `saturated` means the configured number of consecutive comparable stalls was reached. JSON output exposes `latest.state`; agents should use that field, rather than the command exit code, to make progress decisions. A surviving mutant is not proof of behavioral equivalence.

`--format json` emits one document. `--format jsonl` emits flushed lifecycle events; diagnostics are JSON Lines on stderr. Public JSON contracts are versioned in [`run-result.schema.json`](docs/json-schema/run-result.schema.json) and [`run-event.schema.json`](docs/json-schema/run-event.schema.json). Event kinds are `run_started`, `baseline_finished`, `mutant_started`, `mutant_finished`, `diagnostic`, and `run_finished`. Parallel events are emitted in completion order; candidate sequence numbers allow stable reordering.

The final summary's `complete` is `false` when any mutant is inconclusive or the run fails or is interrupted. It is `true` only when every selected mutant is `killed` or `survived` and no run-level failure occurred; a successful run with no candidates is also complete. Therefore an exit code of `4` always has `complete: false`.

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

No database is created by default. `--session PATH` stores a run in SQLite and commits each mutant result independently. `--resume` requires `--session` and looks up the newest compatible incomplete run. Compatibility includes source and configuration fingerprints, test argv, safety limits, and the operator set. Profile selection is part of session compatibility, so a focused run never resumes results from a full run and vice versa. Completed `killed` and `survived` results can be reused; `timeout`, `out_of_memory`, `process_limit`, `error`, and `not_run` are run again. An incompatible or already complete run is not silently mixed with new results.

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
