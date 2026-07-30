# Async Session Dispatch Design

## Goal

Prevent SQLite busy waits and schema setup from blocking Tokio scheduler
workers while preserving serialized connection access and transaction
semantics.

## Design

`SessionDispatcher` owns a `SessionHandler` behind an `Arc<Mutex<_>>`. Opening
and configuring the connection is performed with `spawn_blocking`; every load,
lookup, begin, persist, and finish call clones the owner and runs the existing
synchronous handler method on Tokio's bounded blocking pool.

The mutex preserves the current single-connection ordering. Transactions remain
entirely inside the existing synchronous handler methods, so rollback, commit,
primary error codes, and run-finality checks are unchanged. Poisoning is mapped
to a typed `session.dispatch` failure rather than panicking.

`ShellContext` lazily creates one dispatcher for the configured session path.
The run loop already races effect execution against cancellation and its total
deadline. Once SQLite work is off the scheduler worker, those branches can win
while the blocking operation finishes independently. No platform-specific
threading or path behavior is introduced.

## Testing

- Hold a conflicting write lock with a second real SQLite connection.
- Start a session begin effect through the dispatcher and prove an outer
  cancellation/deadline branch returns well before the five-second busy timeout.
- Release the lock and verify the original operation retains its typed database
  result and the dispatcher remains usable.
- Run existing rollback, resume, finality, WAL, and schema tests unchanged on
  Linux, macOS, and Windows.

