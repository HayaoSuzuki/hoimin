# Runtime performance and maintenance improvements

## Approved scope and execution plan

Baseline: `5b80f08`, Windows, Rust 1.99, Python 3.14. The user approved
implementing #813–#817 on separate work branches after updating main. Preserve
public formats, safety policies, diagnostics, candidate identity and ordering.
Do not add a framework, new input format, or speculative architecture.

1. #813: measure candidate counting, encoding and storage separately. Compare
   the existing two-pass push with a bounded single-pass encoder. Keep discovery's
   allocation-free size check. Reuse JSONL, Unicode and exact-limit tests; verify
   the new writer's refusal before growing beyond its byte budget.
2. #814: measure sequential spool replay. Consider reader ownership only if the
   saving justifies preserving replacement/truncation detection, cursor semantics
   and Windows cleanup. Otherwise retain the current implementation and evidence.
3. #815: measure parsing, depth validation, facts and candidate construction on
   bounded small and real-project inputs. Keep only demonstrated improvements;
   preserve source guards, cancellation and candidate-prefix semantics.
4. #816: move bounded file reading to a neutral internal owner and give shared
   report parsing an owner independent of progress/review command orchestration.
   Keep plan validation separate from run-report validation and do not broaden
   review's accepted formats or impose its 64 MiB limit on progress.
5. #817: centralize version syntax and the existing three platform identities in
   a dependency-free Python policy module. Derive release/SBOM/wheel expectations
   from it while preserving contracts and signature checks in their consumers.

Each task records five design, plan, implementation and verification reviews.
Existing meaningful tests take precedence over artificial failing tests that
only pin private structure. New behavior or boundary responsibilities receive
focused tests before implementation. Final independent review is required.

## Resource and measurement policy

Use one owned Cargo target (`.superpowers/runtime-improvements/target`), at most
two build jobs, no incremental compilation and no debug information for ordinary
checks. Release measurements use the same profile for both versions. Run one
measurement process at a time, bound inputs and repetitions, check disk and process
memory, and clean only the owned target at phase boundaries. Do not duplicate full
build trees. Stop a measurement on failure or timeout rather than report it as a
speedup. Retained probes and raw observations must be sufficient to reproduce
reported numbers; distinguish local costs from plan/run totals.

## Design self-review (five passes)

1. Compatibility: the spool remains compact JSON plus one newline, with identical
   sequence, count, cursor and ID rules. No Arrow or schema migration.
2. Failure handling: reject an oversized encoding before writing; counts advance
   only after successful writes. Keep I/O errors and cleanup behavior observable.
3. Memory: a single record is bounded including its newline; discovery still
   counts without encoding into an unbounded buffer. Avoid caching an entire spool.
4. Evidence: code duplication alone does not establish a hotspot. Local storage,
   analysis and external test time must be reported separately.
5. Boundaries: share format knowledge, not command orchestration. Preserve review's
   historical-source behavior and verify's current-source/fingerprint checks.

## Plan self-review (five passes)

1. Baseline: main fast-forwarded to `5b80f08`; clean branch
   `perf/813-candidate-store` created before retained edits.
2. Dependencies: #813–#815 have a comparison order, not mandatory feature
   dependencies. #816 and #817 can be reviewed independently.
3. Tests: reuse existing corpus/property/boundary checks; add only tests for new
   resource responsibilities. Preserve AAA and avoid conditional test scenarios.
4. Reproducibility: record build profile, input sizes, repetitions, exit status,
   CPU/wall time and process peak memory; separate compile time from runtime.
5. Delivery: commit English evidence with each change, use stacked PRs where
   helpful, retain investigation-only outcomes, and clean owned build artifacts.

## Execution ledger

- Planning: approved in chat; native inline execution, followed by an independent
  final review. No uncommitted user changes were present.
- Resource baseline: approximately 172 GB free on C:. System-wide free-memory
  query is unavailable in the sandbox; do not infer a free-memory value from it.

## #813: keep the existing encoder

