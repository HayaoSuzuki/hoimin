# Corpus Distillation Experiment Implementation Plan

> **For agentic workers:** Execute inline and request one independent final review.

**Goal:** Complete #803's bounded before/after corpus comparison.

**Architecture:** Temporarily extend the existing manually dispatched fuzz workflow
on a research branch. Restore one size-checked cache read-only, use one instrumented
python_analyzer binary, replay three times on each side, and compare feature IDs
from two fresh merge control files. Remove the temporary workflow extension before
the final research PR; retain the original experiment script and evidence.

**Tech Stack:** Python 3.14, pinned nightly-2026-10-08/cargo-fuzz 0.13.2, libFuzzer.

**Spec:** #803 and the approved bounded follow-up in the #805 assessment.

## Global constraints

One worker. Whole hosted job <=20 minutes including setup/build; execution
deadline leaves cleanup time. Corpus cache <=32 MiB compressed; snapshot <=64 MiB,
50,000 files, 4,096 bytes/input. Fuzzer RSS <=1,024 MiB, input timeout 5 seconds.
No corpus cache writes, product changes or mandatory new CI job. Existing managed
seeds and original corpus remain unchanged. Use a fresh hosted target directory
and cargo clean it in the final step. Never clean the shared local build.

## Steps

- [x] Implement a research-only script using existing Campaign deadlines/process
  groups; hash the corpus snapshot, record stage times, and strictly parse the
  STARTED/FT/COV records. Test truncated controls and missing features locally.
- [x] Add temporary manual-only workflow mode, select one bounded default-branch
  cache and restore exact key without fallback to another corpus.
- [x] Dispatch once, acquire bounded results, compare all three replay times,
  merge cost and exact feature/PC sets on the identical binary.
- [x] Decide adopt/reject/limited evidence, remove temporary dispatch changes,
  update knowledge/catalog evidence, perform independent review and create PR.

## Design self-review: five passes

1. Checked user-authorized #803 scope, keeping #802/#804 closed and #806 separate.
2. Checked cache restoration is read-only and all experiment outputs are separate.
3. Checked exact binary, fixed snapshot/seeds, feature identities and PC indices;
   counts alone cannot justify replacement.
4. Checked total job/stage budgets, one worker, RSS, compressed/decompressed input
   limits and dedicated cleanup ownership.
5. Checked this experiment does not imply a permanent compression service; a
   negative result or platform/measurement limitation is an acceptable outcome.

## Plan self-review: five passes

1. Checked source pin and actual latest cache metadata before dispatch.
2. Planned three bounded replay measurements on each side and separate merge cost.
3. Planned failure tests for incomplete control data and missing feature identity,
   not mirrored happy-path implementation tests or a costly CLI suite.
4. Planned retention of source SHA, snapshot/binary hashes, logs and small evidence
   so expiring Actions artifacts do not become the only result record.
5. Planned removal of temporary workflow changes and final independent review
   before delivering research conclusions.
