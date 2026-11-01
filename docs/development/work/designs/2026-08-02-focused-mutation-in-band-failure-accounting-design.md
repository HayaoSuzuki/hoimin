# Focused Mutation In-Band Failure Accounting Design

## Problem

Focused mutation discovery can fail without raising an exception: the cargo-mutants version probe can exit non-zero, the installed version can be unsupported, or inventory generation can exit non-zero. These paths set a terminal run state but leave discovered candidates `pending`, so persisted output looks like an interrupted checkpoint rather than a completed failure.

## Decision

Introduce one small helper for failures detected before mutation execution. It records the terminal state and error and immediately converts every pending candidate to `not_run`, using the run-state value (`tool_unavailable` or `command_failed`) as the reason.

Use the helper at all three in-band failure sites. Exception paths keep their existing accounting, and mutation/baseline failure behavior remains unchanged.

## Verification

Regression tests cover a non-zero version command, an unsupported pinned version, and a non-zero inventory command. Each test checks the returned and persisted candidate accounting and confirms no baseline or mutation command starts.
