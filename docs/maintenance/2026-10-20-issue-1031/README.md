# Ordered candidate replay: Issue #1031

`verify --top` and `verify --sample` previously retained every selected
`MutationCandidate` during spool discovery and in the execution queue. The
implementation now retains selected IDs in core and selected spool positions in
I/O. An idle worker requests its body by ID after complete discovery. There is
one pending candidate read and no additional prefetch.

The comparison base is `3054427a1b1b9a7d2f5cdd1b6ce62053e6c417ac`.
The [implementation plan](../../superpowers/plans/2026-10-20-issue-1031.md) records
the boundary and checks; [reviews](reviews.md) record the design, implementation
and verification passes.

## Behavior and ownership

Core still validates missing IDs at EOF before ordered dispatch, preserves
request order, observes the execution budget at the existing point, and reports
selected candidates beyond `max_mutants` as NotRun. Duplicate discovery uses the
last body. A pending indexed read retains its ID across cancellation, deadline,
disk stop and repeated stop. A failed indexed read cannot supply a report body:
it raises an infrastructure failure, is not retried indefinitely, and allows
remaining known IDs to drain.

`CandidateReadTarget::{Next, Id}` replaces the physical cursor in core
requests/events. `CandidateReplay` owns the cursor and an index containing only
requested IDs. The shell transfers that owner into blocking reads and recovers
it on completion and shutdown. Discarded sequential completions rewind one
position checkpoint, preserving unaccepted candidates for stopped streaming
runs. No candidate body is cached for that recovery. Saved plan/report formats
and CLI arguments are unchanged. There is no legacy API adapter or package
version-floor change.

## Regression evidence

The payload test uses 256 selected candidates, each with 32 KiB in `original`
and 32 KiB in `replacement`. Bodies are allocated inside the measurement window;
state and emitted effects remain alive at the measurement point.

| State | Base retained bytes | Changed retained bytes | Changed peak delta |
| --- | ---: | ---: | ---: |
| Discovery, 2 jobs | 16,938,272 | 29,488 | 95,136 |
| Workers occupied, 2 jobs | 17,002,034 | 288,332 | 357,180 |
| Workers occupied, 8 jobs | 17,399,240 | 1,079,288 | 1,148,136 |

The original body-owning implementation failed all three retention ceilings.
After the change, all three pass. The first meter used process-wide counters;
parallel libtest activity caused a self-check failure unrelated to candidate
ownership. Serial execution confirmed that interference. The committed test
meters synchronous allocations on the current thread, keeps its allocator
self-check, and passes with the default parallel test runner. Its results match
the serial observation.

## Release comparison

On macOS 15.7.7 arm64 with Rust 1.99.0 release builds, all 12 conditions completed
five alternating baseline/changed pairs: 120 executions. Each pair checks every
candidate field, start order, result counts and outcomes. Source, linked library
and binary hashes are recorded in the [baseline build](results/baseline-build.json)
and [changed build](results/changed-build.json) receipts.

The following medians include core/ID/index setup and keep state plus worker
effects alive. N is 512 in every row. P is the size in bytes of **each** original
and replacement field. Retained bytes are measured with all workers occupied;
peak is cumulative through the complete drain. Times include fake worker
completions and result checking, with warm spool page cache.

| P | Selected | Jobs | Retained bytes, base → changed | Peak bytes, base → changed | Complete time ms, base → changed |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 32 | 256 | 4 | 167,048 → 113,882 | 244,674 → 141,018 | 8.531 → 16.293 |
| 4,096 | 256 | 4 | 2,280,328 → 178,906 | 2,341,450 → 195,335 | 10.850 → 15.173 |
| 32,768 | 256 | 4 | 17,189,768 → 637,658 | 17,258,529 → 711,431 | 33.923 → 40.895 |
| 4,096 | 64 | 4 | 601,064 → 101,114 | 612,481 → 117,543 | 8.830 → 9.706 |
| 4,096 | 512 | 4 | 4,519,000 → 281,674 | 4,649,946 → 298,103 | 15.341 → 23.437 |
| 4,096 | 256 | 1 | 2,255,293 → 128,924 | 2,341,450 → 151,417 | 10.963 → 14.952 |
| 4,096 | 256 | 8 | 2,316,780 → 248,482 | 2,344,522 → 264,911 | 11.016 → 14.945 |

With selected count and jobs fixed, changed discovery retains 118,560 bytes at
all three payload sizes; baseline grows from 236,170 to 16,997,002 bytes.
Increasing selected count still grows ID/index metadata, while increasing jobs
admits more bodies. The large-payload stop case retains 452,803 bytes immediately
after cancellation, versus 16,959,961 bytes before the change. Seeded sampling
also preserves the full requested sequence and candidate results.

