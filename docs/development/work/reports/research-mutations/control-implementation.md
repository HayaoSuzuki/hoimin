# Task 1 structural mutation report

Base commit 909bfe855079e0fab00ef322268766f2a630354d. The five stable IDs are finally_unwrap, match_case_delete, match_guard_constant, match_or_alternative_delete and statement_order_swap. All five participate in default planning, exact individual selection, exclusion, ranking and saved verification. Existing selector families retain their established meanings.

## Implementation and files

- crates/hoimin-core/src/config.rs appends the five catalog variants and defaults. Its operator_names and operator_selection integration targets pin the wire names and expanded defaults.
- crates/hoimin-python/src/analyzer/rust/control_structure.rs handles finally ownership, dead case headers, short-circuit guards, capture/reachability checks and streaming OR deletions. It validates grouped and explicitly continued input using tokens from the checked parse.
- crates/hoimin-python/src/analyzer/rust/statement_order.rs exchanges adjacent eligible siblings. It checks local-name dependencies, directives and unsupported scope/transfer forms. Both ordinary and awaited calls use the same path. Cases with direct local read/write dependencies are conservatively excluded even when an earlier local value might exist.
- crates/hoimin-python/src/analyzer/rust.rs visits module and nested suites once, retains Ruff's match-case walking, and uses existing source selection and candidate admission. rust/research_control_tests.rs, rust/stopped_collector_tests.rs and rust_tests.rs cover exact spans, generated edits, sparse selection, cutoff on failed admission and prior operators.
- crates/hoimin-cli/src/plan/ranking.rs and ranking_tests.rs classify cleanup as exception handling, case/guard/OR as high-value control and adjacent exchange as behavioral. src/analyzer/payload_probes.rs lists the additions separately from the pinned measurements.
- crates/hoimin-cli/tests/behavioral_research/control_contracts.rs uses the actual JSON plan, candidate byte ranges and CPython interpreter through tests/operator_function_contracts.rs. default_operator_selection.rs verifies per-ID exclusions, saved weak/strong checks and source encodings. cli_config.rs and fixtures/valid-python-operators.json update the catalog contracts.
- docs/user/guides/usage.md gives individual exclusions and operator behavior. docs/user/reference/cli.md is generated from the CLI command metadata.

## Red and green evidence

Before any emitter or ID existed, cargo test -j 2 --quiet -p hoimin-python research_control_tests -- --test-threads=2 finished with exit 1. Ten positive cases observed empty candidate lists. Six negative controls passed. An earlier helper build error moved leased strings out of a shared candidate. Cloning the read values fixed that setup before collecting the actual RED. First GREEN passed 16/16 with exit 0.

Review regressions were reproduced separately. OR token joining failed two cases with empty replacement text instead of a required space, directive and compiler rejection failed three of 38 cases, and four failed-producer tests counted callbacks after failure. A real compiler probe reproduced two invalid case headers with backslash-indented bodies. The corrected implementation includes independent tests of each fault. Twelve existing operator-specific assertions initially observed newly enabled control candidates. Their fixtures now request the operators they assert about.

cargo test -j 2 --quiet -p hoimin-core -p hoimin-python -- --test-threads=2 exited 0 with 8,729 passed and 15 ignored across all targets. The Python lib alone passed 4,830 and ignored five. The full output is in task-1-core-python.log beside this report. This run preceded the final continued-tab correction and match-state field grouping, which have separate focused reruns. Passing both a cargo and a harness --quiet caused two earlier invocations to exit before running any tests. All reported test runs use one quiet flag.

The combined cli_config, default_operator_selection and operator_function_contracts targets initially passed 311, 76 and 95 cases respectively. The ten per-operator weak/strong saved verifications were part of that run. CLI behavior uses the worktree's CPython 3.14.9. Final checks are recorded below.

## Five implementation reviews

