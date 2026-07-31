# Fingerprint Input Preflight Recheck Implementation Plan

> **Issue:** #141 — re-verify fingerprint-input files before workspace execution

## Goal

Guarantee that the fingerprint-input hashes recorded by a verify run describe
the immutable workspace snapshot actually used for baseline and mutant
execution. Reject a file that changes after verify preparation instead of
attributing results to stale configuration provenance.

## Design

Re-resolve fingerprint inputs inside workspace preflight, after the initial
workspace manifest is built and immediately before the shared snapshot is
materialized. Compare the newly resolved records with the records frozen into
normalized configuration.

This ordering closes the relevant TOCTOU window:

- A change before validation is visible to the recheck and is rejected.
- A change between validation and snapshot materialization disagrees with the
  initial manifest and is rejected while the snapshot is copied.
- A change during or after snapshot materialization disagrees with either the
  manifest hash or the final manifest rescan.

Validation continues to read the original root, preserving the documented
independence between fingerprint selection and worker-copy include/exclude
policy. The surrounding manifest/snapshot checks couple copied inputs to the
immutable worker view without requiring ignored or explicitly excluded
fingerprint inputs to be copied.

Record which prepared fingerprint paths are copy targets in `VerifiedPlan`,
before verify preparation returns. Pass that private sidecar into selected
execution. During preflight, compare those paths and all manifest-matched
selectors directly with the initial manifest. This closes content, addition,
and deletion ABA cycles without treating intentionally ignored or excluded
fingerprint inputs as missing worker files.

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

## Task 2: Recheck inside the snapshot boundary

**Files:**

- Modify: `crates/hoimin-cli/src/fingerprint_inputs.rs`
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/src/workspace/copy.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/lib.rs`

Add a small helper that resolves the normalized selectors from the original
root and compares the sorted, unique records with the expected configuration
records. Preserve existing resolver diagnostics internally, while mapping any
failure at this lifecycle boundary to the stable changed-input code.

Add an internal validated-preflight path while preserving the existing
workspace handler API. Keep this order:

1. Build the initial workspace manifest.
2. Recheck fingerprint inputs against the original root and directly compare
   copied records with the initial manifest.
3. Materialize the immutable shared snapshot from that manifest.
4. Re-scan the workspace and reject manifest drift.
5. Prepare the run fingerprint and return `PreflightCompleted`.

On failure, use the existing preflight effect ID so the machine follows its
normal fatal diagnostic and cleanup path before worker creation or baseline
execution. Cover content ABA and unchanged ignored/explicitly-excluded exact
and glob inputs in deterministic tests.

Run the focused plan and fingerprint-input suites, shell/machine lifecycle
tests, formatting, Clippy, full workspace tests, contract-feature tests, and
`git diff --check`. Request an independent review before creating the PR.
