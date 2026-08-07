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

## Minimum supported Rust version

`workspace.package.rust-version` in `Cargo.toml` is the minimum supported Rust
version (MSRV). CI checks the complete locked workspace with that compiler.
Run the same gate locally before updating Rust dependencies:

```console
rustup toolchain install 1.88 --profile minimal
cargo +1.88 check --workspace --all-targets --all-features --locked
```

The committed `Cargo.lock` must remain compilable on the MSRV. When a dependency
update raises its compiler requirement, select the newest dependency release
that still supports the MSRV. If the project deliberately raises its MSRV,
update `workspace.package.rust-version`, the `msrv` CI job, its workflow
contract test, and this section in the same pull request. Stable CI remains
required in addition to the MSRV gate.

Before running the standalone wheel smoke script, you must build a release wheel
first with `uv run maturin build --release`. Alternatively, set `HOIMIN_WHEEL`
to the exact path of an existing wheel to test. The script only selects and
tests an existing artifact; it does not build one.

## Reproduce randomized Rust test order

CI supplements the stable cross-platform suite with Rust's standard nightly
test harness in randomized order:

```console
rustup toolchain install nightly-2026-07-27 --profile minimal
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

The harness prints the generated seed. Replay a failing order exactly with:

```console
cargo +nightly-2026-07-27 test --workspace -- \
  -Z unstable-options --shuffle-seed <SEED>
```

The nightly job supplements rather than replaces the stable Ubuntu, Windows,
and macOS test jobs.

## Shutdown deadline invariants

Once total timeout, cancellation, or a fatal failure starts shutdown, the
first cause and its absolute shutdown deadline are immutable. Total timeout is
anchored to the original run deadline plus the fixed two-second grace;
cancellation and fatal failure receive the same grace from their first
observation. Later stop signals or failures must not restart or extend that
budget. Every cancellable post-stop scheduler wait, completion receive, process
drain, blocking-I/O drain, resource close, and internal metrics finalization
must use the same deadline.

At grace expiry, accept already-buffered completions before aborting Tokio task
wrappers so returned workspace ownership is recovered when possible. Do not
claim cleanup, session completion, or `run_finished` unless its completion was
accepted before expiry. Resource ownership still held by the shell is moved to
a detached blocking cleanup rather than leaked; that cleanup may finish after
the run future returns and is never reported as accepted. Tokio cannot cancel
an already-running `spawn_blocking` operation: aborting its wrapper detaches
that operation. The CLI exits after reporting the infrastructure failure,
while a library caller may observe the detached operation finish later. This
deadline cannot preempt
an arbitrary synchronous `Write`: report output and its flush run inline, so a
blocked caller-provided writer can delay the run future and even the expiry
diagnostic. The bounded-return invariant therefore assumes synchronous output
writes make progress.

## Extending collection and structural mutations

The Rust analyzer keeps token-local mutations in its token scanner and adds an
AST candidate pass for calls, literals, subscripts, and slices. Both passes
emit the same candidate form and use the shared selection, profile filtering,
deduplication, source ordering, and candidate-limit pipeline. Keep new
structural rewrites to one contiguous AST span; build the replacement from the
original source text so nested expressions, comments, and spelling are
preserved.

Raw-token replacements are restricted to the AST-proven token-start allowlist
recorded by `AstFacts`. It records only the operator spellings in the precise
AST gaps for their supported roles, such as comparisons, boolean and binary
expressions, unary expressions, augmented assignments, boolean literals, and
`break`/`continue`. A token with an ambiguous grammatical role is skipped
conservatively: matching text alone is never enough to make it a candidate.
Annotation-span exclusions remain in effect, and this gate does not change the
existing selection, profile filtering, deduplication, source ordering, or
candidate-limit behavior.

Bare builtin calls (`any`, `all`, `list`, `tuple`, `set`, `frozenset`, `min`,
`max`, `sorted`, and `reversed`) are suppressed if the matching name is bound
anywhere in the file. Bindings include imports, assignments, definitions, and
parameters. This deliberately conservative rule avoids mutating a shadowed
callable; qualified builtin calls are not candidates. Method mutations are
syntax-directed and do not infer receiver types.

The supported structural shapes are exact: `append(value)` ↔
`extend([value])` only when the inverse list literal has one non-starred
element; `mapping.get(key)` ↔ `mapping[key]` only for a simple name or
attribute receiver, one positional key, and load context; `sort()` ↔
`reverse()` only with no arguments; and `sorted(value)` ↔ `reversed(value)`
only with one positional argument and no keywords. Calls with unsupported
keywords, star arguments, defaults, trailing commas where a rewrite would be
ambiguous, complex mapping receivers, or target contexts are skipped.

Boundary mutations likewise use only load-context subscripts. An index must be
a plain decimal integer literal: emit `+1`, and also `-1` when positive.
For a slice, plain decimal start, stop, and step literals may move to adjacent
valid values, except a step mutation to zero. Negative, empty, non-decimal, and
expression bounds are excluded. Comprehensions, assignment/delete targets,
the `append`/`pop` pair, and set literals wrapped as `frozenset(...)` are not
supported transformations.

Regression tests should use `apply_candidate_and_reparse` to replace the
candidate's one span in its source and verify
`ruff_python_parser::parse_module` accepts the result. This is a test-only
invariant; production does not parse each candidate separately. Keep exact
candidate/replacement assertions alongside this parse-preservation check.

Run focused analyzer tests while changing these rules:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::shadowed_collection_builtins_are_not_mutated_as_calls -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::collection_calls_and_literals_emit_exact_parseable_candidates -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::structure_calls_emit_exact_parseable_candidates -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::structure_index_neighbor_mutates_decimal_load_indices_only -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::structure_slice_neighbor_mutates_decimal_bounds_without_zero_steps -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests
```

