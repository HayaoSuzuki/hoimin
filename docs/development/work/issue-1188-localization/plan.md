# Fault-localization evaluation implementation plan

> For agentic workers: use superpowers:executing-plans to execute this plan.

**Goal:** Measure the additional localization information and cost of mutation
reactions on reproducible Python failures.

**Architecture:** Keep pure analysis separate from pytest observation and bounded
experiment orchestration. Reuse hoimin plan without changing product semantics.

**Tech stack:** Python 3.14, pytest, Hypothesis, coverage.py, existing hoimin binary.

**Spec:** [Evaluation design](design.md).

## Global constraints

- One case per test invocation; parameterize parallel cases and use property-based
  tests for ranking invariants. Arrange, Act and Assert remain explicit.
- Five self-review passes at design, planning, implementation and testing stages.
- Use public-source evidence in committed reproduction artifacts.
- Do not weaken baseline, source-identity or resource contracts of the product.

## Review focus

1. Stable failing tests whose message changes but whose exit status does not.
2. Missing or reordered test identities and setup/teardown failures.
3. Equal scores, zero evidence and omission faults without a single faulty line.
4. Unicode byte spans, stale plans and restoration after timeout or invalid edits.
5. Incomplete sampling and measured costs that can make improved ranks impractical.

## Task 1: Pure analysis

Create analysis.py and test_analysis.py in this directory.
Interfaces: ochiai(failed, passed, total_failed), reactions(baseline, mutant,
mode), rank_intervals(scores, targets), and validation of repeated runs.

- [x] Write table-driven tests for hand-derived scores, fail-to-fail observations,
  identity mismatches, invalid phases and tie intervals; run RED.
- [x] Implement the minimal pure functions and run GREEN.
- [x] Add Hypothesis properties for permutation invariance, score bounds and
  monotonicity; run the complete analysis tests.

## Task 2: Collection and execution

Create collect.py, evaluate.py and test_execution.py here.
Input is an isolated project, selected pytest arguments, declared source files and
an existing hoimin plan. Output is complete or explicitly incomplete observations.

- [x] Write real subprocess tests for observation, phase failures and source
  mutation/restoration; run RED.
- [x] Implement collector and bounded candidate execution; run GREEN.
- [x] Verify failure fingerprints, Unicode spans, stale source rejection and
  timeout behavior with parameterized and property-based boundary cases.

## Task 3: Historical evaluation

Add fixed public-case metadata, reproduction instructions and evidence.

- [x] Verify fixed versions pass and buggy versions reproducibly fail.
- [x] Freeze candidate selection and run two repeats within declared budgets.
- [x] Compare traceback, coverage and two mutation observation models, including
  ties, invalid executions, omissions, truncation and end-to-end time.
- [x] Write the adoption decision from measurements, preserving limitations.

## Task 4: Verification and delivery

- [x] Record five implementation and five test review passes with findings/fixes.
- [x] Run research tests, repository Python tests and relevant documentation checks.
- [x] Obtain an independent branch review and resolve significant findings.
- [ ] Inspect staged content, commit, create a PR referencing #1188 and verify CI.
  Do not close the issue unless its research acceptance criteria are all met.

## Plan self-review

1. Dependencies: analysis consumes observations, not pytest objects; execution
   imports analysis, not the reverse. Fix missing outcome fields before collection.
2. Independence: tests use literal expected rankings and independently generated
   permutations, not the same formula as their expected-value oracle.
3. Reproducibility: pin sources, tests and candidate order before measurements;
   record rejected cases instead of silently substituting favorable results.
4. Failure boundaries: explicitly test process failure and invalid test sets before
   executing historical cases. Resource failure is not suspiciousness evidence.
5. Proportion: four deliverables, no generic benchmark platform or CLI schema.
   Preserve all unmet conditions and avoid claiming human productivity from ranks.
