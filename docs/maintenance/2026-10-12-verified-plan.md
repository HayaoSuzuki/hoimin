# Verified plan ownership

This record covers #835, stacked above #834. The shared design and plan are in
`docs/superpowers/specs/2026-10-12-philosophy-followup-design.md` and
`docs/superpowers/plans/2026-10-12-philosophy-followup.md`.

## Design self-review

1. Enumerated every production field consumer: CLI preview/metrics and shell execution.
2. Distinguished read-only inspection from the sole supported metrics override.
3. Checked that scope and truncation already belong to verification metadata.
4. Traced manifest protection and copied fingerprint inputs into shell execution.
5. Checked compatibility policy: internal Rust APIs have no separate stability guarantee; CLI and saved formats remain public contracts.

## Plan self-review

1. Selected four shared-reference queries for real inspection needs, without mutable accessors.
2. Chose one owned internal execution payload rather than exporting fingerprint inputs for reconstruction.
3. Chose consuming `with_metrics` and `execute` operations; no configuration setter family.
4. Kept session/resume rejection, execution preparation and output delivery in their current order.
5. Selected a compile-fail contract, metrics-preservation behavior check and existing changed-after-preparation regression.

## Internal Rust source migration

`VerifiedPlan` fields become private. Read configuration, selection, preview and
metadata through shared references. Scope and truncation come from the metadata.
Replace `verified.config.output.metrics = path` with
`verified.with_metrics(path)`. For borrowed writers, consume `verified.execute`
instead of extracting config, IDs and copied fingerprint paths separately.
The CLI dispatch's owned output delivery stays on its existing deadline-aware
path. Metrics destinations still undergo the existing execution-time checks.

This changes internal Rust source APIs, not supported CLI/report/package
contracts. The version floor stays 0.4.0; saved plans need no regeneration solely
for this ownership change. No vulnerability or performance improvement is claimed.

## Implementation self-review

1. Compared owned and borrowed dispatch: preview and metrics override retain their order; borrowed execution now uses the consuming plan operation.
2. Compared the shell handoff field by field: configuration, ordered/explicit IDs, selection metadata, manifest paths and fingerprint-copy inputs are preserved.
3. Removed duplicate scope/truncation fields and the unused duplicate tuple return; serialized metadata construction is unchanged.
4. Checked the four inspection methods return shared references, with no mutable handle or general settings setter.
5. Migrated the late fingerprint-deletion test to execute the checked plan rather than reconstruct its inputs; the runtime session/resume guard remains.

## Testing self-review

1. The public-field compile-fail example initially failed because the code compiled; after field privatization both compile-fail contracts pass.
2. A temporary no-op metrics override caused the new preservation test to fail; byte-for-byte restoration made it pass.
3. All 95 plan tests passed, including late fingerprint deletion, ranking, preview, limits and saved-plan validation.
4. All 19 metrics destination and 56 CLI configuration tests passed.
5. Checked formatting, whitespace and the internal source-migration note; whole-stack lint, contracts-feature execution and final review are recorded separately.

Ruling: the field-protection contract supplied the initial red/green cycle. The
metrics-preservation test was added during migration and its sensitivity was
verified by the temporary no-op probe; it is not described as a test that ran
before the initial implementation. The duplicate tuple return was simplified
after the 95-test run; the final stack run covers that final form.

The compatibility policy and runtime lifecycle knowledge were consulted. Their
CLI and safety guarantees do not change, and historical source metadata is not
refreshed merely because code moved. No formal-model change is required.

The all-target check also identified prepared-plan inspection in the default
operator and manifest-buffer lifetime tests. Both now use shared-reference
queries; their behavioral and raw-buffer release assertions remain intact.

## Follow-up: selected execution APIs

Issue #850 removes the selected/ordered shell wrappers that still accepted
configuration and selection as separate arguments. See the
[execution API record](2026-10-13-verified-execution.md) for the removed APIs,
negative-test placement, migration and current verification. The results above
remain evidence for the original #835 change.
