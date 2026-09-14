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

Each plan candidate records `rank`, `score`, and `ranking_reasons`.
The scores are transparent ordering heuristics for focusing effort; they do not
claim that a higher-ranked mutant is more likely to reveal a defect, and
lower-ranked candidates remain valid. `verify` uses the saved ranks and never re-ranks
against changed source or Git state.

Ranking rule version 4 awards the explicit-symbol bonus to a selected symbol
and its dot-delimited descendants in the same file. For example, selecting
`Box` also boosts `Box.check` and `Box.Inner.check`, but not `BoxOther.check`.
Plans created with an older ranking rule must be regenerated before verification.

The `strict` selection policy is the default and uses the saved rank prefix.
The `diverse` selection policy round-robins production files only within equal-score tiers.
Higher-score tiers are exhausted before lower-score tiers.
The verification report records `file_round_robin_v1`.
Verification does not rewrite the plan or change its execution limits.

`--candidate ID` remains available for exact selection and may be repeated.
`--candidate` and `--top` are mutually exclusive, and one selection mode is
required. If `N` exceeds the retained candidate count, `--top N` selects every retained candidate
and reports the actual selected count. A plan with no retained candidates cannot be verified
with `--top`: both selection policies reject it with exit code 2 before running the baseline.
Version-1 manifests
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
On macOS, `--max-memory` is accepted for plan compatibility but is not enforced. Hoimin still enforces monotonic wall-clock deadlines. On timeout or cancellation, while it owns a live root, it terminates that process group and reaps the root; descendants left after the root exits naturally are not guaranteed to be cleaned up. Pass `--allow-best-effort-memory` to `plan` when this memory policy is acceptable. `verify` does not provide that option.

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

Whole-file selectors establish the selected files first. For example, `--source src --file src/calc.py` selects every Python file below `src`, while `--file src/calc.py` alone selects only that file. A `--line` or `--symbol` then narrows a matching file. Selectors for different files remain combined. The resolver merges multiple line ranges on one file. Candidates must satisfy both constraints when a file has line and symbol selectors.

Python source positions recognize LF, CRLF and lone CR, including mixtures, without normalizing file bytes. Previously saved plans with incorrect lone-CR coordinates must be regenerated; existing reports retain their recorded coordinates. Correcting line/column metadata does not change a candidate ID for identical source bytes and the same mutation.

For example, `--file src/calc.py --line src/calc.py:10-12` selects only lines 10 through 12 of `src/calc.py`. Adding `--file src/calc.py` to `--source src --symbol calc:add` preserves the symbol restriction on `src/calc.py`. Other Python files selected by `--source src` remain whole-file targets unless they have a line or symbol selector.

For a `src` layout installed through a path-only editable `.pth`, add an import
root independently of the selected file:

```console
hoimin run --root . --file src/calc.py --import-root src -- python -m pytest -q
hoimin plan --root . --line src/calc.py:10-12 --import-root src -- python -m pytest -q > plan.json
hoimin verify plan.json --top 1 --format json
```

`--import-root DIR` is repeatable on `run` and `plan`. It adds worker import
paths without selecting any mutation targets; a target selector is still
required. Directories are relative to `--root`, including `.`. Absolute paths
and parent components that escape the root are rejected. Harmless components
are normalized and duplicate roots retain their first position. PYTHONPATH
order is the worker root, explicit import roots in supplied order, selected
source roots, then inherited PYTHONPATH with project paths rewritten to the
worker. The named directory must exist in the copied worker before baseline;
an unavailable-root diagnostic means it is missing, is not a directory, or was
excluded by the existing copy policy. The option does not override exclusions.

This supports regular packages exposed by path-only `.pth` entries when Python
honors PYTHONPATH. Finder-based editable installs and custom import loaders are
not guaranteed, and Python `-E`/`-I` ignores PYTHONPATH. Setting inherited
`PYTHONPATH=/absolute/project/src` remains a workaround: hoimin rewrites that
project path to the worker. Neither approach edits the original source or venv.
Saved plans preserve import-root order; `verify` inherits it without a flag.

