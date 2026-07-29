# Classification Failure Cleanup Design

## Context

Issue #54 covers the normal-exit path in `ProcessHandler::run`. The current `Result::and_then` chain invokes `terminate_supervised` only after successful exit classification. Linux cgroup and Windows Job Object classification are fallible, so a classification error can leave descendants holding output pipes until the output grace expires and cleanup is attempted only by an error-discarding `Drop`.

## Design

### Classify-and-terminate transaction

Extract a synchronous helper in `process/mod.rs` that:

1. classifies the observed root termination;
2. attempts supervised-tree termination regardless of the classification result;
3. returns the classified termination only when both operations succeed;
4. preserves classification as the primary error and appends a termination failure when both fail.

The helper runs before output draining. Existing successful-classification and termination-failure behavior remains unchanged.

### Error composition

The existing `append_cleanup_failure` format is reused. When classification fails and cleanup also fails, the returned error keeps code `process.resource.classify` and adds `supervised termination also failed: ...` to its message. When only termination fails, the existing `process.resource.terminate` error remains primary.

### Deterministic test seam

Extend the existing portable test backend fault counters with a classification-failure counter. The portable backend remains infallible in production construction; only hidden test constructors preload the counter.

Integration tests will run a root that spawns a descendant inheriting stdout/stderr, writes its PID, then exits. Injected classification failure must:

- remain the returned primary error;
- trigger immediate supervised termination of the descendant;
- return before the one-second output-drain grace;
- preserve an injected termination failure as appended cleanup detail while the existing fallback/drop cleanup remains available.

This backend-neutral seam exercises the shared process orchestration on the current host. Linux and Windows CI compile and run the same shared logic, while their native classification paths continue to provide the real error sources.

## Compatibility

No public CLI, JSON, or resource-policy contract changes. The hidden portable constructors are test support. Error codes are preserved; only previously lost cleanup detail becomes visible.
