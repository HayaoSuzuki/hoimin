# Issue #432: bound progress history memory

## Contract

Keep decoded mutant data for at most the adjacent reports being compared. Retain only small per-input dispositions and per-comparison results for final rendering. Parsing a single report may still allocate its full byte buffer and typed document; this change does not introduce a streaming JSON parser or a fixed global heap limit.

Preserve public `read_report`, `compare_reports`, `InputReport`, `UsableReport`, `ProgressResult` and `Comparison` APIs. Preserve JSON schema 1, human fields, error codes, unusable-gap reset behavior, candidate identity eligibility, diagnostic ordering, and no partial success output or warnings when a later input fails to parse. Rust MSRV remains 1.88. Add no dependencies and no source comments unless required by safety/lint contracts.

## Design

Read report paths sequentially. Derive a compact disposition for rendering immediately; compare current report with the preceding report; release the older report before loading the next. Cache candidate-set eligibility per usable adjacent pair so rendering does not need mutant data. Use one shared comparison accumulator for both the existing slice-based public compare_reports function and the incremental CLI path; do not duplicate stall/regression logic.

The rendering boundary receives compact dispositions, cached eligibility and ProgressResult. Warning order remains: unusable-input warnings in input order, candidate-set warnings in comparison order, ambiguous-key warnings in comparison order. Final-pair usability comes from dispositions, not the last comparison alone.

Alternatives: only shrinking each mutant struct retains history-linear growth; streaming JSON parsing alone leaves the whole decoded history alive. Dropping metadata and rendering as inputs arrive breaks the current late-error and warning-order contract. Adjacent retention addresses the measured cause with existing parsing/validation intact.

## Validation

Create a dedicated allocation-measured integration test with one controlled 2,000-mutant report supplied 2 and 16 times. Peak Rust heap growth for the longer history must stay within 512 KiB of the short history; do not assert wall-clock time or macOS RSS in CI. Measure the actual progress path with sink writers. Reuse the existing report_heap tracking allocator via a small test-support module if needed, preserving that test's behavior and avoiding duplicate unsafe allocation logic.

Existing progress and Lean progress-decision oracle tests validate decision equivalence. Add a mixed-history public CLI regression covering unusable gaps and warning order if existing tests do not exercise the combined case. Add a late invalid-report check for both output formats: no stdout and no earlier warning emitted. Preserve canonical existing semantics, including acceptance rules unrelated to this memory change.

Rerun the issue's standalone 50,000-mutant 2/4/8-report RSS probe on the final CLI and record results as an experiment, not a CI time threshold. Verify fmt, all-feature Clippy, workspace all-feature tests and MSRV. No production Python changes are required.

## Delivery

Separate main-based Issue worktree and PR with this design, implementation plan and report. User authorized autonomous design, implementation, tests and PR; no merge.
