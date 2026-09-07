# Issue #353: preflight source-read optimization

## Result

Workspace preflight now reads and hashes ordinary source files twice instead of
three times. The first content pass builds the manifest and writes the shared
snapshot from the bytes it just hashed. The existing full post-copy manifest
pass still checks the source path set, sizes, and hashes before preflight
returns.

A 1 MiB fixture records these totals:

| Measurement | Before | After |
| --- | ---: | ---: |
| Source content reads | 3 | 2 |
| Source bytes read | 3 MiB | 2 MiB |
| Source hashes | 3 | 2 |
| Source bytes hashed | 3 MiB | 2 MiB |

The metadata capacity inventory reads and hashes zero content bytes.

## Preflight sequence

When an owned-workspace limit is configured, preflight first walks the selected
files and totals their metadata sizes. This check preserves the existing
`planned >= limit` rejection before snapshot allocation. Preflight skips this
extra metadata walk when no owned-workspace limit is configured.

The content-bearing sequence is:

1. Walk each selected file, read and hash it, build the initial manifest, and
   write those exact bytes to a pending snapshot.
2. Before each first write for a logical path, recompute the cumulative owned
   size with checked addition and multiplication. This catches files that grew
   or appeared after the metadata inventory.
3. Run the validation callback against the initial manifest.
4. Build a fresh full manifest from the source and compare every path, size,
   and hash with the initial manifest.
5. Publish the snapshot only after validation and the final comparison succeed.

The initial content walk supplies the plan diagnostics. It uses the existing
normal and explicit-include walks, default exclusions, symlink handling, and
portable-path checks.

## Snapshot and failure guarantees

The snapshot writer consumes the same byte slice that the manifest builder
hashed. It does not reopen or rehash the file. A normal walk and an explicit
include can visit the same logical path more than once. The writer compares
each repeat with its first size and hash, writes and charges the path once, and
returns `OriginalChanged` if the repeated observation differs. Distinct logical
paths that alias on the destination filesystem still return
`SnapshotPathCollision`.

The pending snapshot owner remains live through validation and the final full
manifest comparison. Its drop path removes temporary or managed partial
snapshots after read, capacity, write, validation, or final-comparison errors.
Successful preflight keeps the initial manifest's actual unique byte total for
worker allowances, so a source shrink after metadata inventory reduces the
grant.

The snapshot destination must resolve outside the source workspace. Preflight
canonicalizes both paths and applies the existing platform-aware containment
check before the first content read or snapshot write. It rejects a nested
temporary or managed destination and drops the empty pending snapshot, which
prevents the source walker from descending into its own output. This rejection
uses the existing `workspace.io` error code.

Validation now runs after temporary snapshot creation. A validation failure
therefore performs snapshot I/O before cleanup. The owned-workspace capacity
check still runs before allocation, and the cumulative actual-size guard runs
before each snapshot write.

## Source mutation boundary

The final full content scan remains separate from snapshot creation. It catches
same-size source changes made by validation as well as additions and removals.
As before, a non-atomic filesystem can change a file after that file's final
observation. The implementation guarantees two complete content observations,
an exact-byte snapshot from the first observation, and a completed manifest
comparison before return; it does not provide an atomic filesystem snapshot.

## Verification

The commands below ran on macOS arm64. The Windows-specific destination-alias
regression was updated but was not executed on this host.

- Focused copy tests: 22 passed, 1 ignored.
- The read/hash regression records two reads and two hashes for a 1 MiB file.
- Capacity tests cover equality, checked overflow, growth before the first
  write, an added file after a partial write, and shrinkage.
- Snapshot tests cover duplicate include visits, repeat drift, destination alias
  collision on Windows, validation cleanup, and final-verification cleanup.
- `cargo test --workspace --all-features -- --test-threads=1`: exit 0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit
  0.
- `cargo fmt --all -- --check`: exit 0.
- `git diff --check`: exit 0.

## CI follow-up: directory traversal order

The Linux randomized-order job in run `34099500933` failed because
`added_file_crossing_the_limit_cleans_a_partial_snapshot` expected the two-byte
file to precede the four-byte file. The source walker does not sort directory
entries; either file can precede the other. CI recorded one four-byte write
before the expected capacity rejection.

Swapping the fixture sizes reproduced the same assertion failure on macOS.
The test now covers both size assignments and accepts either valid first-file
size. It still requires exactly one snapshot write, rejection at 12 planned
bytes against a 10-byte limit, and removal of the partial snapshot. This change
does not modify production code or impose sorting on the source walker.

Follow-up verification on macOS arm64:

- The expanded fixture failed with the old assertion: `(1, 4)` versus `(1, 2)`.
- Copy tests passed: 22 passed, 1 ignored, with default parallel execution.
- The same 22 tests passed with `nightly-2026-07-27` and
  `-Z unstable-options --shuffle-seed 353`; 1 benchmark remained ignored.
  Nightly emitted deprecation warnings for existing `fetch_update` calls.
- `cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings`,
  formatting, and whitespace checks passed.
