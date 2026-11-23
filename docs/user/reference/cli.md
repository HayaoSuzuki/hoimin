<!-- Generated; do not edit by hand. -->

# CLI reference

Source: [Rust CLI definitions](/crates/hoimin-cli/src/cli.rs).

Regenerate with `cargo run --locked -p hoimin-cli --example generate_cli_reference`; append ` -- --check` to verify without writing.

See the [usage guide](/docs/user/guides/usage.md) for examples, OS-specific resource limits,
plan/verify inheritance and validation beyond Clap. Conditional requirements
and numeric parser bounds have no general stable Clap reflection API; their
help text and the manual guide remain authoritative. The metadata below lists
reflected arity, repetition, required arguments/groups, conflicts and delimiters.

## hoimin

```text
Bounded mutation testing for focused Python changes

Usage: hoimin <COMMAND>

Commands:
  run       Run mutation tests for an explicitly selected target
  plan      Discover mutation candidates without executing tests
  verify    Execute one or more candidates using settings from a previously generated plan
  progress  Compare chronologically ordered mutation run reports
  review    Record and inspect human assessments independently of test outcomes
  reduce    Search and validate whole-definition test reductions without applying them
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

  -V, --version
          Print version

To select mutation targets, operators and resource limits, run:
  hoimin run --help

To discover candidates before executing tests, run:
  hoimin plan --help
```

Argument constraints:

```text
--help
  required: false
  arity: 0
  repeatable: false
--version
  required: false
  arity: 0
  repeatable: false
```

## hoimin run

```text
Run mutation tests for an explicitly selected target

Usage: hoimin run [OPTIONS] [-- <TEST_ARGV>...]

Arguments:
  [TEST_ARGV]...
          Test executable and arguments, passed directly without a shell

Options:
      --root <DIR>
          Project root used to resolve relative paths

          [default: .]

      --source <DIR>
          Source root containing mutation targets; may be repeated

      --import-root <DIR>
          Worker import directory relative to --root; repeat in precedence order without selecting targets

      --file <PATH>
          Select a complete Python file; may be repeated

      --line <PATH:START-END>
          Select PATH:START-END (an inclusive line range); may be repeated

      --symbol <MODULE:QUALNAME>
          Select MODULE:QUALNAME; may be repeated

      --changed
          Restrict targets to changed Git lines

      --changed-context <N>
          Include N neighboring lines around Git changes (0..=1073741823)

          [default: 0]

      --diff-base <REV>
          Compare changed lines with the merge-base of REV and HEAD

      --include <GLOB>
          Include a normally ignored path while copying; may be repeated

      --fingerprint-include <GLOB>
          Add a root-relative file glob to the session fingerprint; may be repeated

      --fingerprint-file <PATH>
          Add one exact root-relative file to the session fingerprint; may be repeated

      --fingerprint-env <NAME>
          Track inherited NAME before worker rewriting; repeatable ASCII [A-Za-z_][A-Za-z0-9_]* (Unix case-sensitive, Windows uppercase)

      --exclude <GLOB>
          Exclude a path while copying; may be repeated and wins over include

      --operators <OPERATORS>
          Select mutation operator IDs or selector families; repeatable or comma-delimited.

          Omit --operators to use the default runtime set. An explicit selection replaces that set. Selecting a family includes all its listed members, including opt-in IDs. --exclude-operators is applied afterward.

      --exclude-operators <EXCLUDE_OPERATORS>
          Exclude mutation operator IDs or selector families; repeatable or comma-delimited.

          Remove the named IDs or every member of the named families after the default or explicit --operators selection. At least one operator must remain selected.

      --profile <PROFILE>
          Candidate-selection profile

          [default: full]
          [possible values: full, focused]

      --jobs <JOBS>
          Maximum concurrently active workers

          [default: 1]

      --max-mutants <MAX_MUTANTS>
          Maximum mutants to execute

          [default: 100]

      --max-candidates <MAX_CANDIDATES>
          Maximum candidates to discover before mutation starts

          [default: 10000]

      --baseline-timeout <DURATION>
          Baseline test timeout

          [default: 60s]

      --mutant-timeout <DURATION|auto>
          Per-mutant timeout or `auto`

          [default: auto]

      --total-timeout <DURATION>
          Run time budget, including environment capture, fingerprint reads, and execution setup.

          Verify starts this budget after saved-plan validation and reanalysis. Preparation consumes execution time; shutdown uses one additional two-second grace. An OS operation already in progress may finish later while retaining its resource owners.

          [default: 5m]

      --max-memory <BYTES>
          Memory limit (Windows: per root process tree, committed memory; jobs multiplies total allowance)

          [default: 1GiB]

      --max-output <BYTES>
          Retained stdout and stderr per process

          [default: 1MiB]

      --max-copy-size <BYTES>
          Run-wide logical copy-size limit

          [default: 1GiB]

      --max-workspace-size <BYTES>
          Run-wide logical size of Hoimin-owned workspaces, including generated files

          [default: 8GiB]

      --min-free-space <BYTES>
          Mandatory minimum available bytes preserved on every owned-workspace filesystem

          [default: 10GiB]

      --max-processes <MAX_PROCESSES>
          Process limit (Windows: per root process tree, including the root; jobs multiplies total allowance)

          [default: 64]

      --allow-best-effort-memory
          Permit best-effort memory enforcement when hard limits are unavailable

      --metrics <PATH>
          Write performance metrics to PATH

      --session <PATH>
          `SQLite` session path; no database is created unless specified

      --resume
          Resume the newest compatible incomplete run in the session database

  -h, --help
          Print help (see a summary with '-h')

Operator examples, applicability and limits for released hoimin 0.15.0:
  https://github.com/HayaoSuzuki/hoimin/blob/v0.15.0/docs/user/guides/usage.md#mutation-operators
  Source builds: read docs/user/reference/operators.md in the same checkout.

Default runtime IDs:
  aggregate_sum_count, assignment_target_swap, async_iterator_stop_insert, augmented_add_sub
  augmented_bitwise_and_or, augmented_bitwise_shift, augmented_bitwise_xor, augmented_floor_mod
  augmented_matmul, augmented_mul_div, augmented_power, augmented_to_assignment
  augmented_to_binary, await_remove, base64_validate_toggle, base_order_shuffle
  batched_strict_toggle, binary_add_sub, binary_div_floor, binary_floor_mod
  binary_matmul, binary_mul_div, binary_power, bisect_boundary_swap
  bitwise_and_or, bitwise_invert, bitwise_shift, bitwise_xor
  boolean_and_or, boolean_literal, break_continue, bytes_literal_empty
  call_argument_swap, call_result_default, collection_any_all, collection_append_insert
  collection_list_tuple, collection_min_max, collection_set_add_discard, collection_set_frozenset
  collection_set_remove_discard, collection_string_split_rsplit, collection_string_starts_ends, compare_direction
  compare_eq_ne, compare_order, comparison_not_implemented, comprehension_filter_invert
  comprehension_filter_remove, conditional_branch_swap, context_exit_omit, context_exit_result_negate
  continue_remove, conversion_call_remove, dataclass_factory_share, decimal_context_precision
  decimal_quantum_shift, decimal_rounding_mode_swap, deepcopy_to_copy, del_remove
  deque_popleft_to_pop, dict_unpack_delete, dict_unpack_order_swap, else_suite_remove
  encoding_errors_ignore, encoding_errors_replace, encoding_errors_strict, exception_cause_omit
  exception_cause_remove, exception_handler_swap, exception_type_pair, filter_predicate_swap
  finally_unwrap, float_literal_neighbor, fstring_conversion_swap, fstring_format_option_swap
  fstring_format_spec_remove, fstring_format_value_neighbor, function_body_return_constant, gather_return_exceptions
  generator_empty, generator_return_insert, generator_skip_first, heap_pushpop_replace
  identity, identity_equality_swap, integer_literal_neighbor, intermediate_rounding_remove
  iterator_stop_insert, itertools_take_drop_swap, json_allow_nan_toggle, lambda_capture_late
  list_comprehension_to_generator, loop_break_insert, map_strict_toggle, mapping_setdefault_to_get
  match_case_delete, match_case_swap, match_class_position_swap, match_guard_constant
  match_or_alternative_delete, match_pattern_wildcard, math_isclose_tolerance, math_precision_loss
  membership, method_call_remove, mutable_default_share, none_truthiness
  nonlocal_remove, operator_function, override_method_delete, range_bound_neighbor
  regex_alternative_delete, regex_ascii_mode, regex_character_set, regex_greediness_toggle
  regex_match_boundary, regex_quantifier_neighbor, remove_not, return_tuple_swap
  return_value_none, round_digits_remove, rounding_function_swap, set_union_intersection_swap
  sort_key_swap, statement_delete, statement_order_swap, statistics_sample_population
  stdlib_conversion_call_remove, stdlib_escape_remove, string_affix_strip, string_casefold_lower
  string_literal_empty, string_segment_empty, structure_append_extend, structure_index_neighbor
  structure_mapping_get_subscript, structure_slice_bound_move, structure_slice_bound_omit, structure_slice_neighbor
  structure_sort_reverse, structure_sorted_reversed, super_call_move, unary_sign
  unicode_digit_ascii, unicode_digit_predicate_swap, unicode_normalization_form, unicode_normalization_remove
  url_query_policy, url_quote_swap, while_condition_false, yield_from_to_yield
  yield_statement_remove, zip_strict_toggle

Opt-in IDs:
  condition_clause_delete, condition_constant, container_element_delete, enum_member_replace
  exception_bare_to_exception, exception_base_boundary, exception_exception_to_bare, exception_hierarchy
  exception_tuple_add_pair, exception_tuple_remove_member, function_body_erase, optional_keyword_delete
  type_dict_mapping, type_iterable_iterator, type_list_sequence, type_nullable_add
  type_nullable_remove, type_sequence_iterable, type_set_abstract_set

Selector families:
  bitwise_ops, call_contracts, collection_ops, collection_processing
  exception_ops, exception_risky, regex, rounding
  structure_ops, type_collections, type_iterables, type_nullable
```

