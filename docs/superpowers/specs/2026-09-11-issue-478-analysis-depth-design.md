# Issue 478: Reject unsafe AST depth before recursive analysis

Issue: https://github.com/tokyogas-tech/hoimin/issues/478

## Cause and observable contract

A valid flat binary expression produces a deeply nested AST. `max_candidates` bounds retained candidates but does not bound AstFacts, name-resolution, annotation or other recursive visitors that run before or around candidate retention. The reported debug and release processes abort at different depths; increasing thread stack size is only a diagnostic control.

Reject an AST beyond a documented finite analysis-depth limit before any recursive analysis pass, with the source path, exceeded limit and depth reason. A rejected file must produce an explicit failed/incomplete analysis outcome, never a successful complete empty candidate list. Preserve cancellation as a different failure reason and preserve normal invalid-syntax behavior. Both candidate-discovery and run analysis paths must propagate the typed error accurately.

## Structural guard and cleanup

Use Ruff's complete AST child traversal rather than a handwritten list of expression variants. Prefer an explicit worklist of borrowed AnyNodeRef plus depth: a shallow SourceOrderVisitor captures immediate children by returning Skip, and the outer loop controls depth. This makes the guard's own stack independent of AST depth. Account for statement, expression, pattern, annotation/type-parameter and interpolated-string children, and verify the enter/leave protocol. A recursive bounded guard is only acceptable if a measured conservative cap bounds its call stack and all recursion paths are covered; do not silently skip a child category.

The depth limit is128: the module is depth1 and each child reported by Ruff source-order traversal, including auxiliary nodes, adds1. Explicit2MiB tests accept126 binary terms at depth128 with a real candidate and reject127 terms at129; nested list annotations likewise retain a real annotation candidate at128 and reject129. Normal runtime debug/release public tests supplement these bounded-thread checks. Do not claim that the number alone proves OS stack safety. Retain ordinary below-limit candidate generation and all selected operators, and avoid using source byte length, number of candidates or a raised RUST_MIN_STACK as a proxy for AST depth.

Parsing and disposal are separate obligations. A pinned-dependency probe confirmed that20,000 terms compile under CPython3.14 and parse under Ruff, but ordinary AST destruction aborts on an explicit2MiB thread. The2k/10k controls pass;40k/80k parse/drop also abort but are rejected by the tested CPython compiler and are not counted as valid-Python acceptance cases. Raw evidence: `/private/tmp/issue478-parser-drop.log`.

Use a dedicated bounded-stack disposal helper: consume the parsed AST, detach recursive Stmt/Expr/Pattern/interpolated-string child nodes through Ruff's existing mutable Transformer into an owned worklist, and drop only shallow shells. Review auxiliary recursive children as well as the named primary kinds. Cover rejection, cancellation after parsing and syntax-error ownership. The analyzer uses parse_unchecked_source to retain partial AST ownership and preserve existing invalid-syntax diagnostics; tested error shapes are a deep expression ending in plus and a deep valid statement followed by an invalid statement. Never use `mem::forget`, leak modules, or rely on a larger fixed stack.

Controller decision: accept the disposal helper because a valid20k input still aborts without it. The cost is an additional owned traversal whose child coverage must stay aligned with the pinned Ruff AST and be reviewed when that dependency changes, plus a heap worklist during disposal. Actual default-runtime/debug/release and repeated explicit2MiB disposal tests are required; an abstract depth proof cannot establish this coverage.

## Error and lifecycle propagation

`analyze_source_cancellable` currently returns only AnalysisCancelled, and its two callers map all errors to cancellation. Introduce an explicit error distinction so depth rejection retains target path/cause through public plan/run diagnostics. Preserve current cancellation/deadline behavior and cleanup of candidate stores/workspaces. Keep the noncancellable test helper honest if its return type or probe error handling changes; do not hide rejection by manufacturing a success output.

## Verification and evidence scope

Capture real subprocess RED on old code, bounded in time and resources, with core dumps disabled: debug2k and release10k terms or the smallest reproducible equivalents. CPython3.14 compiles the same fixtures under its normal settings. After the change, both debug/release CLI plan paths must return a normal nonzero status and path+depth diagnostic, not SIGABRT; cover actual run as well without claiming baseline ordering changes unrelated to this issue. `--max-candidates1` must not bypass depth protection.

Check an exactly-at-limit accepted tree and first-over-limit rejected tree against the documented counting; retain at least one real candidate below the limit so a blanket suppression cannot pass. Cover nested annotations/type parameters, f-string expressions, patterns and ordinary flat breadth where grammar permits. Exercise facts/name resolution/type annotation passes on accepted boundaries, cancellation, rejected-module disposal, and repeated rejections in one process to check cleanup rather than only process-exit reclamation. Use both debug and release and the normal runtime worker-stack setting; any explicit stack-size probe supplements those public runs.

Rust tests and actual subprocess observations establish correspondence. A small Lean depth/worklist model would only prove the abstract guard, not Ruff traversal coverage, parser/drop safety or native stack bounds. Use Lean only if a concrete model-to-implementation claim adds useful evidence; no formal claim may substitute for actual default-stack debug/release tests.

Run focused analyzer/public regressions, full workspace all-features, fmt and all-target/all-feature Clippy. Preserve byte spans, candidate IDs and normal source ordering. No runtime Python helper, no operator-specific exclusion, and no broad unrelated name-resolution refactor.

## Design self-review

1. Traced analysis call order and error mapping: candidate count is too late to bound recursive passes, and both callers must distinguish depth rejection from cancellation. Error handling must not report a complete empty plan.
2. Checked pinned Ruff source_order: enter_node Skip is followed by leave_node; public AnyNodeRef child visitation supports a shallow collector. Included non-expression AST children and separate cleanup/drop risk rather than treating the first guard as sufficient.
3. Checked evidence limits: issue requires debug/release subprocess survival, path/cause, facts/name/annotation/cleanup coverage and normal below-limit preservation. The128 cap is supported by measured boundary validation; no universal stack-safety or upstream-parser proof is claimed.

## Test harness and evidence boundaries

Public subprocesses use repository Python on each platform, temporary output files and a deadline; this avoids relying on Unix true or unread pipe buffers. Run reports remain incomplete on depth failure, and the existing baseline-first run order is preserved. A warm isolated child on a2MiB thread repeats25k-term rejection20times and checks live Rust allocations against its baseline plus1KiB using the existing allocator tracker. Synthetic25k nested format-spec ASTs cover disposal through both FString and TString; these are ownership tests, not claims that Python grammar accepts that source shape. Ruff-valid structural fixtures and actual CPython-valid source cases are reported separately.
