# Pre-implementation oracle consumer contract

> Implementation follow-up: the Rust consumer at `crates/hoimin-core/tests/lean_reduce_spec_oracle.rs` now compares all 952 rows with the production policy as **internal-fixture** cases, including every intermediate trace state. The generated producer metadata remains unchanged; no row is promoted to public strict runner correspondence. See [implementation validation](../../reviews/2026-11-19-issue-1172-reduce.md). The original pre-implementation notes below are historical.

## Artifacts and modes

The sole producer of expected values is `formal/HoiminOracle/ReduceSpecAuditMain.lean`. It generates `formal/HoiminOracle/corpus/reduce-spec.jsonl` with schema `1`. Do not hand-edit the corpus or reproduce its expected predicates in a consumer.

Every row currently has mode `model-only`. There is no `reduce` implementation and no production correspondence test. Running `run` or `verify` against different premises would not promote a case. Do not add an always-skipped test or fake reducer and describe it as a passing adapter.

All current rows have `contract=agreed`, including the accepted numeric, observed-zero-time and deadline clarifications. They remain model-only until implementation correspondence exists. The original specification snapshot remains historical; see `clarifications.md` and `specification-current.md` for the accepted supplement.

## Row protocol

Each row contains `schema`, `mode`, `contract`, `kind`, stable `id`, complete `input`, and Lean-generated `expected`.

| kind | Input | Expected observation | Future comparison boundary |
| --- | --- | --- | --- |
| certification | `context`, `proposal` | certification plus configuration/measurement/component checks and exploration median | Owned reducer decision API; public integration requires a controllable runner |
| pair | rational numerator/denominator, original/reduced ticks | threshold predicate | Exact arithmetic helper; disclose internal-fixture use if not public |
| results | selected IDs and before/after ID-indexed outcomes | exact preservation | Result validation decision API |
| median | exactly three ticks | median | Timing ranking helper |
| overlap | removed and kept half-open spans | nonempty intersection | Source-span guard; AST-to-byte correspondence is an additional obligation |
| trace | context, deadline, ordered logical events | every intermediate state's clock, stop flag, certified best ID, safety | Reducer orchestration with an owned virtual-clock seam, or public trace replay with identical premises |

Opaque identity fields are handles for the source snapshot, plan, command, environment, frozen timeout and exact variant. They are not hashes computed by Lean. The adapter must bind them to actual bytes/configuration and must not accept the same certificate for a different diff. All span coordinates must refer to the same original byte snapshot and be obtained from validated definitions. `baselinePass=true` summarizes successful normal-command executions represented by the evidence, not merely the first attempt.

The input's timing values are arbitrary-precision natural ticks, including one above `u64`. Never silently truncate them. If the production representation cannot configure the same domain, retain the row as model-only or split out the representable portion with explicit premises. A crash or overflow is not automatically a semantic mismatch.

## Observation and outcome rules

1. Parse a row strictly, reject unknown schemas/kinds, preserve integer precision, and create isolated case state.
2. Map its complete premises to the actual implementation entry point. Do not substitute a second implementation of the rule.
3. Normalize stable semantic fields only; compare all expected fields for that case.
4. Classify as `match`, `mismatch`, or `infrastructure error`. Setup errors, runner crashes, resource exhaustion and inability to control the clock belong to infrastructure error or model-only, not mismatch.
5. A fixture-only clock or pure decision helper is `internal-fixture`; `strict` requires the same premises and complete public observations. Promote each slice separately.
6. Preserve the smallest mismatching row, complete intermediate trace, expected/actual fields and reproduction command. Never change Lean to mirror a bug.

## Additional integration obligations

The decision corpus does not prove parsing or physical deletion. Before implementing source transformation, cover async methods, decorators, parametrization as a whole definition, nested definitions, conditional/redefined names, adjacent definitions, empty suites, encoding/newline preservation, overlapping removal ranges, partial reports and cleanup. A fixture left behind after a body-only `pass` edit must not be represented as the certified deletion variant.

Observed zero durations are measurement errors that stop exploration; absent final slots are not observations. The model checks only the declared number of final pairs. Invalid percentage settings are rejected at preflight. No automatic retry or extra control parameter is introduced.

`pairCount` counts completed original/reduced pairs, not individual command executions. A half-completed pair is outside the proposal representation: if either observed half has zero duration or another measurement failure, the adapter must immediately emit a `stop` event even if its partner has not run. It must not hide that observation by keeping `pairCount` unchanged and treating the half as unobserved. Successful halves can wait for their partner, but cannot certify a proposal on their own.

Global budget is an absolute monotonic deadline spanning original evaluation, exploration, repetitions and final confirmation. The model only establishes decision precedence at that boundary. Child termination, cleanup and artifact publication need the repository's resource contracts and implementation tests; Lean's logical clock is not an OS deadline guarantee.

Optimization expectations refer to the best *certified observed exploration median*, with stable old-wins ties. They do not prove the fastest real-world suite, statistical confidence, future fault detection or search completeness. Search must still explore grouped removals; cost is not assumed additive.
