# Issue 478 implementation plan

Spec: ../specs/2026-09-11-issue-478-analysis-depth-design.md

Independent worktree from4adf809; no merges. Controller owns docs/superpowers and docs/knowledge. One architecture-level implementer owns Rust analyzer/error propagation/public regressions, README and docs/development current contract. No subagents/cargo-mutants. Every Cargo uses CARGO_INCREMENTAL=0 and CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target, one active implementer. CPython3.14.7 via untracked .venv symlink. Keep stdout/stderr bounded and record raw logs in /private/tmp/issue478-*.log.

## Plan self-review

1. Dependency order: reproduce with known source provenance, isolate recursive analysis from parsing/disposal, establish depth/error contract, then implement and validate all public callers. The guard alone is insufficient if rejected AST disposal still aborts.
2. Coverage: pair explicit depth rejection with accepted-boundary candidates and flat breadth, multiple AST child kinds, cancellation and cleanup. Both debug and release public CLI tests are required; candidate limits are not a depth guard.
3. Resources/scope: use subprocess deadlines and disable core dumps for intentional RED, record normal runtime versus explicit2MiB thread probes separately. No blind stack enlargement, forgotten AST allocations, giant full-source parser rewrite or unrelated resolver changes. Complete focused tests before fullworkspace, avoid repeated full green runs.

## OKF self-review

1. Read analyzer concept and input-scale audit against analyze_source_cancellable and both callers. Existing candidate-retention evidence explicitly does not prove input AST depth or all-memory bounds.
2. Add issue-specific source to analyzer concept and full design index, with exact source hash. Preserve previous audit revisions and reported unresolved states as historical evidence.
3. Validate source-footnote pairing, source hashes, reserved metadata, links/reachability and spec index including displayed161 count. Recheck hashes after any evidence-driven design refinement.

