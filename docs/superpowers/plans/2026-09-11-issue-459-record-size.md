# Issue #459 Implementation Plan

> Execute task-by-task with executing-plans; autonomous work through PR is authorized.

**Goal:** Never create an apparently executable plan whose candidate exceeds the spool encoding limit.
**Architecture:** Reuse one allocation-free serialized-record size check at store, discovery and manifest boundaries.
**Tech Stack:** Rust 2024, MSRV 1.88, serde_json and existing CLI harnesses.
**Spec:** [Record limit design](../specs/2026-09-11-issue-459-record-size-design.md)

## Constraints

Keep the 2 MiB limit, newline-inclusive comparison and single payload allocation in push. No candidate dropping or baseline-order redesign. Preserve existing serialization and candidate/plan schemas.

## Task 1: Shared bound and regressions

Files: `crates/hoimin-cli/src/analyzer/store.rs`, `analyzer/mod.rs`, `plan.rs`, `tests/plan.rs`.

- [x] Add a plan fixture with a 1.1 MiB list literal; require exit 2, no stdout/baseline marker and path/line/operator/limit diagnostic. Run against old code to observe successful invalid planning.
- [x] Extract counting logic from push into `CandidateStore::record_size(candidate: &MutationCandidate) -> Result<usize, StoreError>`, returning checked payload size + 1.
- [x] Call it before plan discovery retains a MutationCandidate and while validating manifest candidates; use it from push for capacity. Format errors with candidate path, line and operator.
- [x] Extend boundary tests: independent compact serialized bytes + newline must equal shared size; exact limit succeeds, one byte over fails, including escaped and Unicode content.
- [x] Run store tests and the oversized plan regression.

## Task 2: Execution boundary and delivery

Files: `tests/plan.rs`, README, selection OKF, design index.

- [x] Build a genuine old-style oversized manifest from a valid seed by updating source, literal candidate, hash, span and stable ID. Verify must reject the record before baseline.
- [x] Verify a 900 KiB list candidate executes and produces one survived result; direct oversized run reports incomplete failure at analysis with actionable context.
- [x] Document record encoding limit and direct-run baseline ordering; validate workspace/contracts, Clippy, formatting and OKF.
- [x] Perform three implementation/test/PR self-reviews.
- [ ] Commit and create PR fixing #459.

## Plan self-review

1. Coverage: creation, saved-manifest read, actual spool write and success/failure boundaries have distinct assertions.
2. Types/serialization: use MutationCandidate rather than RankedPlanCandidate; sequence and all encoded fields remain in the byte count.
3. Verification: existing exact byte boundary fixtures avoid approximate string-length assumptions; legacy manifest must be semantically genuine, not just invalid metadata.

## OKF self-review

1. Existing selection concept receives the new contract, explicitly distinguishing plan/verify preflight from direct-run ordering.
2. Design source uses its actual untracked SHA-256 and remains draft; no execution evidence is fabricated.
3. Updated design index maintains navigation; final YAML/source/link checks are required after edits.

## Implementation self-review

1. Boundary: extracted the original counter and retained its `payload >= limit` rejection, so payload + one newline may equal the limit. Push still checks sequence/count before serialization and allocates only the validated bounded record.
2. Integration: discovery validates before retaining candidates; header validation counts flattened MutationCandidate fields only. Run's existing push uses the same validator and now adds candidate context on failure.
3. Lifecycle and identity: validation changes no serialization/schema/ID or scheduler ordering. Plan and old-manifest verify reject before baseline; direct run truthfully reports its existing post-baseline analysis failure. No candidates are silently dropped.

## Test self-review

1. Sensitivity: the old plan command returned exit 0 for the 1.1 MiB fixture; the new regression expected 2 and failed before implementation. It now passes with path/line/operator/limit details and empty stdout.
2. Legacy evidence: constructed an actual large list-to-tuple candidate with matching source bytes, span, hash, stable ID and source record from a valid seed plan. Verify rejects before its external marker is written. The smaller 900 KiB fixture successfully plans, verifies and runs.
3. Size oracle: exact record-boundary tests use independent serde_json byte vectors plus one newline, including quotes, backslashes, controls and multibyte Unicode. Store's existing failed-write/no-partial-record and ordered replay tests remain included.

## Final validation

- `cargo test --workspace --all-features`: 1611 passed, 0 failed, 13 ignored, including both packages' contracts and existing Lean corpus adapters.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`, formatting and diff checks: passed.
- OKF: 16 pages, 309 source-footnote pairs, 611 local links checked; YAML/reserved files, reachability, all design indexing and new source hashes passed.
- macOS arm64 / CPython 3.14.7; no new Lean model or local Lean compilation. IDE MCP has no hoimin project open.

## PR self-review

1. Scope: reviewed the extraction against the original push code, all three call boundaries and the documented direct-run baseline limit. Source path, line and operator are included without changing diagnostic codes or schemas.
2. Evidence: exact-byte boundaries, genuine legacy manifest, smaller successful candidate and oversized direct failure are all in tests. Independent reviewer found no actionable defects; final workspace includes the new escaped/Unicode test.
3. Delivery: branch from main 4adf809 contains requested code/tests/docs/OKF/design/plan only. PR template distinguishes executed checks, historical Lean evidence and platform limits; environment symlink is not staged.
