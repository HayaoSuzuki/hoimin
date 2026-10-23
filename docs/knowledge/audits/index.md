# Audits and verification scope

These summaries help maintainers and agents find historical results and their
limits. Check the target revision, environment, inputs and unresolved conditions
before using a result as evidence for a change. A test's existence, a model proof
and an actual host execution are different evidence; see the
[evidence guide](../overview.md#revisions-and-evidence).

Read the relevant summary and follow its original sources:

- [Design and resource audit after PR 1093](design-2026-10-23-post1093.md) - Fixed JSON buffer and Tokio follow-up, new type/state/performance candidates declined, session companion defect; audit included with #1094 implementation.
- [Design and resource audit after PR 1090](design-2026-10-23-post1090.md) - JSON record buffer/write tradeoff, retained type/state boundaries and unresolved companion admission; audit included with #1092 implementation.
- [Design and resource audit after PR 1088](design-2026-10-22-post1088.md) - Session companion hardlinks and transient descriptor copies; audit included with #1089 implementation.
- [Design and resource audit after PR 1084](design-2026-10-22-post1084.md) - Reproduced session hardlink source modification, borrowed tokens and review candidates; #1085–#1087, audit included with implementation.
- [Design and resource audit after PR 1080](design-2026-10-22-post1080.md) - Verify membership work, finite mutation reads and borrowed identity; #1081–#1083 and implementation follow-up.
- [Design and resource audit after PR 1076](design-2026-10-22-post1076.md) - Review admission, borrowed history rendering and mutation source work; #1077–#1079 and implementation follow-up in PR #1080.
- [Design and resource audit after PR 1069](design-2026-10-22-post1069.md) - Original manifest read budgets, first-store publication and rejected type/state changes; #1070–#1071.
- [Design and resource audit after PR 1064](design-2026-10-22-post1064.md) - Source scope, finite reads, Windows state and evidence navigation; #1065–#1068 and implementation follow-up.
- [Design and resource audit after PR 1059](design-2026-10-22-post1059.md) - Historical findings, rejected state simplifications and implemented #1060–#1063 follow-ups.
- [October 2026 design-audit history](design-history-2026-10.md) - Eight archived reviews, completed follow-ups and conditions behind retained or later revised decisions.
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
