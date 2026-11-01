# Issue 630 progress regression exit review

## Contract and scope

`progress --fail-on-regression` returns 1 only after successful rendering when the existing latest state is `regressing`. Without the option, valid comparisons still return 0. Indeterminate results return 0; input, validation, and output errors retain 2. The comparison algorithm, schemas v1/v2, details selection, and report bytes are unchanged.

Design and implementation plan were committed before implementation in `1c78dfc`. Their three review passes cover latest adjacency versus historical counts, output/error precedence, and bounded Lean correspondence.

## Implementation self-reviews

1. **Decision and failure ordering.** Traced all reads, accumulation, rendering, and the public error-to-2 conversion. The flag is checked only after `render(...)?`; a malformed final report cannot be skipped after an earlier regression. Use `result.latest`, not the last retained comparison: unusable final adjacency intentionally resets latest while earlier comparisons remain. No change was required after this trace.
2. **Output and schema preservation.** Reviewed the renderer and both detail modes. No flag is passed into rendering, serialization, input validation, or comparison, so the public data models and warning order remain unchanged. Split the README explanation into a separate exit-policy paragraph for readability. The corpus schema bump is private to the Lean adapter, not a public result schema change.
3. **CLI and oracle boundaries.** Checked default-false clap parsing and every direct `ProgressArgs` constructor. Extended the existing real strong/weak run test to exercise the actual subprocess flag and byte-identical output. The Lean model proves the exit decision for every abstract latest state and flag/error combination; it does not prove Rust I/O or parsing. Those boundaries are exercised through public process tests and failing writers.

## Test self-reviews

1. **Meaningful RED.** Before production changes, all three new public gate tests failed because the flag was unsupported (actual 2 versus required 1, or missing the intended error message). The deliberately always-zero Lean function failed its regression-equals-1 proof. After implementation, all three tests and the model build passed. Logs are `630-red.log`, `630-green.log`, and `630-lean-red.log` under `/tmp/hoimin-batch-604-632/`.
2. **State and fault matrix.** Reviewed improving, regressing, stalled, saturated, differing candidate IDs with fallback regression counts, recovery, unusable trailing adjacency, and a usable regression after an unusable input. Human/JSON × default/details compare exact stdout and stderr with the ungated invocation. Trailing malformed input and independent stdout/stderr failures establish error-2 precedence. These writer tests exercise write failures; they do not claim a new flushing contract.
3. **Model-to-implementation correspondence.** Added two named histories for recovery and unusable adjacency after regression. The generated schema-3 corpus supplies ordinary, gated, and error exit expectations. The typed consumer observes actual subprocess codes, accepts 0/1 as semantic outcomes, and labels spawn/timeout/unexpected-exit/invalid-JSON failures separately. Every strict case is tested with and without details; malformed suffixes test generated error precedence. Existing aggregate and detail assertions remain. Pure-model duplicate-ID and internally injected inconclusive cases remain explicitly separate from strict report parsing.

## Verification evidence

- Focused Rust: 90 progress tests, 5 Lean consumer tests, and 2 progress heap tests passed.
- Lean: fresh model/native entry builds, generated corpus, freshness, sensitivity, and stats passed using the shared exclusive slot and the 20-second/2-GiB guard. New proofs use 10,000 heartbeats. Across these commands the observed maximum was 5.614 seconds and 739,328 KiB RSS, within the guard limits.
- Corpus: 26 cases (19 strict, 6 internal fixtures, 1 model-only); unchanged bounded domain of 50 reports, 2,500 pairs, and 2,343 history checks. Exit sensitivity distinguishes always-zero, indeterminate failure, historical-regression failure, and error masking.
- Independent read-only review by the parent agent: no blockers. Reviewed post-render/latest-only gating, error precedence, exact output parity, history barriers/recovery, writer faults, Lean general exit proofs and strict public error-suffix correspondence.
- Full workspace: 2,380 passed, 22 ignored across 105 test groups; exit 0. Both exact CI formatting checks and both exact CI clippy commands passed; `git diff --check` passed.

No Python source changed. Python lint is not applicable to this patch; existing Python-backed public run fixtures are included in the Rust suite.

## Final integration

Rebased the unpublished branch from `421f5ef` onto merged main `195773a`. `git range-diff` confirms the design and implementation patches are identical (`7e2cc51` and `6e0f2cf`). The base changes are independent glob-exclusion and diagnostic buffering work; no progress-model integration conflict occurred. After rebasing, all 97 focused progress/oracle/heap tests, both exact CI clippy commands, both formatting checks, and the whitespace check passed. The full workspace result above is from the pre-rebase implementation; the patch-identical integration did not require repeating that suite or Lean generation.
