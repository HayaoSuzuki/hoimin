# Normalized Plan Configuration Validation Design

## Context

Issue #27 addresses audit finding `RUST-AUDIT-005`. A plan manifest stores a
normalized `PlanConfig`, but deserialization currently reconstructs its fields
without replaying the semantic checks owned by `RawRunConfig -> RunConfig`.
Consequently, a syntactically valid edited manifest can reach target
resolution, analyzer discovery, or baseline preparation with a configuration
that the normal CLI path rejects.

The fix must reject invalid normalized configuration as
`plan.manifest.invalid` before project execution. Valid manifest
serialization, candidate discovery, fingerprint validation, and the plan
schema version remain unchanged.

## Decision

Add explicit, reusable semantic validation for normalized configuration in
`hoimin-core`.

- `PlanConfig::validate(&self) -> Result<(), ConfigError>` validates the
  persisted normalized representation.
- `RunConfig::validate(&self) -> Result<(), ConfigError>` validates the runtime
  normalized representation.
- Both methods delegate to shared private validators for selection, limits,
  and test arguments.
- `PlanConfig::into_run_config` remains an exact conversion and does not
  duplicate validation.
- `prepare_verify` calls `PlanConfig::validate` immediately after manifest
  header validation and before candidate normalization, target resolution,
  fingerprint resolution, analyzer discovery, or baseline execution.

This keeps serde responsible for representation decoding and makes semantic
validation an explicit trust-boundary operation. It also avoids reconstructing
a raw CLI configuration that has already lost information such as the original
operator selector strings.

## Validation Contract

### Selection

The normalized selection must retain the same dependencies enforced by the
raw CLI constructor:

- at least one source, file, line, symbol, or changed selector is present;
- `diff_base` is allowed only when `changed` is true;
- `changed` requires at least one source;
- symbol selectors require at least one source.

The validator does not resolve paths or inspect the filesystem. Existing
target resolution continues to own those checks after configuration
validation.

### Limits

The normalized limits must satisfy:

- every stored duration is nonzero;
- `jobs` does not exceed the supported `MAX_JOBS`;
- `max_processes` fits in `u32`;
- `jobs <= max_processes`;
- doubling the baseline timeout and adding one second does not overflow.

Serde-backed nonzero integer fields continue to reject zero during decoding.
The explicit validator still owns cross-field and wrapper invariants so callers
do not rely on private-field construction or serde behavior as a semantic
contract.

### Test Command

`test_argv` must contain at least one native command argument. Empty arguments
inside a nonempty command retain their existing representation and are not
reinterpreted by this change.

### Runtime-Only State

`RunConfig::validate` also preserves the existing runtime dependency that
resume requires a session. `PlanConfig` has neither field, so its validator
does not invent runtime state.

Operators, profile, output format, fingerprint records, and resource mode are
already normalized typed values. This change does not add unrelated
canonicalization or filesystem validation for them.

## Error Boundary and Ordering

`prepare_verify` follows this order:

1. read and deserialize the manifest;
2. validate the manifest header;
3. validate `normalized_config`;
4. normalize requested candidate IDs;
5. convert to `RunConfig`;
6. resolve targets and current source records;
7. resolve fingerprint inputs;
8. rediscover and compare requested candidates.

A `ConfigError` from step 3 is wrapped in
`PlanError::ManifestInvalid(error.to_string())`. The existing CLI mapping
therefore emits `plan.manifest.invalid`.

Configuration errors take precedence over requested-ID, source-change,
fingerprint-change, analyzer, and baseline errors. No filesystem or process
handler is invoked before the normalized configuration is accepted.

## Testing

### Core Contract Tests

Add focused tests that construct or deserialize normalized `PlanConfig` and
`RunConfig` values and assert that both validation entry points share the
applicable rules. Cover:

- missing selector;
- invalid selector dependencies;
- empty test argv;
- zero normalized duration;
- excessive jobs or processes;
- `jobs > max_processes`;
- baseline-timeout arithmetic overflow;
- runtime resume without a session;
- a valid round trip.

Tests may deserialize private normalized wrappers through serde to model the
actual persistence boundary. They must not weaken field privacy solely to
manufacture invalid values.

### Plan Boundary Tests

Add a table-driven test that first creates a valid plan, edits one normalized
configuration property per case, and invokes verification. Each case asserts:

- exit behavior reports `plan.manifest.invalid`;
- an analyzer marker is absent;
- a baseline marker is absent;
- no candidate execution begins.

The table covers empty test argv, selector dependencies, zero durations,
excessive jobs/processes, and `jobs > max_processes`. Existing valid and legacy
manifest tests continue to pass unchanged.

### Focused Mutation

Run `cargo mutants` only against the new normalized validators and the
`prepare_verify` validation boundary. The objective is to catch viable
mutations of validation predicates and the early rejection path without
expanding into unrelated configuration normalization or plan replay logic.
Every viable mutation in the retained focused set must be caught; unviable and
timeout outcomes are reported separately.

## Compatibility and Scope

- No valid plan JSON field or value changes.
- `PLAN_SCHEMA_VERSION` remains unchanged.
- Existing CLI constructor validation remains in place and reuses the shared
  normalized rules where doing so preserves error precedence.
- The source-level addition of validation methods is backward compatible.
- Candidate discovery, source/fingerprint comparison, session persistence, and
  report schemas are out of scope.
- The design and implementation plan are committed on
  `fix/issue-27-plan-config-validation` in the Issue #27 worktree.
