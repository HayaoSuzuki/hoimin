# Lean audit design: bounded process-output retention

## Decision

Audit the collector as an ordered-byte-stream machine. Lean owns chunk folding,
the newest-byte ring semantics, observed-count saturation, marker insertion,
and first-error absorption. Rust owns filesystem and Tokio behavior. The
correspondence boundary begins after stdout/stderr chunks have acquired a
receive order; no cross-pipe scheduling order is claimed.

No production change is planned unless a generated same-premise case fails.
If it does, the smallest failing trace becomes a Rust regression before the
collector is changed.

## Pure model

Use bytes represented by bounded natural numbers and a collector state with:

- capacity;
- logical ring contents in oldest-to-newest order;
- next-write position modulo capacity;
- a saturating observed count;
- an optional first I/O error;
- a drained chunk count.

A successful transition keeps only the newest `capacity` bytes. The write
position advances by the complete observed chunk length, including bytes from
a chunk larger than capacity. An errored state continues to count drained
chunks and observed bytes but cannot regain a successful result or replace its
first error.

Finalization compares the ring with a direct reference over the concatenated
ordered stream. If the stream fits, output is complete. If it is truncated and
capacity is strictly greater than the marker length, output is the marker plus
the newest bytes that fill the remaining capacity. At smaller capacities only
the newest bytes are retained. Capacity zero produces no bytes.

## Claims

Kernel-checked theorems will establish:

- a successful ring fold finalizes to the direct reference;
- repartitioning one ordered stream into chunks does not change counts or
  finalized bytes;
- stored and finalized lengths never exceed capacity;
- retained count is `min(observed, capacity)`;
- write position advances by all observed bytes modulo nonzero capacity;
- observed accounting saturates at the modeled `u64` maximum;
- once the first error is present, later chunks are drained and cannot produce
  success or replace that error.

The external proof consumer imports only model and proof modules. Fixed cases,
bounded enumeration, JSON generation, and mutation sensitivity remain outside
that import path.

## Correspondence worksheet

| Premise | Mode | Rust seam | Compared observation |
| --- | --- | --- | --- |
| Empty, fitting, exact, one-over, marker boundary, tiny, zero | `strict` | owned public process fixture where capacity is configurable | `OutputSpoolRef` and exact spool bytes |
| Large chunk, wraparound, multiple wraps, alternate partitions | `internal-fixture` | crate-private `collect_output` adapter | retained/observed counts and exact file bytes |
| Recorded stdout/stderr receive order | `strict` | public process fixture | result for that recorded order only |
| Creation or write failure followed by later chunks | `internal-fixture` | collector fault seam | first error and EOF/drained evidence |
| Count near `u64::MAX` | `model-only`; arithmetic seam is strict | pure Lean and owned Rust helper | saturated observed count |
| Timeout, unreadable artifact, malformed corpus | `infrastructure-error` | harness | diagnostic only |

The generated corpus is closed: exact IDs, modes, scenario-specific premises,
and expected byte/count fields are validated. Model-only rows are never used
as evidence about allocating impossible Rust byte streams.

## Sensitivity

Retain independent broken functions and minimized witnesses for oldest-byte
retention, retained-length position advancement, oversized chunks, wrap skips,
wrap duplication, non-strict marker threshold, marker without tail reduction,
retained-from-writes accounting, wrapping observed count, stopping drain after
error, replacing the first error, partition dependence, and an invalid
cross-pipe ordering claim.

Each sensitivity result is reported separately. Bounded exploration covers
capacities around zero and marker length, chunks around capacity and wrap
boundaries, and several partitions. It is refutation evidence, not a substitute
for universal proofs.

## Fault seam

Prefer a small collector-local abstraction that scripts create, write, and
finalize outcomes while preserving the production file implementation. The
seam must expose only observations needed by #311: whether every queued chunk
was consumed and which error survived. It must not change public APIs or the
output schema.

## Exclusions

Candidate JSONL framing, candidate cursor replay, Tokio correctness, filesystem
correctness below returned I/O results, and platform-independent stdout/stderr
ordering remain out of scope. The marker bytes and public schema are unchanged.

## Delivery

Ship model, proofs, executable cases, a generated JSONL corpus, strict Rust
parsing, collector and public-process correspondence tests, fault-drain tests,
resource measurements, and an audit report in this worktree. Run independent
review and all required CI before squash merge.
