# Default-enable the two recent analyzer operators

The user requests opt-out behavior for both just-added features. Promote
`method_call_remove` and `function_body_return_constant` into the default runtime
selection (50 -> 52). Keep their existing syntactic/type-annotation guards and
mutation behavior. Each can be disabled with `--exclude-operators`; disabling both
recovers the previous 50-operator set. Explicit `--operators` still overrides all
defaults. Serialized plans retain their saved selection. Historical `all_legacy()`
stays 43, the full inventory stays71, and the other six experimental analyzers stay
opt-in. This is the requested rollout policy, not new evidence of effectiveness.

Default discovery may retain more candidates and reach existing candidate/mutant
limits earlier; selection order and coverage may consequently differ. No limit,
ranking, ID, collector, schema or persistence changes. Existing results remain
subject to the current configuration/session compatibility rules.

## Design self-reviews

1. Scope: the user's opt-out request refers to the two just-created features, not every remaining opt-in operator.
2. Default entrypoints: Rust Default and omitted CLI selectors already share one fallback; add exactly two IDs there.
3. Override precedence: explicit selection remains authoritative; exclusions apply last and can remove either or both new defaults.
4. Compatibility: serialized previous50 and legacy43 plans remain frozen; current defaults must not silently expand them.
5. Cost/claims: extra candidates and earlier caps are expected policy effects. Preserve guards and opt-out; do not infer broad effectiveness from prior limited trials.
