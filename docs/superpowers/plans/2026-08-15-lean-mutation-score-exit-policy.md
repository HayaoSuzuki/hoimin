# Mutation score and exit policy Lean audit plan

## 1. Baseline and contract

- [x] Record focused Rust and Lean baselines.
- [x] Map `MutationStatus`, `MutationSummary`, `ExitPolicy`, and `RunMachine`
  fields to the correspondence worksheet.

## 2. Lean semantics and proofs

- [x] Add `MutationScoreExitPolicyModel.lean` with exact counts, scores,
  policy composition, completeness, and exit selection.
- [x] Add `MutationScoreExitPolicyProofs.lean` with universal fold,
  permutation, arithmetic, precedence, and completeness theorems.
- [x] Compile an external proof consumer under the audit resource guard.

## 3. Cases, sensitivity, and corpus

- [x] Add fixed strict, internal, and model-only cases.
- [x] Add every required broken variant and verify that each has a minimized
  witness in the bounded domain.
- [x] Generate and freshness-check a closed JSONL corpus.

## 4. Rust correspondence

- [x] Parse the corpus strictly and reject schema, mode, and premise drift.
- [x] Compare complete `summarize`, score projection, policy, completeness,
  and exit observations.
- [x] Exercise composed private machine flags with an owned core fixture and
  public `RunFinished` output where the premise is configurable.
- [x] If a mismatch is confirmed, add the failing regression first and make
  the smallest production fix.

## 5. Report and delivery

- [x] Write the audit report with overlap, exclusions, counterexample ledger,
  bounded statistics, and resource measurements.
- [x] Run focused tests, full workspace tests, formatting, strict clippy, Lean
  build, corpus checks, and diff checks.
- [ ] Obtain independent review with no Critical or Important findings.
- [ ] Push, create a PR closing #310, wait for required CI, and merge.
