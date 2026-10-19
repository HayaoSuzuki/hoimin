# Six-issue execution and review record

This record accompanies the [implementation plan](2026-10-19-priority-six.md)
from base `1e6200d71237c6e445f337babf02f5f0354b610f`. The issue branch is
`issue-1003-safe-reads-1002-escape-1004-config-1015-cleanup-1010-status-1005-cycles`.
All seven task gates passed. The whole-branch review of `4254936c` found two
Important defects; the single fix wave at `fcb11b94` addressed both, and its
scoped re-review found no new or out-of-scope findings. No product finding remains
open. PR #1023 is published; the reviewed CI policy correction below awaits
controller publication and CI monitoring, together with the approved fuzz
correction below.

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

## CI policy correction after publication

PR [#1023](https://github.com/HayaoSuzuki/hoimin/pull/1023) was first published at
`2712291daf85d91ddbff2b166b23ba16fbab6924`. Its initial dependency audit rejected
the four 0.14.0 shipping crates because their own-crate license exceptions still
ended before 0.14.0. The [policy report](2026-10-19-priority-six-evidence/ci-policy-report.md)
and [approved scoped review](2026-10-19-priority-six-evidence/ci-policy-review.md)
record the four-bound correction at `060b69d7536d40c84b3396cad84e7d2e2ecc3894`.
No workflow, product, test, build input or tool pin changed. The report retains
pinned cargo-deny 0.20.2 root/fuzz RED (exit 4) and GREEN (exit 0) commands;
[small raw logs and commit receipt](2026-10-19-priority-six-evidence/ci-policy/commit-receipt.json)
are archived beside the [source-correspondence receipt](2026-10-19-priority-six-evidence/ci-policy/scope-receipt.json).
The original hosted failure is identified there by run URL and full-log hash.

These three additional five-pass sections bring the indexed minimum to 135;
the original 105 and final-fix 15 entries above remain unchanged:

| CI policy stage | Five passes |
| --- | --- |
| Requirements/design | [Five passes](2026-10-19-priority-six-evidence/ci-policy-report.md#requirements-and-design-review) |
| Implementation | [Five passes](2026-10-19-priority-six-evidence/ci-policy-report.md#implementation-review) |
| Validation | [Five passes](2026-10-19-priority-six-evidence/ci-policy-report.md#validation-review) |

R15 permits reuse of the once-dispatched native Windows/macOS run
[39243290107](https://github.com/HayaoSuzuki/hoimin/actions/runs/39243290107)
at exact head `2712291daf85d91ddbff2b166b23ba16fbab6924` only with complete
source correspondence: the later diff contains solely `deny.toml` policy bounds
and this evidence/companion update. This is not a native execution at `060b69d7`
or the subsequent archive head. Any runtime, test or build-input change would
invalidate reuse. Results remain controller-monitored; no new native success
claim is made here, and no suite was rerun for archival work.

## CI fuzz-oracle correction and native correspondence

The initial Linux CI failure was an oracle no-op: the malformed-status mutation
wrote Error over an already correct Error value. The [fuzz report](2026-10-19-priority-six-evidence/ci-fuzz-report.md)
and [approved scoped review](2026-10-19-priority-six-evidence/ci-fuzz-review.md)
record correction `be67c007d539842ad0e796e26eb2f54df18cab4e`, changing only
`report_sequence.rs` and preserving the exact three-byte regression seed.
Production decoding remains unchanged. The [focused evidence index](2026-10-19-priority-six-evidence/ci-fuzz/index.json)
links original Linux failure provenance, actual native macOS RED/GREEN replay,
57 separate deterministic input executions, a bounded 10,000-run ASan smoke,
119 passing core status tests, exact command receipts and decisive log excerpts.
Source hashes/commits identify tested snapshots; no binary or corpus is copied.

| CI fuzz stage | Five passes |
| --- | --- |
| Requirements/design | [Five passes](2026-10-19-priority-six-evidence/ci-fuzz-report.md#five-requirementsdesign-reviews) |
| Implementation | [Five passes](2026-10-19-priority-six-evidence/ci-fuzz-report.md#five-implementation-reviews) |
| Validation | [Five passes](2026-10-19-priority-six-evidence/ci-fuzz-report.md#five-validation-reviews) |

These 15 passes bring the indexed minimum to 150. R16 retains two disclosed
limitations: manifest-wide fuzz formatting fails on byte-identical baseline
`python_analyzer.rs` import ordering, and macOS atos emits symbolizer warnings.
Changed-target formatting/clippy and actual replay pass. Normal root commit
hooks skip fuzz files by configured exclusion; that skip is not target validation.

The controller [rechecked native-source correspondence](2026-10-19-priority-six-evidence/ci-fuzz/native-source-correspondence.json)
from `2712291daf85d91ddbff2b166b23ba16fbab6924` to `be67c007` for R15.
The complete diff consists of license policy, plan evidence/companion, and the
separate fuzz target/seed. Native workflow, root crates, tests, tools, manifests
and locks are byte-identical. Native jobs do not compile this separate fuzz
target or consume its seed; fake orchestration tests copy the unchanged fuzz
manifest and inspect argument strings. This subsequent archival change is docs
only. Native execution remains attributed exclusively to `2712291`, never to
`be67c007` or the archive commit.

At this archive checkpoint, eight native jobs passed and Windows Rust remains
running. Build run [39243267946](https://github.com/HayaoSuzuki/hoimin/actions/runs/39243267946)
succeeded for all three platform wheels/archives and packaged smoke at the
original head. New automatic Linux/fuzz/policy CI has not yet been dispatched.
These observations do not establish all native jobs passed or later-head success.

## Controller followup

All reviews, correction evidence and sixteen rulings are archived. Push the
reviewed policy, fuzz and evidence commits once, then monitor new automatic PR CI
and the existing exact-head R12 native run. Report actual checked heads, run URLs
and outcomes in the PR body and final response. Do not create a post-CI docs
commit merely to record that the previous head passed. No suite rerun, push,
CI dispatch or cleanup was performed by this archival followup.
