# Task 3 collection processing and precision

Base commit 31b3427c785e06ff438d7c9970128984b0a76201. This task covers #1198 and #1199 in the managed research worktree.

## Changes

- Collection edits cover comprehension filter removal and inversion, lazy predicate polarity, set union/intersection, and sum/count. The emitter handles list, set, dictionary, generator, and async comprehensions at each condition's own source span.
- Precision edits cover round/floor/ceil, explicit digits, the eight Decimal rounding modes, finite literal quantum shifts, explicit context precision, and inner rounding beneath final arithmetic rounding.
- Private helpers share builtin provenance, guarded standard library aliases, lexical initialization, supplied-slot binding, delimiter admission, and evaluation of argument values.
- Stable IDs 112 through 122 join the default runtime catalog. The collection_processing and rounding selectors expand to their own members. Default-plan tests exclude each new ID and compare the remaining candidates.
- The user guide and CLI reference document the selectors and individual exclusion. The Lean corpus registry records the independent Rust and CPython contracts.

## Red and green receipts

- Initial RED with `cargo test -j2 -p hoimin-python processing_tests --quiet -- --test-threads=2` exited 101. All 21 expected positive cases saw no candidate. All 25 rejection cases passed. See task-3-red.log.
- Partial collection checkpoint with the same command yielded 35 passed and 11 failed. Ten failures were the unwired precision cases. The remaining failure identified a missing imported alias lookup needed to prove the reverse filter destination.
- Initial all-family GREEN with the same command passed 50 cases. The expanded suite passed 61 cases before the last resource and compatibility additions.
- The initial CPython 3.14.9 contract run, `cargo test -j2 -p hoimin-cli --test operator_function_contracts research_processing --quiet -- --test-threads=2`, passed 33 cases.
- Broad Rust regression, `cargo test -j2 -p hoimin-core -p hoimin-python --quiet -- --test-threads=2`, passed 8,905 cases with 15 existing ignores. See task-3-core-analyzer.log.
- Impacted CLI regression under CPython 3.14.9, `cargo test -j2 -p hoimin-cli --test operator_function_contracts --test default_operator_selection --test optional_keyword_delete --test valid_python_corpus --test cli_config --quiet -- --test-threads=2`, passed 712 cases. See task-3-cli.log.
- The first lint pass exposed the catalog crossing the function length limit. A second pass identified four new local findings in precision binding and integration. The default selection is now an explicit constant and all five findings are corrected. `cargo clippy -j2 -p hoimin-python --lib --quiet` exited 0. See task-3-clippy-followup.log.

## Implementation self-reviews

1. **Fault coverage and duplicate conditions.** Checked each edit against its semantic distinction. Suppressed literal bools, existing unary not, and single identity or membership inversions that duplicate earlier operators. Kept equality and chained comparisons because result inversion can differ from changing an overloaded comparison.
2. **Receiver and callable identity.** Reviewed builtin lookups, import aliases, class lookup, module escape and write hazards, lexical shadowing, deletion, and nonlocal writes. Added initialization event and scope checks for exact receivers. Prefer an existing destination alias. Separate lexical import shadow from changes to the builtin namespace.
3. **Evaluation order and compiler validity.** Checked lazy filter construction, one-shot sum consumption, set operand order, and reordered keywords in round and quantize. Sum generators reject walrus and yield inside the moved iterable. A single sparse delimiter index governs additions for collection, precision, and the preceding call operators.
4. **Decimal semantics.** Bound precision and mode to API slots and rejected duplicate, unknown, and expanded arguments. Quantum shifts use finite exponent and adjusted exponent checks for Decimal literals, including trailing zeros, scientific notation, integer values, Unicode decimal digits in the common repertoire, and tuple exponents. Guarded getcontext where setcontext can install a subclass. Preserved evaluation of the context and rounding options during inner quantization removal.
5. **Admission and work.** Checked selection at each actual candidate start, including multiline tuple exponents. Check supplied arity before binding fixed signatures. Stream Decimal text without allocating joined decoded pieces. Discovery polls cancellation and uses existing source encoding, ranking and candidate admission.
6. **Compatibility.** Moved the delimiter index and argument evaluation helper with their existing semantics. Verified that all 11 IDs append to the catalog and that selectors preserve member order. Earlier call operators passed the impacted regression.

