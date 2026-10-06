# hoimin

hoimin is a bounded mutation-testing CLI for focused Python changes. Its Rust analyzer mutates copied source files, runs the test command once for the baseline and once per selected mutant, and emits versioned machine-readable results.

## Run it

`uvx` and `pipx run` install hoimin's native binary wheel in an isolated environment for a single invocation. Rust performs analysis in-process; the command after `--` is the test command for the project being checked.

```console
uvx hoimin run --root . --file src/calc.py --format json -- python -m pytest -q
```

### Live progress and debugging

When stderr is a terminal, `run`, `plan`, and `verify` show a progress line once
per second during execution, plus a final line. Each line shows the current stage,
completed mutant count, and elapsed wall time, including time spent waiting for
tests. For example:

```text
hoimin: running baseline | 0 completed | elapsed 2s
hoimin: testing mutants | 3 completed | elapsed 8s
hoimin: finished | 4 completed | elapsed 10s
```

Completed counts include reused results and exclude candidates that were not run.
`plan` discovers candidates without executing mutants, so its completed count is
zero. Redirecting stderr disables live progress; stdout keeps its selected report
format. The `hoimin progress` command compares saved reports and has no live display.

Enable diagnostic logs with `RUST_LOG`:

```console
RUST_LOG=hoimin_cli=debug hoimin run --root . --file src/calc.py --format json -- python -m pytest -q > result.json 2> debug.jsonl
```

Logs use text on terminal stderr and JSON Lines on redirected stderr. With logging
enabled, redirected stderr can contain both tracing records and existing diagnostic
events; tracing records do not use the versioned run-event schema. Filter by module,
such as `RUST_LOG=hoimin_cli::process=debug`, to inspect process execution. Run,
worker, and mutant identifiers connect logs from concurrent test processes.

Logging is off by default. `RUST_LOG=off`, an empty value, or an invalid filter
disables it without disabling terminal progress. Library callers choose their own
tracing subscriber. Diagnostic output uses a bounded queue: a slow sink can lose
log records, and shutdown waits at most 250 ms to flush. These logs supplement
the run reports; use the reports for complete mutation results.

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
Before running or previewing a batch, verification checks every retained
candidate's saved ranking against the plan's rules and resolved selectors.
This check borrows candidate bodies instead of copying all original/replacement text.

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
- Each explicit `--source` must exist. Missing sources fail with exit 2 before baseline execution in `run`, `plan`, and saved-plan `verify`, including when combined with valid sources or `--changed`. Existing empty directories and valid selections filtered to zero candidates still succeed. A regular Python file is also accepted as a source. This existence check does not change the discovery policy for symbolic links.
- `--file PATH` selects an entire Python file and may be repeated.
- `--line PATH:START-END` selects an inclusive line range and may be repeated.
- `--symbol MODULE:QUALNAME` selects a function, method, or class resolved below `--source` and may be repeated.
- `--changed` restricts selection to staged, unstaged, and untracked Git changes. It requires `--source`.
  Git change ranges are mapped to Python physical lines (LF, CRLF, or CR) before intersection with `--line`. A Git LF-delimited line can contain several CR-delimited Python lines; original source bytes are preserved.
  For ordinary tracked changes, Git patch output is restricted to resolved targets in bounded literal-path batches. A selected rename or copy keeps the full rename-aware diff to preserve Git's original pairing and line ranges; the metadata inventory still covers the repository.
- `--changed-context N` includes up to N neighboring lines on each side of Git changes. It requires `--changed`, defaults to 0, and accepts integers from 0 through 1073741823. With N > 0, a pure deletion selects up to N surviving lines on each side of the deletion boundary. Ranges are clipped at the current file boundaries; empty and deleted files contribute no lines. Untracked files still select their complete contents.
- `--diff-base REV` uses the merge base of `REV` and `HEAD` for `--changed`. It is invalid without `--changed`.

An explicit `--symbol` must name an existing function or class definition in the
resolved Python file, including methods, nested definitions and package
`__init__.py` definitions. A missing definition is an exit-2 error before the
baseline in `run`, `plan` and `verify`, even with `--changed` and no changed
lines. Existing definitions with no candidates under the selected operators,
profile, lines or Git diff remain valid empty selections. Imported names and
assignment aliases do not count as definitions.

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

