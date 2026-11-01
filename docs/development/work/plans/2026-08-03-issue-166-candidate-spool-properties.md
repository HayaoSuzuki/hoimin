# Issue #166 Candidate Spool Properties

## Goal

Protect the private candidate spool's ordered replay and serialized record-size contracts with
generated data and exact boundary examples.

## Scope and placement

`CandidateStore`, its replay offsets, and `MAX_SPOOL_RECORD_BYTES` are private analyzer
implementation details. The tests therefore remain in `crates/hoimin-cli/src/analyzer/store.rs`
under `#[cfg(test)]`. No production limit is weakened and no public test-only API is introduced.

## Contracts

- A 128-case property generates non-empty ordered candidate vectors with sequences `1..=n`,
  normalized relative Python paths, known operators, bounded spans and locations, and Unicode-heavy
  paths, source fragments, replacements, IDs, and optional symbols.
- The complete replay equals the original generated vector, and replay at the terminal offset is
  exactly `Ok(None)`.
- The test starts with offset zero and records every later intermediate offset only from a
  successful `replay_one`. Replaying independently from each offset must equal the corresponding
  suffix of the original vector; expected values never come from a prior replay.
- A bounded padding helper measures `serde_json::to_vec` and creates payloads of exactly
  `MAX_SPOOL_RECORD_BYTES - 2`, `MAX_SPOOL_RECORD_BYTES - 1`, and
  `MAX_SPOOL_RECORD_BYTES` bytes.
- The first two payloads are accepted and their newline-terminated files measure exactly limit
  minus one and limit. The exact-limit payload returns `StoreError::RecordTooLarge` with the
  production limit and leaves count, written-record count, and file length at zero.

Default proptest source-parallel failure persistence remains enabled. Any generated regression
seed will be committed beside the source test.

## Verification

Run:

```text
cargo test -p hoimin-cli analyzer::store
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```
