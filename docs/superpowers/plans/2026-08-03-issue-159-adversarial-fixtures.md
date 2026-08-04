# Issue #159 Consumer-local Adversarial Fixtures

Parent: [#159](https://github.com/tokyogas-tech/hoimin/issues/159)

## Decision

The requested input classes are covered beside the code that consumes them. A shared fixture
crate would couple Python syntax, Git patch grammar, native filesystem objects, JSON events, and
SQLite schema eras without sharing any useful construction semantics. This audit therefore keeps
small analyzer tables in `analyzer::rust_tests`, private Git grammar generators in `target::git`,
native tree construction in workspace tests, and serialization constructors in their integration
suites.

Small private analyzer tests remain co-located under `src`; integration and end-to-end evidence
remains under `tests`, matching the repository's existing Rust test layout.

## Consumer-local audit matrix

| Consumer | Benign variant | Adversarial variants | Exact evidence | Prior work |
| --- | --- | --- | --- | --- |
| Analyzer | An ordinary operator-free single-quoted string | Ordinary strings containing each mutable lexical token (`==`, `!=`, ordering operators, membership/identity/boolean words, augmented/binary operators, control-flow words, booleans, signs, and `not`) | `ordinary_string_literals_ignore_every_mutable_operator_token` in [`rust_tests.rs`](../../../crates/hoimin-cli/src/analyzer/rust_tests.rs) asserts zero literal-content candidates and exact preservation of every byte surrounding a separate mutable expression | Interpolated-token defect [PR #171](https://github.com/tokyogas-tech/hoimin/pull/171) / #96 |
| Analyzer | Single- and double-quoted text with nested opposite quotes | Mutable tokens inside single, double, triple-single, and triple-double quoting | `nested_quote_pairs_preserve_every_surrounding_string_byte` in [`rust_tests.rs`](../../../crates/hoimin-cli/src/analyzer/rust_tests.rs) runs exact benign/adversarial pairs and reconstructs the only permitted trailing-expression mutation byte-for-byte | This task completes the ordinary/nested-quote row left after [PR #171](https://github.com/tokyogas-tech/hoimin/pull/171) |
| Analyzer | F-string and T-string literal content with an operator-free interpolation | Operator-bearing literal content plus mutable `+` and `is not` interpolation expressions in both flavors | `interpolated_string_pairs_skip_literals_and_retain_expressions` in [`rust_tests.rs`](../../../crates/hoimin-cli/src/analyzer/rust_tests.rs) requires zero literal candidates and the exact two expression candidates | [PR #171](https://github.com/tokyogas-tech/hoimin/pull/171) |
| Git zero-context diff | Generated ordinary printable added/removed lines | Content beginning `++`, `--`, `@@`, and `Binary files a/x.py and b/y.py differ` in every generated hunk | `target::git::tests::generated_hostile_zero_context_diffs_match_ranges` in [`git.rs`](../../../crates/hoimin-cli/src/target/git.rs) compares parser output with an independent range model. Example regressions remain in `changed_content_cannot_replace_the_diff_destination` and `changed_content_cannot_exclude_another_python_file` in [`target_handler.rs`](../../../crates/hoimin-cli/tests/target_handler.rs) | `++`/`--`: [PR #172](https://github.com/tokyogas-tech/hoimin/pull/172); binary ambiguity: [PR #196](https://github.com/tokyogas-tech/hoimin/pull/196); complete generated grammar: [PR #225](https://github.com/tokyogas-tech/hoimin/pull/225) |
| Workspace | `FixtureProject::new` creates two shallow regular UTF-8 files; `reset_restores_changed_and_deleted_files_and_removes_new_files` exercises normal reset | Real FIFO read/write/remove/reset, non-UTF-8 files/directories, exactly 128 nested directories, 129 excessive levels, and `TARGET.py`/`target.py` aliasing on a case-insensitive destination | [`workspace_handler.rs`](../../../crates/hoimin-cli/tests/workspace_handler.rs): `reading_a_fifo_returns_without_waiting_for_a_peer`, `writing_a_fifo_returns_without_waiting_for_a_peer`, `removing_a_fifo_returns_without_waiting_for_a_peer`, `reset_removes_a_fifo_without_waiting_for_a_peer`, `reset_removes_a_non_utf8_file_and_restores_manifest_content`, `reset_handles_a_tree_at_the_supported_depth`, and `reset_reports_a_depth_error_beyond_the_supported_depth`; [`workspace/copy.rs`](../../../crates/hoimin-cli/src/workspace/copy.rs): Windows-only `disk_snapshot_rejects_paths_that_alias_on_its_filesystem` | FIFO [PR #177](https://github.com/tokyogas-tech/hoimin/pull/177); depth [PR #178](https://github.com/tokyogas-tech/hoimin/pull/178); non-UTF-8 [PR #179](https://github.com/tokyogas-tech/hoimin/pull/179); collision [PR #210](https://github.com/tokyogas-tech/hoimin/pull/210) |
| Report JSON/JSONL | `RunStarted::minimal` in `jsonl_flushes_each_event_and_keeps_diagnostics_on_stderr`, plus the local `events()` baseline | Original/current schema-v2 JSON reports and complete JSONL streams with every era-representable optional populated | [`report_handler.rs`](../../../crates/hoimin-cli/tests/report_handler.rs): `original_schema_v2_report_fixture_matches_the_published_schema`, `current_report_golden_matches_typed_semantic_regeneration`, and `report_event_goldens_are_typed_complete_sequences`; artifacts under [`golden/reports`](../../../crates/hoimin-cli/tests/golden/reports) and [`golden/events`](../../../crates/hoimin-cli/tests/golden/events) | Verification-selection schema [PR #186](https://github.com/tokyogas-tech/hoimin/pull/186) / #149; complete corpus [PR #231](https://github.com/tokyogas-tech/hoimin/pull/231) |
| Session SQLite | Local `persist_request` supplies the ordinary current-schema persistence row with absent symbol and termination | Native SQLite v1/v2/v3 artifacts with symbol and output columns populated; v3 additionally populates termination kind and exit code | [`session_handler.rs`](../../../crates/hoimin-cli/tests/session_handler.rs): `golden_session_schema_eras_migrate_without_semantic_loss`, `current_session_golden_matches_semantic_regeneration`, `all_optional_session_rows`, and `expected_logical_rows`; artifacts under [`golden/sessions`](../../../crates/hoimin-cli/tests/golden/sessions) | Complete corpus [PR #231](https://github.com/tokyogas-tech/hoimin/pull/231) |

## All-optionals consumption audit

`all_optional_report_events(current)` is not accepted merely because it constructs values. The
JSON tests call `assert_report_optionals`, and the JSONL tests call `assert_event_optionals`, after
round trip. Together those assertions consume every optional representable in each era:

| Record | Optional fields proven non-null or era-absent |
| --- | --- |
| `RunStarted` | `normalized_config`; nested `selection.diff_base`, `output.metrics`, and `session`; current-era `verification_selection` |
| `MutationCandidate` | `symbol` |
| `MutantFinished` | `termination` and `output` |
| `RunSummary` | `counts.score`; current-era `verification_selection` |

The initially published schema-v2 artifacts correctly omit the later verification-selection
fields; every other optional above is populated and asserted in both original and current shapes.

`all_optional_session_rows(version)` is consumed by `expected_logical_rows(version)`, and
`assert_golden_session_rows` compares every selected SQL value with that expected vector both
before and after migration. The nullable-column audit is therefore complete:

| Table | Nullable columns | Era assertion |
| --- | --- | --- |
| `candidates` | `symbol` | Populated in v1, v2, and v3 |
| `results` | `output_token`, `output_retained`, `output_observed` | Populated in v1, v2, and v3 |
| `results` | `termination_kind`, `termination_exit_code` | Not representable and asserted null after migration for v1/v2; populated as `exit`/`7` in v3 |

## Reconciliation

Every row in #159 now has exact executable evidence. The only new code is the analyzer-local table;
Git, workspace, report, and session constructors remain with their sole consumers. No narrowed
follow-up is required, and the parent issue can be closed after this branch is reviewed and merged.

## Verification

```text
cargo test -p hoimin-cli --lib analyzer::rust::rust_tests
cargo test -p hoimin-cli target::git
cargo test -p hoimin-cli --test target_handler
cargo test -p hoimin-cli --test workspace_handler
cargo test -p hoimin-cli --test report_handler
cargo test -p hoimin-cli --test session_handler
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```