`--root DIR` resolves relative paths and defaults to the current directory. Combining explicit selectors with `--changed` intersects each explicit target with changed lines and any `--changed-context` neighborhood. Context never expands an explicit line or symbol selector. When that target also has a symbol selector, a candidate must be both in that Git selection and inside the selected symbol. The `changed_line` ranking reason and its 200-point boost apply to this entire selection, including context lines; the reason does not assert that the candidate itself was edited. Plans save the context setting, and `verify` reuses it. A `--symbol` requires `--source`; when `--source` is present, file and line paths must be inside a source root.

Target, fingerprint, and copied-workspace paths use a portable `/`-separated representation. Native Windows path inputs are normalized to that form. On Unix, a concrete filename containing a literal backslash is rejected before collection because it cannot be represented unambiguously. Backslashes in glob options retain their existing escape syntax; the concrete paths matched by a glob are validated after walking.

Copied workspace files and directories may contain `:` on Linux and macOS, including fixtures such as `.dockerfiles/appconfig/app:env:conf-sample`. These files participate in copying, worker reset, and original-file integrity checks. Windows workspace paths still reject `:` to prevent drive-prefix and alternate-data-stream interpretation. Mutation candidate and fingerprint paths retain their stricter portable-path restrictions. If a fixture is unnecessary for tests, omit it explicitly with `--exclude '.dockerfiles/**'`.

`--include GLOB` can restore ignored or hidden files. `--exclude GLOB` adds exclusions and wins when both match. Both options may be repeated. Exclude patterns are interpreted only as globs: for example, `src/[ab].py` excludes `a.py` and `b.py`, not a file literally named `[ab].py`.

Target discovery and worker copying always exclude `.git`, `.venv`, `venv`, `env`, `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.pyre`, `.pytype`, `.tox`, and `.nox` below the project root. Names are compared case-insensitively on Windows and exactly on other platforms. Includes cannot restore these entries. Automatic source scans omit them; explicit `--file` and `--line` selectors inside them fail during target resolution with the path and remediation. Choose source outside the excluded directory. The project root itself remains usable even if its name is on this list. Ignore files apply in non-Git roots too; hidden mutation targets still require an include.

`--fingerprint-file PATH` records exactly one regular file at the specified `--root`-relative path and may be repeated. Every path component must remain beneath `--root`; symlink and reparse-point components are rejected instead of followed. It does not search nested directories, and characters such as `*`, `?`, and `[` are treated literally. Use it for a root-level configuration file without also selecting files with the same name in nested worktrees.

`--fingerprint-include GLOB` records every matching file as an explicit session-fingerprint input. A basename-only glob such as `pyproject.toml` can match that name at any depth. Both fingerprint options only invalidate compatible-run reuse: they do not control worker copying, select mutation targets, or implicitly watch files. Use `--include GLOB` independently when a fingerprinted input must also be copied into each worker; no files are implicitly watched.

`--fingerprint-env NAME` tracks an inherited environment variable for resume compatibility
and may be repeated in `run` or `plan`. Names must match
`[A-Za-z_][A-Za-z0-9_]*`: Unix names are case-sensitive; Windows names normalize
to uppercase. Name order and duplicate declarations do not matter. A selected
variable becoming unset, empty, or a different value changes compatibility;
unselected variables retain their existing behavior. Values are hashed using
native Unix bytes or Windows UTF-16 code units without lossy Unicode conversion.

Capture occurs during command preparation, before worker `PYTHONPATH` and
metadata rewriting. Only the selected names and a digest enter normalized config,
reports, sessions, and plan manifests; this option does not add captured values
in plaintext. Hashing does not guarantee secrecy for low-entropy values.
`verify` reads the names saved by `plan` and rejects a changed inherited snapshot
with `plan.fingerprint_env.changed` before its baseline runs. Regenerate the plan
after an intentional environment change. Concurrent environment mutation by an
embedding caller is outside this snapshot contract.

For example, when a test needs an ignored fixture input copied into its worker:

```console
hoimin run --root . --source src --fingerprint-include tests/fixtures/settings.toml --include tests/fixtures/settings.toml -- python -m pytest -q
```

Each candidate must fit a 2 MiB compact JSON spool record, including JSON escaping, UTF-8 and one trailing newline. `plan` rejects oversized candidates instead of saving an unusable plan; `verify` rejects oversized saved records before baseline. Direct `run` discovers candidates after baseline and reports an incomplete infrastructure failure if a record exceeds this limit. The diagnostic identifies the source path, line, operator and limit. Candidate-count limits do not override this byte limit.

