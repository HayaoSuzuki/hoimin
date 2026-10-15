# Issue #900 review record

For reviewers checking the implementation and verification evidence.

## Implementation review passes

1. Constructor inventory: inspected every `DiskFailure` construction and migrated core lifecycle, machine tests, CLI monitor, shell join failures, and post-drain fixtures. Removed the unused test label import exposed by compilation.
2. Consumer inventory: found three separate optional-payload conversions; centralized secondary conversion in the typed reason and report conversion in `From<DiskFailure>`, removing the fallback message for impossible missing data.
3. Test concern review: found the first generated test mixed wire round trips with evidence mapping; split those properties into separately named tests and added a finite report projection table.
4. Wire review: found old-format rejection alone did not check mixed fields or required payloads; added missing/null-payload cases for every variant and a typed-reason-plus-code rejection case.
5. Contract/documentation review: found serialization and report projection have different contracts; documented the intentional saved failure break separately from stable report fields and left the formal label model unchanged. Added local knowledge-page metadata and a development-guide link so maintainers can discover the contract.

Baseline limit: the initial baseline command was still compiling when the first red test was added. It executed all 14 pre-existing policy tests successfully, then stopped at the intended regression failure before running the oracle target. An untouched full-workspace baseline was not captured in this worktree.

TDD evidence: before production changes, `legacy_disk_failure_wire_is_rejected` failed its `result.is_err()` assertion (14 existing policy tests passed). Then the generated typed API contract failed with missing `DiskFailureReason`, `code()`, and `to_secondary`. After implementation, 16 policy tests, 4 guard oracle tests, and 77 machine tests passed. Added focused review coverage is included in final verification.

Proptest applicability: arbitrary Unicode messages, full-width byte counts, and measurement nanoseconds exercise payload/evidence fidelity. The four finite variants and malformed wire classes use table-driven checks. Existing lifecycle and formal oracle suites cover stop priority, first-stop retention, and cleanup obligations.


## Verification review passes

1. Payload/evidence fidelity: the existing sticky-primary monitor assertions compared labels after migration; strengthened them to compare complete typed reasons. Added a generated lifecycle property where only measurement duration changes, checking exact secondary deduplication.
2. Formal correspondence: reviewed both Rust oracle projections; they deliberately extract `reason.kind()` because Lean models finite labels. Lean proofs/generator build and sensitivity checks succeed. Initial byte check exposed 22 CRLF line endings; generated and checked-in JSON values and LF-normalized bytes are identical. A checkout-only LF normalization makes the formal `--check` succeed; restored original checkout bytes afterward.
3. Documentation execution: first site build used an external output directory and correctly failed the repository destination guard. Reran with required `--output site`; it succeeds. Boundary adapter and docs tests: 56 passed, 1 skipped.
4. Environment isolation: full Rust fixtures need the worktree `.venv`; created a native directory junction to the existing main environment rather than changing fixture tests. All Cargo operations use the issue-900 target and one job; no other targets or worktrees are cleaned.
5. Whole-project verification: workspace clippy (`--workspace --all-targets -- -D warnings`) succeeds. Full Rust and Python suites are in progress; final command results and any platform skips/failures are recorded below before commit.

Documentation diagnostic: 10/10, no failed rows after adding reader/scope, verified variant and saved-format contracts, and executable verification commands. Scoped design diagnostic: Seven of eight diagnostic rows were evaluated and passed (8.75/10 equivalent); no evaluated row failed. The time-allocation row is unmeasured; record design/review and implementation time in a future run to evaluate it. The payload/evidence and report mappings each have one owner and no compatibility facade.

Full Python suite: 987 passed, 32 skipped in 253.03 seconds.


## Parent review

The parent independently reviewed production payload/code conversion, monitor and report semantics. It found complete duration equality preserved in lifecycle/monitor handling and the report projection unchanged. Its test review requested a stricter one-concern boundary in the added validation checks. Split required-payload rejection from independent-code rejection, and split code derivation from generated evidence conversion. Wire round trips, payload validation, code validation, report projection, and duration deduplication each have separate test functions with Arrange/Act/Assert blocks. Existing legacy tests retain their established scope.


A second independent reviewer (#899 implementer) found no actionable production issues. Equality checks materialize an owned secondary value, including cloning error message/code strings. This failure-path allocation tradeoff keeps the mapping in one place; normal continuing observations do not take this path.

The parent completed the shared development environment with the already-pinned corpus dependencies after the first Python run. A second full Python run covers that environment; its final result supersedes the earlier skip count below.

Final full Python environment run: 990 passed, 29 skipped in 323.02 seconds.

OKF completion review found source footnotes, source catalog entries, and index reachability missing from the first documentation draft. Added those records with hashes of the actual files read; moved reusable Cargo commands to the development guide and linked to them from the contract. Existing historical targets remain unchanged.


## Full-workspace retry evidence

The first `cargo test --workspace` compiled successfully but stopped after 65 target summaries (1,236 passed tests) at `lean_result_lifecycle_oracle::result_lifecycle_oracle_correspondence`, scenario `stop_preserves_accepted`. Its Windows adapter sets a fixed 1-second total timeout; the actual run timed out before either mutant executed, while the expected premise requires the first killed result to have been accepted. The Unix adapter instead guards the durable-result premise. This case does not traverse a disk failure in the observed trace.

The unchanged selected case then passed with bounded test threads (1.21 seconds). The full suite is rerunning with `--no-fail-fast -- --test-threads=2` and `RUST_TEST_THREADS=2` to finish the unexecuted targets and retain every failure. No product code or fixture timeout was changed in response. The initial full run used one build job and default test-thread parallelism; the retry caps runtime parallelism explicitly.

Final clippy (`--workspace --all-targets -- -D warnings`) passed after the completed implementation and test review. OKF structure validation passed for all 49 knowledge Markdown files. Final full-suite results follow below.


## Final verification result

- `cargo test --workspace --no-fail-fast -- --test-threads=2`: exit 0; 2870 passed, 27 ignored across 155 target/doctest summaries on Windows. `RUST_TEST_THREADS=2` and one build job were set. The initial fixed-timeout failure and unchanged successful reproduction remain recorded above.
- `disk_policy` includes 23 tests: separate wire round trips, required payload rejection, independent code rejection, code derivation, evidence conversion, report payload projection, secondary retention, and duration-sensitive deduplication.
- Final workspace clippy with all targets and warnings denied: exit 0. Formatting and whitespace checks pass.
- Full Python suite after the shared pinned CI dependencies were completed: 990 passed, 29 skipped. Documentation tests: 16 passed; site build succeeds.
- Lean disk guard generator/proofs build, unchanged normalized-byte corpus check, and sensitivity checks pass. Existing Rust disk guard and shutdown corpus adapters execute in the full suite. No new theorem about Rust or OS cleanup is claimed.
- OKF YAML/reserved-file validation covers 49 knowledge Markdown files. Added source references, footnotes, hashes, catalog entries, and design-index reachability are checked against actual files.

The checks run on Windows; platform-gated Linux/macOS behavior is left to CI. The initial baseline limit and Windows timing-sensitive fixture remain explicit evidence limits.