Argument constraints:

```text
--root
  required: false
  arity: 1
  repeatable: false
  default: .
--source
  required: false
  arity: 1
  repeatable: true
--import-root
  required: false
  arity: 1
  repeatable: true
--file
  required: false
  arity: 1
  repeatable: true
--line
  required: false
  arity: 1
  repeatable: true
--symbol
  required: false
  arity: 1
  repeatable: true
--changed
  required: false
  arity: 0
  repeatable: false
  default: false
--changed-context
  required: false
  arity: 1
  repeatable: false
  default: 0
--diff-base
  required: false
  arity: 1
  repeatable: false
--include
  required: false
  arity: 1
  repeatable: true
--fingerprint-include
  required: false
  arity: 1
  repeatable: true
--fingerprint-file
  required: false
  arity: 1
  repeatable: true
--fingerprint-env
  required: false
  arity: 1
  repeatable: true
--exclude
  required: false
  arity: 1
  repeatable: true
--operators
  required: false
  arity: 1
  repeatable: true
  delimiter: ,
--exclude-operators
  required: false
  arity: 1
  repeatable: true
  delimiter: ,
--profile
  required: false
  arity: 1
  repeatable: false
  default: full
  values: full, focused
--jobs
  required: false
  arity: 1
  repeatable: false
  default: 1
--max-mutants
  required: false
  arity: 1
  repeatable: false
  default: 100
--max-candidates
  required: false
  arity: 1
  repeatable: false
  default: 10000
--baseline-timeout
  required: false
  arity: 1
  repeatable: false
  default: 60s
--mutant-timeout
  required: false
  arity: 1
  repeatable: false
  default: auto
--total-timeout
  required: false
  arity: 1
  repeatable: false
  default: 5m
--max-memory
  required: false
  arity: 1
  repeatable: false
  default: 1GiB
--max-output
  required: false
  arity: 1
  repeatable: false
  default: 1MiB
--max-copy-size
  required: false
  arity: 1
  repeatable: false
  default: 1GiB
--max-workspace-size
  required: false
  arity: 1
  repeatable: false
  default: 8GiB
--min-free-space
  required: false
  arity: 1
  repeatable: false
  default: 10GiB
--max-processes
  required: false
  arity: 1
  repeatable: false
  default: 64
--allow-best-effort-memory
  required: false
  arity: 0
  repeatable: false
  default: false
--metrics
  required: false
  arity: 1
  repeatable: false
--session
  required: false
  arity: 1
  repeatable: false
--resume
  required: false
  arity: 0
  repeatable: false
  default: false
test_argv
  required: false
  arity: 1..
  repeatable: true
  follows: --
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin plan

```text
Discover mutation candidates without executing tests

Usage: hoimin plan [OPTIONS] [-- <TEST_ARGV>...]

Arguments:
  [TEST_ARGV]...
          Test executable and arguments, passed directly without a shell

