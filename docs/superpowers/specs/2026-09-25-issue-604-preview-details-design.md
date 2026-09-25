# Issue 604: preview candidate details

Dry-run users need to distinguish mutations on the same source line without joining preview IDs back to a saved plan. Extend each preview candidate with `column`, `operator`, `original`, and `replacement`, copied from the same validated manifest candidate already selected by ID. Keep rank, discovery ordering, strict/diverse selection and offsets unchanged. No additional analysis, runtime setup, baseline, mutation command, worker copy or session is introduced.

JSON and JSONL still contain one complete object. The current closed preview schema is version 1, so adding required fields produces version 2. Update the schema at its existing URL and explain in README that version-1-only clients must add version-2 support; the CLI does not emit a legacy preview. Saved plan schema 4 and run-report schemas are unchanged. Columns are 1-based, like lines; operator is a nonempty string; original and replacement are strings that may be empty.

Human rows become `ORDER: ID rank=RANK PATH:LINE:COLUMN operator=OPERATOR ORIGINAL -> REPLACEMENT`, with both mutation strings represented using Rust's quoted debug escaping. Newlines, tabs, quotes, backslashes and terminal control characters therefore remain readable within one candidate row. No truncation is introduced.

Alternatives: retaining version 1 would contradict its closed schema; adding another candidate reanalysis would duplicate existing validated information and risk divergence; multiline human blocks would make same-line candidate comparison harder. The direct projection and single-line representation meet the requested scope.

## Design self-reviews

1. Data provenance: existing preview already indexes the validated manifest by selected ID. Added fields belong in this projection; no ranking or rediscovery API is needed.
2. Compatibility: current `additionalProperties: false` makes an unversioned field addition incompatible. Choose schema 2, document migration explicitly, and keep old plan format unchanged.
3. Human output: raw strings could inject new rows or terminal controls. Use quoted debug formatting for both texts, covering deletion/empty strings without special cases. Preserve existing metadata header. No further scope changes found.
