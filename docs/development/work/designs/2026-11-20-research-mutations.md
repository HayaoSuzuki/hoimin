# Research mutation design

This design is for maintainers implementing issues #1190, #1191, #1192, and #1195 through #1200. The user explicitly confirmed all nine. The objective is to expose missing behavioral specifications, including Unicode boundaries, exceptional cleanup, call contracts, dispatch, collection processing, precision, and sequencing.

## Architecture

Extend the Rust AST analyzer with deep modules that own one semantic domain. Each module discovers supported sites, proves the facts needed for its rewrite, and emits immutable source-span replacements through the existing candidate sink. The collector owns selection, symbol identity, source encoding, cancellation and candidate admission. Do not introduce a plugin framework or a Python analyzer.

Domain specifications and five-pass investigations are in ../reports/research-mutations/control-design.md, regex-design.md, and calls-design.md. These are part of this design. Module internal signatures are implementer decisions; candidate identity and behavioral contracts are shared interfaces.

| Issue | Deliverable | Required observable distinction |
| --- | --- | --- |
| #1190 | finally_unwrap | Normal cleanup retained, abrupt-path guarantee removed |
| #1191 | Regex method, anchors and ASCII/Unicode boundaries | Full matching/search, newline positions, digit/word/space/complement/boundary/case-fold acceptance |
| #1192 | stdlib_escape_remove | Input metacharacters reach the consuming parser without escaping |
| #1195 | call_argument_swap | Callee sees reordered values with original caller evaluation order |
| #1196 | call_result_default | Call/await executes once, result replaced with a compatible default |
| #1197 | Match arms, guards and OR alternatives | Dedicated dispatch, rejection and fallback |
| #1198 | Comprehension/filter, set and aggregation | Membership and element processing, not only arithmetic changes |
| #1199 | Rounding and Decimal precision | Tie direction, quantization, intermediate rounding and context precision |
| #1200 | statement_order_swap | Adjacent operations retain their code but run in the other order |

## Global constraints

- Implement all nine issues; examples are not a cap on semantic scope.
- Keep source analysis in Rust; no GUI, embedded LLM, Python collector or pytest plugin.
- Preserve arbitrary user verification commands and existing plan/verify/report contracts.
- Append stable operator IDs, register adopted runtime operators in defaults and appropriate selectors, and document exclusion by ID.
- Reuse existing selection, source encoding, cancellation and resource-admission paths; discovery must stop when the sink stops.
- Statically unresolved identity is not proof of a standard-library callable. Reject malformed syntax, unsafe binding assumptions and unsupported dynamic expansions explicitly.
- Mutate one semantic site per candidate; preserve unrelated source and evaluation effects unless the chosen fault model changes them.
- Use one test invocation for one case, Arrange/Act/Assert groups, named rstest cases and bounded proptest strategies. Test runtime behavior with actual CPython as well as candidate spans and parsing.
- Record at least five substantive self-review passes at each stage: design, implementation plan, implementation and tests. External reviews supplement these.
- Preserve the original dirty checkout. Work only in the managed research-mutations worktree. Do not push, merge or close issues as part of implementation.

## Design alternatives

Inline additions to the already large collector would spread regex/provenance/control-flow knowledge into traversal. A generic rewriting framework would add a public extension surface without a current consumer. Selected approach: private semantic modules behind existing emit callbacks, with only genuinely shared binding/source-edit utilities reused.

## Design self-review

1. Scope: mapped each of the nine issues to a behavioral deliverable. Corrected the temptation to treat regex as digit-only or calls as syntax-only. Explicitly include complements, scoped flags, compiled receivers and Decimal context.
2. Semantic preservation: rejected suite dedenting for finally; use sequential if True headers to preserve suites and string literals. Argument swaps preserve expression evaluation order, not merely source order. Result defaults preserve awaited execution.
3. Identity and safety: imported aliases need rebinding/escape analysis; dotted urllib.parse cannot be treated as a one-level module alias. Unknown flags, signatures or bindings cannot justify specialized rewrites.
4. Complexity and resources: separated domain knowledge from traversal, kept immutable edit sinks, linear adjacent pairs for argument/statement ordering, and bounded regex scanning. No cross-product of mutations or full candidate materialization before limits.
5. Verification and operations: required executable witnesses, Latin-1/BOM/CRLF, selected and excluded sites, serialization and default selection. Recorded Unicode-versus-ASCII intent as specification-dependent; a survivor is not proof of a defect.

## Software design philosophy evaluation

Score before implementation: 8.75/10 (7 of 8 diagnostics). This is an architectural assessment, not a claim that implementation has passed review.

| Diagnostic | Result | Evidence or action |
| --- | --- | --- |
| Each module describable in one sentence | Pass | Each owns a semantic mutation domain |
| Interfaces simpler than implementation | Pass | Emit replacements; hide parsing/provenance/reachability |
| Implementations change without callers | Pass | Existing collector candidate protocol remains stable |
| Interface comments describe abstraction | Pass in design | Carry invariants into actual module interface comments |
| Reviews discuss design | Pass | Five passes and final independent complexity review |
| Important decisions hidden | Pass | Regex mode stack, callable identity and structural eligibility stay internal |
| Boundaries understandable without implementation | Pass | Domain table and module purpose comments |
| 10-20 percent invested in design improvement | Not yet verified | Record actual implementation refactoring and resource measurements; reassess at final review |

## Acceptance

Each family needs positive and rejection cases, stable selection/default behavior, bounded discovery, parse-valid mutants, and runtime examples demonstrating weak versus specification-based strong tests. Final verification covers workspace tests, contracts, clippy, formatting and Python integration. Exact commands and results belong in the implementation ledger, not predictions in this design.

## Builder-check rulings

These refinements settle ambiguities in the domain reports.

- Positional argument swaps are allowed for unresolved/dynamic callees and variadic signatures: arity and original value-evaluation order stay unchanged. Reject syntactic expansions and duplicate explicit keyword labels, and reject invalid binding when a known signature proves it invalid. The requirement to prove explicit parameter roles applies to keyword and mixed-boundary swaps, not positional pairs. A declared *args signature alone does not forbid exchanging explicit positional values.
- aggregate_sum_count rejects a source iterable expression containing a named expression (walrus), yield, or another construct forbidden in a comprehension iterable. This is a syntax-validity constraint, not a restriction to a few example input kinds. Add a CPython compile regression for sum((items := source())).
- Regex literal references are mutated at their uses with a lazy materialization path. A site with the same source start and operator emits at most max_candidates + 1 distinct canonical variants, sufficient for the existing prefix overflow witness. Deduplicate edit descriptors before materializing full referenced literals. Do not stop discovery globally merely because CandidatePrefix is full: later candidates can rank before its worst entry. Literal-token edits stay local where possible. Add a long-reference/low-budget property proving retained results equal the unlimited prefix and generated bytes do not scale with all atoms. Sink failure and cancellation stop immediately.
