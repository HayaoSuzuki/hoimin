# Independent review of #1029 and #1030

Reviewer: separate Codex agent `/root/review_1028_1030`, 2026-10-20.
Base: `1f7feae33fa310aae87fbadb390105f6d770439d`; reviewed the working-tree
diff and new files after the workers restored all temporary sensitivity changes.
No Critical, Important or Minor actionable findings remained.

The review traced optional-call admission to the existing candidate start/scope
predicate and checked that the guard affects only the optional-keyword branch.
It inspected selected child and method-name regressions, the deliberately failing
over-broad-return control, and the restored 495-pass/3-ignored analyzer result.
All three measured production hashes matched the final sources.

For visibility, the review checked the class-key index against the direct-Class
condition in `binding_id`, including importer context, ordering, cancellation
and output limits. It inspected the independent exhaustive oracle, operation
counters and public before/after comparisons. The benchmark review checked
fixture and output work outside measurement, cumulative/retained/peak meanings,
raw result/table agreement, and source/link artifact metadata. The temporary
cache's extra peak bytes and small-input timing increase remain explicit limits.

A provenance completion note requested the visibility link-time sidecars; the
worker added them under `receipts/` before the final review. No product fix was
required. The reviewer ran no builds or tests, avoiding interference with serial
measurements; the parent owns final whole-workspace validation.

This review does not re-audit unchanged algorithms, establish new formal proofs,
or establish performance on Linux or Windows.


The reviewer subsequently checked the parent's test-only Clippy corrections.
`writeln!` preserves the generated fixture bytes and empty-length assertions
preserve the same expectations. The three measured production hashes still
match. No additional actionable finding remained after this follow-up.
