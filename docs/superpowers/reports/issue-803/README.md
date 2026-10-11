# Bounded corpus distillation experiment

Follow-up to the [fuzzing adoption assessment](../issue-805/README.md), issue #803.
Baseline main: `fe5a37f`; experiment source: `2764007` (full SHA in evidence/source-sha.txt).
The [plan](../../plans/2026-10-12-corpus-distillation.md) records the budget and
design/plan self-reviews. No production input surface or corpus replacement
mechanism is introduced.

## Experiment conditions

One manual GitHub Actions execution, one worker, whole job <=20 minutes including
setup/build. Fixed cache `fuzz-corpus-v1-Linux-38172184143-1` was selected from
main after its 3,489,846-byte compressed size was checked through the cache API;
the acquisition limit was 32 MiB. Restore has no fallback and no save step.

The target is only `python_analyzer`, nightly-2026-10-08, cargo-fuzz 0.13.2,
Python 3.14. The experiment hashes the original snapshot, managed seeds and
instrumented binary. Restored input plus seeds are limited to 50,000 files,
64 MiB total, and 4,096 bytes/input; at least 8 GiB free disk is required before
build. Fuzzer RSS is limited to 1,024 MiB, each input to 5 seconds, each child
output file to 64 MiB. Existing Campaign process-group termination and monotonic
deadlines enforce the remaining job budget.

Three `-runs=0` replays precede a fresh `-merge=1` into a separate compact
directory. Three compact replays precede a second fresh merge control file.
Both sides include the same managed seeds. Replay commands have a 120-second
stage ceiling and merges a 180-second ceiling, all clipped by the common deadline.
No new fuzz exploration is mixed into the comparison.

