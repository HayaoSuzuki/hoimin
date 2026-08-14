# Bounded process-output retention Lean audit

## Verdict

Issue #311 found no same-premise mismatch in Rust's bounded process-output
collector. Empty, fitting, exactly-full, truncated, tiny, zero-capacity,
oversized-chunk, multi-wrap, alternate-partition, and recorded-order cases all
match the Lean oracle. Observed-byte saturation and error/drain behavior also
match. No production policy correction was required.

The Rust refactor introduces a private output-sink seam without changing the
public API or successful file implementation. It permits a deterministic first
write failure so the existing drain loop and first-error behavior can be tested.

## Audited contract

The premise is the ordered sequence of byte chunks received by the collector.
For that sequence and capacity `k`:

- observed bytes are counted with `u64` saturation;
- retained count is `min(observed, k)`;
- the collector drains through EOF after spool creation or write failure;
- fitting output is exact;
- truncated output is the 35-byte marker plus the newest bytes that fill the
  remaining space when `k > 35`;
- when `k ≤ 35`, truncated output contains only the newest `k` bytes;
- the next ring position advances by every observed byte, including skipped
  bytes in a chunk larger than the ring;
- the first I/O error remains the result.

This contract does not assign a platform-independent order to independently
scheduled stdout and stderr producers. A recorded mixed-pipe row is replayed
only after receive order is fixed.

## Kernel-checked claims

The model represents received and written bytes separately, along with ring
position, saturated observation, first error, and drained chunk count. Lean
checks these universal claims:

- `successful_run_refines_reference`;
- `chunk_partition_invariant`;
- `successful_ring_refines_newest`;
- `successful_position_tracks_all_bytes`;
- `successful_observed_saturates`;
- `saturated_observed_matches_stream_length` under the explicit premise that
  the received stream length is representable by the modeled counter;
- `successful_retained_eq_min`;
- `successful_final_bytes_bounded` and `finalBytes_length_le`;
- `marker_requires_strict_spare_capacity`;
- `receiveFailure_records_first_and_does_not_write`;
- `receive_after_error_drains_without_writing`;
- `run_written_after_error`;
- `recordError_preserves_first`;
- `error_run_remains_error_and_drains`.

The refinement and partition theorems state the representable-length premise
explicitly. This is the production premise for an allocated Rust byte stream;
the near-`u64::MAX` arithmetic row is not promoted to a byte-stream claim.

The external consumer imports only `OutputRetentionModel` and
`OutputRetentionProofs`. Fixed cases, enumeration, JSON generation, and broken
variants remain in the executable path.

## Corpus and correspondence

The closed JSONL corpus contains 16 rows:

| Mode | Rows | Rust observation |
| --- | ---: | --- |
| `strict` success | 7 | public `ProcessHandler`, `OutputSpoolRef`, exact spool bytes |
| `internal-fixture` success | 5 | `collect_output`, ring position helper, exact bytes and counts |
| `internal-fixture` error | 2 | deterministic create/write failure, EOF completion, first error |
| `model-only` arithmetic | 1 | `u64::MAX - 2 + 8` through owned saturation helper |
| `infrastructure-error` | 1 | harness classification only |

The strict rows cover zero and nonzero empty output, fitting partitioned output,
exact capacity, one byte over a tiny capacity, capacity equal to marker length,
and one byte more than marker length. Internal rows cover a chunk larger than
capacity, multiple wraps, equivalent partitions, and a fixed mixed-pipe receive
order.

The Rust adapter requires the exact ID/mode/scenario mapping, unique IDs,
schema 1, scenario-specific inputs, consistent retained counts, bounded
positions, and exact drained counts. It rejects unknown fields, duplicate IDs,
crossed modes, and unrelated error premises. Every byte is deserialized as
`u8`.

## Refutation sensitivity

Each required broken family has a separate witness:

| Broken family | Witness |
| --- | --- |
| retain oldest bytes | four bytes, capacity three |
| advance by retained bytes | five-byte chunk, capacity four |
| keep wrong side of oversized chunk | four-byte chunk, capacity three |
| skip at wrap | six bytes, capacity four |
| duplicate at wrap | six bytes, capacity four |
| use non-strict marker threshold | 36 bytes, capacity 35 |
| marker without tail reduction | 40 bytes, capacity 36 |
| retained count from successful writes | first three-byte write fails |
| wrap observed count | maximum 10, seed 9, increment 3 |
| stop draining after error | three later one-byte chunks |
| overwrite first error | error 7 followed by error 11 |
| depend on partitioning | one chunk versus three chunks |
| claim fixed cross-pipe order | two distinct two-byte pipe fragments |

All 13 variants are detected. Fixed exploration reaches capacity 36 and stream
length 40, around zero, the marker boundary, oversized chunks, and wraps. These
checks provide refutation evidence; the durable claims come from the universal
proofs.

## Fault seam and production impact

`collect_output` still opens the same Tokio file with the same options. The
private generic collector loop now receives a sink implementing two operations:
ring write and finalization. The production sink delegates to the pre-existing
functions. On a write error the sink is dropped, all remaining chunks are
received, and the saved error is returned after EOF. A bounded-channel test
sends 32 chunks after a first-write failure, which would block if the collector
stopped draining.

## Overlap and exclusions

Candidate JSONL spool framing, record counts, cursor replay, and candidate
retention remain owned by issues #127 and #166. This audit does not prove Tokio,
filesystem syscalls, pipe scheduling, or spool durability below returned I/O
results. It does not change marker bytes or the public report schema. The
near-`u64::MAX` row is arithmetic-only because such an allocated byte stream is
not a realizable Rust fixture.

## Resource measurements

Each command ran alone under a 20,000 ms deadline, a 786,432 KiB
root-plus-descendant RSS ceiling, and 250 ms sampling.

| Command | Elapsed ms | Peak RSS KiB | Exit / reason |
| --- | ---: | ---: | --- |
| proof module build | 296 | 2,688 | 0 / `child_exit` |
| external proof consumer | 559 | 2,176 | 0 / `child_exit` |
| sensitivity, including executable rebuild | 2,758 | 743,568 | 0 / `child_exit` |
| fixed cases | 291 | 2,240 | 0 / `child_exit` |
| corpus freshness | 281 | 2,736 | 0 / `child_exit` |

No retained command reached either limit. The largest sample remains 42,864
KiB below the RSS ceiling.

## Verification commands

```text
lake build
lake env lean /tmp/hoimin-output-retention-proof-consumer.lean
lake exe generate_output_retention -- --cases
lake exe generate_output_retention -- --sensitivity
lake exe generate_output_retention -- --check corpus/output-retention.jsonl
cargo test -p hoimin-cli --lib process::output::tests --all-features
cargo test -p hoimin-cli --test process_handler --all-features
cargo test -p hoimin-cli --all-features
cargo test --workspace --all-features
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```
