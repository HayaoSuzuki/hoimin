# Review boundaries, finalization ownership and candidate retention

Base: `5265aa2cc9f36b6b98ded50c6823039d8999b665` (main, merged #1049).
Scope: [#1050](https://github.com/HayaoSuzuki/hoimin/issues/1050),
[#1051](https://github.com/HayaoSuzuki/hoimin/issues/1051),
[#1052](https://github.com/HayaoSuzuki/hoimin/issues/1052),
[#1053](https://github.com/HayaoSuzuki/hoimin/issues/1053).

## Design and preserved contracts

- Review source observations use one capability-relative reader per command and
  bounded no-follow reads per distinct source. A final-entry or parent-directory
  swap cannot redirect reads outside the pinned project root. Internal/external
  symlinks and nonregular/missing/unreadable/oversized sources are Unavailable;
  invalid lexical paths remain errors. The outside-symlink diagnostic therefore
  changes from a special Invalid error to Unavailable. Artifact symlink handling
  is independent and unchanged. Encoding/span and candidate-order precedence,
  per-file grouping and independent record freshness observations remain.
- The shared bounded reader inspects opened-handle size before allocation and
  retains the limit+1 growth guard. It does not replace the descriptor boundary
  with a path metadata check or promise a deadline for every filesystem call.
- `ExecutionFinalization` owns lifecycle admission, measurement quiescence,
  cleanup attempts/results and end-space observations. It derives safety from
  the existing canonical lifecycle predicate and scan state, without a second
  cached safety bool or allocating snapshots on scheduler checks. An unjoined
  scan remains unsafe after later successful observations. A joined measurement
  error does not pretend to supply a byte count.
- Actual-removal requests require all four quiescence conditions. Deferred
  cleanup has a separate API that records evidence without authorizing removal.
  Attempts become terminal before admission, even if refused. Normal/outer paths
  share completion handling and summary projection. Shutdown grace, primary and
  secondary errors, execution/delivery distinction, retained roots and existing
  Lean production-adapter connections remain. Proof/model sources are unchanged.
- Worker transitions move owned enum payloads and borrow identity fields. The
  separate candidate/result copies needed by apply, output or persistence until
  ACK remain. No blanket Arc conversion or result/status/schema change is used.
- Review selected append/show run one globally ordered joined scan plus an event
  count. Selection controls retention, not validation: unrelated malformed
  candidates/drafts, empty/noncontiguous histories and orphan events still reject
  operations. An owned append avoids cloning the target history. Full history,
  revisions, transaction boundaries, conflict/rollback and DB/event limits remain.
  Global validation and decoding are still linear in the stored history.

No new configuration, format version, migration or compatibility fallback is
introduced. Package floor stays 0.14.0. The live usage guide's stale schema-8
session paragraph is aligned with the already-current report schema 10.

## Regression and measurement evidence

The tests were written against the old implementation before the optimizations.
Source swap and internal-link regressions failed at runtime. Worker allocation
checks failed because identity-only transitions copied source bodies. Review
query/heap/history-clone checks failed. A corruption fixture initially failed
while injecting an orphan because SQLite foreign keys were enabled; foreign keys
are disabled only on that test's tampering connection, never in production.
The old outside-symlink test now checks the explicitly selected Unavailable policy.

Before the finalization refactor, 152 shell tests passed with two ignored. The
new owner tests compare normal/outer observations across every drain combination,
sticky failures/unjoined scans, failed measurement evidence, refused attempts,
summary projection and single-shot failed free-space observations. Existing
public failure-injection tests cover reap/output/monitor/measurement failures,
recordless/deferred cleanup, duplicate requests, report errors and late completion.

The bounded debug-build measurements below use identical fixtures before/after;
they are allocation/work regressions, not release throughput benchmarks.

| Operation/workload | Before | After |
| --- | ---: | ---: |
| Core transitions, two candidates, 256 KiB per text field, no session: allocated bytes | 6,298,100 | 1,053,946 |
| Same workload: peak additional live Rust bytes | about 1,048,900 | 525,248 |
| Core transitions, 64 small candidates, no session: allocated bytes | 224,896 | 173,888 |
| Review append, 193 candidates/195 events: data SELECTs | 195 | 2 |
| Same append: peak additional live Rust bytes | 1,070,912 | 23,053 |
| Review append, one candidate/128 events: prior events cloned | 128 | 0 |
| Same append: peak additional live Rust bytes | 1,142,595 | 584,292 |
| Review show, 193 candidates/195 events: peak additional live Rust bytes | 1,115,257 | 54,510 |

The worker test measures public transition windows, with bodies allocated before
measurement; fixture setup is included in printed elapsed time. Session-enabled
cases exercise reuse, not sustained persistent execution. Each candidate gets a
fresh machine fixture. Large remaining allocations include the independent output
copy that must survive ACK; they are not evidence of a leak. The workload is not
a scheduler/OS/whole-run RSS benchmark.

Review counters count data queries, not unchanged schema/PRAGMA checks. All
193 candidate keys and 195 event drafts are still decoded and validated once.
The thread-local allocator measures Rust allocations, not SQLite C heap or
process RSS; inputs freed from before a measurement could distort net-delta
interpretation, so this fixture retains its prepared input state. Elapsed values
are observations only and have no pass/fail threshold or universal speed claim.

## Verification record

- `cargo test --locked --offline --workspace --all-features`: 3,907 passed,
  0 failed, 33 ignored across 187 suite/doc-test results.
- `cargo test --locked --offline -p hoimin-core`: 875 passed,
  0 failed, 2 ignored with contracts disabled.
- Workspace fmt and all-target/all-feature Clippy with `-D warnings`: passed.
- Independent `fuzz/` workspace all-bin check and Clippy: passed. This is build/lint
  verification, not a local full fuzz campaign.
- `.venv/bin/pytest -q`: 1,223 passed, 14 skipped.
- Strict docs build: passed. OKF YAML/reserved-file parsing and entry-point
  reachability: 50 pages; changed-concept link targets/new source hashes/footnotes:
  three concepts. Historical source hashes were not refreshed.
- Independent static reviews of both domains: no Critical/Important findings.
  The cleanup admission API concern was refined into separate actual-removal and
  deferred-evidence APIs, then independently reviewed again without new findings.
- Existing public source-symlink test initially expected the previous Current
  behavior. It now checks Unavailable, retained history, and rejected append
  without DB modification. The final integrated run passes this policy.

Completed details are also in [validation.json](validation.json).
Local execution is macOS arm64; native Linux/Windows checks remain with CI.

## Resource and review policy

One numbered worktree/branch uses the existing shared Cargo target and a symlink
to the existing Python environment. Cargo runs only in the coordinating process,
with jobs=2; agents edit disjoint source files and do not build. Synthetic inputs
are bounded, source-limit files are sparse, and no large benchmark or copied
build tree is required. Disk free space is sampled between phases.

Focused self-review covered source-path authority, cleanup admission versus
attempts, worker ACK ownership, review corruption/rollback, and measurement
claims. Independent read-only reviewers compared both changed domains against
base; final findings and any refinements are recorded with the completed checks.
