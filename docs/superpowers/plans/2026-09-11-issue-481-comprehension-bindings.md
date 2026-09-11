# Issue 481 implementation plan

Spec: ../specs/2026-09-11-issue-481-comprehension-bindings-design.md

Independent worktree from4adf809. Controller owns docs/superpowers and docs/knowledge; one architecture implementer owns Rust, applicable current README/development and formal source/corpus/consumer changes. User authorized implementation/commit/push/PR and3selfreviews perstage. No merge, no cargo-mutants, no worker delegation. Every Cargo uses CARGO_INCREMENTAL=0 and CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target. No concurrentCargo. Repository .venv symlink is CPython3.14.7, untracked and never committed. Logs in /private/tmp/issue481-*.

## Plan self-review

1. Sequence: inspect lexical and execution-fact boundaries, capture public RED and Python observations, establish bounded formal expectations if meaningful, then production routing and semantic controls. A current-scope-to-parent replacement alone cannot satisfy lazy/empty behavior.
2. Coverage: source/destination pairs, all comprehension forms, nested scopes/directives, lambda and iteration-target controls, zero/lazy execution and resumed generator. Require actual candidate preservation and runtime false-kill regression, not only private resolver assertions.
3. Ownership/resources: one worker, shared target, bounded subprocesses and guarded serial Lean. Controller handles docs/OKF/rulings so worker edits cannot conflict. Green fullsuite is not rerun without a new concern; task/final reviewers inspect evidence.

## OKF self-review

1. Read analyzer concept, operator contract, scope/flow model and real NameResolutionBuilder; existing reports remain historical and do not certify named-target routing.
2. Add issue-specific design source and precise comprehension-binding claim to analyzer, preserving older revisions and source-footnotes. Full design index gets exact title and161 entries.
3. Validate16page metadata/source hashes, pairs, links, reachability and actual/displayed design count before implementation; repeat after evidence-driven design changes. Model proof, Rust correspondence and CPython observations remain separate.

## Task 1: Correct comprehension named-expression binding and verify runtime correspondence

Read spec above and /private/tmp/issue481-preflight-notes.md and issue481 in /private/tmp/hoimin-bug-issues.json. Worktree /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-481, base4adf809, branch fix/issue-481-comprehension-bindings. Own Rust analyzer and relevant tests, current README/development only if contract needs updating, formal/HoiminOracle scoped model/proof/generator/corpus/consumer changes. Do not edit docs/superpowers or docs/knowledge; send design findings to controller before material expansion. No subagents/push/PR.

- [x] Reproduce publicplan wrong source/destination builtin candidate before production changes and execute matching CPython source to establish actual bindings. Use actual controlledbinary, repositoryPython and boundedportable subprocesses. Preserve originalsource and normal positive control.
- [x] Route named targets through nearest non-comprehension containing scope, stopping at lambda/function; reuse globals/nonlocals and existing binding/fact machinery. Keep RHS evaluation scope/order and ordinary for-targetlocality. Review nestedcomp, directives declaredincontainingfunction, outerloopbackedge and nameoccurrence handling.
- [x] Preserve conservative zero/lazy writes, static functionlocals and delayed generator mutation possibilities. Inspect reset/import/delete then next() counterexample; raise smallest sound design refinement if current ordered facts cannot represent it. Avoid blanket file/operator suppression or pretending unexecuted writes are definite.
- [x] Meaningful Lean scope/flow extension if bounded: inspect existing BindingFlowModel/Cases/Proofs, generator and consumer. Model named destination derivation explicitly (manual shadowedFrame is not ASTroutingproof), prove skipping comp / stoppingboundary / directive properties and oldrule brokenwitness, generateexpectedpublicoutcomes and sensitivity. Include source-to-model mapping and CPython checks. If impossible within this finite scope, send concrete limitation for controller decision before omitting. Read lean-test-oracle and lean-formal-audit instructions when used.
- [x] All Lean commands serial guarded: formal/HoiminOracle/tools/lean_resource_guard.py --timeout-seconds30 --rss-limit-mib2048 --sample-ms250 --stats /private/tmp/issue481-NAME.json -- COMMAND; syntax uses separate flag/value args. Retain package -j1 -DElab.async=false. Use worktree-local copied .lake cache (never symlink maincache); no aggregate unbounded build/limitincrease/admit/sorry/manualcorpus. Sandbox ps monitor may need require_escalated; infrastructureerror is not semantic evidence. Reference prior issue460 audit for successful workflow if useful, without editing that branch.
- [x] Tests module/function/nested/list/set/dict/generator/global/nonlocal/lambda and both source/dest. Empty/lazy cases reflect unknown versus staticlocal, ordinaryiterationtargets do not leak, unrelatednames/scopes still generate real candidates. Actualrun falsekill becomes zero relevantcandidate and passingbaseline. Publicplan candidates agree with CPython runtime checks. Extend builtinpairs as meaningful to sharedresolver, no broad unrelated annotation/operator refactor.
- [x] Focused Rust/public/formalcorrespondence then fullworkspaceallfeatures, fmtcheck, alltarget/allfeatureClippy-Dwarnings; record commands/results and diagnosed failures. Existingformaloracles remain required. Check IDEMCP if availablehoimin only; lastquery reportsprojectnotopen, don't inspect unrelatedprojects.
- [x] Three concrete implementation and three test selfreviews; report model/correspondence/CPythonlimits, exactlogs and any controller decisions. Commitonlyownedfiles and write .superpowers/sdd/2026-09-11-issue-481-comprehension-bindings/task-1-report.md. Stop after report/commit for taskreview; no push/PR or repeatedgreenfullsuite.