The parser requires a complete sequential STARTED/FT/COV triple for every input,
including the final newline. It unions integer feature IDs and PC-table indices
from each control and compares the actual sets, not just their sizes. These are
instrumentation-specific identities, not proof that every semantic regression
input can be discarded. Managed seeds are independently hashed and retained.
The format was checked against
[LLVM's merge implementation](https://github.com/llvm/llvm-project/blob/main/compiler-rt/lib/fuzzer/FuzzerMerge.cpp);
the experiment's actual vendored format must also pass the strict parser.

## Results

[Run 38177234047](https://github.com/HayaoSuzuki/hoimin/actions/runs/38177234047)
completed successfully in 5 minutes 27 seconds. Corpus restore took approximately
2 seconds at Actions step timestamp resolution. Build took 164.680 seconds;
initial merge 36.810 seconds; verification merge 13.497 seconds. Python was
3.14.8; Cargo reported 1.101.0-nightly (f3865b2a4 2026-09-29), installed under
nightly-2026-10-08. The fuzz lock pins libfuzzer-sys 0.4.13.

| Measurement | Original | Compact |
| --- | ---: | ---: |
| Input directory files (managed seed directory also replayed) | 13,314 | 4,687 |
| Input directory bytes | 1,686,023 | 553,829 |
| Replay 1 wall seconds | 18.862 | 9.488 |
| Replay 2 wall seconds | 19.113 | 8.986 |
| Replay 3 wall seconds | 19.564 | 9.087 |
| Median replay wall seconds | 19.113 | 9.087 |
| Merge feature count | 78,929 | 78,892 |
| Merge PC count | 24,479 | 24,479 |

Replay includes process launch and managed seeds, with no compilation or new
mutation exploration. The observed median reduction was 10.026 seconds (about
52%). Initial merge alone would amortize after about four such replays **if**
feature preservation held, ignoring additional verification/setup. Including the
verification merge needs roughly six; charging this experiment's cold build too
needs about 22. These arithmetic estimates are not steady-state CI savings or
performance guarantees. This is one binary/snapshot and three sequential replays,
not a randomized or independent multi-machine benchmark.

The exact set comparison failed: **47 original feature IDs were absent**, 10
other IDs appeared, and **3 original PC indices were absent with 3 others added**.
Thus equal PC counts did not mean equal PC sets. The original corpus and its 10
managed seeds (648 bytes) remained unchanged, checked by before/after hashes.
No crash, OOM or timeout was reported; the recorded merge/replay RSS stayed below
600 MiB. Hosted cleanup removed 1,035 build files, 579.0 MiB.

There is already run-to-run variation without compaction: the three original
replays reported cov=24,446/24,442/24,444 and ft=78,886/78,895/78,888. Those engine
counts are not directly interchangeable with merge's feature measurements, but
show that fixed seed and an unchanged corpus did not make these executions'
observations stable. This experiment cannot attribute the merge set differences
solely to removing inputs or identify the source of the variation. It has one
control file per side; no repeated identity-level baseline study was performed.

## Adoption decision

**Do not adopt automatic or scheduled corpus replacement.** Compaction shows a
real replay-cost opportunity, but the stipulated feature-preservation gate failed.
Retain the existing discovered corpus and all managed regressions. No cache was
saved or deleted, and no compressed replacement was published.

Close this bounded investigation with this result rather than extend it into a
new fuzzer scheduler or tracing/determinism project. Reconsider maintenance only
if a future concrete need justifies a stable feature-verification method and
its total cost. This is not a claim that libFuzzer merge is generally unsafe,
that missing instrumentation IDs represent lost semantic tests, or that the
original fuzz target has a correctness defect.

## Retained evidence and reproduction limits

The [evidence directory](evidence/) includes the exact measurements, stage argv/
limits/status, small logs and environment files. The two sorted full feature/PC
sets are retained as deterministic gzip JSON (about 0.5 MiB total evidence).
Their uncompressed hashes match evidence.json and permit independent set
comparison after Actions artifacts expire. Raw control files and the compact
corpus are in the 4,102,770-byte Actions artifact (ID 11716655657) for its retention
period; the original corpus was restored from the fixed cache. The full input
corpus and binary are not committed, so long-term replay depends on those
artifacts/cache remaining available or recreating a new separately identified
snapshot. Hashes alone cannot reconstruct them.

The temporary dispatch extension was removed from `.github/workflows/fuzz.yml`;
its executed version is available at experiment commit 2764007. The final script
adds explicit type annotations, a combined-byte guard and positive parser tests;
these do not change the recorded ordinary-input run. It is a research script,
not a supported maintenance command or automatically executed CI test.

Local verification of the retained analysis:

```powershell
python -m pytest docs/superpowers/reports/issue-803/test_experiment.py -q
python -m ruff check --no-force-exclude docs/superpowers/reports/issue-803
python -m ruff format --check --no-force-exclude docs/superpowers/reports/issue-803
python -m ty check --config 'src.include=["docs/superpowers/reports/issue-803"]' docs/superpowers/reports/issue-803
```

Workflow structure/budget/restore-only checks passed. The local workflow-lint
command could not run because actionlint was absent; the temporary workflow was
accepted/executed by GitHub, and there is no workflow diff in the final result.
No local Rust build was performed and no shared local target was cleaned.

## Implementation self-review: five passes

1. Checked fixed target/cache, restore-only workflow and separate merge/replay
   directories; no existing corpus/cache can be replaced by partial output.
2. Checked file/byte/count limits, minimum free disk, child file size, RSS and
   input timeouts; all subprocess stages reuse existing deadline/group handling.
3. Checked incremental FT/COV records are unioned across every completed input,
   with missing/truncated records rejected before feature-preservation claims.
4. Checked baseline and compact replays use the same binary, seed, flags and
   managed seeds, with setup/build/merge recorded separately from replay time.
5. Checked final source annotations/combined input limit do not alter the measured
   ordinary input case; final research material will not retain workflow dispatch
   changes or create a permanent compression job.

## Test self-review: five passes

1. First failure checks failed with the missing experiment module; implementation
   was then added. This initial collection failure was not counted as fault sensitivity.
2. A property checking truncated prefixes exposed acceptance before the final
   newline; requiring complete output fixed the actual parser defect in the probe.
3. Positive property cases verify complete records retain their real IDs, avoiding
   a reject-everything parser that could pass only negative fixtures.
4. Equal-sized differing feature sets correctly report the missing/added IDs;
   no feature-count equality shortcut establishes preservation.
5. Three research checks passed, and explicit-file Ruff/format/type checks were
   used because ordinary repository configuration excludes research documents.
   No unrelated heavyweight CLI tests were added or run.

## Independent final review

A separate reviewer found no material issues and marked the research ready for
PR. They independently checked the retained datasets and hashes, exact set
differences, median/amortization arithmetic, source registrations and absence
of product/workflow changes. Their Ruff, formatting and type checks passed.
Their pytest run was blocked by temporary-directory filesystem permissions;
the implementer's successful run executed all three checks (3 passed).