The bounded single-pass prototype is **not adopted**. For ordinary small records
it was slower, while the full CLI comparison showed no meaningful improvement.
The production `store.rs` is unchanged. Retain the
[experimental patch](probes/813-bounded-encoder.patch) for reproducibility, not as
a recommended implementation.

Windows, Rust 1.99, locked dependencies, normal release profile (thin LTO), two
build jobs. The initial test build took 135 seconds; release builds took 286 and
163 seconds. Build time is excluded from the following observations. Each store
scenario ran before (three times), after (three times), then before again (three
times), with no concurrent build. These are warm filesystem-cache observations;
the spool was freshly written before each replay. Other desktop applications
were not disabled, and these small samples do not establish confidence intervals.

| Storage scenario | Records | Existing median | Prototype median | Change |
| --- | ---: | ---: | ---: | ---: |
| 32 ASCII characters | 100,000 | 0.2591 s | 0.3177 s | 22.6% slower |
| 32 repetitions of escaped/Unicode text | 50,000 | 0.1801 s | 0.2122 s | 17.8% slower |
| 256 KiB ASCII payload | 256 | 0.0564 s | 0.0416 s | 26.2% faster |

Storage times measure the push loop: encoding and file writes, excluding store
creation, final flush/sync and cleanup. The counting and Vec-encoding probes are
separate diagnostic operations, not subtracted measurements of production
`record_size`: the diagnostic counter does not implement its overflow/boundary
checks. Original medians were respectively 0.0122/0.0310 s (small),
0.0143/0.0316 s (escaped) and 0.0153/0.0204 s (large). The prototype adds a
per-chunk budget check and capped buffer growth; removing a serializer pass alone
does not guarantee a faster push.

Alternating three before/after pairs on a synthetic 120-function source gave:

| Command | Existing wall median | Prototype wall median | Process CPU median (before / after) |
| --- | ---: | ---: | ---: |
| plan, 1,080 candidates | 0.0283 s | 0.0281 s | 0.0156 / 0.0156 s |
| run, three executed candidates | 1.0398 s | 1.0276 s | 0.5313 / 0.5625 s |

The plan path does not call push, so it is a control rather than evidence of a
storage speedup. Run returned the expected exit 4 because the three-mutant budget
left 1,077 candidates unexecuted: baseline passed, two mutants were killed, one
survived, no timeout/error/resource stop occurred. The complete plan and all
1,080 candidate/status/termination projections matched between versions.
Do not interpret roughly 12 ms less wall time, with higher CPU and noisy pairs,
as a demonstrated product improvement.

Store-process peak working set was at most 5.6 MB; CLI peak was at most 14.3 MB.
These are whole-process observations, including setup/cleanup, and exclude child
process memory. Workers had a 256 MiB limit, one job, 30 seconds total and a 64 MiB
workspace budget; reported workspace peak stayed below 1 MiB. A normal-permission
system query showed about 5 GiB free RAM; observed rustc peaks stayed below 1 GiB.

### Reproduction and evidence

1. Check out `5b80f08` and copy [the Rust driver](probes/candidate-store.rs) to
   `crates/hoimin-cli/examples/profile_candidate_store.rs`.
2. Set `CARGO_TARGET_DIR` to a task-owned directory, `CARGO_BUILD_JOBS=2`,
   `CARGO_INCREMENTAL=0`, and build with
   `cargo build --locked --release -p hoimin-cli --example profile_candidate_store --bin hoimin`.
3. Preserve the two executables outside the Cargo target; apply the experimental
   patch and repeat the same build. Do not run measurements during compilation.
4. With Python 3.14 on Windows run
   `python docs/performance/probes/store-matrix.py EXE OUTPUT_DIRECTORY LABEL`
   for each driver. Run
   `python docs/performance/probes/cli-pairs.py BEFORE.exe AFTER.exe OUTPUT_DIRECTORY`
   for the full CLI pair. Use literal paths in place of uppercase placeholders.
5. Remove the copied example and reverse the patch after reproducing the trial.

