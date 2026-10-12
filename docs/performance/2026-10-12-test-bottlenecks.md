# Python and Rust test execution costs

## Scope and approved approach

Baseline: `44ccb12`, Windows, Python 3.14 and Rust 1.99.0. The user approved
measuring both test suites and improving demonstrated bottlenecks. This is a
bounded maintenance change: retain test cases, assertions, property strategies
and resource/cleanup guarantees. Separate compilation, collection, execution
and serialization. Adopt changes only after equivalent checks and repeated
measurements; do not assume Arrow or another runner will improve small records.

## Plan

1. Measure Python collection and the complete existing suite with a fixed seed;
   profile separately so instrumentation does not contaminate baseline timings.
2. Build Rust test artifacts in an owned directory with two build jobs and no
   debug information. Measure build separately from warmed test execution.
3. Inspect slow tests, process startup, serialization and repeated CI commands.
   Select the smallest supported improvement; retain real process integration
   coverage and platform-specific checks.
4. Alternate before/after runs on the same inputs and host. Check case identities,
   assertions and outcomes, not just wall time. Record unsupported platforms and
   noisy measurements explicitly.
5. Run relevant quality gates and an independent final review. Commit evidence
   and current maintenance guidance, open a PR, and clean owned Cargo artifacts.

## Design self-review (five passes)

1. Scope: test/development costs, not changing mutation verdicts or public formats.
2. Correctness: distinguish pure unit work from actual process/OS enforcement;
   never globally patch sleep or replace subprocess coverage with mocks.
3. Resources: use two Rust build jobs, a single measurement at a time, check free
   disk, and clean only this task's Cargo target.
4. Measurement: cold compilation and warm runtime are different quantities;
   profiling overhead and Windows results cannot establish Linux speedups.
5. Alternatives: measure redundant work and startup before adding concurrency,
   new dependencies, or Arrow; retain JSON unless evidence supports migration.

## Plan self-review (five passes)

1. Baseline: fresh main and clean work branch confirmed before retained changes.
2. Comparability: fixed Python seed, same tools/inputs, paired repeated runs.
3. Coverage: preserve all collected cases and explicit failure/cleanup checks.
4. Failures: separate sandbox/environment failures from software failures; stop
   invalid timing runs instead of counting failures as fast successes.
5. Delivery: preserve raw observations, limitations, quality results and review;
   avoid introducing permanent profiling infrastructure without recurring value.

## Selected changes

The Windows Python baseline collected 1,006 cases: 977 passed and 29 were
skipped, in 243.55 seconds. The two complete/late-failure Lean orchestration
tests took 58.20 and 36.75 seconds. Their existing fake `python3` only recorded
arguments and returned a selected status; it did not run Lean or resource guards.
Replace that recording executable with a Bash stub, avoiding Python startup,
JSON encoding and rereading the complete log for every invocation. Keep executing
the actual workflow script and preserve its exhaustive order and failure checks.
The small test-only transcript uses NUL delimiters and argument counts so empty
arguments, quotes, newlines and Unicode remain distinguishable. This is not a
replacement for any public JSON format. A bounded property test protects those
argument boundaries, including a temporary directory containing spaces/Japanese.

