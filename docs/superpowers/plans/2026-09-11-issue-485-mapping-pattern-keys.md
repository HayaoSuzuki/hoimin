# Issue 485 implementation plan

Spec: ../specs/2026-09-11-issue-485-mapping-pattern-keys-design.md

This independent worktree starts from4adf809. The controller owns docs/superpowers and docs/knowledge. One architecture implementer owns analyzer Rust code, public tests and relevant current documentation. No merges, delegation or cargo-mutants. Every Cargo command uses `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`; only one Cargo process at a time. Use the untracked .venv symlink for CPython3.14.7 and /private/tmp/issue485-* for logs. Destructive mutation checks use temporary fixtures only.

## Plan self-review

1. Start with actual plan candidates and CPython compilation, then isolate the same-mapping key comparison. Ruff parsing alone cannot expose this error; the reported actual run must distinguish invalid kills from valid positive candidates.
2. Inspect pinned Ruff integer and float representations before selecting semantic equality. Candidate-domain restrictions may avoid general arithmetic, but numeric cross-type equality and integer-real-to-complex rounding need explicit evidence and a sound algorithm.
3. Keep boolean/binary operator selection explicit because the independent base does not contain #468. Require per-mapping restoration, nested/value/dict positives, source IDs/spans and full Rust verification; avoid production Python compilation or blanket pattern suppression.

## OKF self-review

1. Read analyzer and operator contracts against the pattern visitor and candidate construction. Map the new eligibility rule to literal-key uniqueness rather than general parse validity or runtime named-key equality.
2. Add the exact issue source and design-index heading while preserving historical revisions. Explain CPython constant equality and accepted candidates, including the limits of any formal model.
3. Validate all source hashes, footnote pairs, links, reachability and the161 design entries/display count. Refresh hashes when the numerical design is finalized.

## Task 1: Exclude mapping-key edits that introduce literal duplicates

Read the spec, /private/tmp/issue485-preflight-notes.md and issue485 in /private/tmp/hoimin-bug-issues.json. Worktree: /Users/hayao/RustroverProjects/hoimin/.worktrees/issue-485; base4adf809; branch fix/issue-485-mapping-pattern-keys. Own relevant analyzer Rust code, public tests and current README/development docs if needed. The controller owns docs/superpowers and docs/knowledge. Do not spawn agents, push or create a PR. Send concrete evidence and the smallest sound alternative before materially changing the design or adding a numeric dependency.

- [x] Reproduce the reported boolean-pair and complex-conjugate mapping failures through actual public plan candidates. Prove original fixtures compile, then apply candidate edits and use CPython compile to capture RED; an ast.parse-only result is insufficient. Capture the actual import-only false-kill result with a passing baseline.
- [x] Inspect Ruff AST mapping context, numeric literal representation and candidate construction. Select a sound bounded numeric comparison for the actual boolean/complex edit domain, or justify an existing maintained numeric representation. Do not use source text, structural literal Eq, epsilon equality or indiscriminate f64 casts. Account for complex construction rounding, radix/underscore real integers, infinity and signed zero. Communicate the algorithm and limits before committing to implementation.
- [x] Track sibling literal keys within each mapping and suppress only a candidate whose replacement creates equality with a sibling. Preserve nested/adjacent mapping boundaries, value-pattern visits and ordinary dict expressions. Keep accepted candidate IDs, order, byte spans and selection semantics. Use a per-mapping key index or equivalent justified bound rather than a per-candidate full sibling scan; include a meaningful wide-mapping work bound if new indexing needs evidence. Do not execute arbitrary attribute keys or production Python.
- [x] Public compilation matrix covers both boolean/complex key positions, cross-numeric boolean equality, negative real values, signed/imaginary zero, large integer and float precision boundaries, radix/underscore and infinity. Original programs must compile before mutant errors are evidence. Pair exclusions with real retained candidates and compile every selected retained candidate in the bounded matrix.
- [x] Actual run no longer counts the reported invalid edits as kills; baseline succeeds, originals remain unchanged. Positive controls preserve noncolliding edits, single-key patterns, nested/adjacent mappings, mapping values and ordinary dict behavior. Explicit boolean/binary operator selections keep the independent #468 unary grammar issue outside this patch.
- [x] Decide whether Lean can establish a concrete useful equality/collision obligation with executable Rust correspondence. Abstract uniqueness alone is not numerical evidence. If used, follow existing serial30second/2048MiB/250ms guard, -j1 and -DElab.async=false; register exact generator/CI/corpus metadata and run tests/test_ci_workflow.py if CI changes. Do not enlarge limits or invent broad proofs.
- [x] Run focused analyzer/public compilation/run checks, full `cargo test --workspace --all-features`, `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`. IDE MCP currently reports hoimin not open; use Cargo diagnostics and avoid unrelated projects.
- [x] Perform three implementation and three test self-reviews. Report precise commands, logs, RED/GREEN and any justified limits. Commit only owned files and write .superpowers/sdd/2026-09-11-issue-485-mapping-pattern-keys/task-1-report.md. Stop for review, without repeating a green full suite absent a new concern.