`--root DIR` resolves relative paths and defaults to the current directory. Combining explicit selectors with `--changed` intersects each explicit target with changed lines. When that target also has a symbol selector, a candidate must be both on a changed line and inside the selected symbol. A `--symbol` requires `--source`; when `--source` is present, file and line paths must be inside a source root.

Target, fingerprint, and copied-workspace paths use a portable `/`-separated representation. Native Windows path inputs are normalized to that form. On Unix, a concrete filename containing a literal backslash is rejected before collection because it cannot be represented unambiguously. Backslashes in glob options retain their existing escape syntax; the concrete paths matched by a glob are validated after walking.

`--include GLOB` can restore ignored or hidden files. `--exclude GLOB` adds exclusions and wins when both match. Both options may be repeated.

Target discovery and worker copying always exclude `.git`, `.venv`, `venv`, `env`, `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.pyre`, `.pytype`, `.tox`, and `.nox` below the project root. Names are compared case-insensitively on Windows and exactly on other platforms. Includes cannot restore these entries. Automatic source scans omit them; explicit `--file` and `--line` selectors inside them fail during target resolution with the path and remediation. Choose source outside the excluded directory. The project root itself remains usable even if its name is on this list. Ignore files apply in non-Git roots too; hidden mutation targets still require an include.

`--fingerprint-file PATH` records exactly one regular file at the specified `--root`-relative path and may be repeated. Every path component must remain beneath `--root`; symlink and reparse-point components are rejected instead of followed. It does not search nested directories, and characters such as `*`, `?`, and `[` are treated literally. Use it for a root-level configuration file without also selecting files with the same name in nested worktrees.

`--fingerprint-include GLOB` records every matching file as an explicit session-fingerprint input. A basename-only glob such as `pyproject.toml` can match that name at any depth. Both fingerprint options only invalidate compatible-run reuse: they do not control worker copying, select mutation targets, or implicitly watch files. Use `--include GLOB` independently when a fingerprinted input must also be copied into each worker; no files are implicitly watched.

For example, when a test needs an ignored fixture input copied into its worker:

```console
hoimin run --root . --source src --fingerprint-include tests/fixtures/settings.toml --include tests/fixtures/settings.toml -- python -m pytest -q
```

Each candidate must fit a 2 MiB compact JSON spool record, including JSON escaping, UTF-8 and one trailing newline. `plan` rejects oversized candidates instead of saving an unusable plan; `verify` rejects oversized saved records before baseline. Direct `run` discovers candidates after baseline and reports an incomplete infrastructure failure if a record exceeds this limit. The diagnostic identifies the source path, line, operator and limit. Candidate-count limits do not override this byte limit.

## Limits and defaults

The defaults are:

| Option | Default | Scope |
| --- | ---: | --- |
| `--jobs` | `1` | concurrent workers; maximum 256 and never greater than `--max-processes` |
| `--max-mutants` | `100` | mutants executed |
| `--max-candidates` | `10000` | candidates discovered before execution |
| `--analyzer-timeout` | `30s` | complete plan/verify discovery phase |
| `--baseline-timeout` | `60s` | baseline process |
| `--mutant-timeout` | `auto` | each mutant; `max(5s, 2 × baseline elapsed + 1s)` |
| `--total-timeout` | `5m` | complete run |
| `--max-memory` | `1GiB` | Windows: committed memory per root tree; Linux cgroup: run-wide |
| `--max-output` | `1MiB` | combined retained stdout and stderr per process |
| `--max-copy-size` | `1GiB` | run-wide logical bytes copied across all workers |
| `--max-workspace-size` | `8GiB` | logical bytes in generated workspaces and run-owned output |
| `--min-free-space` | `10GiB` | mandatory filesystem reserve before more work starts |
| `--max-processes` | `64` | Windows: processes per root tree, including the root; Linux cgroup: run-wide |
| `--format` | `json` | `json`, `jsonl`, or `human` |
| `--profile full` / `--profile focused` | `full` | candidate-selection profile |
| `--fingerprint-include GLOB` | none | invalidates compatible session reuse; does not copy worker files |
| `--fingerprint-file PATH` | none | fingerprints one exact root-relative file; does not copy worker files |

