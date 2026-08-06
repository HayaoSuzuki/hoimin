# hoimin

hoimin is a bounded mutation-testing CLI for focused Python changes. Its Rust analyzer mutates copied source files, runs the test command once for the baseline and once per selected mutant, and emits versioned machine-readable results.

## Run it

`uvx` and `pipx run` install hoimin's native binary wheel in an isolated environment for a single invocation. Rust performs analysis in-process; the command after `--` is the test command for the project being checked.

```console
uvx hoimin run --root . --file src/calc.py --format json -- python -m pytest -q
```

### Plan and verify with an agent

Create a ranked plan before improving tests, then verify its highest-ranked retained candidates:

```console
hoimin plan --root . --source src --changed \
  --allow-best-effort-memory \
  --total-timeout 15m \
  -- python -m pytest -q > PLAN.json
# improve tests, then run the saved top 10 without extracting IDs
hoimin verify PLAN.json --top 10 --format json > reports/batch-a-001.json
# alternatively, spread an equal-score tier across production files
hoimin verify PLAN.json --top 10 --selection-policy diverse
```

Each version-2 plan candidate records `rank`, `score`, and `ranking_reasons`.
The scores are transparent ordering heuristics for focusing effort; they do not
claim that a higher-ranked mutant is more likely to reveal a defect, and
lower-ranked candidates remain valid. `verify` uses the saved ranks and never re-ranks
against changed source or Git state.

The `strict` selection policy is the default and uses the saved rank prefix.
The `diverse` selection policy round-robins production files only within equal-score tiers.
Higher-score tiers are exhausted before lower-score tiers.
The verification report records `file_round_robin_v1`.
Verification does not rewrite the plan or change its execution limits.

`--candidate ID` remains available for exact selection and may be repeated.
`--candidate` and `--top` are mutually exclusive, and one selection mode is
required. If `N` exceeds the retained candidate count, `--top N` selects every retained candidate
and reports the actual selected count. Version-1 manifests
must be regenerated with the current `hoimin plan`.

`--candidate` is repeatable, so one `verify` invocation can execute multiple planned candidates.
`verify` inherits the test command, execution limits, timeout settings, and resource policy from
`PLAN.json`; only verify-specific output choices such as `--format` are selected at verify
time. It cannot override plan-time settings. In particular, `verify --top` retains all limits
from the manifest. A timeout-capacity warning is an estimate, not a guaranteed failure, and does
not adjust those limits automatically. To change `--jobs` or `--total-timeout`, create a new plan;
the same applies to `--allow-best-effort-memory` or another execution or resource setting.

The default total timeout is five minutes. Give a large candidate selection enough headroom
when creating the plan, or split it into stable batches across multiple `verify` invocations.
On macOS, `--max-memory` is accepted for plan compatibility but is not enforced. CPU-time limits
and process-group cleanup remain available; pass `--allow-best-effort-memory` to `plan` when this
best-effort memory policy is acceptable. `verify` does not provide that option.

`plan` discovers candidates but does not run a baseline or test command, copy a worker, or create or reuse a session. A manifest with `truncated` set to `true` contains only a partial candidate set, and `plan` exits 4; it cannot establish full coverage of the selected targets. On such a plan, `--top N` means the top N among retained candidates, not among candidates that discovery did not retain. `verify` still runs the retained selection, but its report remains incomplete and exits 4 because discovery was truncated. `verify` rejects a changed target or fingerprint input before its baseline runs. Each `verify` command runs a fresh baseline and does not use a session. Plan manifests are trusted local invocation data, not a security boundary.

Use a narrower selector to check one symbol:

```console
pipx run hoimin run --root . --source src --symbol calc:add --format jsonl -- python -m pytest -q
```

In a persistent installation, replace the launch prefix with `hoimin`. Everything after `--` is passed directly as native argv to the child process. hoimin does not join it into a command string, invoke a shell, expand globs or variables, or interpret shell quoting.

## Select mutation targets

At least one target selector is required:

