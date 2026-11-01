# Report Stable Mutant Identity Implementation Plan

**Goal:** Reject repeated stable mutant identities without restricting parallel
execution of distinct mutants.

**Architecture:** Extend `ReportSequence` with run-lifetime identity history and
reuse the same ID-to-sequence rule when validating persisted progress reports.

## Task 1: Add failing core lifecycle tests

**Modify:** `crates/hoimin-core/tests/report_policy.rs`

1. Add sequential and concurrent different-sequence reuse cases.
2. Add same-ID/same-sequence duplicate and finish mismatch cases.
3. Add a distinct-ID parallel control.
4. Run the focused tests and confirm the current implementation accepts at
   least the sequential and concurrent reuse cases.

## Task 2: Implement typed stable identity validation

**Modify:** `crates/hoimin-core/src/report.rs`

1. Add typed duplicate-identity and identity-sequence-mismatch errors.
2. Add a `BTreeMap<String, u64>` retained for the run lifetime.
3. Validate identity before active lifecycle checks.
4. Insert the identity only after the event passes every invariant.
5. Run core report-policy tests and confirm they pass.

## Task 3: Add a failing progress-reader regression

**Modify:** `crates/hoimin-cli/tests/progress.rs`

1. Create summary-consistent documents containing a repeated stable ID with the
   same and different candidate sequences.
2. Assert both are rejected as invalid structure.
3. Run the focused tests and confirm the current reader accepts them.

## Task 4: Enforce identity in the persisted-report reader

**Modify:** `crates/hoimin-cli/src/progress/input.rs`

1. Track candidate ID to sequence while validating mutant events.
2. Return distinct stable structure messages for duplicate identity and
   sequence mismatch.
3. Preserve existing schema, summary, run-ID, and event-sequence validation.
4. Run focused progress tests and confirm they pass.

## Task 5: Verify and deliver

1. Run `cargo fmt --all --check`.
2. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
3. Run `cargo test --workspace`.
4. Run `git diff --check`.
5. Request an independent code review and resolve Critical/Important findings.
6. Push the dedicated branch and create a PR closing Issue #83.
