# Issue #438: Index explicit symbol ranking by path

## Contract

Ranking results must remain identical: candidate identity/content, rank, score, reason order, tie-break order, and validation outcome. Public APIs and ranking-rule version stay unchanged because the ranking rules do not change. The optimization must eliminate per-candidate scans over the entire target array, both for top-level candidates and candidates inside functions, with or without symbol targets.

## Design

Build a borrowed path-to-symbol-set index once per rank_candidates call. Include only target entries with nonempty symbols; union symbols when multiple TargetSlice entries refer to the same path. Match using the existing Utf8Path equality/hash semantics, rather than inventing case folding or converting to strings. For a candidate with a symbol, look up its path and test symbol membership. Candidates without a symbol cannot receive ExplicitSymbol. An empty index entails no per-target search.

Keep the index local to the call and borrow path/symbol strings from targets. No caching across runs, new dependencies, public types, or concurrency state. Existing explicit-line normalization, changed-line boost, operator mapping, and final sort remain as they are. Index construction is expected O(T + S), membership checks expected O(1) (excluding key length), with O(S) additional index storage for S symbol entries; sorting remains O(N log N). Hash iteration order must never influence output order.

Do not gate the optimization on selection.symbols alone: callers provide already resolved TargetSlice.symbols, and existing ranking tests legitimately pass symbol-bearing targets with Selection::default(). Do not index only one TargetSlice per path, since the old existential scan accepts symbols from any duplicate path entry.

## Design self-review 1 — semantic source of truth

Reviewed rank_candidates, validate_ranking_against, and selector reason tests. Raw selection flags are not sufficient evidence for the resolved symbol set. Decision: build from target.symbols and retain identical ordering of reason insertion. No ranking version bump.

## Design self-review 2 — key equality, duplicates, and ownership

Reviewed path comparisons and the fact that targets may contain repeated paths through the internal API. Replacing Utf8Path equality with string identity or case folding could change behavior. Decision: borrowed Utf8Path keys and unioned borrowed symbol sets. No overwrite-last behavior and no cloned source/candidate payloads.

## Design self-review 3 — asymptotics and limited scope

Moving candidate.symbol.is_some() before the old scan fixes only top-level candidates. Decision: indexed membership for every symbol-bearing candidate as well. Avoid unrelated line-selector optimization, which has different path normalization and interval rules. Keep wall-clock assertions out of CI; measure both symbol-free and symbol-bearing workloads and compare complete serialized ranking output with the old implementation.
