# Task 3 — #976 review source membership index

Base: `1fb0409bf34fa2ebce9053c4111552116178e7e1`. Authoritative requirements: task-3-brief.md and `/tmp/hoimin-design-audit-5d3aee95/impl-976.json` (read in full). Approved design; implementation decisions authorized by controller. TDD and verification-before-completion skills and writing-good-tests reference read. Controller reserves the full integrated suite for Task 10; this task runs focused checks only.

## Requirements/design self-review (five passes)

1. Reachability: actual synchronous `plan::read_for_review` calls `validate_header`, canonicalizes project, then checks stable ID and scans sources for every candidate. Existing `ValidationStats` covers `validate_requested_descriptors`, a different verify path, and cannot establish this regression.
2. Ordering: header validates source uniqueness first. Preserve header and project canonicalization ordering and per-candidate ID-before-source diagnostics; do not prevalidate all candidate IDs or reorder candidate records.
3. Ownership/complexity: borrowed `BTreeMap<&Utf8Path, &str>` local to the reader avoids path/hash/payload clones. A single construction visits each source once; queries use path identity consistent with existing Camino path equality.
4. Regression oracle: real reader test will serialize a valid manifest with 256 candidates sharing the last of 257 sources, then assert exact candidate payload/order preservation independently of source visits and query counts. Test-only thread-local counters follow existing review join instrumentation. RED must expose repeated source visits, not a wall-clock threshold.
5. Compatibility/property: separate missing-record, mismatched-hash, duplicate-path, bad-ID and mixed-error precedence cases exercise production reader. Bounded generated source priorities permute records while fixed candidate order and values remain unchanged. First/last bounded public CLI probe is observational only; no generalized performance claim.

## TDD RED

`env CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --offline --locked -p hoimin-cli --lib plan::review_tests:: -- --nocapture`

Direct log: `/tmp/hoimin-design-audit-5d3aee95/task-3-red.log`. Exit 101; 9 passed, 1 failed. The actual reader work regression failed at the source-visits assertion: 65,792 visits (256 × 257), expected 257. Candidate payload/order equality had already passed. All nine semantic controls passed, including the 32-case source permutation property. This was a behavioral failure after successful compilation, not a fixture failure.

## Implementation self-review (five passes)

1. Data ownership: map keys use `source.path.as_path()` and values use `source.hash.as_str()`, both borrowed from the decoded manifest. No String or candidate payload clone; map is local and gone before manifest ownership is returned.
2. Construction boundary: exactly one `.collect()` builds the source map immediately after `validate_header`; existing duplicate-record validation precedes it, preventing silent overwrite acceptance. Header record validation itself is unchanged.
3. Candidate validation: original loop and candidate descriptor/stable ID computation remain in original order; each successful ID check is followed by one map lookup and exact hash comparison. Missing and inconsistent records retain their existing error category/message.
4. Canonicalization: project root canonicalization still precedes all candidate errors; manifest path canonicalization/content hash and returned manifest are unchanged. Index uses Camino path ordering/equality rather than text-key conversion.
5. Instrumentation and scope: cfg(test) thread-local counters count actual source visits and candidate lookup sites in production reader. No altered public signature, runtime cache/config framework, schema, adapters or candidate sorting. All new tests are under the focused plan review test module; property has 32 cases with 2–32 source records and fixed eight candidates.

## Verification commands and evidence

