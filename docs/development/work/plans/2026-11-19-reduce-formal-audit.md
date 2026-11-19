# Reduce specification formal audit plan

> **For agentic workers:** Use the repository's Lean audit workflow. This task creates verification artifacts before product implementation; it does not implement `reduce`.

**Goal:** Audit #1172 for specification defects and generate deterministic pre-implementation expectations from Lean.

**Architecture:** A cheap imported model and kernel-checked proofs define the decision contract. A separate executable explores bounded cases, retains broken-model witnesses, and generates a versioned JSONL oracle. All cases are `model-only` until an actual reduce entry point exists.

**Tech Stack:** Repository-pinned Lean 4.32.2, Std, existing process-tree resource guard and Lean CI.

**Spec:** GitHub #1172, retrieved at task start; a snapshot and digest are retained with the audit report.

## Global constraints

- Product Rust/Python implementation is outside this task. No source-deletion runner or fake production adapter.
- Keep agreed rules separate from proposed resolutions of ambiguous rules.
- No `sorry`, added axioms, unbounded heartbeats or exhaustive evaluation in imported modules.
- One guarded Lean command at a time, initially 20 seconds / 2048 MiB; record cost and bounds.
- Work in the current clean workspace; preserve unrelated changes and do not commit or publish product code.

## Review focus

- A kept nested definition can be erased by removing its ancestor.
- Equal counts or subset matches do not establish equality of ID-indexed outcomes.
- Certificates must belong to the exact variant, plan, environment and timeout.
- Exact threshold/deadline boundaries and incomplete repeated measurements must be explicit.
- Tentative candidates must never replace certified artifacts, including after interruption or replay.

## Tasks

- [x] Freeze Issue text and write correspondence worksheet before modeling. Mark every case `model-only`; list no-implementation correspondence explicitly.
- [x] Add a failing Lean audit entry, then define executable predicates for validation, exact result preservation, timing and protected spans. Check fixed positive/negative examples.
- [x] Add a certification/state-transition model; prove all-trace artifact safety and best-certificate invariants. Retain minimal witnesses for broken atomicity, identity and boundary rules.
- [x] Enumerate a small boundary domain in a dedicated executable. Generate JSONL, verify freshness, sensitivity and deterministic regeneration; record resource use.
- [x] Add guarded CI build/corpus checks. Write the self-contained report, reproduction commands, counterexample ledger and future adapter contract.
- [x] Re-run verification, inspect the diff, and report proved scope, specification questions and unimplemented correspondence separately.

## Execution ledger

- Start: working tree clean; Issue #1172 fetched successfully. Lean toolchain and resource guard already present. No production reduce adapter exists.
- Ruling: retain all new oracle rows as `model-only`; no fabricated strict run against `run`/`verify`, which cannot configure the new reduce premises.

- Complete: 19 kernel-checked theorems, 810 generated model-only rows, depth-3 exploration (259 traces / 726 transitions), 12 detected broken variants, freshness and deliberate stale-copy rejection.
- Ruling: C1–C3 remain specification clarifications for owner review; the oracle labels proposed numeric/deadline cases and does not rewrite the agreed Issue.
- Infrastructure: full pre-existing library build exceeded the 2048 MiB process-tree guard and was stopped; no memory-limit increase or full-build retry. Removed unnecessary root imports and verified a consumer importing only the new proof module.
- Review: no product reducer or fake adapter added; corpus generated only by Lean; CI freshness/sensitivity registered; report links and git diff whitespace checked.

- Supplement authorized: user requested C1–C3 completion, software-design-philosophy evaluation and PR creation. Accepted protection-by-span, strict percentage domain, observed-zero-time stop, and deadline-first equality.
- Supplement verification: 22 theorems, 952 rows, seven-event depth 3 (400 traces / 1134 transitions), 14 detected broken variants; red-to-green clarification examples. Product reduce remains out of scope.
