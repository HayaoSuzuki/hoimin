# Issue 630: opt-in latest-regression exit status

Add `hoimin progress --fail-on-regression` for CI callers who want a completed progress result and a failing exit status when its latest state is regressing. Keep default success status 0. With the option, return 1 only for `latest.state == regressing`; improving, stalled, saturated and indeterminate remain 0. Document indeterminate explicitly; do not add a separate fail-on-indeterminate option in this bounded change.

## Execution and compatibility

Parse the boolean with existing clap arguments and carry it in ProgressArgs. Continue reading and validating every input, updating the existing accumulator and rendering the complete chosen output. Only after successful rendering compute `i32::from(fail_on_regression && result.latest == ProgressState::Regressing)`. Existing read/parse/validation/render errors remain ProgressError, mapped by the public entry point to exit 2. A regression never short-circuits later input validation or warning/output delivery.

Use `result.latest`, never the last stored comparison or any historical regression. This preserves barriers and recovery: killed→survived→killed exits 0; killed→survived→unusable exits 0; unusable→killed→survived exits 1. Candidate-set mismatch remains indeterminate even when content fallback counts a regression.

The flag changes no output bytes, schema v1/v2 selection, diagnostics, details, score, counts, stalls or saturation. It works with human/JSON and optional details. No new output field or file, dependency, resource limit, or comparison allocation is required.

## Model and testing

Extend the existing Lean progress decision oracle with the exit decision, default preservation, only-regression and error-precedence lemmas (10,000 heartbeats). Add two named history cases for recovery after regression and an unusable tail after regression, preserving the existing bounded domain. Generate versioned expectations for ordinary and opt-in exits, then observe actual public CLI exit codes and unchanged documents. Unit/integration fault writers establish the real render-error precedence excluded from the pure model.

Use the exclusive global Lean slot with the repository 20-second/2-GiB guard; no parallel Lean. The model does not prove parsing, writer behavior or subprocess cleanup. Existing aggregate/detail expectations remain authoritative and unchanged except for the additional histories.

## Design review passes

1. Latest-state review: the last comparison can precede an unusable tail; use the accumulator's latest state and add both recovery/barrier witnesses. Do not infer regression from an aggregate count when candidate sets differ.
2. Error/output review: computing or returning the gate during traversal would hide a later malformed report, and returning before rendering could hide write errors. Place the decision strictly after the existing renderer and retain the public error2 mapping.
3. Scope/schema review: indeterminate needs an explicit policy, not a misleading regression failure. Keep it0, document jq for callers who want broader gating, preserve all output schemas/bytes and avoid an unrequested extra flag.
