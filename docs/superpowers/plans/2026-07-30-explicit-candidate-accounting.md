# Explicit Candidate Accounting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject explicit verification when any requested candidate ID is absent from analyzer replay.

**Architecture:** Retain the immutable requested candidate filter and record IDs matched during streaming replay. Validate the requested-minus-matched remainder at EOF, reusing the existing typed missing-candidate error and leaving ordered/unfiltered paths unchanged.

**Tech Stack:** Rust 1.85+, hoimin-core state machine, Tokio CLI integration tests, Cargo

## Global Constraints

- Explicit verification succeeds only when every requested ID is accounted for.
- Missing IDs produce `MachineError::SelectedCandidateMissing`.
- Missing candidates never produce a complete final report.
- Ordered selection retains saved-rank execution order.
- Unfiltered runs retain existing streaming behavior.

---

### Task 1: Specify explicit missing-candidate behavior

**Files:**
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-core/src/machine.rs`

**Interfaces:**
- Consumes: `RunState::with_candidate_filter`
- Produces: private `matched_candidate_ids: BTreeSet<String>`
- Produces: existing `MachineError::SelectedCandidateMissing(String)` at EOF

- [ ] **Step 1: Add failing single-missing machine test**

Request `m2`, replay `m1`, then replay EOF. Assert the EOF transition returns
`MachineError::SelectedCandidateMissing(m2)` and cannot return final-report
effects.

- [ ] **Step 2: Add failing partial-match machine test**

Request `m1` and `m2`, drive `m1` through apply, start, run, finish, and reset,
then replay EOF. Assert the summary records only the actual `m1` result and EOF
returns `SelectedCandidateMissing(m2)`.

- [ ] **Step 3: Verify RED**

```console
cargo test -p hoimin-core --test machine explicit_candidate_filter_rejects_missing
```

Expected: both tests fail because EOF currently finalizes successfully.

- [ ] **Step 4: Implement matched-ID accounting**

Initialize `matched_candidate_ids` empty. For explicit unordered replay, select
only requested IDs newly inserted into the matched set. At EOF, compare the
requested set against matched IDs and return the first missing ID.

- [ ] **Step 5: Verify GREEN**

Run the Task 1 focused command and the existing
`candidate_filter_skips_unrequested_candidates` and ordered-filter tests.

### Task 2: Cover the selected CLI boundary

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: `hoimin_cli::shell::run_selected_loop`
- Consumes: `VerificationSelection` with explicit-candidate policy
- Produces: integration evidence that incomplete analyzer replay is non-success

- [ ] **Step 1: Add failing integration test**

Create a real fixture configuration, request a nonexistent candidate ID through
`run_selected_loop`, and assert the returned error contains the missing ID and
the `selected candidate was not discovered` diagnostic.

- [ ] **Step 2: Verify RED**

```console
cargo test -p hoimin-cli --test run_e2e selected_loop_rejects_a_requested_candidate_missing_from_the_spool
```

Expected: fail because the selected loop currently returns successful exit zero.

- [ ] **Step 3: Verify GREEN after the core change**

Run the same test and expect the selected loop to return an error. Assert no
complete JSON report is emitted.

### Task 3: Review, verify, and publish

**Files:**
- Inspect: `README.md`
- Verify: all changed files

**Interfaces:**
- Consumes: Tasks 1-2
- Produces: PR closing #82

- [ ] **Step 1: Run focused verification**

```console
cargo test -p hoimin-core --test machine
cargo test -p hoimin-cli --test run_e2e selected_loop_rejects_a_requested_candidate_missing_from_the_spool
```

- [ ] **Step 2: Run full verification**

```console
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check
```

Every command must exit zero.

- [ ] **Step 3: Review README impact**

Confirm no user-facing option or workflow changed. If a README-only follow-up is
needed, commit it separately with `[skip ci]`; otherwise leave README unchanged.

- [ ] **Step 4: Request review and publish**

Request independent review against `origin/main`, resolve all Critical and
Important findings, push `fix/issue-82-explicit-candidate-missing`, and create a
PR that closes #82.
