# Issue 613: bounded buffering for stderr JSON diagnostics

Baseline failures export retained output as 16-KiB diagnostic chunks. JSON serialization currently sends individual escaped string fragments directly to the unbuffered stderr writer, producing roughly one million writes for a 1-MiB short-line log. Coalesce only the JSON/JSONL diagnostic stderr path while preserving emitted bytes, event boundaries, routing, acknowledgements and errors.

## Design

Add a private `jsonl::write_buffered_event` helper that wraps the borrowed stderr writer in an event-local `BufWriter` with explicit 8-KiB capacity. Reuse `write_event` for serialization, newline and explicit flush. Always consume the buffer using `into_parts` after the attempt, discarding any unwritten bytes before returning the original result. This prevents `BufWriter::drop` from retrying buffered partial output after an error. `handle` acknowledges only after serialization and both buffer/underlying flush succeed, retaining `ReportIo` and the same effect ID on errors.

No global buffering or destructor-dependent success is introduced. Human diagnostics and normal JSONL stdout continue through their existing writers. The JSON document spool, report/event schema, baseline chunk size, truncation, lossy decoding and terminal-text sanitization remain unchanged. Capacity is fixed; no whole diagnostic/event Vec is retained. Event-local allocation also bounds lifetime and avoids changing the generic ReportHandler storage/interface.

A persistent handler-level stderr buffer could amortize allocations but would complicate ownership and shutdown/error behavior. Serializing into a Vec would scale extra memory with event size. The event-local fixed buffer addresses the measured write amplification with the smallest error boundary.

## Verification boundary

Count actual public ReportHandler writer calls for plain, short-line and quote/backslash/control-heavy payloads; compare exact bytes against ordinary serde JSON plus one newline and parse messages back. Large-input heap tests must establish bounded extra memory. Inject partial writes, write-zero, interrupted writes, transient write errors and flush errors; assert no success acknowledgement and no destructor retry after failure. Existing report delivery/flush and real baseline tests preserve surrounding behavior. Use a release public run_with_io probe where practical to measure actual baseline diagnostics before/after; report write counts, payload equality and timings separately.

No semantic decision/state transition changes are intended. Existing Rust report-delivery/oracle consumers provide preservation evidence; no new Lean model or process is needed for this serialization mechanism.

## Design review passes

1. Error review: a plain temporary BufWriter would retry buffered bytes on Drop after a write error. Consume it with `into_parts` on both success and failure, retaining the original error and forbidding post-failure retry.
2. Scope review: wrapping all stderr would also change human and unrelated messages, and wrapping stdout would not address the stated unbuffered diagnostic path. Restrict the helper call to JSON/JSONL diagnostics in ReportHandler.
3. Memory/evidence review: one large Vec hides write amplification by trading unbounded memory. Choose fixed 8 KiB and verify the public writer-call reduction, exact escaped bytes, event flush visibility and heap scaling separately from noisy wall-clock timing.
