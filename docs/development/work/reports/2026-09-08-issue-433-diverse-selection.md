# Issue #433: diverse selection report

## Change

Within each equal-score tier, selection now moves nonempty per-file groups through an active `VecDeque`. Each selected candidate removes the front group, takes its next candidate, and requeues that group only when it still has candidates. This keeps first file appearance, within-file order, score precedence, limits, and strict selection unchanged.

## Queue-work evidence

A temporary, release-mode copy of the supplied standalone harness counted group visits. It used real ranking and selection code; the counter was kept only in `/private/tmp/hoimin-selection-perf-task1`.

| Distribution | Before visits | After visits |
| --- | ---: | ---: |
| 16 singleton files, 32 dense-file candidates | 544 | 48 |
| 1,000 singleton files, 1,000 dense-file candidates | 1,001,000 | 2,000 |

The old implementation visited `(singletons + 1) * dense` groups. The active queue visits one group for each diverse candidate selected.

## Validation

- Baseline: `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo test -p hoimin-cli selection_tests` — 5 selection tests passed.
- Old temporary counter: `CARGO_TARGET_DIR=/private/tmp/hoimin-selection-perf-task1/target cargo run --release --bin count` in the copied harness — 544 visits for 16/32.
- New temporary counter: the same command after copying the replacement and adding a temporary pop counter — 48 visits for 16/32.
- Focused tests after the change: `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo test -p hoimin-cli selection_tests` — 7 selection tests passed.
- Formatting: `cargo fmt --check`.
- Scoped lint: `CARGO_TARGET_DIR=/private/tmp/hoimin-python-operator-target cargo clippy -p hoimin-cli --lib --tests -- -D warnings`.
- Diff whitespace: `git diff --check`.

## Independent release measurements

A standalone harness included the repository's actual ranking and selection modules. It generated `binary_add_sub` candidates with default selection, verified every score was 70 and called `validate_ranking`. Singleton files sorted before `z_dense.py`. Only selection (including grouping and ID cloning) was timed; construction and ranking were excluded. Values below are the median of three release-mode calls on macOS arm64 / Rust 1.98. The after harness compared every selected ID and its position with the original implementation for both policies on all four distributions; all matched.

| Singleton files | Dense-file candidates | Diverse before (ms) | Diverse after (ms) |
| ---: | ---: | ---: | ---: |
| 1,000 | 9,000 | 4.852 | 0.735 |
| 1,000 | 99,000 | 35.517 | 8.852 |
| 5,000 | 95,000 | 135.563 | 9.303 |
| 10,000 | 90,000 | 247.782 | 9.776 |

These synthetic cases establish the benefit when files have uneven candidate counts. The default policy is strict and the default candidate cap is 10,000, so the larger results do not describe ordinary default runs. Timing is informational; the counted queue visits establish the removed repeated work without depending on scheduling or allocator timing.

Local measurement artifacts: `/private/tmp/hoimin-selection-perf-project/src/main.rs` (before), `/private/tmp/hoimin-433-after-bench/src/main.rs` (after and output equivalence), and `/private/tmp/hoimin-433-{before,after}-bench.log`.

## Workspace compatibility

- `cargo +1.88 check --offline --workspace --all-targets --all-features --locked` — passed.
- `cargo test --offline --workspace --all-features -- --test-threads=1` — 1,557 passed, 0 failed, 12 ignored (66 result groups).
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` — passed.
- `cargo fmt --all --check` and `git diff --check` — passed.
