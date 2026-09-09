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

## Final independent validation

- `cargo test --offline --workspace --all-features -- --test-threads=1`: 1,582 passed, zero failed, 13 ignored across 67 result groups.
- `cargo +1.88 check --offline --workspace --all-targets --all-features --locked`: passed.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: passed. Formatting and diff checks passed.
- Independent task review approved spec compliance and code quality. A separate final whole-branch review of `bb07b3d..99e32e3` approved the change without required fixes.
- Design and plan each contain three completed self-review rounds.

## Final release comparison

Measured on macOS arm64 with Rust 1.98, after local Cargo verification finished. Each value is the median of three before/after pairs using the exact baseline `store.rs` from `bb07b3d` (blob `52d6c8f820b2b6f31468dce9824d216bdfcc31d2`) and final production source at `99e32e3`.

| Records | Before (ms) | After (ms) |
| --- | ---: | ---: |
| 1,000 | 98.557 | 6.009 |
| 10,000 | 977.721 | 24.042 |

Every pair asserted complete spool-byte equality. Timings include store creation, all pushes, and finish's flush/sync/keep; fixture generation, byte comparison, and file removal are outside timing. A representative baseline record made 94 logical Writer calls including newline. This counts Rust Writer calls, not an OS syscall trace. The new implementation passes a whole record to one write_all invocation, which still handles short writes correctly.

This is a synthetic spool-writing benchmark, not a whole-CLI speed ratio. Parsing, candidate execution, and replay costs are excluded. There is no timing threshold in CI. The extra buffer is bounded by one accepted record (at most 2 MiB); records are not retained across pushes.

### Reproduction

Create a temporary Cargo binary crate with a path dependency on this checkout's `crates/hoimin-core` plus `camino = "1"`, `serde_json = "1"`, `tempfile = "3"`, and `thiserror = "2"`. Save `git show bb07b3d:crates/hoimin-cli/src/analyzer/store.rs` as `store-before.rs` at the temporary crate root. Use the following `src/main.rs`, set `HOIMIN_REPO` to the absolute Issue #442 checkout path, and run `cargo run --release` with Rust 1.98. The measurement used tempfile 3.27.0.

```rust

#![allow(dead_code)]
use std::{io::Write, time::Instant};
use hoimin_core::{ByteSpan, MutationCandidate};
mod before { include!("../store-before.rs"); }
mod after { include!(concat!(env!("HOIMIN_REPO"), "/crates/hoimin-cli/src/analyzer/store.rs")); }
#[derive(Default)]
struct Writes(usize);
impl Write for Writes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> { self.0+=1; Ok(bytes.len()) }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
fn main() {
    for n in [1_000,10_000] {
        let candidates=(1..=n).map(|i| MutationCandidate {
            id:format!("{i:064x}"),sequence:i,path:"src/calculation.py".into(),
            span:ByteSpan{start:10,length:1}, original:"+".into(),replacement:"-".into(),
            operator:"binary_add_sub".into(),line:2,column:4,symbol:Some("calculate".into()),file_hash:"0".repeat(64),
        }).collect::<Vec<_>>();
        let mut writes=Writes::default();
        serde_json::to_writer(&mut writes,&candidates[0]).unwrap();
        writes.write_all(b"\n").unwrap();
        let mut old_times=vec![];let mut new_times=vec![];
        for _ in 0..3 {
            let start=Instant::now();
            let mut old=before::CandidateStore::new(n).unwrap();
            for c in &candidates {old.push(c).unwrap();}
            let old=old.finish().unwrap();
            old_times.push(start.elapsed().as_secs_f64()*1000.0);
            let start=Instant::now();
            let mut new=after::CandidateStore::new(n).unwrap();
            for c in &candidates {new.push(c).unwrap();}
            let new=new.finish().unwrap();
            new_times.push(start.elapsed().as_secs_f64()*1000.0);
            assert_eq!(std::fs::read(&old.token).unwrap(),std::fs::read(&new.token).unwrap());
            std::fs::remove_file(old.token).unwrap();std::fs::remove_file(new.token).unwrap();
        }
        old_times.sort_by(f64::total_cmp);new_times.sort_by(f64::total_cmp);
        println!("n={n} writes_per_record={} before_ms={:.3} after_ms={:.3}",writes.0,old_times[1],new_times[1]);
    }
}
```
