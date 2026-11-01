# Lean Candidate Ranking Proof Strengthening Design

## Goal

Strengthen the candidate-ranking audit with an unbounded theorem that every ID
returned by diverse top-selection belongs to the saved ranked candidates.

## Scope

The change is proof-only. It does not alter candidate ranking, diverse ordering,
the Rust implementation, corpus records, or public CLI behavior. Existing finite
checks continue to cover output uniqueness, saturated length, and non-increasing
score tiers.

## Proof structure

Prove membership preservation compositionally across the existing functions:

1. `firstForPath?` can return only an element of its candidate input.
2. `selectRound` can return only elements of its candidate input.
3. `eraseSelected` can retain only elements of its candidate input.
4. `roundRobinWithFuel` can emit only elements of its candidate input.
5. `roundRobinTier` inherits the same property.
6. `diverseOrderWithFuel` and `diverseOrder` can emit only elements of the saved
   ranked input.
7. Mapping and truncation yield `diverseSelect_member_of_saved`, the public
   theorem over selected IDs.

Each theorem uses `set_option maxHeartbeats 100000`. Compilation runs as the only
potentially expensive Lean process under a 20-second external deadline. A timeout
or memory symptom stops proof expansion; it is not answered by raising bounds.

## Verification

A small consumer file imports the public proof module and invokes the new theorem.
It must fail before the theorem exists and pass after implementation. The full
Lean library build and candidate audit corpus freshness, statistics, and
sensitivity commands must also pass under the same resource envelope.

## Documentation

Update the existing candidate-ranking audit report to distinguish the new
unbounded origin guarantee from the finite uniqueness, length, and tier-order
checks. Record the focused verification commands and resource behavior.
