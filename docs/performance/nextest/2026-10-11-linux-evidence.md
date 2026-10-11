# Linux nextest evaluation evidence

Captured from the successful Rust job in [run 38113472996](https://github.com/HayaoSuzuki/hoimin/actions/runs/38113472996/job/114393883068).
The overall trial CI failed its two existing workflow-equality tests; this is not an all-CI success claim.
Commands, GNU time output, sampled resources, inventory counts/ignored identities and log hashes follow.
Raw logs, full inventory and JUnit were uploaded as artifact 11693095696 with seven-day retention.

```json
{
  "run_url": "https://github.com/HayaoSuzuki/hoimin/actions/runs/38113472996",
  "job_url": "https://github.com/HayaoSuzuki/hoimin/actions/runs/38113472996/job/114393883068",
  "run_attempt": 1,
  "head_sha": "1bd6866524b3d6c4e7d26a67f36814567362a704",
  "base_sha": "00893465e0328204453e5008dc3884035b85c7bd",
  "artifact_id": 11693095696,
  "artifact_name": "nextest-evaluation-linux",
  "environment": "rustc 1.99.0 (b940084d7 2026-09-28)\nbinary: rustc\ncommit-hash: b940084d7eb6a299eb4bfeb8e34901bc051e7ac4\ncommit-date: 2026-09-28\nhost: x86_64-unknown-linux-gnu\nrelease: 1.99.0\nLLVM version: 23.1.1\ncargo-nextest 0.9.148 (cd1d6d546 2026-10-07)\nrelease: 0.9.148\ncommit-hash: cd1d6d5467322dbc0d39a80cb671d000ac302798\ncommit-date: 2026-10-07\nhost: x86_64-unknown-linux-gnu\n4\nRUST_TEST_THREADS=libtest default\n",
  "install_seconds": 1,
  "build_gnu_time": "\tCommand being timed: \"cargo test --workspace --locked --no-run --message-format json\"\n\tUser time (seconds): 229.12\n\tSystem time (seconds): 26.67\n\tPercent of CPU this job got: 211%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 2:00.91\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 1714148\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 10831\n\tMinor (reclaiming a frame) page faults: 8800516\n\tVoluntary context switches: 65117\n\tInvoluntary context switches: 13182\n\tSwaps: 0\n\tFile system inputs: 8\n\tFile system outputs: 10960000\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
  "inventory": {
    "tests": 2785,
    "binaries": 145,
    "ignored": 24,
    "sha256": "6b899df73d86c8ebb94dd23782195a62003c7883587f3f38e30b8ae4ee48128c",
    "nextest_list_sha256": "dbdf347d91ff6462991394065076774688563e36c25d7ba046f4bdf6838fcbd6",
    "ignored_identities": [
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "analyzer::rust::rust_tests::benchmark_adversarial_ast_fact_indexes",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "analyzer::rust::rust_tests::benchmark_candidate_line_positions",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "analyzer::rust::rust_tests::benchmark_long_single_line_candidate_columns",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "plan::sampling::sample_fixed_seed_frequency_inspection",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "plan::tests::benchmark_candidate_conversion_and_manifest",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "plan::validation_tests::benchmark_requested_descriptor_preprocessing",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "process::tests::root_child_fixture_waits",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "resource::portable::unix_tests::rlimit_cpu_probe_fixture",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "shell::tests::benchmark_blocking_io_dispatch",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "target::fs::tests::measure_exact_file_discovery_scaling",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "workspace::copy::tests::benchmark_worker_materialization_io",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "workspace::mutation::tests::benchmark_original_manifest_work_per_mutant_cycle",
        true
      ],
      [
        "hoimin-cli",
        "lib",
        "hoimin_cli",
        "workspace::reset::tests::benchmark_workspace_reset_io",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "plan",
        "changed_plan_fifo_child",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "rust_analyzer",
        "rust::rust_tests::benchmark_adversarial_ast_fact_indexes",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "rust_analyzer",
        "rust::rust_tests::benchmark_candidate_line_positions",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "rust_analyzer",
        "rust::rust_tests::benchmark_long_single_line_candidate_columns",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "sampling",
        "sample_authored_project_evaluation",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "session_handler",
        "ownership_child_fixture_holds_run",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "valid_python_corpus",
        "rust::rust_tests::benchmark_adversarial_ast_fact_indexes",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "valid_python_corpus",
        "rust::rust_tests::benchmark_candidate_line_positions",
        true
      ],
      [
        "hoimin-cli",
        "test",
        "valid_python_corpus",
        "rust::rust_tests::benchmark_long_single_line_candidate_columns",
        true
      ],
      [
        "hoimin-core",
        "lib",
        "hoimin_core",
        "target::tests::measure_explicit_file_lookup_scaling",
        true
      ],
      [
        "hoimin-core",
        "lib",
        "hoimin_core",
        "target::tests::measure_explicit_range_scaling",
        true
      ]
    ],
    "per_binary_counts": [
      {
        "package": "hoimin-cli",
        "kind": "lib",
        "binary": "hoimin_cli",
        "tests": 804
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "abstract_set_provider",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "analysis_depth",
        "tests": 9
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "analyzer_handler",
        "tests": 41
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "augmented_to_assignment",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "baseline_output",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "boundary_selection",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "builtin_evaluation_order",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "changed_excluded_paths",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "changed_git_scope",
        "tests": 7
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "cli_config",
        "tests": 59
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "collection_annotation_builtins",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "comprehension_named_bindings",
        "tests": 15
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "condition_clause_delete",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "condition_constant",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "container_element_delete",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "conversion_call_remove",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "declaration_only_annotations",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "default_operator_selection",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "deferred_annotation_imports",
        "tests": 13
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "diagnostic_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "disk_shutdown",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "disk_workspace",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "enum_member_replace",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "exception_hierarchy",
        "tests": 22
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "exception_hierarchy_scopes",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "fingerprint_env",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "fingerprint_env_plan",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "fingerprint_input_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "fingerprint_inputs",
        "tests": 31
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "function_body_erase",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "function_body_return_constant",
        "tests": 9
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "import_roots",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "integer_literal_neighbor",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_annotation_scope_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_binding_flow_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_bounded_candidate_discovery_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_candidate_ranking_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_candidate_span_oracle",
        "tests": 7
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_changed_context_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_changed_lines_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_changed_target_oracle",
        "tests": 7
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_compound_pattern_guard_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_disk_shutdown_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_environment_fingerprint_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_except_star_flow_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_exception_hierarchy_oracle",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_exception_match_binding_oracle",
        "tests": 8
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_glob_selection_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_implicit_finally_oracle",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_line_diverse_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_multiple_handler_join_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_nested_match_exit_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_nested_try_flow_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_nullable_gate_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_paging_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_prepared_annotation_import_oracle",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_prepared_namespace_oracle",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_private_annotation_import_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_progress_decision_oracle",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_progress_input_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_result_lifecycle_oracle",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_resume_diagnostic_oracle",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_schema_migration_oracle",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_session_oracle",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_shutdown_oracle",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_source_order_oracle",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "lean_workspace_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "line_diverse",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "manifest_buffer_lifetime",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "mapping_pattern_keys",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "method_call_remove",
        "tests": 7
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "method_replacement_selection",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "metrics_destinations",
        "tests": 25
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "missing_source",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "negative_neighbors",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "nullable_builtin_provenance",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "operator_function_contracts",
        "tests": 18
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "optional_keyword_delete",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "plan",
        "tests": 88
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "process_handler",
        "tests": 34
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "progress",
        "tests": 92
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "progress_compare_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "progress_details_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "progress_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "progress_jsonl_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "ranking_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "report_handler",
        "tests": 31
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "report_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "resume_budget",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "resume_diagnostics",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "return_tuple_swap",
        "tests": 3
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "run_e2e",
        "tests": 76
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "rust_analyzer",
        "tests": 300
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "sampling",
        "tests": 9
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "selection_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "session_handler",
        "tests": 36
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "source_discovery",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "source_encoding",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "source_encoding_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "statement_delete",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "string_literal_empty",
        "tests": 4
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "string_segment_empty",
        "tests": 9
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "target_handler",
        "tests": 50
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "telemetry",
        "tests": 7
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "type_parameter_bindings",
        "tests": 8
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "valid_python_corpus",
        "tests": 283
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "while_condition_false",
        "tests": 5
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "windows_resource_scope",
        "tests": 0
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "with_suppression",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "workspace_empty_directories",
        "tests": 6
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "workspace_handler",
        "tests": 40
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "workspace_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "workspace_preflight_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "workspace_recovery",
        "tests": 24
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "workspace_stream",
        "tests": 2
      },
      {
        "package": "hoimin-cli",
        "kind": "test",
        "binary": "workspace_stream_heap",
        "tests": 1
      },
      {
        "package": "hoimin-cli",
        "kind": "bin",
        "binary": "hoimin",
        "tests": 3
      },
      {
        "package": "hoimin-core",
        "kind": "lib",
        "binary": "hoimin_core",
        "tests": 33
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "budget",
        "tests": 11
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "budget_projection",
        "tests": 8
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "candidate_policy",
        "tests": 18
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "config_json",
        "tests": 7
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "contracts",
        "tests": 2
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "disk_policy",
        "tests": 14
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_budget_oracle",
        "tests": 6
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_disk_guard_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_mutation_score_exit_policy_oracle",
        "tests": 4
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_oracle",
        "tests": 6
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_process_output_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_report_sequence_oracle",
        "tests": 5
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_timeout_limit_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "lean_top_budget_projection_oracle",
        "tests": 2
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "line_selection_index",
        "tests": 4
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "machine",
        "tests": 76
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "operator_selection",
        "tests": 16
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "plan_config",
        "tests": 18
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "report_policy",
        "tests": 40
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "report_resume",
        "tests": 1
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "resume_policy",
        "tests": 23
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "source_encoding",
        "tests": 8
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "source_encoding_properties",
        "tests": 2
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "target_policy",
        "tests": 31
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "telemetry",
        "tests": 14
      },
      {
        "package": "hoimin-core",
        "kind": "test",
        "binary": "workspace_effects",
        "tests": 4
      }
    ]
  },
  "measurements": [
    {
      "argv": [
        "cargo",
        "test",
        "--locked",
        "-p",
        "hoimin-cli",
        "--test",
        "run_e2e",
        "--test",
        "process_handler",
        "timeout",
        "--",
        "--show-output"
      ],
      "exit_code": 0,
      "wall_seconds": 15.967490193999993,
      "sample_interval_seconds": 0.2,
      "samples": 77,
      "sampled_peak_tree_processes": 8,
      "sampled_peak_tree_rss_kib": 185296,
      "name": "cargo-probe-1",
      "gnu_time": "\tCommand being timed: \"cargo test --locked -p hoimin-cli --test run_e2e --test process_handler timeout -- --show-output\"\n\tUser time (seconds): 1.58\n\tSystem time (seconds): 0.27\n\tPercent of CPU this job got: 11%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 0:15.95\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 50832\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 20\n\tMinor (reclaiming a frame) page faults: 60326\n\tVoluntary context switches: 2307\n\tInvoluntary context switches: 92\n\tSwaps: 0\n\tFile system inputs: 16952\n\tFile system outputs: 1208\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
      "log_sha256": "5c2cbd33274b8d9e5badb60045e89320bca7a82db9418b49838cd50c1ea9d3f9",
      "capability_skip_lines": [
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)"
      ]
    },
    {
      "argv": [
        "cargo",
        "nextest",
        "run",
        "--profile",
        "ci",
        "--locked",
        "-p",
        "hoimin-cli",
        "--test",
        "run_e2e",
        "--test",
        "process_handler",
        "timeout"
      ],
      "exit_code": 0,
      "wall_seconds": 20.31594457999995,
      "sample_interval_seconds": 0.2,
      "samples": 98,
      "sampled_peak_tree_processes": 8,
      "sampled_peak_tree_rss_kib": 126608,
      "name": "nextest-probe-1",
      "gnu_time": "\tCommand being timed: \"cargo nextest run --profile ci --locked -p hoimin-cli --test run_e2e --test process_handler timeout\"\n\tUser time (seconds): 1.48\n\tSystem time (seconds): 0.34\n\tPercent of CPU this job got: 9%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 0:20.23\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 106856\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 1\n\tMinor (reclaiming a frame) page faults: 90929\n\tVoluntary context switches: 2712\n\tInvoluntary context switches: 104\n\tSwaps: 0\n\tFile system inputs: 0\n\tFile system outputs: 1232\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
      "log_sha256": "c8183f1b6bf4114cd8f7e1685d955713dfe693cfb7c005abf74012470aa3e630",
      "capability_skip_lines": [
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)"
      ]
    },
    {
      "argv": [
        "cargo",
        "nextest",
        "run",
        "--profile",
        "ci",
        "--locked",
        "-p",
        "hoimin-cli",
        "--test",
        "run_e2e",
        "--test",
        "process_handler",
        "timeout"
      ],
      "exit_code": 0,
      "wall_seconds": 20.348847621000004,
      "sample_interval_seconds": 0.2,
      "samples": 98,
      "sampled_peak_tree_processes": 8,
      "sampled_peak_tree_rss_kib": 126240,
      "name": "nextest-probe-2",
      "gnu_time": "\tCommand being timed: \"cargo nextest run --profile ci --locked -p hoimin-cli --test run_e2e --test process_handler timeout\"\n\tUser time (seconds): 1.54\n\tSystem time (seconds): 0.33\n\tPercent of CPU this job got: 9%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 0:20.25\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 106836\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 0\n\tMinor (reclaiming a frame) page faults: 90491\n\tVoluntary context switches: 2695\n\tInvoluntary context switches: 84\n\tSwaps: 0\n\tFile system inputs: 0\n\tFile system outputs: 1232\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
      "log_sha256": "704611898c127e06c85eb4df382211c77cca171bec498e16f3273e44402c315b",
      "capability_skip_lines": [
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)"
      ]
    },
    {
      "argv": [
        "cargo",
        "test",
        "--locked",
        "-p",
        "hoimin-cli",
        "--test",
        "run_e2e",
        "--test",
        "process_handler",
        "timeout",
        "--",
        "--show-output"
      ],
      "exit_code": 0,
      "wall_seconds": 14.324770539999975,
      "sample_interval_seconds": 0.2,
      "samples": 69,
      "sampled_peak_tree_processes": 10,
      "sampled_peak_tree_rss_kib": 205320,
      "name": "cargo-probe-2",
      "gnu_time": "\tCommand being timed: \"cargo test --locked -p hoimin-cli --test run_e2e --test process_handler timeout -- --show-output\"\n\tUser time (seconds): 1.60\n\tSystem time (seconds): 0.25\n\tPercent of CPU this job got: 13%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 0:14.14\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 50428\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 0\n\tMinor (reclaiming a frame) page faults: 60985\n\tVoluntary context switches: 1437\n\tInvoluntary context switches: 96\n\tSwaps: 0\n\tFile system inputs: 0\n\tFile system outputs: 1208\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
      "log_sha256": "78afbe6932e008d1b7602b2e2fa4b4e7442bcda4a884bc50a01fa1612f61be4c",
      "capability_skip_lines": [
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)"
      ]
    },
    {
      "argv": [
        "cargo",
        "test",
        "--workspace",
        "--locked",
        "--",
        "--show-output"
      ],
      "exit_code": 0,
      "wall_seconds": 146.11794433100005,
      "sample_interval_seconds": 0.2,
      "samples": 699,
      "sampled_peak_tree_processes": 10,
      "sampled_peak_tree_rss_kib": 404232,
      "name": "cargo-workspace",
      "gnu_time": "\tCommand being timed: \"cargo test --workspace --locked -- --show-output\"\n\tUser time (seconds): 175.55\n\tSystem time (seconds): 40.87\n\tPercent of CPU this job got: 148%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 2:26.09\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 368092\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 1880\n\tMinor (reclaiming a frame) page faults: 8653633\n\tVoluntary context switches: 227388\n\tInvoluntary context switches: 60867\n\tSwaps: 0\n\tFile system inputs: 4944\n\tFile system outputs: 2396512\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
      "log_sha256": "82ecb62294b6fb544d1f64ec828458ef603a1f81a53b32ccc510b703785ed520",
      "capability_skip_lines": [
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)"
      ]
    },
    {
      "argv": [
        "cargo",
        "nextest",
        "run",
        "--workspace",
        "--locked",
        "--profile",
        "ci"
      ],
      "exit_code": 0,
      "wall_seconds": 151.099335161,
      "sample_interval_seconds": 0.2,
      "samples": 726,
      "sampled_peak_tree_processes": 13,
      "sampled_peak_tree_rss_kib": 244756,
      "name": "nextest-workspace",
      "gnu_time": "\tCommand being timed: \"cargo nextest run --workspace --locked --profile ci\"\n\tUser time (seconds): 149.97\n\tSystem time (seconds): 47.13\n\tPercent of CPU this job got: 130%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 2:31.07\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 170732\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 556\n\tMinor (reclaiming a frame) page faults: 11646116\n\tVoluntary context switches: 238873\n\tInvoluntary context switches: 21483\n\tSwaps: 0\n\tFile system inputs: 0\n\tFile system outputs: 2401344\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
      "log_sha256": "e489d1c371399cc672248c686d5a296ffe7d2a4ee246a98cdc497c60da04ee66",
      "capability_skip_lines": [
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)",
        "SKIP: Linux cgroup v2 hard-limit capability unavailable: failed enabling delegated controllers with `+memory +pids`; existing subtree_control is ``; enable delegated cgroup controllers failed: Permission denied (os error 13)"
      ]
    },
    {
      "argv": [
        "cargo",
        "test",
        "--workspace",
        "--locked",
        "--doc"
      ],
      "exit_code": 0,
      "wall_seconds": 0.8293739179999875,
      "sample_interval_seconds": 0.2,
      "samples": 4,
      "sampled_peak_tree_processes": 3,
      "sampled_peak_tree_rss_kib": 353060,
      "name": "doctests",
      "gnu_time": "\tCommand being timed: \"cargo test --workspace --locked --doc\"\n\tUser time (seconds): 0.63\n\tSystem time (seconds): 0.09\n\tPercent of CPU this job got: 100%\n\tElapsed (wall clock) time (h:mm:ss or m:ss): 0:00.73\n\tAverage shared text size (kbytes): 0\n\tAverage unshared data size (kbytes): 0\n\tAverage stack size (kbytes): 0\n\tAverage total size (kbytes): 0\n\tMaximum resident set size (kbytes): 367976\n\tAverage resident set size (kbytes): 0\n\tMajor (requiring I/O) page faults: 0\n\tMinor (reclaiming a frame) page faults: 22888\n\tVoluntary context switches: 184\n\tInvoluntary context switches: 7\n\tSwaps: 0\n\tFile system inputs: 0\n\tFile system outputs: 24\n\tSocket messages sent: 0\n\tSocket messages received: 0\n\tSignals delivered: 0\n\tPage size (bytes): 4096\n\tExit status: 0\n",
      "log_sha256": "3747a43e7f8b51ffb58ae3f9706002fc744d96201868e46336bda4b2c7052746",
      "capability_skip_lines": []
    }
  ],
  "checkout_merge_sha": "096c48216966b59a392a022747cc8df937bd3230",
  "runner_image": "ubuntu-24.04 / 20261004.327.1",
  "python_version": "3.14.8"
}
```
