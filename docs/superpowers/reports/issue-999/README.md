# Issue #999 durable evidence archive

Start with the [adoption report](../2026-10-19-issue-999-creusot-adoption.md).
`records/` preserves the task briefs, reports, independent reviews, controller
progress and integration handoffs. `records/progress.md` preserves every
controller `Ruling:` and its recorded cost; earlier failed attempts retain their
original classification. These copies preserve historical wording, including
then-pending checks and original scratch paths. The adoption report gives the
current disposition.

`evidence/task-N.tar.gz` preserves the actual logs, summaries, generated proof
artifacts and measurement sidecars for that task. Extract into a separate
directory with `tar -xzf evidence/task-N.tar.gz -C DESTINATION`; its top-level
`task-N/` maps to the original task evidence directory. Task 2 maps to the
original `evidence/task-2/`. Task 4 includes native warm/cold acceptance and the
failed cold/Windows attempts, separate from local ARM development runs.
`SHA256SUMS` covers archived files. Historical absolute runtime paths in logs
identify the original environment; they need not exist on the reader's machine.

The archive contains execution evidence, not a reusable solver-answer cache.
Use current sources and the [proof guide](../../../../formal/creusot/README.md)
to run fresh verification. Review generated content and applicable upstream
licenses before repackaging proof artifacts; see
[licensing](../../../licensing.md#creusot-verification-dependencies).

`evidence/task-5-final-ci.tar.gz` preserves the final hosted CI collection under
`task-5-final-ci/`: current-source native warm/cold artifacts, full job logs,
run metadata, and failed and repaired native platform runs. It complements the
earlier Task 5 snapshots rather than replacing them. `records/task-5-fix1-review.md`
retains its then-pending native Windows finding; the adoption report records the
latest disposition. The progress ledger preserves all seven controller Rulings.

`evidence/task-5-final-validation.tar.gz` contains the focused documentation/OKF
logs and preparation/validation scripts under `task-5-final-validation/`. The
[final acceptance report](records/task-5-final-acceptance-report.md) records their
results and the final native acceptance, with whole-branch review still pending.
