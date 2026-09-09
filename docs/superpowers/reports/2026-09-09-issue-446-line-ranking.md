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
- The ranking change leaves symbol selection, changed/operator reasons, scoring, and sorting unchanged. Controller-owned release comparison and workspace gates remain pending.