1. **Keyword, grouping and token boundaries.** Traced ownership through bare try, handled try, nested groups, OR nested in sequences and AS patterns. The top-level OR delimiter scan preserved grouping but deleting the left side of “case(1)|2” joined the surviving literal to the case keyword. The analogous final deletion in “case 1|(2)as y” joined the literal to AS. Added a separator exactly at a joining identifier/number boundary.
2. **Compiler scope and indentation.** Compared CPython compilation of captures, nonlocals, generator guards, class-pattern reads and continued logical lines. A token's INDENT range can contain a backslash and a second physical line. Copying the entire token as a generated indent broke two valid originals. The first correction extracted leading whitespace, which still raised TabError in one of three new CPython regressions. A continued tab indent has a different alternate-column comparison than a physical tab indent. The rewrite retains the full continuation spelling and adds any required spaces before its first backslash. Newline spelling comes from local tokens. The original suite and its string contents stay in place. Validate duplicate constant keys using Ruff's Python-equality-aware HashableExpr and reject forbidden __debug__ bindings before emitting match mutations.
3. **Ordering and source trivia.** Reviewed dependencies through bare names, attribute/subscript receivers and indexes, validation assertions, awaited effects and explicit super calls. Scanning for the second statement's trailing directive through the next logical newline crossed into an unrelated third multiline statement and suppressed a legal pair. Restrict the trailing scan to the physical line. Cache the result in the suite so a long semicolon-separated line is scanned once for its trailing directive.
4. **Selection and failed admission.** Reviewed all four emitter entry points, lazy selection, bounded candidate storage and OR scanning. Instrumented prefailed collectors observed 1, 2, 1 and 5 unnecessary cancellation polls. Stop helpers before inspecting the site and return the existing admission failure before invoking cancellation. The real collector retains and serializes the original candidate format. The byte ledger bounds overlapping nested finally proposals.
5. **Traversal and integration.** Removed duplicate match child traversal from the initial implementation. The collector scopes match eligibility around Ruff's existing walk_stmt/walk_match_case hooks, allowing nested valid matches to restore their enclosing state. Module entry goes through the same suite-pair walk as function, class, match, handler, else and cleanup suites. Checked that ranking source, selector names, exclusions, reference generation and the original explicit-selection contracts agree.

## Five test reviews

1. **Positive and negative oracles.** The initial ten positive cases used exact original/replacement pairs and real default discovery. Negative controls alone could pass with a missing operator. Added separate illegal reachability, mismatched OR bindings, duplicate captures, equivalent literal keys and forbidden-name cases, and confirmed that the latter two failed before the correction.
2. **Execution versus parse.** Added CPython cases verifying actual cleanup on normal/handled, return, escaping/grouped exceptions, break, continue, generator close versus exhaustion and asyncio cancellation. Pattern cases exercise one subject evaluation, attribute side effects, generator classification, preserved nonlocal bindings, guard-short-circuit side effects and surviving OR captures. Order cases check invalid-save prevention, observer-visible state, ordered calls and await ordering.
3. **Formatting and generated inputs.** Bounded proptests generate 32 examples each of nested cleanup formatting, semicolon/CRLF suites and equal-binding OR alternatives. A separate twelve-example property executes a generated route against a deletion-independent reference function in CPython. Added three real CPython regressions for explicit continuations in case/body indentation. Finite cases have separate named invocations.
4. **Fault detection and public reports.** Executed actual saved plans independently under weak and strong application checks. A normal-only cleanup assertion, a fallback-only match check and a call-count assertion allowed mutants to survive. Checks of an early return, the selected/guarded alternatives and the exact notification sequence killed them. Added separate Latin-1, BOM and ASCII saved-source tests verifying raw-byte restoration.
5. **Admission, attribution and independence.** Added isolated tests for strict zero-payload failure, line/symbol anchors, compiler indentation limits and zero work on a prefailed sink. Corrected legacy default-sensitive tests by expressing their own nominal operator selection. No fixtures remove a claimed candidate to accommodate a malformed edit. Reviewed AAA boundaries for generated and finite cases, including temporary subjects and captured output from independent interpreter processes.

## Debug CLI measurements

Individual synthetic inputs used the selected operator, the real debug CLI and CPython. Elapsed milliseconds cover process startup and plan serialization. These single observations are not benchmark distributions. The first process was cold. Payload below counts bytes of original plus replacement source text, excluding candidate metadata. Duplicate counts compare exact span, original and replacement against all pre-existing default operators on the same input.

