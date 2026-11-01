# Typed mutation outcomes

For maintainers changing execution, reports, or session persistence. This contract replaces independent status and nullable evidence with one owned mutation outcome. The approved issue scope requires current-only serialization; there are no legacy adapters.

## Outcome contract

`MutationOutcome::Executed { termination, output_state, output, elapsed }` requires complete execution evidence. Its status derives centrally from termination and output state: close timeout is error; complete output uses the existing exit/resource/cancellation classification. `Reused { status: ReusedMutationStatus }` permits only killed or survived. `NotRun` means no classified completed execution result is available; a process may have started. Reused and NotRun project zero elapsed time and absent output.

`MutantResult` retains run identity, candidate, owned outcome, resource mode, and independent diagnostics. `MutantFinished` carries the same outcome. No stored or report status competes with execution evidence. Diagnostics explain failures and never reconstruct output state. Baseline policy remains unchanged; `ProcessFinished` requires explicit output state on the wire.

The session stores typed outcome JSON, with resource mode and candidate identity retained in their existing locations. Lookup derives the narrow StoredResult status from the decoded outcome. Session schema 5 rejects prior versions; report schema 6 rejects flat legacy result records. Historical golden files remain historical; add current fixtures for both schemas.

## Preservation boundaries

Preserve status classification, reusable killed/survived policy, zero elapsed and absent output for synthetic results, synthetic start/finish ordering, first-stop and baseline exit policy, diagnostic persistence, and candidate/result transaction atomicity. Preserve cancellation as an Executed outcome when a completed process event supplies evidence. Preserve stopped candidate drain as NotRun where no completed event exists. Root issue 897 can retain the entire MutantResult without decomposing its outcome.

This changes representation and removes contradictory combinations. It does not establish a stronger operating-system execution or cancellation guarantee.

## Design review passes

1. Compared ProcessFinished, MutantResult, and MutantFinished: status and output state had different authorities. Correction: one enum owns evidence and derives status; reports carry the enum itself.
2. Traced stored-result lookup and resume_policy: only killed/survived can be reused. Correction: restrict ReusedMutationStatus to those two variants rather than accepting all MutationStatus values.
3. Traced stopped-candidate drain: running candidates can lack a classified completed result. Correction: name NotRun and explicitly permit a process to have started.
4. Traced persistence and machine report emission: output state was reconstructed from a diagnostic code. Correction: store the outcome directly and keep diagnostics independent.
5. Compared public report and SQLite compatibility rules: both need new current schemas, and old fixtures must remain reproducible. Correction: report 6/session 5 with new current goldens and rejection tests; remove legacy defaults and mismatch validators rather than preserve no-op adapters.

## Scoped design diagnostics

Target: the outcome boundary, not the whole repository. Seven evaluable diagnostic rows pass by design: one-sentence purpose, smaller interface, classification hidden centrally, documented promises, design-focused reviews, owned classification decision, and explicit consumer boundaries. Time-allocation row is unmeasured: record design/review/implementation time in a future change before claiming that row. Provisional score 7/8 (8.75/10 equivalent); implementation review must verify these claims. No failed measured rows; unmeasured time allocation prevents an evidenced 10/10.
