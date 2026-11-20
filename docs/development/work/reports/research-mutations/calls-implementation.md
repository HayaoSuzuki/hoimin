# Task 2 call contracts

Base commit 610e7ed40d4de455f8fcfa332451cd4f4a9ebe1c. This task covers #1192, #1195 and #1196 in the managed research worktree.

## Production changes

- `call_contracts.rs` owns escape eligibility, adjacent argument-role edits, result defaults and the sparse delimiter-depth guard.
- `callable_imports.rs` adds a family-local strict policy for direct member calls, package and submodule bindings, alias hazards and the builtin namespace. Legacy operators use the original resolver policy.
- `optional_keywords.rs` shares same-module signature identity and complete explicit argument binding. The optional-keyword operator retains its original source-order, signature and namespace gates.
- `function_body.rs` distinguishes ordinary synchronous returns and coroutine results from generator and async-generator calls.
- `rust.rs` supplies lexical type and value-use facts, selection, source encoding, candidate admission and cancellation.
- `config.rs`, `ranking.rs`, the operator registry and CLI documentation append stable IDs 109 to 111, add the `call_contracts` selector family and enable its members by default.
- The analyzer and CLI tests cover source form, CPython execution, saved verification, default inclusion and individual exclusion.

## Red and green receipts

1. Initial RED was `cargo test -j2 -p hoimin-python call_contract_tests --quiet -- --test-threads=2`. Exit 1. All 16 positive cases received no candidates. All 17 rejection cases passed.
2. Initial GREEN used the same command. Exit 0. All 33 cases passed.
3. Expanded RED used the same command. Exit 1. The 48 passing cases included URL annotations, alias hazards, grouped source spans and wide-prefix bounds. Four cases exposed discarded Boolean and conditional tails, a changed builtin namespace member and unsupported proven `sorted` keyword roles.
4. A subsequent run used the same command. Exit 1. The earlier four gaps passed. The three new positive recursion and forward-reference cases failed while a caller that could resolve to a builtin remained excluded. This drove the distinct new late-lookup gate.
5. A direct CPython 3.14.9 compiler probe verified all four wrapper templates at their threshold. All eight originals compiled. Mutants for an ordinary result and positional pair compiled at 198 outer parentheses and failed at 199. Empty-set results and mixed pairs compiled at 197 and failed at 198.
6. The first broad Rust run found two integration assumptions. The analyzer's operator-function tests counted unrelated default candidates and exposed 53 expectation failures. The CLI config test retained the old fixed array length. Both were corrected at their test boundaries. Earlier unit suites passed through their existing invariants.

Final verification passed.

- `cargo test -j2 -p hoimin-core -p hoimin-python --quiet -- --test-threads=2` exited 0 with 8,804 passing cases and 15 existing ignores. See `task-2-core-analyzer.log`.
- `cargo test -j2 -p hoimin-cli --test operator_function_contracts --test default_operator_selection --test optional_keyword_delete --test valid_python_corpus --test cli_config --quiet -- --test-threads=2` exited 0 with 660 passing cases, including CPython 3.14.9 execution, compiler boundaries and saved plans. See `task-2-cli.log`.
- After the last eligibility fixes, the focused analyzer command from step 1 exited 0 with 60 passing cases. See `task-2-final-focused.log`.
- `cargo test -j2 -p hoimin-cli --test operator_function_contracts research_call_contracts --quiet -- --test-threads=2` exited 0 with 20 passing cases under CPython 3.15.1. See `task-2-cpython315.log`.

## Implementation self-reviews