| Operator | Candidates | Replacement bytes | Source payload bytes | Milliseconds | Exact duplicates | Weak survivors | Strong kills |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| finally_unwrap | 1 | 36 | 68 | 1692 | 0 | 1 | 1 |
| match_case_delete | 1 | 54 | 63 | 36 | 0 | 1 | 1 |
| match_guard_constant | 2 | 36 | 46 | 32 | 0 | 2 | 2 |
| match_or_alternative_delete | 2 | 0 | 8 | 35 | 0 | 2 | 2 |
| statement_order_swap | 1 | 26 | 52 | 37 | 0 | 1 | 1 |

Depth-four nested bare cleanup produced four candidates, 284 replacement bytes and 31 ms. Depth twelve produced twelve candidates, 3,028 replacement bytes and 34 ms. Their contiguous replacement spans intentionally overlap because the bare form changes both headers. Use the shared payload cap to limit retained text. Weak/strong results come from the dedicated behavioral fixtures, not from the timing fixtures.

## Eight design diagnostics

1. Module purposes are concise. Control structure handles protected suites and dispatch. Statement order decides when two complete adjacent fragments can move.
2. Callers pass AST nodes into private source-emitting methods. They do not construct patch plans or discover token boundaries.
3. Existing consumers, serialized candidate structures, verification commands and source encodings continue to use the established interfaces.
4. Module and local comments explain lexical scope, token joining, logical indentation and physical directive attachment. These invariants explain restrictions that are invisible to a caller.
5. Review changed the design through reuse of the match walker, streaming OR emission, failed-sink rejection and a local directive cache.
6. Source spans, capture matching, directive details, and name footprints are private to their respective modules.
7. The root design table, module summaries and CLI operator descriptions explain the boundaries without reading implementation branches.
8. The implementation includes measured design-improvement work on traversal reuse, directive scanning, retention limits and compiler probes. No timesheet measured the exact percentage spent on design, so the numeric 10–20 percent target remains unverified.

The first seven diagnostics have direct supporting evidence. Broader measurements across real projects, all nine issues and multiple supported Python versions belong to final integration.
## Final verification

- The 38 focused analyzer cases passed with the final continued-indent logic and grouped traversal state. The 26 CLI/CPython behavior cases, including all three continued-indent cases and the twelve-draw execution property, passed.
- All 13 saved-plan control cases passed, including independent weak/strong checks and raw-source restoration for the three encodings.
- Four valid-Python inventory integration cases passed. A public CLI JSON check verified seven distinct planned candidates covering all five operator IDs, with exception_handling 90, high_value_control 100 and behavioral 80 as assigned.
- The existing Windows CLI lib-test harness could not compile the internal ranking-test target: process/coverage_tests.rs calls a missing PortableSupervisor test method, and workspace/root_coverage_tests.rs uses follow without importing OpenOptionsFollowExt. A separate unused trees warning occurs in metrics_destination.rs. These unrelated paths were reported for the final integration task. Ranking behavior was independently exercised through the public binary.

- Final strict Clippy passed for all hoimin-python targets, the core library and both changed core test targets, the CLI library and four CLI integration targets. The separate broader core all-targets Clippy command found an unrelated pre-existing needless_pass_by_value in crates/hoimin-core/tests/lean_report_sequence_oracle.rs:569. Task 5 owns the overall workspace gate.
- cargo fmt --all --check, generate_cli_reference -- --check and git diff --cached --check exited 0. After the lint-only test edits the 38 analyzer cases and all 27 direct failed-producer cases passed again.
- Committed 19 task paths as b03a5db6e02b80ac038e06081f738ccc1ea30716. The only nonignored working-tree entry after commit is the root agent's separate windows-baseline.md.
## Controller review fix round 1

The review used b03a5db6e02b80ac038e06081f738ccc1ea30716. All four Important findings reproduced before the fix.

