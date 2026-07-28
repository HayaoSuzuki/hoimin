# Plan Resource Policy Validation Design

## Goal

Reject a `hoimin plan` invocation before manifest emission when the planning host cannot
honor the requested resource policy and the user has not explicitly allowed best-effort
memory enforcement.

## Decision

Planning performs a hard, side-effect-free platform policy check. On macOS, omitting
`--allow-best-effort-memory` returns exit code 2, writes the existing actionable resource
error to stderr, and writes nothing to stdout.

This does not make plans host-bound. The manifest continues to describe portable execution
settings, and `verify` repeats host-side validation because verification may happen on a
different operating system. Explicit best-effort permission also does not force a weak
backend: Linux continues to select its hard cgroup backend when available.

## Components

### Resource policy validation

The resource module exposes a side-effect-free validation function for planning. It checks
only static platform policy:

- macOS uses the portable backend policy and requires explicit best-effort permission;
- Windows has a hard Job Object backend and passes;
- Linux is unchanged at plan time because hard cgroup availability is runtime environment
  state, not a portable manifest property.

The existing backend constructors remain responsible for runtime acquisition and
verification-time enforcement.

### Plan creation

`plan::create` invokes resource policy validation before fingerprint resolution, target
resolution, source reads, or analyzer execution. A policy failure becomes a typed
`PlanError` whose display text preserves the existing `ResourceError` guidance.

The CLI's existing plan error path already maps any `PlanError` to exit code 2, stderr, and
no manifest on stdout.

## Error contract

On unsupported hard memory enforcement without opt-in, the diagnostic remains:

```text
portable resource limits require --allow-best-effort-memory: macOS uses process groups and RLIMIT_CPU; max-memory is not enforced
```

No partial or otherwise apparently usable manifest is emitted.

## Testing

- A platform-independent unit seam tests rejection and acceptance without pretending the
  CI host is macOS.
- A macOS-gated CLI integration test exercises the real `plan` command, checks exit 2,
  empty stdout, actionable stderr, and proves the test command did not run.
- Existing plan tests continue passing because their helper already supplies
  `--allow-best-effort-memory`.
- The full workspace test suite and Clippy cover Linux and Windows non-regression through
  CI; runtime verify validation remains unchanged and retains its existing tests.

## Non-goals

- Do not acquire or probe a runtime resource backend while planning.
- Do not record the planning operating system in the manifest.
- Do not weaken verify-side policy validation.
- Do not add an automatic best-effort opt-in.
- Do not change manifest schema version 2.