## Extending Python exception mutations

Python exception candidates are collected by the Rust analyzer's
`ExceptHandler` AST pass and then use the shared selection, profile, source
ordering, deduplication, and candidate-limit pipeline. The default
`exception_type_pair` operator replaces only simple, unqualified handler names
with a curated counterpart: `ValueError`/`TypeError`, `KeyError` with
`IndexError` and `AttributeError`, `FileNotFoundError`/`PermissionError`,
`ConnectionError`/`TimeoutError`, `ImportError`/`ModuleNotFoundError`, and
`ZeroDivisionError`/`OverflowError`.

Exception names are suppressed when the file may bind the name through an
assignment, import, parameter, comprehension, match capture, or `except ... as`
target. This file-wide shadowing policy is intentionally conservative and does
not attempt scope-sensitive inference. Qualified and dynamic handler types,
`except*`, and tuple members outside the curated built-in name set are skipped.

The five structural operators in `exception_risky` are explicit-only:
bare-handler insertion/removal, the `Exception`/`BaseException` boundary, and
curated tuple add/remove rewrites. Tuple candidates use parser token ranges and
the original source text so commas, comments, trailing commas, and line endings
remain intact and every replacement can be reparsed. The BaseException boundary
can change handling of `SystemExit`, `KeyboardInterrupt`, and `GeneratorExit`,
so it must not be added to the default selection. Exception mutation currently
covers `except` clauses; `raise` expressions are a separate future extension.

When changing these rules, keep exact candidate and replacement assertions next
to `apply_candidate_and_reparse` checks. Run the focused tests before the full
analyzer module:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_type_pair_candidates_are_curated_and_syntax_directed -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_risky_candidates_require_explicit_selection_and_reparse -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_bindings_are_conservative -- --exact
```

## Extending plan ranking

Plan manifests use schema version 2 and ranking rule version 3. The schema
version describes the manifest's serialized shape; the ranking rule version
describes the category and scoring semantics used to order its candidates.
Change the ranking rule version whenever those semantics change, even when the
manifest schema itself does not.

Every canonical mutation operator is exhaustively assigned to exactly one
fixed-score category:

- `high_value_control` (100): comparisons, membership and identity tests,
  boolean operations, `not`, boolean literals, and `break`/`continue`.
- `exception_handling` (90): safe and risky exception-handler mutations.
- `behavioral` (80): collection and structural behavior mutations.
- `arithmetic` (70): arithmetic, unary-sign, and bitwise mutations.
- `type_annotation` (50): type-annotation mutations.

Keep the `MutationOperator` match exhaustive instead of adding a fallback arm.
This makes a new operator fail compilation until its ranking category has been
chosen. Unknown external operator strings remain invalid and do not acquire a
generic category.

`plan::create` validates its completed manifest with the same header validator
used by `verify` before returning it. Preserve this output-boundary invariant:
generated plans must satisfy schema, source-record, ranking, candidate-ID, and
normalized-path checks rather than deferring an internal inconsistency until a
later verification command.

## Test provenance comments

Use `// pins: issue #NNN` only when an assertion intentionally preserves a
surprising policy or a former defect whose expected result is not self-evident.
Do not annotate ordinary exact assertions.

Place the comment immediately before the assertion or operation whose outcome
needs that context. Prefer the concrete former-defect issue over an umbrella or
property-testing issue. Do not annotate generators, fixture constructors, or
routine schema values, and do not backfill unrelated tests mechanically.

## Verify delegated cgroup v2 CI

CI runs `linux-cgroup-v2-hard` only for a push to `main` when the repository
variable `HOIMIN_CGROUP_V2_DELEGATED` is `true`. The matching self-hosted runner
must be online and carry all of these labels: `self-hosted`, `linux`, `x64`,
and `cgroup-v2-delegated`.

Find the main-push run and inspect its jobs and delegated log:

