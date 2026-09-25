# Issue 629 implementation plan

1. Commit this design and plan before code. Add a public allocator regression for
   four workspace stages and observe failure against the current implementation.
   Add binary boundary/tail mismatch and outside-hardlink restoration controls.
2. Add bounded read/hash/copy/compare helpers with short-read, Interrupted, EOF,
   and checked-size handling. Change manifest processing and snapshot writing to
   consume one checked source handle and return its final manifest entry. Preserve
   duplicate content checks, limit checks, final revalidation, and directory sets.
3. Add checked snapshot handle access and streaming write/restore APIs to WorkerRoot
   and its Windows backend, retaining byte-slice adapters where public callers need
   them. Close retained handles before directory cleanup. Stream worker creation
   with charge/release and optional contract hash in the same pass.
4. Stream reset comparison, rewind the same snapshot on mismatch, unlink/create
   through the retained parent, and preserve permissions. Update reset I/O metrics
   and tests only for the documented changed-file reread. Test with and without
   contracts, existing native races, size mismatches, permissions, and cleanup.
5. Tighten existing preflight allocator bounds, document content-size versus metadata
   bounds and the reset read tradeoff, and record three implementation and three
   test self-reviews with findings. Run focused tests then full workspace, exact
   CI Clippy/fmt and diff checks. Root independently reviews before publication.
   Publish only within the authorized two-branch stack; root owns CI/merge/cleanup.

## Plan reviews

1. RED uses real public WorkspacePlan/WorkerWorkspace calls and allocator observations,
   so reverting any stage to fs::read/read_to_end fails. Each stage is measured
   separately; data setup cannot hide or dominate the measurement.
2. Audit of existing tests found exact source read/hash counts, no-write limit
   failures, duplicate-snapshot preservation, Windows case collisions, unchanged
   inode, and contract postchecks. Keep these and explicitly adjust only reset
   changed-file snapshot bytes; do not count chunks as separate files.
3. Sequence checked-handle abstractions before materialization/reset callers so
   Linux/macOS and Windows share streaming logic while retaining platform opens.
   Use the assigned Cargo cache with local-crate cleaning; no concurrent builds in
   another worktree. Full tests and independent review are required before push.
