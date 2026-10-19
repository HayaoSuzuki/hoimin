# Six-issue execution and review record

This record accompanies the [implementation plan](2026-10-19-priority-six.md)
from base `1e6200d71237c6e445f337babf02f5f0354b610f`. The issue branch is
`issue-1003-safe-reads-1002-escape-1004-config-1015-cleanup-1010-status-1005-cycles`.
All seven task gates passed. The whole-branch review of `4254936c` found two
Important defects; the single fix wave at `fcb11b94` addressed both, and its
scoped re-review found no new or out-of-scope findings. No product finding remains
open. PR publication and native CI remain controller-owned followups.

| Task | Implementation and fixes | Independent gate |
| --- | --- | --- |
| 1, #1003: safe source reads | `451a4fdd` | [Approved](2026-10-19-priority-six-evidence/task-1-review.md) |
| 2, #1002: human escaping | `3d9e33d1`, `6d78789e` | [Initial findings](2026-10-19-priority-six-evidence/task-2-review.md), [fix1 approved](2026-10-19-priority-six-evidence/task-2-rereview-1.md) |
| 3, #1004: checked configuration | `8720b41a` | [Approved](2026-10-19-priority-six-evidence/task-3-review.md) |
| 4, #1015: janitor observations | `b199e83d`, `6ea967d6` | [Initial finding](2026-10-19-priority-six-evidence/task-4-review.md), [fix1 approved](2026-10-19-priority-six-evidence/task-4-rereview-1.md) |
| 5, #1010: derived status and human IDs | `b2b01195`, `faff6415` | [Initial finding](2026-10-19-priority-six-evidence/task-5-review.md), [fix1 approved](2026-10-19-priority-six-evidence/task-5-rereview-1.md) |
| 6, #1005: shared import graph | `3aaf82b0` | [Approved](2026-10-19-priority-six-evidence/task-6-review.md) |
| 7: integration, documentation and archive | `4254936c` | [Approved](2026-10-19-priority-six-evidence/task-7-review.md) |

The [acceptance matrix](2026-10-19-priority-six-evidence/acceptance-matrix.md)
maps each issue to its behavioral evidence. The [issue snapshot](2026-10-19-priority-six-evidence/issue-snapshot.json)
retains the six original bodies and triage text without implying that their
historical reproductions were rerun. Reports retain failed review rounds,
meaningful RED/GREEN results, test setup corrections and platform limitations.

## Self-review index

Each linked section contains five numbered passes. These 21 sections preserve
the minimum 105 self-review entries; additional fix-round passes remain in the
full reports. Independent reviews above are additional gates.

