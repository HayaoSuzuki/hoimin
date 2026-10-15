# Canonical workspace handler methods

This record covers #837, stacked above #835. The shared design and plan are in
`docs/superpowers/specs/2026-10-12-philosophy-followup-design.md` and
`docs/superpowers/plans/2026-10-12-philosophy-followup.md`.

## Design self-review

1. Read all six forwarding bodies: they only call corresponding handler methods.
2. Searched crate call and import sites; repository consumers use methods.
3. Checked maintained guides and distributable skills: none instruct callers to use these free functions.
4. Read the original MVP plan: the forwarding interface is historical implementation structure, not an independent safety mechanism.
5. Checked compatibility policy: internal Rust source APIs have no separate stability promise; external absence is not asserted.

## Plan self-review

1. Identified exact function/documentation boundaries, excluding `effect_failed`.
2. Kept handler signatures, implementation, ownership and error mapping unchanged.
3. Selected removal instead of adding deprecation/dispatcher layers to an internal API.
4. Selected existing workspace behavior tests and final all-target compilation.
5. Required an explicit Rust source migration note and unchanged CLI/package version floor.

## Source migration

Call `handler.handle_preflight(request)`, `handler.handle_create_worker(request)`,
`handler.handle_apply_mutation(request)`, `handler.handle_reset_worker(request)`,
`handler.handle_cleanup(request)` or `handler.handle_verify_originals(request)`
instead of the same-named free function with `handler` as its first argument.
Return types, failure IDs, safety validation and operation order are unchanged.
The internal Rust API changes immediately; the supported CLI, saved formats and
package layout do not change, and the version floor remains 0.4.0.

## Implementation self-review

1. Compared the diff with all six original bodies: only forwarding functions and their comments are removed.
2. Checked the neighboring import-root validator and `effect_failed` are unchanged.
3. Rechecked callers after deletion: canonical handler method calls remain.
4. Added no trait, wrapper, dependency or test-only production hook.
5. Distinguished repository search evidence from a claim about external consumers.

## Testing self-review

1. Baseline workspace-handler tests passed: 30 tests.
2. Post-removal workspace-handler tests passed: the same 30 tests.
3. Checked the source diff contains only the six free-function removals (55 lines).
4. Rechecked maintained guides/skills and repository call sites; no migration was needed there.
5. Checked formatting and whitespace; final all-target compilation, strict lint and stack review cover cross-crate consumers.

No transfer-body mirror test was added. OS identity and cleanup contracts remain
in existing handler and owned-workspace tests. The policy, original MVP plan and
runtime lifecycle knowledge were consulted; their historical statements are not
rewritten as current API documentation or fresh proof results.

## Apply request migration (Issue #931)

`WorkspaceHandler::handle_apply_mutation(request)` now takes only `ApplyMutation`.
Put the candidate in `request.candidate`; the synchronous handler and owned task
both apply that candidate. Remove the separate argument and clones needed only
to pass it. Invalid request candidates are rejected while retaining the worker.
The old two-argument signature has no compatibility wrapper. This internal Rust
source change does not alter CLI options, report or saved formats. The
[Issue #931 record](../superpowers/plans/2026-10-16-issue-931.md) documents the
verification scope; the reviews above describe the earlier #837 change.
