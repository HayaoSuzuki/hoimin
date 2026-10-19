# Acceptance and evidence map

Task 7 checkpoint after all six issue-scoped independent gates and the final native integration checks passed; independent integration/branch reviews and CI remain pending. This is a working map; pending cells are not completion claims. Task 7 must reconcile against final commits and actual logs.

| Issue | Required behavior | Current evidence | Gate |
| --- | --- | --- | --- |
| #1003 | Safe source reads for hash and full validation, post-resolution symlink/FIFO races, bounded hash allocation, preserved ID/path/error priority | 451a4fdd; task-1-report.md and task-1-review.md; six deterministic race cases and 1/16/64MiB allocation observations | Passed |
| #1002 | One human-text escape owner; machine bytes unchanged; preview inert; Clap contexts/argv0; explicit baseline LF/TAB exception | 3d9e33d1 + 6d78789e; task-2-report.md, task-2-review.md, task-2-rereview-1.md; hostile argv0 RED10fail/2pass→GREEN12pass, generated cases | Passed after fix1 |
| #1004 | Immutable checked config, checked original-map serde, public preparation/strict review reject before I/O; preserve opaque progress and typed environment | 8720b41a; task-3-report.md and task-3-review.md; six invalid input kinds with filesystem poison controls, mutation compile-fail RED→GREEN, related integration666 | Passed |
| #1015 | Janitor owns truthful bounded evidence/counts; normal holds summarized, failures visible; default/verbose same facts/verdict/exit; partial/absent/unresolved and observed scan omissions explicit | b199e83d + 6ea967d6; task-4-report.md, task-4-review.md, task-4-rereview-1.md; partial deletion RED/GREEN, bounded/root/format tests, Windows-only assertion fixed | Passed after fix1 |
| #1010 | Computed canonical wire status checked/discarded on decode; no mutable duplicate; full escaped human rows; all schema9 readers/writers/spool/reuse paths | b2b01195 + faff6415; task-5-report.md, task-5-review.md, task-5-rereview-1.md; all status cases, strict public controls, real parallel/resume/spool checks,119 separated core cases | Passed after fix1 |
| #1005 | Shared iterative SCC/depth analysis; conservative H256 user-approved; ordinary semantics/cancellation; independent references/counts and release time/heap evidence | 3aaf82b0; task-6-report.md and docs/maintenance/2026-10-19-issue-1005/report.md; meaningful Rust RED5/5,77 related +29 CLI +8 release +8 runner passed; independent references/counts/release time/heap/RSS recorded | Passed |

## Cross-task invariants for final checks

- Root data/execution state has one authority: SourceReader; checked RunConfig; bounded janitor evidence; MutationOutcome. No compatibility bypasses.
- Report/event schema9 is published once with package floor0.14.0; Task4 and Task5 intermediate commits are not separate releases. Plan10/session5/fingerprint13/candidate1/ranking4/IDv1 remain unchanged unless an explicit later ruling states otherwise.
- Human-only escaping is field-local, baseline.output is the only explicit multiline projection, and JSON/JSONL logical values are preserved. New janitor and status rows use the existing helper.
- Task3 output/fingerprint operations preserve only the semantic validator invariant, not filesystem freshness or destination safety. Task4 presentation stays outside saved config.
- No changes to #806, held formal-proof extensions, exit5, report command/text alias, line-selection or visibility-index performance features.
- Native macOS evidence is labeled; Linux/Windows and PR CI are not silently inferred. Production Creusot math/tool pins remain intact.
- Every task report records five distinct design, implementation and validation self-review passes; independent review is an additional gate. One generated behavioral case per new test function, AAA, independent expectations.
- Final archive must retain all controller rulings R1 onward, any reviewer findings and resolutions, exact verification results/limitations, and bounded resource ownership.

## Completed review findings and rulings

Task2 independent findings: external argv0 reached generated Usage; table loops ran independent scenarios in one test. Fix1 set trusted bin_name hoimin and generated separate cases. Scoped re-review approved both with no new breakage. Tasks1 and3 independent reviews found no issues. All unresolved task-external verification limits belong to final integration/CI, not silently accepted test failures.

Task4 independent finding: Windows-only cleanup test used a structured message member on String; fix1 restored String::contains. Native Windows execution stays pending R12. Task5 independent finding: combined projection/acceptance/rejection tests violated single-behavior AAA; fix1 split direct/tagged behaviors and matching/rejection/property cases. Both scoped re-reviews approved, no remaining findings.

Task6 independent review found no Critical/Important/Minor issues and verified measurement provenance, references and final medians; task-6-review.md. Task7 integration and whole-branch review remain separate gates.

## Final native integration checkpoint

Workspace:3,334 passed/33 ignored; core contracts:688/2; CLI contracts:2,183/28; CLI-reference example:7; frozen Python:1,201 passed/11 skipped; performance registry:40 passed. Workspace and CLI-contract totals exclude two successful nested one-test helper summaries in each CLI-library harness segment. Formatting, clippy, Ruff, ty, reference freshness and scoped OKF/coherence checks passed. Exact commands, checked head and logs are in task-7-report.md and durable checks.json.

Task7 scoped independent review:pending. Final whole-branch review:pending. PR/automatic Linux and R12 manual native CI:pending. No post-publication outcome is inferred.