- `--source DIR` selects Python files below a source root and may be repeated. `--source .` selects every discovered Python file below the configured root. An absolute `--source` path exactly equal to an absolute `--root` has the same effect.
- `--file PATH` selects an entire Python file and may be repeated.
- `--line PATH:START-END` selects an inclusive line range and may be repeated.
- `--symbol MODULE:QUALNAME` selects a function, method, or class resolved below `--source` and may be repeated.
- `--changed` restricts selection to staged, unstaged, and untracked Git changes. It requires `--source`.
- `--diff-base REV` uses the merge base of `REV` and `HEAD` for `--changed`. It is invalid without `--changed`.

Explicit selectors form a union. For example, `--source src --file src/calc.py` selects `src/calc.py` and every Python file below `src`. Use `--file` alone for a run limited to named files.

`--root DIR` resolves relative paths and defaults to the current directory. Combining explicit selectors with `--changed` intersects each explicit target with changed lines. When that target also has a symbol selector, a candidate must be both on a changed line and inside the selected symbol. A `--symbol` requires `--source`; when `--source` is present, file and line paths must be inside a source root.

`--include GLOB` can restore files excluded by ignore rules or built-in copy exclusions. `--exclude GLOB` adds exclusions and wins when both match. Both options may be repeated.

`--fingerprint-file PATH` records exactly one regular file at the specified `--root`-relative path and may be repeated. Every path component must remain beneath `--root`; symlink and reparse-point components are rejected instead of followed. It does not search nested directories, and characters such as `*`, `?`, and `[` are treated literally. Use it for a root-level configuration file without also selecting files with the same name in nested worktrees.

`--fingerprint-include GLOB` records every matching file as an explicit session-fingerprint input. A basename-only glob such as `pyproject.toml` can match that name at any depth. Both fingerprint options only invalidate compatible-run reuse: they do not control worker copying, select mutation targets, or implicitly watch files. Use `--include GLOB` independently when a fingerprinted input must also be copied into each worker; no files are implicitly watched.

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
| `--profile full` / `--profile focused` | `full` | candidate-selection profile |
| `--fingerprint-include GLOB` | none | invalidates compatible session reuse; does not copy worker files |
| `--fingerprint-file PATH` | none | fingerprints one exact root-relative file; does not copy worker files |

By default, there are no include/exclude overrides or SQLite session, and `--changed`, `--resume`, and `--allow-best-effort-memory` are disabled.

Every numeric limit must be nonzero. Memory, process, copy, and total-timeout limits are run-wide and are not multiplied by `--jobs`. On Windows, Job Objects provide hard process and memory enforcement. On Linux, delegated cgroup v2 provides hard enforcement. When hard enforcement is unavailable, Unix uses best-effort process-group and rlimit controls; such a run is rejected unless `--allow-best-effort-memory` is explicit. On macOS specifically, the memory limit is not enforced, while CPU-time limits and process-group cleanup remain available. Reports identify `hard` or `best_effort` resource mode.

### Tuning parallel runs

Start with `--jobs 1` and a focused test command. Record the baseline elapsed time and
estimate one test worker's memory use before increasing concurrency gradually. A small
target can often keep `--jobs 1 --max-memory 1GiB --mutant-timeout auto`.

`--max-memory` is one run-wide limit shared by the analyzer, baseline, and all concurrent
workers; it is not multiplied by `--jobs`. When increasing `--jobs`, set an explicit
`--mutant-timeout` with headroom for contention instead of assuming that the `auto` value
derived from a single baseline will remain sufficient. For example, a focused baseline
that takes about 14 seconds can be tried with the following measured settings:

`--max-memory` constrains descendant processes, not the hoimin CLI itself. The CLI keeps
one immutable pristine workspace snapshot on disk for the run and shares it across
workers, reading files on demand for reset. Per-worker copy accounting remains governed
by `--max-copy-size`; allow disk capacity for the shared pristine copy in addition to the
materialized workers.

