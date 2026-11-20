# Research mutation integration verification

The subsequent [Windows workspace repair](../2026-11-20-windows-workspace-failures.md) addresses all 29 native failures recorded here. Its final all-features workspace run passes 17,424 tests with no failures; see that report for the verification details. The counts and decisions below describe the earlier research-mutation revisions.

This report records the final integration work for the nine research issues, based on 2e6cdc7502e974b7127e1891298c07fcfbb497c2. Use the domain reports for the individual eligibility rules and earlier test receipts. The managed Windows worktree used Rust 1.100.0 and CPython 3.14.9 and 3.15.1. Source and user-documentation work ends at `727ddb2f`; the test-only fixture commits are recorded below.

## Acceptance evidence

Each listed ID uses the existing catalog, default selection and individual exclusion. Fresh plans select 107 runtime IDs from the 126-ID catalog. The named cases in `current_default_candidates_are_individually_excludable_case` also exercise `base_order_shuffle`, `override_method_delete` and `super_call_move`, the three earlier default additions.

| Issue | IDs | Executable evidence |
| --- | --- | --- |
| #1190 | `finally_unwrap` | `cleanup_after_handler`, `cleanup_on_loop_transfer`, `cleanup_on_task_cancellation`, `suspended_generator_cleanup` and `partial_exception_group_skips_cleanup` cover fallthrough and distinct abrupt paths. `cleanup_overrides_pending_exception` observes a cleanup return and a raised exception with its context in separate CPython cases. `research_control_encoded_saved_cleanup` verifies the saved edit against original Latin-1, BOM and ASCII source bytes. |
| #1191 | `regex_match_boundary`, `regex_character_set`, `regex_ascii_mode` | `regex_acceptance`, `boundary_span` and `effective_mode_and_literal_spelling` check full, prefix and substring matches, observed spans, anchors, newline positions, complemented categories, Unicode wordness and scoped flags. `decoded_source` and `crossing_raw_backslash_parity_preserves_the_boundary` cover source-token mapping. `fix1_match_type_contract`, `fix1_conditional_conversion_contract` and the two class-union matrices check rejection and nearby safe cases. Analyzer work counters and `referenced_variants_obey_unlimited_ranked_prefix` check cached discovery and limited output. |
| #1192 | `stdlib_escape_remove` | `call_mutations_preserve_evaluation_effects` observes HTML and URL argument order, raised option evaluation and regex bytes. `call_contract_saved_verification` differentiates alphanumeric checks from metacharacter, ampersand and URL-component checks. Analyzer call-contract cases reject unsafe bindings. |
| #1195 | `call_argument_swap` | Positional receiver, keyword, mixed, awaited variadic and exception cases in `call_mutations_preserve_evaluation_effects` observe the value transposition and original evaluation trace. `call_contract_saved_verification` checks the distinct account roles. The delimiter and sparse-selection regressions check valid rewrites and bounded work. |
| #1196 | `call_result_default` | `call_mutations_preserve_evaluation_effects` checks cancellation, preserved errors, unevaluated branches and distinct fresh defaults. `call_contract_saved_verification` separates a write check from the returned-count assertion. Analyzer cases cover scope, consumed values and declared-return proof. |
| #1197 | `match_case_delete`, `match_guard_constant`, `match_or_alternative_delete` | `guard_short_circuits_after_capture`, `dead_arm_keeps_nonlocal_and_multiline_debug_string`, `deleted_pattern_does_not_read_class_attribute` and `generated_or_dispatch_matches_the_reference_decision` cover dispatch, bound values, side effects and nested patterns. `review_mapping_keys_match_cpython_compilation` and `review_guard_compiler_delimiter_boundary` compare compiler behavior. Saved controls distinguish fallback-only checks from targeted arms and alternatives. |
| #1198 | `comprehension_filter_remove`, `comprehension_filter_invert`, `filter_predicate_swap`, `set_union_intersection_swap`, `aggregate_sum_count` | `collection_and_builtin_rounding_contract` covers async and lazy filtering, predicate exceptions, receiver kind, empty sum and explicit values. `collection_filters_match_a_reference_loop_over_generated_lists` checks duplicates and ordered results. The generator/delimiter cases exercise compiler validity and `research_processing_saved_verification` checks membership-sensitive assertions. |
| #1199 | `rounding_function_swap`, `round_digits_remove`, `decimal_rounding_mode_swap`, `decimal_quantum_shift`, `decimal_context_precision`, `intermediate_rounding_remove` | `collection_and_builtin_rounding_contract` checks builtin dispatch, nonfinite behavior, ties and argument evaluation. `decimal_and_intermediate_precision_contract` checks mode precedence, exponents, signed zero, traps, flags, context restoration and rounding stage. `decimal_quantum_matches_integer_reference_across_signed_midpoints` compares against an independent numeric witness. |
| #1200 | `statement_order_swap` | `externally_observed_order` and `awaited_call_order` check state, assertions and event order. `review_order_attachment_and_binding` covers line directives and private dependencies. `research_control_saved_verification` distinguishes a call count from a trace. |

