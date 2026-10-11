---
type: Reference
title: Catalog scope and evidence guide
description: Design, audit and implementation relationships, source revisions, evidence levels and maintenance.
status: draft
catalog_revision: a7daea0b557cd435c1e55b540392fbdd116348e1
sources:
- id: readme
  resource: ../usage.md
  revision: 5a45bdc3bd444771c4c57e30fe211f174850ce93
  working_tree: untracked
  sha256: 6894a34743882cc26fdd1f39cb536c94dfc20d5f1dcf33b3cb7f3da9b13e6bef
- id: development
  resource: ../development.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: modified
  sha256: 0cd970d39487cbdcf589949acacfe39f82d9878df459a3078abcd91639faa40b
- id: boundary
  resource: ../superpowers/reports/2026-09-11-boundary-contract-audit/README.md
  working_tree: untracked
  sha256: 6afdc6ecff7fac290da8e43f3d27f5af25718405fcd86e0f493ff51280df1d40
- id: progress
  resource: ../superpowers/reports/2026-09-11-progress-input-lean.md
  revision: a7daea0b557cd435c1e55b540392fbdd116348e1
  working_tree: clean
- id: japanese-style
  resource: https://gist.github.com/k16shikano/fd287c3133457c4fd8f5601d34aa817d/8f2d57610a73efc97d743c9b0b0ecb1002e09fa4
  revision: 8f2d57610a73efc97d743c9b0b0ecb1002e09fa4
- id: workflow
  resource: ../okf-workflow.md
  working_tree: untracked
  sha256: 223711d00bd253408e373e1df7839435363c5756e9e0f37771d675707b5a57f4
---

# Purpose and audience

hoimin is a mutation-testing CLI that makes small changes to Python source and checks whether tests detect them. It generates focused mutation candidates and executes them in isolated working copies. Results are machine-readable, and selected candidates can be verified again.[^readme]

This OKF bundle organizes designs and audits by topic. Start with [designs](design/index.md) for responsibilities and conditions, then read [audits](audits/index.md) for evidence and limitations. Original issue-specific documents remain in place and are listed in the [design catalog](references/design-documents.md) and [audit/report catalog](references/audit-documents.md).

# Coverage

- Summaries: architecture, analyzer, target selection and plan/verify, execution/resources/cleanup, sessions, and output.
- Audits: Lean proofs and implementation comparisons, the July 2026 Rust audit, the September 11 boundary audit, and subsequent progress-input fixes.
- Source catalogs: every Markdown file under `docs/superpowers/specs/`, `docs/superpowers/reports/`, and `docs/audits/`. Only major sources are summarized; catalog entries identify files and their first headings.
- `docs/superpowers/plans/` retains historical implementation procedures in place. Follow the corresponding design for details.

# Revisions and evidence

Created on 2026-09-11. `catalog_revision` records the repository commit consulted when organizing the catalog. Each historical audit identifies its target through `audit_revision` or the body text.

`sources[].revision` identifies the source revision read; `working_tree` records its tracking state at that time. Untracked or modified sources also record content SHA-256. These fields, `catalog_revision`, and `audit_revision` are local extensions, not standard OKF fields.

The September 11 boundary-audit files were untracked when this catalog was created. Links worked in that working tree but not in a clone of the referenced commit. Sharing requires including those sources or replacing references with shared material.[^boundary]

| Evidence label | Meaning |
| --- | --- |
| Adopted in a design | A decision at the time; not necessarily implemented or current behavior |
| Statically inspected implementation | Relevant code was read at the referenced commit; execution success is not implied |
| Execution confirmed in the original report | Historical results limited to the recorded revision, environment, and inputs |
| Proof within a model | A property under the Lean model's assumptions |
| Implementation comparison | An adapter applied a corpus of inputs and expected outcomes to the implementation and compared specified observations |

Read Lean proofs and Rust comparisons within their respective scopes.[^development] Distinguish dates even for reports about the same feature. For example, progress inputs were raised in the September 11 audit and later fixes and comparisons were recorded separately.[^boundary][^progress]

# Initial organization and Japanese prose review (2026-09-11)

Sources and related implementation were read, and OKF structure, sources, footnotes, and links were checked. Rust/Python/Lean were not rerun, and current GitHub Issue state was not checked. Concepts retained `status: draft` because content verification was incomplete; historical test success or prose review alone did not justify `verified`.

The following results came from PyYAML 6.0.3 and local structure/reference checks after the Japanese prose review, before integrating the development procedure. Counts describe that point in time, not the continuously maintained total. No official validator was used.

| Check | Result |
| --- | --- |
| OKF scope | 12 concepts and 4 indexes, totaling 16 Markdown files |
| YAML and reserved-file structure | All passed |
| Sources and footnotes | IDs/footnotes checked for 306 source entries; content correspondence also checked for 305 local sources |
| Local links | All 606 referenced targets existed |
| Source catalogs | 160 designs and 91 audit reports matched the target directories' Markdown sets |
| Navigation | All 16 pages reachable from the entry point |
| Untracked sources | SHA-256 values of 8 source files matched their content at creation |

Link checks covered that working tree. Writing guidance was separately retrieved from a public Gist with its revision recorded. External URL availability and every link inside original sources were not checked.

For continuing development, follow the [OKF workflow](../okf-workflow.md): consult relevant concepts and sources before work, update contracts/decisions/verification scope in the same change, and add new designs/reports to source catalogs.[^workflow]

Relative links outside the bundle assume the same repository. Sharing the bundle also requires the linked source material. Search-service indexing needs separate configuration.

# Historical Japanese prose review (2026-09-11)

Bodies, headings, and index descriptions were checked using Japanese technical-writing guidance at revision `8f2d57610a73efc97d743c9b0b0ecb1002e09fa4`. Review covered definition order, paragraph topics, preservation of speculation and unverified conditions, translated metaphors, and excessive omission.[^japanese-style]

The main changes defined unexplained implementation terms in Japanese and separated design decisions, static inspection, and historical execution results into different paragraphs. References to root clarified whether they meant a process or directory, and the Lean corpus and comparison-test roles were explained. Code identifiers, source revisions, audited commits, and quoted source headings were preserved.

YAML and references were checked again, with results recorded above. One of the 306 sources was the writing guidance added for that review. Current operational guides are now maintained in English; this section preserves the historical review record.

[^readme]: [Usage guide](../usage.md).
[^development]: [Development guide](../development.md).
[^boundary]: [Boundary audit README](../superpowers/reports/2026-09-11-boundary-contract-audit/README.md).
[^progress]: [Progress-input follow-up](../superpowers/reports/2026-09-11-progress-input-lean.md).
[^japanese-style]: [Japanese technical-writing guidance](https://gist.github.com/k16shikano/fd287c3133457c4fd8f5601d34aa817d/8f2d57610a73efc97d743c9b0b0ecb1002e09fa4).
[^workflow]: [OKF development workflow](../okf-workflow.md).
