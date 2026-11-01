# Issue 362 comparable score report

## Scope

This change renames the human progress label from `score` to `comparable score`.
It documents the population used by the comparison scores in the README and the
progress JSON schema. The comparison calculation and JSON field names remain
unchanged.

## Cause

`hoimin progress` calculates `current_score` from common mutants whose results
are conclusive in both adjacent reports. The human renderer labeled that value
`score`, which could be read as the latest report's whole-run mutation score.
The two values differ when a mutant becomes conclusive in the current report.

## Test evidence

The regression fixture gives one mutant a conclusive result in both reports and
changes a second mutant from timeout to killed. The latest report has a whole-run
score of `0.5`; the shared conclusive intersection has a score of `0.0`. The test
checks the rendered `comparable score` label and retains the JSON
`current_score` and `score_delta` assertions.

The test first failed against `score: 0.000000`, then passed after the label
change. Final verification produced these results:

- `cargo test -p hoimin-cli --test progress`: 60 passed.
- `cargo fmt --all -- --check`: exit 0.
- `cargo clippy -p hoimin-cli --all-targets -- -D warnings`: exit 0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo test --workspace -- --test-threads=1`: exit 0.

The workspace Clippy command without `--all-features` reached an existing
`clippy::too_many_lines` error in
`crates/hoimin-core/tests/lean_report_sequence_oracle.rs:368`. This change does
not touch that file.
