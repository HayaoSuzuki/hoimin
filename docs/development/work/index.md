# Development records

These records retain the scope, revision, environment and results of individual
tasks. Use the [development guides](../index.md) for maintained procedures and the
[knowledge catalog](../knowledge/index.md) for current explanations and evidence boundaries.

| Category | Records |
| --- | --- |
| Designs | [Original designs](designs/) and the [design catalog](../knowledge/references/design-documents.md) |
| Implementation plans | [Plans](plans/) |
| Reviews | [Reviews](reviews/) |
| Verification and investigations | [Reports](reports/) and the [audit/report catalog](../knowledge/references/audit-documents.md) |
| Code and behavioral audits | [Audits](audits/) |
| Issue and CI maintenance | [Maintenance records](maintenance/) |
| Dated performance evidence | [Measurements](performance/) |
| Documentation reorganization | [Layout and verification plan](documentation-layout/plan.md) |

For new work, keep a task's design, plan, review and durable evidence together in
`docs/development/work/<issue-or-topic>/`. The category directories above retain
existing records and their cross-references. Add current conclusions to the owning
guide or knowledge concept; do not require users to read a dated record to use hoimin.

Raw measurement outputs and receipts retain their recorded contents. Paths inside
those outputs describe the original execution. Markdown links point to the current
location; recorded revisions and hashes continue to identify the historical evidence.
Use Git history with rename following to find a source's path at that revision.