Every Cargo and commit command uses `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. Commands ran serially in the existing worktree/target; no dependency changes. `cargo fmt --all` completed successfully (`task-3-fmt.log`).

| Command (following resource env) | Direct log | Result |
| --- | --- | --- |
| `cargo test --offline --locked -p hoimin-cli --lib plan::review_tests:: -- --nocapture` | task-3-green.log | 10 passed; exit 0 |
| `cargo test --offline --locked -p hoimin-cli --features contracts --lib plan:: -- --nocapture` | task-3-plan.log | 57 passed, 3 ignored; exit 0 |
| `cargo test --offline --locked -p hoimin-cli --features contracts --lib review:: -- --nocapture` | task-3-review.log | 78 passed; exit 0 |
| `cargo test --offline --locked -p hoimin-cli --features contracts --test plan --test review_cli --test lean_survivor_review_oracle -- --nocapture` (initial) | task-3-cli.log | Lean oracle 1 passed; plan 104 setup failures, 1 ignored; exit 101; review_cli not reached |
| Same integration command after preparing ignored `.venv` symlink | task-3-cli-prepared.log | Lean oracle 1 passed, plan 104 passed/1 ignored, review_cli 17 passed; exit 0 |

The initial integration failure is environmental: all 104 cases assert at tests/plan.rs:2805 before test execution because the worktree lacked `.venv/bin/python`. Systematic-debugging skill read; inspection traced the required path in `python_executable()`, confirmed the main repo environment exists and imported pytest, then created an ignored worktree `.venv` symlink to `/Users/hayao/RustroverProjects/hoimin/.venv`. Interpreter: CPython 3.14.7, pytest 9.1.1. No `uv sync`, pip install, or dependency/environment mutation was performed. Controller confirmed this reuse is appropriate. Only the affected integration command was rerun; passing unit checks were not repeated.

Initial setup-failure case names (each subsequently passed in task-3-cli-prepared.log):

```text
changed_context_discovers_neighbor_operators_and_survives_verify
    changed_plan_skips_an_untracked_fifo_with_a_bounded_subprocess
    changed_selection_plan_matches_the_real_run_candidate_and_id
    cli_verify_rejects_a_bom_counted_column_before_baseline
    create_plan_emits_versioned_manifest_without_runtime_side_effects
    exception_family_plan_preserves_candidate_descriptor
    explicit_class_symbol_ranks_and_verifies_its_method_before_an_unrelated_function
    fitting_candidate_record_limits_agree_for_plan_verify_and_direct_run
    fixed_batch_bounds_preserve_truncation_and_reject_invalid_ranges
    fixed_batch_partition_covers_120_candidates_without_overriding_limits
    fixed_batch_real_cli_reports_disjoint_ids_and_repeated_progress
    oversized_candidate_plan_fails_with_context_before_baseline
    oversized_candidate_record_limits_agree_for_plan_verify_and_direct_run
    oversized_legacy_candidate_is_rejected_before_verify_baseline
    plan_analyzer_timeout_dispatch_returns_two_without_a_manifest
    plan_candidate_limit_emits_partial_manifest_and_exit_four
    plan_candidates_match_shared_discovery_for_normalized_selectors
    plan_invalid_syntax_returns_two_without_a_manifest
    plan_newline_output_failure_preserves_complete_json_and_returns_two
    plan_oversized_analyzer_timeout_returns_two_without_panicking
    plan_preparation_failures_return_two_without_a_manifest
    plan_rejects_a_literal_backslash_source_without_rewriting_it
    plan_rejects_an_explicit_uncopyable_file_before_baseline
    plan_rejects_unapproved_best_effort_memory_before_project_work
    plan_serialization_output_failure_returns_two_without_execution
    plan_serializes_fingerprint_inputs_only_in_normalized_config
    plans_for_new_operator_families_pass_verify
    preparation_errors_precede_the_analyzer_deadline_without_baseline
    public_plans_preserve_bom_sources_and_python_columns
    python_physical_lines_match_plan_run_and_verify_selection
    root_selection_excludes_uncopyable_sources_in_plan_run_and_verify
    source_scoped_plan_preserves_candidates_as_unrelated_files_grow
    symbol_definition_existing_empty_scopes_are_valid
    symbol_definition_existing_unchanged_scope_is_valid
    symbol_definition_invalid_syntax_is_not_diagnosed_as_missing
    symbol_definition_missing_is_rejected_before_baseline
    symbol_definition_missing_is_rejected_with_clean_git
    symbol_definition_package_init_is_valid
    symbol_definition_requires_exact_definition_not_assignment_or_prefix
    symbol_definition_verify_checks_unrequested_symbol_files
    truncated_plan_accepts_a_contained_candidate
    truncated_plan_accepts_a_retained_candidate_from_a_later_file
    verified_metrics_override_preserves_checked_execution_and_preview
    verify_accepts_a_requested_candidate_from_a_later_file
    verify_accepts_generated_sequence_permutations_and_same_span_operators
    verify_analyzer_timeout_dispatch_stops_before_the_test_command
    verify_deduplicates_requested_ids_and_rejects_max_mutants_overflow
    verify_does_not_analyze_an_unrequested_file_after_source_record_validation
    verify_exact_file_ignores_nested_names_but_rejects_root_change
    verify_explicit_candidates_do_not_emit_budget_shortfall_warning
    verify_metrics_exports_baseline_failure_with_jsonl_output
    verify_metrics_exports_existing_sidecar_for_each_selection
    verify_metrics_exports_total_timeout_with_jsonl_output
    verify_metrics_is_not_written_when_plan_validation_fails
    verify_metrics_manifest_collision_is_also_rejected_with_owned_writers
    verify_metrics_manifest_collisions_reject_before_baseline_in_public_cli
    verify_metrics_manifest_distinct_entries_remain_replaceable
    verify_metrics_preserves_write_warning_and_source_collision_contracts
    verify_metrics_real_cli_uses_invocation_directory_and_preserves_results
    verify_preview_details_distinguish_same_line_mutations_in_all_formats
    verify_preview_details_escape_multiline_quoted_text_without_extra_rows
    verify_preview_explicit_ids_follow_discovery_not_rank_argument_or_saved_sequence
    verify_preview_invalid_inputs_match_normal_validation_before_runtime
    verify_preview_omitted_and_zero_offsets_select_the_same_policy_prefix
    verify_preview_output_failure_returns_error_without_execution
    verify_preview_public_cli_preserves_policy_range_and_execution_order_without_side_effects
    verify_preview_rejects_metrics_without_touching_existing_destination
    verify_preview_truncated_formats_owned_dispatch_and_runtime_boundary
    verify_prioritizes_invalid_normalized_config_over_requested_id_limits
    verify_ranking_large_bodies_preserves_public_preview_and_checks_unselected_entries
    verify_rejects_a_ranking_version_three_plan_before_baseline_with_regeneration_guidance
    verify_rejects_a_tampered_requested_symbol_after_scoped_rediscovery
    verify_rejects_absent_required_arrays_before_baseline
    verify_rejects_candidate_sequences_that_are_not_a_complete_permutation
    verify_rejects_changed_fingerprint_input_before_baseline
    verify_rejects_changed_source_before_baseline
    verify_rejects_duplicate_manifest_candidates_and_missing_requested_ids
    verify_rejects_fingerprint_input_deleted_after_preparation_before_execution
    verify_rejects_invalid_normalized_config_before_project_work
    verify_rejects_malformed_headers_and_source_paths
    verify_rejects_obsolete_selection_representation_before_project_work
    verify_rejects_previous_plan_schema_before_project_work
    verify_rejects_removed_outer_root_before_baseline
    verify_rejects_stale_lone_cr_coordinates_before_baseline
    verify_rejects_tampered_candidate_before_baseline
    verify_reports_source_change_before_ranking_for_changed_symbol_plan
    verify_runs_only_requested_candidates
    verify_stale_details_preserve_missing_fingerprint
    verify_stale_details_preserve_missing_symbol
    verify_stale_details_preserve_source_priority
    verify_stale_details_preserve_unreadable_fingerprint
    verify_stale_details_report_all_six_changes_before_tests_in_both_modes
    verify_top_above_a_truncated_plan_selects_every_retained_candidate
    verify_top_budget_shortfall_preserves_jsonl_stdout
    verify_top_budget_shortfall_warns_on_stderr_and_preserves_json_and_plan
    verify_top_diverse_does_not_cross_score_tiers
    verify_top_diverse_rejects_empty_plan_before_baseline
    verify_top_diverse_round_robins_equal_score_candidates_without_mutating_the_manifest
    verify_top_empty_plan_preserves_offset_error_precedence
    verify_top_executes_the_highest_ranked_retained_candidate
    verify_top_real_cli_reports_diverse_order_and_preserves_strict_rank_prefix
    verify_top_resolves_the_saved_rank_prefix_and_retained_scope
    verify_top_strict_rejects_empty_plan_before_baseline
    zero_changed_context_excludes_neighbor_operators