## Numeric design refinement

Controller accepted the bounded boolean0/1 plus nonzero-imaginary complex identity model and per-mapping index. Required maintained BigUint parsing/ToPrimitive for complex integer-real conversion rather than custom53-bit rounding logic. Resolved packages: num-bigint0.4.8, num-integer0.1.47, existing num-traits0.2.19; cost two dependencies and bounded operation-specific parsing allocations. No semantic or test waiver. Keep zero-imaginary edits and bool identity recognition for existing zero-imaginary complex0/1 keys. No new Lean model because abstract uniqueness does not establish numeric folding.

Three refinement reviews:1 inspected RuffInt parser constructors and proved Big implies u64 overflow for exact0/1 recognition;2 inspected pinned BigUint float-conversion rounding and MSRV, preserving the distinction between integer overflow and literal float infinity;3 checked same-mapping indexed lookup, signedzero and valid-original premise, and refreshed both OKF source hashes. Public CPython compilation must validate the claimed correspondence.

Controller implementation review:1 read Identity/key_identity/complex conversion against the bounded domain, verifying ordinary integer equality never casts to float and zero-imaginary values0/1 participate in boolean collisions;2 read the per-mapping HashSet and token-range integration for linear sibling visits and exact token eligibility;3 requested sound significant-digit rejection before BigUint arithmetic for already non-foldable huge originals, with all four maximum-digit finite boundaries retained in actual CPython tests. No custom rounding or general numeric subsystem was introduced.

Implementation test-harness corrections reported before final verification: plan ranking is not raw byte-span order, and a real valid surviving mutant gives exit1. Correct added expectations to established public contracts, distinguish the prematurely started intermediate full run from final verification, and require focused GREEN before the final full run. These are corrected test assumptions, not acceptance waivers.

## Final controller self-reviews

Implementation:1 read numeric identities and conversion against CPython's valid-literal domain, including zero-imaginary preservation and exact integer0/1;2 traced per-pattern index, token range and exclusion before retention without changing accepted candidate construction;3 inspected lockfile/dependency source and significant-digit boundaries, with no custom rounding or new runtime Python path.

Tests:1 inspected CPython3.14.7 RED using actual pre-fix candidate IDs and byte spans, distinguishing the earlier host3.12 ad-hoc observation;2 checked48 original/36 retained-mutant compilations and three real runs, including the survivor control and existing rank/sequence behavior;3 independently counted70 successful summary groups to1617 passed/0 failed/13 ignored and read final Clippy, preserving intermediate harness failures and limiting final post-full edits to lint/test portability.

Design/OKF:1 updated bounded numeric model and dependency decision with its cost;2 checked finite digit guards preserve all four radix boundaries and kept invalid originals outside acceptance evidence;3 refreshed both source hashes, validated16 pages/309 source-footnote pairs/612 links/reachability and161 design files/index/display entries.

PR:1 describe duplicate-key compilation failure and candidate-local resulting behavior;2 report actual public RED/GREEN and final verification with correct2001-key wide fixture, no claim of #468 or general numeric proof;3 include dependency/temporary-allocation cost, OKF checklist and issue closure, then submit for independent task and whole-branch review. Draft/private/tmp/hoimin-pr-485.md.

