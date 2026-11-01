# Task 6: coupled execution selection (#998)

## Change and scope

`RunSelection::{all, explicit, ordered}` checks execution IDs against immutable
`VerificationSelection` before a run exists. A private All/Explicit/Ordered enum
owns membership, explicit matched history, ordered Collecting/Ready state and
provenance. Explicit/Top permit zero selected IDs. Explicit and Top count unique
IDs; Top preserves the first occurrence's order. Sample execution IDs must
exactly equal its nonempty unique evidence, including order, seed and population.

`RunState::with_selection` consumes this owner during initialization. Scheduling,
missing-ID detection, budget gating/projection and both report endpoints use it.
`RunState::new` remains the ordinary All convenience. The unchecked filter
constructors and later provenance setter are removed. The ordered stop merge
still uses the original position search; Task 7 owns its optimization.

`VerifiedPlan` keeps its protected configuration/input boundary and owns the
checked selection directly. `ResolvedVerifySelection`, shell `CandidateSelection`
and the separate metadata handoff are removed. Preview reads order and provenance
from the same owner. Callers inspect `plan.selection().candidate_ids()`,
`ordered_ids()` or `verification_selection()`; the redundant standalone
`VerifiedPlan::verification_selection` passthrough is removed.

This corrects representable API inconsistencies, not a reproduced CLI failure.
There is no report wire/schema, SQLite, dependency or CLI-option change. Current
guides and OKF reconciliation remain assigned to Task 8; API comments are updated.

## Requirements review

1. Re-read issue #998 and the accepted specification. Distinguished API design debt from the already-correct production CLI construction path; no claim that existing CLI output was wrong.
2. Enumerated mode combinations. All has no metadata argument; Explicit admits only CandidateIds; Ordered admits only Top/Sample. Every metadata mode is matched exhaustively at construction.
3. Checked count and sampling boundaries separately. Counts describe deduplicated membership, requested counts stay unchanged, Top/Explicit selected=0 remains valid, Sample IDs/order must exactly agree with valid nonempty unique evidence.
4. Traced behavioral requirements through existing tests: explicit streaming, requested Top/Sample order, post-restoration budget observation, missing IDs, duplicate discoveries, stopped collection, late completions and exactly-once NotRun accounting.
5. Checked exclusions and ownership boundaries: retain VerifiedPlan, no compatibility adapters or schema/storage changes, no Task 7 rank algorithm, no broad workspace test rerun. All in-tree old constructor/setter/type callers were migrated.

## Design review

1. Selected one checked value with one private runtime enum instead of a checked wrapper around independently stored execution and metadata. RunState has one selection field; no post-initialization public mutation exists.
2. Retained semantically distinct data: membership index for lookup, requested-order vector for replay, explicit matched history for duplicate suppression/missing detection, sample seed/population for provenance. No persistent rank map was introduced.
3. Moved OrderedCandidateState and its Collecting/Ready ownership transfer into the focused selection module without changing the phase semantics or candidate transfer algorithm. The machine retains effect scheduling and worker transitions.
4. Constructed RunSelection at plan resolution, then consumed it once through VerifiedExecution and shell. Removed independent shell mode matching and the former core metadata-mode budget inference. Preview borrows order and metadata instead of accepting a separate provenance parameter.
5. Reviewed interface depth using software-design-philosophy. Initial boundary score 5/10: failed simple interface, hidden decisions/change locality, abstraction comments and newcomer boundary clarity. The new selection boundary scores 10/10 across the eight diagnostics: one purpose, checked constructors hide invariants, implementation changes are internal, comments state normalization/errors, design review is recorded, mode ownership is hidden, handoff is explicit, and design work is the task itself. No failed diagnostic remains for this scoped boundary; the pre-existing large machine/test files are not restructured beyond the selection responsibility.

## Implementation review

1. Checked every constructor path against the metadata invariant. Mode checks precede count checks; Sample raw IDs are compared before Top-style normalization so duplicates cannot be silently accepted as sample evidence.
2. Compared membership and missing-ID behavior before/after. Explicit inserts matched IDs only when selected, suppresses duplicate execution, and checks sorted missing membership. Ordered discovery still replaces repeated records by ID, checks missing IDs in request order, and retains Ready semantics.
3. Followed budget/report data flow. Ordered presence gates the observation after baseline restoration; the checked metadata supplies selected count. RunStarted and RunFinished clone the same immutable owner. All never obtains verification metadata.
4. Read the stop path diff independently. Candidate extraction, stable sorting, sequence fallback, retirement, synthetic outputs and accounting are unchanged; only the source of owned ordering moved. `selection.ordered_ids()` remains available for Task 7's temporary rank index.
5. Checked API removal, error style and lints. Repository-wide Rust call-site search found no old filter constructors, setter or split shell/plan types. Lint review found API-comment formatting and an unnecessary constructor expect; the constructor now returns a SampleIdsMismatch instead of exposing a panic path. Removed the redundant plan metadata forwarding method rather than adding a new panic contract or compatibility wrapper.

