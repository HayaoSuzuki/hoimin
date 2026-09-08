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
