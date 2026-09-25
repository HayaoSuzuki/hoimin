# Issue 599: Verify selection preview

## Intent and scope

Expose the selected candidates of a saved plan before spending time on baseline and mutation tests. `verify PLAN --dry-run` accepts the existing candidate/top/offset/policy selectors and performs exactly the normal plan validation. A successful preview returns exit 0 even for a truncated plan; invalid input returns exit 2 with no preview document. It makes no assertion about mutation outcomes or runtime resource availability.

## Interface and output

Use a boolean `--dry-run` on verify, conflicting with `--metrics`. Preserve the existing default JSON format. JSON and JSONL both emit one compact JSON object followed by a newline; human output prints selector metadata and one candidate per line. A separate command would duplicate selection options; exporting plan rank alone cannot represent diverse ranges. The existing command with an early return is the smallest shared implementation.

The independent preview schema has `schema_version: 1`, `kind: "verify_preview"`, `plan_schema_version: 4`, `ranking_rule_version: 4`, `verification_selection` (the existing mode, policy, requested, selected, scope, plan_truncated object), `offset` (zero-based integer for top, null for explicit IDs), `retained_candidates` (manifest count), and `candidates`. Each candidate contains `id`, `rank` (one-based saved rank), `selection_order` (one-based position within this batch), `path` (plan-relative), and `line` (one-based). The candidates array is in selection order. The report deliberately has a distinct kind and schema from execution reports and cannot serve as progress history.

For top selection, order is the existing resolved ranked-ID vector after policy and offset. For explicit selection, duplicates are removed and order follows analyzer discovery, as normal verify does; neither command-line order, lexical ID order, nor saved rank is used. Reuse the candidate discovery already performed during validation to obtain this order, rather than trusting editable saved sequence numbers. Selection order describes scheduling intent, not parallel completion order or guaranteed execution after time/resource failures.

## Shared preparation and side effects

Extend `VerifiedPlan` with compact preview data assembled inside `prepare_verify_selection_inner`, after header, normalized config, selection limits, source/fingerprint, ranking, copy-manifest validation, descriptor validation and rediscovery all succeed. Return discovered selected IDs from the existing candidate validator; do not add a second selector or discovery pass. For ranked selections retain the selector vector; for explicit IDs retain the validator's discovery order. Resolve row metadata from the same parsed manifest with an ID map.

Both owned CLI dispatch and borrowed `run_with_io` call the same preparation and preview writer. The dry-run branch returns before shell execution, metrics destination handling, baseline, worker materialization, session creation, runtime metrics, and execution events. Read-only source/workspace checks and analyzer rediscovery remain intentional. `--dry-run --metrics` is rejected by argument parsing before either destination can be touched. Preserve ordinary verify execution behavior and report schemas. Correct the touched help's obsolete version-3 plan wording to version 4.

## Validation and evidence

Public binary tests compare preview IDs and order to `mutant_started` events from normal JSONL verify using serial workers. Cover literal strict/diverse orders across tied files and a lower score tier, nonzero offsets, tail clipping, explicit duplicates/reversed arguments, truncated and empty plans, stale source/fingerprint inputs, corrupt candidate/rank/schema data and excessive candidate limits. Check command marker, session, plan bytes and a dedicated TMPDIR snapshot before executing normal verify. Check JSON/JSONL/human and metrics conflicts. Also cover borrowed dispatch and a failing output writer.

Add a standalone JSON Schema and schema validation of real preview output. Update README batching guidance and the OKF selection contract, design/report catalogs and relevant index. No new selector/state machine is introduced, so no independent Lean model is needed. Record three separate reviews each of design, plan, implementation and tests with their actual findings and evidence.

## Review clarifications

Output write failures return exit 2; a partially written document on a broken destination is possible, as with plan output. Successful validation must finish before any preview bytes are written. Public temporary-directory tests pass a dedicated directory through the child process environment, never mutate global test-process environment. Sequence metadata in a plan is not authoritative for explicit execution order: current candidate descriptors exclude sequence, so deriving preview order from it would introduce an observable mismatch.
