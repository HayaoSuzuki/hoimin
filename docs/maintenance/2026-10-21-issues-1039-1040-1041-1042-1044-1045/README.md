# Performance fixes for issues #1039, #1040, #1041, #1042, #1044 and #1045

## Scope and invariants

Base: `a892eee0` (main). These six issues authorize internal optimizations with unchanged candidate identity, order, diagnostics, truncation, input validation, resource limits and cleanup ownership. The issue bodies are the acceptance specification:

- #1039: extract private visible-alias roots once per collector that visits classes; preserve mangling and exclusions.
- #1040: resolve qualified annotation spellings by their first identifier; preserve unqualified alias ordering and allowed-name checks.
- #1041: build preview payloads only for dry runs after the same validation; ordinary execution retains no display-only payload.
- #1042: reuse traversal depth when sorting worker entries; preserve depth/native-path order and no-follow restoration.
- #1044: intersect related exception identities with module visibility without visiting every invisible sibling at each site; preserve eligibility reasons and cancellation.
- #1045: skip ring-spool seeks only when the cursor is known to be aligned; preserve wrap, pending-write errors, bounded drain and first-failure ownership.

Issue URLs use `https://github.com/HayaoSuzuki/hoimin/issues/NUMBER`.

## Resource and measurement policy

One worktree and one build process at a time; reuse `/Users/hayao/RustroverProjects/hoimin/target`, set `CARGO_BUILD_JOBS=2`, and share the existing Python environment. Keep benchmark fixtures and logs under this plan's ignored scratch directory; retain only compact measurements and reproduction instructions in Git. Check disk use between build phases. Never clean another task's target or the shared environment.

Regression gates use real production operation counts and allocator observations, not millisecond thresholds. Compare release binaries built from the same locked dependencies; retain only the baseline executable rather than a second build tree. Measure small inputs and independently varied issue axes, validate output equality, and distinguish allocator heap from RSS, stage timings from whole CLI timings, and calculated counts from measured counts. No smaller candidate limits or reduced validation count as optimizations.

## Design

Use a lazy per-collector set of borrowed private alias roots for #1039. For #1040 use the first qualified identifier to select its single module binding. For #1044 keep one constructor-compatible count per existing child group and choose the smaller of visible identities and adjacent identities at each site; retain no all-pairs cache.

Separate execution preparation from preview preparation with a shared internal checked result for #1041. A normal `VerifiedPlan` carries only execution inputs; a dedicated preview preparation entry point constructs display data from the same checked selection. Update consumers directly, without a compatibility wrapper that recreates discarded payload.

Store the already computed depth in `WorkerEntry` for #1042. Keep the output ring algorithm for #1045, making its private file boundary measurable and retaining seeks for zero logical position, skipped offsets and within-chunk wraps.

## Evidence

Implementation, RED/GREEN results, operation counts, heap observations, release comparisons and final validation will be recorded here as they are obtained. No new performance or platform claims are established by this initial specification.
