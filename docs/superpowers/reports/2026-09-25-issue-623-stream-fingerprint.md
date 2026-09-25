# Issue 623 reviews and verification

Design and plan were committed before production changes (`c20b7c7`), each with three review passes. No fingerprint schema or public record changes are introduced.

## Implementation review passes

1. Capability boundary: compared Unix and Windows paths before/after extraction. Both byte reads and digest reads consume the same inspected regular-file handle. Unix retains no-follow/nonblocking flags and post-open metadata; Windows retains open_final verification and encloses consumption in the existing non-file error reclassification. Parent directories remain pinned and no path is reopened for streaming.
2. Error and ordering contracts: exact paths retain input order and duplicate reads, glob/exact overlap still uses the exact digest, sorted record output and all error prefixes remain unchanged. RootRelativeReader uses the same is_missing classifier for read/open failures, including symlinked-parent rejection. The standalone whole-file helper had no other callers and was replaced without changing other byte-reading consumers.
3. Memory behavior: inspected locked blake3 1.8.7 update_reader/copy_wide, which uses a fixed 64-KiB stack buffer and retries interrupted reads. Empty input and trailing chunks are handled by the library. No mmap, new file-size limit, digest framing or serialization changes were added.

## Test review passes

1. Sensitivity: process-global allocation measurement surrounds public resolve only; input writing and independent one-shot digest construction occur before the window. The old code failed at 1 MiB exact (1,049,225 peak bytes); all 27 old/new observations are retained for exact/glob/overlap and 1/16/64 MiB with three repeats.
2. Semantic controls: all returned records are checked, including path and digest. Binary inputs contain every byte value; separate tests exercise empty, one byte, 64-KiB boundaries and an unaligned tail. Existing missing, symlink, directory, duplicate and error-priority tests passed (29 total fingerprint tests).
3. Filesystem safety: added digest coverage to FIFO rejection and a paused-parent replacement race proving hashing reads the opened parent rather than the replacement path. All 27 WorkerRoot tests passed, including unchanged byte-read races. Independent review examined both platform branches and found no blockers. Windows execution remains covered by repository CI, not claimed as a local macOS run.

## Results

New resolver peak heap was 905 bytes for exact mode at every size and repeat, and at most 9,533 bytes for glob/overlap (normally 9,517), compared with whole-file growth before the change. This excludes the fixed stack buffer and is a Rust allocator measurement, not process RSS. No speed improvement is claimed. The committed JSON contains all observations.

Full workspace: 2300 passed, 0 failed, 22 ignored across 98 test groups. Both exact CI Clippy commands, workspace/vendor formatting and diff checks passed.