[Compressed raw observations](runtime-2026-10-12/store-observations.json.gz)
include argv, wall/CPU time, peak RSS, exit status and complete output. Windows
process CPU accounting has coarse resolution for very short commands. Store
inner-loop wall time and whole-process CPU have different scopes, especially
for replay whose preparation writes the spool.

### Implementation self-review (five passes)

1. Bounds: rejected chunks precede allocation; geometric capacity is capped;
   reserve the newline before the final push to prevent doubling at the limit.
2. Identity: count/sequence checks remain before encoding; successful records
   match the existing compact JSON plus newline, including escaped Unicode.
3. Failure: typed record-size errors and unchanged counts on failed encoding or
   writes are preserved; the temporary prototype passed all ten store tests.
4. Scope: discovery's size check was not removed; no whole-spool memory cache or
   public format change was introduced.
5. Adoption: normal-sized records regressed. Reverse only this experiment's
   patch and retain evidence; do not ship unused writer code or its extra tests.

### Verification self-review (five passes)

1. Baseline: seven existing store tests passed. Three new writer-boundary tests
   failed before the type existed, then all ten store tests passed.
2. Boundaries: existing tests cover exact limits including newline, Unicode,
   failed writes/counts and cleanup; new checks exercise pre-allocation refusal.
3. Measurements: repeated samples exclude build time and failed commands. The
   first CLI probe incorrectly supplied plan's nonexistent `--format`; another
   rejected the intentional budget exit 4. Neither is a valid timing sample.
4. Equivalence: complete plan equality and all mutant descriptor/status/termination
   projections matched in the valid paired experiment.
5. Environment: sandboxed full CLI unit tests had 732 passes, 123 failures and
   16 ignored, including managed-root access denial. This is not a passing suite;
   native-permission final verification remains required for retained code work.

Ruling: retain investigation only for #813 — a large-record microbenchmark win
does not justify slowing the usual small-record path. Revisit only if real input
profiles demonstrate a different workload. The trial's cost and limitations are
recorded instead of claiming a general optimization.

## #814: retain stateless replay for small batches

Do not retain a spool reader in the production shell now. The original replay
loop for 100,000 small records took a median 2.519 seconds (six observations),
but the primary workflow verifies small selected batches. Seven additional
observations with freshly prepared spools gave:

| Replayed records | Minimum | Median | Maximum |
| ---: | ---: | ---: | ---: |
| 10 | 1.20 ms | 1.45 ms | 10.45 ms |
| 100 | 10.90 ms | 12.66 ms | 15.13 ms |
| 1,000 | 33.68 ms | 34.60 ms | 36.88 ms |

The samples are noisy, especially the first small batch, and filesystem cache
conditions are not a cold-storage benchmark. Even eliminating the complete
measured ten-record operation would save only about 1.45 ms in its median sample.
This is an observation for these inputs, not a worst-case bound or a guarantee
that large workloads cannot benefit. The three-mutant CLI run above took about
one second. Do not scale the 100,000-record average linearly to small batches.

Source inspection confirms one open and metadata query per replay call, a seek
and previous-newline check for a nonzero offset, and a fresh BufReader and line
Vec. These are source-level operations, **not measured syscall/allocation counts**.
Read-ahead may repeatedly read bytes beyond the requested record. A retained
buffer could avoid those operations, but can also return cached bytes after
in-place modification; an open handle can continue reading an old file after
path replacement (or prevent replacement/deletion on Windows). Comparing length
alone does not detect same-length corruption. Preserving all existing observations
would require an explicit ownership/integrity protocol or invalidation work,
along with shell cancellation and cleanup changes.

Ruling: do not build a retained-reader prototype solely to improve a millisecond
operation in the selected small-batch workload. The current measured cost does
not justify weakening checks or adding lifetime/invalidation machinery. No
reader variant or syscall/allocation profiling was performed, and no speedup is
claimed. Reconsider with an actual large-batch user bottleneck. This narrows the
original experiment plan based on measurements rather than completing every
possible probe regardless of its value.

[Small-batch observations](runtime-2026-10-12/replay-small-batches.json) contain
argv, time, memory, status and driver output. Reproduce each sample with the
original driver using `replay COUNT 32`, for COUNT 10, 100 and 1000, seven times.
The driver bounds records/payloads and removes its spool directory on exit.

