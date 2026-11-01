# Reproducible Rust Test Shuffle CI Design

## Goal

Detect accidental Rust test-order dependencies by running the complete workspace test suite
in a randomized, reproducible order without replacing the existing stable test matrix.

## Decision

Add one independent Ubuntu CI job using Rust's standard nightly test harness:

```console
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

The nightly toolchain is pinned by date so the runner does not change underneath an
unchanged revision. The standard stable quality and Rust jobs remain the authoritative
cross-platform gates.

## CI job

The `rust-shuffle` job:

- depends on `quality`;
- runs on `ubuntu-latest`;
- checks out the same revision as every other job;
- installs Python 3.14 and uv;
- installs `nightly-2026-07-27` with the minimal profile;
- runs `uv sync --frozen`, because Rust integration tests require the controlled Python
  interpreter under `.venv`;
- invokes the full workspace with `-Z unstable-options --shuffle`.

Ubuntu is sufficient for the initial order-dependency signal. Existing stable Ubuntu,
Windows, and macOS jobs continue covering platform differences, so the shuffle job does not
multiply nightly cost across operating systems.

## Failure reproduction

The standard harness prints its generated shuffle seed. Development documentation provides
the replay command:

```console
cargo +nightly-2026-07-27 test --workspace -- \
  -Z unstable-options --shuffle-seed <SEED>
```

The seed is not fixed in ordinary CI because varying the order on each run is the behavior
under test. A failure is made deterministic only during diagnosis.

## Executable workflow contract

A standard-library Python test reads `.github/workflows/ci.yml` and verifies durable
properties rather than parsing implementation details:

- the existing stable workspace test command is still present;
- `rust-shuffle` is an independent job on Ubuntu;
- the nightly toolchain contains an exact date;
- the job runs `uv sync --frozen`;
- the test command contains both `-Z unstable-options` and `--shuffle`.

The contract extracts only the `rust-shuffle` job block so another job cannot accidentally
satisfy its assertions.

## Non-goals

- Do not replace libtest with a third-party framework.
- Do not replace stable CI with nightly.
- Do not use cargo-nextest priorities as a substitute for random order.
- Do not randomize Python tests in this change.
- Do not add nightly to release workflows.
- Do not run the initial shuffle job on all supported operating systems.
