# Prepared annotation imports (#605)

A registered import-dependent annotation pair may be emitted only when both root names retain the registered import provenance. Prepared class mappings can supply names absent from the AST. The existing `annotation_import_stable` checks AST writes but misses that mapping. We reuse `may_have_prepared_namespace` rather than interpreting arbitrary metaclass code.

## Policy

In `annotation_import_stable`, preserve the existing lexical class skip and explicit global redirect, then reject a visible prepared namespace before nonlocal routing or local/import ownership. A custom mapping can override even an explicit class import, so those imports remain conservatively excluded. Nonlocal annotation lookup can consult the prepared dictionary before closure bindings. Qualified aliases use the same root-name stability check; a safe global alias may still supply an alternative spelling. Every endpoint is checked independently. Ordinary classes, module annotations, lexical descendants, and both-global declarations retain candidates when existing stability rules allow them. One global endpoint does not certify the other. Function-local unevaluated annotation handling remains unchanged.

This intentionally excludes clean custom mappings, explicit `object` bases and `metaclass=type`, matching the existing class flag. Executing metaclass code or inferring dictionary contents would introduce a second, unsound interpreter; excluding every class would unnecessarily remove ordinary-class positives. Future-string annotation evaluation and arbitrary dynamic module mutation are outside the formal runtime model.

## Formal correspondence

Preserve issue 605's `identity visible injected = !(visible && injected)` and runtime `allowed` predicate without weakening them. Add static `eligible` based on both endpoint visibility and an ordinary namespace premise; prove eligibility implies runtime allowed under that premise. The original 4 contexts × 2 source × 2 destination cases remain 16 distinct inputs, with exact runtime identities. A custom but empty class has runtime allowed=true and static eligibility=false. Soundness does not require emission in every runtime-safe case.

| Premise / observation | Lean | Public setup / observation | Mode |
| --- | --- | --- | --- |
| Endpoint class visibility | two Booleans | module, class, method annotation, lexical nested function; CPython identity | strict |
| Endpoint override | two Booleans | prepared dictionary or mapping; evaluated annotation identity | strict |
| Ordinary namespace | Boolean | plain class vs header | strict |
| Runtime permission | allowed | source/destination identity plus candidate implication | strict |
| Static eligibility | eligible | public plan count and exact target pair/span | strict |
| False kill exclusion | runCheck | original integer-destination example, baseline plus public run JSON | strict |
| Broken one-sided / class-capturing policies | fixed witnesses | Lean only | model-only |

Add ordinary/global/nonlocal/import/qualified-alias controls. Models and proofs belong in an imported module; serialization stays in a dedicated generator. Add corpus freshness and sensitivity to existing guarded CI. External Lean limits: one global command at a time, 20 seconds, 2048 MiB, each theorem heartbeat 10000. No transitions or trace search. Atomicity and idempotency do not apply to pure name lookup.

## Verification and review

Observe unit and public correspondence RED before production changes; retain baseline identity probes even when a plan has no candidates. Then run focused tests, full workspace, fmt, exact CI clippy flags, guarded model/generator/freshness/sensitivity. Record three actual self-review passes each for design, plan, implementation and tests; root performs independent review and publication.