### #814 design self-review (five passes)

1. Scope: candidate spool replay is separate from public progress-report input.
2. Integrity: preserve offsets, previous newline, sequence, count and EOF checks.
3. Ownership: retaining an open handle changes path-replacement and Windows
   cleanup behavior; a reader cache is not just a local BufReader optimization.
4. Memory: avoid entire-spool retention and account for the bounded line buffer
   plus read-ahead if a future reader is introduced.
5. Value: measure the intended small-batch use before paying for new state.

### #814 plan self-review (five passes)

1. Baseline: the original production store remains unchanged after #813.
2. Inputs: measure three bounded batch sizes with seven repetitions each.
3. Isolation: no builds or other measurement tasks run during samples.
4. Interpretation: distinguish the inner replay loop from preparation and process
   startup; avoid pretending source operation counts are OS measurements.
5. Stop rule: permit investigation-only delivery when the total saving is small.

### #814 implementation self-review (five passes)

1. Probe: use the actual public replay implementation without a mock reader.
2. State: count/sequence setup and the START cursor match normal replay.
3. Budget: at most 1,000 small records for the additional probe; no large cache.
4. Cleanup: store/reader handles are dropped before the owned temp directory.
5. Outcome: no runtime change, new dependency or unused reader abstraction.

### #814 verification self-review (five passes)

1. All 21 probes returned zero and reported the expected count/mode.
2. Retain raw observations and report min/median/max rather than the best sample.
3. Review existing analyzer_handler tests for changed/truncated records, invalid
   offsets/sequences and caller-owned directory lifetime; no assertions removed.
4. Do not claim safety equivalence of an unimplemented reader variant.
5. Keep native-permission full-suite verification in the final retained-code
   stage; this investigation adds no production code or redundant regression test.

## #815: measured analysis stages, no retained optimization

Use a temporary instrumentation patch to distinguish source read/decode,
parse, depth validation, facts, token candidates, AST candidates, type candidates,
merge and candidate validation/record sizing. The
[discovery driver](probes/analysis.rs) calls the real public `discover_targets`,
with a 10,000-candidate budget and seven repetitions in one process. Report the
median of the six repetitions after the initial warm-up. Timers measure wall
time on the discovery worker, **not sampled CPU stacks** or allocation counts.

[Input inventory](runtime-2026-10-12/analysis-inputs.json) pins revisions, paths,
byte counts and SHA-256. Inputs are proofreadtools's 2,281-byte variant helper,
Django's 100,029-byte `django/db/models/base.py`, and a reproducible 57,559-byte
source containing 1,000 simple functions. No fetched project code was executed,
imported or added to product inputs. These single files are not a benchmark of
either complete project, pandas, or CPython.

Warm default/full measurements, in milliseconds:

| Input | Candidates | Total discovery | Parse | Facts | Token work | AST candidates | Validate + size |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| proofreadtools | 31 | 0.700 | 0.094 | 0.042 | 0.026 | 0.093 | 0.050 |
| Django model base | 1,309 | 8.265 | 2.063 | 0.515 | 0.692 | 1.556 | 1.841 |
| generated functions | 9,000 | 27.164 | 2.733 | 1.317 | 1.982 | 5.680 | 10.804 |

Other Django medians: source read 0.204 ms, decode/source-validation context
0.211 ms, depth 0.140 ms, merge 0.218 ms. Generated-input equivalents were
0.220, 0.113, 0.205 and 2.033 ms. Type-candidate generation was below 0.001 ms
for these inputs/selections. Token work includes constructing its line/token
indexes. Validate + size includes byte-span conversion, stable identity and
candidate guards, JSON size validation, and retention in discovery's Vec; it
does not include CandidateStore::push or writing a spool. Individual medians
must not be summed to reconstruct the median total. Total also includes root
capability setup, scheduling, drops and instrumentation output.

