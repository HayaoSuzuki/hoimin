# Issue 486 implementation plan

Spec: ../specs/2026-09-11-issue-486-type-parameter-bindings-design.md

Independent base4adf809. Controller owns docs/superpowers and docs/knowledge; one architecture implementer owns affected Rust resolver code, tests and current user/development docs. No merges, delegation or cargo-mutants. All Cargo runs serially with `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`. Use untracked .venv (CPython3.14.7), temporary fixtures and /private/tmp/issue486-* logs.

## Plan self-review

1. Reproduce both source and replacement endpoint errors publicly before changing scope representation. Header/body/outer positives are necessary to reject a blanket generic exclusion.
2. Read every exhaustive NameScopeKind branch and class-parent/nonlocal lookup. Insert a real binding scope with explicit annotation-versus-body class access; preserve evaluation order and existing flow state.
3. Use compile-valid CPython identity observations, including forced lazy evaluation where needed. Any Lean work must derive lookup and correspond to public Rust observations; CI generator additions require the independent Python workflow inventory tests.

## OKF self-review

1. Compare analyzer and selection contracts with the current resolver and authoritative annotation/type-parameter scope rules.
2. Add issue-specific design source and exact design-index heading without changing historical revisions or claiming all previous audit inputs cover PEP695.
3. Validate metadata, hashes, source-footnote pairs, links, reachability and161 design files/index/display entries after final refinements.

## Task 1: Resolve type-parameter binding scopes

Read the spec, /private/tmp/issue486-preflight-notes.md and issue486 in /private/tmp/hoimin-bug-issues.json. Worktree: /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-486; branch fix/issue-486-type-parameter-bindings; base4adf809. Own relevant analyzer Rust code, public tests and current documentation. Controller owns docs/superpowers and docs/knowledge. Do not delegate, push or create PRs. Send concrete evidence and the smallest sound alternative before a material scope/design expansion.

- [x] Capture public plan RED for generic function/class names shadowing both builtin substitution endpoints, using CPython-valid original fixtures. Reproduce an actual incorrect run result where it adds evidence; no production Python dependency.
- [x] Inspect NameResolutionBuilder, every scope resolver branch, parameter/header traversal and existing type-parameter name extraction conventions. Establish a narrow annotation/type-parameter scope between outer and body; do not substitute ordinary body locals or skip whole generic declarations.
- [x] Preserve function decorators/defaults outside and annotations/bounds/default types inside their proper scope. Class bases/keywords see type parameters, decorators remain outside. Inspect existing type-position suppression before deciding whether any type-alias expressions are affected. Preserve #263 annotation runtime-candidate suppression; positive runtime header controls use eligible decorators/defaults/bases/keywords, while annotation identity can be observed separately through CPython. Preserve nested body/closure/comprehension lexical access and restore outer scope afterward.
- [x] Distinguish annotation access to an immediately enclosing class namespace from ordinary method/comprehension lookup that skips class bindings. Keep generic class type parameters visible through the relevant parent scope. Preserve legal directives and ordinary function locals; do not use invalid nonlocal-typeparameter fixtures as acceptance evidence.
- [x] Public matrix covers source/destination names across collection, any/all, min/max, sorted/reversed and exception families, generic functions/classes and legal TypeVar/TypeVarTuple/ParamSpec declarations. Assert real retained positive IDs/spans at outside/header/body boundaries. Force CPython lazy annotation/bound/default evaluation for observations when required, and compile originals first.
- [x] Determine whether bounded Lean scope semantics would add useful evidence. If used, derive lookup/routing and generated expectations, compare with actual Rust/CPython observations, and document abstraction limits. Follow existing serial30second/2048MiB/250ms guard, -j1 and -DElab.async=false. Register exact generator/corpus/CI inventory and run tests/test_ci_workflow.py when CI changes. No limit increases or table-only proof claims.
- [x] Run focused public/binding tests, full `cargo test --workspace --all-features`, fmt and Clippy all targets/features. IDE MCP last reports hoimin notopen; use Cargo diagnostics and avoid unrelated projects.
- [x] Perform three implementation and three test self-reviews. Record commands, RED/GREEN evidence, exact final totals, limits and rulings. Commit only owned files and write .superpowers/sdd/2026-09-11-issue-486-type-parameter-bindings/task-1-report.md. Stop for review without repeating green suites absent a concrete concern.