## Scope clarification from implementation inspection

The existing BindingEffect lattice does not recover DefinitelyBuiltin after assignment/import/delete: the first two are Shadowed, deletion is Unknown. A delayed generator write recorded as MaybeBind cannot be reset into a false builtin fact by these operations. Controller accepted reuse of this conservative contract with concrete resumed-generator tests; no additional generator lifetime machinery or test waiver. Three refinement reviews:1 traced all ordered effects and absence of builtin recovery;2 retained RHS evaluation and loopbackedge/directive obligations;3 updated spec hashes and OKF checks without claiming interprocedural modeling.

Formal integration choice: implement a separate bounded named-binding routing module/corpus/consumer importing BindingFlowModel, retaining the existing BindingFlow fixed schema and corpus unchanged. Expectations derive from destination selection, maybe/static write and resolution; include old-current-scope sensitivity and public/CPython correspondence. This follows the original bounded extension scope and introduces no compatibility waiver.

Controller correspondence selfreviews:1 read actual model destination/write/resolve pipeline and11sourcecases rather than infer ASTrouting from manualshadowing;2 required owner-ID and nonlocal-update abstraction limits, plus explicit broken boundary/directive/static variants;3 verified unique frame IDs in finitecases, publiccandidate observation/CPythonvalue separation, rehashedspec and passedOKF. These refinements clarify the proved/observed scope; no deferred correctness finding or waived test.

## Formal audit record

`HoiminOracle/ComprehensionBindingModel.lean` derives a named-expression destination from an innermost-first scope path, applies a possible module write or static function declaration, and uses the existing `BindingFlow.resolveCandidate` to decide builtin-pair eligibility. `ComprehensionBindingAuditMain.lean` generates `corpus/comprehension-bindings.jsonl`; `crates/hoimin-cli/tests/comprehension_named_bindings.rs` executes each source using repository CPython and the actual `hoimin plan` binary.

## Claim and correspondence boundary

The modeled rule skips comprehension frames, stops at an ordinary function (including a lambda), and honors the containing scope's global/nonlocal directive. The kernel-checked theorems state each routing rule for arbitrary path tails and retain a fixed witness against the old current-scope write. Eleven explicit scenarios have at most four frames and two tracked builtin endpoints. There is no exhaustive language or interprocedural execution model.

| Premise / observation | Lean representation | Rust / CPython correspondence | Mode |
| --- | --- | --- | --- |
| Nested comprehensions | `comp` frames followed by module | AST comprehension scopes; actual nested source | strict |
| Function/lambda boundary | `.function` frame | `Expr::Lambda` and function definitions introduce function scope | strict |
| Global directive | containing frame `.global`, module owner | `record_binding` writes module uncertainty; CPython checks actual changed value | strict |
| Nonlocal directive | enclosing function has preexisting static binding | Rust needs no additional local declaration: `record_binding(nonlocal)` returns; enclosing static binding suppresses eligibility. CPython checks updated value | strict |
| Empty/lazy module write | `.absent.meet .shadowed = .unknown` | `MaybeBind`, though CPython can observe builtin before execution | strict |
| Function declaration before execution | whole-function `.shadowed` | static locals and CPython `UnboundLocalError` | strict |
| Iteration target | write to current frame explicitly | ordinary `for` targets stay local | strict |
| Later builtin candidate | `allows` after dropping exited frames | complete relevant candidate count; IDs, original spans, operator, replacement checked | strict |
| Destination owner ID | generated `owner` | model metadata, not an observed Rust owner ID | model-only |