Restricting selection to `boolean_literal` gave warm discovery medians of
0.568/4.192/7.034 ms for proofreadtools/Django/generated, retaining 0/55/0
candidates. The existing selector checks already reduce AST work; the source
must still be parsed and facts validated. The focused profile retained
31/1,289/9,000 candidates, with warm totals 0.690/8.037/26.395 ms. It is not
expected to substantially accelerate sources whose candidates it does not omit.
Process peak working set remained below 17.1 MB across the nine configurations.

Ruling: retain phase evidence, not a traversal rewrite. In the larger real file
parse and candidate validation/construction matter at least as much as AST
candidate generation; facts are not the dominant cost. Existing context/index
reuse and early selector checks are present. The generated case spends most time
in necessary candidate work, not a demonstrated redundant walk. A few
milliseconds per file do not justify introducing caches, skipping validation,
or changing traversal/order semantics without an actual workload bottleneck.
These results do not rule out future improvements on different input scales.

### Reproduction

1. Run [fetch-analysis-inputs.py](probes/fetch-analysis-inputs.py) with authenticated
   gh to fetch the pinned files and generate the synthetic input, each below
   1 MiB. Source code is parsed only.
2. Apply [the temporary timing patch](probes/815-analysis-timing.patch), copy
   `analysis.rs` to `crates/hoimin-cli/examples/profile_analysis.rs`, and build
   that example with the same locked release profile and two-job resource policy.
3. Run `python docs/performance/probes/analysis-matrix.py EXE instrumented`.
   Each configuration has a 60-second process deadline. This probe calls discovery,
   not external test execution; its memory observations are not a hard memory cap.
4. Reverse the instrumentation patch, rebuild the same example and run the matrix
   again with label `control`. Compare candidate hashes, counts and diagnostics;
   remove the copied example afterwards. Profiling must not become production
   logging or a new runtime input.

[Instrumented observations](runtime-2026-10-12/analysis-instrumented.json) retain
all seven samples and per-phase stderr. Formatting/logging adds overhead to
discovery totals, especially the smallest source; phase times are approximate
diagnostics rather than a performance guarantee.

The [uninstrumented control](runtime-2026-10-12/analysis-control.json) matched
all 63 candidate hashes, counts, truncation flags and diagnostics, and emitted no
profiling stderr. Its default/full warm medians were 0.526/7.466/25.120 ms;
boolean-only medians 0.478/3.880/6.922 ms; focused medians
0.465/7.496/24.795 ms. The instrumentation therefore affects totals and must not
be used as evidence of an optimization. No runtime source changes are retained.

After these measurements, `cargo clean --release` on the verified task-owned
target removed 1,837 files / 742.2 MiB. Debug artifacts remain for the refactors;
approximately 171 GiB disk space is free. Frozen experiment executables in the
task scratch directory are retained only until final review.

### #815 design self-review (five passes)

1. Scope: separate parsing, indexing, candidate production and validation rather
   than call every walk redundant.
2. Safety: keep syntax disposal, depth checks, source identity and cancellation.
3. Scale: bound individual source size, candidates, repetitions and deadline.
4. Selection: inspect default, narrow operator and focused profile independently.
5. Adoption: only retained code with demonstrated useful savings would be justified.

### #815 plan self-review (five passes)

1. Revisions: fixed upstream commits and content hashes, not moving main branches.
2. Comparability: same driver/build profile, warm-up separated from six repetitions.
3. Phases: capture actual paths and exclude post-timer output hashing from discovery.
4. Controls: rebuild without timers and compare exact candidate hashes/diagnostics.
5. Delivery: preserve reproducible patches and measurements, remove instrumentation.

### #815 implementation self-review (five passes)

1. No replacement parser, test double, or imported upstream dependency.
2. Timers surround existing stages without changing candidate production order.
3. No collected candidate cache or schema/operator extension.
4. Exact counts, truncation flags, diagnostics and candidate hashes are recorded.
5. Treat the temporary instrumentation as a probe, not shipped observability.

### #815 verification self-review (five passes)

1. Nine instrumented configurations returned zero, each with seven observations.
2. Each configuration had stable candidate hashes across its repetitions.
3. No case reached the 10,000-candidate limit; default/full counts were
   31/1,309/9,000, so an incomplete prefix was not mistaken for a full comparison.