`research_operators_respect_public_diff_and_symbol_selection` exercises changed lines, a selected symbol and their intersection through the compiled CLI. It checks the selected sites and the `changed_line` ranking reason. `research_operator_budget_keeps_the_serialized_ranked_prefix` compares limits of 1, 3 and 5 with the full four-family plan, including exact capacity, IDs, order, rank, source edits, truncation exit status and a typed manifest round trip.

See the [control implementation](control-implementation.md), [call implementation](calls-implementation.md), [processing implementation](processing-implementation.md) and [regex implementation](regex-implementation.md) for their detailed acceptance matrices.

## Integration changes

- `statement_order.rs` owns the suite-local directive cache. It interleaves adjacent-pair checks with the same ordered statement visits and cancellation checks. The stopped-collector case goes through that path and retains the first admission error. Focused mode consults the existing arid range index for both exchanged statements.
- The regex emitter renders the scoped change after site selection and descriptor deduplication. The literal writer accepts replacement text or surrounding delimiters. A split raw token with an odd remaining trailing-backslash run uses the same decoded replacement writer. That writer preserves token prefixes, quotes, comments and needed separation, then checks the affected tokens with Ruff and compares decoded units.
- `usage.md` explains eligible calls, cleanup assertions, complements, adjacent word status and specification-dependent survivors. The compatibility entry records the current catalog size and saved behavior. The generated verify reference describes obtaining a manifest from `hoimin plan`.

## Windows test repair evidence

The Windows fixture changes are in four separate commits, `a8e8d572`, `b1030ad4`, `0d3970e8` and `79be0b66`.

- The focused frozen pytest RED reproduced eight failures and one teardown error. The repaired five files passed 203 with 29 skips. The real self-SIGTERM case uses the existing Windows skip contract. The diagnostic run excluded that case before repairing it.
- The Rust test compilation RED found a missing Unix cfg, a missing Windows trait and an unused output, then found three more Unix-only helpers and a by-value expected vector. Default-feature core lint independently reproduced the borrowed error-oracle finding. Final focused lint of CLI/core test targets with all features and default core oracle lint each exited 0.
- The reduction fixture produced nine failures with the Windows `python3` alias. Using the managed interpreter left one failure. An instrumented run proved the native cwd and the verbatim-prefix environment path address the same filesystem directory. The fixture now uses `os.path.samefile` and all 18 cases passed. Windows POSIX-test guards and assertions were preserved.
- Whole-repo Ruff format, Ruff lint and ty each exited 0. The complete frozen pytest run with the same seed as the baseline passed 2,674 with 76 skips and no deselection. The pytest suite covers Python tooling and fixtures. The compiled CLI is exercised separately below.
- Three Lean lifecycle cases checked a Unix cleanup message or translated Python stdout. The Windows helper's success message differs by design. A separate CPython probe confirmed CRLF on a printed line, so the exact-byte fixture now writes directly to the binary stream. All three cases passed with their original diagnostic and output checks.


## Native Windows workspace limitations

The first unfiltered workspace test run reported 2,639 passes, 37 failures and 27 ignored tests in the CLI library target. The same run continued into integration targets. The following diagnoses refer to the recorded failure bodies. `task-5-workspace-tests.log` preserves all case names, messages and exit details.