By default, there are no include/exclude overrides or SQLite session, and `--changed`, `--resume`, and `--allow-best-effort-memory` are disabled.

Every numeric limit must be nonzero. Copy and total-timeout limits are run-wide.

On Windows, each baseline or mutant root process and its descendants share their own full `--max-memory` and `--max-processes` allowance, enforced by a nested Job Object. Memory means committed memory, not RSS; the process count includes the root itself. With `--jobs J`, concurrent trees can approach `J × max-memory` and `J × max-processes` in total. The outer run Job Object owns cleanup and imposes no aggregate memory or process cap. Host-imposed parent Job Objects can make effective limits stricter.

On Linux, delegated cgroup v2 provides run-wide hard memory and process enforcement, shared across workers. When hard enforcement is unavailable, Unix uses best-effort process groups and non-macOS Unix also applies per-process `RLIMIT_AS`. Linux and macOS require explicit `--allow-best-effort-memory` approval for this policy. On macOS, the memory limit is not enforced. Hoimin uses monotonic wall-clock deadlines on portable Unix and, on timeout or cancellation while it owns a live root, terminates that process group and reaps the root. Cleanup of descendants after the root exits naturally is not guaranteed.

Reports identify `hard` or `best_effort` resource mode. This describes enforcement strength, not aggregation scope: on Windows, the memory and process limits in `normalized_config` apply per root tree.

Rust callers construct `WindowsBackend::new()` without run limits and supply caps through each `ProcessLimits` request; the formerly ignored constructor argument has been removed.

The run header records the selected backend in `resource_control.mode` and the stable `resource_control.mechanism` identifier (`portable`, `linux_cgroup_v2`, or `windows_job_object`). Synthetic reused and not-run results report the current run’s selected mode; reused results retain null termination/output and zero elapsed time. Actual process results retain their observed mode.

Linux cgroup hard enforcement requires `--max-memory` to be at least one host
page. Smaller values fail before a test-process cgroup is created or the test command starts;
the diagnostic reports the host-specific minimum. Larger limits continue to
round down to page granularity. This does not impose a new minimum on macOS or
portable best-effort backends.

`--max-copy-size` counts copied source bytes across workers. `--max-workspace-size`
counts generated workspace bytes, including materialized workers and run-owned output.
Keep the 10 GiB reserve even when the workspace limit is smaller. In this policy,
raising a consumption limit or lowering the reserve is explicit risk acceptance:
sampled monitoring can stop new work, but one child can consume the reserve between
samples. Aggregate hard enforcement requires a verified, named quota backend.
Without one, the disk guard provides cooperative enforcement.

During discovery of one source file, each token, AST, and type-annotation
producer retains at most `max_candidates + 1` candidate records and their
deduplication identities before the bounded merge. This limits retained
candidates, not all analyzer memory: source text, parser tokens, AST facts,
and small per-node replacement lists still scale with source size. Therefore
`--max-candidates` is not a general Hoimin memory limit. `--max-memory`
continues to govern descendant processes, not the Hoimin CLI itself.

Analysis supports AST depth up to 128, counting the module as depth 1 and
including auxiliary syntax nodes. A deeper tree fails with its source path and
`analysis depth exceeds supported limit 128`; it does not produce a complete
empty result. This limit applies even with `--max-candidates 1`. Flat files with
many shallow statements are not rejected by this depth limit.
The bundled Rust parser checks remaining stack during recursive parsing and
recovery. Rejected and discarded partial trees are destroyed iteratively, so
deep unary, power, lambda, and conditional expressions can reach this diagnostic.
Parser stack segments and AST storage still consume memory proportional to input.

