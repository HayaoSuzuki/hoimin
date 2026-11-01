# Nested `try` / `finally` Binding-Flow Lean Audit

Date: 2026-08-13  
Branch/worktree: `lean-cost-effective-audit` / `.worktrees/lean-cost-effective-audit`

## Result

The audited Rust analyzer agrees with the Lean oracle on every implementation-facing fixed case: 2 strict public cases and 7 owned internal fixtures. One additional case is model-only. No same-premise production mismatch was found, so no production transfer rule was changed. The Rust change is a `#[cfg(test)]` exit projection and corpus adapters only.

The durable claim is: sequential composition advances only fallthrough; handler-target cleanup is applied to every handler exit before finalizer entry; every reachable exit category passes through the finalizer; a falling finalizer restores the incoming category; an abrupt finalizer replaces it; and the outgoing environment is the meet of all and only the categorized states reachable after this composition.

This claim is explicit in the Lean model. Rust previously encoded the individual transfer operations implicitly in `visit_try`, `apply_finally`, and `route_finally_entry`; this audit adds executable correspondence without changing those production operations.

Excluded: Python exception matching, loop fixed-point convergence, runtime exception values, interprocedural control flow, and equivalence beyond the two modeled import facts (`Sequence`, `Mapping`). Return and raise share Rust's `terminate` category, but have separate source witnesses.

## Lean model and proofs

`composeTry` is the audited boundary. It composes body/`else`, cleaned handler exits, category-wise finalization, reachable-state collection, and `meetAll?`. The retained theorems are:

- `sequential_abrupt_excludes_next`
- `singleton_finally_routes_exactly`
- `falling_finally_preserves_category`
- `abrupt_finally_replaces_category`
- `cleanup_exits_idempotent`
- `compose_try_routes_cleaned_exits`
- `compose_try_outgoing_is_reachable_meet`
- `compose_try_retained_fact_is_common`
- `reachable_meet_retains_only_common_knowledge`

The last five fix cleanup, exact composition, outgoing meet, and retained-fact premises. These are theorems about the finite Lean abstraction, not a proof about arbitrary Rust execution. Rust correspondence is supplied separately by the fixed corpus tests.

No `sorry`, `admit`, new axiom, unlimited heartbeat, or bounded generated search is used. `generated_depth=0` and `event_alphabet=0`; all exploration is by 10 fixed minimal witnesses plus universally quantified theorems.

## Fixed witnesses and correspondence

| ID | Mode | Minimal intermediate/result state | Result |
| --- | --- | --- | --- |
| `finally_annotation_meets_normal_and_raise` | strict | normal and raise reach the annotation with disagreeing `Sequence`; no candidate is sound | match: absent |
| `post_finally_uses_only_fallthrough` | strict | abrupt state does not reach the post-finally statement; fallthrough retains typing `Sequence` | match: one complete `type_list_sequence` candidate |
| `falling_finally_preserves_break` | internal | `break(Mapping)` → finalizer reimports `Sequence` → `break(Mapping, Sequence)` | match |
| `falling_finally_preserves_continue` | internal | `continue(Mapping)` → finalizer → `continue(Mapping, Sequence)` | match |
| `falling_finally_preserves_return_terminate` | internal | `terminate-return(Mapping)` → finalizer → `terminate(Mapping, Sequence)` | match |
| `falling_finally_preserves_raise_terminate` | internal | `terminate-raise(Mapping)` → finalizer → `terminate(Mapping, Sequence)` | match |
| `abrupt_finally_replaces_fallthrough` | internal | fallthrough → raising finalizer → only `terminate(Mapping)` | match |
| `abrupt_finally_replaces_break` | internal | break → raising finalizer → only `terminate(Mapping)` | match |
| `unreachable_post_return_excluded` | internal | `terminate(Mapping)`; post-return reimport never enters the meet | match |
| `nonselected_handler_meet` | model-only | body `Sequence` and handler `Mapping` merge to fallthrough `Mapping` only | Lean-only |

Counts: strict 2, internal-fixture 7, model-only 1, infrastructure-error semantic rows 0. Each of the 9 implementation-facing IDs also passed in isolation using `HOIMIN_NESTED_TRY_CASE`. The strict observation—including count, path, operator, original, replacement, and nullable symbol—is Lean-corpus-owned; Rust only normalizes the public manifest. Every ID has an exact Rust-owned source/marker mapping so premise drift fails closed.