1. **Import provenance.** Checked that an unaliased dotted import binds the package, an aliased dotted import binds the submodule, and an explicit from-import can load the submodule. The package spelling requires evidence that its member was loaded. Escapes and writes invalidate aliases of one family without suppressing another family. The direct callable alias itself is included in the hazard scan.
2. **Evaluation order and source structure.** Checked CPython's ordering for the callee, adjacent arguments, keyword arguments and an awaited operand. Positional tuple reversal preserves the argument text and the separator. Relabeling leaves keyword expressions in place. The mixed pair takes its formal names from proven slots and preserves comments before and after the equals token. Escape removal retains supplied value expression order even when the primary is the final keyword.
3. **Declared results and lexical environment.** Cached the own-scope generator and annotation proof before collection. Reused lexical resolution for builtin reads and text parameters. Added a namespace check for explicit builtin mutations. Distinguish the discarded result of an expression statement from the tested operands of short-circuit evaluation. The direct await includes the complete original operand in its replacement span. Results used as assignment receivers remain consumed. Signed literal comparison suppresses equivalent adjacent values and distinguishes a floating-point zero sign.
4. **Binding parity and late lookup.** Preserved optional-keyword source order, synchronous signatures and callback gates while sharing argument binding. The new rules accept unique definitions in their own recursive bodies, and earlier function-body references when no builtin fallback can change the callable. A review of indirect module namespace writes added a conservative gate for proven keyword roles. Calls before the definition at module scope remain unproven.
5. **Compiler and work bounds.** Boundary probes exposed the need for token depth including redundant parentheses. The new sparse index records only intervals above the relevant nesting thresholds and queries them before replacement construction. Adjacent pairs use independent start selection. The collector reuses existing candidate and payload admission and propagates cancellation or sink failure.

## Test self-reviews

1. **Independent assertions.** The runtime witnesses derive expected traces, bindings, escaped output and return values from literal fixture values. The tests apply the actual plan edit and run a fresh CPython process.
2. **Scope and API families.** Named cases distinguish direct, aliased, dotted and package imports. Negative cases exercise relative imports, aliases that escape, rebinding, nonlocal writes, known bytes at text-only sites and unknown keyword roles.
3. **Failure and control flow.** The witnesses cover an option expression that raises before the primary is evaluated, an escaping call exception, an untaken short-circuit branch and cancellation during an awaited coroutine. The fresh-list case checks both identity and value.
4. **Source transport.** Apply original byte spans outside grouped comments and non-ASCII identifiers. The Latin-1 and UTF-8 BOM saved plans require a specific CPython exit status reached only after a successful import and differing escaped value. This distinguishes a semantic kill from a syntax or encoding error.
5. **Selection and behavioral outcomes.** The bounded property compares each retained threshold with the unlimited ordering. CLI fixtures check default inclusion and per-ID exclusion. Weak and strong checks run independently against saved plans for each escape API, argument transfer and a returned count. The compiler-limit cases run valid controls at each threshold and compile the retained mutants.

## Design diagnostics

| Diagnostic | Result | Evidence |
| --- | --- | --- |
| Module purpose explainable in one sentence | Pass | The call module owns argument and result call-contract edits |
| Interface simpler than implementation | Pass | One call emission entry point with an existing selection and sink protocol |
| Internal changes local to module | Pass | Depth admission, signature checks and replacement templates stay private |
| Interface comments describe guarantees | Pass | The module documents evaluation, cancellation, selection and sink rejection |
| Reviews address design | Pass | Reviews covered shared binding parity, source preservation and resource bounds |
| Important decisions hidden | Pass | Callable provenance and compiler thresholds are unavailable to candidate consumers |
| Boundaries understandable from interfaces | Pass | Collector supplies lexical facts and imports/signatures supply proof |
| Design-improvement time measured at 10–20% | Fail | Review changes are recorded but time was not separately metered |

Score 8.75/10. The failed diagnostic is the separate design-time measurement.

## Scope of proof

Callable and annotation proofs are conservative and local to the checked module. Typed returns depend on the declared contract. The result edit preserves the call or await and can change the lifetime of its original returned object. The legacy Lean registry records these operators as deferred because their independent compiler and runtime witnesses are in the Rust and CPython contract suites. Existing Windows CLI library-test and broad lint issues are tracked in Task 5.

## Commit receipt

Commit `a6898eb7bd8c2debda286975919c6be3a4cd63ae`.

- After the lint-driven refactor, `cargo clippy -j2 -p hoimin-python --lib --quiet` exited 0. See `task-2-clippy-final.log`.
- The final focused checks passed 60 analyzer call cases, 96 optional-keyword cases, and 20 CLI CPython 3.14.9 cases. See `task-2-commit-check.log`.
- The CLI default-selection case for each of the three IDs verified that a default plan contains the operator and that excluding its ID removes only its candidates.
- Staged whitespace verification passed. The worktree status immediately after commit showed only the two coordinator edits under the shared design and plan.

