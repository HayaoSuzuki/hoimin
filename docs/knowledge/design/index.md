# Designs and contracts

These topic summaries explain responsibilities and link to implementation and
historical sources. Start with the [current task guide](../index.md#start-with-your-task)
for operational commands, and check the recorded source revisions before changing
a contract. A design or model proof alone does not establish current execution
success; see the [evidence guide](../overview.md#revisions-and-evidence).

The contracts cover these boundaries:

- [Isolation and state transitions](architecture.md) - C4 views, source-access boundaries, test support, mutations in copies and pure state decisions.
- [Python analysis and candidate scale](analyzer.md) - Syntax, name resolution, exception hierarchies, byte ranges and bounded retention.
- [Target selection, plan and verify](selection-plan-verify.md) - Discovery versus execution, ranking, subsets, sampling, previews and saved-format changes.
- [Disk failure payloads](disk-failure.md) - Required stop evidence, exact deduplication, report projection, and saved failure format.
- [Resources, cleanup and final output](runtime-lifecycle.md) - OS-specific limits, typed process identity, and process, workspace and report completion conditions.
- [Session ownership and report validation](session-report.md) - Persistence, structural decoding, consumer usability policy and JSON diagnostics.
- [JSON mutant record writing](json-report-spool.md) - Batched writes, acknowledgement, short writes and poisoned output state.
- [Fingerprint input glob traversal](fingerprint-inputs.md) - Shared traversal, input-order errors and unselected files.
- [Survivor assessments versus execution outcomes](survivor-review.md) - Candidate identity, review-owned input acceptance, JSON presentation and historical evidence.
- [Collection annotation references](annotation-builtins.md) - Bidirectional concrete types, deferred evaluation and scope checks.

# Related material

- [Catalog entry point](../index.md) - Task routes, playbooks and original source catalogs.
- [Audit summaries](../audits/index.md) - Recorded evidence and unverified conditions.