| Task | Requirements/design: five passes | Implementation: five passes | Validation: five passes |
| --- | --- | --- | --- |
| 1 | [Requirements](2026-10-19-priority-six-evidence/task-1-report.md#requirementsdesign-review-before-implementation) | [Implementation](2026-10-19-priority-six-evidence/task-1-report.md#five-implementation-review-passes) | [Validation](2026-10-19-priority-six-evidence/task-1-report.md#five-validation-review-passes) |
| 2 | [Requirements](2026-10-19-priority-six-evidence/task-2-report.md#five-requirementsdesign-review-passes) | [Implementation](2026-10-19-priority-six-evidence/task-2-report.md#five-implementation-review-passes) | [Validation](2026-10-19-priority-six-evidence/task-2-report.md#five-validation-review-passes) |
| 3 | [Requirements](2026-10-19-priority-six-evidence/task-3-report.md#five-requirementsdesign-review-passes) | [Implementation](2026-10-19-priority-six-evidence/task-3-report.md#five-implementation-review-passes) | [Validation](2026-10-19-priority-six-evidence/task-3-report.md#five-validation-review-passes) |
| 4 | [Requirements](2026-10-19-priority-six-evidence/task-4-report.md#five-requirementsdesign-reviews) | [Implementation](2026-10-19-priority-six-evidence/task-4-report.md#five-implementation-reviews) | [Validation](2026-10-19-priority-six-evidence/task-4-report.md#five-validation-reviews) |
| 5 | [Requirements](2026-10-19-priority-six-evidence/task-5-report.md#five-requirementsdesign-reviews) | [Implementation](2026-10-19-priority-six-evidence/task-5-report.md#five-implementation-reviews) | [Validation](2026-10-19-priority-six-evidence/task-5-report.md#five-validation-reviews) |
| 6 | [Design](2026-10-19-priority-six-evidence/task-6-report.md#five-design-self-reviews) | [Implementation](2026-10-19-priority-six-evidence/task-6-report.md#five-implementation-self-reviews) | [Validation](2026-10-19-priority-six-evidence/task-6-report.md#five-validation-self-reviews) |
| 7 | [Requirements](2026-10-19-priority-six-evidence/task-7-report.md#five-requirementsdesign-reviews) | [Implementation](2026-10-19-priority-six-evidence/task-7-report.md#five-implementation-reviews) | [Validation](2026-10-19-priority-six-evidence/task-7-report.md#five-validation-reviews) |

## Decisions, evidence and validation

[Controller rulings](2026-10-19-priority-six-evidence/controller-rulings.md)
preserve the full ruling/reason/cost text, the shared-interface review and the
archive/publication refinement. R4 records the user-approved conservative H256
rule; R12 requires a manual native Windows/macOS CI run after final publication;
R13 preserves and itemizes inherited historical-reference limits. The original
[depth investigation](2026-10-19-priority-six-evidence/depth-facts.md) retains the
earlier model observations separately from Task 6's Rust and release evidence.

R14 corrects the historical Task 4 blanket missing-lease statement for a
successfully inspected, positively young, empty pre-lease staging root. The
[final fix report](2026-10-19-priority-six-evidence/final-fix-report.md#changes-and-correction-to-the-historical-task-4-statement)
contains that correction and current evidence. Original task reports, reviews
and their past claims remain unchanged.

Task 6's [measurement report](../../maintenance/2026-10-19-issue-1005/report.md)
and adjacent reproducible scripts, raw time/heap/RSS results, fixtures and source
provenance already exist in the repository. This archive references and hashes
those files without copying the measurement directory. Its normal CLI timings,
instrumented requested allocations, operation counts and sampled RSS describe
different quantities. Peak requested heap did not decrease. The exact measured
runner snapshot remains distinct from later tool formatting/type changes.

The [Task 7 checkpoint report](2026-10-19-priority-six-evidence/task-7-report.md) records
final native commands/results, current documentation changes, OKF content review
and pending CI. [Check receipts](2026-10-19-priority-six-evidence/checks.json)
record exact argv, checked head, environment, exit and original log hashes.
Selected excerpts retain suite result lines and decisive failures without
duplicating full repetitive logs. A later documentation-only commit does not
relabel those executions as checks at that commit.

The [archive manifest](2026-10-19-priority-six-evidence/archive-manifest.json)
records copied report hashes, selected log origins/excerpt hashes and linked
measurement artifacts. It excludes its own bytes. The complete task-owned logs
remain under `/tmp/hoimin-priority-six-*`; no build/cache/venv or saved executable
is part of the durable archive.

Current reports/events use schema 9 with package floor 0.14.0. Plan 10, session 5,
fingerprint 13, candidate 1, ranking 4, ID v1 and exit policy retain their formats.
The new status is a checked projection of outcome. Runtime verbosity does not
extend saved configuration. Historical notices, source revisions and formal
execution results retain their original scopes.

Local integration runs on macOS with the frozen task venv. It does not establish
native Linux/Windows behavior, a new local proof execution, actual wheel builds
or a fuzz campaign. Historical missing source paths and unavailable commit
objects are itemized in the [OKF receipt](2026-10-19-priority-six-evidence/okf.log),
with baseline-presence checks in the retained checker. No new/changed reference
uses those exceptions. Plans and maintenance records remain outside the
original-source catalogs under `docs/superpowers/specs`, `reports` and `docs/audits`.

## Final whole-branch review and single fix wave

The [initial final review](2026-10-19-priority-six-evidence/final-review.md)
inspected `4254936ca33240e2f69f6d612fbb408d92090c8a` and found two Important
defects: acceptance of the retired disk member and classification of young empty
pre-lease staging roots. Its [retired-field probe](2026-10-19-priority-six-evidence/final-probes/retired-field-receipt.json)
and [staging probe](2026-10-19-priority-six-evidence/final-probes/staging-receipt.json)
retain the existing-binary identity, valid controls and observed behavior.
Initial probe setup failures remain labeled separately from decisive results.

The [single final fix report](2026-10-19-priority-six-evidence/final-fix-report.md)
records commit `fcb11b946e2730f75c54d4408b5132ff6b57e41a` and the R14 correction.
The [scoped re-review](2026-10-19-priority-six-evidence/final-rereview.md) marks
both findings ADDRESSED, with no new breakage or out-of-scope findings.
Its additional stages contain five passes each, bringing the indexed minimum to
120 while preserving the original seven-task/105-pass index above:

| Final fix stage | Five passes |
| --- | --- |
| Requirements/design | [Five passes](2026-10-19-priority-six-evidence/final-fix-report.md#five-requirementsdesign-self-review-passes) |
| Implementation | [Five passes](2026-10-19-priority-six-evidence/final-fix-report.md#five-implementation-self-review-passes) |
| Validation | [Five passes](2026-10-19-priority-six-evidence/final-fix-report.md#five-validation-self-review-passes) |

The [focused evidence index](2026-10-19-priority-six-evidence/final-fix/index.json)
retains nine command receipts, decisive excerpts and three unique tested diff
snapshots, plus the four-scenario staging probe receipt/script. The 46-file source
inventory was checked before selecting these artifacts; repeated build logs and
fixture trees are omitted. The initial owned-test permission-fixture failure
remains in the record with its correction.

Focused GREEN evidence is 527 passed / 3 ignored, four public staging scenarios
and four passed mandatory hooks. Receipts identify base `4254936c` plus tested
working-tree snapshots. The final snapshot hash matches the committed fix at
`fcb11b94`; these are not postcommit reruns. The earlier broad native checks and
16 receipts retain their original head `3aaf82b0` and documented dirty snapshots.
The [Task 7 commit receipt](2026-10-19-priority-six-evidence/task-7-commit-receipt.json)
also remains an unchanged checkpoint record. No broad suite or proof tool was
rerun for this archival followup.

## Controller followup

Task 7, whole-branch and scoped final reviews, their fix evidence and all fourteen
rulings are archived. Publish the issue branch and PR without merging or releasing. Run automatic PR CI and the R12
manual non-Linux workflow on the final head, then record actual head/run URLs and
outcomes in the PR body and final response. Do not create a post-CI documentation
commit merely to record that the previous head passed. No temporary evidence or
SDD workspace has been deleted by Task 7.