## Persisted implementation report

# Issue 485 implementer report

Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-485`, independent base `4adf809`, branch `fix/issue-485-mapping-pattern-keys`. No delegation, push, PR, merge, cargo-mutants, Python production analyzer, generator or CI change.

## Implementation

`rust/mapping_keys.rs` computes a mapping-local hash index for the actual replacement domain. Bool flips target exactly 0/1. Integers are compared to those targets through exact `as_u64`; ordinary integer keys are never rounded. Nonzero imaginary complex values only compare with complex values. Zero-imaginary conjugation cannot introduce equality in a valid original and remains eligible, while existing zero-imaginary complex values still populate 0/1 identities for bool collisions. Signed zero is canonicalized; legal literal syntax does not construct NaN. Literal float infinity remains supported.

Only integer real parts of complex construction use `num-bigint` 0.4.8 and `num-traits` 0.2.19. The lockfile additionally adds `num-integer` 0.1.47; no existing locked package changes. Local pinned BigUint source uses round-to-odd followed by nearest-ties-to-even conversion and declares MSRV 1.60. Ruff Big integer text is parsed with its original radix and underscores normalized. Significant-digit bounds (decimal309, binary1024, octal342, hex256, ignoring leading zeros) reject impossible finite conversions before BigUint parsing; boundary rounding remains library-owned. Integer conversion infinity is not a foldable valid original complex literal and yields no identity. This design was discussed with and selected by the controller before adopting the dependency; custom rounding was not implemented.

AstFacts records only rejected bool/separator token starts, before token candidate construction. Each mapping creates its own index, with ordinary visitor descent unchanged. Accepted candidate construction/IDs/spans and public ranking remain unchanged. The two passes normalize each key at most twice and perform at most one hash lookup per editable key: expected O(n) work and O(n) storage per mapping, plus source digit/token processing. The 2,001-key wide analyzer fixture rejects 2,000 conjugate edits and retains both the remaining key bool and return bool; it does not assert a flaky wall-clock threshold.

README and development docs describe the bounded eligibility rule. Controller owns design/plan/OKF files; they are not included in the implementation commit.

## Evidence

All Cargo commands used `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`, serially. CPython integration tests use the existing repository helper convention and `.venv/bin/python` (3.14.7). Temporary fixtures only; original subject bytes are asserted unchanged after actual runs. Final test selections explicitly use boolean_literal/binary_add_sub and do not claim the independent #468 unary grammar fix.

- Initial public integration RED: `cargo test -p hoimin-cli --test mapping_pattern_keys -- --nocapture`, `/private/tmp/issue485-red.log`. Original compiles, first bool candidate fails CPython compilation.
- Actual pre-fix public plan/run RED for both reported fixtures: `/private/tmp/issue485-public-red.log`. Both import-only baselines exit0, each run reports killed2 and score1.0. Ad-hoc compile in this first log used host Python3.12.6; actual CLI runs used3.14.7.
- Exact pre-fix candidate descriptors/IDs from those actual runs reapplied using CPython3.14.7: `/private/tmp/issue485-cpython314-red.log`. Both originals compile, all four mutants pass ast.parse and fail compile with duplicate-key SyntaxError; byte spans/original tokens asserted.
- Focused mapping/analyzer tests: `cargo test -p hoimin-cli mapping -- --nocapture`, `/private/tmp/issue485-analyzer.log`, 12 passed. Includes wide-mapping and oversized-integer unit cases.
- Public integration final: `cargo test -p hoimin-cli --test mapping_pattern_keys -- --nocapture`, `/private/tmp/issue485-focused.log`, 5 passed. The 38-case numerical/context matrix contributes23 retained mutants; max-digit boundaries8 originals/4mutants, ordinary dict/adjacent patterns1original/6mutants, UTF8/CRLF ID-selection case1original/3mutants: 48 original compilations and36 retained-mutant compilations total. Additional actual runs: two reported pairs have baseline0, complete=true, zero mutants/kills; valid noncolliding bool control survives1 with established exit1.
- Full: `cargo test --workspace --all-features`, `/private/tmp/issue485-workspace.log`, exit0: 1,617 passed, 0 failed, 13 ignored across70 reported test results (including empty/doc-test results).
- Formatting: `cargo fmt --all -- --check`, `/private/tmp/issue485-fmt.log`.
- Clippy: `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `/private/tmp/issue485-clippy.log`.