## Python source encodings

Hoimin recognizes Python encoding declarations in a standalone comment on the
first line, or on the second line after a blank/comment-only first line.
Without a declaration it uses UTF-8. It supports these codecs in Rust:

| Codec | Accepted names (case-insensitive; underscores may replace hyphens) |
| --- | --- |
| UTF-8 | `utf-8`, `utf8` |
| ASCII | `ascii`, `us-ascii`, `646` |
| Latin-1 / ISO-8859-1 | `latin-1`, `latin1`, `iso-8859-1`, `iso8859-1`, `iso-latin-1`, `latin`, `l1`, `cp819`, `ibm819` |

Python's tokenizer also normalizes names beginning with `utf-8-`, `latin-1-`,
`iso-8859-1-` or `iso-latin-1-` to those codecs. A leading UTF-8 BOM is preserved
in file bytes and requires a tokenizer-normalized UTF-8 declaration: use
`utf-8` rather than the generic `utf8` alias with a BOM. Declarations in strings,
after a statement, or after the second line are ignored. A matching `coding:`
inside an otherwise ordinary standalone comment still declares an encoding.

Unsupported or unknown names, BOM conflicts and invalid UTF-8/ASCII bytes
produce a diagnostic containing the path and declaration. Latin-1 maps every
byte to a character, so it cannot detect that a file was intended to use a
different encoding. Save each file using its declared codec.

Analysis uses Unicode text; candidate spans and file hashes refer to original
bytes. Mutation text is encoded back into the same codec, preserving untouched
bytes, BOM and line endings. A replacement that cannot be encoded is rejected.
`plan` decodes without running a baseline. Normal `run` retains its baseline
before analysis, so a baseline failure may precede the encoding diagnostic;
explicit symbol validation decodes earlier during target resolution. No Python
loader or external Python process is used for production source decoding.

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
| `--fingerprint-env NAME` | none | fingerprints an explicitly selected inherited variable; repeatable |

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

hoimin copies regular files and selected directories, including empty fixture
directories, into isolated workers. Removed directories are restored between
mutants, including directories replaced by files or links. Directory selection
uses the same ignore, include, exclude, default-protection and session-artifact
rules as file copying. A children-only exclusion such as `fixtures/**` can leave
the selected parent `fixtures` empty; exclude `fixtures` itself to remove the
whole tree. Exact directory permissions are not copied, and directories do not
add to logical file-byte budgets. Hoimin does not follow or copy symlinks; each
skipped symlink produces a diagnostic. Original integrity checks include selected
directory presence as well as file contents.

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
opt-in. Hoimin exposes 57 operator IDs in total: 43 default runtime IDs, seven
opt-in type IDs, five opt-in risky exception IDs, `exception_hierarchy`, and
`statement_delete`.

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

User-defined exceptions have a separate, explicit-only operator:

```console
hoimin plan --file src/service.py --import-root src --operators exception_hierarchy -- pytest -q
```

`exception_hierarchy` reads project Python class definitions and explicit imports,
then replaces a visible exception with its direct user-defined parent, child, or
sibling. It supports `except`, `except*`, and `raise`; raised-exception replacements
require both classes to inherit `Exception`'s constructor without a custom
`__init__` or `__new__`. For example, `MissingError(AppError)` and
`ConflictError(AppError)` can be exchanged when both names are visible.
This operator can broaden handlers and is not included in the default selection,
`exception_ops`, or `exception_risky`.