## Lookup architecture refinement

Accept TypeParameters static locals with distinct direct-header, ordinary-body and class-body parent lookup. Class bodies must preserve ordered module lookup through the new scope while ordinary function/comprehension bodies still skip class variables. Split default-expression and annotation traversal. No new Lean/CI model is needed when actual CPython/runtime candidate correspondence supplies evidence.

Three refinement reviews:1 compared all existing NameScopeKind resolver branches and the missing intermediate binding scope;2 separated immediate class header visibility, nested class-body lookup and deferred function-body module behavior;3 retained #263 type-position suppression and requested legal directive/outer-restoration positives before refreshing both OKF source hashes. No semantic/test waiver or new unrelated resolver subsystem.

## Persisted implementation and verification report

# Issue 486 implementation report

Status: implemented; awaiting controller review. Independent base `4adf809`, branch `fix/issue-486-type-parameter-bindings`. Implementation commit: `bae4033` (`fix(analyzer): resolve generic type parameter bindings`).

## Owned changes

- `crates/hoimin-cli/src/analyzer/rust.rs`: add explicit `TypeParameters` scope between the enclosing scope and generic function/class body. Register tracked TypeVar, TypeVarTuple and ParamSpec names before visiting bounds/default types. Function decorators and ordinary defaults remain outside; parameter/return annotations enter the new scope. Generic class arguments enter it; decorators remain outside.
- Direct header lookup may access its immediately enclosing class and ordered module state. Ordinary function/comprehension lookup retains type parameters but skips ordinary class bindings. Class-body parent lookup checks type parameters while continuing to skip enclosing classes and preserve ordered module lookup.
- The new intermediate scope preserves synchronous header visibility for loop back-edge and temporary exception-target state. Existing identical temporary/loop visibility traversal now shares the helper. Nonlocal lookup deliberately does not treat type parameters as legal nonlocal rebinding targets; legal global/nonlocal controls remain covered.
- `crates/hoimin-cli/tests/type_parameter_bindings.rs`: six public/CPython tests; 60 pair/name/type-parameter-kind/declaration matrix plans and 13 additional boundary plans. Each original is compiled by the controlled Python before planning. Retained positive candidates assert exact source byte start, original bytes, line, uniqueness and stable ID calculated from source hash/span/operator/replacement. Source files remain unchanged. Literal collection candidates are deliberately excluded from the builtin-name pair comparison.
- `crates/hoimin-cli/tests/fixtures/type_parameter_bindings.py`: executable CPython identities for function/class decorators, defaults, bases/keywords, generic body/closure/comprehension access, nested class header versus body, ordinary methods, header comprehension/lambda, forced lazy annotations/bounds/defaults, all three type-parameter forms and legal directives. Also reproduces the issue example: original result `[1, 2]`, manually substituted destination raises TypeVar-not-callable TypeError. This is an executable CPython mutation reproduction, not a claim of a pre-fix full `hoimin run` result.
- `README.md`: concise builtin pair scope contract.

## Decisions and scope limits

