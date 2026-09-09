# Issue 446: Index explicit line selection for ranking

## Contract

Baseline main: 58817cfa76bcb7ce47e3bd7c1bb1109ee917df19. rank_candidates and validate_ranking_against must preserve complete ranking output while avoiding a candidate-by-selector scan. A candidate receives ExplicitLine iff at least one selector normalizes successfully, its logical path equals candidate.path under current platform rules, and its inclusive interval contains candidate.line.

User authorizes autonomous design, implementation, testing and PR creation in an Issue-specific worktree, with design and plan each reviewed three times. Worktree .worktrees/issue-446-line-ranking, branch perf/issue-446-line-ranking. Implement after #445 PR creation; this branch is independent of #445.

## Alternatives

1. Hoist path normalization but retain any over all selectors: reduces allocation but retains O(N×L), including many intervals in one file. Rejected.
2. Duplicate path canonicalization in CLI and index there: risks diverging from Windows simple-uppercase and separator rules. Rejected.
3. Add a pure core LineSelectionIndex that owns per-logical-path merged intervals, using the existing core path-equality key and range normalization. Construct once per ranking call and query it for each candidate. Selected.

## Architecture

Add LineSelectionIndex to core target policy, with `new(root: &Utf8Path, selections: &[LineSelection]) -> Self` and `contains(path: &Utf8Path, line: u32) -> bool`. This is an additive Rust API, with no serialized representation or dependency change. Internally use a BTreeMap of the existing logical path equality keys to sorted merged inclusive LineRanges. Normalize each selector path once, group intervals, and normalize each group's intervals once. Discard inverted intervals because the old predicate never matches them; retain start=0 behavior for raw API inputs. Ignore failed path normalization as the old ranker does.

For lookup, derive the existing equality key from the candidate path without extra path normalization. Find the last merged interval whose start is <= line using partition_point; membership is line <= end. Range merging must use the existing saturating adjacency rule at u32::MAX. Keep storage proportional to selector metadata and complexity O(L log L + N(log F + log R)), excluding path-byte processing and ranking's existing sort.

In CLI ranking.rs, build the index outside the candidate map and replace only the ExplicitLine predicate. Preserve reason order, scores, rank, candidate IDs and serialized output. Do not use resolved targets.lines, because it loses selector provenance when file/changed/symbol selectors are mixed. The core remains free of filesystem, process and runtime dependencies.

## Validation

Use an independent direct predicate as the compatibility oracle over deterministic mixtures of paths and intervals. Cover duplicates, overlapping/adjacent ranges, gaps, endpoints, inverted ranges, 0 and u32::MAX, invalid paths, absolute/relative aliases, no selectors, missing paths, same-file many ranges and many files. Test Unix case/backslash distinction and Windows case/separator/simple-Unicode equivalence through the existing path policy. Reuse existing plan/verify end-to-end tests and ranking validation; add a mixed-selector ranking regression with the same expected complete output.

Measure the actual old and new ranking modules in release mode, 1,000 and 10,000 candidates, many files and one file with sparse ranges. Include index construction, ranking and sort; exclude fixture construction and serialization comparison. Assert complete serialized output equality for every pair, report median of three pairs, and do not add timing thresholds to CI. Full workspace/all-features, MSRV1.88, Clippy warnings denied, fmt/diff checks and independent reviews precede PR.

## Design self-review 1 — observable equivalence

Compared the proposed predicate to current ranking.rs. Candidate paths must not receive extra normalization: the old code normalizes only selector paths and then applies logical_paths_equal. This distinction preserves direct API behavior for candidate paths containing dot segments. The index must use the same internal core equality key as that function; reusing target.lines would incorrectly attribute boosts after selection composition.

## Design self-review 2 — scale and boundaries

Checked one-file sparse ranges as well as many files: merging alone cannot help sparse ranges, so binary-search membership is required. Existing normalize_ranges uses saturating_add(1), protecting u32::MAX. Empty indexes should return false before allocating a Windows comparison key. Building once per rank_candidates also covers verify without retaining state between manifests.

## Design self-review 3 — integration and evidence

Checked that only ExplicitLine membership changes; selected_symbols, operator reasons and final sorting stay intact. Existing normalize_ranges can be shared without moving unrelated target resolution behavior. The measured old/new comparison includes index construction and both sparse single-file and many-file inputs; output equality is checked after timing. No filesystem dependency or unbounded candidate-sized auxiliary index is introduced. No unresolved design gap remains.