- A standalone pylint disable-next comment above the first statement changed its attachment after exchange. The new check inspects the preceding tokens for a comment-only physical line, preserving swaps below an unrelated statement's trailing comment. Additional public CLI probes each initially produced one incorrect swap for a UTF-8 BOM preceding pylint, a Pyre fixme and a noinspection directive. Those layouts are now negative cases.
- Forcing a guard adds two nested delimiters. Scan the checked tokens from the guard's if keyword through the last expression token, including redundant grouping omitted from the AST. Skip the wrapper when the peak exceeds 198, which would exceed CPython's 200-level limit. Tests compiled all original subjects; 198 pairs admitted both mutations and the true replacement executed, while 199, 200 and a call containing 198 pairs admitted none.
- A method's private identifier and its explicit mangled spelling designate the same local slot. Dependency footprints now use the nearest class identity, including inherited lexical context in nested functions. Exception analysis and order analysis reuse one name-mangling helper. Separate tests cover both store/read directions, a class with leading underscores and a distinct other-class prefix that remains eligible. CPython printed 2 before the planner rejected the invalid exchange.
- Numeric mapping keys now use exact BigInt values for integer literals and integral floats or complex numbers with zero imaginary parts. Complex construction converts the real component with the existing Python-compatible evaluator. Radix spelling, negative values, a one-unit distinction beyond binary64 precision, nonzero complex values and 18446744073709551616+0j were checked. The CPython oracle compiled the distinct adjacent integer case and rejected both equal-key inputs.

Changed paths were crates/hoimin-python/src/analyzer/exception_hierarchy.rs, analyzer/rust.rs and its control_structure.rs, statement_order.rs, mapping_keys.rs, research_control_tests.rs and stopped_collector_tests.rs modules, plus crates/hoimin-cli/tests/behavioral_research/control_contracts.rs. The changes use existing candidate identities and selection/admission paths.

### Focused command evidence

- RED: cargo test -j 2 --quiet -p hoimin-python research_control_tests::review_ -- --test-threads=2 returned 101. Nine of thirteen tests failed and four positive controls passed.
- RED: cargo test -j 2 --quiet -p hoimin-cli --test operator_function_contracts review_guard_compiler -- --test-threads=2 returned 101. Three near-limit inputs incorrectly produced both edits; 198-level grouping passed.
- GREEN: cargo test -j 2 --quiet -p hoimin-python research_control_tests -- --test-threads=2 passed 54 tests, zero failed. Before the additional three directive variants, the same command passed 51.
- GREEN: cargo test -j 2 --quiet -p hoimin-cli --test operator_function_contracts research_control_contracts -- --test-threads=2 passed 34 and failed zero. A later focused run of research_control_contracts::review_ passed all eight compiler and behavioral review cases.
- GREEN: cargo test -j 2 --quiet -p hoimin-python mapping_keys -- --test-threads=2 passed 35. cargo test -j 2 --quiet -p hoimin-python mangl -- --test-threads=2 passed 10.

Strict Clippy initially found a redundant boolean expression and the collector statement visitor grew to 101 lines. Using the token predicate directly and extracting the class context walk resolved both. Final cargo clippy -j 2 --quiet -p hoimin-python --all-targets -- -D warnings, cargo clippy -j 2 --quiet -p hoimin-cli --test operator_function_contracts -- -D warnings, cargo fmt --all --check and git diff --cached --check each exited 0.
The final post-extraction analyzer rerun passed all 16 review regressions, exit 0. Fix-round commit 33f1d9efdb03b523433379f6b3fcc17d670c91cd contains the eight listed task files.

## Independent review outcome

The first independent review found four Important defects: leading directive attachment, compiler delimiter depth, private-name dependencies and large numeric-key equality. Commit33f1d9efdb03b523433379f6b3fcc17d670c91cd corrected them. The scoped re-review approved all four, with no new Important or Critical findings. The reviewer verified the fix diff and recorded test evidence without repeating passing suites.

Two Minor followups remain assigned to final integration: explicit cleanup return/raise suppression examples and hiding directive-cache lifecycle within the statement-order module. The independent design assessment was7.5/10 before those followups. Final workspace verification also owns the separately documented existing Windows test failures. Completion of this task covers #1190/#1197/#1200, not the remaining six issues.
