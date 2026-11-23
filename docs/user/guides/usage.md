# Usage reference

See the [README](../../../README.md) for installation and a quick start.
The [generated CLI reference](../reference/cli.md) lists public commands, arguments,
help, defaults and reflected Clap constraints. This guide covers examples and
runtime contracts, including OS-specific limits and plan/verify inheritance.
For the current-format policy and changed defaults, see the
[change policy](../reference/compatibility.md).
For questions and bug reports, see the [reporting guide](../about/support.md).

- [Run it](#run-it)
- [Plan and verify with an agent](#plan-and-verify-with-an-agent)
- [Live progress and debugging](#live-progress-and-debugging)
- [Select mutation targets](#select-mutation-targets)
- [Python source encodings](#python-source-encodings)
- [Limits and defaults](#limits-and-defaults)
- [Disk-safe execution](#disk-safe-execution)
- [Mutation operators](#mutation-operators)
- [Mutation profiles](#mutation-profiles)
- [Results](#results)
- [Human review of mutation candidates](#human-review-of-mutation-candidates)
- [Sessions and resume](#sessions-and-resume)
- [Additional runtime mutations](#additional-runtime-mutations)

## Run it

Install hoimin using the [README](../../../README.md#install). Run commands from your
project directory with an environment whose `python` has your test dependencies.
Run the project's tests first and resolve failures before mutation testing.
Rust performs analysis in-process; everything after `--` is the project's test
command, passed directly as arguments without a shell.

### Before execution

Choose the resource policy before `run` or `plan`:

| Platform | Initial policy |
| --- | --- |
| Windows | Keep the default Job Object hard limits |
| Linux with delegated cgroup v2 | Keep the default cgroup hard limits |
| macOS | Explicitly accept `--allow-best-effort-memory`; memory is not capped |
| Linux without delegated cgroup v2 | Explicitly accept `--allow-best-effort-memory`; fallback address-space limits apply per process, not to the whole tree |

Use the commands as written on Windows or hard-limit Linux. On macOS or fallback
Linux, add `--allow-best-effort-memory` **before `--`** to the `run` or `plan`
command only if that policy is acceptable. `verify` inherits the saved policy
and does not accept this flag. See [limits and defaults](#limits-and-defaults)
for enforcement scope and cleanup limitations. A limit or cleanup failure is a
reason to stop and diagnose, not to disable a guard or retry indefinitely.

Single-line commands work in Bash and PowerShell 7. Shell-specific examples
are labeled. Use PowerShell 7 for JSON redirection; legacy Windows PowerShell
can write UTF-16 output that hoimin cannot read as a saved JSON plan.

For a direct run, replace `src/calc.py` with an existing target file:

```console
hoimin run --root . --file src/calc.py -- python -m pytest -q
```

### Plan and verify with an agent

This is hoimin's primary workflow: let an external AI agent investigate a small
batch of survivors and propose specification-backed tests, with humans resolving
ambiguous requirements and reviewing changes. See the [product principles](../about/product-principles.md).
Inspect the baseline and resource policy before starting the loop; stop on resource
or cleanup failures instead of silently raising limits or repeating the run.

This example uses a file rather than Git changes. You can follow it in an empty
directory without initializing Git. Use an environment with pytest installed and
apply the [execution prerequisites](#before-execution) before planning.

1. Create `calc.py`. Its required behavior is to add two numbers:

    ```python
    def add(left, right):
        return left + right
    ```

2. Create `test_calc.py` with a passing but incomplete test:

    ```python
    from calc import add


    def test_add_zero():
        result = add(0, 0)

        assert result == 0
    ```

3. Confirm that the original tests pass:

    ```console
    python -m pytest -q
    ```

    Expect one passing test. Resolve a collection, import or test failure before
    proceeding; a failed baseline cannot tell you whether a mutant was detected.

4. Create `reports/` and a plan selecting only addition/subtraction replacement.
    On Windows or delegated-cgroup Linux, use the command for your shell:

    **Bash:**

    ```bash
    mkdir -p reports
    hoimin plan --root . --file calc.py --operators binary_add_sub \
      --jobs 1 --total-timeout 1m \
      -- python -m pytest -q > PLAN.json
    ```

    **PowerShell 7:**

    ```powershell
    New-Item -ItemType Directory -Force reports | Out-Null
    hoimin plan --root . --file calc.py --operators binary_add_sub `
      --jobs 1 --total-timeout 1m `
      -- python -m pytest -q > PLAN.json
    ```

    On macOS or fallback Linux, use this Bash plan command instead, after creating
    `reports/`, only if you accept best-effort memory control:

    ```bash
    hoimin plan --root . --file calc.py --operators binary_add_sub \
      --jobs 1 --total-timeout 1m --allow-best-effort-memory \
      -- python -m pytest -q > PLAN.json
    ```

    The plan should retain one candidate, replacing `+` with `-`. `plan` does not
    run tests. For a different project, inspect the plan before verification. If
    it retains no candidates, check the file and operator selection; `verify
    --top` rejects an empty selection. If discovery is truncated or fails, narrow
    the target and resolve the diagnostic before treating the plan as complete.

5. Verify the saved candidate and retain the initial report. These single-line
    commands work in Bash and PowerShell 7:

    ```console
    hoimin verify PLAN.json --top 1 --dry-run > reports/preview.json
    hoimin verify PLAN.json --top 1 > reports/before.jsonl
    ```

    Inspect `reports/before.jsonl`: the baseline should pass and the candidate
    should survive because both `0 + 0` and `0 - 0` equal zero. Use the candidate
    ID and replacement from the plan to investigate the actual change. A timeout,
    error, resource stop or incomplete report is not a survivor; see [results](#results).
    This verification exits **0** because it completed, including the survivor. Check
    `complete` and `counts` on the final `run_finished` event, and `termination`
    on the `baseline_finished` event.

6. Keep `calc.py` unchanged and add this test to `test_calc.py`. The requirement
    is addition, so distinct nonzero inputs distinguish it from subtraction:

    ```python
    def test_add_distinct_positive_numbers():
        result = add(2, 3)

        assert result == 5
    ```

    Run `python -m pytest -q` again and expect two passing tests. In real projects,
    have the agent justify the assertion from the specification; ask a human to
    resolve ambiguous behavior. Do not change production code merely to kill a
    mutant or treat suspected equivalence as proof.

7. Verify the same selection from the unchanged plan and compare reports:

    ```console
    hoimin verify PLAN.json --top 1 > reports/after.jsonl
    hoimin progress reports/before.jsonl reports/after.jsonl
    ```

    The new baseline should pass and the same candidate should be killed by the
    new assertion; this verification exits **0**. Preserve both reports with the
    plan and test diff. Test-only
    changes can reuse this plan unless those files are explicitly fingerprinted;
    changing selected source or a fingerprint input requires a new plan.

For your project, replace `calc.py` and the test command, agree a small budget,
and repeat the same selection after justified test improvements. Stop when the
scope is covered, the budget is exhausted, or remaining survivors lack a useful
specification-backed test. Record uncertain or unhelpful candidates with
[human review](#human-review-of-mutation-candidates) rather than chasing a score.

#### Selection options after the first loop

Create a fresh plan for different targets, operators or execution settings.
For a real project with `src/`, you can restrict discovery to Git changes; this
requires Git changes and is not a prerequisite for the example:

```bash
hoimin plan --root . --source src --changed --total-timeout 15m \
  -- python -m pytest -q > CHANGED_PLAN.json
```

Apply the same OS policy choice to this `plan`. Preview a selection before
executing it, and keep the same mode and parameters for before/after reports.
On a plan with enough retained candidates, choose one of these alternatives:

```console
hoimin verify CHANGED_PLAN.json --top 10 --selection-policy diverse
hoimin verify CHANGED_PLAN.json --top 10 --selection-policy line-diverse
hoimin verify CHANGED_PLAN.json --sample 10 --seed 42
```

The first two spread equal-score candidates across files or source start lines.
Sampling requires a complete plan; it is not equivalent to a ranked prefix.

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
The `line-diverse` policy round-robins `(file, start line)` groups within each
score tier. Group order follows the first saved rank and candidates inside each
group keep their saved order. Multi-line mutations belong to their start line;
remaining candidates return on later rounds, so none are suppressed.
The verification report records `file_round_robin_v1`.
For line diversity it records `line_round_robin_v1`.
Older report readers may need updating to accept the new policy value.
Line diversity can expose other test gaps with a small budget, but additional
lines do not guarantee better defect detection or an unbiased mutation score.
Verification does not rewrite the plan or change its execution limits.

`--candidate ID` remains available for exact selection and may be repeated.
`--candidate` and `--top` are mutually exclusive, and one selection mode is
required. If `N` exceeds the retained candidate count, `--top N` selects every retained candidate
and reports the actual selected count. A plan with no retained candidates cannot be verified
with `--top`: all selection policies reject it with exit code 2 before running the baseline.
Version-1 manifests
must be regenerated with the current `hoimin plan`.

`--candidate` is repeatable, so one `verify` invocation can execute multiple planned candidates.
`verify` inherits the test command, execution limits, timeout settings, and resource policy from
`PLAN.json`; execution output is always a JSONL event stream. It cannot override
plan-time settings. In particular, `verify --top` retains all limits
from the manifest. A timeout-capacity warning is an estimate, not a guaranteed failure, and does
not adjust those limits automatically. To change `--jobs` or `--total-timeout`, create a new plan;
the same applies to `--allow-best-effort-memory` or another execution or resource setting.

The default total timeout is five minutes. Give a large candidate selection enough headroom
when creating the plan, or split it into stable batches across multiple `verify` invocations.
On macOS, `--max-memory` is accepted for plan compatibility but is not enforced. Hoimin still enforces monotonic wall-clock deadlines. On timeout or cancellation, while it owns a live root, it terminates that process group and reaps the root; descendants left after the root exits naturally are not guaranteed to be cleaned up. Pass `--allow-best-effort-memory` to `plan` when this memory policy is acceptable. `verify` does not provide that option.

`plan` discovers candidates but does not run a baseline or test command, copy a worker, or create or reuse a session. A manifest with `truncated` set to `true` contains only a partial candidate set, and `plan` exits 4; it cannot establish full coverage of the selected targets. On such a plan, `--top N` means the top N among retained candidates, not among candidates that discovery did not retain. `verify` still runs the retained selection, but its report remains incomplete and exits 4 because discovery was truncated. `verify` rejects a changed target or fingerprint input before its baseline runs. Each `verify` command runs a fresh baseline and does not use a session. After a successful baseline, the worker is restored to the preflight snapshot before any mutant runs; baseline-created files and changes to existing files do not carry into the first mutant. A restoration failure stops execution. Plan manifests are trusted local invocation data, not a security boundary.

`plan` source hashing and `verify` candidate validation read project sources
through the same root-relative regular-file reader. Source hashing streams bytes
without retaining the whole file; candidate validation retains the bytes it
needs for one source at a time. Symlinks and nonregular sources introduced after
target resolution are rejected. This source policy does not impose a hard
deadline on all filesystem I/O.

`verify` reads a plan from a regular file and accepts symlinks to regular files.
Directories, missing paths and nonregular inputs are errors. On Unix, writerless
FIFOs and symlinks to them are rejected without waiting for a writer. This does
not promise a deadline for every filesystem operation or add review's 64 MiB
input limit to `verify`.

`verify` decodes the plan directly from a buffered reader rather than retaining
its entire JSON encoding alongside the decoded candidates. The necessary plan
payloads remain in memory. `verify` and `progress` bound a read to the opened
file's initial length and reject observed growth or early truncation; use
completed files and retry if another process is still writing. This is not an
atomic snapshot and does not detect every in-place edit.

Use a narrower selector to check one symbol:

```console
hoimin run --root . --source src --symbol calc:add -- python -m pytest -q
```

Everything after `--` is passed directly as native argv to the child process. hoimin does not join it into a command string, invoke a shell, expand globs or variables, or interpret shell quoting.

### Live progress and debugging

`run`, `plan`, and `verify` emit no periodic terminal progress lines. To inspect
execution stages, enable tracing with `RUST_LOG`. The `hoimin progress` command
compares saved reports and returns a JSON document.

For the example's `calc.py`, enable diagnostic logs with the command for your
shell. Keep actively written logs outside the project root: writing a log inside
the copied project can trigger original-file integrity checks. These examples
create `../hoimin-diagnostics/`. Apply the [OS policy choice](#before-execution)
to `run` as well.

**Bash:**

```bash
mkdir -p ../hoimin-diagnostics
RUST_LOG=hoimin_cli=debug hoimin run --root . --file calc.py \
  -- python -m pytest -q \
  > ../hoimin-diagnostics/result.jsonl 2> ../hoimin-diagnostics/debug.jsonl
```

**PowerShell 7:**

```powershell
New-Item -ItemType Directory -Force ../hoimin-diagnostics | Out-Null
$previousRustLog = $env:RUST_LOG
try {
    $env:RUST_LOG = 'hoimin_cli=debug'
    hoimin run --root . --file calc.py `
      -- python -m pytest -q `
      > ../hoimin-diagnostics/result.jsonl 2> ../hoimin-diagnostics/debug.jsonl
} finally {
    $env:RUST_LOG = $previousRustLog
}
```

Logs use JSON Lines on stderr, including when stderr is a terminal. With logging
enabled, stderr can contain both tracing records and existing diagnostic events;
tracing records do not use the versioned run-event schema. Filter by module,
such as `RUST_LOG=hoimin_cli::process=debug`, to inspect process execution. Run,
worker, and mutant identifiers connect logs from concurrent test processes.
Use `RUST_LOG=hoimin_cli::progress=info` for execution-stage records and completed
mutant counts. Counts include reused results and exclude candidates that were
not run. These records follow execution events rather than a periodic timer.

`RUST_LOG` supports a default level (`info`), module/target levels
(`hoimin_cli=debug`), and comma-separated combinations
(`warn,hoimin_cli::process=debug`). Levels are `off`, `error`, `warn`, `info`,
`debug`, and `trace`. Advanced span or field directives are outside the supported
logging syntax.

Logging is off by default. `RUST_LOG=off`, an empty value, or an invalid filter
disables tracing. Library callers choose their own tracing subscriber. Tracing
uses a bounded queue: a slow sink can lose log records, and shutdown waits at most
250 ms to flush. These logs supplement the run reports; use the reports for
complete mutation results.

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
hoimin verify plan.json --top 1
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

Git inspection retains at most 64 MiB of stdout per command. Exceeding this
limit is an error: hoimin terminates and reaps the command without accepting
partial changed-line results. Git stderr retains its first 64 KiB and an explicit
truncation marker, while excess stderr is drained. These internal limits are
independent of the test process's `--max-output` setting and use no temporary
spool files.

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
metadata rewriting. Normalized config stores one `environment` snapshot: `{"state":"untracked"}`
when no names are selected, or `{"state":"captured","names":["FLAG"],"digest":"…"}`
with a nonempty canonical name set and aggregate digest. Only that metadata enters
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

Resource control is central to delegating mutation testing to an agent on a local
machine. Choose budgets that leave room for the host and other work. A limit stop
is a reason to diagnose and narrow the next run, not a request to disable the guard.
The enforcement scopes and platform limitations below still apply.

The defaults are:

| Option | Default | Scope |
| --- | ---: | --- |
| `--jobs` | `1` | concurrent workers; maximum 256 |
| `--max-mutants` | `100` | mutants executed |
| `--max-candidates` | `10000` | candidates discovered before execution |
| `--analyzer-timeout` | `30s` | plan only; saved for verify discovery |
| `--baseline-timeout` | `60s` | baseline process |
| `--mutant-timeout` | `auto` | each mutant; `max(5s, 2 × baseline elapsed + 1s)` |
| `--total-timeout` | `5m` | complete run |
| `--max-memory` | `1GiB` | per root tree; Windows measures committed memory, Linux cgroup measures charged memory |
| `--max-output` | `1MiB` | combined retained stdout and stderr per process |
| `--max-copy-size` | `1GiB` | run-wide logical bytes copied across all workers |
| `--max-workspace-size` | `8GiB` | logical bytes in generated workspaces and run-owned output |
| `--min-free-space` | `10GiB` | mandatory filesystem reserve before more work starts |
| `--max-processes` | `64` | per root tree, including the root; Linux cgroup counts tasks, including threads |
| `--profile full` / `--profile focused` | `full` | candidate-selection profile |
| `--fingerprint-include GLOB` | none | invalidates compatible session reuse; does not copy worker files |
| `--fingerprint-file PATH` | none | fingerprints one exact root-relative file; does not copy worker files |
| `--fingerprint-env NAME` | none | fingerprints an explicitly selected inherited variable; repeatable |

The final run summary reports `resolved_mutant_timeout`, the individual cap
actually passed in mutant process requests. The `run_finished` event stores it
as `resolved_mutant_timeout`. The value
uses integer `secs` and `nanos` (for example, `{"secs": 6, "nanos": 800000000}`).
It is `null` if no mutant process request was built.
A failed baseline therefore never reports an unused configured cap as resolved.
This value is separate from process elapsed time and the total run deadline,
which can interrupt execution before the individual cap expires.

By default, there are no include/exclude overrides or SQLite session, and `--changed`, `--resume`, and `--allow-best-effort-memory` are disabled.

Every numeric limit must be nonzero. Copy and total-timeout limits are run-wide.

Worker parallelism and the process allowance per root tree are independent.
For example, `--jobs 4 --max-processes 1` allows four concurrent single-process
test trees. On Linux cgroup v2, each tree must also remain single-threaded.
The worker count remains limited to 256, regardless of the per-tree allowance.

On Windows, each baseline or mutant root process and its descendants share their own full `--max-memory` and `--max-processes` allowance, enforced by a nested Job Object. Memory means committed memory, not RSS; the process count includes the root itself. With `--jobs J`, concurrent trees can approach `J × max-memory` and `J × max-processes` in total. The outer run Job Object owns cleanup and imposes no aggregate memory or process cap. Host-imposed parent Job Objects can make effective limits stricter.

On Linux, delegated cgroup v2 enforces memory and task-count limits separately for each baseline or mutant root and its descendants. Each root cgroup receives the full `--max-memory` and `--max-processes` allowance; the latter also counts threads. With `--jobs J`, concurrent trees can approach `J × max-memory` and `J × max-processes` in total. Hoimin's parent run cgroup has no aggregate memory or task-count cap; limits imposed by ancestor cgroups remain effective.

When hard enforcement is unavailable, Unix uses best-effort process groups and non-macOS Unix also applies per-process `RLIMIT_AS`. Linux and macOS require explicit `--allow-best-effort-memory` approval for this policy. On macOS, the memory limit is not enforced. Hoimin uses monotonic wall-clock deadlines on portable Unix and, on timeout or cancellation while it owns a live root, terminates that process group and reaps the root. Cleanup of descendants after the root exits naturally is not guaranteed.

Reports identify `hard` or `best_effort` resource mode. This describes enforcement strength, not aggregation scope: with Windows Job Objects or Linux cgroup v2, the memory and process limits in `normalized_config` apply per root tree.

Rust callers construct `WindowsBackend::new()` without run limits and supply caps through each `ProcessLimits` request; the formerly ignored constructor argument has been removed.

The run header records the selected backend in `resource_control.mode` and the stable `resource_control.mechanism` identifier (`portable`, `linux_cgroup_v2`, or `windows_job_object`). Synthetic reused and not-run results report the current run’s selected mode; reused outcomes carry only a conclusive killed/survived status, project zero elapsed time, and own no execution output. Actual process results retain their observed mode.

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
deduplication identities before the bounded merge. In addition, one internal
16 MiB (16,777,216-byte) budget bounds retained candidate strings across producers,
conversion and all files of a plan discovery. Runtime analysis shares the same
kind of budget through its handler, including outstanding detached work.
The budget counts UTF-8 logical string lengths and explicit validation/encoding
scratch reservations. Exceeding it fails analysis with `analyzer.payload_limit`
(`plan.discovery` at the plan command), without a successful partial result.
This error may occur before count selection or the existing 2 MiB record check;
it is distinct from requested candidate-count truncation.

Source/parser/AST/facts, diagnostics, allocator overhead and capacities, prior
disk records, one candidate under construction and the fixed annotation batch
of at most three source-dependent replacements are outside this budget.
`--max-candidates` and this payload budget therefore do not bound process RSS.
`--max-memory` continues to govern descendant processes. See the
[accounting contract and measurements](../../development/knowledge/design/analyzer.md#retained-candidate-payload-issue-978)
for ownership, release rules and observed normal-input overhead.

Analysis supports AST depth up to 128, counting the module as depth 1 and
including auxiliary syntax nodes. A deeper tree fails with its source path and
`analysis depth exceeds supported limit 128`; it does not produce a complete
empty result. This limit applies even with `--max-candidates 1`. Flat files with
many shallow statements are not rejected by this depth limit.
The bundled Rust parser checks remaining stack during recursive parsing and
recovery. Rejected and discarded partial trees are destroyed iteratively, so
deep unary, power, lambda, and conditional expressions can reach this diagnostic.
Parser stack segments and AST storage still consume memory proportional to input.

`plan --analyzer-timeout` saves a positive timeout (at most 100 years) in
`normalized_config.discovery.analyzer_timeout`; `verify` inherits it without
an override. Direct `run` rejects this option and records no discovery setting.
For plan creation and verify rediscovery, the saved timeout is one
deadline for the complete discovery phase, not a new deadline per target. A
discovery timeout returns promptly while an already-running blocking analyzer
cooperatively stops and releases the resources it owns. It cannot interrupt a
source read already blocked inside a system call; that read must return before
the detached analyzer can finish stopping.

### Tuning parallel runs

Start with `--jobs 1` and a focused test command. Record the baseline elapsed time and
estimate one test worker's memory use before increasing concurrency gradually. A small
target can often keep `--jobs 1 --max-memory 1GiB --mutant-timeout auto`.

With Windows Job Objects or Linux cgroup hard enforcement, `--max-memory` applies separately
to each root tree: for example, `--jobs 4 --max-memory 4GiB` allows concurrent test trees to
approach 16 GiB in total (committed memory on Windows). Size concurrency and
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

Before changing a mutation target's permissions or contents, hoimin checks its
opened file's link count. A shared inode fails with `workspace.mutation.shared`,
even if its contents and permissions match the source. Cleanup also preserves
external referent permissions; an unsupported safe unlink can leave a cleanup
error. This protects hoimin's own writes and does not prevent a test program
from writing elsewhere.

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
[Windows filesystem safety](../../development/work/designs/2026-09-01-windows-disk-safety-capability-design.md) for
the detailed capability and cleanup model.

## Mutation operators

Use `hoimin run --help` or `hoimin plan --help` to see separate lists of default
runtime IDs, opt-in IDs and selector families. For each ID's meaning, before/after
example, applicability and limitations, see the [operator reference](../reference/operators.md).
The generated [selection index](../reference/cli.md#operator-selection) records
default states and links every ID; the [family index](../reference/cli.md#selector-families)
lists each alias's exact members.

### Choose and preview operators

1. Look up the IDs, defaults and families in the
   [selection index](../reference/cli.md#operator-selection). Read the
   `--operators` and `--exclude-operators` explanations there or in the command help.
2. Create a plan and preview the mutations before running tests. For example,
   select the regex family while excluding the whole-pattern ASCII-mode toggle:

   ```console
   hoimin plan --root . --source src --operators regex --exclude-operators regex_ascii_mode -- python -m pytest -q > plan.json
   hoimin verify plan.json --top 10 --dry-run > preview.json
   ```

   Replace `src` with your source directory and the command after `--` with your
   test command. Planning and dry-run verification analyze source and validate the
   plan without running that command. The preview shows the retained candidates;
   exclusions, applicability guards and candidate limits can leave fewer than ten.
   If the plan contains no candidates, choose different targets or operators;
   `verify --top` rejects an empty selection with exit code 2.

Use an operator's entry to choose test inputs and observations. For example,
`finally_unwrap` distinguishes cleanup on an escaping exception from cleanup on
normal completion. For `regex_character_set`, decide whether the specification
requires ASCII digits or all Unicode decimal digits before interpreting a survivor.
A survivor alone does not decide which behavior is correct.

For annotation mutations, run a type checker as the configured command:

```console
hoimin run --root . --source src --operators type_nullable,type_collections -- uv run ty check
```

To exclude nullable addition, place `--exclude-operators type_nullable_add`
**before** the `--` separator. Arguments after `--` belong to the configured
command on Windows and Unix. A type checker's nonzero exit kills the mutant.

Default selections can grow between releases and change the prefix retained by a
candidate cap. Explicit selections and saved plans keep their recorded operator
sets; reloading a saved plan does not expand it with additional defaults.

### Match documentation to your installation

Check `hoimin --version`. The installed `run`/`plan` help links to the usage guide
at that package version's release tag. For a release, follow that tag's own links;
older releases may not have the separate operator reference. The hosted website
follows `main`, so it can describe behavior absent from your installed release.
For a source build, read the documentation in the same checkout; a package version
alone does not identify an untagged source revision.

## Mutation profiles

`--profile full` is the default and considers every candidate produced by the selected
operators. `--profile focused` suppresses candidates in Python `__main__` guards, bare
`print(...)` calls, `assert` statements, and function default expressions. A
statement exchange is suppressed if either participant is in a suppressed region.
Use `full` when measuring the complete selected target.

```console
hoimin run --profile focused --root . --source src -- python -m pytest -q
```

## Results

Use results to investigate missing behavioral checks, not to maximize a mutation
score. Equivalent or specification-permitted changes may survive without indicating
a useful test improvement. An agent's suspected-equivalent assessment is a judgment,
not a proof, and does not change the execution outcome. Compare scores only with
their candidate sets and verification conditions in view; they are not a standalone
quality guarantee or a CI acceptance target.

### Mutation-progress reports

Compare ordered run reports to track mutation-testing progress. Inputs are oldest-to-newest, and `--patience` defaults to three consecutive comparable stalls.
The command returns JSON and accepts no `--format` selector.

Repeated input paths and byte-identical copies produce warnings on stderr. Warnings identify the repeated input and an
earlier input, with `same path` or `identical bytes` as the reason. Inputs remain
in order: self-comparisons still count as stalls and can reach saturation, and
unusable reports still break the stall chain. No strict rejection is applied.
Different reports that share a run ID (for example, before and after resume)
are not duplicates on that basis. Copy detection compares raw bytes, so the
same execution saved with different whitespace is not
detected as a copy. Copy confirmation is best-effort if files change or become
unreadable after validation; reports should remain unchanged during comparison.

Inputs are JSONL event streams saved from `hoimin run` or `hoimin verify`, using
report schema 10. Aggregate JSON documents and older or unsupported future schemas
are rejected with exit code 2. Each physical line is decoded as an event.
There is no fixed total-size or line limit. Memory still includes finished mutant results, sequence-validation state,
and the current event's decoding storage. Progress retains only adjacent report
pairs for comparison. The initial-file-length read policy described above also
applies to progress. For example:

```console
hoimin progress reports/before.jsonl reports/after.jsonl
```

JSONL is read one event per nonblank line. `run_started` must come first and
`run_finished` must be present and last. `mutant_started` must precede the
matching `mutant_finished`; diagnostic events are validated and then discarded.
Run IDs, event sequence, baseline order, mutant identities, results and summary
counts must be consistent. Truncated lines, missing completion, mixed runs and
duplicate events are rejected with exit code 2. A valid stream whose completed
summary marks the run incomplete, or whose baseline failed, is unusable for
comparison. CRLF and a complete final line without
a newline are accepted.

Read historical reports with their producing binary, or run the tests again with
the intended Hoimin version to generate new reports. Reports are not converted
or rewritten automatically. Regenerating a plan alone cannot recreate a past
execution report.

The JSONL reader retains comparison candidates and event decoding state;
it does not retain the diagnostic/start-event history or the entire input
string. Its memory use still depends on candidate data and the largest event.


```console
hoimin progress --patience 3 reports/before.jsonl reports/after.jsonl reports/latest.jsonl
hoimin progress reports/*.jsonl
```

Comparisons require adjacent reports that are both complete and have successful baselines. `saturated` means the configured number of consecutive comparable stalls was reached. Only immediately adjacent stalled comparisons contribute to this count. An improving, regressing, or indeterminate comparison resets the consecutive stall chain.

Valid progress comparisons exit with code 0, including `regressing` and
`indeterminate` results. Inspect JSON `latest.state` and the comparison history
to decide what to investigate next. Input, validation, and output failures exit
with code 2. Progress provides no regression gate. A surviving mutant is not
proof of behavioral equivalence.

The JSON `previous_score`, `current_score`, and `score_delta` fields use the intersection of common mutants with conclusive
results in both reports. Read the latest run summary for its whole-run mutation
score.

For split verification, choose stable candidate batches and keep a separate oldest-to-newest
report history for each batch. `--offset K --top N` skips K entries in the complete
selected-policy ordering, then selects up to N candidates.

Omitting `--offset` is equivalent to `--offset 0`, including in preview output:

```console
hoimin plan --root . --source src --max-mutants 100 -- python -m pytest -q > plan.json
mkdir -p reports
hoimin verify plan.json --top 100 --offset 0 > reports/batch-a-001.jsonl
hoimin verify plan.json --top 20 --offset 100 > reports/batch-b-001.jsonl
# After improving tests, repeat the same ranges from the same plan.
hoimin verify plan.json --top 100 --offset 0 > reports/batch-a-002.jsonl
hoimin verify plan.json --top 20 --offset 100 > reports/batch-b-002.jsonl
hoimin progress reports/batch-a-001.jsonl reports/batch-a-002.jsonl
hoimin progress reports/batch-b-001.jsonl reports/batch-b-002.jsonl
```

This example partitions a plan with 120 retained candidates. Keep the same
`--selection-policy` for its batches: `strict` uses saved rank order; `diverse`
uses the complete equal-score file-round-robin order; `line-diverse` uses the
complete equal-score start-line-round-robin order. Both apply offset/top after
ordering, so page boundaries never restart the rotation. Changing
the policy between batches can change membership. The JSONL mutant events
retain the actual selected candidate IDs; save the range/policy commands with
those reports for reruns.

Preview a batch before running its tests:

```console
hoimin verify plan.json --top 20 --offset 100 --selection-policy diverse --dry-run > preview.json
```

`--dry-run` performs the same plan, source, fingerprint and candidate validation
as verification, then exits without baseline or mutation tests, worker copies,
sessions or execution metrics. It conflicts with `--metrics`. A valid preview
exits with code 0, including for a truncated plan when using ID/top selection;
sampling rejects truncated plans. Invalid selections and stale
plans exit with code 2. Runtime resource availability is checked when executing.

### Reproducible random samples

Use a sample when you want a limited number of mutants drawn across all saved
ranks. Both the positive count and unsigned 64-bit seed are required:

```console
hoimin verify plan.json --sample 100 --seed 42 --dry-run > sample-preview.json
hoimin verify plan.json --sample 100 --seed 42 > reports/sample-001.jsonl
hoimin verify plan.json --sample 100 --seed 42 > reports/sample-002.jsonl
hoimin progress reports/sample-001.jsonl reports/sample-002.jsonl
```

The population consists of the candidates in the complete saved plan, as limited
by its selectors, profile and operators. Sampling rejects `truncated: true` and
empty plans. A request above the population size selects the entire population
in sampled order. If the actual selection exceeds the plan's `max_mutants`,
verification rejects it before tests run; create a plan with a sufficient budget.
`--sample` conflicts with `--top`, `--candidate`, `--offset` and
`--selection-policy`. `--seed` is valid only with `--sample`.

The `splitmix64_fisher_yates_v1` policy uses a fixed 64-bit pseudorandom generator,
rejection sampling and a forward partial Fisher–Yates shuffle without replacement.
The same plan, count, seed and algorithm version produce the same IDs and dispatch
order across platforms. Parallel test completion order and elapsed time can vary.
Increasing the count keeps the previous selection as a prefix.

Preview and JSONL run metadata use `mode: sample`,
`scope: sampled_candidates` and the versioned `policy` above. The
`verification_selection.sampling` object contains `population`, `seed` and
`selected_ids` in dispatch order; `requested` and `selected` distinguish requested
and actual counts. Preserve u64 seeds as integers when reading JSON: a JavaScript
Number cannot represent every accepted seed exactly. Run-start and summary
metadata retain the full selection even when tests fail before all mutants run.
Existing selection modes omit the optional `sampling` object.

Completion and mutation score describe the selected sample. Mutants outside it
have no outcome; killing the sample does not certify the population. Timeouts,
cancellation and other unfinished outcomes retain the usual incomplete-result
semantics. This feature reports no confidence interval or real-defect detection
estimate. Fixed-seed frequency checks and small authored-project measurements are
available in the [evaluation](../../development/work/reports/issue-695/review.md).

For repeated evaluation, preserve the plan, count and seed and keep one report
history per sample. Changes to tracked source or fingerprint inputs require a new
plan; confirm its ordered selected IDs before treating it as the same sample.
For a fixed candidate set in a regenerated plan, replay saved IDs with repeated
`--candidate ID` options (that mode uses its existing discovery order).
Changing seeds or populations can make `progress` indeterminate under its existing
candidate identity and ambiguity checks. Source and fingerprint validation still
applies in every mode.

When saved source or fingerprint input records differ from the workspace,
verification reports `modified`, `added` or `removed` with quoted root-relative
paths. Normal verification and dry-run show the same diagnostic before tests
run, with empty stdout and exit code 2. Details are sorted by path, limited to
ten paths, and followed by the number of additional paths omitted. Control
characters in paths are escaped. Earlier discovery or read errors retain their
own diagnostic rather than being classified as record differences.

Dry-run validates the saved plan and selected candidates without copying the
workspace. Fingerprint copy membership uses path selection rather than reading
every unrelated copied asset; with no fingerprint inputs that walk is skipped.
Copy-content read or length-change errors may therefore appear during actual
preflight, and a successful preview does not certify all copied asset bodies.

The [preview schema](../reference/json-schema/verify-preview.schema.json) is independent
of run reports: `kind` is `verify_preview` and `schema_version` is 2.
Dry-run always returns one JSON object.
The ordered `candidates` array gives each candidate's `id`, saved `rank`,
`selection_order`, `path`, `line`, `column`, `operator`, `original` and `replacement`.
Details come from the same validated plan candidate. Rank, line and batch selection
order start at 1; column is the plan's 0-based Python source column.

Plain-text errors, help, progress warnings and command diagnostics escape controls
in external fields, including ESC, CR, LF and TAB; ordinary Unicode such as Japanese
remains readable. JSON encoders escape strings while preserving the logical values
of paths, IDs and other fields. Baseline child transcripts keep their separate
streamed-output policy, including readable newlines and tabs; dry-run never executes
baseline or test commands.

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

When a baseline fails or reaches its process timeout, `run` and `verify` copy its retained combined stdout/stderr to stderr before execution cleanup. Redirect stderr to save these diagnostics, for example `hoimin run ... > result.jsonl 2> baseline.log`. Execution reporting keeps stdout unchanged and encodes stderr diagnostics as JSON Lines (`baseline.output`). The header records retained/observed byte counts and truncation; `--max-output` limits the retained tail. Output is streamed in 16 KiB chunks, preserving split UTF-8 characters; invalid bytes become U+FFFD and terminal control characters other than newline/tab are escaped. Chunk offsets refer to retained raw bytes. Successful baselines stay quiet. Read failures produce `baseline.output.read`; cancellation or a blocked diagnostic sink can stop export at the existing total timeout and shutdown grace, so interrupted export may be partial.

`run` and `verify` emit flushed JSONL lifecycle events; diagnostics are JSON Lines on stderr. The execution event contract is versioned in [`run-event.schema.json`](../reference/json-schema/run-event.schema.json). Event kinds are `run_started`, `baseline_finished`, `mutant_started`, `mutant_finished`, `diagnostic`, and `run_finished`. Parallel events are emitted in completion order; candidate sequence numbers allow stable reordering.

Report schema 10 requires `status` in each JSONL
`mutant_finished` event. Read this field for the candidate's verdict, for example
`{"status":"not_run","outcome":"not_run"}`. The writer computes it from
`outcome`; strict progress/review readers reject a disagreement with exit 2.
JSON Schema checks the required field and its seven allowed values; only the
strict reader checks equality with the outcome. Summary, progress and resume
continue to derive their classifications from outcome evidence.

JSONL `mutant_finished` events include the full candidate
ID, source position, operator, original and replacement expressions, and verdict.
Use these fields to identify results when parallel workers finish in a different
order.

The final summary's `complete` is `false` when any mutant is inconclusive or the run fails or is interrupted. It is `true` only when every selected mutant is `killed` or `survived` and no run-level failure occurred; a successful run with no candidates is also complete. Therefore, an exit code of `4` always has `complete: false`.

`progress` accepts JSONL reports from regular files, including symlinks
to regular files. It rejects directories, missing paths and nonregular inputs;
on Unix this includes writerless FIFOs without waiting for a writer. Review's
64 MiB input limit does not apply to `progress`.

Run `hoimin progress --details before.json after.json` to identify improvements and
regressions in the **final adjacent input pair**. Details include candidate ID,
operator, previous/current status and source positions (one-based lines, zero-based
columns). JSON string values preserve the original field values. `--details-limit N`
sets the maximum returned changes (default 100; zero shows only omission counts), ordered
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

`progress` uses [progress schema 3](../reference/json-schema/progress-result.schema.json)
with or without `--details`. The required `details` field is `null` when details
were not requested. With `--details`, it is an object, including `available: false`
when the final pair cannot be compared. Aggregate decisions and exit status are
independent of whether details were requested. See the
[current progress format](../reference/compatibility.md#single-progress-output-schema).

### Operational metrics

Run metrics are an opt-in operational sidecar, separate from the execution event stream. Write them by passing a destination to `--metrics`:

```console
hoimin run --root . --source src --metrics metrics.json -- python -m pytest -q
hoimin verify plan.json --top 20 --metrics batch-metrics.json > batch-report.jsonl
hoimin verify plan.json --candidate m1_ID --metrics candidate-metrics.json > candidate-report.jsonl
```

`verify --metrics` supports explicit candidate IDs and both strict and diverse top selections. Relative metrics paths use the directory where you invoke hoimin, independently of the saved project root. The destination is not stored in the plan and does not change candidate selection or execution limits. Metrics begin when shell execution starts; plan validation and rediscovery time are not included. A failure during plan preparation leaves the destination untouched. Once execution starts, failed baselines and interrupted runs follow the same sidecar and warning rules as `run`.

The sidecar uses the versioned [`run-metrics.schema.json`](../reference/json-schema/run-metrics.schema.json) contract. Its `executed` count never exceeds `discovered` and equals the sum of per-worker `processes`. Metrics are operational observations: they do not affect resume compatibility and are not embedded in execution events. A metrics write failure warns without changing the mutation result. A confirmed collision with the verify input manifest, a selected source, explicit fingerprint input, session database, active SQLite companion or session ownership lock is rejected before the baseline. You can write metrics inside the project or replace an existing metrics file. For a verify input specified through a symlink, both the specified entry and its resolved file are protected. A separate hardlink or final symlink can be replaced while preserving its protected referent.

Hoimin checks the entry that the final atomic rename replaces and retains its parent directory handle through writing and temporary-file cleanup. Renaming or replacing the parent's original pathname does not redirect metrics into a replacement directory: output stays in the retained directory or fails safely. On Windows, the retained handle can prevent the parent from being renamed. If Hoimin cannot establish a safe destination identity, it skips metrics and emits a `metrics.write` warning without changing the mutation result. This includes unresolved filesystem case behavior and some prospective non-ASCII or Windows alias names.

The schema validates the document structure, schema version, and nonnegative integer values. The producer additionally guarantees unique stage names and workers in ascending worker-ID order; these semantic constraints are enforced by `RunMetrics::validate()` rather than JSON Schema.

Mutant statuses are:

- `killed`: the mutant test process exited nonzero and output collection completed, or a saved killed result was reused;
- `survived`: it exited zero and output collection completed, or a saved survived result was reused;
- `timeout`: its deadline elapsed;
- `out_of_memory`: the kernel reported an OOM kill in the supervised root cgroup subtree;
- `process_limit`: the root tree's process limit stopped it (Linux cgroup also counts threads);
- `error`: infrastructure could not produce a valid result; output collection's `close_timed_out` overrides the termination, including `Exit(1)`;
- `not_run`: no classified completion is available, including cancellation or a run-wide limit; a process may have started before cancellation.

The score is `killed / (killed + survived)`. Inconclusive statuses are excluded. If no killed or survived mutant exists, `score` is `null`.

Exit codes are:

| Code | Meaning |
| ---: | --- |
| `0` | complete, including runs with survivors |
| `2` | CLI, configuration, analyzer, storage, workspace, or other infrastructure error |
| `3` | baseline failed |
| `4` | incomplete because of timeout, memory/process/candidate/mutant limits, or `not_run` |
| `130` | user cancellation |

Higher-priority conditions win in this order: cancellation, infrastructure error, baseline failure, incomplete result.
Survivors remain visible in mutant statuses, summary counts and scores; they do
not change the exit code of a complete run.

For `run`, `--total-timeout` starts after CLI configuration validation and before
capturing environment values or reading and hashing fingerprint inputs. Startup
janitor selection and cleanup, managed workspace and spool creation, resource
backend preparation, and the initial disk measurement consume that same budget.
Preparation does not reset the deadline before target discovery, copying,
baseline, analysis, or mutation tests. With the same configured duration, slower
preparation can leave time for fewer mutants.

For `verify`, the deadline starts after the saved plan has been loaded,
validated, and reanalyzed and a verified execution plan has been received. The
shared execution preparation is included; those earlier plan checks are excluded.
`plan` and `verify --dry-run` keep their existing contracts.

If preparation times out, no later preparation stage, baseline command, or mutant
command is started. The diagnostic identifies preparation and does not emit a
report containing an unexecuted baseline. An orderly preparation timeout exits
with `4`, and user cancellation exits with `130`. Preparation, rollback, and
diagnostic output failures exit with `2`.

`--total-timeout` stops normal work at its configured deadline. Scheduler,
process, blocking-I/O, and internal-finalization waits then receive at most a
fixed additional two seconds for orderly shutdown. The grace starts at the run
deadline or the first cancellation request; changing stages does not restart it.
Preparation rollback and diagnostics share it, including cleanup of multiple
managed roots. A timeout whose cleanup
completes inside that grace remains an incomplete run with exit code `4`. If
cleanup cannot finish before the shutdown grace expires, the run is an
infrastructure failure with exit code `2`. CLI report writes and flushes share
that shutdown deadline. Once the budget expires, the CLI can exit without a
final stderr diagnostic; stdout can contain a partial report if its consumer
stalls. Library run and verify entry points require owned `Write + Send + 'static`
writers and use the same delivery deadline. A started blocking operation can
outlive the wait; it retains its writer and delivery resources until it returns.
The same ownership rule applies to preparation I/O and live disk measurements:
stopping the wait does not prove the operation has stopped, and its leases and
managed roots remain owned until completion. Existing janitor rules recover
residual roots when safe. The timeout does not guarantee immediate interruption
of an OS operation.

## Sessions and resume

The project root is stored only in `run_started.normalized_config.selection.root`. Saved plans use schema 10. Regenerate schema-9 and older
plans; read old reports with their producing binary or rerun with this version.
See the [0.12.0 compatibility notice](../reference/compatibility.md#discovery-timeout-ownership-in-0120).

Run and verify emit schema-10 JSONL; consumers validating the closed schema must update. `progress` accepts only schema-10 JSONL and rejects historical, unsupported future and mixed-version reports. When `--resume` is requested, events include `run_started.resume`: `{"status":"resumed"}` or `{"status":"fresh","reason":"CODE"}`. Runs without `--resume` omit this field.

A compatible incomplete run always wins, even if newer runs are incompatible. If an eligible row appears only during the later history inspection, `candidate_changed` takes precedence. Otherwise reasons use this precedence: `budget_decreased` (matching incomplete runs require a higher `--max-mutants`), `matching_run_complete`, `fingerprint_mismatch` (other incomplete history), `no_incomplete_run` (only nonmatching complete history), then `no_prior_run`. `candidate_changed` means session history changed during resume selection. These codes describe observed history, not which configuration field or file changed. Existing old-schema, corrupt-data, database-read, and ownership errors remain errors.

A successful baseline is followed by restoration to the preflight snapshot before
the first mutant runs. If restoration fails or the run is cancelled during it,
no mutant starts.

No database is created by default. `--session PATH` stores a run in SQLite and commits each mutant result independently. `--resume` requires `--session` and looks up the newest compatible incomplete run. Compatibility includes ordered import roots and source roots, source and configuration fingerprints, test argv, verdict-affecting limits, resource policy, and the operator set. Profile selection is part of session compatibility, so a focused run never resumes results from a full run and vice versa. `--jobs` and `--max-output` are operational settings and may change when resuming; reports record their current values, and reused results do not import output retained under the earlier limit. Completed `killed` and `survived` results can be reused; `timeout`, `out_of_memory`, `process_limit`, `error`, and `not_run` are run again under the current settings. An incompatible or already complete run is not silently mixed with new results.

`--max-mutants` may stay the same or increase when resuming an incomplete run.
The limit counts reused results as well as newly executed mutants: increasing
1 to 3 can reuse the first result and execute the next two. The database records
the latest accepted limit, and a lower limit cannot resume that run. A baseline
still runs on every invocation. Other verdict-affecting limits must remain compatible.

Existing session databases must have exactly one hard link. hoimin rejects a
multi-link file before SQLite writes or creates companion files, including when
it contains a current session. Use a new `--session PATH`; hoimin does not remove
aliases or repair the file. New paths and single-link empty/current databases
remain supported.

SQLite sessions use schema 5, which stores one typed mutation outcome per result.
hoimin creates this schema directly for an uninitialized database and reopens
existing schema-5 databases without rewriting saved rows. Schema versions 1-4
and unsupported future versions are rejected. Choose a different `--session PATH`
to start fresh, and keep older databases with their producing binary if you need
to read or resume them. Files are not automatically converted, deleted, or
replaced. The 0.8.0 floor covers this incompatible change.

An existing schema-5 run can still lack a recorded historical budget. Start a new
run when that incomplete run reports `session.resume.incompatible`; completed
results remain available in the database.

### Mutation result outcomes

Report schema 8 represents each mutant result as one `outcome`, replacing
independent `status`, nullable `termination`/`output`, `output_state`, and
`elapsed_ms` fields from older formats:

- `executed` carries termination, output state, output spool, and elapsed duration.
- `{"reused": {"status": "killed"}}` or `{"reused": {"status": "survived"}}` identifies reuse from a compatible session. It contributes zero elapsed time and no output to this run.
- `"not_run"` means no classified completed result is available. A process may have started before interruption. It contributes zero elapsed time and no output.

An executed outcome has this form:

```json
{
  "executed": {
    "termination": {"Exit": 1},
    "output_state": "complete",
    "output": {"token": "spool", "retained": 3, "observed": 8},
    "elapsed": {"secs": 0, "nanos": 5000000}
  }
}
```

Elapsed time requires a closed object with `secs` in the u64 range and `nanos`
from 0 through 999,999,999. Array aliases and normalized oversized nanoseconds
are rejected.

For executed outcomes, `close_timed_out` means `error` regardless of termination.
With `complete` output, exit 0 means `survived`, any other exit means `killed`,
resource/time limits retain their named status, and cancellation means `not_run`.
Diagnostics remain separate evidence and do not determine output state or status.
Progress, review, and summary counts derive these statuses. Baseline
report fields retain their existing representation. Update strict report consumers
to schema 8; old flat mutation result records are rejected without conversion.

Plan schema version 10 stores fingerprint input records only in
`normalized_config.fingerprint_inputs`, alongside the validated environment snapshot
and independent import roots (ranking rule version 4). `sources` separately records
mutation targets. Regenerate schema-9 and older plans before verify or review using
the original plan arguments and intended environment; neither command converts old
files. The former top-level `fingerprint_inputs` field is rejected.
Report schema 8 records typed changed selection and session intent, and retains the
environment snapshot introduced in schema 5 in place of the independent
`fingerprint_env` / `fingerprint_env_hash` fields. Update strict report consumers
to schema 8; read schema-7 and older reports with their producing binary, or rerun
with this version. The release floor is 0.13.0. Files are not automatically converted
or deleted. Fingerprint schema 13 excludes reuse of results produced before the
baseline restoration fix. Discovery timeout remains outside execution compatibility,
as introduced in schema 12. See the [0.13.0 compatibility notice](../reference/compatibility.md#baseline-restoration-and-result-reuse-in-0130).
Completed old history retains fresh-run behavior.
Fingerprint schema version 13
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

## Additional runtime mutations

For per-operator examples and guards, see [statements, conditions and function results](../reference/operators.md#statements-conditions-and-function-results), [collections](../reference/operators.md#collections-indexing-and-slicing), and [state and object contracts](../reference/operators.md#state-ordering-and-object-contracts). Check defaults and family membership in the [selection index](../reference/cli.md#operator-selection).

## Human review of mutation candidates

`review` records human assessments of saved plan candidates in an explicitly
selected local SQLite sidecar. List, show and record return JSON and accept no
`--format` selector. It does not run the analyzer or tests, alter the
plan/report, change mutation scores or progress decisions, or suppress candidates.

An external agent can also use these commands to record reasoned assessments
within its authorized task. The command does not invoke an LLM or establish
equivalence. Humans resolve specification ambiguities and review the proposed
tests; an agent must retain uncertainty when it lacks evidence.

Inspect the saved plan with these single-line commands in Bash or PowerShell 7:

```console
hoimin review list PLAN.json --store reviews.sqlite3
hoimin review show PLAN.json --store reviews.sqlite3 --candidate CANDIDATE_ID
```

Replace `CANDIDATE_ID` with the ID from your plan, and use your plan's path in
place of `PLAN.json`. To record a justified assessment, use your shell's form:

**Bash:**

```bash
hoimin review record PLAN.json --store reviews.sqlite3 --candidate CANDIDATE_ID \
  --assessment actionable --reason 'A required field can be omitted without a failure' \
  --expected-revision 0
```

**PowerShell 7:**

```powershell
hoimin review record PLAN.json --store reviews.sqlite3 --candidate CANDIDATE_ID `
  --assessment actionable `
  --reason 'A required field can be omitted without a failure' `
  --expected-revision 0
```

Replace the example reason with evidence for the actual selected candidate;
the required-field example is not an assessment of the addition tutorial.

Assessments are `actionable`, `not_useful`, `suspected_equivalent`, and
`needs_investigation`. No record means `unreviewed`, distinct from `not_useful`.
Suspected equivalence is a reviewer judgment, not a proof. A specification-permitted
reorder may be unhelpful, while an undetected required-field omission may identify
a useful test improvement; neither changes the actual mutation outcome.

`record` requires a nonblank reason of at most 4096 UTF-8 bytes. Repeat
`--evidence TEXT` for at most eight inert references, each at most 2048 bytes.
References are not opened or executed. Control characters other than newline
and tab are rejected. JSON output escapes strings and preserves their logical values.

Use revision 0 for the first assessment. For an update, read the latest revision
with `show` and pass it to `--expected-revision`. Concurrent or outdated writes
fail instead of overwriting history. Repeating an assessment with the current
revision appends an event. If output fails after commit, the diagnostic names
the saved revision; inspect it before retrying. Retrying the old revision cannot
append a duplicate event.

The store is bound to one canonical project root and each candidate's full
descriptor and source hash. It is not automatically transferred to another
checkout or moved root. Modified sources are `stale`; missing or unreadable
sources are `source_unavailable`. Review reads sources through the same
root-relative file reader as plan/verify. Symlink and reparse-point components
are not followed, including links whose targets are inside the project; those
sources are unavailable. Nonregular sources are also unavailable, without
waiting for a FIFO writer. Invalid relative paths remain input errors.
These assessments do not count as reviewed current candidates. `list --stale`
includes stored candidates absent from the plan separately. List and show do
not create a missing store.

Plan-only output has execution state `not_loaded`. Add `--report run.jsonl` for
actual results from a complete schema-v10 JSONL execution stream with a passing baseline and
matching project/candidates. A plan candidate absent from the report is
`not_in_report`; recording it with that report is rejected. Aggregate JSON and older
reports are not supported by review. Plan-only recording remains allowed.

Limits are 64 MiB per plan/report, 16 MiB per store, and 10,000 events per store.
Source inspection is bounded to 64 MiB; larger sources are unavailable for
review. Writer locks wait at most one second. Journals need additional disk
space. Corrupt, unknown, or unrelated databases are rejected without automatic
repair. Use a local filesystem; network filesystem concurrency is outside the
initial contract. Store files and SQLite auxiliary paths must not alias plan,
report, source, or fingerprint inputs.

Starting with hoimin 0.8.0, review JSON uses version 2 of the independent
[review schema](../reference/json-schema/review-result.schema.json). Plan references contain
`path` and `content_hash`; report references contain an `artifact` object with
those fields and a required `run_id`. A review without a report keeps `report: null`.
SQLite review stores use `user_version = 2`. Version 1 stores are rejected on
read and write; there is no automatic conversion. Keep an old store separately
and use another `--store` path to record reviews in version 2.
See the [review contract](../../development/knowledge/design/survivor-review.md) for identity and
formal verification boundaries.

## Premature loop exit

See [`loop_break_insert`](../reference/operators.md#loop_break_insert) for insertion points, loop-else behavior and applicability.

## Early iterator exhaustion

See [`iterator_stop_insert`](../reference/operators.md#iterator_stop_insert) and [`async_iterator_stop_insert`](../reference/operators.md#async_iterator_stop_insert) for synchronous and asynchronous protocol-entry mutations.

## Generator behavior

See [loops, iterators and generators](../reference/operators.md#loops-iterators-and-generators) for eager/lazy conversion, early return, skipped first items, empty streams and delegation.

## Missing values, shared state and delegation

See [state, ordering and object contracts](../reference/operators.md#state-ordering-and-object-contracts) for missing values, shared defaults, outer bindings, dictionary insertion, copying, asynchronous results and formatted representations. See [`yield_from_to_yield`](../reference/operators.md#yield_from_to_yield) for delegation.

## Loop continuation, comparison delegation and exception causes

See [`continue_remove`](../reference/operators.md#continue_remove), [`comparison_not_implemented`](../reference/operators.md#comparison_not_implemented), and [`exception_cause_remove`](../reference/operators.md#exception_cause_remove) for examples and limits.

## Dataclass ownership, sorting keys, captures and encoding

See [`dataclass_factory_share`](../reference/operators.md#dataclass_factory_share), [`sort_key_swap`](../reference/operators.md#sort_key_swap), [`lambda_capture_late`](../reference/operators.md#lambda_capture_late), and [`encoding_errors_ignore`](../reference/operators.md#encoding_errors_ignore) for examples and eligibility rules.

## Explore test reductions

`reduce` searches for whole Python function or method definitions that can be
removed while preserving the complete killed/survived outcomes of explicitly
selected saved-plan mutants. It measures the ordinary test command from the plan;
it accepts arbitrary commands, including commands other than pytest.

```console
hoimin reduce /tmp/plan.json \
  --candidate m1_ID_FROM_PLAN \
  --test 'tests/test_subject.py:TestSubject.test_*' \
  --keep 'tests/test_subject.py:TestSubject.test_regression' \
  --total-timeout 5m --min-reduction 5 --output /tmp/reduction
```

Use exact root-relative file paths and qualified definition names. `*` matches
any sequence of characters and `?` matches one character in a name. Repeat
`--test`, `--keep`, or `--candidate` to select more inputs. Ambiguous redefinitions
are rejected. Keep takes precedence, including a kept nested definition inside
an otherwise removable function. A parametrized function is one removal unit.
Mutation scope and execution settings come from the saved plan; reduction files
must not overlap its mutation sources or explicit fingerprint inputs.

The time budget includes original-suite evaluation, grouped/split search,
repeated measurements, and final confirmation. Candidate ranking uses the median
of three ordinary executions. Confirmation alternates original and reduced suites
for three pairs; every pair must meet the threshold. The default reduction is
5%; a decimal percentage with up to six fractional digits must be strictly
between 0 and 100. An automatic mutant timeout is fixed from the first original
execution and reused throughout the search.

The output must be a new directory outside the project. `report.json` records
selection, timing, per-mutant outcomes, identities, and the stop reason.
`execution-N.bin` retains bounded combined child output. `proposal.diff` exists
only for a fully confirmed proposal. Original files are never edited. The diff
reproduces the verified bytes, including supported source encoding and line
endings. Exit 0 means a completed search, including a search with no confirmed
reduction; 2 indicates invalid input or an unusable original suite, 3 an
infrastructure failure, 4 an incomplete run due to time/resources, and 130 a
cancellation. Earlier confirmed evidence can survive a later stop. Reserving the
remaining time for confirmation can also end an incomplete search before the
absolute deadline.

After a completed search, cleanup uses the remaining total budget and the
two-second shutdown grace. A stopped search gets only the shutdown grace,
capped by the original deadline plus two seconds; changing cleanup stages
does not restart it. If cleanup cannot finish, the report retains its failure
and any previously confirmed proposal. Inspect `exit_code` and `stop` as well
as `best` before treating the whole command as successful.
Cleanup checks for the first Ctrl+C between bounded waits and removal slices;
an observed interrupt shortens the existing deadline and is reported as cancellation.

Review the retained output: ordinary nonzero mutant exits cannot generally
distinguish assertion failures from import or collection errors. Preserved mutant
outcomes do not establish that a test is unnecessary or that future fault
detection will be preserved. A person or external agent must review the meaning
of the tests and decide whether to apply the diff. Search is local and bounded;
it does not guarantee a globally optimal suite.