The successful Linux CI run
[38200201784](https://github.com/HayaoSuzuki/hoimin/actions/runs/38200201784)
ran the same `run_e2e` binary twice: 76 passed in 63.31 seconds as part of
`cargo test --workspace`, then 76 passed in 63.65 seconds in the standalone step
(64 seconds including Cargo). Remove that second invocation from automatic
Linux and manual non-Linux workflows, and from the complete local gate list.
The workspace default-feature suite, reference-example tests, generated-reference
check, contracts, shuffle and OS-specific tests remain. Two workflow checks
failed before the removal and pass after it. Focused local `run_e2e` remains
documented.

## Implementation self-review (five passes)

1. Process coverage: only an already-fake recording executable changes. Actual
   process/resource/timeout tests and the checked-in Lean audit script are intact.
2. Recording fidelity: count-prefixed NUL records preserve empty/control/Unicode
   arguments. UTF-8 shell input fixes a Japanese-argument mismatch demonstrated
   against the original native-Python stub on this Windows host.
3. Failures: the stub exits with the same selected status; the parent Bash script
   still decides whether to stop. Existing first-failure and resource-failure
   assertions retain their exact call counts and command checks.
4. Rust coverage: Cargo's default workspace targets include integration tests;
   the measured CI confirms the identical `run_e2e` artifact and all 76 cases
   already run. Do not remove the separate example test or feature/OS jobs.
5. Compatibility and scope: no product Rust/Python logic, CLI, JSON schema,
   dependency, supported platform or package/version-floor changes. Do not adopt
   Arrow, nextest, broad fixture caching, reduced property budgets or global
   sleep patches without evidence.

## Measurement method and limitations

The complete Python comparison observed 243.55 seconds before and 184.73
seconds after (58.82 seconds / 24.2% less). The after run passed 980 cases with
the same 29 skips: the original 1,006 cases plus the new recorder property and
two workflow checks. This one full-suite pair is descriptive, not a universal
speedup estimate. Seed 812 fixes pytest-randomly's order/reseeding; Hypothesis's
generation seed and example database remain at their defaults. The separate
three-scenario comparison uses fixed workflow inputs without property generation.

The new-target Windows Rust build took 327.70 seconds; the warmed workspace
command passed in 604.76 seconds with 147 top-level executable summaries,
2,790 passed and 27 ignored tests (zero doctests). The ignored tests and
platform-specific conditional paths do not establish coverage of another OS.
The slowest executable summaries were:

| Windows target | Seconds | Passed / ignored |
| --- | ---: | ---: |
| `valid_python_corpus` | 61.73 | 285 / 3 |
| `run_e2e` | 58.83 | 68 / 1 |
| `lean_nullable_gate_oracle` | 47.84 | 2 / 0 |
| `lean_environment_fingerprint_oracle` | 32.73 | 3 / 0 |
| `lean_changed_lines_oracle` | 22.12 | 4 / 0 |
| `collection_annotation_builtins` | 21.32 | 5 / 0 |

These targets combine real CLI/CPython/type-checker observations with setup and
I/O. Their total elapsed times do not attribute costs to JSON. Retain these
boundary checks; fewer child startups through carefully bounded batching could
be investigated separately, without weakening isolation or assertions.

The three fixed orchestration scenarios were repeated against the original
`44ccb12` test module and the modified module. The original module was copied
to scratch with only `ROOT` redirected to the current checkout; the Lean workflow
script and module/generator inventory are unchanged. These are sums of JUnit
case times (setup/call/teardown), excluding collection and terminal reporting:

| Fixed three-case workload | Before seconds | After seconds |
| --- | ---: | ---: |
| Within the complete suites | 95.95 | 40.54 |
| Separate repeat | 96.63 | 42.01 |

The repeated reduction is about 56.5%; two runs per variant are evidence for
these fixed fixtures, not a confidence interval or cross-platform prediction.
The isolated repeat's pytest console totals were 97.09 and 42.34 seconds.
JUnit suite times differ slightly from console totals because they are captured
at different points of pytest shutdown.

The separate `cProfile` run passed 71 cases: SBOM properties, CI selection and
the complete Lean orchestration case. Its instrumented parent-process total was
53.025 seconds. Summing **self time** for functions whose normalized filename
contains `/json/` gives 0.233 seconds (about 0.44% of that instrumented total).
The retained function tables include both cumulative/self costs and all matching
JSON frames; cumulative parent/child times must not be added together. These
are wall-timer attributions, not a whole-process CPU or serializer-only benchmark.
They exclude child-process JSON work, Rust serialization and some work outside
those frames; they cannot cap the benefit of every possible JSON optimization.
They do not provide evidence to prioritize an Arrow migration in this change.

## Evidence and reproduction

[measurements.json](test-execution-2026-10-12/measurements.json) retains complete
Python case observations, paired fixed workloads, Rust executable summaries,
build/runtime times, the historical CI head revision, profile tables and a
SHA-256/size manifest of gzip-compressed raw logs, JUnit and Cargo artifacts.
Gzip timestamps are zero. Decode the compressed files before comparing their
uncompressed hashes. The historical CI log contains only the original Cargo-test
and environment-sync step lines, including timestamps; unrelated checkout logs
are omitted.

Reproduce from a separate checkout of `44ccb12` and this branch, with the same
installed dependency lock and host. Use fresh task-owned temporary/target paths:

```console
python -m pytest --collect-only -q --randomly-seed=812
python -m pytest -q --randomly-seed=812 -p no:cacheprovider --basetemp=<fresh-path> --durations=25 --junitxml=<result.xml>
cargo test --locked --workspace --no-run --message-format=json
cargo test --locked --workspace
python -m cProfile -o <profile.pstats> -m pytest tests/test_sbom_properties.py tests/test_ci_selection.py tests/test_ci_workflow.py::test_bounded_audit_covers_every_module_and_generator -q --randomly-seed=812 -p no:cacheprovider --basetemp=<fresh-path>
```

Set the four Cargo variables documented above and a new `CARGO_TARGET_DIR` before
building. Select the three `test_ci_workflow.py` names in `paired_fixed_workloads`
for focused repeats. Use the original checkout rather than copying the whole
modified tree; the scratch-copy technique above was used only for these fixed
Lean scenarios. Preserve command exit status and check every expected case before
interpreting durations. The temporary analysis helper is not a supported tool.

## Test self-review (five passes)

1. Inventory: compared complete JUnit identity sets, outcomes and skip sets;
   all original 1,006 identities and 29 skips remain, with exactly three additions.
2. Boundary fidelity: the bounded argument property passes with empty strings,
   quotes, whitespace/newlines, Japanese and a directory containing spaces and
   Japanese. The original mismatch and before-fix workflow failures are retained.
3. Failure sensitivity: exhaustive command/order checks and exact early/late
   failure counts pass in both full and repeated runs. Real resource/cleanup
   integration tests were retained and the Rust workspace passed.
4. Timing validity: compared fixed scenarios twice, separately from profiling;
   distinguished pytest order from Hypothesis generation, JUnit/console clocks,
   compilation/runtime, Windows/Linux and parent/child measurements.
5. Quality and scope: repository-wide Ruff format/check and ty pass; strict
   actionlint, ShellCheck and offline auditor-persona zizmor pass. Existing
   documented zizmor exclusions and unsupported Python-shell warnings remain;
   no lint exclusions were added. Rust source is unchanged, so no extra rebuild
   or Clippy cycle is needed beyond the successful fresh build/workspace run.

The strict documentation build/link validation also passed. After preserving the
Cargo observations, checked the resolved target path against the task-owned path
and ran `cargo clean --manifest-path Cargo.toml --target-dir <owned-target>`.
It removed 2,827 files / 4.3 GiB, leaving about 172.2 GiB free. The main `target/`
and other tasks' artifacts were not cleaned. Runtime peak RSS was not measured;
the available-memory snapshot during the run was about 5,100 MiB.

The complete Python runs use seed 812, disabled pytest cache writes, fresh
task-owned temporary directories, `--durations=25` and JUnit output. The initial
collection took 1.76 seconds. Setup failures from an earlier restricted-sandbox
trial were caused by access denial in pytest's shared temporary root; that run
is not a performance baseline. The successful complete baseline and subsequent
comparisons run with the same permissions and dedicated temporary roots.

Rust uses `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0` and a fresh, task-owned
`.superpowers/test-performance/cargo-target`. The initial build of all workspace
test artifacts took 327.70 seconds on Windows. This is a new target-directory
build, not a cold operating-system or dependency-download cache. Cargo's warm
command includes startup/freshness checks and the workspace's normal libtest
concurrency. Timed full suites are run sequentially, without another build or
profile in parallel. Small static inspections and focused correctness checks
during the build are not competing benchmark campaigns.

The earlier Linux CI is a separate environment/revision, not a Windows speed
comparison. Its `uv sync` took 164 seconds, workspace step 382 seconds (including
164 seconds of test compilation), Python wheel-job pytest step 208 seconds, and
redundant Rust E2E step 64 seconds. Job step clocks have one-second resolution;
libtest supplies the more precise E2E execution times. Rust sources are unchanged
by this patch. Do not add wrapper elapsed times to nested test summaries or count
recursive test-harness children as extra workspace cases.

Profiling is diagnostic and separate from uninstrumented comparisons. Python
parent profiling cannot measure CPU costs in Python/Rust/Git child processes.
Per-binary Rust wall times include subprocess waits, fixture I/O and assertions;
they do not isolate serializer CPU time. Memory snapshots/free disk checks are
operational checks, not sampled peak measurements or resource enforcement proof.

## References

- [Pytest Make Super Faster with 10 Methods](https://www.slideshare.net/slideshow/pytest-make-super-faster-with-10-methods-at-pycon-jp-2025/283458532):
  profiling, duration reports, collection/import costs and fixture work motivate
  measuring existing costs first. Database-specific changes do not apply here;
  pytest already limits discovery to `tests`, and these commands do not enable
  coverage measurement, so changing coverage's execution core is not a current
  optimization.
- [Cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html): default
  integration targets, `--no-run`, workspace and feature selection explain why
  the duplicate E2E step adds no separate test selection.
- [Python profiling](https://docs.python.org/3.14/library/profile.html): separate
  diagnostic profiling from repeatable uninstrumented timings.
- [Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html):
  contiguous column scans, vectorization and supported zero-copy access may
  benefit large analytic batches. They do not establish an end-to-end win for
  small nested documents or eliminate conversion from ordinary Python objects.
- [Previous nextest assessment](nextest/2026-10-11-linux.md): the evaluated
  two-thread configuration did not justify adopting another runner. This patch
  does not generalize that result to other concurrency settings/platforms.

## Independent final review

A separate read-only reviewer found no actionable issues in the implementation
or retained evidence. The reviewer independently checked all 21 artifact hashes,
Python case identities/outcomes, Rust executable summaries, historical duplicate
E2E evidence, profile arithmetic and knowledge source hashes. The reviewer did
not rerun suites or builds; the execution results above are the implementer
checks. The review confirmed that real resource coverage and failure assertions
remain intact and that measurement limits are disclosed.
