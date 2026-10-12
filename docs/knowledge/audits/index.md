# Audits and verification scope

These summaries help maintainers and agents find historical results and their
limits. Check the target revision, environment, inputs and unresolved conditions
before using a result as evidence for a change. A test's existence, a model proof
and an actual host execution are different evidence; see the
[evidence guide](../overview.md#revisions-and-evidence).

Read the relevant summary and follow its original sources:

- [Reading Lean evidence](lean-evidence.md) - Model proofs, finite search, Rust comparisons and host execution.
- [2026-09-11 boundary audit](boundary-2026-09.md) - Syntax, selection, execution and persistence findings, including static and unverified scope.
- [Progress-input validation and follow-up](progress-input.md) - Later fixes, 368 historical observations and changed comparison-test assumptions.
- [July 2026 Rust audit](rust-2026-07.md) - Original findings, quality checks and unverified host behavior.
- [Performance by input shape](performance-shapes.md) - Execution gates, release measurements, dependent PRs and evidence limits.
- [Cross-boundary contract fixtures](boundary-contracts.md) - Strict execution, complete reporting and unverified conditions.
- [Implicit exceptions and finally](implicit-finally.md) - Typing-derived merges, small Lean models and public-plan comparisons.
- [September 2026 analysis regression integration](analysis-2026-09.md) - Original evidence, fix PRs and formal tests for #545–#549.
- [Kani feasibility investigation](../../superpowers/reports/2026-10-11-issue-778-kani.md) - One bounded Rust budget-contract experiment; adoption deferred after timeout/OOM.

# Related material

- [Catalog entry point](../index.md) - Overall scope and reading order.
- [Catalog task routes](../index.md#start-with-your-task) - Current operational guides and contracts.
- [Original audit/report catalog](../references/audit-documents.md) - Original documents and recorded source revisions.
