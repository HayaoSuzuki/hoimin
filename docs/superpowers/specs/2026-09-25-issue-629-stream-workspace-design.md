# Issue 629: bounded workspace content buffers

Preflight, worker creation, and reset currently retain a complete file, and reset
retains both snapshot and worker content. Replace these buffers with fixed-size
streaming while retaining the shared disk snapshot, file metadata, and Issue 616
selected-directory inventory. This is an architectural internal API change, not
a CLI or persisted-schema change. The approved scope prioritizes existing handle
safety and measurable heap bounds over eliminating every repeated read.

## Selected approach

Manifest discovery opens each selected source once and passes its checked handle
to a file processor. A common fixed-buffer loop computes the manifest hash and
feeds identical chunks to the snapshot writer. Standalone manifest verification
uses the same loop without a destination. Actual streamed bytes determine size;
metadata provides an early limit check, followed by checks before each write.
Keep the final source verification pass. Duplicate discovery rehashes and compares
against the first entry without overwriting its snapshot or counting it twice.

Snapshot and worker reads use retained-root no-follow regular-file handles. Worker
creation streams and charges actual bytes before writing; errors release charged
bytes and clean the pending worker. Contract hashes are computed from that same
copy stream. Permissions come from checked handles and are restored after writing.

Reset retains its existing tree walk and directory restoration. Compare equal-size
worker and snapshot streams through two fixed buffers and inspect both EOFs;
metadata never proves content equality. Preserve unchanged file identity. On
inequality, close the worker comparison handle, rewind the same snapshot handle,
remove the old entry, and stream a new file through the retained parent. Never
repair an existing hardlink in place. Size/type/missing mismatches skip worker
content reads. Contract verification uses the same streaming comparison.

Unchanged reset reads each side once. Same-size changed reset may read snapshot
content twice, with no extra source or snapshot reopen. This trades bounded memory
for restoration reads and must be visible in metrics and performance documentation.
Always writing a temporary replacement adds writes to unchanged reset; in-place
repair changes hardlink semantics. Both alternatives are rejected.

## Safety and failure boundaries

Preserve Unix O_NONBLOCK and no-follow opens, regular-file validation after open,
Windows root-relative NtCreateFile/reparse/final-name rules, and existing retained
parent race hooks. Generic streaming callbacks execute while checked handles are
live. Snapshot path collisions must fail before truncation. New retained snapshot
handles close before their owned directory cleanup. Partial copies never become
published workers. Copy/owned limits use checked arithmetic, reject equality where
the existing policy does, and release reservations on errors. Source growth cannot
write an unchecked chunk; final verification still detects source drift.

## Evidence

A separate public-API allocator binary measures preflight, creation, unchanged
reset, and same-size tail-change reset for 1/8/32 MiB files, three repetitions.
Fixture writing and content validation stay outside measured intervals and use
fixed buffers. A real eager-read positive control must exceed the same fixed bound.
Keep file-count and worker-count metadata tests separate from content-size tests.
Preserve byte accounting and I/O metrics, updating changed-reset reads explicitly.
Boundary, permissions, source drift, missing/type/link, hardlink referent, cleanup,
and existing platform/race tests complement allocation evidence. No RSS or runtime
speedup guarantee follows from this work. No new Lean model is required for this
I/O refactor; native handle tests and real allocator measurements are the evidence.

## Design reviews

1. Read the source/hash/snapshot pipeline and its duplicate/limit tests. A completed
   ManifestEntry cannot precede a streaming write, so the callback must consume a
   checked file and return the final entry. Keep duplicate hashes and actual sizes.
2. Read Unix and Windows write/snapshot paths and reset identity/race tests. Moving
   bytes alone is insufficient: checked handles, close ordering, permissions, and
   unlink-before-create must survive. Added explicit retained snapshot ownership.
3. Compared bounded memory with reset I/O requirements. Constant buffering plus no
   unchanged writes cannot also retain single-pass restoration after a tail mismatch.
   Recorded the approved rewind tradeoff and rejected in-place repair/spooling.
