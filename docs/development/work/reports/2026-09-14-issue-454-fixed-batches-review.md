# Issue 454 fixed-batch review

Base 8b33167; reviewer Codex; worktree .worktrees/issue-454.

## Design self-review

1. Mapped issue acceptance to range semantics: strict saved order and diverse global order must be distinguished. Slicing before diverse would reorder each batch; explicitly rejected it.
2. Checked selection metadata and progress inputs. Existing actual candidate IDs identify the set; introducing an offset schema field is unnecessary. Document invocation retention and separate progress histories.
3. Reviewed bounds: max_mutants applies after skipping; saturated addition cannot overflow; empty/out-of-range fails while end-clipped suffix and truncated plans retain current top semantics.

## Plan self-review

1. Checked enum consumers: retain Top construction for existing library callers, add TopRange and handle both exhaustively.
2. Checked helper dependencies and tests: independent expected 120 IDs and a hand-authored diverse table avoid reconstructing expectations with the implementation.
3. Checked empty/overflow/error paths precede baseline and cannot raise inherited limits. The plan calls out root timing/resource/fingerprint preservation and end-to-end reports.

## Implementation self-review

1. Traced clap selection construction and every enum consumer. Existing Top callers remain unchanged; explicit offset becomes TopRange. Offset requires top and conflicts with candidate IDs before plan I/O.
2. Checked the numeric proof against Rust operations: an in-range offset and positive count produce a positive prefix; saturating addition is clamped to retained length before allocation. Out-of-range skips return no IDs and resolution diagnoses them before baseline.
3. Traced diverse across score tiers and uneven file groups. The existing global order is computed before skipping, so the second batch does not restart file rotation. max_mutants applies to the final suffix, while report requested remains the batch count.

## Test self-review

1. The first parser-to-preparation regression failed with unknown --offset; implementation made the 120-site partition pass for both policies. The expected union comes from all saved IDs, and exact diverse slices use a hand-written table rather than the selection helper.
2. Added exact-end, past-end, usize::MAX offset/count, truncated suffix, inherited max_mutants overflow and illegal CLI combinations. Real CLI reports prove IDs for two distinct batches and a repeated batch; progress checks their common set.
3. Independent review found unbounded subprocess output waits in the new integration. Replaced them with Tokio 30-second deadlines and kill-on-drop. Clippy found format-collect in fixture construction; changed it to writeln into one String. These corrections change the harness, not selection expectations.

## OKF self-review

1. Read existing selection contract and progress documentation. Added one section to that concept; did not create another batch catalog or alter old provenance.
2. Matched the new design and review source IDs to footnotes and indexes. The validator checks YAML and local links separately from the three content reviews.
3. Read the final Japanese contract against the implementation: global diverse order, zero-based offset, positive top, retained/truncated scope, selected-size limit and separate histories agree. Report IDs identify actual selection; commands retain offset/policy, not a new report schema field.

## PR self-review

1. Mapped all six issue acceptance conditions to the design, implementation, partition regression and real report/progress test. No limit override or implicit whole-plan completion is claimed.
2. Inspected the final file list for unrelated runtime changes, generated build files and the local .venv symlink. Only selection, tests, README and requested documents belong in the commit.
3. Reviewed the PR body with concrete verification and remaining limits. Dedicated target results are local macOS evidence; existing ignored fixtures and unrun native/wheel/full-workspace lanes are separate. Publication is recorded in the batch ledger after checking the returned URL/head.

## Independent review

The issue-467 agent reviewed range construction, overflow, global diverse slicing, final batch limit and report/progress identity. No correctness blocker found. Its subprocess-deadline finding was applied and the changed regression rerun.

## Verification

Rust 1.98.0/macOS, dedicated worktree target with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`.

- Initial fixed_batch integration: expected unknown --offset failure before implementation.
- New fixed_batch integrations: 3 passed.
- Global diverse slicing unit: 1 passed.
- Full plan suite: 58 passed, 1 ignored subprocess fixture.
- CLI all-target/all-feature Clippy: passed after fixture formatting correction.
- OKF: PyYAML parses 19 pages; 757 local links, source IDs/footnotes, changed hashes and full source indexes pass.

No production Python changed; mutation testing is not applicable. Full workspace, wheel, new Lean proof and Linux/Windows execution were not run locally. The selection helper may retain the bounded global prefix temporarily; this change does not claim constant memory independent of plan size.