| Cases in the first CLI library run | Count | Evidence and disposition |
| --- | ---: | --- |
| Canonical ranking inventory | 1 | The five category lists contained 126 distinct IDs and the old count asserted 109. The correct 126 count and the independent set-equality requirement passed on rerun. This was the research integration's regression. |
| Metrics index injection and parent path | 4 | Three fault hooks exist only on the Unix index path. The Windows parent fixture now uses a verbatim absolute spelling. A native probe showed ordinary path normalization removes `missing\..` while the verbatim path returns NotFound. The applicable Windows cases passed. |
| Progress duplicate and source metadata | 2 | Opening a directory as a file returned Windows PermissionDenied and its public duplicate observer correctly omitted an annotation. Native metadata beneath a regular file returned NotFound. The fixture cases now verify those outcomes and retain their Unix assertions. |
| Worker temporary storage | 1 | Windows refused to rename a live, pinned temporary root before reaching the worker operation. The Windows fixture now arms the existing worker-temp injection. Both child cases passed. Unix retains the directory rename. |
| Locked lease and coordinator fixtures | 13 | Independent handles reading locked files returned Win32 error 33 before assertions for invalid prefixes, poisoned lease, reclamation control, contention, oversized namespace or heartbeat. The large namespace tests returned and the oversized selection test reached this read failure. |
| Renaming retained files | 3 | Win32 sharing violation 32 raised in live coordinator or lease rename setup before the intended identity test. |
| Root deletion capability | 3 | Two retry fixtures returned AccessDenied on unlink. Taking owner capabilities a second time failed with `managed root deletion capability is unavailable` before the Unix-style empty-slot assertion. |
| Platform-specific fault-injection paths | 6 | Four coordinator bootstrap hooks and one publication rollback hook did not produce the expected failure in their Windows path. The parent-inspection write fixture did not observe its expected error projection. |
| Janitor evidence paths | 4 | The ready/poisoned coordinator and malformed-lease fixtures reported different categories. A reclaim-removal fixture failed at a missing file in its evidence setup. |

The first eight failures in the repaired rows have separate focused green receipts or preserve Unix-only hooks under their platform condition. The 29 remaining cases exercise workspace ownership and native file capabilities independently of mutation discovery. A source comparison with the original revision found no production changes in the metrics, progress, target or workspace paths. Those wider native workspace cases remain visible in the full test logs and require separate ownership work.

The isolated source archive at `4cf630f977dc266c6633969b57d0f9f251bb91d8` reproduced ten representative failures covering all five categories left in the table. The archive carried only the test compilation guards for the Unix quiescence fault and Windows open-options trait. It built its own target with a single coverage-remapping option and used the same Windows host. The command ran ten named cases and exited 101 with all ten matching their failure stages. Ten of the 29 remaining cases were reproduced this way. The other 19 are classified by their failure stages and unchanged analysis-independent code paths. See `task-5-base-native-probe.log`.

## Saved-plan and report verification

All replay reports recorded the hard Windows Job Object backend. The preserved capture receipt identifies the hoimin 0.14.0 binary by SHA-256 `8f812c97aa188fee34163cdb1cce1161bdedddad21a9578df6934be03d425788`. The final default hoimin 0.15.0 binary had SHA-256 `e80c93226147ecc38549aa1d9bd73e9a0ea8a4e24569cb90721d56136b1da832`. The original binary still matches its capture hash. The final CLI loaded both original manifests with schema 10 and ranking rule 4. Both dry runs returned the saved IDs and ranks. Both executions passed the baseline and completed their JSONL reports with the same source edits, IDs and concrete operator set.

| Preserved plan | Operators | Candidates | Killed | Survived |
| --- | ---: | ---: | ---: | ---: |
| Explicit `binary_add_sub` | 1 | 1 | 1 | 0 |
| Default | 85 | 7 | 5 | 2 |

Manifest and source SHA-256 values were identical before and after execution. Full receipts and raw CLI outputs are beside `task-5-replay.py` in the ignored workspace. This evidence establishes replay for these two actual captures.

A separate public CLI run planned five mutations spanning cleanup, regex character sets, escaping, comprehension filtering and rounding. Ordinary inputs produced five survivors. Testing an early return, a Unicode digit, an ampersand, filtered values and an explicit rounding boundary produced five kills from the same saved plan. Both baselines passed, and `hoimin progress` reported `improving`. Both the saved manifest and the subject stayed byte-identical. These checks used the arbitrary saved test command `python -B check.py`.