## Test self-reviews

1. **Span precision.** Named cases assert particular originals and replacements. They distinguish bool, identity, membership, existing not, equality, chained expressions, and the status of literal, local, and preceding module receiver initializers.
2. **Runtime observability.** CPython traces distinguish predicate side effects, per-item negation, laziness, duplicate items, async dictionary filtering, and a raised predicate exception. A frozenset case checks the result kind and the operand evaluation trace.
3. **Numeric oracle independence.** Decimal tests check ties, negative values, sign, exponent, flags, traps, the restoration of scoped context, and lost intermediate rounding. An independent integer calculation checks 195 signed coefficients against CPython before and after a quantum edit.
4. **Compiler and source boundaries.** Compile original and changed files at the 200-delimiter ceiling, including a grouped callable import and a grouped Decimal receiver. Compile the walrus control and check its absent candidate. The bounded properties check expected source spans, ordering at low limits, parse validity and numeric scales.
5. **Operational contracts.** The default-plan suite confirms every new candidate is present and individually excludable. Exact selector cases enumerate each member. Zero-payload tests check candidate admission and verify that an unselected Decimal literal avoids payload charging.
6. **Saved verification.** Run weak and strong commands independently against persisted plans. Positive-only collection assertions and non-midpoint precision checks survive. Boundary and signed midpoint assertions kill the corresponding mutations.

## Design diagnostics

| Diagnostic | Result | Evidence |
| --- | --- | --- |
| Each module describable in one sentence | Pass | Collection edits, precision edits, exact initializers, Decimal context, and Decimal quantum each own a separate semantic question |
| Interfaces simpler than implementations | Pass | The collector supplies a source site, selection and a bounded candidate sink |
| Implementations change without callers | Pass | Replacement shapes, binding details and numeric bounds remain private |
| Interface comments describe abstraction | Pass | Comments state evaluation, initialization, source and cancellation guarantees |
| Reviews discuss design | Pass | Reviews corrected duplicate candidates, import identity, source depth and repeated scans |
| Important decisions hidden | Pass | AST traversal does not know signatures, Decimal lexical forms or replacement construction |
| Boundaries understandable from interfaces | Pass | Semantic emitters consume checked provenance and tokens |
| Design improvement time measured at 10–20% | Fail | The ledger records design refinements. Separate time tracking in a subsequent task would permit direct measurement |

Score 8.75/10.

## Scope of proof

Receiver detection depends on lexical identity and a provable initialization. Quantum shifts act on explicit finite Decimal literal constructor inputs. Binary floating-point Decimal input can set the FloatOperation signal during creation. Intermediate rounding covers arithmetic beneath a final rounding call. Task 5 owns workspace-wide Windows CLI library-test and lint baselines.


## Final verification

- `cargo test -j2 -p hoimin-core --test operator_names --test operator_selection --quiet -- --test-threads=2` passed 246 cases. See task-3-final-core.log.
- `cargo test -j2 -p hoimin-python processing_tests --quiet -- --test-threads=2` passed 75 cases. See task-3-final-focused.log.
- `cargo test -j2 -p hoimin-cli --test operator_function_contracts --test default_operator_selection research_ --quiet -- --test-threads=2` passed 121 cases under CPython 3.14.9. See task-3-final-cpython314.log.
- `cargo test -j2 -p hoimin-cli --test operator_function_contracts --test default_operator_selection research_processing --quiet -- --test-threads=2` passed 39 cases under CPython 3.15.1, including four saved verification scenarios. See task-3-cpython315.log.
- Analyzer library clippy, workspace formatting and staged whitespace checking each exited 0. The staged paths contain the Task 3 implementation, tests, catalog, ranking, CLI integration, and user docs.

