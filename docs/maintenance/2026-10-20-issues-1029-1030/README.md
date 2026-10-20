# Analyzer work admission and exception visibility

Base: `1f7feae33fa310aae87fbadb390105f6d770439d`.
Branch: `perf/issues-1029-1030`.

The user selected [#1030](https://github.com/HayaoSuzuki/hoimin/issues/1030)
and [#1029](https://github.com/HayaoSuzuki/hoimin/issues/1029) for one performance
change. [#1028](https://github.com/HayaoSuzuki/hoimin/issues/1028) has a separate
branch and PR because it changes the public progress output contract.
The [implementation plan](../../superpowers/plans/2026-10-20-issues-1029-1030.md)
records the shared constraints and task boundaries.

## Environment and baseline

- macOS on Apple Silicon; Rust/Cargo 1.99.0; Python 3.14.7.
- Clean-base `cargo test -p hoimin-python --lib`: 457 passed, 3 ignored.
- Baseline benchmark source is a detached worktree at the base commit, with its
  own release target directory. The changed source and the progress branch use
  different Cargo targets so a concurrent build cannot replace a tested CLI.
- Benchmark timing runs must be serial and separate from compilation/tests.
  Cumulative requested allocation, retained requested heap and peak requested
  heap are distinct; none represents RSS or allocator usable size.

## Integration review focus

1. Admission checks use the same call-start rule as candidate acceptance; they
   suppress only the optional whole-call producer and preserve child traversal.
2. Zero payload budget and candidate-limit changes cannot re-enable discarded
   replacement construction.
3. Class-member indexing preserves importer-specific resolution, loaded
   descendants, ambiguity and shadowing; output ordering remains stable.
4. Class-bearing alias output retains its own work and limits. The index cannot
   hide output growth behind a claim about eliminated unrelated scans.
5. Cancellation and error behavior remain observable independently of measured
   normal-path speed; fixtures, warmup and output hashing stay outside measured
   operation intervals.

## Integration checks

- [x] Focused red/green regressions and selected-output equivalence.
- [x] Serial paired release measurements and reproducible source/binary metadata.
- [x] Independent whole-branch review and resolution of material findings.
- [x] Workspace tests, core/CLI contracts, Clippy and formatting.
- [x] Python support and relevant schema/documentation checks.
- [x] Current knowledge contract/source updates and structure/reference checks.
Publication follows these checks; the associated PR and Git history identify the final commit.

Native Linux/Windows performance and resource-backend behavior are outside this
macOS measurement record. Existing oracle comparisons do not establish new
formal proofs. Current development instructions prohibit cargo-mutants; those
historical procedures are not part of these checks.


## Evidence map

- [Optional-call evidence](optional-keywords.md): 15 focused regressions,
  five reviews per stage, selected-output comparisons and small-input caveats.
- [Visibility evidence](exception-visibility.md): independent operation gates,
  exhaustive oracle, public output comparisons, output-size costs and cache heap.
- [Independent review](independent-review.md): final source/hash and evidence
  review, with no remaining actionable findings.
- [Measured source manifest](production-source-manifest.json): exact base and
  changed production source hashes; probe/link metadata accompany each result.
- [Optional raw measurements](optional-measurement.json) and
  [visibility raw measurements](visibility-measurements.json): individual
  observations, workload identities, summaries and comparison results.

Final whole-branch verification follows the test-only lint corrections below.
The last added test demonstrated that returning from `collect_call` too early would
lose a selected method-name candidate. That sensitivity change was restored;
only tests and evidence changed after the measured release build.


## Integration corrections

The first all-target/all-feature Clippy check rejected seven style violations
in the new tests: intermediate formatted strings in fixture construction and
empty-collection assertions. Fixture builders now append with `writeln!`, and
empty outputs assert length zero. No production source, measured probe or test
expectation changed. The initial lint failure log and result are retained under
`.superpowers/issues-1029-1030/initial-*`; final checks run after these edits.


## Final integration validation

All commands in [the validation receipt](validation.json) exited 0 on the final
source tree. Counts are per harness scope; contract-feature runs repeat relevant
workspace cases and must not be added as distinct coverage.

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 3,802 passed, 33 ignored |
| `cargo test -p hoimin-core --features contracts` | 845 passed, 2 ignored |
| `cargo test -p hoimin-cli --features contracts` | 2,422 passed, 28 ignored |
| Python suite, fixed seed 812 | 1,201 passed, 11 skipped |
| CLI-reference example tests | 7 passed |
| Workspace/vendor format and Clippy; Ruff; ty | Passed |
| Generated CLI-reference freshness and documentation site build | Passed |
| OKF YAML/reserved-file structure | 50 pages passed |
| New OKF source paths, footnotes and current hashes | 4 entries passed |

The first Python type check found that `hypothesmith` was missing from the shared
development environment. `uv sync --frozen --group fuzz --no-install-project`
installed the three locked fuzz dependencies; ty and the full Python suite then
passed. The first documentation build invocation used a scratch output path and
was rejected by the tool's existing dedicated-site safety guard. Re-running at
the required repository `site/` path passed. Neither correction changed product
source. Earlier Rust passes remained valid and were not repeated for these
environment and invocation corrections.