For `plan` creation and `verify` rediscovery, `--analyzer-timeout` is one
deadline for the complete discovery phase, not a new deadline per target. A
discovery timeout returns promptly while an already-running blocking analyzer
cooperatively stops and releases the resources it owns. It cannot interrupt a
source read already blocked inside a system call; that read must return before
the detached analyzer can finish stopping.

### Tuning parallel runs

Start with `--jobs 1` and a focused test command. Record the baseline elapsed time and
estimate one test worker's memory use before increasing concurrency gradually. A small
target can often keep `--jobs 1 --max-memory 1GiB --mutant-timeout auto`.

`--max-memory` is shared across concurrent test workers under Linux cgroup hard enforcement.
On Windows, it applies separately to each root tree: for example, `--jobs 4 --max-memory 4GiB`
allows concurrent test trees to approach 16 GiB of committed memory. Size concurrency and
per-root caps together. The in-process analyzer is not covered by these descendant limits. When increasing `--jobs`, set an explicit
`--mutant-timeout` with headroom for contention instead of assuming that the `auto` value
derived from a single baseline will remain sufficient. For example, a focused baseline
that takes about 14 seconds can be tried with the following measured settings:

`--max-memory` constrains descendant processes, not the hoimin CLI itself. The CLI keeps
one immutable pristine workspace snapshot on disk for the run and shares it across
workers, reading files on demand for reset. Per-worker copy accounting remains governed
by `--max-copy-size`; allow disk capacity for the shared pristine copy in addition to the
materialized workers.

During reset, a worker file whose size differs from the pristine copy is restored
without reading its changed contents. Same-size files still undergo full byte and
permission comparison, including when their modification time has not changed.

```console
hoimin run --root . --source src --profile focused --jobs 4 --max-memory 4GiB --mutant-timeout 2m -- python -m pytest -q
```

Measure your own suite rather than treating these values as a sizing formula. If results
contain `out_of_memory`, lower `--jobs` or raise `--max-memory`. If they contain `timeout`,
lower `--jobs` or raise `--mutant-timeout`.

hoimin copies regular files into isolated workers. It does not follow or copy symlinks; each skipped symlink produces a diagnostic. The original tree is checked for changes and workers are reset between mutants.

These controls reduce accidental resource exhaustion. hoimin executes user-selected Python and test programs and is **not a security boundary** for untrusted code.

## Disk-safe execution

Hoimin treats disk safety as an ownership problem as well as a capacity limit. It
meters generated workspaces and run-owned output against `--max-workspace-size`,
checks `--min-free-space` before dispatching more work, and stops new mutation
work when either guard is reached. These checks are cooperative unless a
verified quota backend provides aggregate hard enforcement.

Cleanup is limited to run roots whose identity, filesystem, and ownership were
retained from creation. Hoimin does not follow symlinks or reparse points during
cleanup, use wildcard recursive deletion, or continue through an identity or
volume change. On Windows, pinned directory handles and handle-relative child
operations protect workspace creation, publication, measurement, and cleanup;
an unsupported identity, replacement race, sharing violation, or close failure
fails closed instead of falling back to pathname-based deletion. Caller-provided
output directories remain caller-owned.