Commit fc31f73f4dcc473b9f1c568adee467924348de76.

## Independent review round 1 fixes

1. Comprehension condition edits consult adjacent tokens and add the separators needed by the replacement. CPython cases cover an adjoining left bracket and following if, for, and async for clauses. A span assertion covers async token preservation because merging True and async can still compile.
2. The shared evaluated-arguments helper retains a bound receiver and the original generator expression as separate tuple values. Ruff's AST explicitly distinguishes borrowed call delimiters from an already-parenthesized generator. The shared sparse depth index admits a new pair of generator delimiters only when it fits. CPython controls cover comments, the adjacent depths of 197 and 198 around the element, and an already-grouped generator at the limit.
3. Exact Decimal result proof recognizes a resolved builtin round call with an exact Decimal operand and a proven integer digits expression. A runtime witness changes 7.2 to 7.4 when the intermediate builtin rounding is removed. Negative tests cover a shadowed round, missing digits, and None. Standalone quantum selection exercises the same proof.
4. The builtin name resolver supplies the complete checked destination spelling for sum, filter, and round. It uses the ordinary name or a stable imported alias or module member. A rebind invalidates the corresponding alias. This places the spelling decision behind one interface.
5. Decimal quantum detection validates list and tuple displays for both the three-item constructor input and its coefficient. Exact source edits replace only the exponent. Tests cover malformed or dynamic digits and execute a nested list display in CPython.
6. Separate CPython reference-loop checks exercise each filter edit on 781 generated lists. Additional runtime cases measure negative ndigits and NaN across digit omission. The operator inventory continues to describe legacy Lean corpus coverage.

### Round 1 verification

- RED with `cargo test -j2 -p hoimin-python processing_tests --quiet -- --test-threads=2` exited 101. It recorded 82 passes and 10 failures for known review cases. See task-3-fix1-red-analyzer.log.
- RED with `cargo test -j2 -p hoimin-cli --test operator_function_contracts research_processing --quiet -- --test-threads=2` exited 101 under CPython 3.14.9. It recorded 41 passes and nine failures. See task-3-fix1-red-cpython.log.
- The first retry showed that matching generator and call ranges misses a generator with redundant element parentheses. The AST's parenthesized flag gives the required grammar fact. A transient Windows LNK1104 reported by the unrelated reduce test binary was bypassed with the scoped library suite.
- Final `cargo test -j2 -p hoimin-python --lib GROUP --quiet -- --test-threads=2` exited 0 for processing_tests, call_contract_tests, and callable_imports, with 94, 70, and 119 passes respectively. See the task-3-fix1-final-GROUP.log files.
- Final CPython 3.14.9 CLI contract runs exited 0 for research_processing and research_call_contracts, with 51 and 20 passes. CPython 3.15.1 passed all 51 research_processing cases. See task-3-fix1-final-research_processing.log, task-3-fix1-final-research_call_contracts.log, and task-3-fix1-cpython315.log.
- `cargo clippy -j2 -p hoimin-python --lib --quiet`, `cargo fmt --all -- --check` and staged whitespace verification exited 0.

Round 1 fix commit 98a246fde4be92528ed05d9ab7cb61925a777cd7.

## Independent review outcome

The initial review found six Important groups: comprehension lexical boundaries, implicit generator grouping, Decimal provenance through builtin round, builtin destination aliases, Decimal sequence literal coverage, and independent generated collection/negative-digits/NaN witnesses. Commit98a246fd addressed all six. Scoped re-review approved the fixes with no new blocking defects. The review confirmed that the generated-list loop checks one semantic property per named invocation. The shared destination helper restored implementation independence; the independent design assessment is now7/8 demonstrated,8.75/10, with design-time fraction still unmeasured. The inventory/schema clarification is recorded in the domain design: deferred status denotes missing legacy Lean-model corpus coverage, while compiler/runtime fixtures provide independent executable validation.