```console
gh run list --workflow CI --event push --branch main
gh run view <RUN_ID>
gh run view <RUN_ID> --job <JOB_ID> --log
```

Successful hard-backend evidence requires `linux-cgroup-v2-hard` to complete
and its `Require delegated cgroup v2 hard tests to run` log to contain no
`SKIP:` marker. If no online idle runner matches every label, GitHub leaves the
job queued until a matching runner becomes available and fails it after 24
hours. Check the repository or organization Actions runner page for runner
status and labels; do not replace the job with a hosted or best-effort runner.

After changing the Rust analyzer or its tests, use the bounded workflow below
to collect focused evidence. For the required complete Rust inventory, run
`cargo mutants --workspace` as described in the Rust mutation testing section.

## Focused 30-minute Rust mutation workflow

Use `tools/focused_mutation.py` to collect bounded evidence about the
highest-ranked Rust changes. It requires `cargo-mutants` 27.1.0. Run it from
the repository worktree and put its artifacts outside the repository:

```console
output_dir="$(mktemp -d /tmp/hoimin-focused-run.XXXXXX)"
uv run --frozen python tools/focused_mutation.py \
  --budget 30m \
  --base origin/main \
  --output "$output_dir"
```

For a fixed path in automation, the equivalent output argument is
`--output /tmp/hoimin-focused-run`. Do not name the output directory with a
`mutants.out` prefix.

By default, the tool discovers eligible Rust files changed from `--base` and
ranks their cargo-mutants inventory. Repeat `--file PATH` and `--symbol NAME`
to explicitly focus discovery; either selector can be supplied more than once.
Add `--iterate` only when deliberately reusing cargo-mutants' prior caught and
unviable results during test development. The 30-minute budget reserves the
last five minutes for checkpointing and reporting, so it stops starting
mutations at that boundary.

`run.json` is checkpointed throughout. `run.json` remains the recoverable
machine-readable source of truth. Per-command arguments, stdout, and stderr are
also retained below the same output directory. `report.md` is generated or
refreshed during finalization as the human-readable summary. The tool attempts
finalization after a timeout, handled interruption, baseline failure, or tool
error, but `report.md` may be absent after an abrupt unhandled process
termination before finalization. In that case, recover from `run.json` and the
command artifacts. Treat incomplete output as a partial report: candidates
marked `not_run`, `pending`, `timeout`, `unviable`, or `error` remain
unverified.

Exit code `0` means the run reached `completed` or `budget_exhausted`; the
latter is an expected bounded result, not evidence that every candidate ran.
Exit code `130` means interruption. Exit code `2` means a configuration,
baseline, cargo-mutants, command, or reporting failure. A killed mutant is
evidence that the selected test command detects that change. A survivor is not proof of a bug.
Manually classify each survived result by inspecting the exact mutation,
relevant production behavior, tests, and recorded command artifacts.
Also investigate timeouts, unviable mutants, errors, and unverified candidates
instead of treating them as passes.

To measure how much a focused run reduced the candidate set, pass a compatible
previous full `run.json` with `--prior-inventory PATH`. The report then compares
the focused candidate count with that inventory; without it, the reduction
ratio is explicitly unmeasured. This comparison does not make a focused run a
complete inventory.

Focused results guide short test-improvement loops, but release evidence still
requires the full command below. Run `cargo mutants --workspace` after the
focused work, and do not use `--iterate` for the required final inventory.

## Rust mutation testing

Install `cargo-mutants` locally, then use the full command to discover every
outcome. While adding tests, `--iterate` reuses previously caught and unviable
outcomes; do not use it for the required final check.

```console
cargo install --locked cargo-mutants --version 27.1.0

# Discover all remaining outcomes.
cargo mutants --workspace

# While adding tests, reuse caught and unviable outcomes from the prior run.
cargo mutants --workspace --iterate

# Required final check: do not use --iterate here.
cargo mutants --workspace
```

The required reproducible priority check was validated with `cargo-mutants`
27.1.0:

```console
cargo mutants --workspace --jobs 4 \
  --file crates/hoimin-core/src/machine.rs \
  --file crates/hoimin-core/src/target.rs \
  --file crates/hoimin-cli/src/cli.rs \
  --file crates/hoimin-cli/src/process/mod.rs \
  --re '(TryFrom<Command> for ParsedCommand>::try_from|parse_bytes|raw_config|ProcessHandler::run|ProcessStartGate::cancel|ProcessCancellation::cancel|RunState::accept_completion|RunState::schedule_read_or_finalize|targets_are_normalized|changed_is_normalized)'
```

`mutants.out/missed.txt` requires a behavior test unless the exact mutant is
equivalent. Resolve `timeout.txt`, a failed baseline, and tool errors;
`unviable.txt` is inconclusive. Each allowed exception is an anchored
complete-name `exclude_re` with a TOML reason comment. This workflow is local
and does not run in CI.
