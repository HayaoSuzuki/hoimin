# Issue #438 ranking index

`rank_candidates` now builds a local borrowed `Utf8Path` to symbol-set index from resolved target slices. Entries for repeated paths are unioned, empty symbol lists are skipped, and candidate lookup preserves the prior exact path equality and ranking-reason order.

The saved baseline at `/private/tmp/hoimin-audit2-ranking.log`, measured against base `730e68f`, recorded 10,000 candidates and 10,000 targets in 2369.921 ms, compared with 0.844 ms for the empty-target control. Both runs had identical ranking output. Those are pre-change measurements, not fixed-code timings.

The eight focused ranking tests cover resolved symbols with `Selection::default()`, duplicate target paths, multiple files, absent candidate symbols, preserved scores/ranks/reasons, and `Utf8Path` equality boundaries. An interior `.` segment matches its normalized equivalent, while `src/A.py` and `src/a.py` remain distinct on every platform. Both `cargo test -p hoimin-cli ranking_tests --lib` and `cargo test -p hoimin-cli --all-features ranking_tests --lib` passed, along with `cargo test -p hoimin-cli plan --lib`, scoped Clippy, and formatting. The independent comparison and broad validation are recorded below.

## Final validation

- Full workspace: `cargo test --offline --workspace --all-features -- --test-threads=1` passed 1,568 tests across 67 result groups; 12 tests were ignored. This run compiled production commit `cd74aa0`. The subsequent `727101d` changed only tests and this report; its eight ranking tests passed both with default features and with all features.
- Latest head: Rust 1.88 `cargo +1.88.0 check --offline --workspace --all-targets --all-features --locked` passed, as did `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`.
- Formatting and `git diff --check` passed.
- Task review identified missing explicit path-equality boundary coverage. Commit `727101d` addressed it, and scoped re-review approved the change. A separate final whole-branch review of `730e68f..727101d` found no required changes.
- Design and implementation plan each contain three completed self-review rounds.

## Release comparison

Measured on macOS arm64 with Rust 1.98, after local Cargo validation stopped. Each case has one candidate per file and the same number of resolved targets. `top_level` has no candidate symbols; `functions` has candidate symbols but no selected target symbols; `selected_symbols` selects the matching function in every file. Selection flags remain default, so resolved targets supply the symbol selection.

Each value is the median of three before/after pairs. Fixture construction, input cloning, and serialization are outside the timed region. The complete serialized ranking outputs were asserted equal after every pair, including scores, reasons, ordering, and candidate fields. The baseline is the exact `ranking.rs` from `730e68f`, Git blob `3b79c24ef02b3e748ee01363dd6868a56a580600`.

| Candidates / targets | Case | Before (ms) | After (ms) |
| --- | --- | ---: | ---: |
| 1,000 | top_level | 26.087 | 0.083 |
| 1,000 | functions | 23.150 | 0.066 |
| 1,000 | selected_symbols | 11.971 | 0.270 |
| 10,000 | top_level | 2316.689 | 0.807 |
| 10,000 | functions | 2283.432 | 0.814 |
| 10,000 | selected_symbols | 1146.906 | 2.459 |

This is a synthetic ranking-only workload, not an end-to-end CLI benchmark. Ten thousand separate files is a large distribution, even though 10,000 candidates fits the default cap. Candidates start in their deterministic output order; the comparison does not characterize all sorting workloads. Measurements are evidence for removing the repeated target scan, not a CI timing threshold or a guarantee for other machines.

### Reproduction

Create a temporary Cargo binary crate with `camino = "1"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, and a path dependency on this checkout's `crates/hoimin-core`. Save `git show 730e68f:crates/hoimin-cli/src/plan/ranking.rs` as `ranking-before.rs` in that temporary crate. Set `HOIMIN_REPO` to the absolute path of the Issue #438 checkout, use the following `src/main.rs`, and run `cargo run --offline --release` with the repository's Rust toolchain.

```rust
#![allow(dead_code)]
use std::{hint::black_box, time::Instant};
use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, MutationCandidate, Selection, TargetSlice};
mod before { include!(concat!(env!("CARGO_MANIFEST_DIR"), "/ranking-before.rs")); }
mod after { include!(concat!(env!("HOIMIN_REPO"), "/crates/hoimin-cli/src/plan/ranking.rs")); }
fn main() {
    for n in [1000, 10000] {
        for mode in ["top_level", "functions", "selected_symbols"] {
            let candidates = (0..n).map(|i| MutationCandidate {
                id: format!("{i:064x}"), sequence: i as u64 + 1,
                path: Utf8PathBuf::from(format!("src/f{i:06}.py")),
                span: ByteSpan { start: 10, length: 1 },
                original: "+".into(), replacement: "-".into(), operator: "binary_add_sub".into(),
                line: 2, column: 4, symbol: (mode != "top_level").then(|| "calculate".into()),
                file_hash: "0".repeat(64),
            }).collect::<Vec<_>>();
            let targets = candidates.iter().map(|c| TargetSlice {
                path: c.path.clone(), lines: vec![],
                symbols: if mode == "selected_symbols" {vec!["calculate".into()]} else {vec![]},
            }).collect::<Vec<_>>();
            let mut old_times=vec![];let mut new_times=vec![];
            for _ in 0..3 {
                let input=candidates.clone();let start=Instant::now();
                let old=before::rank_candidates(&Selection::default(),black_box(&targets),input);
                old_times.push(start.elapsed().as_secs_f64()*1000.0);
                let input=candidates.clone();let start=Instant::now();
                let new=after::rank_candidates(&Selection::default(),black_box(&targets),input);
                new_times.push(start.elapsed().as_secs_f64()*1000.0);
                assert_eq!(serde_json::to_vec(&old).unwrap(),serde_json::to_vec(&new).unwrap());
            }
            old_times.sort_by(f64::total_cmp);new_times.sort_by(f64::total_cmp);
            println!("n={n} mode={mode} before_ms={:.3} after_ms={:.3}",old_times[1],new_times[1]);
        }
    }
}
```