4. Report warm wall time, instrumentation overhead and process peak RSS honestly;
   do not label timers a CPU-stack or whole-project profile.
5. Native-permission retained-code verification and independent review remain
   final gates; no extra corpus tests are needed for removed instrumentation.

## #816: artifact contracts own their readers

Retained implementation: a private `artifacts` module owns bounded regular-file
reading and run-report format validation. `plan` owns plan header/identity checks;
`review` owns source freshness, assessment and report-to-plan comparison;
`progress` owns ordered comparison, duplicate-input policy and rendering.

| Knowledge | Owner |
| --- | --- |
| Regular-file and growth-aware byte limit | `artifacts::bounded_read` |
| JSON/JSONL run schema, sequence and summary validation | `artifacts::run_report` |
| Current-schema read-only review projection | `artifacts::run_report::read_review_json` |
| Plan schema, descriptors, sources and verify selection | `plan` |
| Progress input disposition and duplicate handling | `progress` |
| Historical-source assessment and protected destinations | `review` |

The report decoder and its JSONL/dispatch modules moved without changing their
validation bodies. Fingerprinting moved beside reading; duplicate confirmation
remains with progress. Existing public progress exports are re-exported, not
wrapped in extra forwarding functions. The historical `ProgressError` type name
and messages remain for compatibility. Read-only plan error formatting preserves
its historical review-prefixed diagnostics locally; the shared reader itself
has no dependency on plan, progress or review.

The 64 MiB constant applies to the existing read-only plan/review/source paths;
progress retains its existing per-format reading behavior and no new size limit.
Verify's fingerprint/current-source checks, request/real-path protection and
review's ID/descriptor comparisons did not move into a generic permissive reader.
No new accepted format, schema, input path or CLI option was introduced.

### #816 design self-review (five passes)

1. Dependencies: artifacts does not import command modules; plan no longer calls
   review's file reader, and review no longer calls progress's decoder.
2. Format ownership: plan and run reports remain separate contracts; no universal
   serializer/plugin abstraction.
3. Limits: regular-file checks, initial metadata and bounded growing-file read
   remain; the read-only limit is not imposed on progress.
4. Semantics: review tolerates historical source states intentionally; verify
   still requires current-source and fingerprint validation before execution.
5. Compatibility: retain public exports, all serde attributes, error categories,
   diagnostic text and JSONL streaming behavior.

### #816 plan self-review (five passes)

1. Move format bodies together with private dispatch/JSONL modules and hash reader.
2. Keep disposition/duplicate policy in progress rather than importing it back
   into the new artifact owner.
3. Convert neutral read errors at the caller boundary, preserving legacy messages.
4. Add only exact-size arbitrary-byte, oversize and missing-file tests for the
   new shared read responsibility; reuse existing review/plan/oracle/heap tests.
5. Verify native Windows behavior, strict lint and workspace tests before delivery;
   do not mistake sandbox access failures for product regressions.

### #816 implementation self-review (five passes)

1. Compare moved report validator/JSONL/dispatch bodies with their original files.
2. Check scope of every former `pub(super)` method after moving modules.
3. Check the neutral error conversion preserves review Io versus Invalid variants.
4. Keep protected destination paths, candidate validation and verify logic intact.
5. Search for reverse command dependencies and unnecessary forwarding functions.

### #816 verification self-review (five passes)

1. New shared-reader tests failed before implementation and passed in the native
   unit run; the property test uses 32 cases of at most 1,024 arbitrary bytes.
2. Full CLI unit run: 855 passed, zero failed, 16 ignored in 27.45 seconds.
3. This native run resolves the earlier sandbox-only managed-root access failure;
   do not delete or weaken the failing security/ownership checks.
4. Existing partial-final-read fingerprint test moved with its production reader.
5. Full workspace, strict lint, contract-feature checks and independent final
   review are recorded at delivery; they are not inferred from unit success.

## #817: dependency-free release policy

