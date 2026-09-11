# Issue 473: Rank candidates inside explicitly selected symbols

Issue: https://github.com/tokyogas-tech/hoimin/issues/473

## Contract and cause

An explicit symbol selector selects that symbol and its descendants at dot boundaries. The analyzer already accepts `Box.check` for `Box`, while ranking indexes the same file's selected names and checks only exact equality. Thus a selected class method gets no explicit_symbol reason and can rank below an unrelated whole-file candidate. The intended reason contributes exactly 250 points once.

Match a candidate symbol against the exact selected name or any dot-delimited ancestor in the same resolved target file. `Box` matches `Box`, `Box.check` and `Box.Inner.check`, but not `BoxOther.check`. Nested functions follow the same qualified-name rule. Multiple matching parent/child selectors contribute one reason, not one per match. A same-named symbol in another file, or a candidate without a symbol, receives no symbol reason from this file's selectors.

## Implementation design

Retain the existing per-file HashMap of selected-name HashSets. For a candidate with a symbol, check its full name and borrowed dot-delimited ancestor slices against that file's set. This avoids allocation for each ancestor and avoids reintroducing a scan of all selectors for every candidate. Keep existing target path identity, line/changed/operator reasons, deterministic tie ordering, ranking validation and diverse-selection behavior. There is no need for a new public core scope abstraction or unrelated analyzer refactor; use explicit boundary tests to keep the same selection semantics.

## Version compatibility

Advance ranking rule version 3 to 4 because scores and order change. Keep the actual manifest schema version 3 unchanged: the serialized shape does not change. Current README and development text incorrectly call that shape version 2; update those current references to version 3 while explaining ranking version 4.

Saved plans with ranking rule version 3 are rejected before baseline through the existing header-version check, with a diagnostic that tells the user to regenerate the plan. Do not silently reorder or rescore an old manifest. Candidate byte spans and IDs remain unchanged. Historical ranking-v3 designs and audit reports keep their original version statements as historical evidence.

## Verification

Capture a failing rank/score regression and the issue's public two-file example before production changes. Table-test class→method, function→nested function, nested class, exact name, dot-prefix negatives, duplicate parent/child selectors, cross-file names and missing symbols. Assert one explicit_symbol reason worth 250, literal total score and order.

Create a public plan from `a.py:other` and `calc.py:Box.check` with `--source . --symbol calc:Box` and boolean_literal. The selected method must have score 350 and rank 1, while the unrelated function stays score 100. Invoke actual CLI `verify --top 1` and inspect executed candidate ID/path/symbol, successful baseline, and a real killed boolean mutation. Verify a saved schema-3/ranking-3 plan is rejected with regeneration guidance before the test command runs, while the new schema-3/ranking-4 plan executes.

Run existing ranking, selection and Lean candidate-ranking correspondence tests plus full workspace all-features, fmt and all-target/all-feature Clippy. The existing Lean oracle takes explicit_symbol as a boolean input and covers downstream reason/scoring/order; it does not prove this qualified-name resolver. Literal scope-boundary tests and real plan→verify correspondence cover the changed rule, so no new Lean model is needed. Distinguish this scope from historical audit claims.

## Design self-review

1. Semantic boundary: compared analyzer selection with ranking's exact HashSet membership; retained dot boundaries, file identity and one reason per candidate. Prefix-only matching and accumulated parent bonuses would violate the contract.
2. Index and compatibility: retained per-file membership indexing and borrowed ancestors, and separated actual schema3 from ranking3→4. Checked existing header validation and identified stale current docs without rewriting historical evidence.
3. Observable behavior: paired exact unit scores with public plan ordering and actual verify --top execution; required old ranking-version rejection before baseline. Existing downstream Lean boolean-input coverage is not claimed as scope-resolution proof.
