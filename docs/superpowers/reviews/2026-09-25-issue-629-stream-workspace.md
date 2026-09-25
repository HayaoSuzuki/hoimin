# Issue 629 review and verification

Design and plan were committed as `f925ad2` before tests or production edits.
The parent is Issue 616 (`39c53d4`), whose directory inventory and restoration
remain intact. This change bounds content buffering; entry metadata still scales
with the selected tree. No new CLI option, schema, or Lean model is introduced.

## Implementation reviews

1. Traced manifest discovery through snapshot creation and final verification.
   Both hashes and saved bytes now consume identical chunks from one checked
   source handle; duplicate visits rehash without replacing the first snapshot.
   Initial metadata checks preserve early limit rejection and incremental checks
   reject growth before the next write. Existing final revalidation remains.
2. Audited checked-handle lifetimes and the Unix/Windows branches. Snapshot readers
   close before owned-directory cleanup. Windows opens still validate the final
   name, regular type, and reparse status before callbacks, and the writable handle
   has permission-write access. Restore retains unlink-before-create and the same
   parent hook. Found and fixed error reclassification that would have mislabeled
   callback write/permission failures as snapshot-read failures.
3. Traced reset equality, rewind, permissions, and allowance rollback. Comparison
   checks complete streams, independent short reads, both EOFs, and permissions;
   size mismatch skips worker reads. Changed reset rewinds the existing snapshot
   handle. Added partial-charge failure/retry and growing-stream limit tests.
   Clippy identified oversized stack arrays; moved fixed buffers to the heap and
   repeated allocation measurement without weakening its common fixed bound.

## Test reviews

1. The allocator test calls public preflight/create/reset APIs in an isolated
   integration binary. Fixture setup and verification occur outside measurements.
   Each stage is measured separately for 1/8/32 MiB and three repetitions. Real
   eager fs::read exceeds the same 512 KiB bound; the old implementation fails.
   Existing metadata/worker-count and retained-heap axes remain separate.
2. Added 0/1/65535/65536/65537/131073-byte binary fixtures, first/middle/last-byte
   changes, shorter/longer contents, directory replacement, and changed original
   source. Short-read/Interrupted tests exercise the actual streaming helpers.
   External hardlink content remains unchanged and restored files no longer alias
   the referent. Existing native permission, symlink, inode, parent-race, and
   cleanup tests remain part of focused/full verification.
3. Checked instrumentation against actual reads. Source file/hash events remain
   per completed file, not per chunk. Snapshot reader bytes include the changed
   reset rewind. Only changed-file snapshot bytes were added to the reset oracle;
   unchanged padding is still read once. Contract postchecks add their own pass.
   No assertion or documentation claims RSS, all-operation constant memory, or
   native Windows execution from these macOS measurements.

## Allocation evidence

Requested live heap bytes above the baseline, excluding allocator-internal
retention and realloc implementation copies. All three repetitions agree per row.

| File | Version | Preflight | Create | Reset unchanged | Reset tail change |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 MiB | RED | 1,090,533 | 1,049,160 | 2,097,688 | 2,097,688 |
| 8 MiB | RED | 8,430,565 | 8,389,192 | 16,777,752 | 16,777,752 |
| 32 MiB | RED | 33,596,389 | 33,555,016 | 67,109,400 | 67,109,400 |
| 1/8/32 MiB | GREEN | 105,439 | 65,950 | 131,608 | 131,608 |

Each measured operation also verifies full restored content, logical byte counts,
and copy accounting. The unchanged/changed measurements include the accepted
snapshot path rather than a replacement implementation. Whole-fixture elapsed
times (including setup and verification) were roughly 34/267/1090 ms before and
34/260/1063 ms after at 1/8/32 MiB; these observations are not timing gates or an
isolated operation speedup claim. The changed-reset extra snapshot pass is explicit.

## Validation

RED: public allocator regression failed at its fixed bound on the original code.
GREEN: allocator test passed with the values above. Related integration tests,
including binary/hardlink and prior workspace heap tests, passed. Contracts-enabled
workspace tests: 170 passed, 0 failed, 3 ignored. Both exact CI Clippy commands and
both formatting checks passed. Full workspace: 2,397 passed, 0 failed, 22 ignored
across 110 test/doc-test result groups. This full run includes the final fixed-heap
buffers, source-growth and partial-charge tests, and all source changes.

Independent root review found no blockers in buffer bounds, checked chunk arithmetic,
opened metadata, duplicate rehashing, drop order, Windows writable-handle rights,
same-handle permissions, reset rewind/unlink/create, and charge-before-write/error
release. This was a production-code review, not native Windows execution. One
low-priority observation remains within the existing Windows callback semantics:
if the source entry concurrently becomes non-regular, its error reclassification
can report InvalidPath even when the processing callback failed while writing.
No stronger concurrent-path diagnostic guarantee is claimed by this change.

After Issue 616 merged, rebased only the two Issue 629 commits onto main `99c27dc`.
No conflicts occurred and both range-diff entries were unchanged. Cleaned local
workspace artifacts before verification: contracts-enabled workspace tests passed
170 (3 ignored), and seven related public/allocator integration binaries passed
47. Both exact CI Clippy commands and both formatting checks passed again. The
full-workspace count above predates this rebase; this final run verifies the
streaming implementation against the newly merged main changes.

## CI performance registry correction

CI completed the Rust suite but rejected the performance gate: the registered
preflight test still used its old name, so Cargo executed zero tests. Updated
only that registry selector to the renamed bounded-buffer test. The reset test
rename has no active registry reference; historical design commands remain intact.

Reproduced evidence is the CI gate log with zero executed tests. After correction,
the complete performance runner passed all 34 active gates, including exactly one
preflight test. Registry validation (29 shapes), all 16 performance-runner Python
tests, and git diff --check passed. No Python or Rust implementation changed, so
this correction does not require another full Rust suite or mutation run.