A corpus row's `strict` mode concerns public candidate eligibility. `owner` is explanatory metadata, not a strict implementation observation. Frame directives are shared by the two names in the reused model; these finite sources constrain the untouched endpoint so that this abstraction preserves the candidate decision. This does not prove arbitrary per-name directive resolution. Source paths are supplied alongside Python source, not derived by a verified Python parser. CPython assertions independently check executed bindings; they do not claim that an empty or unresumed comprehension executed its write.

Class-body comprehension walrus syntax is excluded because CPython rejects it. Arbitrary generator scheduling, alias/call effects, exceptions during RHS execution, and value/type inference are outside this model. The existing builtin resolver has no event that restores a previously uncertain name to definitely-builtin: assignment/import yields shadowed, deletion yields unknown, and `MaybeBind` preserves shadowed or produces unknown. Consequently reset/import/delete followed by generator resumption needs no new lifetime mechanism for this fix. Separate public regressions execute all three reset forms and assert the resumed write.

## Sensitivity and execution

The executable checks explicit broken variants: old current-scope writes; ignored global/nonlocal directives; crossing a function boundary; and omitted static declarations for unexecuted comprehension bodies. Global, lambda and static variants also change candidate eligibility. Nonlocal owner sensitivity is model-only because the preexisting outer local already blocks the candidate in Rust. Transactionality and idempotency do not apply to this lexical model.

From `formal/HoiminOracle`, prefix each command with:

```sh
python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/issue481-check.json --
```

Run serially with the package's `-j1 -DElab.async=false` settings:

```sh
lake build HoiminOracle.ComprehensionBindingModel
lake build generate_comprehension_bindings
lake exe generate_comprehension_bindings -- --output corpus/comprehension-bindings.jsonl
lake exe generate_comprehension_bindings -- --check corpus/comprehension-bindings.jsonl
lake exe generate_comprehension_bindings -- --sensitivity
```

CI builds the module separately and checks corpus freshness and sensitivity under the existing guard. Never edit the generated corpus manually. Use a worktree-local cache copy, not a symlink to another worktree's cache. A guard `monitor_error` is an infrastructure error, not a failed semantic claim.

The issue-481 run used CPython 3.14.7. Clean formal RED proved the historic witness false under the deliberately old write, then the fixed model passed. Public RED emitted invalid source and destination candidates; an actual mutation run reported one false kill. Fixed execution reported baseline `Exit: 0`, no mutants and zero kills. The public test suite also checks set/dict comprehensions, nested scope boundaries, outer loop back edges, RHS evaluation scope, source preservation, and list/tuple, set/frozenset, min/max and sorted/reversed pairs.

## Final implementation and verification record

Implemented on `fix/issue-481-comprehension-bindings` from base `4adf809`. Owned commit: `0fcb3bf` (`fix: route comprehension named bindings to their containing scope`). No push or PR. Controller owns design/plan/OKF; this task owns the Rust change, tests, new bounded Lean model/generator/corpus, and CI registration.

## Behavior and design decision

`NameResolutionBuilder::record_named_target` skips consecutive comprehension scopes and reuses `record_target` / `record_binding` in the containing scope. It temporarily increments conditional depth for routed targets, so zero-iteration/lazy module writes become `MaybeBind` while function locals remain static. Globals/nonlocals, possible bindings and outer loop back edges retain their existing machinery. `Expr::Named` visits its RHS first in the original evaluation scope and records its target at expression end. Ordinary iteration targets remain comprehension-local.

Controller accepted no new lifetime mechanism: this version has no builtin-recovery effect. Assignment/import is Bind, delete is Unknown, and MaybeBind cannot make an uncertain/shadowed name definitely-builtin. Actual reset/import/delete then next(generator) cases confirm suppression and the CPython changed value.

## Evidence

All Cargo invocations used `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`, serially. Repository `.venv` is CPython 3.14.7, uncommitted.

