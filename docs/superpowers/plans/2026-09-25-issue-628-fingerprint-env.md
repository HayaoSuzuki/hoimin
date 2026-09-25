# Issue 628 implementation plan

> Execute inline using the executing-plans workflow; the user authorized design, implementation, tests, commits, and PR publication.

**Goal:** Explicit inherited-environment inputs affect resume/plan compatibility without adding captured plaintext to persistence or output.

**Spec:** `docs/superpowers/specs/2026-09-25-issue-628-fingerprint-env-design.md`.

**Base:** Issue624 `19ffb88`; rebase excluding its commits onto main after624 merges. Maintain a two-level stack only.

## Task 1: RED and normalized capture

- [ ] Add a public real-run regression for strict1 → weak0 resume plus a fresh control; observe missing-option RED before production changes.
- [ ] Add typed config tests for the name grammar, duplicate/order normalization, Unix/Windows case policy, native value distinction, and legacy missing fields. Add injected-reader capture tests for absent/empty, framed boundaries, invalid-native-byte distinctions, unselected inputs, and no-value serialization.
- [ ] Add raw/normalized run/plan fields and validators, shared CLI option, capture module, run preparation, and fingerprint field13/schema11. Update direct struct constructors and pinned version assertions.
- [ ] Observe the real-run regression GREEN; add same-value, untracked, name-list, empty/native-value, and privacy controls. Use subprocess environments and deadlines, not global environment mutation.

## Task 2: plan/verify and model correspondence

- [ ] Add plan schema5, persisted names/digest validation, and current-environment comparison before baseline; retain verified values in the prepared run path. Test unchanged/changed environment, malformed fields, empty legacy config decoding, and a baseline marker proving early rejection.
- [ ] Add the finite Lean environment model with a deliberately flattened RED, general four-value proofs and negative controls. Generate versioned32-case corpus only through Lean.
- [ ] Register model, executable, corpus and sensitivity in the existing serial CI registry/lakefile order, updating the closed registry contract test.
- [ ] Add typed unique-ID Rust corpus adapter that observes public initial/resumed runs with actual SQLite persistence; separate infrastructure failures from semantic mismatches. Preserve status/budget/exit checks and strict-mode expectations.
- [ ] Request global Lean slot, execute model/native/generate/freshness/sensitivity/stats under20sec/2GiB, and release explicitly. New proofs10k heartbeats.

## Task 3: reviews and delivery

- [ ] Update README/help/development and additive normalized-config schema documentation for grammar, platform/native rules, startup capture, hash privacy limit, run/plan/verify behavior, and version boundaries.
- [ ] Three implementation self-reviews (capture/encoding/privacy; prepared run/verify/persistence; platform/version contracts) and three test reviews (RED sensitivity; exact semantic/privacy controls; real oracle boundary). Record findings and obtain independent read-only review.
- [ ] Run focused config/fingerprint/session/plan/oracle tests, full workspace, exactCI clippy2/fmt2, and workflow Python unittest/lint as appropriate to registry changes. Record evidence without modifying shared environments.
- [ ] Commit final changes, rebase onto latest main excluding624 if merged, perform risk-appropriate integration checks, and publish via gh stack. Root owns CI/merge/cleanup.

## Plan self-reviews

1. Acceptance coverage: include a fresh weak control and actual run-ID/termination/executed observations; merely asserting a changed digest would miss reuse-policy errors. Cover selected-name changes separately from value changes and ignore unselected values by construction.
2. Error/privacy boundaries: verify must reject a changed environment before any test baseline, and incomplete persisted metadata must not silently become an untracked plan. Scan all generated artifacts for a unique secret marker absent from source/argv, and test invalid native bytes without lossy conversion.
3. Oracle/resource scope: retain exactly32 public cases, derive expectations only in Lean, use typed closed/versioned unique cases, and keep native/platform/hash properties in Rust tests rather than claiming a proof of them. Register CI commands in the existing closed order and acquire the single Lean slot before execution.
