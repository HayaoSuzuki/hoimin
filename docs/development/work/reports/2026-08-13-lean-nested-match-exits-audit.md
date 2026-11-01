# Lean Nested Match Exit Correspondence Audit

Date: 2026-08-13
Issue: #300, phase 2

## Result

PASS for the repaired model premises. All four implementation-facing internal
fixtures and both strict public cases match the Rust analyzer. Five fixed
sensitivity families reject their deliberate broken variants. No production
semantic mismatch was found, so the analyzer's production transfer rules did
not change; `rust.rs` changes are limited to owned `#[cfg(test)]` observation
and mutation seams.

Lean proves only the reduced compositional model described here. It does not
prove Rust or Python behavior. The correspondence tests separately execute the
production collector and public CLI at the six fixed premises.

## Durable claim and boundary

This phase audits:

> A `match` nested in an exception handler produces only its reachable
> fallthrough, break, continue, and terminate exits. Handler-target cleanup is
> applied once to every exit, `finally` preserves or replaces its category,
> and the surrounding loop consumes break and continue only at the loop
> boundary.

The model reuses `BindingFlow.Exits` and `NestedTryFlow.composeTry`. It adds
match-branch composition, a loop natural-entry meet, and loop-boundary
consumption. Included behavior is categorized nested exits, unreachable arm
exclusion, cleanup-before-finally composition, continue back edges, and
post-loop joins. Excluded behavior is parser correctness, runtime exception
selection, arbitrary fixed-point convergence, multiple handler selection,
compound pattern bindings, and `except*`. Those last three remain issue #300
phases 3–5.

## What Lean established

`NestedMatchExitProofs.lean` contains kernel-checked, bounded-heartbeat proofs:

- `compose_match_preserves_breaks`,
  `compose_match_preserves_continues`, and
  `compose_match_preserves_terminates`: every reachable abrupt branch state
  remains in its original category;
- `unreachable_unmatched_adds_no_exit`: an absent unmatched path contributes
  no fallthrough state;
- `nested_handler_cleanup_precedes_finally`: the nested match handler exits
  are cleaned before they are routed through `finally`;
- `nested_match_outgoing_is_reachable_meet`: outgoing knowledge is exactly
  the meet over the composed reachable exit states;
- `consume_loop_uses_continue_back_edges`: the natural loop entry includes
  zero iteration, body fallthrough, and all continue states; and
- `consume_loop_propagates_only_terminates`: break and continue are consumed
  at the loop boundary while body/else terminate exits propagate.

These theorems are over explicit `Env` and `Exits` premises, not arbitrary
Python ASTs. No axiom, `sorry`, unbounded heartbeat, generated trace search, or
grammar enumeration is used.

## Corpus and correspondence

The Lean-owned schema-1 JSONL corpus has six fixed rows:

| Mode | Rows | Observation | Result |
|---|---:|---|---|
| `internal-fixture` | 3 | Complete post-`finally` categorized exit snapshot from the containing production `try` | 3 matches |
| `internal-fixture` | 1 | Full normalized production loop-head fact set at one unique marker | 1 match |
| `strict` | 2 | Complete public candidate record: count, path, operator, original, replacement, symbol | 2 matches |
| `model-only` | 0 | None required; every phase-2 premise is production-observable | — |

The strict rows both expect absence: one because the nested continue removes
the definite `Sequence` fact at the loop head, and one because the post-loop
environment meets the reachable break and natural-exit paths. The internal
adapter observes production state and does not reproduce the transfer rules.
Schema, mode, observation kind, source, marker uniqueness, field grouping,
duplicate IDs, and closed row identities are validated before comparison.

## Sensitivity

All five fixed families returned `true`:

- flatten nested break/continue/terminate into fallthrough;
- retain an unreachable match arm;
- omit a reachable abrupt arm;
- move handler cleanup outside the handler-to-finally boundary; and
- omit a nested continue from the loop-head meet.

Phase-1 already owns skip-`finally` and basic category-routing witnesses, so
they were not duplicated. Selected/non-selected handlers, pre-pattern and
pre-guard compound failures, OR-arm definite bindings, and `except*`
remainder/sibling behavior are inapplicable here and remain assigned to
phases 3–5.

## Counterexample and mismatch ledger

One initial same-premise comparison failed:

- Row: `handler_match_break_continue_categories`
- Difference: Lean expected the fallthrough state to retain both `Mapping`
  and `Sequence`; production retained only `Sequence`.
- Classification: model defect, not a production bug.
- Cause: the Lean fixture placed the inner `try` at a `knownBoth` entry, but
  the actual `try` is inside a loop fixed point. The continue arm assigns
  `Mapping`, so the next loop head and therefore all reachable inner entries
  meet `Mapping` to unknown.
- Repair: change only the Lean premise to the loop-head `sequenceOnly`
  environment and regenerate the corpus.
- Regression: the corrected row remains in the checked-in corpus and now
  matches the complete production exit snapshot.

No specification ambiguity, infrastructure error affecting semantic results,
or confirmed Rust bug remains. An initial sandboxed guard invocation returned
`monitor_error` because process RSS monitoring was unavailable inside the
sandbox; all retained Lean evidence was rerun successfully with the same
guard limits outside that sandbox boundary.

## Resource evidence

Every retained Lean command ran alone with a 20,000 ms wall deadline, 768 MiB
root-plus-descendant RSS cap, 250 ms sampling, and `-Kjobs=1` for builds.

| Command purpose | Elapsed | Peak RSS | Exit |
|---|---:|---:|---:|
| `lake -Kjobs=1 build HoiminOracle.NestedMatchExitProofs` | 585 ms | 56,528 KiB | 0 |
| `generate_nested_match_exits --sensitivity` | 289 ms | 2,912 KiB | 0 |
| `generate_nested_match_exits --check corpus/nested-match-exits.jsonl` | 309 ms | 3,072 KiB | 0 |
| `generate_nested_match_exits --stats` | 292 ms | 288 KiB | 0 |

The retained domain reports generated depth 0, event alphabet 0, explored
states 0, transitions 0, six fixed cases, and five sensitivity families. No
larger run was attempted or abandoned.

## Exact verification commands

From `formal/HoiminOracle`, each of the following was wrapped separately in:

```text
python3 tools/lean_resource_guard.py --timeout-seconds 20 \
  --rss-limit-mib 768 --sample-ms 250 --stats <stats-file> -- <command>
```

The commands were:

```text
lake -Kjobs=1 build HoiminOracle.NestedMatchExitProofs
lake env generate_nested_match_exits --sensitivity
lake env generate_nested_match_exits --check corpus/nested-match-exits.jsonl
lake env generate_nested_match_exits --stats
```

From the repository root:

```text
cargo test -p hoimin-cli --lib nested_match_exit_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --test lean_nested_match_exit_oracle --no-fail-fast
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff origin/main...HEAD --check
```

All completed with exit code 0. The focused internal suite ran four tests; the
strict suite ran two tests. The full workspace run completed with zero
failures, with only tests explicitly marked ignored omitted.