- Original public RED: `cargo test -p hoimin-cli --test comprehension_named_bindings -- --nocapture`, `/private/tmp/issue481-red-both.log`: 5 failing behaviors, 2 passing controls. CPython source assertions passed before actual binary plan comparisons failed for source and destination, empty/lazy, static-local, and containing-global cases.
- Expanded historic sensitivity: restored original Rust temporarily, `cargo test -p hoimin-cli --test comprehension_named_bindings`, `/private/tmp/issue481-red-expanded.log`: 7 failed /2 passed. Actual run regression observed `summary.counts.killed = 1`; passing baseline and erroneous mutant were real child executions. Restored the fix afterward.
- Exact facts RED: `cargo test -p hoimin-cli --lib comprehension_named_binding_execution_facts_and_rhs_order`, `/private/tmp/issue481-red-facts.log`: expected unknown, original returned definitely-builtin. Fixed test passed in `/private/tmp/issue481-green-all-focused.log`.
- Focused final GREEN: `cargo test -p hoimin-cli --test comprehension_named_bindings --test lean_binding_flow_oracle`, `/private/tmp/issue481-public-oracle-green.log`: 11 new public tests and 4 existing oracle tests pass. Tests execute module list/set/dict/nested/generator, both endpoints, lambda/function boundary, static locals, directives, empty/lazy, delayed resets, RHS and loop controls. Other list/tuple, set/frozenset, min/max, sorted/reversed pairs have source/destination suppression and real positive candidates. Positive candidates validate IDs, spans, replacements and operator; source files remain byte-identical. Fixed actual run asserts baseline Exit0, complete report, killed0 and mutants[].
- Full final: `cargo test --workspace --all-features`, `/private/tmp/issue481-workspace.log`: exit0, 1620 passed, 0 failed, 13 ignored across69 result groups. Includes existing downstream formal oracles.
- `cargo fmt --all -- --check`: exit0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `/private/tmp/issue481-clippy.log`: exit0.
- `git diff --check`: clean. No sorry/admit/unbounded heartbeats introduced. No callable IDE/JetBrains project tools found in available metadata; no unrelated projects inspected.

## Lean model and correspondence

New `ComprehensionBindingModel.lean` imports the existing BindingFlow model, derives destination, applies write facts and invokes existing candidate resolution. Four kernel-checked routing theorems cover skipping, normal boundaries, global and nonlocal routing; a fixed theorem distinguishes old current-scope binding. `ComprehensionBindingAuditMain.lean` generates11 cases with at most4 frames and2 names. Generated expectations are not manually edited or copied from Rust. CI registers separately bounded module/executable builds, freshness and sensitivity checks.

Explicit broken variants are old current-scope write, ignoring containing directives, crossing function boundaries, and dropping static declarations when a body does not execute. Global/lambda/static witnesses change candidate outcomes; nonlocal owner sensitivity is model-only since Rust's enclosing static local already suppresses the candidate. Transactions/idempotency do not apply. Corpus `owner` is metadata, not a Rust owner observation. Shared Frame.directive is bounded to these sources and untouched-name mapping; no claim about arbitrary per-name directive combinations. Source-to-model paths are explicit fixtures, not a verified Python parser. Public strict observation is candidate eligibility; CPython independently checks actual value/UnboundLocalError behavior. No arbitrary generator interprocedural/lifetime analysis or illegal class-body walrus semantics claimed.

Detailed correspondence worksheet and reproducible commands are `/private/tmp/issue481-formal-audit.md` for controller persistence.

Every Lean run was serial, with worktree-local copied cache and guard `python3 tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 2048 --sample-ms 250 --stats /private/tmp/issue481-NAME.json -- COMMAND`; package arguments remain `-j1 -DElab.async=false`.

| Run suffix | Result | elapsed ms | peak KiB |
| --- | --- | ---: | ---: |
| lean-dependency | `lake build HoiminOracle.BindingFlowModel`, pass |3580|702048|
| lean-red-clean | `lake env lean -j1 -DElab.async=false HoiminOracle/ComprehensionBindingModel.lean`, old-rule theorem false |3021|663520|
| lean-green | `lake build HoiminOracle.ComprehensionBindingModel`, pass |2747|643888|
| lean-generate | `lake env lean -j1 -DElab.async=false --run ComprehensionBindingAuditMain.lean --output corpus/comprehension-bindings.jsonl`, pass |3026|682160|
| lean-freshness | same --run with --check corpus/comprehension-bindings.jsonl, pass |3012|684864|
| lean-sensitivity | same --run with --sensitivity,11cases/4frames/true |3030|674128|
| lean-executable | `lake build generate_comprehension_bindings`, pass |4381|695872|
| lean-ci-check-fixed | `lake exe generate_comprehension_bindings -- --check corpus/comprehension-bindings.jsonl`, pass |7738|779680|
| lean-ci-sensitivity | `lake exe generate_comprehension_bindings -- --sensitivity`, pass |561|56848|

Each suffix corresponds to `/private/tmp/issue481-SUFFIX.json`. No timeout/RSS limit hit; no limit increase. Initial `lean-model` was `monitor_error`126 (41ms), so ps monitoring required escalation; it is infrastructure evidence only. Initial theorem elaboration needed propositional equality rather than BEq for simp (`lean-red`); clean RED then failed solely for the intended false witness. Exact CI invocation first exposed a forwarded leading `--` (`lean-ci-check`, exit2); generator now strips that delimiter and exact invocation passes. Early public-run test used incorrect report JSON keys and other-pair sources introduced literal collection candidates: fixed assertions to actual report schema and used range inputs to isolate builtin calls. These were fixture/adapter errors, not production bugs or weakened expected decisions. Initial shell path mistakes made no semantic observations.

