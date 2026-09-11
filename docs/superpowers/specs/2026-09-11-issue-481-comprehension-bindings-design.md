# Issue 481: Resolve comprehension assignment targets in their containing scope

Issue: https://github.com/tokyogas-tech/hoimin/issues/481
Normative rule: [PEP 572 scope of the target](https://peps.python.org/pep-0572/#scope-of-the-target), checked2026-09-11.

## Cause and contract

The Rust name-resolution builder treats a named expression target like an ordinary comprehension iteration target. A walrus target inside list/set/dict comprehensions or generators instead binds in the nearest containing non-comprehension scope. Nested comprehensions are skipped; a lambda or ordinary function is a boundary. The containing scope's global/nonlocal declarations apply. Preserve the comprehension-local nature of for targets and the existing first-iterable evaluation in the outer scope.

A mutation requires both original and replacement names to resolve to the intended builtin. Source any or destination all rebound through a comprehension must therefore suppress collection_any_all at the affected call, just as ordinary assignment does. Preserve real candidates for unrelated names and scopes; do not suppress all comprehension files or all builtin mutations.

## Routing and execution facts

Introduce one explicit named-target routing operation and reuse the existing binding/directive machinery. Do not duplicate global/nonlocal behavior or move the RHS into the destination scope. RHS occurrences belong to their evaluation scope; the assignment's binding effect occurs after RHS evaluation, using the expression end offset where ordered facts apply. Ordinary named expressions retain their existing scope.

Module/class ordered facts must reflect zero iterations and generator laziness. A potentially executed write must not become a definite Bind merely because conditional_depth was zero outside the comprehension. Function locals remain static across the whole function even when the comprehension is empty or a generator is never resumed. Preserve possible_bindings, loop back-edge summaries, temporary scope restoration and directive handling when recording the outer write.

Delayed generator writes must remain conservative after creation and later reset/import/delete before next(). Inspection of the existing BindingEffect lattice shows no DefinitelyBuiltin reset: ordinary assignments/imports resolve Shadowed, deletion resolves Unknown, and neither recovers the builtin. Rerouted MaybeBind therefore cannot be erased into a definite builtin by these operations. Retain this existing conservative contract and add concrete delayed-generator regression evidence; no new generator lifetime state is needed unless a reachable counterexample demonstrates recovery. Keep uncertainty limited to the affected name/scope and do not claim general interprocedural execution analysis.

## Verification

Capture actual public plan RED and CPython3.14 runtime binding observations before production edits. Cover module/function/nested list/set/dict/generator, global/nonlocal, both source and destination rebound, lambda boundary, empty and unexecuted generator, generator resumed after creation, and ordinary iteration-target nonleakage. Positive controls must generate actual candidate IDs/spans and retain unrelated scopes/names. The reported false kill should become zero relevant candidates in an actual run with a passing Python baseline. Use repository Python with portable subprocess invocation, bounded child execution and source-preservation assertions.

Read existing BindingFlow Model/Cases/Proofs and Rust corpus consumer. The existing model resolves supplied frames but does not derive the destination of NamedExpr from an AST. A useful bounded extension models skipping comprehension frames, stopping at a function/module boundary and directive routing, with executable expected candidate outcomes plus corresponding Python source cases. Prove the routing invariants and a fixed witness against the old current-scope rule if this can be done without speculative language modeling. Generated expectations must come from the model, not be copied from Rust. Clearly distinguish modeled routing/flow, source-to-model correspondence and CPython execution observations. If the existing formal structure cannot support a meaningful bounded claim, document the reason and use actual Rust/CPython tests; no decorative proof or manually shadowed frame may be presented as proof of AST routing.

All Lean work uses the repository resource guard serially,30s wall/2048MiB RSS/250ms sampling, -j1 and -DElab.async=false. No aggregate unbounded build, limit increase, sorry/admit or manual corpus edits. Record peaks, failures, sensitivity and freshness. Existing downstream oracle tests remain required.

Run focused analyzer/public/oracle tests, fullworkspaceallfeatures, fmt and alltarget/allfeatureClippy. No runtime Python loader, broad parser changes or dependency on separate issue PRs. This branch starts4adf809, so #478 depth guard and #486 future type-parameter scopes are not assumed.

## Design self-review

1. Compared PEP572 with the actual Expr::Named/currentScope and comprehension entry code. Separated iteration targets, nested comprehension destination and lambda boundaries; preserved global/nonlocal ownership.
2. Traced record_binding, conditional_depth, possible_bindings and ordered offsets. Flagged zero/lazy execution and delayed generator writes as independent from lexical destination; require concrete regression evidence rather than a routing-only patch.
3. Inspected existing Lean scope/flow model and consumer boundary. Formal claims must derive routing and expected outcomes; Rust/CPython tests supply AST/runtime correspondence and positive controls. Preserve resource limits and no unrelated language refactor.

## Bounded oracle correspondence

The dedicated ComprehensionBindingModel imports the existing BindingFlow model. Its innermost-first frame path determines the containing frame, global/nonlocal destination, possible module write or static function declaration, then candidate eligibility. The generated corpus has11 finite source scenarios and uses the existing public CLI test consumer with CPython assertions. The owner ID is model metadata, not an observed Rust scope ID. Rust nonlocal writes reuse the enclosing static binding suppression rather than materializing the model update; candidate eligibility is the strict implementation observation, while CPython checks the actual assigned value. The reused Frame directive abstraction is limited to the declared-name scenarios in this corpus. It does not prove a full Python symbol table or arbitrary per-name directive combinations.

Sensitivity includes deliberately broken current-scope binding, ignored directives, crossing the function boundary and omitting a static declaration. Source validity, unique frame identities and source-to-frame mapping are correspondence premises checked for the finite cases; routing theorems alone do not establish them.
