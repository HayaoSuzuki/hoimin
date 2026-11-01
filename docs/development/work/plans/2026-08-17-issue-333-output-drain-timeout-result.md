# Issue #333: Output drain timeout result implementation plan

**Goal:** Turn an output-pipe close timeout after known mutant termination into
one durable error result without cancelling remaining mutants.

**Architecture:** Preserve process termination and output completeness as
separate core event fields. The process boundary degrades only the narrow
known-mutant/close-timeout case; the machine owns status and diagnostic
classification.

**Tech stack:** Rust, Tokio, serde, Lean 4, Cargo tests, hoimin mutation tests.

## Task 1: Specify the closed decision table in Lean

**Files:**

- Add: `formal/HoiminOracle/HoiminOracle/ProcessOutputModel.lean`
- Add: `formal/HoiminOracle/HoiminOracle/ProcessOutputProofs.lean`
- Add: `formal/HoiminOracle/HoiminOracle/ProcessOutputCases.lean`
- Add: `formal/HoiminOracle/ProcessOutputAuditMain.lean`
- Add: `formal/HoiminOracle/corpus/process-output-outcome.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

1. Model execution kind, process outcome, output outcome, fatality, result
   status, termination preservation, diagnostics, and continuation.
2. Prove the narrow degradation and fatal-preservation properties.
3. Add broken-family sensitivity checks and a closed strict case set.
4. Generate and check the committed JSONL corpus.

## Task 2: Add failing Rust correspondence tests

**Files:**

- Add: `crates/hoimin-core/tests/lean_process_output_oracle.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-cli/src/process/mod.rs`

1. Parse and validate every strict oracle row at the real CLI combiner.
2. Assert baseline identity, public classification, public diagnostic
   provenance, report-sequence validity, and two-mutant machine continuation.
3. Update combiner tests for mutant degradation, baseline fatality, primary
   failure precedence, and unrelated output failure fatality.
4. Run the focused tests and record that they fail before production changes.

## Task 3: Implement the result boundary

**Files:**

- Modify: `crates/hoimin-core/src/event.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/src/report.rs`
- Modify: `crates/hoimin-cli/src/process/mod.rs`

1. Add the defaulted `ProcessOutputState` and include it in
   `ProcessFinished`.
2. Preserve the initial spool reference as the bounded fallback.
3. Use `mutant_id`, not worker presence, to degrade only a known mutant plus
   `process.output.close.timeout` to `CloseTimedOut`; keep every other failure
   path unchanged.
4. Produce `MutationStatus::Error` and the stable output-timeout diagnostic in
   the machine while retaining the known termination.
5. Carry the output state and diagnostic through public `MutantFinished`
   reporting, render the diagnostic to stderr for human output, and enforce its
   code, level, mutant identity, known termination, error status, and fallback
   output in sequence validation.
6. Update all event fixtures explicitly.

## Task 4: Verify behavior and coverage

1. Run the Lean generator, corpus check, sensitivity checks, and Rust adapter.
2. Run focused process and machine regressions.
3. Run focused mutation testing for the combiner and machine result path.
4. Run formatting, clippy, the Rust workspace, Python tests, and wheel smoke.
5. Inspect the complete diff and request independent review.

## Task 5: Deliver and clean up

1. Commit the design, plan, oracle, implementation, and tests.
2. Push the issue branch and open a PR that closes #333.
3. Merge only after review and all CI jobs succeed.
4. Fast-forward local main, rerun the focused regression, and remove the
   remote/local branch and issue worktree.
