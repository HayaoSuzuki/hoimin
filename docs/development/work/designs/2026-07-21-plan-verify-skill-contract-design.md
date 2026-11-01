# Plan/Verify Skill Contract Design

## Problem

The bundled mutation-testing skills now prescribe the `plan`/`verify` workflow, but
`tests/test_skills.py` still asserts the retired `run`/`progress` workflow. The mismatch was merged
because the final skill-document changes used `[skip ci]`; the next pull request exposed the same
twelve failures on every operating system.

## Decision

Keep the plan-first skill workflow and update its executable contract. The contract test will
continue to verify byte-identical Codex and Claude mirrors and exact frontmatter. Its required body
blocks will instead encode the new workflow's durable safety properties:

- inspect and test production code before planning;
- create an external temporary plan and preserve all fingerprint inputs;
- select exact candidate IDs and verify them with a fresh baseline;
- stop on unusable, failed, incomplete, or cancelled results;
- regenerate stale plans when source, fingerprint inputs, selection, or execution configuration
  changes;
- improve one selected candidate at a time without production-only mutant fixes;
- use `progress` only for reports with identical candidate-ID sets;
- report unverified and truncated-away candidates honestly.

Assertions will use complete prose and command blocks from the published skills. This keeps the
test user-visible and implementation-independent while making omission of a safety rule fail CI.

## Alternatives Rejected

- Revert the skills to `run`/`progress`: contradicts the newly shipped plan/verify API.
- Delete stale required blocks without replacements: makes mirror presence pass while losing the
  behavioral contract.
- Assert every line or hash the files: couples the test to editorial formatting instead of safety
  semantics.

## Verification

First run the existing test after rebasing onto `main` and retain its twelve expected failures.
After updating the contract, run the focused skill test, the full Python suite used by wheel smoke,
the Rust workspace tests, formatting, clippy, and repository diff checks. The focused test must
pass on the same merged tree that previously failed.
