# Behavioral evidence for documentation changes

This record covers #834, based on main `437e19f`. The design and stack plan are
in `docs/superpowers/specs/2026-10-12-philosophy-followup-design.md` and
`docs/superpowers/plans/2026-10-12-philosophy-followup.md`.

## Guarantee ownership

The removed assertions require exact prose or command spelling; they do not
execute the guarantee described by that prose. The existing evidence remains:

| Removed wording checks | Behavioral or rendering owner |
| --- | --- |
| Python ranked workflow and saved-rank descriptions | Rust plan/verify selection, inherited limits, truncation, ranking validation and preview tests |
| Rust agent-plan workflow descriptions | The same plan/verify tests, including diverse selection, saved-plan immutability, stable batches and fingerprint changes |
| Rust focused-profile wording and synthetic CRLF prose | CLI profile parsing, focused/full execution and session-compatibility tests |
| Report-handler metrics/complete prose assertions | Parsed metrics example, executable Usage run examples, report schemas, metrics destination and incomplete-result tests |

The report-handler tokenizer still joins the two explicit shell continuation
forms; it does not execute environment assignments or redirection. Real-shell
smoke evidence from #833 remains historical evidence at that revision.
Site generation validates links and anchors, and the rendered numbered-step
test protects code placement. No test claims to prove the quality or completeness
of explanatory prose; maintainers review that content.

## Design self-review

1. Traced both duplicate ranked-workflow checks to the same Usage file.
2. Identified the focused-profile test as prose-only; actual profile/session tests remain.
3. Kept executable run examples and their JSON/JSONL schema validation.
4. Kept the renderer regression and strict site checks rather than another parser.
5. Chose parsed configuration to check the metrics example without binding its whitespace.

## Plan self-review

1. Located the two Rust tests and the Python module before deleting them.
2. Compared selection/ranking/truncation descriptions with existing plan behavior tests.
3. Compared metrics/incomplete descriptions with report and destination behavior tests.
4. Selected a temporary missing-separator negative probe against the actual extractor.
5. Limited edits to tests and records; README and production CLI remain unchanged.

## Execution

Baseline Python documentation tests passed: 26 tests; baseline report-handler
tests passed: 33. After deleting the ten Python wording cases, the 16 remaining
documentation tests passed. All 33 report-handler tests passed after the change.
Strict site generation passed.

## Implementation self-review

1. Compared the removed Python module with the ownership table: no executable assertion was removed.
2. Compared the two removed Rust tests: they only loaded and searched prose.
3. Kept command extraction, both continuation regressions and schema checks unchanged.
4. Checked metrics coverage through the parsed `RunConfig`, before the actual example executes.
5. Checked the complete diff for production, Usage and README changes: none.

## Testing self-review

1. Compared the baseline and post-change 33-test report-handler results.
2. Confirmed the remaining 16 Python documentation tests, including rendering, pass.
3. Temporarily removed the first run example's `--`; the actual contract failed at the separator check.
4. Temporarily removed the metrics option from the example; the actual contract failed at the parsed-metrics assertion.
5. Restored original Usage bytes in `finally`, reran the executable contract successfully, and checked strict site generation and whitespace.

The negative probes are task-local scratch, not permanent prose mirror tests.
Whole-stack lint, runtime checks and independent review are recorded separately.