## Sensitivity matrix

All seven reported families returned `true`:

| Family | Broken transition distinguished |
| --- | --- |
| cleanup boundary | cleanup before handler body, omitted cleanup, and cleanup delayed until after a finalizer that observes target presence |
| finalizer coverage | finalizer applied only to fallthrough; break, continue, return, and raise witnesses |
| category preservation | falling finalizer flattened to fallthrough for break, continue, and terminate |
| abrupt replacement | incoming category retained for fallthrough and break inputs |
| unreachable join | unreachable post-terminate state contaminates `composeTry` outgoing meet |
| omitted abrupt | one reachable disagreeing terminate exit omitted from `composeTry` outgoing meet |
| cleanup idempotency | duplicated cleanup invents a fact, with both correct and broken paths observed through `composeTry` |

## Mismatch and defect ledger

No confirmed production bug was found. Consequently there is no Lean counterexample requiring a production Rust regression/fix.

Three authoring/model defects were found during TDD and retained as closed checks:

1. A strict expected fixture accidentally depended on `Mapping`; fixed by the minimal `sequenceOnly` environment and fixed-ID validation.
2. Lean fact rendering order differed from Rust normalization; fixed in the Lean renderer to the stable `Mapping`, then `Sequence` order.
3. Strict markers initially pointed only to comments and could not overlap candidates; fixed to the exact annotation text and guarded by unique-marker/source checks.

Review then found that `composeTry` itself could be broken without failing the original proofs/cases. This was a model-test defect, not a Rust mismatch. A RED consumer (`unknownIdentifier` for the three composition theorems) was followed by the three retained theorems, `composeTry`-based fixed expectations, and composition-boundary sensitivity checks. Review also moved complete strict expectations into the Lean corpus and closed every source premise.

There are no unresolved owner questions within this slice. Corpus freshness remains an explicit guarded Lean gate; the ordinary Rust tests consume but do not regenerate the corpus because the standard Rust CI job does not install Lean.

## Resource evidence

Every retained Lean command ran alone with a 20 s timeout, 768 MiB aggregate RSS cap, 250 ms sampling, and Lake `-Kjobs=1` for builds.

| Check | elapsed ms | peak RSS KiB | exit/reason |
| --- | ---: | ---: | --- |
| model build | 560 | 56,496 | 0 / child_exit |
| proof build | 300 | 2,768 | 0 / child_exit |
| original theorem consumer | 2,165 | 575,888 | 0 / child_exit |
| composition theorem consumer | 567 | 54,496 | 0 / child_exit |
| sensitivity | 583 | 685,488 | 0 / child_exit |
| fixed cases | 573 | 686,320 | 0 / child_exit |
| stats | 586 | 686,000 | 0 / child_exit |
| corpus freshness | 585 | 685,856 | 0 / child_exit |

The first baseline attempt recorded `elapsed_ms=34`, `peak_rss_kib=0`, exit 126, `monitor_error`. Root cause was the sandbox denying the guard's `ps` process-tree sampling; it is infrastructure evidence only. All retained results above were rerun with the required process-monitor permission.

## Reproduction and quality gates

Run from `formal/HoiminOracle` (one at a time):

```bash
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/STAT.json -- lake -Kjobs=1 build HoiminOracle.NestedTryFlowModel
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/STAT.json -- lake -Kjobs=1 build HoiminOracle.NestedTryFlowProofs
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/STAT.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --sensitivity
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/STAT.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --cases
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/STAT.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --stats
python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/STAT.json -- lake env lean --run NestedTryFlowAuditMain.lean -- --check corpus/nested-try-flow.jsonl
```

From the repository worktree root, the final gates are:

```bash
cargo test -p hoimin-cli analyzer::rust::nested_try_oracle_tests -- --nocapture
cargo test -p hoimin-cli --test lean_nested_try_flow_oracle -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 -m unittest tests.test_lean_resource_guard -v
git diff --check
```

Commits before this report: `ddab6c5`, `f805c36`, `2a9726a`, `57903f8`, `d3580e0`. The review-hardening and this report are committed together after final verification.
