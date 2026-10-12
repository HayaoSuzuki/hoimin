# Philosophy follow-up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Run inline; use a fresh reviewer at the end.

**Goal:** Address #834–#837 in separately reviewable stacked branches.

**Architecture:** Keep existing handlers and safety protocols. Consolidate
behavioral evidence, encapsulate validated execution inputs, remove unused
forwarders and represent cleanup progress explicitly.

**Tech Stack:** Rust 1.99.0, Python 3.14, pytest, MkDocs, existing Cargo dependencies.

**Spec:** `docs/superpowers/specs/2026-10-12-philosophy-followup-design.md`

## Global constraints

- Version floor 0.4.0; no new dependency, schema, CLI, package or platform contract.
- README unchanged; maintained documentation English.
- AAA and branchless new tests; prefer existing behavioral protection.
- Five self-reviews per stage and issue, then final independent review.
- Two build jobs, two test threads, one task-owned target and bounded cleanup.

## Review focus

1. Missing command separators must still fail executable documentation checks.
2. Metrics updates must preserve selection, limits, preview and protected inputs.
3. Files changed after plan preparation must still fail before baseline.
4. Workspace cleanup errors must retain their IDs, order and ownership behavior.
5. Interrupted cleanup must not become successful or eligible for a second attempt.

## Task 1: Documentation contracts (#834)

**Files:** `tests/test_ranked_plan_docs.py`, CLI `tests/run_e2e.rs`,
`tests/report_handler.rs`, new maintenance record.

**Interfaces:** Keep existing command extraction and schema checks; no production API.

- [ ] Map each deleted prose assertion to existing behavioral or rendering evidence.
- [ ] Remove the Python wording-only module and two Rust prose-only tests.
- [ ] Replace report-handler prose assertions with a parsed metrics-example check.
- [ ] Run documentation Python tests, report-handler tests and strict site generation.
- [ ] Run a temporary negative probe removing the separator from an extracted command;
  confirm failure, then restore the original bytes.
- [ ] Record five checks at every stage and commit on `refactor/834-doc-contracts`.

## Task 2: Verified execution (#835)

**Files:** CLI `plan.rs`, `lib.rs`, `shell.rs`, `tests/plan.rs`, maintenance record.

**Interfaces:** `VerifiedPlan::{config, selection, verification_selection, preview}`
return shared references; `with_metrics` consumes self; `execute` consumes self
and borrows output writers. Internal `VerifiedExecution` owns config, selection,
selection metadata, manifest inputs and copied fingerprint inputs.

- [ ] Add an API-level behavior test for metrics-only updates preserving inspected
  configuration, ordered selection and preview; observe failure before implementation.
- [ ] Privatize fields, remove duplicate metadata and unnecessary fingerprint getter.
- [ ] Migrate dispatch, shell payload consumption and inspection tests. Use `execute`
  in the existing changed-after-preparation fingerprint regression.
- [ ] Run plan/verify, metrics destination and CLI configuration checks.
- [ ] Document internal Rust source migration and commit a second stack layer.

## Task 3: Workspace API (#837)

**Files:** CLI `workspace/mod.rs`, maintenance record.

**Interfaces:** Retain all existing `WorkspaceHandler::handle_*` methods.

- [ ] Recheck policy and all repository consumer/import sites.
- [ ] Remove only the six forwarding functions; do not rewrite handler behavior.
- [ ] Check workspace behavior and compile all targets; document source migration.
- [ ] Record distinct five-stage checks and commit a third stack layer.

## Task 4: Cleanup progress (#836)

**Files:** CLI `shell.rs`, maintenance record.

**Interfaces:** Internal `CleanupProgress::{NotAttempted, Attempted, Cleaned}`,
replacing the two independent attempted/cleaned pairs at existing boundaries.

- [ ] Enumerate original assignment and observation sites before editing.
- [ ] Replace state representation without moving cleanup or lifecycle operations.
- [ ] Run existing interruption, clean/deferred/failed cleanup and ordering checks.
- [ ] Confirm unchanged DiskLifecycle/model correspondence; no new proof claim.
- [ ] Record five checks at every stage and commit a fourth stack layer.

## Stack verification and delivery

- [ ] Run formatting, strict Clippy for all targets/features, relevant Python checks,
  strict site generation and full Rust workspace tests with `--no-fail-fast`.
- [ ] Run contracts-enabled checks for changed runtime behavior.
- [ ] Obtain a fresh independent review of the complete diff and fix findings.
- [ ] Clean the checked task-owned target, submit with `gh stack`, write accurate PR
  descriptions and attach every created PR. Do not merge or publish releases.
