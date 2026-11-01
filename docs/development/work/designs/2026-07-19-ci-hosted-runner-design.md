# Hosted CI Reliability Design

## Goal

Make GitHub-hosted Linux and Windows CI exercise the supported portable path
reliably while preserving dedicated validation of the Linux cgroup v2 hard
backend.

## E2E Resource Policy

The shared `run_e2e` argument builders pass `--allow-best-effort-memory`.
GitHub-hosted Linux runners do not delegate the `memory` and `pids` cgroup v2
controllers, so this explicitly selects the supported portable fallback instead
of treating runner permissions as a product failure. The existing delegated
self-hosted cgroup job remains the hard-backend test.

## Wheel Build

The wheel smoke job invokes `uvx maturin build --release`. Maturin is acquired
only for the CI build command; it is not added to project or wheel runtime
dependencies. The existing smoke script continues to validate the installed
wheel and `uvx` invocation.

## Verification

- `cargo test --workspace` passes on GitHub-hosted Linux and Windows.
- Contract tests use the same portable fallback on GitHub-hosted Linux.
- Wheel smoke builds and tests on GitHub-hosted Linux and Windows.
- The delegated self-hosted cgroup v2 hard job remains unchanged.