```

Ignored existing cases: `plan::sampling::sample_fixed_seed_frequency_inspection`, `plan::tests::benchmark_candidate_conversion_and_manifest`, `plan::validation_tests::benchmark_requested_descriptor_preprocessing`, and integration `changed_plan_fifo_child`. They remain descriptive/benchmark/subprocess-fixture exclusions; no new ignored tests.

Public CLI probe ran once after integration rebuilt target/debug/hoimin:

`python3 /tmp/hoimin-design-audit-5d3aee95/reproduce-review.py --binary /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-971-979-cli-analyzer-resource-safety/target/debug/hoimin`

Output: `/tmp/hoimin-design-audit-5d3aee95/task-3-review-after.jsonl`; stderr: task-3-probe.log (empty), exit 0. Binary SHA256 after `197f685cd44f5817be9fbbea8319ef8215cefa9c3239b6d4dd0c7e601055f619`; before `75274321743f745b2f2f4e0d8929b8a75b66b24cc293395d3f676573eab3fe59` (controller's task-3-review-before.jsonl).

| Candidates | Sources | Before first/last | After first/last |
| --- | --- | --- | --- |
| 2,000 | 1,001 | 0.1343s / 0.3162s | 0.1311s / 0.1311s |
| 2,000 | 10,001 | 0.2381s / 2.0671s | 0.2328s / 0.2306s |

Every command exited 0 with empty stderr and the same input byte counts (895,459 / 1,795,459). This is a single artificial bounded fixture observation, not a general performance claim or threshold regression. The structural work-count test supplies deterministic regression evidence.

## Verification self-review (five passes)

1. RED/GREEN validity: baseline source-visits assertion failed with 65,792 versus 257 after candidate values/order comparison succeeded; after indexing all 10 direct-reader tests pass, requiring exactly 257 source visits, 256 lookup calls, one map construction. No timing threshold is used.
2. Validation/control coverage: contracts-enabled plan suite passes 57 cases with three pre-existing ignored cases. Separate production-reader tests reject missing source, wrong hash, duplicate path with different hash, stable-ID mismatch, same-candidate ID/source errors, and earlier source error before a later ID mismatch.
3. Order/generated coverage: the 32-case bounded property permutes 2–32 unique source records and retains all eight candidate payloads, reversed sequence order and original source order exactly. Named first-source and late-source tests pass; no candidate reorder is introduced by index traversal.
4. Consumer/correspondence coverage: 78 review tests pass, including `matching_review_command_joins_once_and_keeps_plan_display_order`, source freshness/unavailability and report identity checks. All 104 prepared plan integrations and 17 review CLI scenarios pass; Lean review identity/history oracle passes. Initial missing-.venv setup failures and their resolution are explicitly retained above/by name in direct logs.
5. Resource/observational audit: one Cargo process at a time with all resource settings; offline/locked test builds, existing target and shared read-only interpreter. Public CLI binary checksum changed after rebuilding and all four first/last probe cases exit 0. Probe results do not establish average-project performance. Full-workspace integrated suite remains controller Task 10.

## Completion preparation self-review (five passes)

1. Acceptance audit: exactly one borrowed source index per reader invocation; deterministic large late-source regression, finite invalid-input cases, mixed-error ordering, generated source permutations and downstream CLI/join/freshness checks all covered.
2. Scope audit: source edits restricted to `crates/hoimin-cli/src/plan.rs` and new focused `crates/hoimin-cli/src/plan/review_tests.rs`. No schema/CLI/dependency/current-doc/shared-plan/ledger edits; Task 10 owns maintained evidence updates.
3. Ownership/compatibility audit: no candidate/path/hash payload clone added for indexing, no reusable cache framework; project and manifest canonicalization/content hashing retained, original candidate loop retained, duplicate paths still rejected in unchanged validate_header.
4. Packaging audit: this report and the prepared `.venv` symlink are ignored. Report is not force-added. Only the two Rust files will be staged; direct logs remain in /tmp/hoimin-design-audit-5d3aee95. Mandatory hooks are left enabled; commit inherits all four Cargo resource variables.
5. Verification/claim audit: 257 nonduplicated focused contracts-enabled tests passed, four existing ignored cases. No unexpected production failures or unresolved architectural questions. Initial setup-only failures are retained and resolved. `git diff --check` passes; mandatory fmt/Clippy hooks and staged-file audit are the final commit checks.

## Commit and final completion audit (five passes)

Commit: `bd7463b107676457bb6fbb1001caaecb7b131ffa` — `fix(review): index plan source membership once`.

1. Mandatory hook evidence: `/tmp/hoimin-design-audit-5d3aee95/task-3-commit-hooks.log` records all four hooks passing (workspace/vendor fmt and workspace/vendor Clippy), followed by successful commit; hooks were not disabled or skipped. All four required Cargo resource variables were inherited by hooks.
2. Permission audit: initial ordinary commit was denied by sandbox at worktree `index.lock` before hooks (`task-3-commit.log`). The authorized Git commit was then run with escalation and succeeded; no automatic approval rejection or unresolved permission remained.
3. Commit scope audit: `git show --format= --name-only HEAD` lists only `crates/hoimin-cli/src/plan.rs` and `crates/hoimin-cli/src/plan/review_tests.rs`. No transient report, `.superpowers` file, shared `.venv`, shared plan or ledger was committed.
4. Final state audit: `git status --short` is empty; `git diff HEAD --check` succeeds. `git check-ignore` confirms this report and `.venv` are ignored and preserved locally. Production source and focused tests are unchanged since successful verification; no repeat suite is needed for report-only updates.
5. Handoff audit: status DONE. Contracts-focused verification is 257 passed / four existing ignored; direct regression RED/GREEN recorded; public CLI bounded probe output and both binary checksums retained. Full integrated tests/current maintained docs remain explicitly assigned to controller Task 10. No remaining concern for this task's narrow platform-independent indexing change.


## Controller measurement correction after Task4

The initial after-change CLI above came from contracts-enabled integration, whereas the before-change binary used default features. The structural work-count regression and task review are unaffected, but use the following aligned-feature observations for numerical comparison. The controller built the default-feature CLI at `4e99249c` using the same debug/profile/resource environment, then reran the same bounded reviewed script. Build log: `/tmp/hoimin-design-audit-5d3aee95/task-3-default-build.log`; observations: `task-3-review-after-default.jsonl`. Binary SHA-256: `6aa787c66c09c31d4fa206a827e80d2c3379ba1e9f35523a29c5f03072128751`.

| Candidates | Sources | Matching source | Before seconds | Aligned after seconds |
| ---: | ---: | --- | ---: | ---: |
| 2,000 | 1,001 | First | 0.1343 | 0.1499 |
| 2,000 | 1,001 | Last | 0.3162 | 0.1420 |
| 2,000 | 10,001 | First | 0.2381 | 0.2618 |
| 2,000 | 10,001 | Last | 2.0671 | 0.2616 |

All four final subprocesses exited 0 with empty stderr. These are single observations, not speed guarantees or timing gates; index construction adds work even for a first-position match. The deterministic evidence remains one borrowed index build, one source visit per source and one lookup per candidate. No production/test change or suite rerun was needed for this build-feature correction.
