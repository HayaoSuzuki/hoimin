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