`tools/release_policy.py` now owns version syntax and the existing three platform
identities, Rust targets, assembly wheel patterns and exact published wheel tags.
Release assembly, SBOM generation, PyPI staging and provenance wheel counts derive
their expectations from that table. Existing release version contracts remain in
`release.py`; cryptographic verification remains in `pypi_provenance.py`.

Assembly intentionally accepts broader compatible wheel tags than stable PyPI
staging. The refactor preserves that distinction, all metadata/license/checksum
checks and the historical three-wheel diagnostic. Direct script invocation and
module invocation both resolve the same policy beside their source. The policy
does not load external configuration or generate GitHub workflow definitions.

The new regression test runs the staging entry point with Python's site packages
disabled. It failed before the change because importing the version pattern
indirectly imported `deal`. Existing staging byte-identity and rejection tests
continue to cover actual wheels; the new case checks only dependency isolation.

### #817 design self-review (five passes)

1. Keep a pure data owner with standard-library types rather than a release framework.
2. Keep stable PyPI tags distinct from compatible assembly patterns.
3. Keep version contracts and signature checks at their existing execution boundaries.
4. Preserve direct script imports from arbitrary working directories.
5. Preserve workflow targets and distribution inputs; no release or version bump.

### #817 plan self-review (five passes)

1. First demonstrate the accidental staging dependency with site packages disabled.
2. Move policy data only, deriving compatible names used by existing callers.
3. Replace the independent provenance count with the policy's platform count.
4. Reuse wheel metadata, checksum, SBOM and provenance rejection tests.
5. Check strict lint, typing, both invocation modes and the complete Python suite.

### #817 implementation self-review (five passes)

1. Compare all three platform names and Rust targets against the release workflow.
2. Compare broad assembly regexes and escaped exact staging tags with their originals.
3. Check stable and development version expressions retain their original boundaries.
4. Check no tooling dependency or network operation enters the pure policy module.
5. Review the complete diff: signing, publisher identity and SBOM validation stay intact.

### #817 verification self-review (five passes)

1. Confirm the regression's initial failure is missing `deal`, not a fixture error.
2. Verify the new entry-point check alongside existing byte-preserving staging tests.
3. Verify existing release and SBOM direct-script fixtures from temporary directories.
4. Verify all strict Python lint/format/type checks without adding suppressions.
5. Record full-suite results and independent review below before delivery.

Focused Python verification: 146 passed, 10 skipped in 86.43 seconds, including
the new site-packages-disabled entry point. Ruff checks, Ruff format checks and
strict ty checks passed across the repository. Full Python/Rust results and the
independent review are recorded below as separate completion gates.

## Final verification and independent review

Retained runtime changes were reviewed at `7c6f10f5d4864aa432c35d7585a8ccf6549f4c71`
against baseline `5b80f08`. A fresh independent read-only reviewer reported no
critical, important or minor findings. It confirmed the moved validator bodies,
the 63 control/instrumented analysis observations and the 21 small-replay samples.
It did not reproduce benchmarks, execute tests, contact publication services or
infer performance on other machines. Full-suite/build results below were checked
by the implementer after that review; remaining changes are delivery evidence.

| Check | Result |
| --- | --- |
| `cargo test --locked --workspace` | 2,793 passed, 27 ignored; exit 0 |
| `cargo test --locked -p hoimin-core -p hoimin-cli --lib --features contracts` | 891 passed, 18 ignored; exit 0 |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| Full Python suite, seed 817 | 981 passed, 29 skipped, 282.51 s; exit 0 |
| Release/SBOM/staging/provenance Python tests, seed 813 | 146 passed, 10 skipped, 86.43 s; exit 0 |
| Ruff check, Ruff format check and strict ty | exit 0 |
| `python -m tools.docs_site --output site` | exit 0 |
| `git diff --check` | exit 0 |

Rust counts exclude nested child-test summaries, which would double-count two
cases. The contracts run covers library tests, not a second execution of all
workspace integration tests. Hosted platform builds and actual publication were
not executed locally. No formal model changed, and no new proof claim is made.