No Lean additions: real scope routing plus direct public Rust/CPython correspondence provides the useful evidence here; no table-only formal claim. No generators or CI changes. Type aliases and annotation expressions are already excluded from runtime candidate collection by annotation spans (#263), so no general PEP 649/type-alias rewrite. No #481 walrus work. Ordered/static/conservative module lookup and dynamic namespace uncertainty otherwise retain their existing policies. No production Python dependency. No native platform code changed: verification was macOS; Windows/Linux native cfg execution is not claimed. IDE project unavailable per controller preflight, so Cargo diagnostics used. No cargo-mutants, push, PR or merge.

## Implementation self-reviews

1. Enumerated every NameScopeKind matcher, resolve_scope/resolve_parent/resolve_class_parent/resolve_nonlocal, binding/wildcard recording, loop visibility and temporary bindings. Chose separate direct-header and ordinary-body paths, avoiding treating generics as ordinary body locals. Compared name extraction against KnownImports and operator-functions conventions.
2. Reviewed evaluation/traversal boundaries and scope restoration. Confirmed actual CPython nested class headers read enclosing class values while class bodies, ordinary methods and header-comprehension bodies skip them; type parameters remain lexical. Found a new intermediate-scope loop-header regression with a public RED, then made synchronous header traversal transparent without granting ordinary class-body access.
3. Reviewed final diff for directives, annotation suppression, source/destination symmetry, class/module ordering, nested closures and scope exit. Verified original non-generic paths remain structurally the same, except parameter defaults/annotations are explicitly separated. Confirmed no annotation runtime positives were required and no unrelated resolver subsystem/CI changes were introduced.

## Test self-reviews

1. Initial public RED demonstrated real false source/destination candidates, including class headers, bodies, closures and comprehensions. Refined the helper to compare builtin-name candidates rather than unrelated collection literal candidates. Matrix originals compile; all three type-parameter syntaxes are observed with CPython identities.
2. Added same-generic-body positive controls, outside/header positives, legal directives and ordinary locals, ordered module and exception/loop boundaries. Public boundary RED was 4 passed/1 failed, showing a wrongly retained generic header in a module loop. Green after the transparent scope fix.
3. Strengthened retained-candidate assertions from lines to exact byte starts and stable IDs, separating annotation/default expressions across lines to avoid an annotation candidate masquerading as a default candidate. Added explicit class-base/decorator and header-comprehension-versus-first-iterable observations. Initial added base fixture returned object and hit Python's object/Generic MRO conflict; changed it to a distinct Base class, then the affected six-test module passed. This was a fixture error, not an analyzer regression.

## Commands and evidence

Every Cargo build/test used `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`, serial invocations, no new target. Controlled `.venv` CPython 3.14.7; bounded temporary fixtures. Free disk checked at about 13 GiB during full validation (17 GiB initially).

- `cargo test -p hoimin-cli --test type_parameter_bindings`: `/private/tmp/issue486-red.log`, 0 passed/2 failed with false candidate lines `{1,3,5,6,7,9}` versus `{1,7,9}` and header lines `{3,4,5,9,10,14,15,16,17}` versus `{3,4,14,17}`. The first exploratory helper run also counted literal candidates and was corrected before this saved RED.
- First implementation GREEN: `/private/tmp/issue486-green.log`, 2 passed.
- Boundary RED/GREEN: `/private/tmp/issue486-boundary-red.log`, 4 passed/1 failed; `/private/tmp/issue486-boundary-green.log`, 5 passed.
- Expanded public checks: `/private/tmp/issue486-focused-public.log`, 6 passed.
- `cargo test -p hoimin-cli --lib analyzer::`: `/private/tmp/issue486-focused-analyzer.log`, 191 passed, 0 failed, 2 ignored, 408 filtered. An initial `analyzer::rust_tests` filter selected zero tests (601 filtered), so it was corrected; zero-test execution is not counted as validation.
- `cargo test --workspace --all-features`: `/private/tmp/issue486-workspace.log`, exit 0. Top-level results: **1612 passed, 0 failed, 13 ignored** across 68 result lines including zero-test/doc-test suites. Raw 70-line log sum is 1614 passed because two subprocess harnesses each print 1 pass/600 filtered; exclude those from top-level totals.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: `/private/tmp/issue486-clippy.log` and `/private/tmp/issue486-clippy-final.log`, exit 0.
- Final test-only fixture expansion initially failed (5 passed/1 failed): `/private/tmp/issue486-focused-public-final.log`, object/Generic MRO fixture issue. Corrected final affected module: `/private/tmp/issue486-focused-public-final-green.log`, **6 passed, 0 failed**, 1.00 seconds. Production code is unchanged since full-workspace GREEN; no unjustified full-suite repetition.
- `cargo fmt --all -- --check` and `git diff --check`: exit 0.
- Initial scratch CPython probe omitted the required method argument; corrected it to `method(None)` before retaining the fixture.

Controller-owned docs/superpowers and docs/knowledge changes are deliberately excluded from the implementation commit; `.venv` remains untracked.

Commit initially hit the sandbox index.lock write restriction; the authorized commit succeeded through sandbox escalation. No automatic-review rejection occurred. Final strict Clippy after the fixture correction exited 0 (0.55 seconds).

## Controller-requested actual CLI run supplement

After implementation commit, controller requested an actual run regression. Added `generic_type_parameter_destinations_cannot_create_false_kills` in `crates/hoimin-cli/tests/run_e2e.rs`, committed as `55ea5c3` (`test(cli): prevent generic type parameter false kills`). It uses the existing `run_project_options` helper and actual run entrypoint, controlled Python, one job, 5-second baseline and 15-second total limits, and at most one mutant. Both generic function and class fixtures assert baseline `Exit(0)`, successful CLI exit, complete result, empty mutant array, zero kills and preserved original source.

The exact `list((1, 2))` example has a legitimate independent tuple-literal/list-literal candidate. With controller agreement, the run fixture uses `f[tuple](items): return list(items)` and passes the tuple in the test command; class fixture uses `list(range(1, 3))`. This isolates the erroneous builtin-name destination and permits an honest zero-mutant assertion. The exact reported source remains covered by the executable CPython original/mutant fixture; no literal mutation is suppressed to manufacture a zero count.

- Focused new test: `/private/tmp/issue486-run-final.log`, 1 passed, 59 filtered, 0.33 seconds.
- Complete affected run module: `/private/tmp/issue486-run-module-final.log`, **60 passed, 0 failed**, 13.77 seconds.
- Strict all-target/all-feature Clippy after addition: `/private/tmp/issue486-clippy-run-final.log`, exit 0.
- Final fmt and staged diff checks: exit 0.

Self-review of supplement: (1) test command actually imports and executes the function/class; (2) report baseline and completion assertions prevent vacuous zero-candidate success; (3) legitimate literal mutation is isolated, time/resource limits use existing harness, and source preservation is asserted. Production code remains unchanged since full-workspace validation. The recorded full-workspace count of 1612 predates this one added test; do not relabel it as a fresh 1613-test full run.

## Controller review and PR preparation

The direct-header/class-body/ordinary-body distinction fits the original binding-scope design; no semantic or test waiver. Root reviewed the resolver branches and CPython fixture, then requested actual CLI run evidence. The run fixture isolates name substitution from the legitimate literal mutation in the exact reported source. Detailed costs are limited to an additional static binding scope and maintaining the explicit header traversal alongside Ruff visitors; no new dependencies or native API.

Three controller implementation reviews:1 compared every changed lookup path with old ordinary-scope behavior;2 checked separate defaults/annotations and immediate class access against executable CPython identities;3 checked loop/temporary binding routing, directives, positive candidate IDs and unchanged source construction after the worker's boundary fix.

Three PR self-reviews:1 checked issue scope and independent base against owned commits, preserving #263 and #481 separation;2 distinguished public RED/CPython mutation reproduction, full-workspace counts before the added test and final focused results;3 checked design/plan/OKF source metadata and index counts, kept untracked .venv out of commits and disclosed native/IDE/Lean limits. Independent task/final review outcomes are recorded when available.

Final OKF check:16 pages,309 source-footnote pairs,612 local links, all pages reachable; actual design files161, indexed161, displayed161. No previous audit is relabeled as a proof of PEP695. Design, plan, OKF, implementation, tests and PR each received at least three recorded self-reviews.

Independent task reviewer review486_task approved4adf809..55ea5c3 with no actionable findings. Additional read-only inspection covered omitted resolver/directive context and temporary-binding propagation; no suites rerun or deferred risks. Final whole-branch review follows the documentation commit.