Options:
      --analyzer-timeout <DURATION>
          Cooperative deadline for discovery across all selected targets, inherited by verify

          [default: 30s]

      --root <DIR>
          Project root used to resolve relative paths

          [default: .]

      --source <DIR>
          Source root containing mutation targets; may be repeated

      --import-root <DIR>
          Worker import directory relative to --root; repeat in precedence order without selecting targets

      --file <PATH>
          Select a complete Python file; may be repeated

      --line <PATH:START-END>
          Select PATH:START-END (an inclusive line range); may be repeated

      --symbol <MODULE:QUALNAME>
          Select MODULE:QUALNAME; may be repeated

      --changed
          Restrict targets to changed Git lines

      --changed-context <N>
          Include N neighboring lines around Git changes (0..=1073741823)

          [default: 0]

      --diff-base <REV>
          Compare changed lines with the merge-base of REV and HEAD

      --include <GLOB>
          Include a normally ignored path while copying; may be repeated

      --fingerprint-include <GLOB>
          Add a root-relative file glob to the session fingerprint; may be repeated

      --fingerprint-file <PATH>
          Add one exact root-relative file to the session fingerprint; may be repeated

      --fingerprint-env <NAME>
          Track inherited NAME before worker rewriting; repeatable ASCII [A-Za-z_][A-Za-z0-9_]* (Unix case-sensitive, Windows uppercase)

      --exclude <GLOB>
          Exclude a path while copying; may be repeated and wins over include

      --operators <OPERATORS>
          Select mutation operator IDs or selector families; repeatable or comma-delimited.

          Omit --operators to use the default runtime set. An explicit selection replaces that set. Selecting a family includes all its listed members, including opt-in IDs. --exclude-operators is applied afterward.

      --exclude-operators <EXCLUDE_OPERATORS>
          Exclude mutation operator IDs or selector families; repeatable or comma-delimited.

          Remove the named IDs or every member of the named families after the default or explicit --operators selection. At least one operator must remain selected.

      --profile <PROFILE>
          Candidate-selection profile

          [default: full]
          [possible values: full, focused]

      --jobs <JOBS>
          Maximum concurrently active workers

          [default: 1]

      --max-mutants <MAX_MUTANTS>
          Maximum mutants to execute

          [default: 100]

      --max-candidates <MAX_CANDIDATES>
          Maximum candidates to discover before mutation starts

          [default: 10000]

      --baseline-timeout <DURATION>
          Baseline test timeout

          [default: 60s]

      --mutant-timeout <DURATION|auto>
          Per-mutant timeout or `auto`

          [default: auto]

      --total-timeout <DURATION>
          Run time budget, including environment capture, fingerprint reads, and execution setup.

          Verify starts this budget after saved-plan validation and reanalysis. Preparation consumes execution time; shutdown uses one additional two-second grace. An OS operation already in progress may finish later while retaining its resource owners.

          [default: 5m]

      --max-memory <BYTES>
          Memory limit (Windows: per root process tree, committed memory; jobs multiplies total allowance)

          [default: 1GiB]

      --max-output <BYTES>
          Retained stdout and stderr per process

          [default: 1MiB]

      --max-copy-size <BYTES>
          Run-wide logical copy-size limit

          [default: 1GiB]

      --max-workspace-size <BYTES>
          Run-wide logical size of Hoimin-owned workspaces, including generated files

          [default: 8GiB]

      --min-free-space <BYTES>
          Mandatory minimum available bytes preserved on every owned-workspace filesystem

          [default: 10GiB]

      --max-processes <MAX_PROCESSES>
          Process limit (Windows: per root process tree, including the root; jobs multiplies total allowance)

          [default: 64]

      --allow-best-effort-memory
          Permit best-effort memory enforcement when hard limits are unavailable

  -h, --help
          Print help (see a summary with '-h')

Operator examples, applicability and limits for released hoimin 0.15.0:
  https://github.com/HayaoSuzuki/hoimin/blob/v0.15.0/docs/user/guides/usage.md#mutation-operators
  Source builds: read docs/user/reference/operators.md in the same checkout.

Default runtime IDs:
  aggregate_sum_count, assignment_target_swap, async_iterator_stop_insert, augmented_add_sub
  augmented_bitwise_and_or, augmented_bitwise_shift, augmented_bitwise_xor, augmented_floor_mod
  augmented_matmul, augmented_mul_div, augmented_power, augmented_to_assignment
  augmented_to_binary, await_remove, base64_validate_toggle, base_order_shuffle
  batched_strict_toggle, binary_add_sub, binary_div_floor, binary_floor_mod
  binary_matmul, binary_mul_div, binary_power, bisect_boundary_swap
  bitwise_and_or, bitwise_invert, bitwise_shift, bitwise_xor
  boolean_and_or, boolean_literal, break_continue, bytes_literal_empty
  call_argument_swap, call_result_default, collection_any_all, collection_append_insert
  collection_list_tuple, collection_min_max, collection_set_add_discard, collection_set_frozenset
  collection_set_remove_discard, collection_string_split_rsplit, collection_string_starts_ends, compare_direction
  compare_eq_ne, compare_order, comparison_not_implemented, comprehension_filter_invert
  comprehension_filter_remove, conditional_branch_swap, context_exit_omit, context_exit_result_negate
  continue_remove, conversion_call_remove, dataclass_factory_share, decimal_context_precision
  decimal_quantum_shift, decimal_rounding_mode_swap, deepcopy_to_copy, del_remove
  deque_popleft_to_pop, dict_unpack_delete, dict_unpack_order_swap, else_suite_remove
  encoding_errors_ignore, encoding_errors_replace, encoding_errors_strict, exception_cause_omit
  exception_cause_remove, exception_handler_swap, exception_type_pair, filter_predicate_swap
  finally_unwrap, float_literal_neighbor, fstring_conversion_swap, fstring_format_option_swap
  fstring_format_spec_remove, fstring_format_value_neighbor, function_body_return_constant, gather_return_exceptions
  generator_empty, generator_return_insert, generator_skip_first, heap_pushpop_replace
  identity, identity_equality_swap, integer_literal_neighbor, intermediate_rounding_remove
  iterator_stop_insert, itertools_take_drop_swap, json_allow_nan_toggle, lambda_capture_late
  list_comprehension_to_generator, loop_break_insert, map_strict_toggle, mapping_setdefault_to_get
  match_case_delete, match_case_swap, match_class_position_swap, match_guard_constant
  match_or_alternative_delete, match_pattern_wildcard, math_isclose_tolerance, math_precision_loss
  membership, method_call_remove, mutable_default_share, none_truthiness
  nonlocal_remove, operator_function, override_method_delete, range_bound_neighbor
  regex_alternative_delete, regex_ascii_mode, regex_character_set, regex_greediness_toggle
  regex_match_boundary, regex_quantifier_neighbor, remove_not, return_tuple_swap
  return_value_none, round_digits_remove, rounding_function_swap, set_union_intersection_swap
  sort_key_swap, statement_delete, statement_order_swap, statistics_sample_population
  stdlib_conversion_call_remove, stdlib_escape_remove, string_affix_strip, string_casefold_lower
  string_literal_empty, string_segment_empty, structure_append_extend, structure_index_neighbor
  structure_mapping_get_subscript, structure_slice_bound_move, structure_slice_bound_omit, structure_slice_neighbor
  structure_sort_reverse, structure_sorted_reversed, super_call_move, unary_sign
  unicode_digit_ascii, unicode_digit_predicate_swap, unicode_normalization_form, unicode_normalization_remove
  url_query_policy, url_quote_swap, while_condition_false, yield_from_to_yield
  yield_statement_remove, zip_strict_toggle

