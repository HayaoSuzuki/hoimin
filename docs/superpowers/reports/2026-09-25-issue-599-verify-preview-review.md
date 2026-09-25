# Issue 599: Verify preview review and verification

## Design reviews before implementation

1. **Selection semantics:** traced `resolve_verify_selection`, `validate_requested_candidates`, shell candidate selection and core `CandidateLoaded`. Explicit selection uses discovery order while ranked selection uses its resolved vector. A saved-sequence sort would be wrong because descriptor validation excludes sequence. The design uses IDs from the existing validation rediscovery and requires explicit order parity tests.
2. **Side-effect and output boundaries:** read both dispatch paths, workspace validation-manifest construction, and metrics argument conversion. Added explicit writer-failure exit 2 and validation-before-output rules; retained read-only validation and rejected metrics at parsing. JSONL now explicitly means one preview object, avoiding confusion with run events.
3. **Compatibility and scope:** checked actual plan/ranking versions and `VerifiedPlan` construction sites (`rg` found only its production constructor). Kept independent preview schema 1, plan/ranking 4 and normal execution behavior; corrected the proposed help wording. Confirmed truncation success does not claim complete mutation execution. No additional selector or Lean state model is introduced.

## Plan reviews before implementation

1. **Spec coverage:** mapped every output/side-effect requirement to the two tasks. Added explicit saved-sequence tampering and borrowed writer failure to review focus, and parity checks against serial `mutant_started` order rather than completion order.
2. **Interface and feasibility:** inspected internal validator's `Result<(), PlanError>` and both dispatches. Its return can carry discovered IDs without changing error precedence; the plan explicitly places collection after validation. Repository search found no JSON Schema helper, so corrected the plan to check Python validator availability and report its limits.
3. **Isolation and completeness:** scanned for placeholders (no matches), checked that all edits live in the issue worktree, and checked preview metadata can be projected from the already parsed manifest. Added per-child TMPDIR instead of process-global environment changes and prohibited a manifest reread. Design/plan are committed before code edits.

## Execution evidence

Design and implementation plan prepared before production or test implementation. Implementation and test review results will be appended as the work is performed; no success is asserted in advance.
