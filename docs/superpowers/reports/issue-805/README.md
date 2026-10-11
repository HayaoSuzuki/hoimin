# Fuzzing adoption decisions for #805 and #807

Assessed on 2026-10-12 against main
`9debe7ef30ee0968f85e3a7c21282c973fafd548`.
See the [design](../../specs/2026-10-12-fuzz-adoption-design.md) and
[execution plan](../../plans/2026-10-12-fuzz-adoption.md).

## Decisions

| Candidate | Decision and evidence | Remaining work |
| --- | --- | --- |
| #803 corpus distillation | Continue a manual, bounded experiment on `python_analyzer`; initial replay is material even though compressed caches are small. | Merge and compare feature **identities** on one pinned Linux binary/snapshot before adopting any automatic maintenance. Not executed here. |
| #802 shared sampling property / Bolero | Keep proptest and cargo-fuzz. A dependency-free oracle works with both random adapters, but about a dozen existing assertion lines per harness do not justify a new framework or cross-directory helper yet. | Linux libFuzzer/Bolero comparison and shrinking equivalence remain unmeasured; reopen adoption only for a concrete replay or maintenance problem. |
| #804 target/event | Decline instrumentation for the proposed `ci_selection` target. Current path classification has directly expressible examples and a simple union of four flags, with no demonstrated difficult search boundary. | Revisit only with an actual missed semantic boundary, not a length-maximization objective. |
| #807 Fuzzing Book application | Existing structured generators, runners, lifecycle oracles and failure artifacts already implement the useful architectural separation. An initially suspected sampling gap was disproved by independent review: existing Lean-corpus integration tests already pin the output. | Record no additional justified change; remove the redundant trial vectors. Do not add a product API or fuzzing engine. |

These are maintenance decisions, not evidence of improved survivor discovery,
mutation score, or automatically correct agent-generated tests. #806 stays
independent. HypoFuzz, propfuzz, propverify and additional solver work are not adopted.

## #803: actual corpus costs