### Committed paths

- `crates/hoimin-cli/src/analyzer/payload_probes.rs`
- `crates/hoimin-cli/src/plan/ranking.rs`
- `crates/hoimin-cli/src/plan/ranking_tests.rs`
- `crates/hoimin-cli/tests/behavioral_research/call_contracts.rs`
- `crates/hoimin-cli/tests/cli_config.rs`
- `crates/hoimin-cli/tests/default_operator_selection.rs`
- `crates/hoimin-cli/tests/fixtures/valid-python-operators.json`
- `crates/hoimin-cli/tests/operator_function_contracts.rs`
- `crates/hoimin-core/src/config.rs`
- `crates/hoimin-core/tests/operator_names.rs`
- `crates/hoimin-core/tests/operator_selection.rs`
- `crates/hoimin-python/src/analyzer/rust.rs`
- `crates/hoimin-python/src/analyzer/rust/call_contract_tests.rs`
- `crates/hoimin-python/src/analyzer/rust/call_contracts.rs`
- `crates/hoimin-python/src/analyzer/rust/callable_imports.rs`
- `crates/hoimin-python/src/analyzer/rust/function_body.rs`
- `crates/hoimin-python/src/analyzer/rust/optional_keywords.rs`
- `crates/hoimin-python/src/analyzer/rust/stopped_collector_tests.rs`
- `crates/hoimin-python/src/analyzer/rust_tests.rs`
- `docs/user/guides/usage.md`
- `docs/user/reference/cli.md`

## Review round 1 fixes

Commit `734cce0735996f3bec04bfd25101a1af4ae25c4b` addresses the three Important findings in `task-2-review.md`.

- `callable_imports.rs` treats explicit `__globals__` access as a namespace hazard. `optional_keywords.rs` applies the same rule to same-module signatures, including when a different function exposes their shared module dictionary.
- `callable_imports.rs` recognizes documented regex flags and Decimal rounding values. A qualified read still needs a verified module binding. Explicit constant imports are safe to read, and writes to a module attribute or exposure of an unknown attribute continue to invalidate family provenance.
- `call_contracts.rs` binds only after finding a selected, eligible adjacent pair, then caches that call's binding for later pairs. `optional_keywords.rs` precomputes the counts of required formals and validates solely from supplied arguments. Excess positional values accepted by `*args` cannot occupy keyword-only slots.
- `call_contract_tests.rs` and the import and signature unit tests cover each correction. The filtered work fixture uses 512 defaulted formals and 96 two-argument calls. It observes zero binding invocations with all pairs excluded and one when only the last pair is selected. The direct binder test limits cancellation polls to four regardless of declared arity.

`cargo test -j2 -p hoimin-python --lib --quiet -- --test-threads=2 call_contract_tests optional_keyword callable_imports` produced 275 passes and 11 expected failures before the changes. Both filtered cases performed 96 binds. Two supplied arguments produced 6 binding polls with a narrow signature and 517 with a wide signature. See `task-2-fix1-red.log`.

The same command passed 286 cases after the three fixes and 288 after adding the two variadic-slot controls. `cargo fmt -p hoimin-python -- --check`, `cargo clippy -j2 -p hoimin-python --lib --quiet`, and the staged whitespace check also exited 0. See `task-2-fix1-green.log`, `task-2-fix1-final.log`, and `task-2-fix1-clippy-final.log`. The final commit contains four analyzer files. The two shared design and plan edits remained unstaged.

## Independent review outcome

The initial review found three Important issues: explicit callable-global mutation, overly broad rejection of immutable constants, and formal-width work before selection. Commit734cce07 fixed all three with RED/GREEN regressions. Scoped independent re-review approved each fix and found no new Critical, Important or Minor findings. The review confirmed zero binding invocations for96 excluded calls, one for the selected call, and per-call work bounded by supplied arguments. No out-of-scope followups remain for this task.
