# Issue #1092: fixed-capacity JSON mutant record serialization

Base: main `21c5659d4a6a37633bd1a29f691debd0e84f4280` after PR #1090.
Implementation continues on `audit/issues-1092`, in `.worktrees/audit-1092`,
where [the audit](../../knowledge/audits/design-2026-10-23-post1090.md) was saved.
The PR includes that audit and its navigation; historical hashes remain pinned
to the audited base. The unresolved session companion policy is outside this issue.

## Delivery and memory contract

JsonReport::write_mutant serializes its optional separator and event through a
record-local 8 KiB BufWriter. Explicit flush succeeds before has_mutants is
updated and before the existing lifecycle returns from Poisoned to Open. On any
ordinary write/serialization/flush error, into_parts discards unsent bytes without
Drop retry. I/O errors retain the `write mutant record` operation context;
non-I/O serialization errors retain their typed category. The existing poison
state rejects both subsequent mutant records and finalization.

This preserves the batching/acknowledgement purpose of
[the #463 design](../../superpowers/specs/2026-09-14-issue-463-json-record-write-design.md)
while replacing its full encoded record Vec. Small records still produce one
spool write. Large records can use multiple grouped writes, including direct
writes of large string slices. No persistent buffer, new lifecycle, public API,
CLI option, schema, dependency or compatibility adapter is added. Run/baseline
buffers and final spool flush/seek/copy remain separate. Flush follows the Rust
Write contract and does not promise disk durability.

## RED/GREEN allocation and write measurements

[report_heap.rs](../../../crates/hoimin-cli/tests/report_heap.rs) calls the actual
ReportHandler::handle with 16 and 64 events, and original/replacement strings of
1 byte or 4/16/64 KiB each. Event construction, expected serialization and handler
setup occur before measuring. A noncollecting Read+Write+Seek spool counts writes,
bytes and flushes without retaining the document; stdout/stderr are sinks. Read
and seek panic because these windows must not finalize discarded data. Each event
gets a fresh allocation window so consuming a fixture cannot hide later peaks.
The existing count-independence test shares the same measurement lock.

The new bounded-allocation regression failed before the production edit and
passes afterward. The table uses the focused test alone; its after run sets
`--test-threads=1`. Both counts produce the same peak and writes per event:

| Bytes per original/replacement | Before peak bytes | After peak bytes | Before writes/event | After writes/event |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 512 | 8,192 | 1 | 1 |
| 4,096 | 17,112 | 8,192 | 1 | 2 |
| 16,384 | 66,264 | 8,192 | 1 | 5 |
| 65,536 | 262,872 | 8,192 | 1 | 5 |

[Before/after observations](allocations.json) retain counts and total bytes, which are
identical before/after for each shape. Afterward there is one inner flush/event;
previously there was no inner flush during record delivery. The 100-small-event
regression still verifies 100 writes. These are requested Rust heap allocations
in the record-delivery window, not allocator overhead, fixture-owned data, RSS,
filesystem syscalls, command-wide peak or timings. Small records incur a larger
bounded temporary allocation; no speed or whole-command memory percentage is
claimed.

The allocator tracks the entire test process. The measurement lock serializes
both heap tests, but harness activity can still add noise. In the earlier combined
run, the 16-small-event case observed 8,800 bytes rather than 8,192. The JSON also
retains that observation; the focused run reconciles the table with its source
measurements. The regression allows 16 KiB to tolerate small incidental allocations
while rejecting the old payload-proportional buffer.

## Failure and output regressions

The new failure test also failed on the old code: a flush fault was acknowledged
because record delivery did not flush. After the change, small and large payloads
cover partial I/O errors, zero progress and flush failure before acknowledgement.
Write counters check no Drop retry, and no later writes on either a mutant retry
or finalization. Interrupted writes and short writes are retried to completion;
a separate regression compares final JSON bytes for three differently sized
records, including quotes, backslashes, newlines and Unicode across buffer
boundaries. Existing poison, zero-progress, golden, schema and count-independent
heap regressions remain. Serialization uses the same typed OutputEvent; it has
no injectable custom failing serializer, so no synthetic serialization-failure
public API is added solely for testing.

The first combined report test invocation had five fixture setup failures because
the worktree lacked its Python environment link. A link to the existing root
`.venv` fixed setup without installing or syncing dependencies. Subsequent test
commands isolate TMPDIR/TMP/TEMP. The first workspace run used a path inside this
worktree and hit the Unix socket SUN_LEN path limit in one pre-existing test;
the rerun uses a short, private `/private/tmp/h1092.*` directory instead. The new regressions call
ReportHandler directly and do not launch the CLI or startup janitor.

## Verification

- `cargo test --offline --locked --workspace`: 4,017 passed, 34 ignored, no failures.
- CLI library/report tests with `contracts`: 1,222 passed, 25 ignored, no failures.
- Workspace Clippy with all targets and all features: passed with warnings denied.
- `report_handler` and `report_heap`: 153 and 2 passed, including new regressions.
- Focused allocation test alone with `--test-threads=1`: passed; all eight rows above.
- Documentation tests: 21 passed; site generation succeeded.
- Knowledge inventory: 59 pages, 56 YAML headers (55 concepts and the root index),
  3 plain indexes; no missing local references or unreachable pages. Historical
  audit hashes: 34 checked against the base revision. Current modified source
  hashes: 3 checked against working files.

A separate read-only reviewer found no production or failure-path issues and
identified the measurement-table discrepancy described above; the focused rerun
and retained noisy observation resolve it. No external review or end-to-end
benchmark is implied. Cargo uses two build jobs, no incremental compilation and
no development debug info; test commands reuse cached dependencies offline and
locked. Owned target/site outputs and the short test temporary directory are
removed after checks; audit/worktree and local evidence remain.

## Follow-up: dependency audit on PR #1093

The [initial CI dependency audit](https://github.com/HayaoSuzuki/hoimin/actions/runs/39882316045/job/120182312386)
failed on the inherited Tokio 1.54.0 lock entry. RustSec RUSTSEC-2026-0384
identifies an Interval/time-driver race and marks >=1.54.1 as patched; 1.54.0 was
also yanked. The official 1.54.1 changelog records the timer-change revert in
[tokio-rs/tokio#8692](https://github.com/tokio-rs/tokio/pull/8692).

At the user's request, root Cargo.lock now selects 1.54.1 with the registry
checksum. Cargo's incidental Windows dependency-edge reselection was excluded;
locked all-feature metadata validates the minimal result. Parsed lockfile
comparison confirms Tokio is the only changed package; dependency requirements,
fuzz/Cargo.lock, CLI/schema, package floors and runtime code are unchanged.
The custom-build inventory was rechecked: 223 packages, the same 53 custom-build
targets, and no Tokio build script.

The official cargo-audit 0.22.2 aarch64 macOS release binary reproduces the old
lockfile failure (one vulnerability and the yanked warning). Both updated root
and unchanged fuzz lockfiles then pass against the same RustSec snapshot,
`640bf5a9f06e66d8721e8772766531fe77c8630d`. The tool/DB were kept in the ignored local evidence directory during verification;
no audit ignore or policy exception was added. This is a fresh dependency audit,
separate from the pre-update #1092 test results above. Follow-up verification completed:

- `cargo test --offline --locked --workspace --all-features`: 4,012 passed,
  34 ignored, no failures, using two build jobs and a short private TMPDIR.
- Documentation tests: 21 passed; site generation and knowledge inventory passed.
- Read-only follow-up review verified the minimal lock change, crate checksum,
  advisory/changelog correspondence and unchanged custom-build inventory.

The downloaded audit tool and DB are removed after verification; logs and a copy
of the advisory are retained locally. Historical #1092 allocation/source hashes
above are unchanged by this dependency patch.
