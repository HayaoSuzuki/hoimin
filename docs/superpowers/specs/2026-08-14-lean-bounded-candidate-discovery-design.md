# Lean bounded candidate discovery audit design

## Goal

Formally audit candidate discovery under `--max-candidates`, replay Lean-owned
expectations through Rust internal and public observations, and make the
smallest Rust repair only if strict correspondence exposes a mismatch.

The durable claim is:

> For a successful discovery and limit `k`, the retained candidates are the
> first `k` distinct eligible candidates in production order. Truncation is
> reported exactly when that reference sequence contains more than `k`
> candidates. Across ordered targets, the same global limit is shared,
> sequences stay contiguous, and a truncated non-final target finishes the
> spool and produces an explicitly incomplete public plan.

All artifacts live in `.worktrees/lean-bounded-candidate-discovery` on branch
`audit/lean-bounded-candidate-discovery`.

## Design choice

Three approaches were considered:

1. Prove properties of Rust's binary heap and every AST producer directly.
   This offers tight implementation coupling but creates a large, brittle
   model of incidental storage details.
2. Model only `List.take`. This proves the public prefix rule cheaply, but
   would leave producer overflow and cross-target finalization unchecked.
3. Use a compositional model: an unbounded ordered/filter/deduplicate reference,
   a bounded prefix result, producer `k+1` windows, and an ordered target fold.
   Pair its generic prefix proofs with finite three-producer cases and Rust
   internal/public adapters.

Approach 3 is selected. It gives the best cost-to-evidence ratio: universal
proofs cover the semantic contract, while adapters pin the abstractions to the
heap merge, global capacity, spool, and plan behavior that can regress.
The user delegated selection of the recommended design and authorized
implementation and integration without another design checkpoint.

## Correspondence worksheet

| Contract element | Lean representation | Rust site | Observation | Mode |
| --- | --- | --- | --- | --- |
| production order | candidate `orderKey` plus emission index | `RetainedCandidate::cmp`, final sort | ordered candidate roles | `internal-fixture` |
| eligibility before capacity | `eligible` filter in `reference` | `make_candidate` and `retained_by_profile` before `push` | retained roles | `internal-fixture` |
| identity deduplication | stable deduplication by `identity` | `CandidatePrefix.identities`, merged `seen` | unique retained roles | `internal-fixture` |
| producer lookahead | `producerWindow k = take (k+1)` | `CandidatePrefix::new` saturated `k+1` capacity | producer peaks and overflow | `internal-fixture` |
| global prefix and truncation | `bounded reference k` | merge, sort, `truncate(k)`, producer overflow | candidates and `truncated` | `internal-fixture` |
| ordered target capacity | target fold with remaining global capacity | `discover_targets_blocking` and handler store count | roles and global sequences | `internal-fixture` |
| truncated spool completion | terminal result on target truncation | `analyze_and_store` | finished spool, contiguous sequences | `internal-fixture` |
| incomplete public plan | truncated manifest, diagnostic, exit 4 | plan command | JSON manifest, diagnostic, exit | `strict` |
| `usize::MAX` saturation | natural-number boundary case | `saturating_add(1)` | theorem/case only | `model-only` |

An adapter mismatch counts as a Rust bug only when inputs, ordering key,
identity, eligibility, limit, and observation mode all match this worksheet.

## Formal structure

`BoundedCandidateDiscoveryModel` defines candidates, the unbounded reference,
bounded results, producer windows, and target outcomes. Imported proof modules
establish:

- retained candidates equal `reference.take k`;
- retained length is at most `k`;
- truncation is equivalent to `k < reference.length`;
- zero limit retains nothing and still reports truncation for a nonempty
  reference;
- emitted sequences are exactly `1 .. retained.length`;
- target accumulation never exceeds the global limit;
- once a target truncates, the fold is terminal and no later target is read.

The executable owns deterministic JSONL serialization, fixed cases, bounded
enumeration, and sensitivity checks. Imported modules contain no corpus I/O or
exhaustive evaluator.

## Fixed cases and sensitivity

The corpus includes:

- an out-of-order duplicate stream;
- three producer streams whose earliest candidates interleave;
- eligibility rejection before capacity;
- a two-target complete global prefix;
- a non-final target that truncates and finishes a contiguous spool;
- zero limit;
- a `usize::MAX` model-only saturation boundary;
- one strict public-plan case with truncation, diagnostic, and exit 4.

The audit must distinguish implementations that use `k` rather than `k+1`
producer lookahead, emission order rather than production order, consume
capacity before eligibility, count duplicates before deduplication, discard a
globally earlier candidate during merge, ignore producer overflow when the
merged length is exactly `k`, reset capacity per target, continue after a
truncated non-final target, or omit any public incomplete-plan signal.

## Rust adapters and repair policy

A `#[cfg(test)]` adapter in the analyzer module observes `CandidatePrefix` and
the three real producer paths without widening the production API. Integration
tests cover ordered multi-target discovery, finished spool and contiguous
sequence numbers, and the strict public plan contract. The Rust side parses a
closed schema and never recomputes Lean expectations.

If correspondence fails under matching premises, first preserve the Lean
counterexample as a focused failing Rust regression. Then make the minimum
production change and rerun focused and full verification. If correspondence
matches, production behavior is left unchanged.

## Resource and delivery policy

Run one Lean command at a time with a 20-second external deadline where the
platform supports it, a 768 MiB RSS guard where observable, and local theorem
heartbeat limits of 100,000. Resource-monitor failures are infrastructure
errors, not semantic results. Final verification covers corpus freshness,
sensitivity, proof build, focused Rust adapters, workspace tests, formatting,
clippy, and `git diff --check`. The PR closes issue #309 and is merged only
after an independent review and required CI checks pass.