## Tests review

1. Added constructor tests before implementation: the initial RED is a missing-API compile failure, explicitly not a runtime reproduction of a CLI bug. Subsequent real guard-removal checks demonstrated five behavioral failures when count/Sample agreement validation was bypassed; production source was restored automatically before GREEN.
2. Used named macro-generated cases for finite mode/count/order/empty dimensions, with literal IDs and independent expected errors. Added positive explicit duplicate normalization preserving requested count, Top stable dedup, valid Sample seed/population/order and empty Top/Explicit controls.
3. Replaced invalid All+Top/Explicit fixture shortcuts with actual selected IDs. Ordered fixtures now acknowledge their mandatory budget effect. Explicit duplicate spool discovery is tested against two literal dispatch IDs and exactly two killed outcomes.
4. Added four named All/Explicit/Top/Sample dispatch/report agreement cases and separate Sample cancellation/deadline cases. Expected orders are literal m1,m2 versus m2,m1; stopped reports retain original provenance, correct exit codes and two NotRun results. Both Top and Sample budget timing are tested around baseline restoration.
5. Reviewed integration evidence: existing adversarial state-machine schedules, missing-ID and stop/late-completion cases pass; targeted real CLI Top, Sample, Explicit, preview and protected-plan tests pass. Compile-fail docs exercise All-with-metadata rejection and VerifiedPlan protection. No source-text assertion or wall-clock threshold was added.

## Verification evidence

All Cargo commands and hooks use the same resource environment:

```sh
export CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export RUST_TEST_THREADS=2 PREK_CONCURRENCY=1 CARGO_NET_OFFLINE=true
```

- Initial RED: `cargo test -p hoimin-core --test run_selection`; E0432 for absent RunSelection/RunSelectionError before implementation. Log: `/tmp/hoimin-issues-992-998/task6-red.log`.
- Behavioral guard check: temporarily bypassed the two selected-count checks and Sample ID/order comparison, then ran the same target. Five named cases failed (explicit/ordered deduplicated counts; Sample different IDs, order, duplicates), eight controls passed. A shell EXIT trap restored the exact source. Log: `task6-validation-guard-red.log` under the same evidence directory. This is a regression sensitivity check, not the initial TDD RED.
- GREEN: `cargo test -p hoimin-core --test machine --test run_selection --test verification_selection --test budget_projection --test lean_top_budget_projection_oracle`: 110 + 13 + 19 + 8 + 2 = 152 passed, no failures or ignored cases. Log: `task6-core.log`.
- `cargo test -p hoimin-cli --test plan verify_top`: 11 passed. Covers real CLI order, retained/truncated scope, JSON/JSONL shortfall warnings and error precedence. Log: `task6-cli-top.log`.
- `cargo test -p hoimin-cli --test sampling`: 8 passed, 1 existing ignored authored-project evaluation; includes public Lean sampling oracle, preview/execution agreement, failed baseline and timeout provenance. Log: `task6-cli-sampling.log`.
- Five exact `--test plan` cases passed: `verify_runs_only_requested_candidates`, `verify_deduplicates_requested_ids_and_rejects_max_mutants_overflow`, `verify_explicit_candidates_do_not_emit_budget_shortfall_warning`, `verify_preview_explicit_ids_follow_discovery_not_rank_argument_or_saved_sequence`, `verified_metrics_override_preserves_checked_execution_and_preview`. Logs: `task6-cli-{explicit,dedup,explicit-budget,preview,verified}.log`.
- `cargo test -p hoimin-cli --lib explicit_candidate_run_rejects`: 2 passed (missing ID and empty spool). Log: `task6-cli-missing.log`.
- `cargo test -p hoimin-core --doc RunSelection` and `cargo test -p hoimin-cli --doc VerifiedPlan`: 1 + 2 compile-fail tests passed. Logs: `task6-{core,cli}-doc.log`.
- `cargo clippy -p hoimin-core -p hoimin-cli --all-targets --all-features -- -D warnings`: passed after the documented lint corrections. Log: `task6-clippy.log`. `cargo fmt --all` and `git diff --check` passed.

No native-platform claim is made by these pure-core and host CLI checks. The
existing ignored sampling evaluation is a measurement writer, not a failed test.
Commit-hook outcomes and commit identity are recorded in the task report.
