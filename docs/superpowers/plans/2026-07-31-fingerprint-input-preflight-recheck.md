# Fingerprint Input Preflight Recheck Implementation Plan

> **Issue:** #141 — re-verify fingerprint-input files before workspace execution

## Goal

Guarantee that the fingerprint-input hashes recorded by a verify run describe
the immutable workspace snapshot actually used for baseline and mutant
execution. Reject a file that changes after verify preparation instead of
attributing results to stale configuration provenance.

## Design

Re-resolve fingerprint inputs immediately after
`WorkspaceHandler::handle_preflight` has finished the shared snapshot and
before `prepare_fingerprint` or `PreflightCompleted`. Compare the newly
resolved records with the records frozen into normalized configuration.

This ordering closes the relevant TOCTOU window:

- A change before or during snapshot construction is visible to the
  post-snapshot recheck and is rejected.
- A change after the recheck cannot alter the already-materialized shared
  snapshot used by workers.

Reuse the existing capability-relative fingerprint resolver so link,
reparse-point, traversal, UTF-8, and read-error policy stays identical. Report
both resolution failures and record mismatches with the existing
`plan.fingerprint_input.changed` diagnostic and the preflight effect ID. No
new core effect or machine phase is required.

## Task 1: Add a deterministic verify regression

**Files:**

- Modify: `crates/hoimin-cli/tests/plan.rs`
- Modify: shell tests only if a narrower hook is needed

Create a plan and successfully complete verify preparation, then change a
fingerprint input at that deterministic seam before running the selected
shell loop. Assert:

1. The run exits with the expected infrastructure/configuration failure.
2. The diagnostic code is `plan.fingerprint_input.changed`.
3. Baseline and mutant execution do not start.
4. A test-command marker is not created.

This reproduces the bug without sleeps or timing-dependent threads. Add
focused cases for deletion or unsafe replacement only where they clarify
error mapping; the resolver's no-follow and path behavior already has its own
dedicated tests.

## Task 2: Recheck against the completed snapshot boundary

**Files:**

- Modify: `crates/hoimin-cli/src/fingerprint_inputs.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`

Add a small helper that resolves the normalized selectors from the original
root and compares the sorted, unique records with the expected configuration
records. Preserve existing resolver diagnostics internally, while mapping any
failure at this lifecycle boundary to the stable changed-input code.

In the `RunEffect::Preflight` handler, keep this order:

1. Complete `handle_preflight` and its immutable shared snapshot.
2. Recheck fingerprint inputs.
3. Prepare the run fingerprint.
4. Return `PreflightCompleted`.

On failure, use the existing preflight effect ID so the machine follows its
normal fatal diagnostic and cleanup path before worker creation or baseline
execution. Document the ordering invariant near the call site.

Run the focused plan and fingerprint-input suites, shell/machine lifecycle
tests, formatting, Clippy, full workspace tests, contract-feature tests, and
`git diff --check`. Request an independent review before creating the PR.
