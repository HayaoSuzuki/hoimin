# Preserve transition error during shutdown design

## Problem

When a state transition fails, the run loop cancels outstanding work and drains
process tasks before returning the transition error. The transition-error branch
currently propagates a drain failure with `?`, so a simultaneous process-task
failure replaces the transition error that caused shutdown.

Other shutdown branches already preserve the primary error and append an
optional drain failure. The transition branch should follow the same ordering.

## Design

Introduce a small formatter for a primary failure plus an optional drain
failure. It returns the primary failure unchanged when draining succeeds and
formats `"{primary}; {drain_failure}"` when draining fails. Reuse it in every
shutdown path that already has a primary error, removing duplicated formatting
and making the error precedence explicit.

In the transition-error branch, collect `drain_processes(...).await.err()` and
return the combined message. Cancellation and draining behavior otherwise stay
unchanged.

## Tests

Unit tests pin both formatter cases: a successful drain keeps the transition
error exactly, while a failed drain retains the transition error first and
appends the process-task failure. Existing shell and workspace tests guard the
surrounding run-loop behavior.
