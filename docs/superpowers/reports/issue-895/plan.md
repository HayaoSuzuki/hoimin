# Issue 895 implementation plan

Execute the approved typed mutation outcome contract in this isolated main-based branch. Audience: an implementer who has not read the conversation. Read design.md first. Root issue 897 depends on the stable MutationOutcome/MutantResult names.

## Tasks

1. Baseline: run the existing core resume policy target with debug=0, incremental=0, jobs=1, test threads=2, dedicated target issue-895. Record the actual result before production changes.
2. RED: add focused one-concern AAA tests for rejection of flat MutantFinished wire, then typed outcome property tests for status derivation and wire round trips. Observe behavioral red and missing-API compile red separately.
3. GREEN core: introduce MutationOutcome and ReusedMutationStatus in resume.rs, replace MutantResult/MutantFinished parallel fields, require ProcessFinished.output_state, bump report schema to 6. Remove unreachable contradictory-result validation and migrate ReportSequence to structural lifecycle checks. Keep classification in one location.
4. GREEN consumers: machine.rs emits/stores owned outcomes and never infers state from diagnostic codes. Progress/review/survivors derive status/elapsed/output from the enum. Preserve summary/reuse and synthetic event behavior.
5. GREEN persistence: session/schema.rs version 5 stores outcome_json rather than nullable evidence columns and independent status. session/mod.rs writes/decodes that enum and derives StoredResult status. Retain transaction atomicity and diagnostic checks. Add current session golden; reject versions 1 through 4 unchanged.
6. Migrate fixtures/tests mechanically where the old representation had a valid equivalent. Replace contradiction tests with independent strict-wire rejection cases. Preserve report lifecycle/formal semantics by rejecting unrepresentable raw input at the boundary and updating the oracle projection or model/corpus only where the explicit contract changes. Add current report golden; preserve old files.
7. Run focused core, machine, session, artifact, and property tests. Review implementation five distinct times. Commit a stable, focused-verified implementation for root issue 897 while whole-workspace verification continues.
8. Verify fmt, clippy workspace/all-target warnings denied with default features and contracts when feasible, full workspace tests with bounded threads, Python corpus/docs tests, full Python suite, docs site, relevant Lean generator/corpus/sensitivity checks. Inspect each output, record environmental failures precisely, and do not repeat broad green suites without a new reason.
9. Update relevant canonical docs/knowledge, catalog/source hashes, report/session schema reference and migration guidance. Perform five actual verification review passes, record scoped diagnostic scores, commit final docs/fixes. Parent publishes with gh Git Data API; create and attach PR after exact remote ref confirmation. Parent watches CI; no merge.

## Plan review passes

1. Dependency trace found report validators called from both sequence and artifact readers. Added explicit removal/migration of those consumers instead of leaving a vacuous validator.
2. Session contract trace found contracts-only RawResult decoding. Added contracts-feature checking and typed decoding to persistence task.
3. Test inventory found mutation report assertions spread across many CLI integration targets. Separate mechanical migration from new one-concern assertions to avoid silently weakening existing behavior.
4. Formal inventory found report-sequence and process-output oracle adapters. Added boundary/raw-input projection review; keep unchanged properties and label any changed contract instead of claiming equivalence for invalid old states.
5. Resource/release review found parallel issue builds and downstream dependency. Added debug-zero isolated target/thread bounds and a focused-verified commit checkpoint before full verification, without claiming completion prematurely.

## Test design

Use property tests for arbitrary exit codes, elapsed/output evidence round trips, and independent diagnostic codes. Use parameterized cases for finite termination/output-state and synthetic/reuse mappings. Each new or rewritten test checks one concern and has Arrange, Act, Assert blocks. SQLite malformed rows and report schema rejection are boundary tests; no generated OS scheduling promises. Existing subprocess and corpus suites cover policy equivalence rather than duplicating implementation.