## Implementation reviews

1. **Catalog and compatibility.** Checked the 22 additions against catalog names and the default/exclusion fixtures. Confirmed the generated verify help now describes a plan manifest without a stale schema number. The raw 0.14 captures check concrete resolved sets independently of fresh defaults.
2. **Selection and ranking.** Inspected unselected family sites in the mixed fixture and the candidate start of a multiline cleanup. The diff and symbol paths reuse selection and the candidate matcher. The limited plan's typed round trip retains rank, scores and edit payloads.
3. **Directive lifetime and focused selection.** Checked that one trailing comment applies across its semicolon line and a nested suite creates its own state. Cancellation and sink failure are checked at the same points. Each participant in an exchange uses the existing focused-profile arid-span index.
4. **Regex representation.** Reviewed per-variant rendering, scope delimiters, raw parity, encoded backslashes, multi-token covers, comments and quote separation. Only the literal writer reasons about token spelling. Reference materialization still follows selection, deduplication and budget checks.
5. **Platform contracts.** Reviewed all modified mocks and cfg guards against their call sites. The Windows fixtures use a real signature or a native capability double as appropriate. Actual signals and shell permissions retain their real platform prerequisites. The reduction path assertion compares directory identity.

## Test reviews

1. **Cleanup oracle.** The CPython return and raise cases check different baseline outcomes and the exact original exception under mutation. The raise case checks the original exception context. Each test selects one edit by source span.
2. **Regex oracles.** Reflowed the generated cases without changing their bounds, number of cases, polarity, decimal repertoire, capture or span checks. The prefix property still checks truncation, retained order and generated bytes. The split-raw witness executes both forms.
3. **Public selection coverage.** Separate named cases cover diff, symbol and intersection. The unchanged source family is deliberately present. Ranking assertions distinguish diff attribution from ordinary symbol selection. Focused arid tests cover the first, second and both statements, with ordinary-focused and full-profile controls.
4. **Serialized plan and report checks.** Compared bounded candidates with the independent unlimited run, and compared report fields with the saved plan's corresponding projection. Selection-only ranks are checked through dry-run preview. Both old selection sizes and full verification outcomes have receipts.
5. **Failure modes and fixtures.** Fresh RED receipts captured the Windows failures before enabling their intended branches. The SIGTERM crash was excluded from reproduction after the prior diagnostic identified it. Supported-platform tests retain their signal behavior. Test command and source checks used the real executable at both Python versions.

## Software design diagnostics

| Diagnostic | Result | Evidence |
| --- | --- | --- |
| Each module has a single describable purpose | Pass | Statement order checks adjacent statement exchanges. Regex syntax and literal mapping have separate responsibilities. |
| Interfaces are simpler than implementations | Pass | The family still emits source spans and replacement text. The literal writer handles quoting behind two edit shapes. |
| Internal changes can stay local | Pass | Directive cache representation stays in the order module, and scope spellings stay outside the token writer. |
| Interface comments describe the abstraction | Pass | The suite entry documents ordered visits. The surrounding edit documents which delimiters can be inserted directly. |
| Reviews consider design | Pass | The reviews above cover ownership, work bounds, wire compatibility and platform assumptions. |
| Important decisions are hidden | Pass | Eligibility and source representation stay in the relevant domains. Candidate admission stays with the common collector. |
| Boundaries can be understood without reading implementations | Pass | The acceptance table and usage guide describe effects, the domain reports describe eligibility, and the public CLI contracts cover the shared boundary. |
| Design-improvement time is measured at 10–20 percent | Unverified | The work records specific design corrections and executable checks. No contemporaneous timing split was captured. |

Seven of eight diagnostics are supported, for 8.75/10. The independent Task 5 source review approved `727ddb2f`. It deferred two minor observations about the suite call name and existing Git line-ending warnings to the whole-branch review.

## Final verification