Opt-in IDs:
  condition_clause_delete, condition_constant, container_element_delete, enum_member_replace
  exception_bare_to_exception, exception_base_boundary, exception_exception_to_bare, exception_hierarchy
  exception_tuple_add_pair, exception_tuple_remove_member, function_body_erase, optional_keyword_delete
  type_dict_mapping, type_iterable_iterator, type_list_sequence, type_nullable_add
  type_nullable_remove, type_sequence_iterable, type_set_abstract_set

Selector families:
  bitwise_ops, call_contracts, collection_ops, collection_processing
  exception_ops, exception_risky, regex, rounding
  structure_ops, type_collections, type_iterables, type_nullable
```

Argument constraints:

```text
--analyzer-timeout
  required: false
  arity: 1
  repeatable: false
  default: 30s
--root
  required: false
  arity: 1
  repeatable: false
  default: .
--source
  required: false
  arity: 1
  repeatable: true
--import-root
  required: false
  arity: 1
  repeatable: true
--file
  required: false
  arity: 1
  repeatable: true
--line
  required: false
  arity: 1
  repeatable: true
--symbol
  required: false
  arity: 1
  repeatable: true
--changed
  required: false
  arity: 0
  repeatable: false
  default: false
--changed-context
  required: false
  arity: 1
  repeatable: false
  default: 0
--diff-base
  required: false
  arity: 1
  repeatable: false
--include
  required: false
  arity: 1
  repeatable: true
--fingerprint-include
  required: false
  arity: 1
  repeatable: true
--fingerprint-file
  required: false
  arity: 1
  repeatable: true
--fingerprint-env
  required: false
  arity: 1
  repeatable: true
--exclude
  required: false
  arity: 1
  repeatable: true
--operators
  required: false
  arity: 1
  repeatable: true
  delimiter: ,
--exclude-operators
  required: false
  arity: 1
  repeatable: true
  delimiter: ,
--profile
  required: false
  arity: 1
  repeatable: false
  default: full
  values: full, focused
--jobs
  required: false
  arity: 1
  repeatable: false
  default: 1
--max-mutants
  required: false
  arity: 1
  repeatable: false
  default: 100
--max-candidates
  required: false
  arity: 1
  repeatable: false
  default: 10000
--baseline-timeout
  required: false
  arity: 1
  repeatable: false
  default: 60s
--mutant-timeout
  required: false
  arity: 1
  repeatable: false
  default: auto
--total-timeout
  required: false
  arity: 1
  repeatable: false
  default: 5m
--max-memory
  required: false
  arity: 1
  repeatable: false
  default: 1GiB
--max-output
  required: false
  arity: 1
  repeatable: false
  default: 1MiB
--max-copy-size
  required: false
  arity: 1
  repeatable: false
  default: 1GiB
--max-workspace-size
  required: false
  arity: 1
  repeatable: false
  default: 8GiB
--min-free-space
  required: false
  arity: 1
  repeatable: false
  default: 10GiB
--max-processes
  required: false
  arity: 1
  repeatable: false
  default: 64
--allow-best-effort-memory
  required: false
  arity: 0
  repeatable: false
  default: false
test_argv
  required: false
  arity: 1..
  repeatable: true
  follows: --
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin verify

```text
Execute one or more candidates using settings from a previously generated plan

Usage: hoimin verify [OPTIONS] <--candidate <ID>|--top <N>|--sample <N>> <PLAN>

Arguments:
  <PLAN>
          Path to a manifest saved by hoimin plan

Options:
      --candidate <ID>
          Candidate ID to execute; repeat for multiple planned candidates

      --top <N>
          Execute the N highest-ranked candidates retained in the plan; strict order is the default

      --sample <N>
          Sample N candidates without replacement from a complete plan

      --seed <SEED>
          Explicit reproducible sampling seed (unsigned 64-bit integer)

      --offset <K>
          Skip K candidates in the complete selected-policy ordering before taking --top N

      --selection-policy <POLICY>
          Select saved-rank order or equal-score file/line diversity for --top.

          Diverse round-robins files; line-diverse round-robins (file, start-line) groups. Both keep higher-score tiers first.

          [possible values: strict, diverse, line-diverse]

      --dry-run
          Validate and preview selected candidates without executing tests

      --metrics <PATH>
          Write execution metrics to PATH, relative to the invocation directory

  -h, --help
          Print help (see a summary with '-h')

Execution and resource settings come from PLAN and cannot be overridden. Disk safety limits --max-workspace-size and --min-free-space are inherited from PLAN. Create a new plan to change them.
```

Argument constraints:

```text
manifest
  required: true
  arity: 1
  repeatable: false
--candidate
  required: false
  arity: 1
  repeatable: true
--top
  required: false
  arity: 1
  repeatable: false
--sample
  required: false
  arity: 1
  repeatable: false
  conflicts: --offset, --selection-policy
--seed
  required: false
  arity: 1
  repeatable: false
  conflicts: --top, --candidate
--offset
  required: false
  arity: 1
  repeatable: false
  conflicts: --candidate
--selection-policy
  required: false
  arity: 1
  repeatable: false
  values: strict, diverse, line-diverse
  conflicts: --candidate
--dry-run
  required: false
  arity: 0
  repeatable: false
  default: false
  conflicts: --metrics
--metrics
  required: false
  arity: 1
  repeatable: false
--help
  required: false
  arity: 0
  repeatable: false
required group selection: --candidate, --top, --sample; multiple: false
```

## hoimin progress

```text
Compare chronologically ordered mutation run reports

Usage: hoimin progress [OPTIONS] <REPORT> <REPORT>...

Arguments:
  <REPORT> <REPORT>...
          Ordered run report files to compare

Options:
      --patience <PATIENCE>
          Consecutive unchanged comparisons before the history is saturated

          [default: 3]

      --details
          Show identified changes in the final adjacent input pair

      --details-limit <DETAILS_LIMIT>
          Maximum transition details to show; zero reports only omission counts

          [default: 100]

  -h, --help
          Print help
```

Argument constraints:

```text
--patience
  required: false
  arity: 1
  repeatable: false
  default: 3
--details
  required: false
  arity: 0
  repeatable: false
  default: false
--details-limit
  required: false
  arity: 1
  repeatable: false
  default: 100
reports
  required: true
  arity: 2..
  repeatable: true
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin review

```text
Record and inspect human assessments independently of test outcomes

Usage: hoimin review <COMMAND>

Commands:
  list    List current candidates and their assessments
  show    Show one candidate and its review history
  record  Append a human assessment using an expected revision
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

Argument constraints:

```text
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin review list

```text
List current candidates and their assessments

Usage: hoimin review list [OPTIONS] --store <STORE> <PLAN>

Arguments:
  <PLAN>


Options:
      --store <STORE>


      --report <REPORT>


      --stale
          Include stored candidates absent from the current plan in a separate list

  -h, --help
          Print help
```

Argument constraints:

```text
plan
  required: true
  arity: 1
  repeatable: false
--store
  required: true
  arity: 1
  repeatable: false
--report
  required: false
  arity: 1
  repeatable: false
--stale
  required: false
  arity: 0
  repeatable: false
  default: false
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin review show

