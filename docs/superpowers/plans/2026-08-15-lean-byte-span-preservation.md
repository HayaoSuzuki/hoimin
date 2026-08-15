# Mutation byte-span preservation Lean audit plan

## 1. Baseline and correspondence

- [x] Record candidate validation, analyzer, plan, spool, session, workspace,
  and public CLI baselines.
- [x] Inventory each real transport projection and its observable fields.
- [x] Fix strict/internal/model-only/infrastructure modes before generating data.

## 2. Lean semantics and proofs

- [x] Add source, boundary, location, descriptor, identity, validation,
  transport, application, and reset semantics.
- [x] Prove accepted span/original/location facts, transport and identity-tuple
  preservation, exact prefix/replacement/suffix application, rejection
  non-modification, and reset independence.
- [x] Keep model/proofs importable without cases, sensitivity, or generators.

## 3. Cases, sensitivity, and corpus

- [x] Add closed ASCII, multiline, multibyte, transport, rejection, reset,
  model-only, and infrastructure cases.
- [x] Detect every applicable sensitivity family independently.
- [x] Generate and freshness-check a typed JSONL corpus with bounded statistics.

## 4. Rust correspondence and repair

- [x] Replay strict rows through real analyzer/public plan and run boundaries.
- [x] Check serde, spool, machine-effect, and session-ID projections.
- [x] Check workspace application bytes, every rejection unchanged, and reset
  before a second candidate.
- [x] If a same-premise mismatch exists, add its failing regression first and
  implement the smallest Rust correction.

## 5. Delivery

- [x] Write the audit report and counterexample ledger.
- [x] Run resource-guarded Lean checks, focused and workspace Rust tests,
  formatting, strict clippy, and diff checks.
- [ ] Obtain independent review with no Critical or Important findings.
- [ ] Push, open a PR closing #312, wait for all required CI, squash merge, and
  remove the dedicated worktree.
