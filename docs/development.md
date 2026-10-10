# Development

## Knowledge workflow

Start development work with [the OKF catalog](knowledge/index.md). Read the
relevant contracts and audit limitations, then inspect their sources and the
affected implementation. Update the corresponding OKF concepts in the same
change when behavior, design decisions, procedures, or audit evidence change.
Add a new concept when the work establishes a distinct reusable topic.

Follow [the OKF authoring and review procedure](okf-workflow.md) for update
criteria, source provenance, and validation. Include consulted pages, updated
pages (or the reason no update was needed), and checks performed in the PR or
final handoff. These steps also apply when no personal `create-okf` skill is
installed.

## Resource policy at the core boundary

Select the process backend before constructing `RunState`. Pass its
`ProcessHandler::resource_control()` description as the final argument to
`RunState::new`, `with_fingerprint`, `with_candidate_filter`, or
`with_ordered_candidate_filter`. `RunStarted::minimal` also requires an explicit
`ResourceControl` as its final argument. Rust callers using the older constructor
signatures must supply this argument; there is no inferred or default backend.
Tests that supply a policy exercise core reporting, not native OS enforcement.
The report schema retains its existing `mode` and singular `mechanism` fields.

## Local quality gate

Install [prek](https://github.com/j178/prek) and enable the Git pre-commit hook
once per checkout:

```console
uv tool install prek
prek install
prek run --all-files
```

The hooks in [`prek.toml`](../prek.toml) run the same Rust formatting and Clippy
checks as CI, including the vendored parser. They use Cargo from `PATH` and
the toolchain (with rustfmt and Clippy) pinned in `rust-toolchain.toml`.
Formatting is checked without rewriting files; run `cargo fmt --all` or
`cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml` to fix it.

Hooks run when staged changes include Rust sources, Cargo manifests or lockfiles,
Rust toolchain, Cargo, rustfmt or Clippy configuration, or `prek.toml`.
Documentation-only changes skip them. Each hook checks its entire workspace
once, in sequence. The separate `fuzz/` workspace uses the explicit checks
documented below and does not trigger these hooks.

Run `prek validate-config prek.toml` after changing the hook configuration.
Use `prek run --all-files` for all four Rust checks, or
`prek run cargo-clippy --all-files` for the workspace Clippy check alone.

Run the quality gates locally with the same commands used in CI:

```console
uv sync --frozen --group fuzz --no-install-project
uv run --frozen --no-sync ruff format --check .
uv run --frozen --no-sync ruff check --no-fix .
uv run --frozen --no-sync ty check
cargo fmt --all -- --check
cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
uv run --frozen pytest
uvx maturin build --release
uv sync --frozen --no-install-project
uv run --frozen --no-sync python tests/wheel_smoke.py
```

## CI selection and workflow validation

See [CI selection and workflow validation](ci.md) for path classification,
the aggregate required check, pinned actionlint/ShellCheck/zizmor commands,
documented inline exceptions, and before/after measurements.

## Dependency vulnerability audits

The `Dependency audit` workflow checks committed dependencies on pull requests,
merge queues, pushes to `main`, and daily at 02:23 UTC. It can also be started
manually. Python and Rust audits run independently, and a detected vulnerability
or a failed advisory lookup fails the corresponding job.

Python auditing uses uv 0.12.13 with the `audit-command` preview feature. It audits
all extras and dependency groups, including development and fuzz dependencies,
for Linux, macOS and Windows using separate target-platform selections on a
Linux runner. These are dependency checks, not execution tests on those systems.
`--frozen` preserves the committed `uv.lock`. Rust auditing uses cargo-audit 0.22.2
and checks both the root workspace and the independent fuzz workspace against
the current RustSec database. Known vulnerabilities fail the audit; cargo-audit's
informational warnings retain their default severity.

Run the same checks locally:

```console
uv audit --frozen --preview-features audit-command --python-platform linux
uv audit --frozen --preview-features audit-command --python-platform macos
uv audit --frozen --preview-features audit-command --python-platform windows
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --file Cargo.lock
cargo audit --file fuzz/Cargo.lock
```

Audits require access to the advisory services and detect known dependency
vulnerabilities; they do not prove the absence of security defects. Renovate
continues to propose dependency and lockfile updates separately. Review and fix
findings instead of adding blanket exclusions or allowing failed audits to pass.

## Property-based boundary tests

Proptest is already a development dependency in both Rust crates. The
`boundary_property_` tests complement fixed examples and Lean correspondence
cases with generated encoding boundaries, mixed-newline selections, and
streaming copy/hash/comparison inputs. They run in the normal `cargo test
--workspace` suite; no separate CI service or dependency is needed.

Run just these properties, or increase their case count with a reproducible seed:

```console
cargo test -p hoimin-core --test source_encoding_properties
cargo test -p hoimin-cli --lib boundary_property_
PROPTEST_CASES=1024 PROPTEST_RNG_SEED=20260926 cargo test -p hoimin-core --test source_encoding_properties
PROPTEST_CASES=1024 PROPTEST_RNG_SEED=20260926 cargo test -p hoimin-cli --lib boundary_property_
```

The default is 256 successful generated cases per property. Keep the default
random seed in ordinary CI runs to explore new combinations. On failure,
proptest shrinks the input (these properties cap shrinking at 2048 iterations),
prints the failing input, and persists a regression seed. Commit real regression
files alongside the fix and retain a literal regression test when the discovered
case expresses an important contract. Replay uses the same input strategy;
changing the strategy can change what a persisted seed generates. See the
[proptest failure persistence documentation](https://proptest-rs.github.io/proptest/proptest/failure-persistence.html).

Input sizes are bounded. Stream tests use arbitrary small byte vectors and
repeated generated patterns around one and two 64 KiB buffers. Their readers use
independent fixed fragment sizes, optional single interruptions before progress,
and terminal I/O errors; they do not explore every possible read schedule or OS
filesystem race. Properties compare against materialized scalar/line observations,
whole-slice equality, and one-shot hashes. They are finite tests, not proofs.

## Coverage-guided fuzzing

`fuzz/` is a separate Cargo workspace with its own committed lockfile. It depends
on `hoimin-core` by path; libFuzzer does not enter the shipping workspace or the
ordinary `cargo test --workspace` run. The targets supplement proptest:

- `source_encoding`: arbitrary bytes exercise encoding declarations and malformed
  input. Successful decodes must preserve bytes on re-encoding and match scalar
  boundary maps. Explicit Latin-1 and valid UTF-8 variants must decode successfully.
- `source_index`: UTF-8 text is checked against a sequential scalar walk for every
  byte offset, including split scalars, EOF, mixed LF/CRLF/CR, and a leading BOM.
- `candidate_validation`: constructs valid UTF-8 and Latin-1 candidates with
  independently calculated spans and coordinates, then checks rejection of nine
  single-field corruptions and unencodable Latin-1 replacements. The first five
  input bytes choose the codec and two little-endian scalar boundary indices;
  the remaining bytes supply the source payload.
- `analyzer_protocol`: parses arbitrary JSONL, checks observed summary counts,
  and constructs valid records to exercise wrong effect IDs, missing/mismatched
  summaries, decreasing candidate offsets, records after summary, and byte limits.
- `python_analyzer`: runs the production Python analyzer with all mutation
  operators and either the full or focused profile (selected by input length
  parity). Checks immediate cancellation, candidate validation, ordering, the
  32-candidate cap, and prefix preservation when that cap is reduced to one.
  Syntax errors and supported depth-limit errors are valid outcomes.
- `report_sequence`: deserializes arbitrary JSON event lines, round-trips valid
  events, and feeds accepted events into the report lifecycle validator. It also
  constructs a complete run with one mutant for every input and verifies result
  classification, terminal-state rejection, and recovery after a rejected result.
- `target_resolution`: exercises arbitrary path normalization and constructs
  bounded discovery inventories with file, line, and symbol selectors. It checks
  exact resolution against an independent reference model, line-index membership,
  and changed-line normalization and intersection.

The analyzer protocol and Python analyzer targets compile production analyzer
modules directly using `#[path]`, so no fuzz-only API is added to the shipping
crate. Keep their dependency declarations in `fuzz/Cargo.toml` aligned with
`crates/hoimin-cli/Cargo.toml` and retain the root workspace's vendored Ruff
parser patch in the fuzz workspace.
These are in-memory checks; filesystem discovery, file I/O, subprocesses, and
mutation execution are outside their scope. The five structured targets reject
inputs larger than 4096 bytes to bound per-input work, including during artifact
replay.

Install cargo-fuzz and the pinned nightly (the locked libfuzzer-sys version
requires a C++17 compiler).
These commands run from the repository root. The normal Rust toolchain stays
unchanged:

```console
cargo install cargo-fuzz --version 0.13.2 --locked
rustup toolchain install nightly-2026-10-08 --profile minimal
cargo +nightly-2026-10-08 fuzz build
for target in source_encoding source_index candidate_validation analyzer_protocol python_analyzer report_sequence target_resolution; do
  mkdir -p "fuzz/corpus/$target"
  cargo +nightly-2026-10-08 fuzz run "$target" "fuzz/corpus/$target" "fuzz/seeds/$target" -- -max_total_time=30 -max_len=4096 -timeout=5 -rss_limit_mb=1024 || break
done
```

AddressSanitizer, debug assertions, and overflow checks use cargo-fuzz's defaults.
The first corpus directory receives newly discovered inputs; the second contains
committed seeds, including non-UTF-8 bytes. Keep that order so fuzzing does not
write generated inputs into `seeds/`. Corpus, artifacts, coverage, and build
outputs are ignored. Increase `-max_total_time` for longer local runs. The limit
applies to fuzzing time, not compilation; `-timeout` bounds an individual input.

Replay and minimize a reported failure, using its actual artifact path:

```console
cargo +nightly-2026-10-08 fuzz run source_encoding fuzz/artifacts/source_encoding/crash-<hash>
cargo +nightly-2026-10-08 fuzz tmin source_encoding fuzz/artifacts/source_encoding/crash-<hash>
```

Retain the minimized input in the target's `seeds/` directory and add a normal
Rust regression test with the fix. A bounded successful run is evidence only for
the inputs executed. The Python target checks candidate structure and retention,
not whether every replacement has the intended Python runtime semantics. Cookie
error classification and OS/process behavior are not exhaustively checked.

Formatting and lint checks for the separate workspace are explicit:

```console
cargo fmt --manifest-path fuzz/Cargo.toml -- --check
cargo clippy --locked --manifest-path fuzz/Cargo.toml --bins -- -D warnings
```

See the [cargo-fuzz documentation](https://rust-fuzz.github.io/book/cargo-fuzz.html).

### Bounded CI fuzzing

The automatic CI workflow runs `Fuzz (bounded)` on Linux after `quality`, in
parallel with the existing test jobs. Its **11-minute job timeout includes
setup, compilation, fuzzing, and artifact/cache handling**. The longest test
jobs in main runs [36142375064](https://github.com/tokyogas-tech/hoimin/actions/runs/36142375064),
[36142944345](https://github.com/tokyogas-tech/hoimin/actions/runs/36142944345), and
[36158455427](https://github.com/tokyogas-tech/hoimin/actions/runs/36158455427)
took 11m18s, 11m35s, and 12m09s: 11 minutes is approximately 91–98% of those
durations. This is a fixed budget; revisit it when normal test durations change.

The first step sets a ten-minute active deadline, leaving approximately one
minute for uploading diagnostics and saving small caches. `tools/ci_fuzz.py`
deducts elapsed setup time, installs the optional Python dependencies, builds
all fuzz targets, tests the generator, and generates 50 examples with each
hypothesmith strategy. It divides the remaining time among all targets in
`fuzz/Cargo.toml`, reserving ten seconds per target for startup and shutdown.
An exhausted budget, a failed command, or a target timeout fails the job; an
unexecuted target is never counted as a pass. Subprocess timeouts kill the
process group, including compiler and fuzzer children.

The cargo-fuzz executable and discovered corpus are cached. Large compiled
target directories are not cached, keeping post-job work small. CI builds use
16 codegen units to reduce compilation time. Both generation and fuzzing use
a seed derived from the workflow run ID. The `fuzz-report` artifact retains
per-stage command logs, a JSON summary with timings and completed targets, and
any crash inputs for seven days. Artifact upload runs even after failure,
although cancellation or the hard job timeout can interrupt it. Use the saved
crash input for replay; the seed alone does not reproduce a time-bounded run
against an evolving corpus.

### Scheduled fuzzing

The `fuzz.yml` workflow runs every day at 18:17 UTC and gives each target 60
seconds of mutation time. A manual run accepts 30, 60, or 300 seconds per
target:

```console
gh workflow run fuzz.yml --ref main -f seconds=300
```

The scheduled workflow uses the same pinned nightly, cargo-fuzz version,
Hypothesmith generators, seeds, and resource limits as bounded CI. It passes a
fixed per-target duration to `tools/ci_fuzz.py` instead of dividing an
ten-minute CI budget. A 55-minute active deadline leaves five minutes for
cache and artifact steps before the workflow's 60-minute limit.

Each run restores the latest corpus for its branch and saves the enlarged
corpus under a new cache key. GitHub retains command logs, the JSON summary,
and crash inputs as `fuzz-diagnostics` artifacts for 14 days. The save and
upload steps run after a fuzzing failure; cancellation or the workflow timeout
can still interrupt them.

The initial local validation found a parser panic on a nested unterminated
f/t-string inside a format specification. It is fixed in the vendored parser;
the minimized inputs are seeds `fstring-format-spec-recovery` and
`tstring-format-spec-recovery`, and `foreign_middle_token_in_a_format_spec_is_invalid_syntax`
in `crates/hoimin-cli/tests/rust_analyzer.rs` is the ordinary regression test.
GitHub Actions execution and cold Linux build timing have not yet been verified.

### Generate Python inputs with hypothesmith

The optional `fuzz` dependency group pins hypothesmith 0.3.3. The lockfile also
records Hypothesis, LibCST, and Lark; these dependencies are not installed by the
ordinary dev-only sync. Run from the repository root:

```console
uv sync --frozen --group fuzz --no-install-project
uv run --frozen --no-sync python tools/hypothesmith_corpus.py --examples 100 --seed 20260926 --strategy grammar
uv run --frozen --no-sync python tools/hypothesmith_corpus.py --examples 100 --seed 20260926 --strategy libcst
cargo +nightly-2026-10-08 fuzz run python_analyzer fuzz/corpus/python_analyzer fuzz/seeds/python_analyzer -- -max_total_time=30 -max_len=4096 -timeout=5 -rss_limit_mb=1024
```

The generator writes to `fuzz/corpus/python_analyzer` by default; `--output DIR`
selects another directory. `grammar` uses `from_grammar()` and `libcst` uses
`from_node()`, both with automatic complexity targeting enabled. `--examples`
is the Hypothesis example budget, not a promised number of distinct files.
Whitespace-only, invalid, and oversized examples are counted and skipped; repeated
inputs are deduplicated by SHA-256. UTF-8 bytes and physical newlines are preserved.
`--max-bytes` defaults to 4096 and accepts values from 1 through 4096, matching the
Rust target's limit. This bounds saved inputs, not generator memory or wall time.

Generated source is compiled for syntax validation by CPython, never executed.
The tool reports JSON on stdout with the seed, strategy, limits, Python/library
versions, and written/existing/skipped counts. Exit 0 means at least one usable
input was written or already present; exit 1 means no usable input was collected;
invalid options or missing optional dependencies exit 2. Generation errors remain
failures, even if earlier examples were already saved.

Reuse the same seed, options, interpreter, and locked dependencies for replay;
generation is not stable across dependency upgrades. Change the seed to explore
different inputs. Corpus files and Hypothesis caches are ignored; promote a
discovered regression to a committed seed and a normal regression test.

Both strategies were exercised on CPython 3.14.7 with Hypothesis 6.168.1,
LibCST 1.9.0, and Lark 1.3.1. That does not establish coverage of every Python 3.14
syntax form. The existing Rust fuzz target permits syntax diagnostics because
libFuzzer can turn a valid seed into invalid Python; this integration is not an
assertion that CPython and the Rust parser accept identical languages.

Run generator tests with the optional dependencies enabled:

```console
uv run --frozen --group fuzz --no-install-project pytest tests/test_hypothesmith_corpus.py
```

Without that group, only the two real-generator integration cases are skipped;
the byte-preservation, size-boundary, syntax-rejection, and CLI validation tests
still run. See the [hypothesmith project](https://github.com/Zac-HD/hypothesmith).

## Python formatting, lint, types and tests

Ruff, ty and pytest are development dependencies managed by `uv.lock`. For Python
checks alone, `uv sync --frozen --group fuzz --no-install-project` installs the tools
and the optional fuzz generators without building the Rust executable. Run
`uv run --frozen --no-sync ty check` and `uv run --frozen --no-sync pytest` in that
environment; the full quality gate above also builds and validates the wheel.

To apply formatting or safe lint fixes explicitly:

```console
uv run --frozen --no-sync ruff format .
uv run --frozen --no-sync ruff check --fix .
```

Ruff targets Python 3.14, uses an 88-column formatter, and enables `ALL` rules.
The exclusions in `pyproject.toml` follow the supplied kraken-hub policy for
docstrings, assertions, formatter conflicts and selected style rules. Additional
per-file exceptions allow standalone script modules and literal expected values in
test assertions. Required subprocess execution, CLI output and existing orchestration
complexity have individual documented suppressions; adding a suppression requires
the same explanation. Unused imports are reported rather than automatically
removed, and unsafe fixes are not enabled.

Maintained Python files under `tests/`, `tools/`, `formal/HoiminOracle/tools/`
and `crates/hoimin-cli/tests/support/` are checked. Vendored code, historical
documentation/audits, generated output, worktrees and mutation fixture projects
are excluded so formatting cannot rewrite test inputs or recorded evidence.

The same maintained directories are included in ty's type checking. Run
`uv run --frozen --no-sync ty check` from the repository root. The configuration
targets Python 3.14 and all platforms, so availability of platform-specific APIs
must be checked before use. All ty rules are enabled as errors, including checks
for missing generic arguments, unsound assignments and returns, and unused or
blanket ignore comments. Warnings also fail the check. Strict equality semantics
and strict generic narrowing are enabled; `type: ignore` comments cannot suppress
ty diagnostics. See the [ty configuration reference](
https://docs.astral.sh/ty/reference/configuration/).

JSON and YAML inputs need checked types at their boundaries. Preserve precise
record types through helpers and tests instead of using `Any` or casts to bypass
diagnostics. Ruff's existing annotation rules complement ty by requiring function
annotations. This does not prove that every third-party or dynamically loaded
value has a static type; runtime checks are still needed where data enters typed
code. Mutation fixture projects are excluded because some deliberately contain
type errors, and their acceptance tests run their own checker configuration.

Pytest uses strict mode and collects from `tests/`, excluding fixture projects.
Tests use module-level functions, plain `assert`, `pytest.raises`, and
`pytest.mark.parametrize` for independent cases. Use `tmp_path` and fixtures for
test setup. Use the `mocker` fixture from [pytest-mock](
https://pytest-mock.readthedocs.io/en/latest/usage.html) for mocks and patches;
pytest restores patches after each test. Use `monkeypatch` for environment
variables and configuration values.
Rust end-to-end tests run the sample project's pytest tests under
`tests/fixtures/`. The boundary-contracts job also runs its Python checks with
pytest from the frozen uv environment.

Automatic Linux and manual non-Linux quality jobs run the same read-only Ruff
and ty checks. Their wheel smoke jobs execute the Python suite with pytest before
building the wheel. See the [design, implementation plan and review record](
superpowers/plans/2026-09-24-python-quality.md).

### Coverage and randomized test order

The dev group also includes `pytest-cov` and `pytest-randomly`. The latter
automatically randomizes test order and seeds Python's random generator. Use
pytest without `-q` to see the chosen seed in its session header. To reproduce
an order-dependent failure, pass the reported seed or reuse the last local seed:

```console
uv run --frozen pytest --randomly-seed=12345
uv run --frozen pytest --randomly-seed=last
```

To measure branch coverage of the Python tools:

```console
uv run --frozen pytest --cov=tools --cov-branch --cov-report=term-missing
```

Coverage is opt-in and has no minimum percentage gate. This command measures
`tools/` in the pytest process; it does not measure the Rust implementation or
every subprocess. Generated coverage data and common report outputs are ignored
by Git. See the [pytest-cov configuration guide](
https://pytest-cov.readthedocs.io/en/latest/config.html) and
[pytest-randomly usage](https://github.com/pytest-dev/pytest-randomly#usage).

## CI platform execution policy

Pull requests and pushes to `main` run the repository gates on Linux. Linux CI
consumes runner capacity; it is the platform deliberately kept automatic.
Windows and macOS validation lives in the separate `Manual non-Linux CI`
workflow and starts only through `workflow_dispatch`. Its results are not a
dependency or merge condition for automatic Linux CI.

After the workflow exists on the default branch, run it once against the final
ref that needs non-Linux evidence:

```console
gh workflow run non-linux-ci.yml --ref <REF>
```

Do not dispatch it for intermediate commits. The tag-triggered release workflow
is separate from this validation policy.

## Pinned workflow actions

Workflow files are the source of exact GitHub Action commit pins. Keep each
remote action on a full 40-character SHA. Add version comments when updating
pins so dependency tooling can identify the release. Contract tests validate
the pin format and expected action identity,
then check job behavior independently of the particular commit. Do not copy
current SHAs into test expectations. Changes to permissions, inputs, commands,
shells, environments and the artifact-only release policy still require their
existing contract checks. See the [combined update plan and review evidence](
superpowers/plans/2026-09-24-actions-update-contracts.md).

The release workflow also owns the exact `maturin-version` input. Both platform
builds must use the same canonical `vMAJOR.MINOR.PATCH` pin. Release contract tests
validate this format and agreement without duplicating the current version;
all other inputs and publication restrictions remain exact. See the
[maturin update plan and validation](
superpowers/plans/2026-09-24-maturin-release-contract.md).

## Pinned Rust toolchain

`rust-toolchain.toml` is the source of the exact stable Rust version used for
repository development and blocking CI, including rustfmt and Clippy. Run `rustup toolchain
install` from the repository root to install that exact declaration. The
weekly latest-stable canary reports upcoming compatibility issues without
changing the blocking toolchain.

The workflow contract test requires a complete `major.minor.patch` stable pin,
the minimal profile, and both components. It does not duplicate the current
version number, so a patch or minor update does not require changing the test.
Wheel package versions must still match the package metadata exactly.

Update the pin and minimum supported Rust version together in one PR. Hoimin
follows the latest stable release after validation; development, blocking CI,
and wheel builds use that exact version. The floating `stable` channel is only
used by the weekly canary. A new release does not silently change a historical
checkout's build toolchain.

## Minimum supported Rust version

`workspace.package.rust-version` in `Cargo.toml` must equal the exact stable pin
in `rust-toolchain.toml`. The workflow contract tests reject a mismatch. There
is no separate old-compiler CI job. Source builds require the declared version
or newer; users installing a compatible prebuilt wheel do not need Rust.
Dependencies are not held back to preserve support for an older compiler.

For each stable update:

1. Check the [official release list](https://blog.rust-lang.org/releases/) and
   update both declarations in the same PR, including patch versions. Install
   with `rustup toolchain install` and check the locked workspace with
   `cargo check --workspace --all-targets --all-features --locked`.
2. Run the quality gates above, including the vendored parser and contracts.
   Validate all supported wheel builds and smoke tests through the release
   workflow's PR preview; dispatch non-Linux CI once against the final ref.
3. Check the pinned nightly can still build the workspace. When updating it,
   use an available recent `nightly-YYYY-MM-DD` and update both fuzz workflows,
   `tools/ci_fuzz.py`, the shuffle job, cargo-fuzz cache keys, workflow contracts,
   these command examples, and the required check name in
   `infra/github/Pulumi.yaml`. Run bounded fuzzing and shuffled workspace tests.
4. Review the canary result and the source-build compatibility change before
   merging. Do not publish a release merely to test the toolchain update.

Renovate's [rust-toolchain manager](https://docs.renovatebot.com/modules/manager/rust-toolchain/)
detects exact stable pins. The existing repository-wide `minimumReleaseAge` of
seven days applies to these update PRs. Maintain that waiting period for routine
updates; an urgent manual update still requires the checks above. A toolchain
update PR is not assumed to update Cargo's minimum version: update `rust-version`
on the same branch before merging. The equality contract prevents accepting a
one-sided update. Nightly dates in scripts and YAML remain a coordinated manual
update rather than a daily floating-channel upgrade.

Install the smoke script's dependencies with `uv sync --frozen --no-install-project`,
then use `uv run --frozen --no-sync` to run it. This avoids rebuilding hoimin or
replacing the wheel being tested.

Before running the standalone wheel smoke script, you must build a release wheel
first with `uvx maturin build --release`. Alternatively, set `HOIMIN_WHEEL`
to the exact path of an existing wheel to test. The script only selects and
tests an existing artifact; it does not build one.

## Reproduce randomized Rust test order

CI supplements the stable cross-platform suite with Rust's standard nightly
test harness in randomized order:

```console
rustup toolchain install nightly-2026-10-08 --profile minimal
cargo +nightly-2026-10-08 test --workspace -- -Z unstable-options --shuffle
```

The harness prints the generated seed. Replay a failing order exactly with:

```console
cargo +nightly-2026-10-08 test --workspace -- \
  -Z unstable-options --shuffle-seed <SEED>
```

The nightly job supplements rather than replaces the automatic stable Ubuntu
jobs. Windows and macOS validation uses the manually dispatched workflow
described above.

## Lean state-machine oracle

The formal model covers effect completion, stopping, cleanup, and final-report
lifecycle semantics. Lean proves properties of that model; it does not prove
the Rust implementation. A Rust adapter separately drives the public
`RunState` and `transition` API with the Lean-generated corpus to check their
correspondence.

Run the gates from the repository root in this order:

```console
(cd formal/HoiminOracle && lake build)
(cd formal/HoiminOracle && lake exe generate -- --check corpus/state-machine.jsonl)
cargo test -p hoimin-core --test lean_oracle
HOIMIN_ORACLE_CASE=cleanup_is_emitted_once \
  cargo test -p hoimin-core --test lean_oracle \
  oracle_correspondence -- --exact --nocapture
```

The generated JSONL corpus is owned by Lean and must not be edited by hand.
New cases begin in report mode while their model/implementation boundary is
reviewed. Promoted strict cases are blocking; infrastructure errors always
fail.

The six follow-up audit models from 2026-09-15 also participate in the Lean
build, corpus freshness, and sensitivity gates. Their implementation gaps
remain in report mode; see the [promotion report](superpowers/reports/2026-09-15-followup-lean-promotion.md)
for model boundaries, corpus paths, and the unresolved issue ledger.

The Linux `Lean audit` CI job is configured to compile every package module
serially before building the aggregate library. It then runs every registered
corpus freshness check and each generator that exposes a sensitivity gate. The
workflow is the canonical list of package targets, corpus paths, and gate
commands; update its contract test whenever a library module or `lakefile.toml`
executable changes.

Every build and generator invocation runs alone through
`tools/lean_resource_guard.py`, with a 30-second wall-time limit, 2 GiB
aggregate RSS limit, and 250 ms sampling. Resource statistics are retained as
the `lean-audit-stats` workflow artifact even when a gate fails. The Lake package
uses `-j1` and `-DElab.async=false` to keep elaboration serial within each
process. Run local Lean checks serially with the same guard rather than starting
an unbounded aggregate build. Corpus freshness checks compare generated output;
regenerate through the corresponding `lake exe generate* -- --output ...`
command when an intentional model change requires it. Generated JSONL files
must not be edited by hand.

The earlier 171-command sequence passed from an empty build cache in a
one-CPU Linux aarch64 container with a hard 2 GiB limit and no swap. The longest
command took 13.198 seconds; peak aggregate RSS was 1,077,976 KiB. Budget
statistics take their exploration depth at runtime and run only for `--stats`,
so corpus checks do not initialize the exhaustive statistics search. The
statistics formulas, depth-six audit, proofs, and corpora remain unchanged.
The initial GitHub-hosted run exceeded the former 20-second limit while
building `ShutdownProofs` at 1,022,208 KiB peak RSS. The current 30-second limit
retains the 2 GiB memory bound; the local measurements above used 20 seconds.
The hosted follow-up passed all 171 commands. `ShutdownProofs` was the longest
at 24.625 seconds and also had the highest RSS at 1,033,644 KiB.

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

## Metrics destination permission

The owned blocking Preflight validates metrics output against resolved source
entries, explicit fingerprint inputs and session artifacts before the baseline.
Its completion returns a typed destination together with workspace ownership;
the shell accepts both before applying the core event. The finalizer uses only
that resolved path. A collision failure can otherwise return a modeled nonzero
result and reach finalization, so checking `run_result.is_ok()` cannot authorize
output. Cleanup completions preserve the preflight decision. A later baseline
failure retains authorized metrics; a Rust error retains `metrics.incomplete`.

Compare the parent directory and native entry name for rename replacement.
Distinct hardlinks and final symlinks remain separate output entries. Unix
inspection caches directory names and lazily indexes no-follow file identities
to resolve inexact spellings; file identity alone does not equate outputs. The
session artifact resolver supplies the canonical database, its companion files,
and the ownership directory used by SessionHandler on every platform. Protect
that literal directory entry and its resolved tree. Inspect the originally
configured database entry before preflight replaces the session path with its
canonical path. On Windows, also retain protection of companion names under the
configured basename when the final entry is an alias.

For prospective ASCII case aliases in one directory, query macOS pathconf,
Windows directory case information, or the ext4/f2fs casefold flag on Linux.
Unknown filesystem behavior, ambiguous entry aliases and prospective non-ASCII
comparisons with protected names withhold metrics and produce a late
`metrics.write` warning. Unsupported Windows trailing-dot, trailing-space,
stream and prospective short-name spellings follow that policy. Keep ordinary
directory or missing-parent output failures as warnings. Require an existing
resolved parent before granting write permission, even if a baseline might
create the directory later. A prospective ownership tree can still establish
a collision before this permission check. Preserve directory-only output syntax
before path normalization: a trailing separator or terminal `.` / `..` withholds
metrics, so normalization cannot turn a directory requirement into replacement
of its final symlink. These checks establish
preflight destination identity; they do not freeze the filesystem against
external renames during a run. Native platform tests must establish the path
semantics; abstract lifecycle oracles alone cannot do so.

## Analyzer timeout invariants

Plan creation and verification rediscovery apply the normalized
`--analyzer-timeout` as one absolute deadline for the complete discovery phase;
the deadline is not restarted for each target. Both plan entry points report
the same `plan.discovery: analyzer.timeout` diagnostic and exit 2. Plan
creation produces no manifest, and verification does not start the test
command.

Discovery runs as an owned blocking task with cooperative cancellation. When
the deadline expires, the async caller returns promptly and detaches an analyzer
that is already running; the detached task observes cancellation, stops, and
releases every resource it owns. Tokio cannot preempt synchronous work already
executing in that task. In particular, a source read blocked inside a system
call must return before cancellation can be observed and the detached task can
finish.

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
expressions, unary expressions, augmented assignments, boolean literals,
`True`/`False` singleton match patterns, and `break`/`continue`. The pattern
traversal admits boolean singletons inside nested patterns but excludes `None`,
wildcard and capture patterns, and string values. A token with an ambiguous
grammatical role does not produce a candidate: matching text alone never grants
eligibility.

Mapping-pattern boolean flips and complex separator flips are excluded when
only that edit would duplicate another literal key in the same mapping. A
per-mapping hash index models Python equality for the replacement domain:
boolean targets are exactly 0/1, while nonzero imaginary parts only compare
against complex keys. Zero-imaginary separator edits preserve equality and
remain eligible. Ordinary integer keys are never rounded for comparison;
integer real parts of complex literals use `num-bigint` conversion to match
Python construction, including radix spelling and ties-to-even rounding.
Float infinity and signed zero are handled separately from integer conversion
overflow, which cannot form a valid original complex pattern literal.
Nested mappings, value patterns, and dictionary expressions retain their own
eligibility. Integration tests compile public plan mutants with CPython and
check that import-only runs do not count invalid duplicate-key edits as kills.

Native Python operator syntax uses these token-local mappings:

- `**` becomes `*` (`binary_power`) and `@` becomes `*` (`binary_matmul`).
- `^` becomes `&` (`bitwise_xor`) and unary `~` becomes unary `+`
  (`bitwise_invert`).
- `**=` and `@=` become `*=` (`augmented_power` and `augmented_matmul`).
- `&=` and `|=` exchange spellings (`augmented_bitwise_and_or`), `^=` becomes
  `&=` (`augmented_bitwise_xor`), and `<<=` and `>>=` exchange spellings
  (`augmented_bitwise_shift`).

The binary, unary, and augmented-assignment AST roles admit these spellings only
in their operator positions. Decorators, keyword unpacking, annotations,
strings, and comments remain excluded.

`operator_function` is a default runtime selector for the Python 3.14 `operator`
callables (53 canonical names and 46 documented dunder aliases). The independent
`OperatorImports` index trusts only unique, unconditional module-level imports:
`import operator`, module aliases, and absolute `from operator import ...`
aliases. It does not alter the audited builtin resolver. Callable references are
eligible in both direct calls and higher-order uses such as `map(op.add, xs, ys)`.

| Callable names | Replacement |
| --- | --- |
| `eq` / `ne`, `lt` / `le`, `gt` / `ge` | Exchange each pair |
| `add` / `sub`, `mul` / `truediv`, `floordiv` / `mod` | Exchange each pair |
| `pow`, `matmul` | `mul` |
| `and_` / `or_`, `lshift` / `rshift` | Exchange each pair |
| `xor` | `and_` |
| `neg` / `pos` | Exchange |
| `abs` | `neg` |
| `index`, `inv`, `invert` | `pos` |
| `not_` / `truth`, `is_` / `is_not`, `is_none` / `is_not_none` | Exchange each pair |
| `iadd` / `isub`, `imul` / `itruediv`, `ifloordiv` / `imod` | Exchange each pair |
| `ipow`, `imatmul` | `imul` |
| `iand` / `ior`, `ilshift` / `irshift` | Exchange each pair |
| `ixor` | `iand` |
| `concat` / `iconcat`, `countOf` / `indexOf` | Exchange each pair |
| `getitem` | `contains` |
| `contains` | `(lambda container, item, /: item not in container)` |
| `setitem` | `(lambda container, key, value, /: None)` |
| `delitem` | `(lambda container, key, /: None)` |
| `call` | `(lambda target, /, *args, **kwargs: None)` |

For function pairs through a module alias, only the member token changes. A
dunder source keeps dunder spelling when the destination has a documented alias;
for example, `__not__` becomes `truth`. A from-import reference becomes
`__import__('operator').replacement` only when the builtin `__import__` has no
binding or namespace uncertainty. Lambda mutations replace the complete callable
reference. All forms retain argument text, order, count, and evaluation frequency;
lambdas intentionally suppress the underlying operation. Callable identity and
introspection are not preserved. Helpers `attrgetter`, `itemgetter`,
`methodcaller`, and `length_hint`, undocumented aliases, and functions outside
the documented Python 3.14 inventory are excluded. The Python 3.14 identity
predicates have no documented dunder aliases; names such as `__is_none__` and
`__is_not_none__` are not candidates.

The index invalidates an imported name if any other binding anywhere in the
module affects it, including parameters, definitions, type parameters, imports,
assignment/deletion targets, comprehensions, walrus expressions, exception and
pattern captures, and `global`/`nonlocal` declarations. Relative, conditional,
and local imports are not trusted. References before the import are excluded,
including function bodies defined before the import even if callers would run
them afterward. Stores, annotations, and match patterns do not produce callable
candidates; pattern grammar cannot generally accept the replacement expressions.

All imported-alias loads evaluated in a class namespace are excluded. A custom
or inherited metaclass can supply even ordinary names through `__prepare__`
without an AST assignment target. The exclusion includes class-body statements
and method decorators and defaults; it deliberately skips safe class-namespace
loads instead of attempting metaclass-provenance inference. Ordinary method and
lambda bodies use function/global lookup and remain eligible. Class-body list,
set, and dict comprehensions and generator expressions likewise use their
implicit function scope for targets, filters, later iterables, and produced
expressions. Their leftmost iterable remains a class-namespace lookup and is
excluded. Import aliases
beginning with `__` remain excluded throughout class-definition ranges,
including method bodies and nested functions, because private-name mangling and
compiler-provided names such as `__class__`, `__module__`, and `__qualname__`
can resolve them to other objects.

Wildcard imports, dynamic namespace operations, explicit `__dict__`/`vars`
access, and writes to builtin `__import__` invalidate namespace certainty.
Taking references to namespace operations (`exec`, `eval`, `globals`, `locals`,
`vars`, `setattr`, `delattr`, `__import__`) also counts, including qualified
attributes and aliases imported from `builtins` or `operator`. Namespace
dunders (`__dict__`, `__setattr__`, `__delattr__`, `__getattribute__`) are also
uncertain, including from-import aliases. `sys.modules` access and imports
are conservative exclusions. Attribute names associated with these operations
are treated as uncertain even on other objects. A write/delete through any
imported `operator` module alias, including local aliases, invalidates all
operator imports. Letting a module alias escape through assignment, argument
passing, or another bare load also invalidates all operator imports, avoiding
unsound assumptions about mutation through another alias. Ordinary writes such
as `self.value = value` do not invalidate independent operator bindings.

These checks are intra-module and deliberately conservative. They assume normal
standard-library imports, do not resolve project import search paths, and do not
prove absence of external monkey-patching or custom import loaders. The index
traversal checks cancellation, and its candidates enter the existing bounded AST
producer, selection, profile filtering, and deterministic merge.

Every approved raw-token operator is then checked against the annotation
containment index and skipped when its complete token range lies inside an
annotation. Deliberate annotation mutations remain in the separate opt-in
`type_*` producer. These gates do not change the existing selection, profile
filtering, deduplication, source ordering, or candidate-limit behavior.

For one source file, the token scanner, AST pass, and type-annotation pass
each retain no more than `max_candidates + 1` candidate records and
deduplication identities before their bounded merge. This is deliberately a
candidate-retention bound, not a general Hoimin memory bound: source text,
parser tokens, AST facts, and small per-node replacement lists remain
proportional to source size. `--max-memory` controls descendants rather than
the Hoimin CLI, so it does not bound these analyzer structures.

Before facts, name resolution, or annotation analysis, `rust/depth.rs` checks
AST depth using a borrowed `AnyNodeRef` worklist. The module has depth 1; every
child exposed by Ruff's source-order visitor adds one, including auxiliary
nodes. The supported limit is 128. For `value = 1+...+1`, 126 terms reach depth
128 and 127 terms exceed it. Rejection is a typed `AnalysisError::DepthExceeded`,
mapped by both analyzer callers to `analyzer.depth` with the target path.
Cancellation remains a separate error. Plan creation fails without a manifest;
run reports incomplete analysis after its normal baseline stage.

The parser is a local backport of `littrs-ruff-python-parser 0.6.2`, selected via
the root Cargo patch. Ruff PR #25464's recursive checkpoints use `stacker`
(locked to 0.1.25): each checks a 128 KiB red zone and grows a 1 MiB segment when
needed. Context assignment, assignment/delete validation, old-decorator checks,
and pattern-to-expression recovery have checkpoints too. This preserves the
grammar and syntax diagnostics before applying Hoimin's AST-depth limit, without
assuming that one larger worker stack is sufficient for every input.

The parser's retained/unchecked module API preserves partial invalid trees so
Hoimin can dispose of them safely while keeping existing invalid-syntax
behavior. Rejected, invalid, and preflight-cancelled trees use an owned worklist:
Ruff's `Transformer` detaches statements, expressions, patterns, and interpolated
string elements before their shallow shells drop. These four callbacks cut every
recursive cycle in the pinned Transformer's traversal, including nested format
specifications. This disposal implementation lives in the parser's `ast_cleanup`
module and also releases discarded with/match speculative results, invalid
keyword patterns, and discarded `as` subpatterns. Hoimin's `depth::dispose` calls
the same implementation. Accepted trees use ordinary drop after passing the depth check.
Both worklists can allocate in proportion to input size; they are stack-depth
controls, not general memory bounds.

Regression coverage includes exact accepted/rejected boundaries with actual
binary and annotation candidates on a 2 MiB thread, auxiliary AST kinds,
repeated 25,000-term rejections with retained-allocation checks, and debug/release
public plan/run subprocesses at 2,000, 20,000, and 25,000 terms. The 20,000-term
fixture previously overflowed during ordinary AST drop alone. These observations
validate the selected limit on tested stacks; they do not prove arbitrary parser
inputs or every platform stack safe. In particular, the guard runs after parsing.

Candidate-local punctuation queries must use
`AstFacts::candidate_tokens_in_range` with the smallest relevant AST range.
Do not scan `Tokens::iter()` for each call, literal, or exception tuple: that
turns a module containing many small candidates into quadratic work. The
test-only candidate token lookup statistics count calls and tokens in the
returned slices. Treat their operation-count bound as the complexity contract;
wall-clock timings are diagnostic only.

`AstFacts` finalizes immutable lookup indexes after its AST walk. Annotation
and focused-profile arid containment use sorted starts with prefix-maximum end
offsets, so overlapping ranges remain exact with `O(log n)` lookup. Unary
`not` operands use an exact-start hash lookup with amortized `O(1)` access.
Definition ranges are swept into disjoint innermost-scope segments and queried
by binary search in `O(log n)`. Do not replace these with a raw-vector fallback
or make lookup correctness depend on token/visitor call order.

Property tests compare the indexes with independent linear definitions for
overlap, nesting, gaps, equal starts, and half-open boundaries. The ignored
adversarial benchmark covers hundreds of scopes and annotations plus thousands
of unary `not` and focused arid facts:

```console
cargo test --release -p hoimin-cli --lib \
  benchmark_adversarial_ast_fact_indexes -- --ignored --nocapture
```

It asserts exact candidates and logarithmic comparison ceilings through
test-only counters. Elapsed time is printed for profiling but is deliberately
not a test threshold. When adding a fact query, extend the semantic comparison
and operation-count assertions rather than introducing a timing-sensitive CI
gate.

Bare builtin calls (`any`, `all`, `list`, `tuple`, `set`, `frozenset`, `min`,
`max`, `sorted`, and `reversed`) use scope-aware shadowing checks. A pair is a
candidate only when both its source and replacement names definitely resolve
through Python's builtins namespace at that occurrence. The resolver follows
whole-function local binding, module/class source order, and closure lookup.
It also preserves class non-closure, `global`/`nonlocal`, and the comprehension
leftmost-iterable boundary. Wildcard imports, conditional bindings, deletions,
bare `exec`/`globals`/`locals`/`vars` calls, missing occurrence facts, and other
ambiguous cases are `Unknown` and suppress the candidate. Qualified builtin
calls are not candidates. Method mutations are syntax-directed and do not
infer receiver types.

The analyzer emits a `split`/`rsplit` swap only when the call supplies a
second positional argument or the named `maxsplit` keyword. Calls that omit
`maxsplit` produce identical string results, so the analyzer skips them.

All type operators share the disallowed-descendant gate. It recursively checks
subscript arguments, unions, tuple/list elements, and starred argument unpacking.
An excluded type (including Any, object, TypeVar, a string forward reference,
Annotated, Callable, Literal, or Protocol) in either mapping argument or a nested
argument suppresses the whole annotation's type mutations. Clean multi-argument
and unpacked annotations remain eligible; this structural check does not establish
that a builtin-spelled name is unshadowed.

The Lean NullableGate model proves blocked-descendant rejection for arbitrary
model depth. Its generated corpus is checked through public CLI plan in
`tests/lean_nullable_gate_oracle.rs`; all 65 rows are strict and cover structural
eligibility, all seven type operators, and the four name-rebinding cases fixed
by issue #564. The adapter rejects report-only modes. Regenerate and check the corpus through
`generate_nullable_gate`; do not change expected pairs in the Rust adapter.

Type-annotation collection records an import-state snapshot at each annotation
site in source order. Signature annotations use their enclosing state, while a
function body predeclares Python-local names before its body is visited. Nested
scopes do not leak their bindings into an enclosing scope. At control-flow
joins, retain only imports known identically on every reachable exit; when no
safe direct-name or module-alias spelling remains, skip the replacement.
Finally annotation entries also include implicit exceptions from expressions
and statement protocols evaluated before later imports (calls, subscripts,
attributes, names, operators, container protocols, formatting, assertions,
iteration, context managers, and class construction). Partial assignment and
pattern targets are conservatively invalidated on these entries. These pending exceptions stay separate from normal fallthrough;
a normally finishing finally resumes them. Implicit entries retain one merged
environment per flow instead of one snapshot per expression. Class globals/nonlocals are
conservatively invalidated without leaking class-local environments. Import
failures and arbitrary dynamic hooks remain outside this analysis.

Synchronous and asynchronous `with` statements also collect body exceptions, even
without an enclosing `finally`. Any manager, including a dynamic or custom
manager, may suppress an exception; the normal successor therefore intersects
body fallthrough with possible suppressed exception states. Explicit raises stay
separate from successful return, break, and continue, which are not suppressed.
Exceptions while evaluating return values can still be suppressed. A falling
finally resumes the original exit category; an abrupt finally replaces it.

Multiple with items follow nested-manager ordering. The first manager cannot
suppress its own entry failure, but an entered manager can suppress failures in
later manager entry, target binding, or an inner manager's exit. Partial target
bindings invalidate affected names conservatively. Possible unsuppressed
exceptions remain available to enclosing handlers and finally blocks. Import
failures remain excluded, so an unconditional import before a later call, or an
import-only body with a simple target, retains eligible typing candidates.


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
`ExceptHandler` AST pass and its `Stmt::Raise` statement pass, then use the
shared selection, profile, source ordering, deduplication, and candidate-limit
pipeline. The default `exception_type_pair` operator replaces only simple,
unqualified handler names in either `except` or `except*`, or the simple
primary name of a supported raise expression, with a curated counterpart:
`ValueError`/`TypeError`, `KeyError` with `IndexError` and `AttributeError`,
`FileNotFoundError`/`PermissionError`, `ConnectionError`/`TimeoutError`,
`ImportError`/`ModuleNotFoundError`, and
`ZeroDivisionError`/`OverflowError`.

`AstCandidateCollector` must enter its exception-type context for both `except`
and `except*`, keep that context balanced, collect dedicated exception
candidates outside the generic collection gate, and visit handler bodies
normally. This keeps collection mutations out of type positions, where Python
requires an exception class or a tuple of exception classes, including generic
`list`/`tuple` constructor-call and literal candidates nested in the type.

Exception names use the same scope-aware resolver as builtin-call pairs. Both
the handler's source name and each inserted or replacement name must definitely
resolve through Python's builtins namespace. Assignments, imports, parameters,
comprehension and match captures, and `except ... as` targets suppress only
occurrences where their binding is visible; sibling scopes do not leak. Class
targets are not closure bindings for methods. Wildcard imports and ambiguous
control flow remain conservative `Unknown` results. Qualified and dynamic
handler types and tuple members are skipped by the safe pair operator. The
starred handler path invokes only this safe simple-name collector; its
`Exception`/`BaseException`, bare-handler, and tuple rewrites remain disabled
even when the explicit-only risky operators are selected.

For a raise statement, `AstCandidateCollector::visit_stmt` inspects only
`StmtRaise.exc`. It accepts `raise ValueError` and calls whose callee is a
simple name, such as `raise ValueError(message)`. The replacement span is only
that name, so arguments and `raise ValueError(...) from cause` remain intact.
Normal AST walking then visits the primary expression and cause exactly once,
preserving candidates from other selected operators without interpreting the
cause as an exception type. Bare re-raise, qualified names, dynamic callees,
shadowed source or destination names, and termination exceptions are skipped.

The five structural operators in `exception_risky` are explicit-only:
bare-handler insertion/removal, the `Exception`/`BaseException` boundary, and
curated tuple add/remove rewrites. Tuple candidates use parser token ranges and
the original source text so commas, comments, trailing commas, and line endings
remain intact and every replacement can be reparsed. The BaseException boundary
can change handling of `SystemExit`, `KeyboardInterrupt`, and `GeneratorExit`,
so it must not be added to the default selection.

When changing these rules, keep exact candidate and replacement assertions next
to `apply_candidate_and_reparse` checks. Run the focused tests before the full
analyzer module:

```console
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_type_pair_candidates_are_curated_and_syntax_directed -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::raise_exception_type_pair_candidates_preserve_the_primary_expression -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::raise_exception_type_pairs_use_scope_aware_source_and_destination_resolution -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::exception_risky_candidates_require_explicit_selection_and_reparse -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::sibling_bindings_do_not_suppress_builtin_or_exception_pairs -- --exact
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests::comprehension_exception_target_and_wildcard_boundaries_are_conservative -- --exact
```

## Extending plan ranking

Plan manifests use schema version 5 and ranking rule version 4. The schema
version describes the manifest's serialized shape; the ranking rule version
describes the category and scoring semantics used to order its candidates.
Change the ranking rule version whenever those semantics change, even when the
manifest schema itself does not.

An explicitly selected symbol contributes 250 points to that symbol and its
dot-delimited descendants in the same resolved file. The bonus is awarded once,
even when both a parent and child selector match; `Box` matches `Box.check`, but
does not match `BoxOther.check`. Verification rejects plans with older ranking
rules and directs the user to regenerate them.

Every canonical mutation operator is exhaustively assigned to exactly one
fixed-score category:

- `high_value_control` (100): comparisons, membership and identity tests,
  boolean operations, `not`, boolean literals, and `break`/`continue`.
- `exception_handling` (90): safe and risky exception-handler mutations.
- `behavioral` (80): collection, structural, and `operator_function` callable
  and protocol behavior mutations.
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

## Rust test workflow

Use the normal Rust tests, contract tests, and Lean oracle checks to validate
changes to the Rust implementation. Select a package, integration test, or test
name when investigating one behavior, then run the workspace checks before
merging:

```console
cargo test -p hoimin-cli --test analyzer_handler
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
```

Rust mutation testing with cargo-mutants has been removed because its memory
and disk consumption is unsuitable for this repository's development workflow.
Do not install or invoke it for repository checks. Earlier audit reports and
implementation plans describe historical runs and are not current instructions.

Hoimin's Python mutation testing remains available through `hoimin plan` and
`hoimin verify`; see the repository's Python mutation-testing skills for target
selection and resource limits.

## Abstract set annotation providers

Collection annotation pairs use provider-specific abstract names:
`typing.AbstractSet` and `collections.abc.Set` both pair with builtin `set`.
Plain/aliased module imports and direct/aliased member imports are supported;
`typing.Set` is not the abstract counterpart. Replacement spellings retain the
resolved module provider, including unaliased `collections.abc`. Existing
source and destination shadowing checks still apply. Tests evaluate generated
annotations under CPython 3.14 in addition to reparsing their source.

## Worker import roots

`RawRunConfig.import_roots` normalizes to an ordered, duplicate-free list in
`RunConfig` and `PlanConfig`, separate from `Selection.sources`. The normalized
config validators reject escaped or non-normalized paths from persisted data.
Historical run configs decode a missing list as empty. Plan schema 5 requires
regeneration of earlier manifests before any baseline runs; fingerprint schema
11 frames the ordered import roots under field tag 9, the ordered include/exclude
copy patterns under tags 10/11, and configured source roots under tag 12,
preventing old-session reuse. Source file hashes retain their set semantics;
source-root order separately captures worker Python import precedence. Patterns retain
their exact spelling and order because negated overrides can change matching
precedence. Operational jobs/max-output settings still do not affect compatibility.
An older incomplete fingerprint can produce `session.resume.incompatible` under
the existing selection policy; start a new run without resume or use another
session path. Existing data is retained.

`WorkspaceHandler::with_import_roots` preserves the existing constructor and
copy lifecycle. The owned blocking worker-materialization task checks that explicit roots
exist as directories in the copied worker before publishing `WorkerCreated`.
Failed validation retains cleanup ownership through the existing pending-worker
path; command environment construction performs no filesystem reads. The error
identifies `--import-root` and suggests checking exclusions. Import roots do
not bypass copying policy or select additional targets. Environment order is
worker root, explicit roots, selected source roots, then rewritten inherited
PYTHONPATH, using the existing OS split/join and deduplication.

`tests/import_roots.rs` creates a network-free `venv --without-pip` and a
regular package exposed by a path-only `.pth`. It records actual module paths
and results for baseline and mutant, checks narrow file/line selection and
plan-to-verify inheritance, and retains an inherited-PYTHONPATH control. This
observes Python import behavior directly; Lean session/workspace oracles do not
prove how Python loaders resolve imports. The setting covers path-only `.pth`
regular packages when Python honors PYTHONPATH, not arbitrary editable finders,
custom loaders, or invocations with `-E`/`-I`.


## Performance cost correspondence

The shape registry now retains the original 21 gates and adds four actual cost
counter gates plus a separate workspace preflight allocator-peak gate. The 56
Lean-owned cases exercise zero/one, tree boundaries and N/2N/4N; never edit
`formal/HoiminOracle/corpus/performance-cost.jsonl` by hand. The generator and
its model/proofs are registered in the existing serial bounded Lean CI lane.

Run `cargo test -p hoimin-cli --lib performance_cost_tests` and
`cargo test -p hoimin-cli --test workspace_preflight_heap`; the registry gate
runs these tests by exact name. Counter correspondence is internal-fixture,
while release time/RSS observations use separate public subprocesses. See the
[performance guide](performance/README.md) and
[cost worksheet](superpowers/reports/2026-09-14-issue-491-cost-correspondence-worksheet.md)
for measured steps, native-integer premises and excluded costs.
## Valid Python candidate corpus

`formal/HoiminOracle/ValidPythonAuditMain.lean` owns the seed489 fixtures and
eligible/ineligible sites. The `valid_python_corpus` integration test compiles
original bytes with CPython3.14 before comparing the direct Rust analyzer with
the real CLI plan. It validates every emitted candidate, checks independent
physical-line/Unicode locations, and compiles each one-candidate replacement.
Runtime probes check representative bindings and protocol behavior.

```console
HOIMIN_OPERATOR_TEST_PYTHON=/path/to/python3.14 \
  cargo test -p hoimin-cli --test valid_python_corpus valid_python_ -- --nocapture
```

The output names every fixture/operator, seed, known-risk triple and uncovered
axis pair. Register every new canonical operator in
`crates/hoimin-cli/tests/fixtures/valid-python-operators.json`; a deferred entry
must explain why it remains outside this bounded corpus. A registered negative
fixture is not evidence of positive coverage for that operator. The producer
controls and required-position gates prevent an all-empty corpus from passing.

Regenerate only through `generate_valid_python`; use the existing 30-second,
2GiB Lean resource guard for output, freshness and sensitivity commands. The
[correspondence worksheet](superpowers/reports/2026-09-14-issue-489-correspondence-worksheet.md)
explains normalized scope/key premises, public observations and model limits.
This corpus supplements the focused operator regressions and existing Lean
adapters; it does not relabel their internal-fixture or model-only cases.
### Cross-boundary contract replay

`tests/fixtures/boundary-contracts.json` connects six boundaries to exact tests.
Run `python3 tools/boundary_contracts.py strict --output /tmp/hoimin-boundaries`
from a worktree with its own Cargo target and prepared `.venv`. Each strict row
must execute and match; zero-test success, unavailable premises and external
deadlines cannot count as matches. The runner removes inherited minimal-case
filters so corpus coverage stays complete.

Run `python3 tools/boundary_contracts.py report --from-results
/tmp/hoimin-boundaries/strict.json --output /tmp/hoimin-boundaries` as one command
to retain failures and unexecuted native/preparation cases. Missing strict
results produce unexecuted rows, not inferred passes. CI runs this after the
existing Lean proof/sensitivity/freshness gate, then adapter checks, strict
replay, all-case report and a minimal result-validation witness. Native Linux
hard controls, Windows PID/fault cases and preparation cancellation remain
explicit report gaps. See [the worksheet](superpowers/specs/2026-09-14-issue-490-boundary-contracts-design.md).
## Python source encoding contract

Use `hoimin_core::decode_python_source` for source bytes that need Python text,
including symbol existence checks. Its `DecodedPythonSource` exposes `text()`,
`encoding()`, `utf8_to_raw()` and `raw_to_utf8()`. The two offset methods accept
only character boundaries. UTF-8/ASCII borrow source text; Latin-1 owns decoded
UTF-8 plus a sparse expansion index. This storage grows with source input and
is not bounded by `max_candidates`.

Candidate spans and source hashes always describe original bytes. Convert
Ruff's UTF-8 spans before constructing persisted candidates, then validate with
`CandidateValidationContext`. It owns decoded facts and shares the existing
Unicode-column index across candidates. The candidate strings are Unicode;
the source codec must encode original and replacement text. Keep replacement
representability checks before worker writes, and write raw prefix, encoded
replacement, raw suffix. Never apply decoded offsets directly to raw Latin-1
bytes or write `replacement.as_bytes()` for every codec.

The stable-ID schema remains 1: it already frames raw source hash/span and
Unicode replacement. Source records and fingerprint inputs remain raw bytes.
Run `cargo test -p hoimin-core --test source_encoding --test candidate_policy`
and `cargo test -p hoimin-cli --test source_encoding --test plan` for codec
changes. The CLI encoding fixture observes CPython worker bytes and values;
existing UTF-8 Lean/source-index proofs do not establish codec correspondence.

## Resume budget compatibility

Fingerprint schema 10 omits only `max_mutants` from verdict compatibility.
SQLite schema 4 persists each run's accepted limit as a positive eight-byte
big-endian unsigned value. Legacy rows retain NULL and remain non-resumable;
the migration does not rewrite candidate/results. Selection finds the newest
compatible incomplete run whose accepted limit does not exceed the requested
limit. After acquiring the existing run ownership lock, a conditional update
checks the exact validated old budget before reserving the new limit. Invalid
budget blobs fail as `session.corrupt`. Scheduler accounting still charges
reused results against the cumulative limit.

`generate_resume_budget` produces 108 bounded strict cases consumed through
public SessionHandler operations in `session_handler`. Lean proves no decreases
and monotonic eligibility; it does not model SQLite locking or the scheduler.
Public CLI tests cover baseline execution, cumulative reuse/execution, completion,
and equality with fresh execution. The existing migration oracle's abstract
current version maps to SQLite 4; its current migration step includes v3 and v4.

## Target discovery inventory

`hoimin_core::resolve_explicit` consumes an inventory already filtered by discovery.
Callers apply include/exclude globs, ignore rules and built-in copy exclusions before
calling it. The core resolver validates and combines selectors; it does not treat
pattern strings as literal filenames or independently implement glob matching.
The CLI uses `target::fs::discover_explicit` for this filtering.

Explicit source discovery walks from the project root but prunes entries outside
the union of selected source subtrees and exact/line paths plus their ancestors.
Both normal and include-restoration walks use this scope; original source order
still controls symbol lookup. A root source retains broad discovery. Non-Python
records inside a selected source remain available to resolution. Malformed names
or traversal errors solely inside pruned unrelated subtrees are outside this
request; selected-path and root errors remain visible, as with exact selectors.
The source-scaling regression counts actual walker visits and retained records:
unrelated descendant file counts do not affect either, while root-level sibling
enumeration can still grow. It does not impose an elapsed-time or RSS threshold.

## Explicit inherited environment fingerprints

`--fingerprint-env` accepts portable ASCII identifiers only. Core config
normalization sorts/deduplicates names and uppercases them on Windows; Unix
preserves case. The CLI captures each selected inherited native value during
preparation, before worker environment rewriting. Its incremental BLAKE3 encoding
contains a domain/version tag, platform tag, entry count, framed name, presence
byte, and framed native value. Unset and empty are distinct; Windows stores UTF-16
units in little-endian order, and Unix uses raw bytes.

Only normalized names and the digest are retained in config. New serde fields
default to untracked when absent and are omitted when empty. Plan schema5 validates
canonical names and a matching digest presence/format, then verify compares the
current captured digest before project work or baseline execution. Fingerprint
schema11 binds the names and digest under field13; SQLite schema4 and budget
selection are unchanged. Captured plaintext is not added to persistence or output;
digests are not an encryption or low-entropy secrecy guarantee.

`EnvironmentFingerprintModel` generates 32 strict cases for tracking on/off and
four before/after values (absent, empty, one, zero). Its proofs cover tracked
fresh-equivalence, unchanged reuse, untracked compatibility, and injective abstract
presence encoding. `lean_environment_fingerprint_oracle` executes real CLI runs
with isolated SQLite databases and checks model-generated verdict/reuse/budget/
termination/exit observations. Hash collisions, arbitrary native bytes, Windows
OS behavior, and concurrent environment mutation are outside the finite model.
Native/framing/platform config tests and public plan/verify/privacy controls cover
those implementation boundaries where stated; tests do not mutate global env.

## User-defined exception hierarchies

`exception_hierarchy` is an explicit-only operator implemented in
`analyzer/exception_hierarchy.rs`, `analyzer/exception_scopes.rs`, and
`analyzer/exception_project.rs`. The project
loader discovers Python inputs using the existing include/exclude policy, without
file/line/symbol/changed mutation filters. Module roots follow worker precedence:
project root, explicit import roots, then source roots. Inherited external
PYTHONPATH entries and third-party packages are outside the analysis scope.

The index retains binding/import/class summaries and decoded-source hashes.
Loaded bytes must also match the prepared fingerprint before decoding. Prepared
Python input paths reserve module origins even if temporarily missing, so a lower
search root cannot silently supply different classes during a delete/restore race. Both
plan discovery and run analysis reuse a cached index for sequential loads within their analysis session;
failed or cancelled builds are not cached. Selected source contents must match the
indexed snapshot. The cache does not enforce single initialization or first-success
publication for concurrent callers. `fingerprint_inputs::resolve_config` adds the allowed Python input
set to existing fingerprint records, and verify/run rechecks recompute the set.
This covers dependency additions and deletions as well as content changes, and the
workspace manifest check verifies copied automatic inputs.

Supported class identities are unconditional top-level single-inheritance classes
without decorators, type parameters, class keywords or custom subclass hooks. Class
bodies can contain methods, pass statements and docstrings. Explicit class imports,
relative class imports, module imports, and aliases are recognized; re-exports and
`from package import submodule` are not inferred. Duplicate/rebound bindings and
namespace manipulation invalidate trust. Relative imports use the known name under which their containing module is loaded,
rather than guessing from its first filesystem root. A file imported under multiple module
names is excluded because Python creates distinct class identities for those loads.
Ambiguous module/package layouts, including namespace portions shadowed by a regular
package at a different root, are conservatively excluded. Top-level CPython 3.14 standard-library/frozen module names and `__main__` are
reserved; an identically named project file cannot establish an import identity.
Names below project packages, such as `pkg.sys`, remain eligible. This lexical model does
not prove safety against arbitrary external monkeypatching or custom import hooks.
Simple, chained and annotated name assignments and assignment expressions propagate
known attribute writes back through their possible aliases, including private names.
Cycles and rebinding retain all possible edges. Keys contain a lexical scope ID
and the normalized name: unrelated parameters and local imports do not share a
vertex with a module binding. Implicit `__class__` writes resolve to the owning
class, including through aliases and nested closures. Global/nonlocal/free references resolve before
propagation; methods skip class namespaces, while class-body reads conservatively
retain both local and outer possibilities. Definition headers, lambda bodies and
comprehensions have separate binding rules, including outer walrus targets and
first-iterable evaluation. Only affected module bindings invalidate module names;
reached import keys invalidate their provider from any scope. Each module has
separate limits of 65536 assignment edges, direct write roots and scope IDs. Each
scope's declaration set is capped at 65536 entries, with 131072 declaration entries
across all scopes. This does not make assignment aliases eligible spellings.
Untracked function/container aliases inside indexed modules can still affect
candidates in other modules. Unsupported effects are not always detected and skipped,
so emitted spellings are not guaranteed
to resolve to exception classes at runtime. The
[original-design review](superpowers/reports/2026-10-05-exception-hierarchy-design-review.md)
records the design gaps; the [alias correspondence audit](superpowers/reports/2026-10-05-exception-alias-lean-audit.md)
records the bounded assignment repair, Lean proofs and extracted-fact comparisons.
The [scope and outcome audit](superpowers/reports/2026-10-05-exception-scope-lean-audit.md)
records scoped identities, diagnostic semantics, and their verification.

Pairs connect direct user-defined parents/children and siblings with a shared
user-defined direct parent. Builtins seed ancestry only. Termination and exception
group ancestry is rejected. Handler mutations allow custom constructors; `raise`
mutations require the entire user-defined ancestry to inherit the plain `Exception`
constructor, with neither `__init__` nor `__new__` overrides. Specialized builtin
constructors are handler-only. Arguments and `from` causes are preserved.

Function parameters, binding targets, captures and definition-header assignments
suppress affected references throughout their lexical scope, including private
bindings after Python class name mangling. Qualified references require their
submodule to have been loaded before the containing function or module-level use. Class bodies are
excluded; method scopes skip class-local bindings. Module definitions used inside a
function must already exist before that function's definition, preventing early
calls from observing an inserted future name. Conservative exclusions may suppress
valid candidates; they never authorize an inserted import.

The loader limits input to 4096 files, 16 MiB per file, 64 MiB total decoded source,
and 65536 entries per module-name, binding and visible-alias table. Ancestry/import traversal is limited to 256 steps and
uses the existing AST depth guard. Cancellation is checked during discovery,
index construction and replacement enumeration. Per-site replacement retention is
bounded by `max_candidates + 1`, preserving the existing producer's source-order
prefix and truncation signal. One `exception_hierarchy_skipped` diagnostic per file
aggregates incomplete-analysis reasons (disabled scope, unsupported expression,
unresolved binding or untrusted module) and locates the earliest skipped site.
Intentional exclusions such as no related class, no visible destination, custom
constructor policy and bare raises do not warn. Diagnostics do not mark analysis
as truncated. The input/table limits above
do not constitute a peak-memory bound: decoded sources, an AST and transient
summaries can coexist, and some summary limits are checked after construction.

Focused verification:

```console
cargo test -p hoimin-core --test operator_selection
cargo test -p hoimin-cli --lib hierarchy
cargo test -p hoimin-cli --test exception_hierarchy
```
