# Final research mutation fixes

The analyzer now rejects set receivers whose proof depends on an exposed module dictionary and method edits for literals that fail both supported regex grammars. A root regex mode declared inline and in the external flags produces one covering edit. The planner defers replacement construction for losing edits, and signature preparation honors cancellation in both name-index traversals.

The reviewed base was `6df6dcb7`. Implementation started from `d4658222`, which includes the root design clarification. All 22 added operator IDs for the nine issues retain their default selection and individual exclusions. The [integration report](integration-implementation.md) covers the full inventory and the prior native Windows disposition.

## Defects and regression evidence

The initial focused run against the reviewed behavior executed 19 cases. Twelve failed and seven controls passed. The tested witnesses and their corrected behavior follow.

| Finding | Initial result | Corrected behavior and evidence |
| --- | --- | --- |
| Module set identity | Three stale module proofs emitted a swap after a write or escape through `marker.__globals__`. A direct `globals()` write also suppressed a genuine function-local set. | Initializer lookup applies module exposure to the binding's owner. Named analyzer cases cover both spellings, module and function reads, escapes, an ordinary set, and a proven local literal. The public runtime cases observe the custom receiver's `union` result. |
| Joint root mode | Both orders of the inline and external mode terms emitted zero candidates. | The candidate covers the two edited components in source order, including unchanged text between them. Real interpreter cases check flags, acceptance, a scoped override, aliases, duplicate external mode terms, comments, a concatenated literal and an independent reference consumer. A separate selection matrix checks the starting line and symbol. |
| Unknown regex flags | A malformed `'('` emitted one direct method edit and two compiled method edits. | Both paths try the cached ordinary and verbose interpretations. Both suppress the malformed literal. Direct and compiled verbose-sensitive positive controls and a dynamic pattern retain method edits. Named runtime cases check ordinary and verbose invocations independently. |
| Reference work | Thirty-two uses projected 131,968 bytes at K=1 and 263,936 bytes at K=3. A subsequent cross-token RED counted 33,927 decoded units while validating losing sites. | The byte counter now equals the sum of replacement lengths in the unlimited first K+1 candidates, including the overflow witness. The complete returned prefix equals the unlimited first K. Cross-token validation stays within 4,160 decoded units for the 32-use fixture. A later-visited, earlier-source candidate still wins. |
| Signature preprocessing | Imported-name and lexical visitors each processed all 66 statements after a stop threshold of four. | Both visitors receive the latched caller callback. On a one-shot stop the build returns cancellation after exactly four visited statements in the specified stage. Existing optional-keyword and call-contract tests exercise completion and binding. |
| Heap case independence | Each shape's test invocation ran all three selectors. | Twelve named shape/selector rows retain the same four inputs, K=3 and allocator mutex. Each row holds its own measurement window, including after a sibling failure. |
| Suite entry | The collector forwarded every suite call through a second name. | The order module owns the single collector `visit_suite` implementation. The stopped-sink matrix exercises that entry directly. |

## Design and plan review

The [root design](../../designs/2026-11-20-research-mutations.md), domain designs, and tracked implementation plan contain the original stage reviews. The following checks apply the final review rulings to this change.

| Pass | Design check | Implementation plan check |
| --- | --- | --- |
| 1 | Identify the owner of a name before using evidence about the module namespace. | Exercise both module reads and a local literal under the same reflection mechanism. |
| 2 | Keep the existing one-range saved edit and preserve text between coordinated changes. | Test both source orders, use-site projection, selection, flags and unaffected consumers. |
| 3 | Ordinary and verbose tokenization determine possible grammar under unknown external flags. | Reuse the scanner and cache on both call paths. Include a literal valid only in verbose mode. |
| 4 | Rank, identity and overflow belong to the common prefix. An unvalidated edit can still provide the first overflow witness. | Compare with unlimited discovery, count copied bytes, force out-of-order visits and measure repeated cross-token validation. |
| 5 | A cancelled name-index build must never publish partial facts. | Trip a one-shot callback during each traversal and count statements there, then run the entire analyzer suite. |