## Task 1: Bound AST analysis depth and preserve typed failure/cleanup

Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-478`, base4adf809. Read full spec, this plan, /private/tmp/issue478-preflight-notes.md and originalissue body in /private/tmp/hoimin-bug-issues.json. Own relevant Rust core/CLI files only if needed, analyzer code/tests/public CLI tests, README/docs/development. Controller owns all docs/superpowers/knowledge. No delegation. Report concrete design concerns and smallest sound alternative before material expansion or deviation; controller will refine design autonomously.

- [x] Known-source semantic RED: CPython3.14 normal compile, old debug2k/release10k flat binary terms or measured equivalents with --max-candidates1. Use bounded subprocesses and core dumps disabled; do not run an unbounded overflow inside the main test runner. Existingmain target/release binary has unknown provenance: label diagnostic-only or build controlledbase; never claim its sourcecommit unverified.
- [x] Check pinned Ruff SourceOrderVisitor/AnyNodeRef immediate-child protocol; implement structural worklist guard before AstFacts/name/annotation passes, covering all recursive AST kinds. Initial limit128 must be confirmed with exact depth counting and measured normal/2MiB boundary tests. Do not use source-size/candidate cap/raised worker stack as proxy.
- [x] Separately validate parse and rejected AST disposal for20k and larger validCPython expressions. If disposal canoverflow, design bounded-stack ownership/disposal using existing AST facilities or a sound bounded alternative. No mem::forget/leak, no silently incomplete model. Send evidence before major cleanup ownership work and do not commit a guard that still aborts on required valid fixtures.
- [x] Introduce explicit depth-error distinction from cancellation; both discover and run analyzer callers must map it to source path/cause and failed/incomplete outcome. Preserve invalid syntax/cancellation semantics and existing ordering/IDs/spans. Adapt noncancellable test helper honestly rather than returning a fake successfulempty result.
- [x] Tests: exact-at-limitaccepted+first-overrejected, belowlimit realcandidate, flatbreadth unaffected, binop/name-resolution/type-annotation/f-string/typeparam/pattern ASTchildren as validgrammar permits, cancellation, repeatedrejections/disposal inoneprocess, originalsource preserved and candidate-store/workspacecleanup. Include largeguarded public run/plan subprocesses (debug+release) proving normalnonzero diagnostic, no SIGABRT, no completedemptyplan. Document baseline timing accurately, do not change unrelatedrunpipelineorder.
- [x] Currentdocs describefinite supporteddepth/rejection diagnostic and distinctionfrommaxcandidates; explain actualtestinglimits. Lean onlyifmeaningfulabstractguard claimwith implementationcorrespondence; do notclaimRuffcoverage/native-stack/parser/drop safety fromabstractproof. If no newLean needed, retain/downstreamexistingoracles tests and stateboundary.
- [x] Run focusedanalyzer/core/publicregressions, controlledreleasevalidation, fullworkspaceallfeatures, fmtcheck, Clippyalltargetsallfeatures-Dwarnings. Exactcommands/logs/failuresandfollowups required. Do not run concurrentCargo orcreatemanytargets; disk~19GiBfree atpreflight, monitorbeforereleasebuild ifneeded.
- [x] Threeimplementation andthreetestselfreviews withconcreteevidence. Commitonlyownedfiles, no pushPR; fullreport to scratchtask-1-report.md. Do not repeatgreenfullsuite without newchange/failure. IDE MCPcannotaccesshoimin becauseprojectnotopen; Cargo providesdiagnostics.

## Ruling: explicit AST disposal is required

Pinned Ruff debug dependency probe on an explicit2MiB thread parses20k terms then aborts during ordinary drop; CPython3.14 compiles the20k input normally.2k/10k controls pass.40k/80k are not valid-Python acceptance evidence because CPython rejects them. Controller approved the smallest sound alternative proposed by implementer: detach recursive children using existing Transformer into an owned worklist and drop shallow shells; include rejection, cancellation and parser-error ownership, using retained parse API if checked parse drops partial AST internally.

Cost: extra owned traversal and heap worklist, with child coverage coupled to pinned Ruff definitions; dependency changes require renewed coverage review. Actual repeated2MiB/default-runtime/debug/release checks must support the final claim. No leak/forget/stack-size waiver. This substantive design ruling and cost must reach final user report.

Design/OKF refinement selfreviews:1 inspected parse/drop logs and separated CPython-valid20k from rejected40k/80k;2 required auxiliary recursion and all parse exit paths, not just guard rejection;3 updated both spec source hashes and reran OKF checks.

## Controller implementation/test review follow-ups

1. Public subprocess fixture initially used Unix `true` and polled termination before reading stdout/stderr pipes. It now uses platform-specific repository Python and temporary output files, avoiding a Windows baseline failure and pipe backpressure. Run JSON asserts complete=false while acknowledging the existing baseline-first order.
2. Named parser boundaries include a deep expression with a trailing plus and a deep valid statement followed by an invalid statement; retained parsing/disposal preserves existing syntax diagnostics for both. Auxiliary source-order/Transformer recursion was reviewed, including the fourth cut point through FString/TString format_spec elements.
3. A separate cleanup integration binary would include the entire analyzer test module again. The allocator-backed isolated child test was moved into existing rust_analyzer.rs, preserving actual heap measurement without another duplicate analyzer suite. Synthetic25k nested format-spec ASTs test FString/TString disposal beyond grammar limits and are not described as CPython-valid source.

These are resolved implementation/test findings, with no compatibility or test-failure waiver. Final verification must include the moved test and latest synthetic fixtures; earlier release cleanup results are intermediate evidence.

## Final implementation and verification record


Worktree: `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-478`
Base: `4adf809`; branch: `fix/issue-478-analysis-depth`.

## Implementation

`rust/depth.rs` checks a borrowed `AnyNodeRef` worklist before facts, name resolution, annotation analysis or candidate production. Module depth is 1; each Ruff source-order child adds 1, including auxiliary nodes. `enter_node` records immediate children and returns `Skip`; Ruff's matching `leave_node` does not mutate any counter. `AnyNodeRef::visit_source_order` dispatches children without re-entering the worklist node. Limit 128 accepts 126 numeric binary terms and rejects 127.

The pinned parser's ordinary AST drop aborts at 20,000 terms on a 2 MiB stack even though CPython 3.14.7 compiles that input. Following the evidence and controller approval, disposal now consumes ownership through an explicit worklist. Ruff Transformer callbacks detach statements, expressions, patterns and interpolated-string elements; the outer loop dispatches each owned node once and drops its shallow shell. Those four callbacks cut recursive cycles, including format specifications. No leaked/forgotten AST, unsafe ownership conversion, raised worker stack or runtime Python helper was introduced.

`parse_unchecked_source(..., PySourceType::Python)` retains invalid partial modules that `parse_module` would drop before returning Err. Invalid syntax still takes the existing diagnostic path; retained partial ASTs use bounded disposal. Cancellation during preflight also disposes the tree. After accepted depth validation, subsequent cancellation and normal return use ordinary drop on the bounded tree.

`AnalysisError` separates cancellation from depth rejection. Both discovery and runtime analyzer callers map depth errors to `analyzer.depth`, retaining effect ID, source path and cause. The noncancellable internal probe requires supported-depth input and includes that requirement in its expect message. It never fabricates successful empty output.

README and development documentation describe the supported depth, counting, error contract, ownership cleanup and limits of the evidence. Controller owns design/OKF changes.

## Controlled RED and parser evidence

All intentional overflow subprocesses used `RLIMIT_CORE=0`/`ulimit -c 0` and 20/30-second deadlines. No existing main-worktree binary was treated as controlled evidence.

Every Cargo command used:

```
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target
```

- `cargo build --bin hoimin --offline`: `/private/tmp/issue478-base-debug-build.log`; before production edits, controlled base debug plan at 2,000 terms returned SIGABRT, `/private/tmp/issue478-base-debug-red.log`.
- `cargo build --release --bin hoimin --offline`: `/private/tmp/issue478-base-release-build.log`; controlled base release plan at 10,000 terms returned SIGABRT, `/private/tmp/issue478-base-release-red.log`. Repro script `/private/tmp/issue478-run-base-release.py` uses the issue CLI options and `--max-candidates 1`; `true` is only a macOS baseline diagnostic command in that historical probe.
- `cargo test --offline -p hoimin-cli --test analysis_depth`: `/private/tmp/issue478-public-red.log`; the regression failed because plan/2,000 terminated by signal 6, before implementation.
- `/private/tmp/issue478-parse-probe.rs`, linked against the pinned Ruff debug rlib with rustc, runs parser then ordinary drop on an explicit 2 MiB thread. `/private/tmp/issue478-parser-drop.log`: 2,000/10,000 parse+drop pass; 20,000 parses then drop aborts; CPython compiles all three. 40,000/80,000 also parse then drop abort, but CPython rejects those and they are not valid-input acceptance evidence.
- `.venv/bin/python` is CPython 3.14.7 with recursion limit 1000. `/private/tmp/issue478-cpython-valid.log` records successful compile for 126, 127, 2,000, 10,000, 20,000 and 25,000 terms. `.venv` remains an untracked symlink and must not be committed.

## Regression evidence

- Final depth unit coverage: `cargo test --offline -p hoimin-cli --test rust_analyzer depth`, `/private/tmp/issue478-depth-focused-final.log`, 7 passed. Release equivalent: `cargo test --release --offline -p hoimin-cli --test rust_analyzer depth`, `/private/tmp/issue478-release-depth-final.log`, 7 passed.
- Earlier focused integration: `cargo test --offline -p hoimin-cli --test analysis_depth --test analysis_depth_cleanup --test rust_analyzer --test analyzer_handler`, `/private/tmp/issue478-focused-integration1.log`, all passed. The temporary standalone cleanup integration binary duplicated analyzer unit tests; its isolated heap test was then moved into existing `rust_analyzer.rs`, and the duplicate file removed. Duplicate executions do not count as additional semantic coverage.
- `cargo test --release --offline -p hoimin-cli --test analysis_depth --test analysis_depth_cleanup`, `/private/tmp/issue478-release-focused.log`, passed public and ownership regressions before the test-only move. Final release depth testing covers the moved heap test and added synthetic nested-format test.
- Public plan/run subprocesses cover 2,000/20,000/25,000 terms. Each returns a normal nonzero status, retains original source and reports path/depth/128. Plan emits no manifest. Run emits JSON with `summary.complete=false` and a baseline, preserving baseline-before-analysis order. Test subprocess output goes to files to avoid pipe backpressure; a 30-second deadline kills/reaps stuck children. The baseline uses the repository Python executable with platform-specific path.
- Invalid partial AST subprocesses cover both a deep expression with trailing `+` and a complete deep statement followed by `broken =`. They retain invalid-syntax diagnostics without abort or substituting a depth error.
- Explicit 2 MiB accepted boundaries generate real binary candidates at 126 terms and real `type_list_sequence` candidates at 125 nested subscripts with a Sequence import. The first over-limit variants reject. Flat 2,000-statement breadth retains a candidate.
- Ruff-valid structural fixtures exercise annotations, type-parameter bounds/defaults, f-strings, t-strings, expressions inside format specs, nested patterns and nested statements. The 130-level statement fixture establishes Ruff traversal coverage, not CPython-valid indentation. A separate auxiliary fixture traverses decorators, comprehensions, parameters, keywords, handlers, with-items, classes, match cases and pattern arguments.
- Synthetic AST ownership stress creates 25,000 nested format-specification elements for both FString and TString and disposes on 2 MiB. These are synthetic owned nodes, not a claim about valid Python formatting grammar.
- The moved allocator-backed test runs 20 repeated 25,000-term rejections in one isolated child/thread with a 2 MiB stack. After parser warm-up, live allocation bytes remain within baseline + 1,024 bytes after each rejection. This checks retained heap allocations rather than a tight RSS threshold or process-exit reclamation.
- Runtime handler test rejects three deep analyses, checks empty candidate spool directory and absent store, preserves source, then analyzes a supported replacement using the same handler. Dropping its owner removes the spool root.
- `/private/tmp/issue478-boundary-cli.py` and `/private/tmp/issue478-boundary-cli.log` record actual debug/release CLI boundary results without `RUST_MIN_STACK`: depth128 exits0 with125 candidates; depth129 exits2 with path/depth diagnostic; flat2,000 statements exits4 with one retained candidate under cap1.
- `/private/tmp/issue478-run-cleanup-cli.log`: actual debug/release run at25,000 terms exits2, reports complete=false and analyzer.depth; execution cleanup reports `clean`, `remaining_root=null`. Delivery reports `cleanup_after_delivery`, matching its normal report-delivery lifecycle. Existing unrelated managed roots remain preserved by the startup janitor.

## Diagnosed intermediate failures

1. Changing the analyzer error type required updating one existing cancellation assertion. An initial broad replacement also changed the separate OperatorImports cancellation assertion; the next compile caught that mismatch and it was reverted. Logs: `issue478-depth-focused1.log`, `issue478-depth-focused2.log`. Focused3 then passed.
2. The first nested-annotation boundary test expected a Sequence replacement without importing Sequence. The existing binding rule correctly produced no type candidate. Adding `from typing import Sequence` made the fixture eligible without changing production semantics. Logs: `issue478-depth-focused4.log`, passing `issue478-depth-focused5.log`.
3. First Clippy pass flagged explicit Default type spelling, a semicolon, and format-collect in new code. All corrected without lint suppression; `issue478-clippy1.log` records failures and `issue478-clippy2.log` records the first green result before the final test edits.

## Three implementation self-reviews

1. Traversal/counting review: checked pinned `source_order.rs`, `node.rs` and generated AnyNodeRef dispatch; child-only dispatch plus Skip prevents guard recursion, and module/leaf counting matches exact boundary tests. No operator-specific variant inventory or source-length proxy was added.
2. Ownership review: checked the full pinned Transformer dispatch and auxiliary callbacks. Statements/expressions/patterns/interpolated elements cover recursive cycles; nested f/t format specs have a separate synthetic stress test. Retained parse errors and preflight cancellation both consume ownership through disposal. Ordinary accepted drop only occurs after depth validation.
3. Error/lifecycle review: traced both call sites and failure codes/IDs. Cancellation still maps to existing `analyzer.cancelled` text; depth maps to path-bearing `analyzer.depth`. Store failure releases ownership; reuse and actual run cleanup pass. Candidate production code, spans, IDs and source ordering are unchanged.

## Three test self-reviews

1. Reproducer/provenance review: source-controlled debug/release RED precedes production edits; intentional native aborts are isolated and bounded. CPython-valid 20k/25k evidence is separate from CPython-rejected 40k/80k and synthetic AST fixtures.
2. Behavioral coverage review: paired rejection with exact accepted candidate generation and flat breadth; covered AST child categories, post-parse cancellation, invalid partial nodes, repeated heap cleanup and store reuse. The original token-cancellation test now accounts for preflight probes so it still cancels in its named token phase instead of silently becoming guard-only coverage.
3. Harness/claims review: public tests use controlled cross-platform Python and file-backed child output with deadlines. Run assertions include incomplete JSON and existing baseline ordering. Moved heap test into existing integration binary to avoid an extra analyzer suite copy. Memory claim is baseline+1KiB live allocation evidence; no universal parser/native stack theorem is claimed.

## Evidence limits

No new Lean model was needed: the material obligations are correspondence with pinned Ruff traversal, native stack measurements and ownership cleanup. Existing Lean-derived oracle tests remain in the workspace suite. The parser still runs before the guard, so this change does not establish safe parsing of arbitrary inputs or universal OS stack safety. Worklist allocations can scale with input breadth; candidate cap/depth cap are not total process-memory bounds. No cargo-mutants, subagents, external targets, push or PR operations were used.

## Final verification

- `cargo test --offline --workspace --all-features`: exit0, `/private/tmp/issue478-workspace-all-features.log`; 69 result groups, 1,623 passed test executions, 0 failed, 13 ignored. This is execution count, including existing integration/private-module overlap, not a count of unique new behaviors. Existing Lean-derived oracle tests passed. The suite was run once after focused/release stabilization and was not repeated after green.
- `cargo fmt --all -- --check`: exit0, `/private/tmp/issue478-fmt-check.log`.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: exit0, `/private/tmp/issue478-clippy-final.log`.
- `git diff --check`: clean. Shared target and disabled incremental compilation were used throughout; measured free disk remained about20GiB after the controlled release build.

## Commit and handoff

Committed only the eight owned implementation/test/current-doc files as `4e10799` (`fix(analyzer): reject unsupported AST depth and dispose safely`). No push or PR. Controller's docs/knowledge and docs/superpowers changes remain uncommitted for controller review; `.venv` remains untracked. The ignored scratch report is the handoff artifact and is not part of the commit.

## Controller PR self-review

1. Scope/provenance: reviewed the eight implementation files against base4adf809 and source-controlled RED evidence; retained the independent issue worktree, current docs and exact supported-depth contract. Parser-before-guard and native-stack limits remain explicit.
2. Evidence: checked final raw all-feature result groups (1,623 pass, 0 fail, 13 ignore), final fmt/Clippy, release boundary and public/cleanup evidence; distinguished the intermediate standalone cleanup harness from the final moved test. No green test failure was waived.
3. Reviewability: PR explains observable error, ownership-disposal requirement and its maintenance cost; checked OKF source hashes, all161 design entries/display count, links and whitespace. External task/final review results are recorded below after completion.

## Independent task review

review478_task found no actionable spec or quality findings in4adf809..4e10799. Reviewer independently checked pinned Ruff child-only/Skip traversal, all four Transformer disposal cut points, retained partial-tree/cancellation ownership, both error callers and final raw logs. No Cargo rerun or mutation; documented parser/worklist/dependency limitations accepted as the measured scope.
