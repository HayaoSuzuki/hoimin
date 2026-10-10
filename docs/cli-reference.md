# CLI reference

<!-- Generated; do not edit by hand. -->

Source: [Rust CLI definitions](../crates/hoimin-cli/src/cli.rs).

Regenerate with `cargo run --locked -p hoimin-cli --example generate_cli_reference`; append ` -- --check` to verify without writing.

See the [usage guide](usage.md) for examples, OS-specific resource limits,
plan/verify inheritance and validation beyond Clap. Conditional requirements
and numeric parser bounds have no general stable Clap reflection API; their
help text and the manual guide remain authoritative. The metadata below lists
reflected arity, repetition, required arguments/groups, conflicts and delimiters.

## hoimin

```text
Bounded mutation testing for focused Python changes

Usage: hoimin <COMMAND>

Commands:
  run          Run mutation tests for an explicitly selected target
  plan         Discover mutation candidates without executing tests
  verify       Execute one or more candidates using settings from a previously generated plan
  progress     Compare chronologically ordered mutation run reports
  completions  Generate shell completion scripts
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

  -V, --version
          Print version

Run contract:
  hoimin run [TARGETS] [SAFETY/OUTPUT/SESSION OPTIONS] -- <TEST_ARGV>...

Target selectors:
  --root --source --file --line --symbol --changed --changed-context --diff-base
Copy options:
  --include --exclude
Mutation options:
  --operators --exclude-operators --profile
Safety options:
  --jobs --max-mutants --max-candidates --analyzer-timeout
  --baseline-timeout --mutant-timeout --total-timeout --max-memory
  --max-output --max-copy-size --max-workspace-size --min-free-space
  --max-processes --allow-best-effort-memory
Output/session options:
  --format <json|jsonl|human> --metrics <PATH> --session --resume
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

## hoimin verify

```text
Execute one or more candidates using settings from a previously generated plan

Usage: hoimin verify [OPTIONS] <--candidate <ID>|--top <N>|--sample <N>> <PLAN>

Arguments:
  <PLAN>
          Path to a version-5 plan manifest

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

      --format <FORMAT>
          Machine-readable output format

          [default: json]
          [possible values: json, jsonl, human]

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
--format
  required: false
  arity: 1
  repeatable: false
  default: json
  values: json, jsonl, human
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
      --fail-on-regression
          Exit 1 when the latest state is regressing; indeterminate stays 0, errors stay 2

      --patience <PATIENCE>
          Consecutive unchanged comparisons before the history is saturated

          [default: 3]

      --format <FORMAT>
          Render the comparison as human-readable text or JSON

          [default: human]
          [possible values: human, json]

      --details
          Show identified changes in the final adjacent input pair (JSON selects schema v2)

      --details-limit <DETAILS_LIMIT>
          Maximum transition details to show; zero reports only omission counts

          [default: 100]

  -h, --help
          Print help
```

Argument constraints:

```text
--fail-on-regression
  required: false
  arity: 0
  repeatable: false
  default: false
--patience
  required: false
  arity: 1
  repeatable: false
  default: 3
--format
  required: false
  arity: 1
  repeatable: false
  default: human
  values: human, json
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

## hoimin completions

```text
Generate shell completion scripts

Usage: hoimin completions <SHELL>

Arguments:
  <SHELL>
          Shell to generate completions for

          [possible values: bash, elvish, fish, powershell, zsh]

Options:
  -h, --help
          Print help
```

Argument constraints:

```text
shell
  required: true
  arity: 1
  repeatable: false
  values: bash, elvish, fish, powershell, zsh
--help
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
          Include only named mutation operators; omit for the default runtime set. Repeatable or comma-delimited

      --profile <PROFILE>
          Candidate-selection profile

          [default: full]
          [possible values: full, focused]

      --exclude-operators <EXCLUDE_OPERATORS>
          Exclude named mutation operators; may be repeated or comma-delimited

      --jobs <JOBS>
          Maximum concurrently active workers

          [default: 1]

      --max-mutants <MAX_MUTANTS>
          Maximum mutants to execute

          [default: 100]

      --max-candidates <MAX_CANDIDATES>
          Maximum candidates to discover before mutation starts

          [default: 10000]

      --analyzer-timeout <DURATION>
          Per-analyzer-process timeout

          [default: 30s]

      --baseline-timeout <DURATION>
          Baseline test timeout

          [default: 60s]

      --mutant-timeout <DURATION|auto>
          Per-mutant timeout or `auto`

          [default: auto]

      --total-timeout <DURATION>
          Wall-clock timeout for the complete run

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

      --format <FORMAT>
          Machine-readable output format

          [default: json]
          [possible values: json, jsonl, human]

      --metrics <PATH>
          Write performance metrics to PATH

      --session <PATH>
          `SQLite` session path; no database is created unless specified

      --resume
          Resume the newest compatible incomplete run in the session database

  -h, --help
          Print help

Mutation operator IDs and selector families:
  augmented_add_sub, augmented_bitwise_and_or, augmented_bitwise_shift, augmented_bitwise_xor
  augmented_floor_mod, augmented_matmul, augmented_mul_div, augmented_power
  augmented_to_assignment, binary_add_sub, binary_floor_mod, binary_matmul
  binary_mul_div, binary_power, bitwise_and_or, bitwise_invert
  bitwise_ops, bitwise_shift, bitwise_xor, boolean_and_or
  boolean_literal, break_continue, collection_any_all, collection_append_insert
  collection_list_tuple, collection_min_max, collection_ops, collection_set_add_discard
  collection_set_frozenset, collection_set_remove_discard, collection_string_split_rsplit, collection_string_starts_ends
  compare_eq_ne, compare_order, condition_clause_delete, condition_constant
  container_element_delete, conversion_call_remove, enum_member_replace, exception_bare_to_exception
  exception_base_boundary, exception_exception_to_bare, exception_hierarchy, exception_ops
  exception_risky, exception_tuple_add_pair, exception_tuple_remove_member, exception_type_pair
  function_body_erase, function_body_return_constant, identity, integer_literal_neighbor
  membership, method_call_remove, operator_function, optional_keyword_delete
  remove_not, return_tuple_swap, statement_delete, string_literal_empty
  string_segment_empty, structure_append_extend, structure_index_neighbor, structure_mapping_get_subscript
  structure_ops, structure_slice_neighbor, structure_sort_reverse, structure_sorted_reversed
  type_collections, type_dict_mapping, type_iterable_iterator, type_iterables
  type_list_sequence, type_nullable, type_nullable_add, type_nullable_remove
  type_sequence_iterable, type_set_abstract_set, unary_sign, while_condition_false
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
--profile
  required: false
  arity: 1
  repeatable: false
  default: full
  values: full, focused
--exclude-operators
  required: false
  arity: 1
  repeatable: true
  delimiter: ,
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
--analyzer-timeout
  required: false
  arity: 1
  repeatable: false
  default: 30s
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
--format
  required: false
  arity: 1
  repeatable: false
  default: json
  values: json, jsonl, human
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
          Include only named mutation operators; omit for the default runtime set. Repeatable or comma-delimited

      --profile <PROFILE>
          Candidate-selection profile

          [default: full]
          [possible values: full, focused]

      --exclude-operators <EXCLUDE_OPERATORS>
          Exclude named mutation operators; may be repeated or comma-delimited

      --jobs <JOBS>
          Maximum concurrently active workers

          [default: 1]

      --max-mutants <MAX_MUTANTS>
          Maximum mutants to execute

          [default: 100]

      --max-candidates <MAX_CANDIDATES>
          Maximum candidates to discover before mutation starts

          [default: 10000]

      --analyzer-timeout <DURATION>
          Per-analyzer-process timeout

          [default: 30s]

      --baseline-timeout <DURATION>
          Baseline test timeout

          [default: 60s]

      --mutant-timeout <DURATION|auto>
          Per-mutant timeout or `auto`

          [default: auto]

      --total-timeout <DURATION>
          Wall-clock timeout for the complete run

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
          Print help

Mutation operator IDs and selector families:
  augmented_add_sub, augmented_bitwise_and_or, augmented_bitwise_shift, augmented_bitwise_xor
  augmented_floor_mod, augmented_matmul, augmented_mul_div, augmented_power
  augmented_to_assignment, binary_add_sub, binary_floor_mod, binary_matmul
  binary_mul_div, binary_power, bitwise_and_or, bitwise_invert
  bitwise_ops, bitwise_shift, bitwise_xor, boolean_and_or
  boolean_literal, break_continue, collection_any_all, collection_append_insert
  collection_list_tuple, collection_min_max, collection_ops, collection_set_add_discard
  collection_set_frozenset, collection_set_remove_discard, collection_string_split_rsplit, collection_string_starts_ends
  compare_eq_ne, compare_order, condition_clause_delete, condition_constant
  container_element_delete, conversion_call_remove, enum_member_replace, exception_bare_to_exception
  exception_base_boundary, exception_exception_to_bare, exception_hierarchy, exception_ops
  exception_risky, exception_tuple_add_pair, exception_tuple_remove_member, exception_type_pair
  function_body_erase, function_body_return_constant, identity, integer_literal_neighbor
  membership, method_call_remove, operator_function, optional_keyword_delete
  remove_not, return_tuple_swap, statement_delete, string_literal_empty
  string_segment_empty, structure_append_extend, structure_index_neighbor, structure_mapping_get_subscript
  structure_ops, structure_slice_neighbor, structure_sort_reverse, structure_sorted_reversed
  type_collections, type_dict_mapping, type_iterable_iterator, type_iterables
  type_list_sequence, type_nullable, type_nullable_add, type_nullable_remove
  type_sequence_iterable, type_set_abstract_set, unary_sign, while_condition_false
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
--profile
  required: false
  arity: 1
  repeatable: false
  default: full
  values: full, focused
--exclude-operators
  required: false
  arity: 1
  repeatable: true
  delimiter: ,
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
--analyzer-timeout
  required: false
  arity: 1
  repeatable: false
  default: 30s
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

## hoimin help

```text
Print this message or the help of the given subcommand(s)

Usage: hoimin help [COMMAND]

Commands:
  verify       Execute one or more candidates using settings from a previously generated plan
  progress     Compare chronologically ordered mutation run reports
  completions  Generate shell completion scripts
  run          Run mutation tests for an explicitly selected target
  plan         Discover mutation candidates without executing tests
  help         Print this message or the help of the given subcommand(s)
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

## hoimin help completions

```text
Generate shell completion scripts

Usage: hoimin help completions
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

## hoimin help help

```text
Print this message or the help of the given subcommand(s)

Usage: hoimin help help
```
