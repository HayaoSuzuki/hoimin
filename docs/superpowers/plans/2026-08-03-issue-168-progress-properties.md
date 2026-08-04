# Issue #168 Progress Comparison Property Coverage

## Scope

This change resolves [#168](https://github.com/tokyogas-tech/hoimin/issues/168) by exercising the public `compare_reports` boundary from the CLI integration suite. It adds no production test hooks and keeps generated reports, the stable-ID oracle, and permutations in `crates/hoimin-cli/tests/progress.rs`.

## Independent model

The generator assigns a unique stable candidate ID to every mutant and selects statuses from all seven `MutationStatus` variants. Every non-empty report has a conclusive candidate. Matching-ID-set reports with at least two candidates deliberately give different IDs the same path, original, replacement, operator, and symbol; separate collision-free cases rotate one ID so added and removed counts are non-zero.

The test oracle independently joins `BTreeMap<candidate_id, status>` values. It counts common, added, removed, inconclusive, improvement, regression, and carried-survivor outcomes and calculates scores directly. It does not call `compare_reports`, `candidate_set_eligibility`, or production score/state helpers.

## Coverage map

| Contract | Integration property |
| --- | --- |
| Self-comparison has no transitions and is stalled, or indeterminate when empty | `compare_property_self_comparison_is_stable` |
| Reversing reports swaps improvements/regressions and added/removed, and negates the optional score delta | `compare_property_reversal_is_antisymmetric_and_order_independent` |
| Independent before/after permutations preserve the complete `Comparison` | `compare_property_reversal_is_antisymmetric_and_order_independent` |
| A killed-to-survived stable ID remains one regression when another ID has identical content | `compare_property_killed_to_survived_uses_id_despite_duplicate_content` |

## TDD evidence

After adding the properties, three temporary production mutations demonstrated that they detect the intended breaks:

1. Replacing matching-set candidate-ID indexing with content-key indexing made all three properties fail, including the duplicate-content regression (`common: 1` instead of `2`, and the regression was lost).
2. Counting `Killed -> Survived` as an improvement made the reversal property and duplicate-content regression fail (`improvements: 1`, `regressions: 0`).
3. Counting an after-only candidate as removed made the reversal/oracle property fail (`added: 0`, `removed: 2` instead of one each).

All three mutations and the generated proptest regression files were removed before GREEN verification.

## Verification

Run:

```text
cargo test -p hoimin-cli --test progress compare_property
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```