Downloaded only the 26,667-byte `fuzz-report` artifact (ID 11714353107) from
[successful run 38172184143](https://github.com/HayaoSuzuki/hoimin/actions/runs/38172184143).
The source SHA is the baseline above. The campaign used seed 3812445783,
nightly-2026-10-08, one fuzzer at a time, `-max_len=4096`, `-timeout=5`, and
`-rss_limit_mb=1024`. Its build stage took 91.789 seconds. The grammar and libcst
seed logs record Python 3.14.8, Hypothesis 6.168.3, Hypothesmith 0.3.3,
libcst 1.9.0 and lark 1.3.1. Both produced zero invalid/oversize outputs;
empty and existing inputs were already classified separately.

The [extracted evidence](ci-evidence.json) records all eight targets, raw-log
SHA-256 values, source identity and stage times. Corpus inputs include the
managed seed directory as well as the restored cache and generated inputs.

| Target | Initial files | Initial bytes | Files retained at INITED | Approximate initialization time | Stage wall time |
| --- | ---: | ---: | ---: | ---: | ---: |
| python_analyzer | 12,957 | 1,637,980 | 4,613 | 17 s | 55.496 s |
| report_sequence | 8,711 | 1,283,151 | 1,626 | 5 s | 58.297 s |
| analyzer_protocol | 5,062 | 451,895 | 1,077 | 1 s | 53.333 s |
| sampling | 20 | 240 | 17 | Not measurable from exec/s=0 | 72.221 s |

Initialization estimates divide INITED runs by its reported integer `exec/s`:
12,958/762, 8,712/1,742 and 5,063/5,063 respectively. These are rounded engine
time estimates, not direct isolated replay measurements. A zero rate does not
mean initialization was free. Lower retained file counts indicate redundancy
for that execution; they do not prove a persistent replacement preserves all
features or that every removed input is semantically unnecessary.

The cache API returned 20 fuzz snapshots at inspection: the earliest listed
was 2,146,805 bytes (2026-10-11 06:55 UTC), and the latest created was 3,489,846
bytes (20:37 UTC). The largest was 3,511,758 bytes. These are compressed archive
sizes, possibly from different branches/runs, not a controlled growth curve.
No cache was downloaded, deleted or replaced. Managed regression seeds remain.

For #803, use one representative snapshot and the same instrumented binary for
before/after replay, then a separate output directory for `-merge=1`. Cap the
whole manual experiment at 20 minutes, 1 worker and 1 GiB RSS, including replay;
abort without publishing a replacement on timeout, empty/partial output or
missing feature-identity evidence. Compare replay time and feature sets, not
just `cov`/`ft` counts. Keep managed seeds and failure inputs outside distillation.
The [LLVM documentation](https://llvm.org/docs/LibFuzzer.html#corpus)
describes coverage-based corpus merging; it does not supply hoimin's missing
before/after measurements. Automatic cmin in PR CI is not justified yet.

Reacquire diagnostics while retained:

```powershell
gh run download 38172184143 --name fuzz-report --dir .superpowers/issue-805-evidence
gh api --paginate 'repos/HayaoSuzuki/hoimin/actions/caches?per_page=100'
```

The downloaded report was nested under `_temp/fuzz-report`. Artifacts expire;
the committed extraction preserves the observations, not a replayable corpus.

## #802: smallest shared property and Bolero

The [standalone probe](probe/src/main.rs) imports the actual sampling module
without changing it. Its common oracle checks count, uniqueness, range and
prefix against a full sample. Both adapters consume `(u16, u16, u64)` and map
population modulo 512 and count modulo 1024, exercising the same bounded domain.
This matches neither production harness's exact generation distribution:
the existing fuzz harness allows populations up to 4096, while the unit
property uses population <512 and count <1024. No domains were reduced in CI.

On Windows x86_64 MSVC, Rust 1.99.0, Python 3.14.6, the initial Cargo build
reported 37.74 seconds (Cargo-reported elapsed time; acquisition costs were not timed separately).
The isolated lockfile resolves 67 dependency packages; it changes neither
product workspace nor the fuzz workspace. Bolero is pinned to 0.13.7 and
proptest to the baseline's 1.11.0. This is a research-only crate with
`publish=false`, not a newly supported maintenance tool or mandatory CI job.

With fixed seed 1, 256 cases each, proptest completed in 0.0441 seconds and
Bolero's default random engine in 0.0363 seconds, including process launch.
These single, tiny runs establish adapter viability only; they do not establish
a speed advantage, equivalent generated inputs, coverage, shrinking quality,
or better bug detection. [Raw results](probe-results.json) preserve outcomes.

Fixed replay `(8,3,42)`, empty input and maximum bounded input all passed.
Four deliberately corrupted result fixtures independently failed count,
uniqueness, range and prefix checks (exit 101). These validate the oracle,
not either engine's ability to discover or minimize a bug. No real defect was
found by the randomized probes. Fixed tuple replay bypasses engine-specific
corpus formats; corpus exchange and failure shrinking were not tested.

The [Bolero API](https://docs.rs/bolero/0.13.7/bolero/)
offers its own typed generators and engine interface. It is useful technology,
but does not remove the adaptation needed for existing proptest strategies or
the libFuzzer input format. Shared assertions are possible without it. With
only this small duplicated oracle, keep the simpler existing harnesses.

Reproduce on Windows from the repository root:

```powershell
cargo build --locked --manifest-path docs/superpowers/reports/issue-805/probe/Cargo.toml --target-dir .superpowers/issue-805-target -j 1
$env:BOLERO_RANDOM_SEED = '1'
.superpowers/issue-805-target/debug/hoimin-sampling-research.exe proptest
.superpowers/issue-805-target/debug/hoimin-sampling-research.exe bolero
.superpowers/issue-805-target/debug/hoimin-sampling-research.exe replay
# Other modes: empty, full; corrupt-count/unique/range/prefix must exit nonzero.
cargo clean --manifest-path docs/superpowers/reports/issue-805/probe/Cargo.toml --target-dir .superpowers/issue-805-target
```

Run each probe under a 60-second parent timeout as in the assessment. Rustfmt
and Clippy with `-D warnings` passed for this crate. Linux libFuzzer trials were
not run: WSL Ubuntu-24.04 exists but no Rust/clang toolchain was available on the
checked paths. Installing one solely for this tiny oracle was stopped at the
design's platform gate. No claim about Linux feature preservation follows.

## #804 and #807: concrete application gaps

`tools/ci_selection.py` already has example coverage for path categories,
mixed changes, empty diffs, rename/deletion and conservative fallback.
No filtering bottleneck or rare semantic condition requiring an optimization
objective was demonstrated. A union/permutation property would largely repeat
the current OR implementation here. Neither `target()` nor `event()` was added;
ordinary Hypothesis events are observations, not a replacement exploration engine.
See the [public Hypothesis API](https://hypothesis.readthedocs.io/en/latest/reference/api.html#hypothesis.target).

The [Fuzzing Book's architecture](https://www.fuzzingbook.org/html/Fuzzer.html)
separates generation and execution, and its
[reduction chapter](https://www.fuzzingbook.org/html/Reducer.html) motivates
preserving a failure while simplifying inputs. Existing hoimin targets already
have independent scalar coordinate checks, constructive encoding/candidate
inputs, JSON round trips and stateful lifecycle validation. `ci_fuzz` already
saves argv, seed, timeout, exit status, logs and partial stage outcomes;
Hypothesmith reports generator/environment versions. No failure demanding a
new reducer, tracer, input format or generic runner abstraction was found.
All probe code here is original; no book text/code or HypoFuzz implementation
was imported.

An initial hypothesis was that `splitmix64_fisher_yates_v1` had no fixed
compatibility outputs. That hypothesis was wrong. Independent review found
`sample_public_cli_matches_every_lean_oracle_case` in
`crates/hoimin-cli/tests/sampling.rs`, which checks selected IDs and order against
360 committed Lean-derived cases. The corpus already includes all three full
permutations proposed by the trial (seeds 0, 42 and u64::MAX), plus seed 1.
A changed PRNG constant can pass the generative property but is already covered
by this independent integration oracle. See the [rejected trial](../issue-807/README.md).
No additional product-code/test change was justified by this assessment, so the
redundant trial vectors and their temporary test dependency were removed.

## Implementation self-review, five separate passes

1. Source boundaries: the probe imports current sampling code and has its own
   workspace/lockfile; root and fuzz manifests were not changed.
2. Oracle checks: verified the four corruption modes fail distinct assertions;
   avoided reporting those artificial failures as product bugs.
3. Adapter conditions: verified identical bounded domains and 256-case caps;
   fixed seeds while retaining engine/distribution limitations in the report.
4. Corpus evidence: checked log hashes and all eight stage names; separated
   rounded replay estimates from unmeasured merge savings/feature preservation.
5. Adoption: retained independent rejection tests and rechecked the suspected
   known-answer gap against the full Lean corpus after independent review, and
   removed redundant trial tests rather than claiming new coverage.

## Test self-review, five separate passes

1. Checked zero exit status for replay, empty/full boundaries and both adapters.
2. Checked nonzero exit status and the failing assertion for each corruption;
   a build failure was not accepted as fault sensitivity.
3. Checked exact versions/seeds and the standalone lockfile; no claim of
   statistically meaningful speed/coverage improvement from one trial.
4. Checked formatting and strict Clippy; did not run unrelated expensive
   integration tests or the ignored 65,536-seed frequency inspection.
5. Checked explicit gaps: Linux engines, corpus merge, feature identity and
   shrinking were not tested; #803 remains a bounded follow-up.

## Final verification and independent review

The independent reviewer initially rejected the missing-compatibility-coverage
claim. After reading the full Lean-corpus integration test, the author withdrew
the redundant trial tests and corrected both reports. The same fresh reviewer
rechecked the corrections and returned ready with no important findings remaining
on 2026-10-12. Review was read-only; no heavyweight suite was rerun by the reviewer.

Final-state standalone build, Rustfmt and strict Clippy passed. Existing
`tests/test_ci_selection.py` and `tests/test_docs_site.py` passed (81 tests,
5.55 seconds). OKF structure, new source hashes/footnotes and changed-page local
links passed. Product crates, fuzz targets, tools, tests and product lockfiles
have no diff against the assessed main. No product behavior was implemented.

The dedicated target directory was cleaned with `cargo clean`: 1,378 files,
433.7 MiB removed. C: free space afterwards was 187,179,016,192 bytes (about
174.3 GiB). Shared build directories and corpus caches were not deleted.
