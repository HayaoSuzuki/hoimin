# Issue 911 review and verification

Base: `64ac5038afa1396e7fe237d17dd82641dee25c56`.
Branch: `refactor/911-verification-selection`. Environment: macOS arm64.

## Design and plan

The [design](design.md) and [implementation plan](../../plans/2026-10-15-issue-911.md)
record five separate review passes each. The issue's existing approved scope was
used directly: validated selection metadata, preserved normal output and no
compatibility reconstruction.

## Test review: five passes

1. Boundary cases use public deserialization and independent JSON literals, not
   private enum layout. Before implementation, 12 of 15 tests failed assertions;
   the three valid-wire tests passed. Baseline report_policy: 32 passed.
2. Mode/policy and mode/scope mismatches are separate table-driven tests. Missing
   and null evidence are variants of the same evidence requirement.
3. Zero-request input also has selected=0, isolating positivity from the separate
   selected<=requested invariant. Non-sample selected=0 stays valid.
4. Proptest varies counts, truncation, all Top policies, seed and ID ordering.
   The persisted regression seed records a sample-count mismatch. Constructor
   output is compared with a literal wire contract separately from round trips.
5. Every new test separates Arrange, Act and Assert and has one contract focus.
   Schema tests first produced 10 failures / 8 passes, then 18 passes after fixing
   policy/scope constraints in the two current schemas.

## Implementation review: five passes

1. Ownership: all fields are private; constructors and Deserialize use the same
   validation. No caller can alter a count or evidence after construction.
2. Wire boundary: direct typed deserialization distinguishes omitted sampling
   from present-null sampling without a JSON intermediate or duplicate parse.
   Unknown outer metadata remains accepted; sampling retains its closed schema.
3. Consumers: Top keeps strict/file/line policies; explicit IDs retain normalized
   counts; sampling preserves its existing selection algorithm and dispatch order.
4. Output: serialization borrows IDs, keeps field names and omits non-sample
   evidence. Human/preview use typed strings instead of JSON round trips.
5. Scope and compatibility: no legacy adapter, schema renumbering or historical
   schema edits; current schemas now express the non-sample mode relationships.
   Cross-field count inequalities are enforced by Rust, not claimed as portable
   JSON Schema constraints. Ordered verification lifecycle remains in #912.

## Verification

Core with all features passed: 456 tests, zero failures, two existing ignored
release-performance probes (36 summaries including doctests). Command:
`cargo test --locked -p hoimin-core --all-features`. The guard recorded 20.3 s,
peak group RSS 673,021,952 bytes, no resource stop. All 19 new selection tests
passed, including five bounded proptest properties. Full workspace and configured Clippy hooks passed as recorded below. No new Lean model is needed for this
value-boundary refactor: algorithms and transition rules are unchanged. Existing
contract/oracle tests remain part of workspace validation. Python tests exercise
JSON Schema; production Python is unchanged, so hoimin has no changed Python
production target for this work.

## Independent design/code review

A separate read-only reviewer checked the final representation, consumers,
current schemas and new tests against the base commit and found no actionable
issues. The reviewer did not run tests. Seven observable philosophy diagnostics
pass (8.75/10); time allocated to design work was not measured, so the eighth
row cannot be claimed. Module purpose, invariant ownership and public comments
are explicit; adding a future selection mode does not require callers to rebuild
its wire-field relationships. This is a scoped assessment, not a whole-repository
score.


## Verification reviews

1. Compared baseline/RED/GREEN outputs. The pre-implementation failures were
   assertion failures about accepted invalid inputs. Final core includes all
   19 new tests, existing contract/oracle cases and report_policy coverage.
2. Rechecked schema acceptance independently with Draft 2020-12: all 18 new
   cases pass; 60 tests pass including neighboring changed-selection, outcome,
   session-mode and environment-snapshot schemas. Ruff check/format and ty pass
   for the new Python test. An initial ty diagnostic was fixed by annotating the
   public Validator protocol instead of the dynamically created validator class.
3. Parsed all 49 knowledge frontmatters and verified new source paths, hashes,
   unique IDs and footnotes. Catalog contains both new report-directory records.
   Source hashes are refreshed after final evidence edits.


Python/documentation follow-up: repository-wide Ruff format (55 files) and Ruff
lint passed. The first full ty run used the existing partial environment and
reported 24 diagnostics caused by missing documentation/provenance/tooling
dependencies. Created an independent temporary environment with
`uv sync --frozen --group fuzz --no-install-project`; full `ty check --python`
against that environment passed, without changing the existing root environment.
The new schema tests and docs-site tests passed together (34 tests, 2.07 s).


Full workspace: `cargo test --locked --workspace --all-features` passed
**2,999 tests**, zero failures, 29 existing ignored fixtures/manual performance
probes, across 161 summaries including doctests. No compiler warnings were
emitted. This includes plan/preview, progress additive metadata, JSON/JSONL/human
reporting, cancellation, sampling and existing Lean-derived oracle suites.
Guard: 388.8 s, peak process-group RSS 1,654,947,840 bytes, no resource stop.

4. Read every workspace summary and checked warning/error markers. No test was
   newly ignored; the existing report golden/preview and machine cancellation
   checks remain unchanged apart from typed fixture construction. The full
   suite validates consumers as well as the new isolated boundary tests.

5. Final scope/quality review: all four configured `prek run --all-files` hooks
   passed (workspace/vendor fmt and workspace/vendor Clippy). Checked the final
   diff for stale field access, accidental public compatibility shims, unrelated
   changes and documentation links. No unresolved review findings. The changes
   are verified on macOS arm64, not Linux/Windows. No wheel build, fresh Lean
   proof or exhaustive state-space claim is made.

All checks above were run on this branch's implementation. Design, plan, test,
implementation and verification each received five recorded self-review passes;
the independent review is additional. Branch documents accompany the code.