## Implementation self-review

1. **Namespace ownership.** Traced the receiver lookup through comprehensions, function locals and module fallback. The shared exposure predicate covers names and attributes. The module exposure fact affects initializers owned by the module. A direct globals call invalidates that module scope. Exact set literals in independent local bindings remain provable.
2. **Regex semantics and spelling.** Reviewed the inline positions, known external bit terms and the literal-use projection separately. The covering range uses source order even for reordered keywords. The renderer preserves the gap between components, the unrelated bits and nested scoped modes. No private source descriptor enters the saved format.
3. **Possible grammar and caching.** The direct and compiled paths share one possible-grammar check. Both interpretations use the existing limits and callback. Prepared values for shared literals are keyed by flags and consumer. Dynamic patterns keep the existing method eligibility rules.
4. **Prefix correctness and cost.** A validated strictly worse key cannot equal a retained identity. An equal key may be a duplicate and still gets checked when overflow is unknown. Once overflow exists, a whole group whose best possible key cannot win can skip validation. The cross-token RED exposed costly source re-decoding during this review and led to the earlier shared cutoff check.
5. **Cancellation and traversal.** Traced the callback from signature preparation through imported-name collection, lexical visits and finalization. The latch handles a transient true result. Other producers of the same name index use the shared constructor. The moved suite entry retains the stop checks before statement and adjacent-pair work.

## Test self-review

1. **Receiver oracle.** The custom object returns an observable value from `union`. The runtime checks require an empty plan as well as that value, and analyzer rows distinguish owner scopes and reflective spellings.
2. **Mode oracle.** Checked both root directions against `Pattern.flags` or `Match.re.flags` and fullwidth-digit acceptance. The referenced case also checks the original pattern at a second consumer. Covering-start tests distinguish the flags line, the later pattern line and the wrong symbol.
3. **Grammar oracle.** The malformed and verbose-sensitive fixtures use the same unknown flag parameter. Each runtime row runs one concrete flag choice. The ordinary-only and dynamic controls protect eligibility for patterns with runtime-dependent behavior.
4. **Admission oracle.** The bounded result compares every candidate field to an unlimited plan. The generated-byte expectation comes from that same plan's K+1 replacement lengths. The cross-token counter measures actual source decoding. The independent shared-prefix properties and payload-failure test exercise duplicates, zero limits, eviction and stopped sinks.
5. **Stopping and case isolation.** Separate counters identify the specific name-index stage that handled the stop. Test-only counters are thread-local. Each heap row owns one input and selector inside the measurement mutex. The runtime tests and existing bounded properties group setup, action and observations.

## Software design diagnostics

| Diagnostic | Assessment | Evidence |
| --- | --- | --- |
| Each module has one describable purpose | Pass | Initializer lookup proves binding identity, regex sites map semantic changes to source, and the prefix retains ranked candidates. |
| Interfaces are simpler than implementations | Pass | The sink asks for a range, operator and renderer. A possible-grammar query hides the two parses. |
| An implementation change can stay behind an interface | Pass | Rank comparison, the overflow witness and validation cutoff live inside the prefix. The regex module supplies an earliest source position. |
| Interface comments state guarantees | Pass | The index documents all-or-cancel publication, and the prefix documents when unvalidated sites can be skipped. |
| Design decisions participate in reviews | Pass | The second reference-cost probe changed the admission boundary after observing repeated decoding. |
| Important decisions are private | Pass | The shared namespace predicate, eligibility lookup and admission cutoff each own the relevant rule. Domain code does not compare operator names for rank. |
| Boundaries are understandable from their contracts | Pass | Named analyzer and runtime cases cover source selection, flags, ranking, binding identity and cancellation. The usage guide describes coordinated mode edits and possible grammar. |
| 10–20 percent of effort invested in design improvement | Unverified | The stage records show the design changes. No contemporaneous time split measures that fraction. Future work needs separate timing to substantiate it. |

Seven of eight diagnostic rows have evidence, for **8.75/10**. The remaining row requires measured time. The public guide examples are exercised in the interpreter-backed and parser tests. The full usage-site build checks the documentation in its published structure.