## Implementation self-review (three passes)

1. Lexical ownership: traced nested comprehension parent walk, lambda/function stop, first iterable outer evaluation, ordinary targets, and containing directives. Confirmed only named targets use new routing, and no blanket operator/file suppression is introduced.
2. Execution/flow: traced RHS first, expression-end event, conditional depth restoration, possible_bindings and back-edge recording. Verified exact unknown/static-shadowed distinctions and pre-comprehension builtin positive; inspected delete/import/reset effects and obtained controller's no-new-lifetime scope clarification.
3. Model/integration: checked derived owner feeds writeAt then resolveCandidate, explicit broken variants, generated-source mapping and CI guarded commands. Documented nonlocal no-write and shared-directive abstraction; corrected CI delimiter behavior. Existing formal oracle files/corpus remain unchanged.

## Test self-review (three passes)

1. Sensitivity: captured actual binary public RED separately for both endpoints, expanded historic regression includes real false kill, exact-fact RED distinguishes Unknown. CPython runs before plan comparison; runtime errors cannot count as semantic passes.
2. Controls/coverage: real candidate IDs/spans/operators/replacements for unrelated names/scopes, ordinary iteration, lambda/function and other builtin pairs. Runtime asserts distinguish empty/lazy from executed writes and static unbound locals; reset forms execute next(). Corrected literal-list fixture interference without relaxing candidate counts.
3. Reliability/limits: portable repository Python path,20second child bound, kill-on-drop, independent temp dirs, exact source preservation, strict corpus schema/unique IDs/all11cases/3controls. Full workspace and fmt/Clippy pass. Lean proof, model metadata, public eligibility and CPython runtime facts are reported separately; no unverified implementation-proof claim.

## Remaining limits

No unresolved required implementation work identified. No claim of exhaustive Python semantics, verified AST routing correspondence, per-name arbitrary directive theorem, generator scheduling analysis, or new builtin-reset precision. Controller/task reviewers may inspect the owned commit and these logs; no push/PR performed by this task.

## Controller PR self-review

1. Scope/provenance: inspected the21line routing change, original4adf809 RED and actualfalsekill, scoped tests/formal/CI changes; no dependency on the other issue branches, runtime injection or unrelated resolver rewrite. Existing conservative uncertainty satisfies the requested zero/lazy rule; no compatibility or test waiver.
2. Evidence: independently counted final69 resultgroups=1620pass/0fail/13ignore and inspected Clippy output plus all guarded JSONstats. Successful max7738ms/779680KiB matches report. Kept first monitor-error, theorem-elaboration and CI-delimiter failures distinct from clean RED/finalGREEN.
3. Reviewability: preserved full formal correspondence worksheet and all3implementation/3test reviews here; PR states observable behavior and finite proof limits. Checked16pageOKF hashes/pairs/links,161actual/index/displaycount and whitespace. Task/final review outcomes recorded after completion.

## Independent task review

review481_task found no actionable findings in4adf809..0fcb3bf after reviewing routing/flow, publicCPython/plan/run regressions, boundedLeanmodel/generator/corpus and CI. Correspondence limitations match implementation; no mutations or test reruns.

## Hosted CI follow-up

The first PR run34599263747 failed Wheel smoke in `LeanAuditWorkflowContractTests.test_bounded_audit_covers_every_module_and_generator`. The new executable was registered in lakefile/CI but absent from the Python inventory. Inspection also found its CI gate had been inserted in the middle while lakefile appended it at the end; the existing test deliberately requires consistent order. Raw job log: /private/tmp/issue481-ci-wheel-clean.log. Lean audit itself passed.

Added the inventory entry and moved only that new CI gate to the matching final position. Preserved all guard commands, stop-on-failure assertions and corpus checks. `python -m unittest discover -s tests -p test_ci_workflow.py -v` passed26 tests in6.830s with repository CPython3.14.7; log /private/tmp/issue481-ci-workflow-fixed.log. No Rust or Lean semantic code changed, so their previously green suites were not rerun locally.

Three follow-up self-reviews:1 traced the hosted assertion through lakefile order, Python mapping and CI gate expansion;2 ran the whole workflow contract module, retaining every inventory and failure-stop assertion;3 inspected the two small registry/order edits and whitespace, recorded the initial integration failure and requested scoped independent review before pushing. No test expectation was weakened and no failure was waived.
