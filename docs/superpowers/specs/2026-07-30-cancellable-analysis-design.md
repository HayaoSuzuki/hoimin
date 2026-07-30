# Cancellable Analysis Design

## Goal

Keep source analysis and candidate spooling responsive to cancellation and the
run deadline without exposing a partial candidate spool.

## Design

`AnalyzerHandler` keeps source capability ownership on the dispatcher, then
moves UTF-8 decoding, parsing, candidate construction, validation, and spool
writes into one `tokio::task::spawn_blocking` operation. The blocking operation
owns the request data and the current `CandidateStore`, so the async scheduler
does not execute parser or filesystem work.

The Rust analyzer accepts a lightweight cancellation callback. It checks the
callback before parsing-dependent work and in long token, AST-derived
candidate, filtering, deduplication, and conversion loops. Cancellation is a
typed analyzer outcome rather than an invalid-syntax diagnostic.

The async side races the blocking task against `ProcessCancellation`. When
cancellation wins, it returns `analyzer.cancelled` immediately. The blocking
task continues only until its next cooperative check and owns the partial
store; dropping that store removes its temporary file. Only a successful final
target calls `CandidateStore::finish`, so cancellation cannot publish a partial
spool as complete.

## Error and State Semantics

- Existing source-read, UTF-8, candidate, and store error codes are preserved.
- A blocking-task panic or runtime shutdown maps to `analyzer.task`.
- A cancelled request discards candidates accumulated by that request. Earlier
  non-final requests remain in their store when cancellation happens before a
  later request is handed to the blocking worker.
- Candidate ordering and the configured candidate limit remain unchanged.

## Testing

- A deterministic analyzer checkpoint seam blocks in-progress analysis until
  cancellation is issued, then proves prompt completion and no published spool.
- A deadline test exercises the same seam through the run loop.
- A large-source test cancels during token traversal and checks bounded
  completion while existing ordering and candidate-limit tests remain green.
- Tests use platform-neutral synchronization primitives and no timing-sensitive
  sleeps beyond an outer failure bound.
