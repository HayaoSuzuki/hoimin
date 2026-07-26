# Progress Stall Adjacency Implementation Report

## Scope

- Issue: #29
- Mutation target: `compare_reports`
- cargo-mutants version: 27.1.0
- Test target: `cargo test --test progress`

The mutation run was deliberately limited to the progress state machine. It
did not mutate the rest of `compare.rs` or the Rust workspace.

## Focused Mutation Result

Command:

```console
cargo mutants --package hoimin-cli --jobs 4 \
  --file crates/hoimin-cli/src/progress/compare.rs \
  --re "compare_reports" \
  -- --test progress
```

- Total: 4
- Caught: 3
- Missed: 0
- Unviable: 1
- Timeout: 0

The unviable mutant replaced the complete `compare_reports` body with
`Default::default()`. It failed to compile because `ProgressResult` does not
implement `Default`. All three viable mutants—two changes to the stall
increment and one inversion of the patience comparison—were caught by the
progress integration tests.

## Behavioral Coverage

- Three adjacent stalls still saturate at patience three.
- A same-ID-set regression between stalls breaks adjacency.
- A same-ID-set inconclusive status between stalls breaks adjacency.
- An improving comparison breaks adjacency.
- An indeterminate empty-common comparison breaks adjacency without changing
  its eligibility classification.
- An unusable report gap cannot carry a stalled suffix into a later decision.

## Verification

The following fresh local checks passed:

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
cargo build --workspace
git diff --check
```

The focused progress suite passed 37 tests. The strict Clippy configuration
was not weakened and no lint exception was added.