Analysis does not import project modules. Dynamic or ambiguous definitions,
re-exports, multiple inheritance, termination exceptions, and exception groups are
skipped. Imports using top-level standard-library module names are conservatively
excluded, even when a project contains a file with that name. Definitions must precede the containing function or the module-level use;
class bodies are skipped, while method bodies use module bindings. No imports are
inserted. A per-file diagnostic reports incomplete analysis with its reason and
first location. Valid exclusions, such as having no related class or an incompatible
constructor, do not produce this diagnostic. All allowed project Python files, including unselected files, become
fingerprint inputs for this operator, so dependency additions, removals and changes
invalidate saved plans and resumed results. Function/container aliases that the
analysis does not track can still change a class binding, even within indexed
project files; such changes are not guaranteed to suppress affected candidates.
The operator does not guarantee that every replacement denotes an exception class
at runtime. See [the implementation boundaries](docs/development.md#user-defined-exception-hierarchies).

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
Class bodies with bases or keyword arguments may use a metaclass-prepared
namespace, so class-visible builtin names are treated as uncertain. This also
applies to builtin names in class-visible type annotations. Bare classes and
empty `()` headers retain ordinary lookup; methods and closures skip the class
namespace, and explicit `global` declarations bypass it per name. Even known
`object` bases or `metaclass=type` are conservatively excluded at class-visible
sites.
PEP 695 type parameters can shadow either name in generic function and class
bodies, including nested closures and comprehensions. Function defaults and
decorators use the enclosing scope; generic class bases and keywords can see
the type parameters. Runtime mutations remain excluded from type positions.

Import-dependent type replacements are emitted only when their direct name or
module alias remains unshadowed at the annotation site. If no safe spelling is
available, the candidate is skipped. Class-visible annotation imports and module
aliases are also excluded when the class may have a prepared namespace, including
explicit class imports and `nonlocal` references. Lexical descendants skip the
class namespace; explicit `global` declarations bypass it for each declared name. Private
import aliases and their mangled spelling (such as `__Seq` and `_C__Seq` inside
class `C`) are conservatively excluded at both endpoints, including methods and
nested functions that retain that compiler context. Leading underscores in class
names are stripped; trailing-dunder aliases and underscore-only class names do
not trigger mangling. Actual private writes to an explicitly mangled import name
also invalidate its provenance in the destination namespace. Clean private
aliases may therefore produce fewer candidates; ordinary aliases retain their
existing behavior.

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

Repeated input paths and byte-identical copies produce warnings on stderr, in
both human and JSON output modes. Warnings identify the repeated input and an
earlier input, with `same path` or `identical bytes` as the reason. Inputs remain
in order: self-comparisons still count as stalls and can reach saturation, and
unusable reports still break the stall chain. No strict rejection is applied.
Different reports that share a run ID (for example, before and after resume)
are not duplicates on that basis. Copy detection compares raw bytes, so the
same execution saved as JSON and JSONL, or with different whitespace, is not
detected as a copy. Copy confirmation is best-effort if files change or become
unreadable after validation; reports should remain unchanged during comparison.

Inputs may mix JSON documents (schema v2/v3/v4) and JSONL event streams
(schema v3/v4) saved from `hoimin run --format jsonl`. Detection uses content, not
filename. For example:

```console
hoimin progress --format json reports/before.json reports/after.jsonl
```

JSONL is read one event per nonblank line. `run_started` must come first and
`run_finished` must be present and last. `mutant_started` must precede the
matching `mutant_finished`; diagnostic events are validated and then discarded.
Run IDs, event sequence, baseline order, mutant identities, results and summary
counts must be consistent. Truncated lines, missing completion, mixed runs and
duplicate events are rejected with exit code 2. A valid stream whose completed
summary marks the run incomplete, or whose baseline failed, is unusable for
comparison just like its JSON document. CRLF and a complete final line without
a newline are accepted. Legacy schema v2 JSONL is unsupported.

The JSONL reader retains comparison candidates and one reusable event buffer;
it does not retain the diagnostic/start-event history or the entire input
string. Its memory use still depends on candidate data and the largest event.


```console
hoimin progress --patience 3 reports/before.json reports/after.json reports/latest.json
hoimin progress --format json reports/*.json
```

Comparisons require adjacent reports that are both complete and have successful baselines. `saturated` means the configured number of consecutive comparable stalls was reached. Only immediately adjacent stalled comparisons contribute to this count. An improving, regressing, or indeterminate comparison resets the consecutive stall chain.

By default, valid progress comparisons exit with code 0. Add `--fail-on-regression` to exit with code 1 only when `latest.state` is `regressing`; `indeterminate` still exits 0. The gate uses the final adjacent pair, so a historical regression followed by improvement or an unusable report does not fail the gate. Input, validation, and output failures retain exit code 2. The command writes its complete human or JSON result before returning the regression exit code. JSON output exposes `latest.state` for callers that need a different policy. A surviving mutant is not proof of behavioral equivalence.

The human `comparable score` and the JSON `previous_score`, `current_score`, and
`score_delta` fields use the intersection of common mutants with conclusive
results in both reports. Read the latest run summary for its whole-run mutation
score.

For split verification, choose stable candidate batches and keep a separate oldest-to-newest
report history for each batch. `--offset K --top N` skips K entries in the complete
selected-policy ordering, then selects up to N candidates:

```console
hoimin plan --root . --source src --max-mutants 100 -- python -m pytest -q > plan.json
mkdir -p reports
hoimin verify plan.json --top 100 --offset 0 > reports/batch-a-001.json
hoimin verify plan.json --top 20 --offset 100 > reports/batch-b-001.json
# After improving tests, repeat the same ranges from the same plan.
hoimin verify plan.json --top 100 --offset 0 > reports/batch-a-002.json
hoimin verify plan.json --top 20 --offset 100 > reports/batch-b-002.json
hoimin progress reports/batch-a-001.json reports/batch-a-002.json
hoimin progress reports/batch-b-001.json reports/batch-b-002.json
```

This example partitions a plan with 120 retained candidates. Keep the same
`--selection-policy` for its batches: `strict` uses saved rank order; `diverse`
uses the complete equal-score file-round-robin order before slicing. Changing
the policy between batches can change membership. The JSON/JSONL mutant records
retain the actual selected candidate IDs; save the range/policy commands with
those reports for reruns.

Preview a batch before running its tests:

```console
hoimin verify plan.json --top 20 --offset 100 --selection-policy diverse --dry-run > preview.json
```

`--dry-run` performs the same plan, source, fingerprint and candidate validation
as verification, then exits without baseline or mutation tests, worker copies,
sessions or execution metrics. It conflicts with `--metrics`. A valid preview
exits with code 0, including for a truncated plan; invalid selections and stale
plans exit with code 2. Runtime resource availability is checked when executing.

When saved source or fingerprint input records differ from the workspace,
verification reports `modified`, `added` or `removed` with quoted root-relative
paths. Normal verification and dry-run show the same diagnostic before tests
run, with empty stdout and exit code 2. Details are sorted by path, limited to
ten paths, and followed by the number of additional paths omitted. Control
characters in paths are escaped. Earlier discovery or read errors retain their
own diagnostic rather than being classified as record differences.

The [preview schema](docs/json-schema/verify-preview.schema.json) is independent
of run reports: `kind` is `verify_preview` and `schema_version` is 2. JSON and
JSONL each contain one object; `--format human` prints metadata and candidate rows.
The ordered `candidates` array gives each candidate's `id`, saved `rank`,
`selection_order`, `path`, `line`, `column`, `operator`, `original` and `replacement`.
Details come from the same validated plan candidate. Rank, line and batch selection
order start at 1; column is the plan's 0-based Python source column. Human rows
show `path:line:column operator=NAME "original" -> "replacement"`; mutation text
is quoted and escaped so newlines, tabs, quotes and backslashes stay within one row.

The closed version-1 preview schema does not accept these added fields. Clients
that accept only preview version 1 must update to version 2; the CLI emits version 2
without a legacy-output option. Saved plan and run-report schema versions are unchanged.

`verification_selection` records mode, policy, requested/selected counts,
scope and `plan_truncated`; `offset` starts at 0 for top selection and is null
for explicit IDs. `retained_candidates` is the count saved in the plan.

Top selection follows the chosen strict/diverse order. Explicit `--candidate`
selection follows discovery order, with duplicates removed. Selection order
specifies the intended scheduling order; parallel completion and early runtime
stops may differ. A truncated preview covers only retained candidates. Save
preview JSON separately from run reports: it contains no mutation outcomes and
cannot be used as `progress` history.

Offset is zero-based, requires `--top`, and cannot accompany `--candidate`.
An empty plan or an offset at/beyond its retained length fails before baseline.
A range extending beyond the end selects the available suffix. Truncated plans
permit only retained ranges and remain incomplete. Each batch inherits the saved
limits, including `max-mutants`; skipped entries do not consume that limit.
Changing selected source or fingerprint inputs still requires a new plan.

After every test improvement, rerun every stable batch and save each report separately.
Pass `hoimin progress` only reports covering the identical candidate-ID set.
The command marks a comparison `indeterminate`, resets its comparable stall
chain, and writes a warning when adjacent candidate-ID sets differ or contain
duplicates. If batch membership changes, start a new history.
Reports from different subsets, their
scores, and their saturation states must not be combined into a synthetic whole-plan result.
Judge overall completion from each batch's latest complete report for the same current test revision,
accounting for the union of candidate IDs selected from the plan.

When a baseline fails or reaches its process timeout, `run` and `verify` copy its retained combined stdout/stderr to stderr before execution cleanup. Redirect stderr to save these diagnostics, for example `hoimin run ... --format json > result.json 2> baseline.log`. Machine-readable formats keep stdout unchanged and encode stderr diagnostics as JSON Lines (`baseline.output`). The header records retained/observed byte counts and truncation; `--max-output` limits the retained tail. Output is streamed in 16 KiB chunks, preserving split UTF-8 characters; invalid bytes become U+FFFD and terminal control characters other than newline/tab are escaped. Chunk offsets refer to retained raw bytes. Successful baselines stay quiet. Read failures produce `baseline.output.read`; cancellation or a blocked diagnostic sink can stop export at the existing total timeout and shutdown grace, so interrupted export may be partial.

`--format json` emits one document. `--format jsonl` emits flushed lifecycle events; diagnostics are JSON Lines on stderr. Public JSON contracts are versioned in [`run-result.schema.json`](docs/json-schema/run-result.schema.json) and [`run-event.schema.json`](docs/json-schema/run-event.schema.json). Event kinds are `run_started`, `baseline_finished`, `mutant_started`, `mutant_finished`, `diagnostic`, and `run_finished`. Parallel events are emitted in completion order; candidate sequence numbers allow stable reordering.

The final summary's `complete` is `false` when any mutant is inconclusive or the run fails or is interrupted. It is `true` only when every selected mutant is `killed` or `survived` and no run-level failure occurred; a successful run with no candidates is also complete. Therefore, an exit code of `4` always has `complete: false`.

Run `hoimin progress --details before.json after.json` to identify improvements and
regressions in the **final adjacent input pair**. Details include candidate ID,
operator, previous/current status and source positions (one-based lines, zero-based
columns). Human output quotes and escapes strings. `--details-limit N` sets the
maximum displayed changes (default 100; zero shows only omission counts), ordered
lexicographically by candidate ID.

The detail fields `previous_input` and `current_input` are zero-based indices into
the original inputs. An unusable final pair yields `available: false`; an earlier
comparison is never substituted. `eligibility` states whether the candidate sets
match, differ, or contain duplicate IDs. Only changes counted by the comparison
with an identical ID unique in both inputs appear as details. Inconclusive and
ambiguous matches are excluded. `omitted` counts identified changes beyond the
limit; `unidentified` counts aggregate changes whose identity cannot be established,
such as content-only matches with different IDs. All aggregate counts and decisions
remain independent of the display limit.

`--details --format json` explicitly selects
[progress schema v2](docs/json-schema/progress-result-v2.schema.json), which adds a
`details` object. Without `--details`, human output,
[progress schema v1](docs/json-schema/progress-result.schema.json), and exit status
retain their existing behavior.

### Operational metrics

Run metrics are an opt-in operational sidecar, separate from the run JSON. Write them by passing a destination to `--metrics`:

```console
hoimin run --root . --source src --metrics metrics.json -- python -m pytest -q
hoimin verify plan.json --top 20 --metrics batch-metrics.json > batch-report.json
hoimin verify plan.json --candidate m1_ID --metrics candidate-metrics.json > candidate-report.json
```

`verify --metrics` supports explicit candidate IDs and both strict and diverse top selections. Relative metrics paths use the directory where you invoke hoimin, independently of the saved project root. The destination is not stored in the plan and does not change candidate selection or execution limits. Metrics begin when shell execution starts; plan validation and rediscovery time are not included. A failure during plan preparation leaves the destination untouched. Once execution starts, failed baselines and interrupted runs follow the same sidecar and warning rules as `run`.

The sidecar uses the versioned [`run-metrics.schema.json`](docs/json-schema/run-metrics.schema.json) contract. Its `executed` count never exceeds `discovered` and equals the sum of per-worker `processes`. Metrics are operational observations: they do not affect resume compatibility and are not embedded in the run-result document. A metrics write failure warns without changing the mutation result. A confirmed collision with the verify input manifest, a selected source, explicit fingerprint input, session database, active SQLite companion or session ownership lock is rejected before the baseline. You can write metrics inside the project or replace an existing metrics file. For a verify input specified through a symlink, both the specified entry and its resolved file are protected. A separate hardlink or final symlink can be replaced while preserving its protected referent.

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

Run JSON/JSONL now emit schema 4; consumers validating the closed schema must update. `progress` continues to read historical schema-2 JSON and schema-3 JSON/JSONL, and rejects mixed-version reports. When `--resume` is requested, JSON and JSONL include `run_started.resume` (the `run.resume` field in a JSON report): `{"status":"resumed"}` or `{"status":"fresh","reason":"CODE"}`. Human output also explains whether the saved run is continuing or a new run is starting. Runs without `--resume` omit this field.

A compatible incomplete run always wins, even if newer runs are incompatible. If an eligible row appears only during the later history inspection, `candidate_changed` takes precedence. Otherwise reasons use this precedence: `budget_decreased` (matching incomplete runs require a higher `--max-mutants`), `matching_run_complete`, `fingerprint_mismatch` (other incomplete history), `no_incomplete_run` (only nonmatching complete history), then `no_prior_run`. `candidate_changed` means session history changed during resume selection; `no_compatible_run` is a fallback for older completion events without diagnostic details. These codes describe observed history, not which configuration field or file changed. Existing old-schema, corrupt-data, database-read, and ownership errors remain errors.


No database is created by default. `--session PATH` stores a run in SQLite and commits each mutant result independently. `--resume` requires `--session` and looks up the newest compatible incomplete run. Compatibility includes ordered import roots and source roots, source and configuration fingerprints, test argv, verdict-affecting limits, resource policy, and the operator set. Profile selection is part of session compatibility, so a focused run never resumes results from a full run and vice versa. `--jobs` and `--max-output` are operational settings and may change when resuming; reports record their current values, and reused results do not import output retained under the earlier limit. Completed `killed` and `survived` results can be reused; `timeout`, `out_of_memory`, `process_limit`, `error`, and `not_run` are run again under the current settings. An incompatible or already complete run is not silently mixed with new results.

`--max-mutants` may stay the same or increase when resuming an incomplete run.
The limit counts reused results as well as newly executed mutants: increasing
1 to 3 can reuse the first result and execute the next two. The database records
the latest accepted limit, and a lower limit cannot resume that run. A baseline
still runs on every invocation. Other verdict-affecting limits must remain compatible.

SQLite session schema 4 preserves older rows but does not invent their missing
historical budget. Start a new session run when an old incomplete run reports
`session.resume.incompatible`; completed results remain available in the database.

Plan schema version 5 stores explicit environment names/digest and independent
import roots (ranking rule version 4). Regenerate older plans before verification.
Fingerprint schema version 11
includes ordered import roots and source roots and the ordered `--include` / `--exclude` copy
patterns, including empty lists for default invocations. Changing source-root order or either copy
pattern list starts a new run even when explicit fingerprint-file bytes match.
Pattern spelling and order are preserved; equivalent but differently spelled
patterns may conservatively start a new run. Identical patterns remain compatible.
If the latest incomplete run uses an older fingerprint schema and no compatible
run exists, `--resume` retains the `session.resume.incompatible` error. Start a
new run without `--resume`, or use a new session path; existing saved results are
not rewritten.

The active session database, its `-wal`, `-shm`, and `-journal` sidecars, and its `.<database-name>.hoimin-locks` directory are excluded from worker copies, copy-size accounting, and original-workspace integrity checks. Explicit `--include` patterns cannot restore these artifacts. Other database fixtures and similarly named files follow the normal copy rules and remain protected by integrity checks. Relative `--session` paths are resolved from the invoking working directory; existing database and parent-directory aliases resolve to the same active artifacts. Session ownership locking still prevents concurrent use of the same run.

## Build and verify a wheel

The package is a native binary wheel, not a Python extension module. Build and smoke-test the wheel locally with:

```console
uv run maturin build --release
uv sync --frozen --no-install-project
uv run --frozen --no-sync python tests/wheel_smoke.py
```

The smoke test installs the wheel into a new environment and runs the Rust-only CLI outside this checkout. For development verification, see [the development guide](docs/development.md). Start design and audit work with [the OKF catalog](docs/knowledge/index.md), and follow [the OKF workflow](docs/okf-workflow.md) to keep it current with each relevant change.

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run maturin build --release
uv sync --frozen --no-install-project
uv run --frozen --no-sync python tests/wheel_smoke.py
```

Windows Job Object tests run on Windows and Linux hard-limit tests require a delegated cgroup v2 runner. The ordinary Linux CI job verifies the explicit best-effort path separately.

## GitHub Releases

Merging a pull request into `main` starts `.github/workflows/release.yml`.
It reserves a `vMAJOR.MINOR.PATCH` tag on the merged commit, builds and checks
all three platforms, then publishes a [GitHub Release](https://github.com/tokyogas-tech/hoimin/releases)
with generated release notes and `SHA256SUMS`:

| Platform | Standalone executable archive | Python wheel |
| --- | --- | --- |
| Windows x86_64 | `hoimin-v<VERSION>-windows-x86_64.zip` | `win_amd64` |
| Linux x86_64 | `hoimin-v<VERSION>-linux-x86_64.tar.gz` | manylinux2014 |
| macOS Apple Silicon | `hoimin-v<VERSION>-macos-aarch64.tar.gz` | macOS arm64 |

Extract the archive for your platform and put `hoimin` (or `hoimin.exe`) on
`PATH`. The standalone Linux executable is built on Ubuntu 22.04; the wheel
uses manylinux2014 for broader glibc compatibility. Standalone executables
do not need Python to show help or analyze source; running Python tests still
requires the target project's Python environment. Wheels require Python 3.14.
Install a downloaded compatible wheel with `uv tool install ./<WHEEL>.whl`.
Access to release downloads follows this repository's visibility.

The first release uses the workspace version (currently `0.1.0`). Later
merges increment the highest stable tag's patch version. Raising the workspace
version can establish a higher minimum version for the next release; keep
`Cargo.toml`, `pyproject.toml`, `Cargo.lock`, and `uv.lock` consistent when
changing it. CI embeds the reserved version into those four files in its
build checkout, without committing version changes back to `main`.

Before reserving a new tag, CI checks that the commit descends from the
highest stable tag's commit, fetching complete history when needed. A delayed
run for an older or divergent commit is skipped without a tag, build, or
release. Each reservation atomically creates the version tag and advances the
`hoimin-release-state` branch to that commit, conditional on the previously
observed branch SHA. A competing reservation rejects the entire push and forces
a fresh history check, even if the runs chose different version numbers.
Existing tags remain reusable, so retries of an older, already tagged release
still work.

The workflow creates `hoimin-release-state` on the first new reservation. Reserve
that branch for automation: do not delete, rewind, or push application changes
to it. Repository rules must allow the workflow to create/update this branch
and create version tags. Version tags are never overwritten; if either update
is rejected, the atomic push writes neither ref.
All concurrent automated reservations must use this protocol; manually created
tags or runs of an older workflow do not participate in its shared lease.

PRs and manual runs build preview packages (`-dev.<RUN_ID>`) and retain them
as Actions artifacts. They do not create tags or releases. For a manual check:

```console
gh workflow run release.yml --ref <BRANCH>
```

If a merged-PR run fails, rerun that Actions run. It reuses the tag for the
same commit, resumes a draft release, and leaves an already published release
unchanged. All three builds and wheel smoke tests must succeed before
publication. Manually pushing a tag does not trigger this workflow.
Publication explicitly uses GitHub's `make_latest=legacy` selection by version
and date, so a delayed older release does not become `Latest` merely by finishing
last. There is no separate client-side read/compare/update of `Latest` that could
race with another run. The GitHub API owns that selection; local tests check the
outgoing request, while hosted execution remains the integration check.

PyPI publication is not enabled. Before enabling it, add a separate manually
triggered workflow, protect its GitHub environment with required reviewers
or equivalent rules, and register PyPI Trusted Publishing for only that
workflow and environment. Grant `id-token: write` only to its publication job,
and update the workflow contract tests in the same change.

### Opt-in call statement deletion

`--operators statement_delete` replaces an independent call statement (for example
`store.persist(record)`) with `pass`. It removes evaluation of the callee and all
arguments. It is disabled by default. Assignment, return, import, compound
statements, and calls containing assignment expressions, `await`, `yield` or
`yield from` (including nested lambda bodies) are excluded. An empty suite remains
valid, and neighbouring statements and trailing comments are preserved. The usual
profile, line/symbol/changed selection, limits and cancellation still apply.
A surviving deletion means the missing effect deserves inspection; it does not
prove a test defect or that the mutation is non-equivalent.
