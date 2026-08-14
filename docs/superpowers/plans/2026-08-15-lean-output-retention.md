# Bounded process-output retention Lean audit plan

## 1. Baseline and contract

- [x] Record focused collector, public-process, and Lean baselines.
- [x] Fix the correspondence boundary at the ordered chunk stream received by
  the collector.
- [x] Record the 35-byte marker, configurable Rust seams, and fault lifecycle.

## 2. Lean semantics and proofs

- [ ] Add an output-retention model with logical ring state, saturated counts,
  finalization, and first-error absorption.
- [ ] Prove ring/reference refinement, partition invariance, bounds, position,
  saturation, and error/drain invariants.
- [ ] Compile an external proof consumer under the audit resource guard.

## 3. Cases, sensitivity, and corpus

- [ ] Add strict, internal, model-only, and infrastructure-error cases with
  scenario-specific premises.
- [ ] Detect every required broken family with an independent minimized
  witness and bounded search ledger.
- [ ] Generate and freshness-check a closed JSONL corpus.

## 4. Rust correspondence

- [ ] Parse the exact corpus contract and reject schema, mode, ID, and premise
  drift.
- [ ] Replay success cases through `collect_output` and compare exact bytes and
  `OutputSpoolRef` fields.
- [ ] Cover public process boundaries and recorded receive-order observations.
- [ ] Add a narrow fault seam for first-error absorption and drain-to-EOF.
- [ ] For any same-premise mismatch, add the failing regression first and make
  the smallest production repair.

## 5. Report and delivery

- [ ] Write the audit report with exclusions, counterexample ledger, bounded
  statistics, and resource measurements.
- [ ] Run focused tests, full workspace tests, formatting, strict clippy, Lean
  build, corpus checks, and diff checks.
- [ ] Obtain independent review with no Critical or Important findings.
- [ ] Push, create a PR closing #311, wait for required CI, and merge.