```text
Show one candidate and its review history

Usage: hoimin review show [OPTIONS] --store <STORE> --candidate <CANDIDATE> <PLAN>

Arguments:
  <PLAN>


Options:
      --store <STORE>


      --report <REPORT>


      --candidate <CANDIDATE>


  -h, --help
          Print help
```

Argument constraints:

```text
plan
  required: true
  arity: 1
  repeatable: false
--store
  required: true
  arity: 1
  repeatable: false
--report
  required: false
  arity: 1
  repeatable: false
--candidate
  required: true
  arity: 1
  repeatable: false
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin review record

```text
Append a human assessment using an expected revision

Usage: hoimin review record [OPTIONS] --store <STORE> --candidate <CANDIDATE> --assessment <ASSESSMENT> --reason <REASON> --expected-revision <EXPECTED_REVISION> <PLAN>

Arguments:
  <PLAN>


Options:
      --store <STORE>


      --report <REPORT>


      --candidate <CANDIDATE>


      --assessment <ASSESSMENT>
          [possible values: actionable, not_useful, suspected_equivalent, needs_investigation]

      --reason <REASON>


      --evidence <EVIDENCE>


      --expected-revision <EXPECTED_REVISION>


  -h, --help
          Print help
```

Argument constraints:

```text
plan
  required: true
  arity: 1
  repeatable: false
--store
  required: true
  arity: 1
  repeatable: false
--report
  required: false
  arity: 1
  repeatable: false
--candidate
  required: true
  arity: 1
  repeatable: false
--assessment
  required: true
  arity: 1
  repeatable: false
  values: actionable, not_useful, suspected_equivalent, needs_investigation
--reason
  required: true
  arity: 1
  repeatable: false
--evidence
  required: false
  arity: 1
  repeatable: true
--expected-revision
  required: true
  arity: 1
  repeatable: false
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin review help

```text
Print this message or the help of the given subcommand(s)

Usage: hoimin review help [COMMAND]

Commands:
  list    List current candidates and their assessments
  show    Show one candidate and its review history
  record  Append a human assessment using an expected revision
  help    Print this message or the help of the given subcommand(s)
```

## hoimin review help list

```text
List current candidates and their assessments

Usage: hoimin review help list
```

## hoimin review help show

```text
Show one candidate and its review history

Usage: hoimin review help show
```

## hoimin review help record

```text
Append a human assessment using an expected revision

Usage: hoimin review help record
```

## hoimin review help help

```text
Print this message or the help of the given subcommand(s)

Usage: hoimin review help help
```

## hoimin reduce

```text
Search and validate whole-definition test reductions without applying them

Usage: hoimin reduce [OPTIONS] --candidate <CANDIDATE> --test <TEST> --total-timeout <TOTAL_TIMEOUT> --output <OUTPUT> <PLAN>

Arguments:
  <PLAN>


Options:
      --candidate <CANDIDATE>


      --test <TEST>
          Select root-relative `PATH:QUALNAME_GLOB`; repeat for additional definitions

      --keep <KEEP>
          Keep definitions using the same `PATH:QUALNAME_GLOB` syntax; keep wins

      --total-timeout <TOTAL_TIMEOUT>
          Absolute search budget, including original evaluation and final confirmation

      --min-reduction <MIN_REDUCTION>
          [default: 5]

      --output <OUTPUT>
          New artifact directory outside the project

  -h, --help
          Print help
```

Argument constraints:

```text
plan
  required: true
  arity: 1
  repeatable: false
--candidate
  required: true
  arity: 1
  repeatable: true
--test
  required: true
  arity: 1
  repeatable: true
--keep
  required: false
  arity: 1
  repeatable: true
--total-timeout
  required: true
  arity: 1
  repeatable: false
--min-reduction
  required: false
  arity: 1
  repeatable: false
  default: 5
--output
  required: true
  arity: 1
  repeatable: false
--help
  required: false
  arity: 0
  repeatable: false
```

## hoimin help

```text
Print this message or the help of the given subcommand(s)

Usage: hoimin help [COMMAND]

Commands:
  run       Run mutation tests for an explicitly selected target
  plan      Discover mutation candidates without executing tests
  verify    Execute one or more candidates using settings from a previously generated plan
  progress  Compare chronologically ordered mutation run reports
  review    Record and inspect human assessments independently of test outcomes
  reduce    Search and validate whole-definition test reductions without applying them
  help      Print this message or the help of the given subcommand(s)
```

## hoimin help run

```text
Run mutation tests for an explicitly selected target

Usage: hoimin help run
```

## hoimin help plan

```text
Discover mutation candidates without executing tests

Usage: hoimin help plan
```

## hoimin help verify

```text
Execute one or more candidates using settings from a previously generated plan

Usage: hoimin help verify
```

## hoimin help progress

```text
Compare chronologically ordered mutation run reports

Usage: hoimin help progress
```

## hoimin help review

```text
Record and inspect human assessments independently of test outcomes

Usage: hoimin help review [COMMAND]

Commands:
  list    List current candidates and their assessments
  show    Show one candidate and its review history
  record  Append a human assessment using an expected revision
```

## hoimin help review list

```text
List current candidates and their assessments

Usage: hoimin help review list
```

## hoimin help review show

```text
Show one candidate and its review history

Usage: hoimin help review show
```

## hoimin help review record

```text
Append a human assessment using an expected revision

Usage: hoimin help review record
```

## hoimin help reduce

```text
Search and validate whole-definition test reductions without applying them

Usage: hoimin help reduce
```

## hoimin help help

```text
Print this message or the help of the given subcommand(s)

Usage: hoimin help help
```

## Operator selection