The public focused-profile RED in the original workspace run showed an extra line-2 candidate from exchanging two separately arid statements. The additional analyzer cases reproduced three omissions in the profile check, with two positive controls passing. The fix uses the existing arid facts for each statement. The same analyzer matrix and the original public focused-profile test passed. The original expected candidate list passed.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` and `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check` | Exit 0. |
| `cargo clippy --locked -j 2 --quiet --workspace --all-targets --all-features -- -D warnings` | Final exit 0 after the focused-profile follow-up. The initial diagnostic run identified six lint findings. They were corrected and verified. |
| `cargo clippy --locked -j 2 --quiet -p ruff_python_parser --lib --no-deps -- -D warnings` | Exit 0. |
| `cargo test --locked -j 2 --quiet --workspace --no-fail-fast -- --test-threads=2` | Exit 101. Reached all 200 harnesses, with 17,361 passed, 41 failed and 45 ignored, excluding nested child recounts. Two task-owned regressions and ten fixture cases have separate green follow-ups. The remaining 29 native workspace cases are detailed above. |
| `cargo test --locked -j 2 --quiet -p hoimin-core --features contracts --no-fail-fast -- --test-threads=2` | Exit 0. 3,865 passed and 10 ignored across 48 harnesses. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --features contracts --no-fail-fast -- --test-threads=2` | Completed under the CI profile with exit 101. Across 145 unfiltered harnesses, 8,298 passed, 29 failed and 30 were ignored. The sole failed target was CLI lib, and its exact 29 failures matched the remaining native workspace set. All integration targets passed, including 139 run_e2e cases. |
| `cargo test --locked -j 2 --quiet -p hoimin-python --lib -- --test-threads=2 regex research_control_tests stopped_collector_tests` | Exit 0. 408 passed after the ownership refactors. |
| `cargo test --locked -j 2 --quiet -p hoimin-python --lib statement_order_profile_filters_arid_operands -- --test-threads=2` | RED exit 101. Three arid cases failed and two controls passed. |
| `cargo test --locked -j 2 --quiet -p hoimin-python --lib research_control_tests -- --test-threads=2` | GREEN exit 0. All 61 structural cases passed, including five focused-profile rows. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --test run_e2e focused_profile_is_reported_and_omits_arid_candidates -- --test-threads=2` | GREEN exit 0. The original focused-profile test passed. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --lib -- --test-threads=2` with ranking inventory and the repaired native fixture filters | GREEN exit 0. 11 passed, with two nested copy child checks also passing. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --test lean_result_lifecycle_oracle -- --test-threads=2` with the three native fixture filters | GREEN exit 0. 3 passed. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --test analyzer_line_selection --test operator_function_contracts --test default_operator_selection -- --test-threads=2` on 3.14.9 | Exit 0. 9 public selector, 241 runtime contract and 117 default-selection checks passed. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --test operator_function_contracts --test default_operator_selection research -- --test-threads=2` on 3.15.1 | Exit 0. 182 runtime and 23 saved/default checks passed. |
| `cargo run --locked -j 2 --quiet -p hoimin-cli --example generate_cli_reference -- --check` and the generator example test target | Exit 0 for both. 7 generator tests passed. |
| `uv run --frozen --no-sync ruff format --check .`, `ruff check --no-fix .`, and `ty check` through the same environment | All exited 0. |
| `uv run --frozen --no-sync pytest -q --tb=short --randomly-seed=2367152648` | Exit 0. 2,674 passed, 76 skipped. |
| `uv run --frozen --no-sync python -m tools.docs_site --output site` | Exit 0 after the focused-profile guide update. The built pages include the new guide text and manifest help. |
| `cargo build --locked -j 2 --quiet -p hoimin-cli --bin hoimin` and `task-5-replay.py` with CPython 3.14.9 | Exit 0 for both after the focused-profile change. Both actual 0.14.0 manifests replayed and the mixed comparison improved. |

All raw command outputs and the status receipt are in the ignored research-mutations workspace. Verification used two Cargo build jobs and two Rust test threads. The initial CLI contracts build was interrupted during compilation to preserve disk capacity. It produced no test result. The replacement follows CI with `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and `CARGO_INCREMENTAL=0`. Debug assertions and the full test selection are active.

The Windows run exercises supported process capabilities and the controlled doubles. Real POSIX process-group and shell cases remain available on their supported host. Static callable provenance, bounded regex syntax, and a survivor's dependence on the selected tests remain limits of the feature. The whole-branch review follows this implementation report.
