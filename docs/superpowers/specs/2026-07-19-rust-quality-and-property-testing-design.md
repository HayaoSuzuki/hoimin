# Rust Quality and Property Testing Design

## Goal

Raise the Rust quality gate with a workspace-wide, enforceable Clippy policy and
add property-based tests for the core data invariants and Rust analyzer safety.

## Scope

- Treat `clippy::all` and `clippy::pedantic` warnings as errors for every
  workspace crate, target, and feature combination.
- Keep exceptions local to the code that needs them and document why each
  exception preserves the intended behavior.
- Use `proptest` for deterministic core invariants and parser/analyzer safety
  invariants.
- Keep the existing Rust-only runtime and dependency-free distribution wheel
  unchanged.

## Clippy Policy

The root `Cargo.toml` owns the common lint policy.  Both workspace crates opt
into that policy so local commands and CI cannot drift.

`clippy::all` and `clippy::pedantic` are `deny`.  `clippy::nursery` and
`clippy::cargo` are intentionally excluded: their churn and dependency-policy
opinions do not provide a stable project-wide correctness gate.  A required
exception is written at the narrowest applicable item with a comment explaining
the domain constraint; global blanket allowances are not permitted.

CI invokes Clippy without ad-hoc lint flags, relying on the manifest policy.

## Property-Based Tests

`proptest` remains a development-only dependency.

Core tests cover these properties:

- Equivalent fingerprint input produces the same fingerprint on repeated
  evaluation.
- Changing a semantically represented input field changes the fingerprint.
- Serializing supported workspace and configuration values never panics and
  produces a non-empty fingerprint.

Analyzer tests generate arbitrary Python source text and assert that analysis
does not panic.  Every returned candidate span is bounded by the source text,
and candidates remain ordered where the analyzer promises ordered output.
Generated test inputs use practical size limits so the normal suite remains
fast and reproducible.  Failing seeds are persisted through Proptest's normal
regression mechanism when applicable.

## Verification

The finished change must pass:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
uv run maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

## Documentation Hygiene

The existing untracked Superpowers plans and implementation reports describe
completed, superseded work.  They are deleted rather than committed.  This
design document and its implementation plan are committed because they govern
the active quality work.