The memory improvement costs additional I/O and parsing time. The large-payload
complete median increases from 33.923 to 40.895 ms; the smaller-payload changed
times range from 10.694 to 27.850 ms across five runs. These observations do not
establish a speed improvement or an end-to-end CLI slowdown ratio. With N=S at
256/512/1,024, changed actual record visits are 512/1,024/2,048; complete medians
are 8.242/16.272/33.161 ms. Every measured run checks the expected logical JSONL
bytes as well as record visits.

[Raw observations](results/measurements.json), [all medians and ranges](results/summary.json),
[comparison](results/comparison.json), and [run environment](results/run.json)
retain the complete evidence. The reader counter measures records and logical
record bytes, not filesystem syscalls or physical disk bytes.

To reproduce from the changed checkout, use isolated build targets:

```sh
git worktree add --detach /tmp/hoimin-1031-base 3054427a1b1b9a7d2f5cdd1b6ce62053e6c417ac
python3 docs/maintenance/2026-10-20-issue-1031/build_probe.py \
  --revision baseline --worktree /tmp/hoimin-1031-base \
  --target /tmp/hoimin-1031-target-base --output /tmp/hoimin-1031-probes/baseline
python3 docs/maintenance/2026-10-20-issue-1031/build_probe.py \
  --revision changed --worktree . \
  --target /tmp/hoimin-1031-target-new --output /tmp/hoimin-1031-probes/changed
python3 docs/maintenance/2026-10-20-issue-1031/run_probe.py \
  --baseline /tmp/hoimin-1031-probes/baseline \
  --changed /tmp/hoimin-1031-probes/changed \
  --logs /tmp/hoimin-1031-raw --results /tmp/hoimin-1031-results
```

Cargo dependencies must already be cached because the builder uses `--offline`.
It includes production store source with one record-counter hook and the actual
sampling implementation; the core comes from the corresponding release rlib.
Only the protocol adapter differs between revisions. Fixture writing occurs
before the meter starts, and result events are checked and dropped immediately.

## Cost boundaries

Selected IDs and their position index still require O(selected IDs) metadata.
Discovery parses one body at a time; active workers and their effects may own a
constant number of body copies per job. Direct indexed reads add one record read
per selected body after the complete scan. This trades additional seek and parse
work for reduced retained heap, without rescanning a prefix for every ID.

The release probe measures core scheduling and actual spool reads. Worker and
process completions are supplied by the harness. It does not load a full saved
plan, retain report history, copy workspaces, run Python tests or measure child
processes. Plan parsing and `VerifiedPlan` data, report buffering and session
storage therefore remain separate costs; this change establishes no reduction
or bound for those phases. Allocator requested bytes exclude allocator overhead
and are not RSS. `--max-memory` does not become a CLI heap limit.

## Integration checks

Executed locally on macOS on 2026-10-20. Rust checks used the repository's pinned
toolchain and a shared Cargo target directory.

| Check | Result |
| --- | --- |
| `cargo test --locked --workspace` | 3,849 passed, 33 ignored; before the review's sequential-retirement correction |
| `cargo test --locked --workspace --all-features` | 3,851 passed, 33 ignored; final correction included, contracts enabled |
| New real-spool stop-boundary regressions | 3 RED before correction, 3 GREEN after |
| Final store tests / shell replay ownership and drain | 25 / 4 passed |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed |
| Vendored parser Clippy, `--locked --lib --no-deps -- -D warnings` | Passed |
| Workspace and vendored parser `cargo fmt -- --check` | Passed |
| Python Ruff format/check and ty | Passed |
| Frozen-environment Python suite | 1,212 passed, 11 skipped; 4 process-monitor failures under the sandbox |
| `pytest tests/test_lean_resource_guard.py` outside the sandbox | All 5 passed; confirms process-list access caused the 4 failures |
| Release comparison and independent evidence audit | 120 executions / 60 pairs passed |

[Rust result counts](test-counts.json) exclude nested filtered test-process output
to avoid double counting. Local raw command logs remain under
`.superpowers/issue-1031/`; focused shell/store logs are
`/tmp/issue-1031-retired-replay-*.log`.

The [independent review record](reviews.md) distinguishes its initial stop
finding, correction and final evidence audit. This work does not claim a new
Lean proof or native Linux/Windows execution. Existing oracle adapters ran in
the workspace suites.
