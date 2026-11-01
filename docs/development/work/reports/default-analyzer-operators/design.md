# Default analyzer operators

Promote seven merged analyzers to the default runtime selection: `statement_delete`,
`integer_literal_neighbor`, `augmented_to_assignment`, `return_tuple_swap`,
`string_literal_empty`, `while_condition_false`, `conversion_call_remove`.
The user selected this option on 2026-10-07. Defaults increase from 43 to 50;
the registry remains 69 IDs. Each can be disabled with `--exclude-operators`.

These seven have narrow syntactic targets and bounded per-site alternatives.
They reuse the existing guards and selection pipeline. This is a rollout policy,
not a claim that their mutations preserve runtime types or behavior.
Condition constants and whole-function erasure remain opt-in because their coarse
mutations overlap finer edits. Clause/container deletion remain opt-in because
candidate fanout grows with operand/element count. Enum/optional-keyword mutation
remain opt-in because they require additional module binding/signature analysis.
The existing type, risky exception and exception hierarchy defaults are unchanged.

`MutationOperatorSelection::default()` owns the new set. Raw CLI normalization
must use it when no operator selectors are supplied. `all_legacy()` stays frozen
at the historical 43 IDs. Explicit selectors replace the default set; exclusions
apply last. Saved plans retain their serialized operator sets without expansion.
No candidate collector, family selector, plan schema or mutation ordering changes.
New default runs may produce more candidates, take longer, and retain a different
prefix under candidate limits. Existing session compatibility checks still apply.
Users can opt out of all seven to recover the historical selection.

Lean proves an abstract selection model: default union, explicit override,
exclusion precedence, and persistence identity. It does not prove Rust execution,
parser behavior or serialized plan validation. Rust/public CLI tests separately
check correspondence, including saved-plan rediscovery.

## Design self-reviews

1. Scope: checked all 13 analyzers; chose seven with bounded per-site edits and recorded why six stay opt-in.
2. Configuration entrypoints: found the independent RawRunConfig fallback; both entrypoints must use the same default.
3. Compatibility: froze all_legacy and explicit family expansion; saved plans must not adopt new defaults.
4. Resource behavior: candidate counts and capped prefixes may change; document the cost and retain opt-out.
5. Proof boundary: selection algebra is model-only; public CLI and persisted-plan tests cover the implementation gap.
