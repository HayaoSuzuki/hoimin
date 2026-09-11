# Validate mutant results before progress comparison

Issue: https://github.com/tokyogas-tech/hoimin/issues/460

A saved report can say `killed` and `Exit(0)` while its counts remain consistent. Progress currently accepts it, so a self-comparison can report saturation. The report writer already rejects this combination through `ReportSequence`; progress validates document structure independently because saved reports do not contain `mutant_started` events.

## Contract and design

Extract the existing single-result checks into `MutantFinished::validate_result`, returning the existing `ReportSequenceError`. Validate output-state required fields, matching output-close diagnostics, and then status against `classify_mutant_result` when termination exists. Preserve this precedence and the existing null-termination compatibility. `ReportSequence` calls this method only after its lifecycle checks. Both schema-v2 and current progress document paths call the same method before producing comparison input. Errors retain the input path and candidate identity; invalid input exits 2 and emits no progress JSON.

This does not impose lifecycle events on saved reports, change classification policy, alter schemas, or infer missing termination values. A complete output with absent termination remains allowed. A close timeout requires its existing fields and diagnostic and legitimately produces `error`, even when the child exited successfully.

## Verification boundary

Extend the existing ProgressInput Lean model and corpus with finite single-mutant status/termination/output-state equivalence classes. Keep the existing summary and baseline cases. Generate expectations from Lean, then exercise real `hoimin progress` for schemas 2 and 3. Include ignored-result-validation and ignored-output-state broken witnesses. The model establishes a semantic contract; the CLI adapter establishes only exercised correspondence, not a proof of Rust code. Diagnostics payload integrity stays covered by direct Rust tests.

## Design self-review

1. Acceptance coverage: both input formats, optional termination, nonzero exit, resource and timeout outcomes, and output-close override are part of the shared contract.
2. Boundary review: document validation must run before history comparison; existing lifecycle error precedence stays intact; no synthetic started events are introduced.
3. Compatibility and scope: reuse existing result errors and classifier; preserve legacy null termination and all existing Lean summary cases. No unrelated report policy changes.
