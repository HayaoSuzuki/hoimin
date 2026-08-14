# Mutation score and exit policy Lean audit plan

## 1. Baseline and contract

- [ ] Record focused Rust and Lean baselines.
- [ ] Map `MutationStatus`, `MutationSummary`, `ExitPolicy`, and `RunMachine`
  fields to the correspondence worksheet.

## 2. Lean semantics and proofs

- [ ] Add `MutationScoreExitPolicyModel.lean` with exact counts, scores,
  policy composition, completeness, and exit selection.
- [ ] Add `MutationScoreExitPolicyProofs.lean` with universal fold,
  permutation, arithmetic, precedence, and completeness theorems.
- [ ] Compile an external proof consumer under the audit resource guard.

## 3. Cases, sensitivity, and corpus

- [ ] Add fixed strict, internal, and model-only cases.
- [ ] Add every required broken variant and verify that each has a minimized
  witness in the bounded domain.
- [ ] Generate and freshness-check a closed JSONL corpus.

## 4. Rust correspondence

- [ ] Parse the corpus strictly and reject schema, mode, and premise drift.
- [ ] Compare complete `summarize`, score projection, policy, completeness,
  and exit observations.
- [ ] Exercise composed private machine flags with an owned core fixture and
  public `RunFinished` output where the premise is configurable.
- [ ] If a mismatch is confirmed, add the failing regression first and make
  the smallest production fix.

## 5. Report and delivery

- [ ] Write the audit report with overlap, exclusions, counterexample ledger,
  bounded statistics, and resource measurements.
- [ ] Run focused tests, full workspace tests, formatting, strict clippy, Lean
  build, corpus checks, and diff checks.
- [ ] Obtain independent review with no Critical or Important findings.
- [ ] Push, create a PR closing #310, wait for required CI, and merge.

