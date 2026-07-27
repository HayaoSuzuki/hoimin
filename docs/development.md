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
