# Issue 261 Plan Analyzer Timeout Design

## Goal

Enforce the normalized `--analyzer-timeout` while `hoimin plan` discovers
candidates and while `hoimin verify` rediscovers requested candidates. A
manifest must never claim an analyzer limit that plan discovery ignored.

## Current failure

`analyzer::discover_targets` is declared async but performs root opening,
source reads, parsing, and candidate construction synchronously on the Tokio
worker that polls it. Neither `plan::create` nor verification rediscovery
passes `config.limits.analyzer_timeout`, so even `--analyzer-timeout 1ns`
succeeds and records an unenforced value.

Wrapping the current future in `tokio::time::timeout` is insufficient: its
synchronous body does not yield, so the timer cannot preempt slow analysis.

## Chosen design

### One owned blocking discovery operation

Move the complete discovery operation behind `spawn_blocking`. The owned task
contains the root reader, targets, operator selection, profile, candidate
limit, and a cancellation token. It checks cancellation before opening or
reading targets, between targets and candidate conversion, and through the
existing cancellable Rust analyzer traversal.

The async caller races the task against one absolute deadline calculated from
the configured analyzer timeout. A ready discovery completion wins a
simultaneous deadline. On expiry the caller cancels the token, drops the join
wrapper without awaiting the blocking thread, and returns immediately. The
detached operation owns all of its resources and releases them when its
cooperative cancellation check completes; it cannot retain mutable plan state
or prevent a subsequent discovery.

The timeout covers the complete plan/verify discovery phase, including source
opening and reading, rather than restarting for every target.

### Consistent plan diagnostics

Discovery timeout uses the stable analyzer identity `analyzer.timeout` and a
message that names `--analyzer-timeout` and its configured duration. Both plan
creation and verification rediscovery map it to the same
`plan.discovery: analyzer.timeout: ...` error and exit code 2. Other plan
creation discovery errors keep `plan.discovery`; verification candidate
identity/read failures keep their existing `plan.candidate.invalid` mapping.

No stdout manifest or verification run output is emitted after discovery
timeout.

### Deterministic test control

A crate-private test-only discovery control pauses inside the blocking task
immediately before analysis and exposes entry/release synchronization. Tests
use a short Tokio deadline only after observing entry, so they do not depend on
CPU load. They prove:

- plan creation reports the timeout and writes no manifest;
- verification rediscovery reports the identical timeout identity before the
  baseline command can run;
- releasing the detached task after timeout drops its owned reader/control;
- a following discovery completes successfully;
- successful plan/verify behavior and the recorded configured limit remain
  unchanged.

## Documentation

Clarify that `--analyzer-timeout` applies to the complete discovery phase used
by `plan` and verification rediscovery. The timeout bounds the caller; an
already-running blocking analyzer cooperatively stops and releases owned
resources afterward. Runtime file-analysis scheduling remains outside this
issue's scope.

## Non-goals

- Changing plan schema, fingerprint compatibility, candidate ordering, or
  candidate-limit behavior.
- Adding a new timeout or grace option.
- Changing total-timeout, baseline, mutant, or process supervision semantics.
- Expanding this issue into a redesign of runtime analyzer scheduling.