The first expanded harness assumed public raw-span ordering and success exit for a surviving mutant; fixed to existing ranked order and survivor exit1. It also compared discovery sequence/rank across changed selection sets; narrowed that comparison to stable ID/span/operator/source fields. An initial full run reached those harness failures before the corrections; preserved at `/private/tmp/issue485-workspace-harness-failure.log`, then rerun only after focused GREEN. Clippy requested doc backticks, checked usize conversions, and flagged exact float comparison. The latter is intentionally required by Python equality and has a narrowly scoped `expect` with reason; no epsilon comparison was adopted.

## Three implementation reviews

1. Numeric-domain review: exact bool/int/float/complex0/1, signed zero, nonzero imaginary exclusion of real-only keys, negative real parts, infinity, both rounding directions around2^64, original radix spelling, and impossible-integer overflow. Selected maintained conversion; no general numeric equality subsystem.
2. Context/work-bound review: per-mapping index and global excluded token starts cannot leak sibling identity between nested/adjacent mappings; values still descend; dict expressions never enter the mapping-pattern index. Two linear passes and one lookup per editable key avoid quadratic sibling rescans. Significant-digit rejection limits expensive parsing for already-invalid enormous literals.
3. Eligibility/compatibility review: suppression precedes candidate retention; no accepted candidate reconstruction, IDs, spans or operator selection implementation changed. Public ranked order and selection-dependent sequence remain existing behavior. #468 unary grammar, named attribute runtime equality and invalid-original acceptance are outside this contract.

## Three test reviews

1. Independent compiler review: CPython compile, not Ruff or ast.parse, validates originals and every retained selected mutant; four actual pre-fix candidates prove RED with stable descriptors. No invalid-original case is used as acceptance evidence; overflow-only unit checks are labelled separately.
2. Positive-control review: both bool/complex positions, cross-numeric collisions, integer/float precision, negative values, real/imaginary signed zero, pure imaginary keys, all radices, infinities, digit-boundary retained singles, nested/adjacent mapping independence, values, dicts, attributes/strings, and retained zero-imaginary edits. Actual run has passing baselines and a real survivor control.
3. Public-contract/resource review: UTF8/CRLF exact spans and operator text, IDs stable under selection and repeated plans, ranked ordering asserted, original bytes preserved. Fixed harness assumptions to existing run/plan contracts. Tests use bounded temp fixtures and existing Python helper environment; no production Python or CI/generator/Lean work added.

## Limits and rulings

No Lean artifact: abstract uniqueness would not prove CPython numeric folding; the bounded-domain reasoning and actual compiler correspondence provide useful evidence directly. No Lean execution or limit increase. IDE diagnostics unavailable because hoimin is not open; Cargo/fmt/Clippy are the verification sources. Initial dependency fetch DNS failed in sandbox, escalated authorized fetch succeeded; no remaining approval block. No additional target directory was created. Full suite is not repeated after green unless behavior changes or new concerns require it; final lint-only/test-portability edits receive the focused tests and Clippy/fmt checks.

Commit `4ad9e43` (`fix(analyzer): exclude duplicate mapping-pattern key mutations`) contains only the eight owned implementation/test/current-documentation files. Final focused rerun:5passed/0failed in1.20s. Final fmtcheck exit0; final all-target/all-feature Clippy exit0 in1.67s. `git diff --check` passed. Controller documents and the untracked .venv symlink remain outside the commit. Stopped for controller review.

Task review outcome: review485_task read4adf809..4ad9e43 and returned clean, with no actionable findings, deferred items or waivers. Reviewed numeric identities/guards, mapping-local scope, token suppression, dependencies and actual compilation/run tests.