## Final verification

Commands ran in the managed research worktree with two Cargo build jobs, two Rust test threads, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and `CARGO_INCREMENTAL=0`. Runtime tests set `HOIMIN_OPERATOR_TEST_PYTHON` to the managed interpreter indicated below. The ignored work directory holds the command logs under `final-fixes-*` and the initial RED summary.

| Command | Result |
| --- | --- |
| `cargo test --locked -j 2 --quiet -p hoimin-python --lib final_review -- --test-threads=2` | Initial RED with 12 failures across all five findings. GREEN with all 23 focused cases after adding source-selection and later-winner coverage. |
| `cargo test --locked -j 2 --quiet -p hoimin-python --lib losing_references_skip -- --test-threads=2` | Additional cross-token RED, exit 101. The subsequent complete analyzer run includes its GREEN. |
| `cargo test --locked -j 2 --quiet -p hoimin-python --lib -- --test-threads=2` | Exit 0. 5,150 passed and 5 ignored, including shared name resolution, payload limits, equal-key duplicates, ranking, selection, stopped sinks, and the optional-keyword suite. |
| `cargo clippy --locked -j 2 --quiet --workspace --all-targets --all-features -- -D warnings` | Final exit 0. One earlier test assertion lint was corrected to show the actual candidate array on failure. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --test analyzer_line_selection --test operator_function_contracts --test default_operator_selection --test lean_candidate_ranking_oracle -- --test-threads=2` | Exit 0 on CPython 3.14.9. 9 selection, 251 runtime, 117 default/saved and 40 ranking cases passed. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --test operator_function_contracts --test default_operator_selection research -- --test-threads=2` | Exit 0 on CPython 3.15.1. 192 runtime cases and 23 default/saved cases passed. |
| `cargo test --locked -j 2 --quiet -p hoimin-cli --test regex_heap -- --test-threads=2 --nocapture` | Exit 0. All 12 shape/selector rows passed. |
| `cargo fmt --all -- --check`, `cargo run --locked -j 2 --quiet -p hoimin-cli --example generate_cli_reference -- --check`, `uv run --frozen --no-sync python -m tools.docs_site --output site` | Each exited 0. |
| `cargo build --locked -j 2 --quiet -p hoimin-cli --bin hoimin`, then managed CPython 3.14.9 running `.superpowers/sdd/research-mutations/task-5-replay.py` | Both exited 0. The rebuilt CLI was hoimin 0.15.0, SHA-256 `545acdf919ac7dd8073077cd8e1f3b44d94e5377372e80a4700d46149c6779eb`. |
| `git diff --check` | Exit 0. Git reported its existing line-ending warning on this host. |

The heap measurements below show candidate count and peak bytes as `count / bytes`. These measurements use identical per-shape source and K=3.

| Shape | Regex | Empty string | Integer neighbor |
| --- | --- | --- | --- |
| Typical | 3 / 8,732 | 2 / 6,926 | 2 / 6,926 |
| Long reference | 3 / 245,864 | 2 / 24,837 | 2 / 24,837 |
| Deep groups | 2 / 64,799 | 1 / 8,068 | 2 / 8,068 |
| Without import | 0 / 24,434 | 1 / 24,434 | 3 / 24,434 |

The existing hoimin 0.14.0 manifests replayed with schema 10 and ranking rule 4. The explicit plan retained its one candidate and killed it. The saved 85-operator default plan retained seven candidates, with five kills and two survivors. Manifest and source digests matched before and after execution. The mixed plan preserved the same five candidate IDs while strengthening its arbitrary test command's assertions changed five survivors to five kills. Progress reported `improving`.

## Remaining limits

The [integration report](integration-implementation.md) records 29 outstanding native Windows CLI-library failures. Ten reproduced individually on the pre-feature base. The other 19 matched the documented failure signatures and unchanged implementation paths. This scoped verification did not rerun that full failing suite. POSIX-only process behavior requires a POSIX host. The name proofs cover source-visible behavior, and regex eligibility uses the documented supported grammar and existing resource limits.