If cleanup is deferred, `run.json` and `report.md` record the exact retained
scratch path and bounded diagnostics. Before removing it manually, verify its
lease and run identifier, then remove only that exact path. Never delete the
managed parent directory or use a wildcard. See
[Windows filesystem safety](docs/development.md#windows-filesystem-safety) for
the detailed capability and cleanup model.

## Mutation operators

Without `--operators`, a run selects all 43 runtime operators. `--operators`
(comma-separated) selects an explicit set; `--exclude-operators` then removes
individual IDs or selector families. Type-annotation `type_*` operators remain
opt-in. Hoimin exposes 55 operator IDs in total: 43 default runtime IDs, seven
opt-in type IDs, and five opt-in risky exception IDs.

| Group | Runtime IDs | Mutations |
| --- | --- | --- |
| Existing expression/control flow | `compare_eq_ne`, `compare_order`, `membership`, `identity`, `boolean_and_or`, `binary_add_sub`, `augmented_add_sub`, `binary_mul_div`, `augmented_mul_div`, `binary_floor_mod`, `augmented_floor_mod`, `unary_sign`, `remove_not`, `boolean_literal`, `break_continue` | comparisons; membership and identity; binary `+`/`-`, `*`/`/`, and `//`/`%`; augmented `+=`/`-=`, `*=`/`/=`, and `//=`/`%=`; unary signs; boolean expressions and literals, including `True`/`False` match patterns; and control flow |
| Collection calls and literals | `collection_any_all`, `collection_list_tuple`, `collection_set_frozenset`, `collection_append_insert` | `any(x)` ↔ `all(x)`; `list`/`tuple` calls and load-context literals outside exception-handler type positions (which Python requires to be an exception class or tuple of exception classes); `set` ↔ `frozenset` calls; `seq.append(x)` ↔ `seq.insert(0, x)` |
| Same-contract methods | `collection_min_max`, `collection_set_add_discard`, `collection_set_remove_discard`, `collection_string_starts_ends`, `collection_string_split_rsplit` | `min(...)` ↔ `max(...)`; `add`/`discard`, `remove`/`discard`, `startswith`/`endswith`, and `split`/`rsplit` when `maxsplit` is supplied |
| Structural calls | `structure_append_extend`, `structure_mapping_get_subscript`, `structure_sort_reverse`, `structure_sorted_reversed` | `append(x)` ↔ `extend([x])`; `mapping.get(k)` ↔ `mapping[k]`; `sort()` ↔ `reverse()`; `sorted(x)` ↔ `reversed(x)` |
| Bitwise operators | `bitwise_and_or`, `bitwise_shift` | `&` ↔ `\|`; `<<` ↔ `>>` |
| Additional operator syntax | `binary_power`, `binary_matmul`, `augmented_power`, `augmented_matmul`, `bitwise_xor`, `bitwise_invert`, `augmented_bitwise_and_or`, `augmented_bitwise_xor`, `augmented_bitwise_shift` | `**` and `@` become `*`; `**=` and `@=` become `*=`; `^` becomes `&`; `~` becomes unary `+`; `&=`/`\|=` and `<<=`/`>>=` exchange; `^=` becomes `&=` |
| Standard-library operator functions | `operator_function` | Mutates trusted Python 3.14 `operator` callable references across comparison, arithmetic, bitwise, unary, truth, identity, in-place, and sequence operations; also covers `contains`, `getitem`, `setitem`, `delitem`, and `call` |
| Boundary operators | `structure_index_neighbor`, `structure_slice_neighbor` | adjacent plain-decimal index and slice-bound values, including unary-minus integers |
| Exception types | `exception_type_pair` | curated pairs such as `ValueError` ↔ `TypeError` in simple `except`/`except*` clauses and supported `raise` expressions |

Boundary operators recognize ASCII decimal digits with an optional unary minus,
including grouped or multiline spellings such as `items[(-1)]` and `items[-(1)]`.
`items[-1]` and `items[:-1]` produce `0` and `-2`; `items[::-1]` produces only
`-2`, because slice-step replacements never contain zero. `-2` produces `-1`
and `-3`. The magnitude must fit `u64` (at most `18446744073709551615`);
larger Python integers and neighbors outside that range are skipped. Unsigned
`0` retains its existing `1` candidate; `-0` produces `1` and `-1`. Hexadecimal,
underscored, float, unary-plus, repeated-sign and computed expressions are
excluded. Type annotations and store/delete subscripts remain excluded.

Boolean and complex-separator mapping-pattern key edits that would duplicate a
sibling literal key are excluded, including Python equality such as `True == 1`. Valid key,
value-pattern, and ordinary dictionary-expression mutations remain eligible.

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

Builtin call and exception pairs require both names to resolve to builtins.
PEP 695 type parameters can shadow either name in generic function and class
bodies, including nested closures and comprehensions. Function defaults and
decorators use the enclosing scope; generic class bases and keywords can see
the type parameters. Runtime mutations remain excluded from type positions.

Import-dependent type replacements are emitted only when their direct name or
module alias remains unshadowed at the annotation site. If no safe spelling is
available, the candidate is skipped.

The `operator_function` selector recognizes the documented callable pairs
`eq`/`ne`, `lt`/`le`, `gt`/`ge`, `add`/`sub`, `mul`/`truediv`,
`floordiv`/`mod`, `and_`/`or_`, `lshift`/`rshift`, `neg`/`pos`,
`not_`/`truth`, `is_`/`is_not`, `is_none`/`is_not_none`, `iadd`/`isub`, `imul`/`itruediv`,
`ifloordiv`/`imod`, `iand`/`ior`, `ilshift`/`irshift`,
`concat`/`iconcat`, and `countOf`/`indexOf`. It also maps `pow` and `matmul`
to `mul`; `xor` to `and_`; `abs` to `neg`; `index`, `inv`, and `invert` to
`pos`; `ipow` and `imatmul` to `imul`; `ixor` to `iand`; and `getitem` to
`contains`. The `contains` replacement reverses membership, while `setitem`,
`delitem`, and `call` evaluate their arguments but suppress the underlying
operation. Documented dunder aliases follow the same mappings. The unrelated
helpers `attrgetter`, `itemgetter`, `methodcaller`, and `length_hint` stay out of
scope, as do functions outside the documented Python 3.14 inventory.

Hoimin trusts only unique, unmodified, unconditional module-level
`import operator` and absolute `from operator import ...` bindings. It skips a
binding after shadowing, deletion, wildcard or conditional imports, dynamic
namespace access, module attribute writes, or an uncertain `__import__`
binding. Hoimin excludes relative and local imports. Stores, annotations, and
match patterns do not produce function candidates; match guards remain ordinary
expressions. Hoimin excludes every imported-alias load evaluated in a class
namespace because a custom metaclass can supply ordinary names without an AST
assignment. Method and lambda bodies remain eligible because they use
function/global lookup. An alias beginning with `__` stays excluded throughout
a class definition because private-name mangling and compiler-provided class
names can resolve it to another object. These checks operate within one module
and assume the normal standard-library `operator` module; they do not prove
anything about a custom import loader or external monkey-patching.

The exception selector `exception_risky` is opt-in only; enable it with
`--operators exception_risky` (or select individual IDs). It exposes
`exception_bare_to_exception`, `exception_exception_to_bare`,
`exception_base_boundary`, `exception_tuple_add_pair`, and
`exception_tuple_remove_member`. These mutations can broaden or narrow a
handler and the BaseException boundary can catch `SystemExit`,
`KeyboardInterrupt`, or `GeneratorExit`; none is enabled by default. Safe
exception pairs are limited to `ValueError`/`TypeError`, `KeyError`/`IndexError`,
`AttributeError`/`KeyError`, `FileNotFoundError`/`PermissionError`,
`ConnectionError`/`TimeoutError`, `ImportError`/`ModuleNotFoundError`, and
`ZeroDivisionError`/`OverflowError`. Qualified or dynamic handlers and tuple
members are skipped. Simple names in `except*` handlers use the same safe pairs
and resolution checks as ordinary handlers, while the five structural
`exception_risky` operators remain limited to ordinary `except` handlers. The
same safe pairs apply to the primary name in `raise ValueError`,
`raise ValueError(...)`, and
`raise ValueError(...) from cause`. Constructor arguments and the cause are
preserved. Bare re-raise, qualified or dynamic primary expressions, shadowed
source or replacement names, and termination exceptions are not changed.

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

The human `comparable score` and the JSON `previous_score`, `current_score`, and
`score_delta` fields use the intersection of common mutants with conclusive
results in both reports. Read the latest run summary for its whole-run mutation
score.

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

The sidecar uses the versioned [`run-metrics.schema.json`](docs/json-schema/run-metrics.schema.json) contract. Its `executed` count never exceeds `discovered` and equals the sum of per-worker `processes`. Metrics are operational observations: they do not affect resume compatibility and are not embedded in the run-result document. A metrics write failure warns without changing the mutation result. A confirmed collision with a selected source, explicit fingerprint input, session database, active SQLite companion or session ownership lock is rejected before the baseline. You can write metrics inside the project or replace an existing metrics file. A separate hardlink or final symlink can be replaced while preserving its protected referent.

Hoimin checks the entry that the final atomic rename replaces and writes to that resolved destination. If it cannot establish a safe destination identity, it skips metrics and emits a `metrics.write` warning without changing the mutation result. This includes unresolved filesystem case behavior and some prospective non-ASCII or Windows alias names.

The schema validates the document structure, schema version, and nonnegative integer values. The producer additionally guarantees unique stage names and workers in ascending worker-ID order; these semantic constraints are enforced by `RunMetrics::validate()` rather than JSON Schema.

Mutant statuses are:

- `killed`: the mutant test process exited nonzero;
- `survived`: it exited zero;
- `timeout`: its deadline elapsed;
- `out_of_memory`: the kernel reported an OOM kill in the supervised root cgroup subtree;
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

`--total-timeout` stops normal work at its configured deadline. Scheduler,
process, blocking-I/O, and internal-finalization waits then receive at most a
fixed additional two seconds for orderly shutdown. A timeout whose cleanup
completes inside that grace remains an incomplete run with exit code `4`. If
cleanup cannot finish before the shutdown grace expires, the run is an
infrastructure failure with exit code `2`. CLI report writes and flushes share
that shutdown deadline. Once the budget expires, the CLI can exit without a
final stderr diagnostic; stdout can contain a partial report if its consumer
stalls. The library's caller-provided, borrowed `Write` APIs retain synchronous
behavior: those writers cannot be forcibly cancelled and can delay return or
diagnostics.

## Sessions and resume

No database is created by default. `--session PATH` stores a run in SQLite and commits each mutant result independently. `--resume` requires `--session` and looks up the newest compatible incomplete run. Compatibility includes ordered import roots, source and configuration fingerprints, test argv, verdict-affecting limits, resource policy, and the operator set. Profile selection is part of session compatibility, so a focused run never resumes results from a full run and vice versa. `--jobs` and `--max-output` are operational settings and may change when resuming; reports record their current values, and reused results do not import output retained under the earlier limit. Completed `killed` and `survived` results can be reused; `timeout`, `out_of_memory`, `process_limit`, `error`, and `not_run` are run again under the current settings. An incompatible or already complete run is not silently mixed with new results.

Plan schema version 4 stores the independent import roots (ranking rule version
4). Regenerate older plans before verification. Fingerprint schema version 7
includes their ordered list, including an empty list for default invocations.
Older session fingerprints cannot be resumed; start a new session and rerun the
baseline and mutants. Existing saved results are not rewritten.

The active session database, its `-wal`, `-shm`, and `-journal` sidecars, and its `.<database-name>.hoimin-locks` directory are excluded from worker copies, copy-size accounting, and original-workspace integrity checks. Explicit `--include` patterns cannot restore these artifacts. Other database fixtures and similarly named files follow the normal copy rules and remain protected by integrity checks. Relative `--session` paths are resolved from the invoking working directory; existing database and parent-directory aliases resolve to the same active artifacts. Session ownership locking still prevents concurrent use of the same run.

## Build and verify a wheel

The package is a native binary wheel, not a Python extension module. Build and smoke-test the wheel locally with:

```console
uv run maturin build --release
uv run python tests/wheel_smoke.py
```

The smoke test installs the wheel into a new environment and runs the Rust-only CLI outside this checkout. For development verification, see [the development guide](docs/development.md). Start design and audit work with [the OKF catalog](docs/knowledge/index.md), and follow [the OKF workflow](docs/okf-workflow.md) to keep it current with each relevant change.

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
