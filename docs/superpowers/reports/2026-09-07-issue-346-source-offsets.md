# Candidate source length validation (#346)

## Change

`CandidateValidationContext::new` now returns `Result` and rejects sources
larger than 4,294,967,295 bytes with `CandidateValidationError::SourceTooLarge`.
The constructor checks the length before traversing newlines, decoding UTF-8,
or hashing the source. Line-start indexing uses checked integer conversions.

The previous constructor cast newline offsets to `u32` without checking the
source length. `ByteSpan` uses `u64`, so candidate spans did not enforce the
assumed limit. Wrapped offsets could invalidate the sorted index used to find
candidate line numbers.

CLI analyzer callers propagate constructor errors as `analyzer.source` failures.
Workspace mutation validation treats an oversized source as an invalid mutation
span. Existing candidate IDs, serialized fields, and supported-source validation
rules remain unchanged. Rust callers of the context constructor must handle its
new `Result` return type.

## Verification

- The extracted line-index routine reproduced both missing length rejection and
  traversal of an oversized source before the fix; both regression tests pass
  with the length check.
- Boundary tests cover empty sources, trailing newlines, `u32::MAX`, and
  `u32::MAX + 1`. They supply lengths and newline offsets to the production
  indexing routine without allocating or reading a 4 GiB fixture.
- `cargo test -p hoimin-core` passed on macOS arm64, including existing candidate
  identity, Unicode/CRLF location, and validation-error precedence tests.
- `cargo test --workspace -- --test-threads=1` passed on macOS arm64.
- `cargo clippy --workspace --lib --bins -- -D warnings` and
  `cargo clippy -p hoimin-core --test candidate_policy -- -D warnings` passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` reported an existing
  `clippy::too_many_lines` error in `lean_report_sequence_oracle.rs:368`.
  The same focused Clippy check reproduced that error on main at `720d8cd`.