```console
hoimin run --root . --source src --profile focused --jobs 4 --max-memory 4GiB --mutant-timeout 2m -- python -m pytest -q
```

Measure your own suite rather than treating these values as a sizing formula. If results
contain `out_of_memory`, lower `--jobs` or raise `--max-memory`. If they contain `timeout`,
lower `--jobs` or raise `--mutant-timeout`.

hoimin copies regular files into isolated workers. It does not follow or copy symlinks; each skipped symlink produces a diagnostic. The original tree is checked for changes and workers are reset between mutants.

These controls reduce accidental resource exhaustion. hoimin executes user-selected Python and test programs and is **not a security boundary** for untrusted code.

## Mutation operators

Without `--operators`, a run selects all 31 runtime operators. `--operators`
(comma-separated) selects an explicit set; `--exclude-operators` then removes
individual IDs or selector families. Type-annotation `type_*` operators remain
opt-in.

| Group | Runtime IDs | Mutations |
| --- | --- | --- |
| Existing expression/control flow | `compare_eq_ne`, `compare_order`, `membership`, `identity`, `boolean_and_or`, `binary_add_sub`, `augmented_add_sub`, `binary_mul_div`, `binary_floor_mod`, `unary_sign`, `remove_not`, `boolean_literal`, `break_continue` | comparisons, membership/identity, arithmetic, boolean/literal, and control-flow mutations |
| Collection calls and literals | `collection_any_all`, `collection_list_tuple`, `collection_set_frozenset`, `collection_append_insert` | `any(x)` ↔ `all(x)`; `list`/`tuple` and `set`/`frozenset` calls; load-context list/tuple literals; `seq.append(x)` ↔ `seq.insert(0, x)` |
| Same-contract methods | `collection_min_max`, `collection_set_add_discard`, `collection_set_remove_discard`, `collection_string_starts_ends`, `collection_string_split_rsplit` | `min(...)` ↔ `max(...)`; `add`/`discard`, `remove`/`discard`, `startswith`/`endswith`, and `split`/`rsplit` |
| Structural calls | `structure_append_extend`, `structure_mapping_get_subscript`, `structure_sort_reverse`, `structure_sorted_reversed` | `append(x)` ↔ `extend([x])`; `mapping.get(k)` ↔ `mapping[k]`; `sort()` ↔ `reverse()`; `sorted(x)` ↔ `reversed(x)` |
| Bitwise operators | `bitwise_and_or`, `bitwise_shift` | `&` ↔ `\|`; `<<` ↔ `>>` |
| Boundary operators | `structure_index_neighbor`, `structure_slice_neighbor` | adjacent plain-decimal index and slice-bound values |
| Exception handlers | `exception_type_pair` | curated `except` type pairs such as `ValueError` ↔ `TypeError` |

The runtime selector families are `collection_ops`, `structure_ops`, and
`bitwise_ops`, and `exception_ops`; for example,
`--exclude-operators collection_ops` removes the collection family while
leaving the other selected IDs enabled. The type selector families are
`type_nullable`, `type_collections`, and `type_iterables`. The type operator
IDs are `type_nullable_remove`, `type_nullable_add`, `type_list_sequence`,
`type_set_abstract_set`, `type_dict_mapping`, `type_iterable_iterator`, and
`type_sequence_iterable`. When loading persisted plan configurations, the
historical `type_mapping` name remains accepted as an alias for
`type_dict_mapping`.

The exception selector `exception_risky` is opt-in only. It exposes
`exception_bare_to_exception`, `exception_exception_to_bare`,
`exception_base_boundary`, `exception_tuple_add_pair`, and
`exception_tuple_remove_member`. These mutations can broaden or narrow a
handler and the BaseException boundary can catch `SystemExit`,
`KeyboardInterrupt`, or `GeneratorExit`; none is enabled by default. Safe
exception pairs are limited to `ValueError`/`TypeError`, `KeyError`/`IndexError`,
`AttributeError`/`KeyError`, `FileNotFoundError`/`PermissionError`,
`ConnectionError`/`TimeoutError`, `ImportError`/`ModuleNotFoundError`, and
`ZeroDivisionError`/`OverflowError`. Qualified or dynamic handlers, `except*`,
and unsupported tuple members are skipped.

