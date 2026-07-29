# Wait Error Cleanup Design

## Scope

Fix #66 so an I/O error returned by the root child's normal wait path cannot
skip explicit supervised-tree termination and bounded root reaping.

## Design

Treat the wait error as the primary failure. Before output collection is
awaited, run the same `terminate_and_reap` path used by cancellation and
timeout. If cleanup succeeds, return the unchanged `process.wait` failure. If
cleanup also fails, append its details to the primary failure using the
existing cleanup-error convention.

Factor the primary-error/cleanup composition into a small async helper that
accepts a cleanup future. This makes the rare wait-error path deterministic to
unit test without relying on platform-specific races or externally reaping a
Tokio child.

## Ordering and failure semantics

1. `child.wait()` returns an I/O error.
2. The supervisor terminates the complete owned tree.
3. The root is killed/reaped with existing bounded retries if necessary.
4. Output tasks receive pipe closure and are awaited.
5. The original `process.wait` error is returned, with cleanup details appended
   only if cleanup failed.

## Testing

Inject successful and failed cleanup futures into the composition helper.
Assert each future is polled, the original wait code/operation remains primary,
and cleanup details are appended in the existing format. Existing
`terminate_and_reap` integration coverage continues to exercise real root
termination and bounded reaping.
