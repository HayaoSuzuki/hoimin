# Issue #334: Reject timeout values outside the supported deadline range

## Problem

Duration parsing accepts values up to `u64::MAX` seconds, while run and
process scheduling later use unchecked `Instant + Duration` expressions. A
huge `--total-timeout` therefore panics before the run starts, and a huge fixed
`--mutant-timeout` can panic after baseline execution. Configuration validation
currently rejects zero durations and auto-timeout arithmetic overflow, but it
does not establish that every effective timeout can be used as an `Instant`
deadline.

## Goals

- Reject oversized analyzer, baseline, fixed mutant, and total timeouts during
  configuration validation.
- Reject a baseline timeout when its derived auto mutant timeout exceeds the
  same supported range.
- Return the existing flag-naming `ConfigError::InvalidLimit` instead of
  panicking before or during a run.
- Apply the same invariant to raw CLI input and deserialized normalized plan
  configurations.
- Keep the validation boundary deterministic across machines and time.
- Ensure internal grace-period derivation cannot overflow an already valid run
  deadline.

## Non-goals

- Supporting runs with century-scale deadlines.
- Changing duration parsing syntax or the serialized duration representation.
- Replacing every internal deadline addition unrelated to user run limits.
- Promising that a run remains alive for the entire accepted timeout.

## Options considered

### Validate with `Instant::now().checked_add`

This directly probes the current platform, but makes accepted configuration
depend on the platform and the current instant. A duration accepted during
validation can also approach the platform boundary before a later deadline is
created. This is rejected as the primary contract.

### Use checked additions only at runtime

This avoids a panic but moves invalid-input handling into multiple execution
paths and can still fail after baseline work has started. It is rejected
because configuration validation is the established flag-naming boundary.

### Establish a fixed cross-platform ceiling

Expose `MAX_TIMEOUT` as 100 years using 365 days per year. Rust documents that
durations around one hundred years can be used comfortably with `Instant` on
all platforms. Every configured duration and the derived auto mutant timeout
must be nonzero and at most this ceiling.

This is the selected design. It is deterministic, leaves an enormous practical
range, and protects every current unchecked deadline created from run limits.

## Detailed design

`hoimin-core` defines the public constant `MAX_TIMEOUT`. Limit validation uses
one helper that rejects a duration when it is zero or greater than the ceiling.
It checks analyzer, baseline, fixed mutant, and total timeout values.

Auto mutant timeout is an effective runtime deadline even though it is not an
independent raw duration. When `MutantTimeout::Auto` is selected, validation
computes `auto_mutant_timeout(baseline)` and rejects it above the ceiling. The
error names `baseline_timeout`, because `--baseline-timeout` is the user input
that determines the invalid derived value. This also subsumes the old
`Duration` arithmetic-overflow check.

`RunLimits::try_from(&RawRunLimits)` constructs the normalized values and then
calls the shared validator. `RunConfig::validate` and `PlanConfig::validate`
continue calling the same validator so persisted normalized configurations
cannot bypass the invariant.

Outer finalization extends the total-timeout deadline by a two-second shutdown
grace. That addition is not another configured duration and can cross an
`Instant` representation boundary even when the original deadline fits.
`ShutdownBudget` therefore uses `checked_add`; if the grace cannot be
represented, it retains the original run deadline. The same fallback applies
to failure and cancellation grace derivation. This preserves the configured
deadline and avoids turning best-effort cleanup headroom into a panic.

The inclusive boundary is intentional: `MAX_TIMEOUT` is accepted, while one
nanosecond above it is rejected. For auto mode, the greatest accepted baseline
is the largest duration whose `max(5s, 2b + 1s)` result is at most
`MAX_TIMEOUT`.

## Formal contract

A Lean model classifies analyzer, baseline, fixed mutant, auto mutant, and
total timeout inputs at zero, the inclusive ceiling, and immediately beyond
the ceiling. It proves that accepted configurations have every effective
timeout in `(0, MAX_TIMEOUT]`, including the derived auto mutant timeout.

Sensitivity checks include a broken validator that checks only the baseline
input and omits its derived auto timeout. A generated corpus is compared with
the public Rust configuration boundary, including the exact accepted and
rejected auto-baseline edges. The Lean model covers configured timeout spans,
not absolute `Instant` values or internal finalization grace; the latter is a
separate checked Rust operation with a boundary regression. The audit does not
prove Rust platform internals; the 100-year ceiling is the explicit assumption
derived from Rust's `Instant` portability contract.

## Test design

- Add raw configuration tests for each direct timeout at zero, the exact
  ceiling, and one nanosecond beyond it.
- Add normalized plan tests for the same direct boundary.
- Test both sides of the auto-baseline derived boundary and its flag identity.
- Compare every strict Lean corpus row with public `RunConfig` and
  `PlanConfig::validate` behavior.
- Add CLI regressions showing huge total and fixed mutant timeout values return
  exit code 2 and name the offending flag without starting project work.
- Check that accepted maximum durations can be added with both standard and
  Tokio `Instant::checked_add` on supported CI platforms.
- Complete a run with the maximum total timeout, and force a finalization-grace
  addition at the platform's representable `Instant` edge to verify fallback.
- Run focused mutation testing over the limit validator before the full suite.