Native Windows permissions were needed for existing managed-root fixtures. The
full Python command was `python -m pytest -q --randomly-seed=817
--basetemp=.superpowers/runtime-improvements/pytest-full --tb=short --maxfail=1`.
Rust used two build jobs and test threads, disabled incremental compilation and
dev/test debug information, and the task-owned target directory. Compressed
[workspace log](runtime-2026-10-12/workspace-816.log.gz),
[contracts log](runtime-2026-10-12/contracts.log.gz),
[Python log](runtime-2026-10-12/python-full.log.gz),
[focused Python log](runtime-2026-10-12/policy-green.log.gz),
[regression RED log](runtime-2026-10-12/policy-red.log.gz),
[Clippy log](runtime-2026-10-12/clippy.log.gz) and
[public build log](runtime-2026-10-12/public-docs-build.log.gz) retain the evidence.

The initial direct MkDocs invocation bypassed the repository's public-page
staging and produced 190 warnings. It is not the supported publication command.
The CI-equivalent `tools.docs_site` build above passed without changing checks;
this resolves the documentation gate noted by the independent reviewer.

Resource checks observed approximately 4.5 GiB free RAM during verification;
parallelism remained at two. After an earlier 742.2 MiB release-output cleanup,
the final `cargo clean` removed 4,643 files / 4.6 GiB from the verified task-owned
target only. Free disk space afterward was 182,663,012,352 bytes. The user's other
Cargo outputs were not cleaned. See the [cleanup log](runtime-2026-10-12/clean-final.log.gz).

## Philosophy review follow-up

Approved bounded scope: clarify the shared run-report reader contracts and derive
the staged-wheel count diagnostic from release policy. Keep the existing error
types and reader behavior. This follow-up is stacked above #823.

### Follow-up design self-review (five passes)

1. Describe actual current acceptance rather than inventing a shared acceptance policy.
2. State the review reader's current-schema, complete, passing-baseline requirement.
3. Expose project-root filesystem resolution and caller-owned byte limits in its contract.
4. Keep compatibility reexports and the broader progress reader intact.
5. Derive the diagnostic count from the same table as the existing count check.

### Follow-up plan self-review (five passes)

1. Create a separate work branch above the existing stack tip before editing code.
2. Read JSON/JSONL dispatch, validation and review call sites before writing comments.
3. Change only comments and the count diagnostic; do not split error types.
4. Reuse provenance rejection/signature tests and strict lint; avoid tests that only mirror wording.
5. Obtain independent review, record verification and append a PR to the stack.

### Follow-up implementation self-review (five passes)

1. Check the comments against schema dispatch and whole-document validation.
2. Check usability conditions against baseline termination and completion checks.
3. Check project-root resolution and byte-budget ownership against review call sites.
4. Compare Rust content after excluding comments: runtime code matches parent `c2858b4`.
5. Confirm the only Python change derives diagnostic text from `RELEASE_PLATFORMS`.

### Follow-up verification self-review (five passes)

1. Run existing provenance tests, including download/signature and rejection policies.
2. Run repository-wide Ruff, Ruff format and strict ty checks.
3. Run Rust formatting and whitespace checks; avoid rebuilding 4.6 GiB for comments.
4. Distinguish this focused verification from the parent's full-suite evidence.
5. Obtain fresh independent review of this follow-up before submitting the PR.

Follow-up verification: all 48 provenance tests passed in 1.74 seconds (seed
825), repository-wide Ruff/format/ty checks passed, and Rust formatting plus
whitespace checks passed. No Rust runtime content changed, so the parent's full
workspace results remain historical evidence rather than a repeated test claim.
No Cargo compilation outputs were created after the preceding cleanup; observed
free disk space was 182,715,891,712 bytes.

A fresh independent reviewer checked `c2858b4..19fb98e` and reported no findings.
It confirmed schema/usability comments, filesystem resolution, caller byte limits,
plan checks, fingerprint scope and policy-derived diagnostics. It did not rerun
tests or assess broader release security, concurrent file mutation behavior,
existing error terminology or time spent on design. Those are unchanged and are
outside this bounded follow-up. Only this delivery record was added after review.
