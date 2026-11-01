# Lean Nested Match Exit Audit Design

Date: 2026-08-13
Issue: #300, phase 2

## Outcome and boundary

Audit this phase-specific claim:

> A `match` nested in an exception handler produces only its reachable
> fallthrough, break, continue, and terminate exits. Handler-target cleanup is
> applied once to every such exit, `finally` preserves or replaces each exit
> category, and the surrounding loop consumes break and continue only at the
> loop boundary.

This phase composes the existing `BindingFlow.Exits` and
`NestedTryFlow.composeTry` semantics. It does not model Python syntax, runtime
exception selection, arbitrary loop convergence, or pattern-binding details.
Multiple handler selection belongs to phase 3, compound patterns to phase 4,
and `except*` to phase 5.

## Recommended architecture

Add a focused `NestedMatchExit` Lean module rather than enlarging the phase-1
case file. `composeMatch` meets reachable fallthrough paths and concatenates
abrupt paths by category. `consumeLoop` meets the zero-iteration, body
fallthrough, and continue paths before `else`, joins body breaks with exits
that complete the loop, and propagates only terminate exits outward. General
category and reachable-meet results live in an imported proof module; fixed
cases, broken variants, JSONL rendering, and statistics remain separate.

The Rust adapter observes the production `AnnotationCollector`: post-try
categorized exits through the existing test-only try projection, loop-head
facts through a marker-qualified extension of the existing fixed-point
projection, and complete public CLI candidates through `run_with_io`. The
adapter never reimplements transfer rules. Test-only mutations flatten nested
match exits or omit the continue back edge so the same fixtures demonstrate
sensitivity. Production semantics change only after a same-premise failure.

## Correspondence worksheet

| Row | Boundary | Mode | Production observation |
|---|---|---|---|
| `handler_match_break_continue_categories` | Two reachable match arms leave one handler by break and continue | `internal-fixture` | Complete post-`finally` exit snapshot of the containing production `try` |
| `handler_match_fallthrough_terminate_categories` | Match arms leave one handler by fallthrough, return, and raise | `internal-fixture` | Complete post-`finally` exit snapshot of the containing production `try` |
| `irrefutable_terminate_excludes_later_break` | An irrefutable terminating arm makes the later break unreachable | `internal-fixture` | Complete post-`finally` exit snapshot; no break state may appear |
| `nested_continue_reaches_loop_head` | A nested continue participates in the loop-head meet | `internal-fixture` | Production fixed-point head at one unique marker |
| `nested_continue_suppresses_public_candidate` | The same loop-head premise suppresses a `Sequence[int]` replacement | `strict` | Complete public candidate observation: count, path, operator, original, replacement, symbol |
| `post_loop_meets_break_and_natural_exit` | The loop consumes match break/continue and exposes only reachable post-loop fallthrough | `strict` | Complete public candidate observation after the loop |

No phase-2 row is `model-only`: each fixed premise has an owned production
observation. Parse failure, marker ambiguity, timeout, RSS stop, panic, and
malformed corpus are `infrastructure-error`, never a semantic mismatch.

## Formal obligations and sensitivity

The proof module establishes, with explicit premises:

- `composeMatch` preserves every abrupt state in its original category;
- an absent/unreachable match path does not participate in fallthrough meet;
- the outgoing environment is the meet of exactly `Exits.states`;
- handler cleanup maps every nested match exit once before `finally` routing;
- `consumeLoop` includes continue states in the natural-entry meet, consumes
  break and continue, and propagates terminate states.

Fixed broken variants must be distinguished for:

- flattening a nested match break, continue, or terminate into fallthrough;
- retaining an unreachable arm in the join;
- omitting a reachable abrupt arm;
- applying handler cleanup at the wrong subtree boundary; and
- omitting a nested continue from the loop-head meet.

The phase-1 skip-`finally` and category-routing witnesses remain authoritative
and are referenced rather than duplicated. Handler selection, pre-pattern and
pre-guard failure, OR-arm definite bindings, and `except*` sibling/remainder
paths are explicitly inapplicable to this phase and remain assigned to phases
3–5.

## Corpus and error contract

Lean owns a small schema-1 JSONL corpus. Every row has one closed identity,
one unique source marker, one of the four approved modes, and an observation
kind that selects a complete try-exit snapshot, loop-head fact set, or public
candidate record. Generated expectations are never hand-edited. A freshness
check must byte-compare the checked-in corpus with Lean output.

Correspondence results are classified as `match`, `mismatch`, or
`infrastructure-error`. Any mismatch report records the smallest source,
expected and actual complete observation, differing fields, Rust/Lean source
locations, impact, and classification. A confirmed production mismatch first
becomes a failing Rust regression using the same corpus row; only then is the
smallest Rust transfer change allowed.

## Resource and verification contract

Every Lean/Lake command runs alone under
`formal/HoiminOracle/tools/lean_resource_guard.py` with a 20-second deadline,
768 MiB root-plus-descendant RSS cap, 250 ms sampling, and `lake -Kjobs=1`.
Non-trivial declarations use bounded `maxHeartbeats`; `maxHeartbeats 0` is
forbidden. This phase uses fixed cases and theorem premises, with generated
depth, alphabet, explored states, and transitions all zero.

Verification order is guarded Lean proofs and broken witnesses, corpus
freshness, focused adapter tests, strict public tests, formatting/linting, and
relevant workspace tests. The final report separates what Lean proved from
what Rust execution observed and records exact commands, elapsed time, peak
RSS, mode counts, sensitivity results, exclusions, and mismatch decisions.
