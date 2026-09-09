# Issue #442 candidate spool write report

## Task 1 implementation

`CandidateStore::push` retains its serialized-size preflight, then converts the accepted JSON byte count plus newline to a bounded `usize` capacity. It serializes one JSONL record into a local `Vec<u8>` and calls `write_all` once. Counters remain unchanged until that write succeeds. The spool format, size boundary, replay API, and `finish` lifecycle are unchanged.

The tests cover exact JSONL bytes for a complete record and an actual write failure created with a read-only file descriptor passed to `NamedTempFile::from_parts`. That failure returns `StoreError::Io`, preserves both counters and the empty spool, and removes the temporary file when the store drops. Existing property, Unicode, record-size, sequence, cursor, and replay coverage remains in place.

## Performance RED

The pre-change issue benchmark is the RED evidence for this performance defect: a representative record made 94 writer calls, and the 10,000-candidate release workload took 881 ms. Compatibility tests pass before this implementation by design, so no timing threshold or private writer-observation test was added. The controller owns final three-pair release comparisons and exact-byte workload validation.

## Validation

All commands used `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target` and `HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/RustroverProjects/hoimin/.venv/bin/python` where relevant.

| Command | Result |
| --- | --- |
| `cargo test -p hoimin-cli analyzer::store::tests --lib` before change | 4 passed |
| `cargo test -p hoimin-cli --test analyzer_handler candidate_store` before change | 2 passed |
| `cargo test -p hoimin-cli analyzer::store::tests --lib` | 6 passed |
| `cargo test -p hoimin-cli --test analyzer_handler` | 40 passed |
| `cargo test -p hoimin-cli --test lean_bounded_candidate_discovery_oracle` | 4 passed |
| `cargo fmt --check` after formatting | passed |
| `cargo clippy -p hoimin-cli --lib --tests -- -D warnings` | passed |

## Self-review

The record capacity derives only from a count already below the 2 MiB limit, including its newline. The record buffer is local to `push`, so it cannot grow with the candidate set or defer an I/O error. `write_all` remains the sole write after validation; failure occurs before counter advancement. No dependencies, public types, schemas, persistent buffering, retry behavior, or replay logic changed.

Controller follow-up remains: release benchmark comparison and exact-byte workload check, workspace/MSRV/full lint validation, independent reviews, and PR creation.