Use this index to check each operator's default state and selector families.
The option explanations below are the same text as in run/plan --help.
See the [operator reference](/docs/user/reference/operators.md) for examples and applicability,
and [choose operators](/docs/user/guides/usage.md#mutation-operators) for workflow and version guidance.

Rows marked **yes** are enabled by default. A dash in the family column means
that the ID has no family alias.

### `--operators`

Select mutation operator IDs or selector families; repeatable or comma-delimited.

Omit --operators to use the default runtime set. An explicit selection replaces that set. Selecting a family includes all its listed members, including opt-in IDs. --exclude-operators is applied afterward.

### `--exclude-operators`

Exclude mutation operator IDs or selector families; repeatable or comma-delimited.

Remove the named IDs or every member of the named families after the default or explicit --operators selection. At least one operator must remain selected.

### Operator IDs

| Operator ID | Default runtime selection | Selector families |
| --- | --- | --- |
| [`aggregate_sum_count`](operators.md#aggregate_sum_count) | yes | [`collection_processing`](#collection_processing) |
| [`assignment_target_swap`](operators.md#assignment_target_swap) | yes | — |
| [`async_iterator_stop_insert`](operators.md#async_iterator_stop_insert) | yes | — |
| [`augmented_add_sub`](operators.md#augmented_add_sub) | yes | — |
| [`augmented_bitwise_and_or`](operators.md#augmented_bitwise_and_or) | yes | [`bitwise_ops`](#bitwise_ops) |
| [`augmented_bitwise_shift`](operators.md#augmented_bitwise_shift) | yes | [`bitwise_ops`](#bitwise_ops) |
| [`augmented_bitwise_xor`](operators.md#augmented_bitwise_xor) | yes | [`bitwise_ops`](#bitwise_ops) |
| [`augmented_floor_mod`](operators.md#augmented_floor_mod) | yes | — |
| [`augmented_matmul`](operators.md#augmented_matmul) | yes | — |
| [`augmented_mul_div`](operators.md#augmented_mul_div) | yes | — |
| [`augmented_power`](operators.md#augmented_power) | yes | — |
| [`augmented_to_assignment`](operators.md#augmented_to_assignment) | yes | — |
| [`augmented_to_binary`](operators.md#augmented_to_binary) | yes | — |
| [`await_remove`](operators.md#await_remove) | yes | — |
| [`base64_validate_toggle`](operators.md#base64_validate_toggle) | yes | — |
| [`base_order_shuffle`](operators.md#base_order_shuffle) | yes | — |
| [`batched_strict_toggle`](operators.md#batched_strict_toggle) | yes | — |
| [`binary_add_sub`](operators.md#binary_add_sub) | yes | — |
| [`binary_div_floor`](operators.md#binary_div_floor) | yes | — |
| [`binary_floor_mod`](operators.md#binary_floor_mod) | yes | — |
| [`binary_matmul`](operators.md#binary_matmul) | yes | — |
| [`binary_mul_div`](operators.md#binary_mul_div) | yes | — |
| [`binary_power`](operators.md#binary_power) | yes | — |
| [`bisect_boundary_swap`](operators.md#bisect_boundary_swap) | yes | — |
| [`bitwise_and_or`](operators.md#bitwise_and_or) | yes | [`bitwise_ops`](#bitwise_ops) |
| [`bitwise_invert`](operators.md#bitwise_invert) | yes | [`bitwise_ops`](#bitwise_ops) |
| [`bitwise_shift`](operators.md#bitwise_shift) | yes | [`bitwise_ops`](#bitwise_ops) |
| [`bitwise_xor`](operators.md#bitwise_xor) | yes | [`bitwise_ops`](#bitwise_ops) |
| [`boolean_and_or`](operators.md#boolean_and_or) | yes | — |
| [`boolean_literal`](operators.md#boolean_literal) | yes | — |
| [`break_continue`](operators.md#break_continue) | yes | — |
| [`bytes_literal_empty`](operators.md#bytes_literal_empty) | yes | — |
| [`call_argument_swap`](operators.md#call_argument_swap) | yes | [`call_contracts`](#call_contracts) |
| [`call_result_default`](operators.md#call_result_default) | yes | [`call_contracts`](#call_contracts) |
| [`collection_any_all`](operators.md#collection_any_all) | yes | [`collection_ops`](#collection_ops) |
| [`collection_append_insert`](operators.md#collection_append_insert) | yes | [`collection_ops`](#collection_ops) |
| [`collection_list_tuple`](operators.md#collection_list_tuple) | yes | [`collection_ops`](#collection_ops) |
| [`collection_min_max`](operators.md#collection_min_max) | yes | [`collection_ops`](#collection_ops) |
| [`collection_set_add_discard`](operators.md#collection_set_add_discard) | yes | [`collection_ops`](#collection_ops) |
| [`collection_set_frozenset`](operators.md#collection_set_frozenset) | yes | [`collection_ops`](#collection_ops) |
| [`collection_set_remove_discard`](operators.md#collection_set_remove_discard) | yes | [`collection_ops`](#collection_ops) |
| [`collection_string_split_rsplit`](operators.md#collection_string_split_rsplit) | yes | [`collection_ops`](#collection_ops) |
| [`collection_string_starts_ends`](operators.md#collection_string_starts_ends) | yes | [`collection_ops`](#collection_ops) |
| [`compare_direction`](operators.md#compare_direction) | yes | — |
| [`compare_eq_ne`](operators.md#compare_eq_ne) | yes | — |
| [`compare_order`](operators.md#compare_order) | yes | — |
| [`comparison_not_implemented`](operators.md#comparison_not_implemented) | yes | — |
| [`comprehension_filter_invert`](operators.md#comprehension_filter_invert) | yes | [`collection_processing`](#collection_processing) |
| [`comprehension_filter_remove`](operators.md#comprehension_filter_remove) | yes | [`collection_processing`](#collection_processing) |
| [`condition_clause_delete`](operators.md#condition_clause_delete) | opt-in | — |
| [`condition_constant`](operators.md#condition_constant) | opt-in | — |
| [`conditional_branch_swap`](operators.md#conditional_branch_swap) | yes | — |
| [`container_element_delete`](operators.md#container_element_delete) | opt-in | — |
| [`context_exit_omit`](operators.md#context_exit_omit) | yes | — |
| [`context_exit_result_negate`](operators.md#context_exit_result_negate) | yes | — |
| [`continue_remove`](operators.md#continue_remove) | yes | — |
| [`conversion_call_remove`](operators.md#conversion_call_remove) | yes | — |
| [`dataclass_factory_share`](operators.md#dataclass_factory_share) | yes | — |
| [`decimal_context_precision`](operators.md#decimal_context_precision) | yes | [`rounding`](#rounding) |
| [`decimal_quantum_shift`](operators.md#decimal_quantum_shift) | yes | [`rounding`](#rounding) |
| [`decimal_rounding_mode_swap`](operators.md#decimal_rounding_mode_swap) | yes | [`rounding`](#rounding) |
| [`deepcopy_to_copy`](operators.md#deepcopy_to_copy) | yes | — |
| [`del_remove`](operators.md#del_remove) | yes | — |
| [`deque_popleft_to_pop`](operators.md#deque_popleft_to_pop) | yes | — |
| [`dict_unpack_delete`](operators.md#dict_unpack_delete) | yes | — |
| [`dict_unpack_order_swap`](operators.md#dict_unpack_order_swap) | yes | — |
| [`else_suite_remove`](operators.md#else_suite_remove) | yes | — |
| [`encoding_errors_ignore`](operators.md#encoding_errors_ignore) | yes | — |
| [`encoding_errors_replace`](operators.md#encoding_errors_replace) | yes | — |
| [`encoding_errors_strict`](operators.md#encoding_errors_strict) | yes | — |
| [`enum_member_replace`](operators.md#enum_member_replace) | opt-in | — |
| [`exception_bare_to_exception`](operators.md#exception_bare_to_exception) | opt-in | [`exception_risky`](#exception_risky) |
| [`exception_base_boundary`](operators.md#exception_base_boundary) | opt-in | [`exception_risky`](#exception_risky) |
| [`exception_cause_omit`](operators.md#exception_cause_omit) | yes | — |
| [`exception_cause_remove`](operators.md#exception_cause_remove) | yes | — |
| [`exception_exception_to_bare`](operators.md#exception_exception_to_bare) | opt-in | [`exception_risky`](#exception_risky) |
| [`exception_handler_swap`](operators.md#exception_handler_swap) | yes | — |
| [`exception_hierarchy`](operators.md#exception_hierarchy) | opt-in | — |
| [`exception_tuple_add_pair`](operators.md#exception_tuple_add_pair) | opt-in | [`exception_risky`](#exception_risky) |
| [`exception_tuple_remove_member`](operators.md#exception_tuple_remove_member) | opt-in | [`exception_risky`](#exception_risky) |
| [`exception_type_pair`](operators.md#exception_type_pair) | yes | [`exception_ops`](#exception_ops) |
| [`filter_predicate_swap`](operators.md#filter_predicate_swap) | yes | [`collection_processing`](#collection_processing) |
| [`finally_unwrap`](operators.md#finally_unwrap) | yes | — |
| [`float_literal_neighbor`](operators.md#float_literal_neighbor) | yes | — |
| [`fstring_conversion_swap`](operators.md#fstring_conversion_swap) | yes | — |
| [`fstring_format_option_swap`](operators.md#fstring_format_option_swap) | yes | — |
| [`fstring_format_spec_remove`](operators.md#fstring_format_spec_remove) | yes | — |
| [`fstring_format_value_neighbor`](operators.md#fstring_format_value_neighbor) | yes | — |
| [`function_body_erase`](operators.md#function_body_erase) | opt-in | — |
| [`function_body_return_constant`](operators.md#function_body_return_constant) | yes | — |
| [`gather_return_exceptions`](operators.md#gather_return_exceptions) | yes | — |
| [`generator_empty`](operators.md#generator_empty) | yes | — |
| [`generator_return_insert`](operators.md#generator_return_insert) | yes | — |
| [`generator_skip_first`](operators.md#generator_skip_first) | yes | — |
| [`heap_pushpop_replace`](operators.md#heap_pushpop_replace) | yes | — |
| [`identity`](operators.md#identity) | yes | — |
| [`identity_equality_swap`](operators.md#identity_equality_swap) | yes | — |
| [`integer_literal_neighbor`](operators.md#integer_literal_neighbor) | yes | — |
| [`intermediate_rounding_remove`](operators.md#intermediate_rounding_remove) | yes | [`rounding`](#rounding) |
| [`iterator_stop_insert`](operators.md#iterator_stop_insert) | yes | — |
| [`itertools_take_drop_swap`](operators.md#itertools_take_drop_swap) | yes | — |
| [`json_allow_nan_toggle`](operators.md#json_allow_nan_toggle) | yes | — |
| [`lambda_capture_late`](operators.md#lambda_capture_late) | yes | — |
| [`list_comprehension_to_generator`](operators.md#list_comprehension_to_generator) | yes | — |
| [`loop_break_insert`](operators.md#loop_break_insert) | yes | — |
| [`map_strict_toggle`](operators.md#map_strict_toggle) | yes | — |
| [`mapping_setdefault_to_get`](operators.md#mapping_setdefault_to_get) | yes | — |
| [`match_case_delete`](operators.md#match_case_delete) | yes | — |
| [`match_case_swap`](operators.md#match_case_swap) | yes | — |
| [`match_class_position_swap`](operators.md#match_class_position_swap) | yes | — |
| [`match_guard_constant`](operators.md#match_guard_constant) | yes | — |
| [`match_or_alternative_delete`](operators.md#match_or_alternative_delete) | yes | — |
| [`match_pattern_wildcard`](operators.md#match_pattern_wildcard) | yes | — |
| [`math_isclose_tolerance`](operators.md#math_isclose_tolerance) | yes | — |
| [`math_precision_loss`](operators.md#math_precision_loss) | yes | — |
| [`membership`](operators.md#membership) | yes | — |
| [`method_call_remove`](operators.md#method_call_remove) | yes | — |
| [`mutable_default_share`](operators.md#mutable_default_share) | yes | — |
| [`none_truthiness`](operators.md#none_truthiness) | yes | — |
| [`nonlocal_remove`](operators.md#nonlocal_remove) | yes | — |
| [`operator_function`](operators.md#operator_function) | yes | — |
| [`optional_keyword_delete`](operators.md#optional_keyword_delete) | opt-in | — |
| [`override_method_delete`](operators.md#override_method_delete) | yes | — |
| [`range_bound_neighbor`](operators.md#range_bound_neighbor) | yes | — |
| [`regex_alternative_delete`](operators.md#regex_alternative_delete) | yes | — |
| [`regex_ascii_mode`](operators.md#regex_ascii_mode) | yes | [`regex`](#regex) |
| [`regex_character_set`](operators.md#regex_character_set) | yes | [`regex`](#regex) |
| [`regex_greediness_toggle`](operators.md#regex_greediness_toggle) | yes | — |
| [`regex_match_boundary`](operators.md#regex_match_boundary) | yes | [`regex`](#regex) |
| [`regex_quantifier_neighbor`](operators.md#regex_quantifier_neighbor) | yes | — |
| [`remove_not`](operators.md#remove_not) | yes | — |
| [`return_tuple_swap`](operators.md#return_tuple_swap) | yes | — |
| [`return_value_none`](operators.md#return_value_none) | yes | — |
| [`round_digits_remove`](operators.md#round_digits_remove) | yes | [`rounding`](#rounding) |
| [`rounding_function_swap`](operators.md#rounding_function_swap) | yes | [`rounding`](#rounding) |
| [`set_union_intersection_swap`](operators.md#set_union_intersection_swap) | yes | [`collection_processing`](#collection_processing) |
| [`sort_key_swap`](operators.md#sort_key_swap) | yes | — |
| [`statement_delete`](operators.md#statement_delete) | yes | — |
| [`statement_order_swap`](operators.md#statement_order_swap) | yes | — |
| [`statistics_sample_population`](operators.md#statistics_sample_population) | yes | — |
| [`stdlib_conversion_call_remove`](operators.md#stdlib_conversion_call_remove) | yes | — |
| [`stdlib_escape_remove`](operators.md#stdlib_escape_remove) | yes | [`call_contracts`](#call_contracts) |
| [`string_affix_strip`](operators.md#string_affix_strip) | yes | — |
| [`string_casefold_lower`](operators.md#string_casefold_lower) | yes | — |
| [`string_literal_empty`](operators.md#string_literal_empty) | yes | — |
| [`string_segment_empty`](operators.md#string_segment_empty) | yes | — |
| [`structure_append_extend`](operators.md#structure_append_extend) | yes | [`structure_ops`](#structure_ops) |
| [`structure_index_neighbor`](operators.md#structure_index_neighbor) | yes | [`structure_ops`](#structure_ops) |
| [`structure_mapping_get_subscript`](operators.md#structure_mapping_get_subscript) | yes | [`structure_ops`](#structure_ops) |
| [`structure_slice_bound_move`](operators.md#structure_slice_bound_move) | yes | [`structure_ops`](#structure_ops) |
| [`structure_slice_bound_omit`](operators.md#structure_slice_bound_omit) | yes | [`structure_ops`](#structure_ops) |
| [`structure_slice_neighbor`](operators.md#structure_slice_neighbor) | yes | [`structure_ops`](#structure_ops) |
| [`structure_sort_reverse`](operators.md#structure_sort_reverse) | yes | [`structure_ops`](#structure_ops) |
| [`structure_sorted_reversed`](operators.md#structure_sorted_reversed) | yes | [`structure_ops`](#structure_ops) |
| [`super_call_move`](operators.md#super_call_move) | yes | — |
| [`type_dict_mapping`](operators.md#type_dict_mapping) | opt-in | [`type_collections`](#type_collections) |
| [`type_iterable_iterator`](operators.md#type_iterable_iterator) | opt-in | [`type_iterables`](#type_iterables) |
| [`type_list_sequence`](operators.md#type_list_sequence) | opt-in | [`type_collections`](#type_collections) |
| [`type_nullable_add`](operators.md#type_nullable_add) | opt-in | [`type_nullable`](#type_nullable) |
| [`type_nullable_remove`](operators.md#type_nullable_remove) | opt-in | [`type_nullable`](#type_nullable) |
| [`type_sequence_iterable`](operators.md#type_sequence_iterable) | opt-in | [`type_iterables`](#type_iterables) |
| [`type_set_abstract_set`](operators.md#type_set_abstract_set) | opt-in | [`type_collections`](#type_collections) |
| [`unary_sign`](operators.md#unary_sign) | yes | — |
| [`unicode_digit_ascii`](operators.md#unicode_digit_ascii) | yes | — |
| [`unicode_digit_predicate_swap`](operators.md#unicode_digit_predicate_swap) | yes | — |
| [`unicode_normalization_form`](operators.md#unicode_normalization_form) | yes | — |
| [`unicode_normalization_remove`](operators.md#unicode_normalization_remove) | yes | — |
| [`url_query_policy`](operators.md#url_query_policy) | yes | — |
| [`url_quote_swap`](operators.md#url_quote_swap) | yes | — |
| [`while_condition_false`](operators.md#while_condition_false) | yes | — |
| [`yield_from_to_yield`](operators.md#yield_from_to_yield) | yes | — |
| [`yield_statement_remove`](operators.md#yield_statement_remove) | yes | — |
| [`zip_strict_toggle`](operators.md#zip_strict_toggle) | yes | — |

## Selector families

A family is a selection alias, not another mutation. Selecting or excluding it
affects exactly the following members. Individual default states are in the index.

### `bitwise_ops`

- [`bitwise_and_or`](operators.md#bitwise_and_or)
- [`bitwise_shift`](operators.md#bitwise_shift)
- [`bitwise_xor`](operators.md#bitwise_xor)
- [`bitwise_invert`](operators.md#bitwise_invert)
- [`augmented_bitwise_and_or`](operators.md#augmented_bitwise_and_or)
- [`augmented_bitwise_xor`](operators.md#augmented_bitwise_xor)
- [`augmented_bitwise_shift`](operators.md#augmented_bitwise_shift)

### `call_contracts`

- [`stdlib_escape_remove`](operators.md#stdlib_escape_remove)
- [`call_argument_swap`](operators.md#call_argument_swap)
- [`call_result_default`](operators.md#call_result_default)

### `collection_ops`

- [`collection_any_all`](operators.md#collection_any_all)
- [`collection_list_tuple`](operators.md#collection_list_tuple)
- [`collection_set_frozenset`](operators.md#collection_set_frozenset)
- [`collection_append_insert`](operators.md#collection_append_insert)
- [`collection_min_max`](operators.md#collection_min_max)
- [`collection_set_add_discard`](operators.md#collection_set_add_discard)
- [`collection_set_remove_discard`](operators.md#collection_set_remove_discard)
- [`collection_string_starts_ends`](operators.md#collection_string_starts_ends)
- [`collection_string_split_rsplit`](operators.md#collection_string_split_rsplit)

### `collection_processing`

- [`comprehension_filter_remove`](operators.md#comprehension_filter_remove)
- [`comprehension_filter_invert`](operators.md#comprehension_filter_invert)
- [`filter_predicate_swap`](operators.md#filter_predicate_swap)
- [`set_union_intersection_swap`](operators.md#set_union_intersection_swap)
- [`aggregate_sum_count`](operators.md#aggregate_sum_count)

### `exception_ops`

- [`exception_type_pair`](operators.md#exception_type_pair)

### `exception_risky`

- [`exception_bare_to_exception`](operators.md#exception_bare_to_exception)
- [`exception_exception_to_bare`](operators.md#exception_exception_to_bare)
- [`exception_base_boundary`](operators.md#exception_base_boundary)
- [`exception_tuple_add_pair`](operators.md#exception_tuple_add_pair)
- [`exception_tuple_remove_member`](operators.md#exception_tuple_remove_member)

### `regex`

- [`regex_match_boundary`](operators.md#regex_match_boundary)
- [`regex_character_set`](operators.md#regex_character_set)
- [`regex_ascii_mode`](operators.md#regex_ascii_mode)

### `rounding`

- [`rounding_function_swap`](operators.md#rounding_function_swap)
- [`round_digits_remove`](operators.md#round_digits_remove)
- [`decimal_rounding_mode_swap`](operators.md#decimal_rounding_mode_swap)
- [`decimal_quantum_shift`](operators.md#decimal_quantum_shift)
- [`decimal_context_precision`](operators.md#decimal_context_precision)
- [`intermediate_rounding_remove`](operators.md#intermediate_rounding_remove)

### `structure_ops`

- [`structure_append_extend`](operators.md#structure_append_extend)
- [`structure_mapping_get_subscript`](operators.md#structure_mapping_get_subscript)
- [`structure_sort_reverse`](operators.md#structure_sort_reverse)
- [`structure_sorted_reversed`](operators.md#structure_sorted_reversed)
- [`structure_index_neighbor`](operators.md#structure_index_neighbor)
- [`structure_slice_neighbor`](operators.md#structure_slice_neighbor)
- [`structure_slice_bound_omit`](operators.md#structure_slice_bound_omit)
- [`structure_slice_bound_move`](operators.md#structure_slice_bound_move)

### `type_collections`

- [`type_list_sequence`](operators.md#type_list_sequence)
- [`type_set_abstract_set`](operators.md#type_set_abstract_set)
- [`type_dict_mapping`](operators.md#type_dict_mapping)

### `type_iterables`

- [`type_iterable_iterator`](operators.md#type_iterable_iterator)
- [`type_sequence_iterable`](operators.md#type_sequence_iterable)

### `type_nullable`

- [`type_nullable_remove`](operators.md#type_nullable_remove)
- [`type_nullable_add`](operators.md#type_nullable_add)