The collection/structural operators are deliberately syntax-directed. They do
not include an `append`/`pop` mutation, comprehensions, assignment or delete
targets, or wrapping set literals in `frozenset(...)`. Unsupported keyword or
starred call forms, arbitrary index expressions, and zero-step slice mutations
are also excluded. See the development guide for the exact accepted shapes.

For example, run a type checker against nullable and collection annotation mutations:

```console
hoimin run --root . --source src --operators type_nullable,type_collections --format json -- uv run ty check
```

For example, append `--exclude-operators type_nullable_add` to retain
nullable-removal mutations only. As with every command after `--`, `uv`, `run`,
`ty`, and `check` are direct argv elements on both Windows and Unix.

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

Comparisons require adjacent reports that are both complete and have successful baselines. `saturated` means the configured number of consecutive comparable stalls was reached. Only immediately adjacent stalled comparisons contribute to this count. An improving, regressing, or indeterminate comparison resets the consecutive stall chain. JSON output exposes `latest.state`; agents should use that field, rather than the command exit code, to make progress decisions. A surviving mutant is not proof of behavioral equivalence.

For split verification, choose stable candidate batches and keep a separate oldest-to-newest
report history for each batch.
After every test improvement, rerun every stable batch and save each report separately.
Pass `hoimin progress` only reports covering the identical candidate-ID set.
The command marks a comparison `indeterminate`, resets its comparable stall
chain, and writes a warning when adjacent candidate-ID sets differ or contain
duplicates. If batch membership changes, start a new history.
Reports from different subsets, their
scores, and their saturation states must not be combined into a synthetic whole-plan result.
Judge overall completion from each batch's latest complete report for the same current test revision,
accounting for the union of candidate IDs selected from the plan.

`--format json` emits one document. `--format jsonl` emits flushed lifecycle events; diagnostics are JSON Lines on stderr. Public JSON contracts are versioned in [`run-result.schema.json`](docs/json-schema/run-result.schema.json) and [`run-event.schema.json`](docs/json-schema/run-event.schema.json). Event kinds are `run_started`, `baseline_finished`, `mutant_started`, `mutant_finished`, `diagnostic`, and `run_finished`. Parallel events are emitted in completion order; candidate sequence numbers allow stable reordering.

The final summary's `complete` is `false` when any mutant is inconclusive or the run fails or is interrupted. It is `true` only when every selected mutant is `killed` or `survived` and no run-level failure occurred; a successful run with no candidates is also complete. Therefore, an exit code of `4` always has `complete: false`.

### Operational metrics

Run metrics are an opt-in operational sidecar, separate from the run JSON. Write them by passing a destination to `--metrics`:

```console
hoimin run --root . --source src --metrics metrics.json -- python -m pytest -q
```

The sidecar uses the versioned [`run-metrics.schema.json`](docs/json-schema/run-metrics.schema.json) contract. Its `executed` count never exceeds `discovered` and equals the sum of per-worker `processes`. Metrics are operational observations: they do not affect resume compatibility and are not embedded in the run-result document. A metrics write failure warns without changing the mutation result.

The schema validates the document structure, schema version, and nonnegative integer values. The producer additionally guarantees unique stage names and workers in ascending worker-ID order; these semantic constraints are enforced by `RunMetrics::validate()` rather than JSON Schema.

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

Releases are built only from matching `v*` tags. Version tags build and retain wheel artifacts without publishing them to PyPI. Public distribution is not enabled.

Before enabling public distribution, add a separate manually triggered workflow, protect its GitHub environment with required reviewers or equivalent rules, and register PyPI Trusted Publishing for only that workflow and environment. Grant `id-token: write` only to its publication job, and update the workflow contract tests in the same change.
