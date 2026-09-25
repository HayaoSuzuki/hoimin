# Issue 624 implementation and verification

The session fingerprint excludes only max-mutants (schema10); SQLite schema4 stores a positive8-byte unsigned big-endian last-accepted budget. Reuse requires a compatible incomplete run with old budget <= requested, reserves that budget after run ownership, and preserves cumulative scheduler accounting. Legacy budgets remain NULL and old data is retained. Design/plan were committed before production changes (b95d3d6); each contains three review passes.

## Implementation self-review passes

1. Compatibility/accounting: inspected the entire fingerprint encoding and both BeginSession/LoadSession construction sites. Only max-mutants was removed; candidate/resource/verdict limits stay encoded. Existing machine reuse increments cumulative accounting and still runs the baseline. No scheduling rewrite was needed. Public RED showed an unwanted new run after increasing the budget; GREEN preserves run ID and executes only the next candidate.
2. Persistence/ownership: reviewed migration ordering, unsigned ordering, legacy rows, latest eligible selection, ownership acquisition and conditional write. Independent review found unchecked corrupt blobs could be reused/repaired. Added exact length/positive decoding before ownership and exact old-blob equality in the conditional update. A corruption regression failed before this change and preserves original bytes afterward. Extracted the validator after Clippy's function-length finding.
3. Error/selection boundaries: an additional independent review found lexically high malformed blobs could be filtered out before validation. Added invalid type/length/zero alternatives to SQL selection and typed-value validation; new high-sorting witness reproduced the omission. Integer/text/real values also fail as session.corrupt. Rechecked that valid lower-budget requests still skip ineligible runs and that completed rows remain excluded. No ownership is published after a failed conditional update.

## Test self-review passes

1. Public behavior: both killed and survived tests assert baseline success, current normalized limit, run identity, actual execution count, reused-null termination, fresh-run equivalent candidates/verdicts, decreasing limit and completion. Expanded completion from a single verdict to both. Added a30s outer CLI deadline so a shutdown regression cannot hang indefinitely.
2. Persistence fixtures: updated versioned SQLite4 golden, preserved v1-v3 original fixtures and logical rows, explicitly checked legacy NULL/new100 budgets. Added full unsigned-range storage and malformed constraints/read controls. Injected v4 failure after v3 work to verify the entire migration transaction rolls back without losing results/diagnostics. Corrected an old fingerprint-version9 assertion and migration-oracle current/future mapping revealed by the full suite.
3. Oracle correspondence: added version1 and stable unique tuple IDs after review. All108 strict cases configure the same budget/complete/compatible/status premises through public SessionHandler APIs, reopen the database, compare selected run and resume_policy, and inspect persisted budget. Broken-budget/status/completion variants are detected separately by Lean sensitivity. No claims are made about SQL interleavings or arbitrary scheduler traces from this model.

## Lean correspondence and bounds

| Premise/observation | Model | Implementation |
| --- | --- | --- |
| old/requested budgets |1,2,3 | BeginSession/LoadSession nonzero limits |
| incomplete/complete |Bool | FinishSession and reopen |
| compatible/incompatible |Bool | equal/different fingerprint bytes |
| reusable status |killed/survived/timeout | PersistResult, lookup, resume_policy |
| selected run |eligible | public load resume reference |
| recorded limit |new if eligible else old | SQLite persisted blob |

All108 cases are strict. General Nat theorems prove no decreases and monotonic eligibility. Finite search is exactly3×3×2×2×3, with no trace enumeration. Each new theorem has10,000 heartbeats; each command uses20s/2GiB with native freshness and sensitivity. Resource telemetry is committed beside this report. Existing session/migration/candidate-span oracles remain part of Rust tests.

## Validation

Final results are recorded below after the final commands complete. Earlier intentional RED logs include increased-budget incompatibility, unchecked corrupt-byte repair, and high-sorting malformed-byte omission. An intermediate full run exposed stale schema constants, which were corrected before final validation. Independent reviews covered production/persistence and public tests/Lean correspondence; all reported semantic findings were addressed.

Final workspace: 2327 passed, 0 failed, 22 ignored across 101 result groups. Exact workspace Clippy passed. Final malformed storage includes high-sorting one/nine-byte blobs and INTEGER/TEXT/REAL values, preserving the original database value on failure.
