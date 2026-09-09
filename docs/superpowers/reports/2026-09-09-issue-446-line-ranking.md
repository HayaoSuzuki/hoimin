# Issue #446: explicit-line ranking index

## Change

`LineSelectionIndex` groups successfully normalized selector paths by the core logical-path equality key, merges valid inclusive ranges, and finds the preceding range with binary search. Ranking builds one index per call and uses it only for the `ExplicitLine` reason.

Candidate paths remain unnormalized at lookup, matching the prior predicate. Invalid selector paths and inverted ranges do not match; raw line zero and `u32::MAX` retain their prior predicate behavior.

## Tests

- RED: the new core API test initially failed with unresolved `LineSelectionIndex`.
- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo test --offline -p hoimin-core --test line_selection_index` — 3 passed.
- `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo test --offline -p hoimin-cli --lib plan::ranking_tests` — 9 passed.
- `cargo fmt --all`, `git diff --check` — passed.

The core oracle covers merged and sparse ranges, endpoints, zero, `u32::MAX`, aliases, invalid selectors, mismatched paths, Unix case/backslash semantics, and conditional Windows case/separator/simple-uppercase semantics. The ranking regression checks exact reasons, scores, ranks, and ordering for mixed line, file, changed, and symbol selections.

## Self-review

- Equality policy is reused through the core key; CLI does not reproduce platform-specific path handling.
- Memory is bounded by normalized selector metadata, and lookups use a file map plus range binary search.
- The ranking change leaves symbol selection, changed/operator reasons, scoring, and sorting unchanged.

## Review correction

The task review identified two omitted direct-oracle inputs. The core oracle now also queries an empty selector slice and three identical path/range selectors. `cargo test --offline -p hoimin-core --test line_selection_index` passed 4 tests, and the scoped `cargo clippy --offline -p hoimin-core --test line_selection_index -- -D warnings`, formatting, and diff checks passed.

## Controller validation

The controller completed the workspace suite (1,596 passed, 13 ignored), all-features MSRV and Clippy checks, and complete-output release equivalence. The 10,000-candidate many-file benchmark changed from 3912.596 ms to 3.881 ms; the sparse single-file case changed from 3564.736 ms to 1.902 ms.

## Final controller verification

Final code/test commit:ad2ea97 (production1f952a9). Full workspace/all-features on final tree:1597passed,0failed,13ignored across68groups. Final MSRV1.88 and Clippy workspace/all-targets/all-features warnings denied passed. Task review and whole-branch review approved; no unresolved findings. Windows cfg tests were not executed on macOS.

Independent actual CLI comparison covered five mixed selector cases. All complete plan JSON documents matched the immutable main binary, and actual verify --top1 executed the saved top candidate and returned survivor exit1.

## Release performance

macOS arm64, Rust1.98, release, three before/after pairs per condition. Both production ranking modules are included in an independent harness. The before source is exactly main58817cf blob e234d5004e5eca2a429a2669be9d223daf932af3. New code is unchanged by the final test-only fix. Input construction and result serialization are outside timing; index construction, reasons and sorting are inside. Every pair asserts complete serialized ranking equality. No other local builds or suites ran during measurement.

| Candidates/selectors | Distribution | Before median | After median |
|---:|---|---:|---:|
|1,000|one per file|39.431ms|0.382ms|
|1,000|one file, sparse ranges|37.205ms|0.176ms|
|10,000|one per file|3912.596ms|3.881ms|
|10,000|one file, sparse ranges|3564.736ms|1.902ms|

These are synthetic ranking-only timings, not total CLI speedups. The measurement uses every-other-line singleton ranges so the single-file case cannot disappear through adjacency merging.

Logs: `/private/tmp/hoimin-446-workspace-final.log`, `hoimin-446-msrv-final.log`, `hoimin-446-clippy-final.log`, `hoimin-446-cli-final.log`, `hoimin-446-benchmark.log`.

<details>
<summary>Release comparison reproduction</summary>

Create an independent edition2024 Cargo project with camino1, serde1(derive), serde_json1, and hoimin-core pointing at this checkout. Save the old ranking module with `git show 58817cf:crates/hoimin-cli/src/plan/ranking.rs > ranking-before.rs` in the project root. Save the following as src/main.rs and run `HOIMIN_REPO=/absolute/path/to/checkout cargo run --release`.

```rust
#[allow(dead_code)]
#[path = "../ranking-before.rs"]
mod before;
#[allow(dead_code)]
mod after { include!(concat!(env!("HOIMIN_REPO"), "/crates/hoimin-cli/src/plan/ranking.rs")); }
use hoimin_core::{Selection, LineSelection, LineRange, MutationCandidate, ByteSpan};
use std::time::Instant;
fn main() {
 for n in [1000_u32, 10000] {
  for same_file in [false, true] {
   let path = |i| if same_file { "src/shared.py".to_string() } else {format!("src/file{i}.py")};
   let selection=Selection {root:"/project".into(),lines:(0..n).map(|i|LineSelection{path:path(i).into(),range:LineRange{start:2*i+1,end:2*i+1}}).collect(),..Default::default()};
   let candidates=(0..n).map(|i|MutationCandidate{id:format!("{i:064x}"),sequence:u64::from(i)+1,path:path(i).into(),line:2*i+1,column:4,span:ByteSpan{start:4,length:1},original:"+".into(),replacement:"-".into(),operator:"binary_add_sub".into(),symbol:None,file_hash:"0".repeat(64)}).collect::<Vec<_>>();
   let mut old_times=vec![];let mut new_times=vec![];
   for _ in 0..3 {
    let input=candidates.clone();let start=Instant::now();let old=before::rank_candidates(&selection,&[],input);old_times.push(start.elapsed().as_secs_f64()*1000.0);
    let input=candidates.clone();let start=Instant::now();let new=after::rank_candidates(&selection,&[],input);new_times.push(start.elapsed().as_secs_f64()*1000.0);
    assert_eq!(serde_json::to_value(&old).unwrap(),serde_json::to_value(&new).unwrap());
   }
   old_times.sort_by(f64::total_cmp);new_times.sort_by(f64::total_cmp);
   println!("n={n} same_file={same_file} before_ms={:.3} after_ms={:.3}",old_times[1],new_times[1]);
  }
 }
}
```

</details>
