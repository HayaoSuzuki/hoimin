# Priority issues #1003, #1002, #1004, #1015, #1010 and #1005

> Execute sequentially with the subagent-driven-development skill. Existing issue specifications and the user's explicit request authorize this work through PR creation. This plan records implementation decisions; it does not claim a separate user approval.

**Goal:** Close the six selected source-reading, terminal-output, configuration, cleanup-reporting, result-projection and graph-analysis defects with production changes, behavioral evidence and a reviewable PR.

**Architecture:** Preserve capability-based filesystem access, validated immutable execution inputs, structured janitor ownership, and outcome-owned classification. Project human/wire output at the existing boundaries. Replace repeated import reachability with shared graph analysis; keep candidate visibility conservative.

**Tech stack:** Rust workspace, Tokio, cap-std/cap-primitives, Serde, blake3, proptest, existing Python/JSON-schema tooling and CI.

**Spec:** The full issue bodies and 2026-10-19 triage sections at https://github.com/HayaoSuzuki/hoimin/issues/1003, /1002, /1004, /1015, /1010 and /1005 are binding (read at base `1e6200d71237c6e445f337babf02f5f0354b610f`). The controller keeps a fetched snapshot in the plan-owned SDD directory. Issue acceptance conditions win over this execution plan. No other backlog issue is included.

## Global Constraints

- Work on the issue-named isolated branch from main `1e6200d71237c6e445f337babf02f5f0354b610f`; do not merge, release or edit unrelated worktrees/user files.
- No backward compatibility adapters, unchecked compatibility constructors or silent schema mixtures.
- At least five distinct self-review passes at each stage: requirements/design, implementation, and validation. Record the five lenses and concrete findings/checks for each stage in the task report; do not merely repeat an approval sentence.
- Tests use Arrange/Act/Assert, one behavioral case per test function or generated parameter case, parameterization for cases of the same contract and property tests for broad invariants. Derive expectations independently; exercise real boundaries. Record meaningful RED then GREEN evidence for changed behavior.
- Implementers work sequentially and never spawn subagents. Each task ends in a commit and a task-scoped independent review before its successor begins. Do not change the next task's behavior early.
- Use `CARGO_BUILD_JOBS=2`, `CARGO_TARGET_DIR=/tmp/hoimin-priority-six-target`, and bounded test threads (`--test-threads=2` for process/filesystem suites). Coordinate builds: only one Cargo operation at a time. Run focused relevant suites per task and full integration checks once at the end; repeat only for changed code/failures.
- Keep task-owned logs under `/tmp/hoimin-priority-six-*` or the plan-owned SDD directory. Watch disk/memory; clean only task-owned stale build outputs. Do not delete other worktrees or targets.
- Preserve #999 production Creusot integration. Do not expand formal-proof scope, modify #806, add new commands or implement other backlog features.
- One combined report/event schema bump from 8 to 9 covers Tasks 4 and 5; package floor becomes 0.14.0 (latest released version read: v0.13.5). Task 4 establishes schema 9, Task 5 completes it before PR publication. Plans, sessions, fingerprints, candidate identities and exit policy stay unchanged unless evidence requires an explicitly recorded ruling.
- Update user guidance and relevant OKF concepts with evidence limits; final integration owns combined catalogs, compatibility notice and release-floor consistency. Never refresh historical evidence as if rerun.
- Ask the controller about material ambiguities; routine choices consistent with these constraints are authorized. Report uncertainty honestly; native macOS tests do not establish native Linux/Windows success.

## Review Focus

1. Source race paths must reach the no-follow, nonblocking, same-descriptor regular-file check. Task 1 owns deterministic swap tests and allocation checks.
2. Escaping must happen only at human presentation boundaries and keep each candidate line self-contained. Tasks 2 and 5 own control-character/Unicode and JSON preservation cases.
3. Mutable raw settings must never impersonate checked execution state; strict review must fail before filesystem work while progress remains opaque. Task 3 owns negative-boundary and compile-fail/property checks.
4. Cleanup counts and derived status must not become second authorities for run classification. Tasks 4 and 5 own failure/omission/verbose consistency and tampered-wire rejection tests.
5. Shared graph analysis must retain non-cycle prefixes, lazy imports and cancellation, with bounded work and measured memory. Task 6 owns graph reference cases, operation counters and release measurements.

## Task 1: Safe source hashing and candidate reads (#1003)

**Files:** `crates/hoimin-cli/src/plan.rs`, `plan/validation_tests.rs` and relevant plan tests; `workspace/reader.rs`; existing `workspace/root.rs` hooks; a focused `plan/source.rs` and heap integration test may be introduced if they isolate source acquisition. Consult `docs/knowledge/design/selection-plan-verify.md` and its relevant source contracts.

**Required behavior:** Both source_records hashing and requested-candidate full-byte validation use PortableFileReader/WorkerRoot capability-relative acquisition. Hash fixed-size chunks from the safely opened descriptor; retain full bytes only where candidate validation needs them. Reuse one reader per operation/root rather than reopening the root for every file. Preserve current hashes, IDs, stale detection, source ordering and useful error paths. No global filesystem hard-deadline claim.

**Work:**
1. Read the exact issue snapshot and current safe-reader/error/test-hook APIs. Record five requirements/design review passes (path authority, descriptor race, data retention, diagnostic ordering, platform scope).
2. Add failing tests for deterministic ordinary-file-to-FIFO and symlink swaps after resolution, normal/stale candidate behavior and hash heap independence. Use test-only synchronization, not sleeps as correctness evidence. Exercise both hashing and validation consumers where practical.
3. Route both consumers through the shared boundary; allow a narrow reader hash method. Keep blocking filesystem work off async executor threads using the established spawn_blocking convention. Share failure mapping without erasing context. No alternate unsafe fallback.
4. Perform five implementation review passes (all target reads, no-follow/regular-file semantics, error priority, allocation ownership, async/cancel boundaries). Fix findings.
5. Run focused plan/verify, reader/root safety and allocation tests, formatting and targeted clippy as appropriate. Record RED/GREEN logs, small/large source heap measurements, five validation reviews (test sensitivity, one-case AAA, boundary completeness, platform limitations, resource use). Commit.

**Report:** `task-1-report.md` adjacent to task brief; status, commits, changed behavior/files, commands/results, self-reviews, risks and any controller rulings. Keep final message short.

## Task 2: Shared human-output escaping (#1002)

**Files:** `crates/hoimin-cli/src/plan/preview.rs`, `report/human.rs`, human diagnostics/review entry points as required; add one focused presentation helper module and register it in `lib.rs` if useful. Existing baseline output policy is separate streamed child-output sanitization, not a reason to rewrite child capture. Relevant CLI tests.

**Required behavior:** Escape external paths/strings at human-output boundaries, including ESC, CR, LF and TAB, while keeping ordinary Unicode readable and every preview candidate on one line. No raw input-origin terminal controls. Do not alter logical paths, candidate IDs, or JSON/JSONL string values. Dry-run must never execute baseline/test commands. Inspect report diagnostics and reuse the same policy rather than copying sanitizer logic per command. Leave a reusable helper for Task 5.

**Work:**
1. Read #1002 snapshot; review requirements five times across preview/run/review/diagnostic boundaries, Unicode, machine formats, dry-run side effects and helper ownership.
2. Add RED parameterized cases for control-bearing filenames and Unicode, public dry-run marker nonexecution, and machine-output logical value preservation. Property-test escaped strings contain no terminal controls/newlines.
3. Implement the shared Display/writer projection with explicit escaping of control characters. Retain intended renderer structure without escaping framework-generated separators. Cover the same external string policy across discovered human paths; keep unrelated formatting stable.
4. Five implementation reviews: coverage of external fields, no double escaping, ASCII/Unicode semantics, logical-data isolation, no duplicate helper or oversized abstraction.
5. Focused preview/human/review/diagnostic tests, fmt and relevant clippy; five validation passes for regression sensitivity, AAA/cases, machine invariants, native filename limitations and false safety claims. Commit and report.

**Report:** `task-2-report.md` adjacent to brief, same report contract as Task 1. Record the helper API for Task 5.

## Task 3: Checked normalized configuration (#1004)

**Files:** `crates/hoimin-core/src/config.rs`, machine/fingerprint consumers, `crates/hoimin-cli/src/shell.rs`, `plan.rs`, `artifacts/run_report.rs`, and affected callers/tests. Read `docs/knowledge/design/selection-plan-verify.md`, `session-report.md`, environment-state and VerifiedPlan contracts before choosing the type boundary.

**Required behavior:** A successfully constructed/accepted execution configuration guarantees existing semantic validation and cannot be made invalid by unchecked public field mutation. Strict review deserialization and public execution preparation reject malformed selection/operators/argv/limits/import roots before filesystem fingerprinting or effects. Preserve request-to-snapshot environment typing, valid run/plan/verify/review, duplicate-key rejection and progress's deliberate opaque normalized_config contract. No blanket getter/setter framework or unchecked constructor.

**Architecture decision:** Prefer a small checked immutable wrapper around construction data or private-field checked config with a compact read-only view. Check every construction/deserialization boundary once using the shared validation rules. Ownership of raw construction data must be distinct from checked execution state; no DerefMut or mutable escape. If choosing between materially different public type boundaries, explain the proposed boundary to the controller before large edits; public Rust API compatibility is not required.

**Work:**
1. Read #1004 snapshot, map raw/plan/environment/snapshot/execution transitions. Five requirements/design reviews: entry-point completeness, immutability, validation ordering, duplicate keys/opaque progress, minimal API surface.
2. RED public-reader/preparation tests for jobs 257, jobs 65/max_processes 64, empty required selector/operator/argv and import_roots ../escape; separate cases and use fixtures proving I/O did not precede rejection. Add compile-fail coverage for post-check mutation and properties for valid construction/serde round trips as appropriate.
3. Implement checked construction and wire acceptance without retaining a public bypass. Adapt production/test callers explicitly, preserving source/fingerprint preparation ownership. Never validate separately in every handler.
4. Five implementation reviews: unchecked construction search, post-check writes, environment invariants, wire error/duplicate handling, cross-crate API complexity.
5. Run affected core config/machine/fingerprint and CLI run/plan/verify/review/progress tests, doctests, fmt and targeted lint. Five validation passes: negative branches, independent fixtures, mutation compile fail, valid paths, opaque progress. Commit and report; retain existing report schema until Task 4.

**Report:** `task-3-report.md` adjacent to brief; include a concise raw-to-checked transition map and migration details for downstream tasks.

## Task 4: Structured janitor summary and verbose retention details (#1015)

**Files:** `crates/hoimin-cli/src/workspace/owned.rs`, cleanup evidence/report projection, core report/diagnostic types and CLI verbosity plumbing as needed; report schemas and fixtures; cleanup tests and user guidance. Consult `docs/knowledge/design/runtime-lifecycle.md` and session/report contracts.

**Required behavior:** Janitor owns structured counts: inspected roots, reclaimed roots, preserved roots by reason, actually deleted items, scan/detail omissions. Keep roots/items distinct and never invent total remaining roots/freed bytes. Default JSON/JSONL/human is summary-centered; ordinary young/active-lease/retain root detail appears only under --verbose. Failure/measurement failure/current-run incomplete cleanup retains useful path/cause by default. Existing root-selection/detail count and string-length caps remain; verbose reports omission. Partial scans/deletions/deadlines are represented truthfully. Verbosity never changes cleanup verdict or exit code. No diagnostic-string parsing.

**Work:**
1. Read #1015 snapshot and existing ownership/detail limits. Five requirements/design reviews: count definitions, hold vs failure classification, scan uncertainty, projection ownership, verbosity independent of effects.
2. RED cases for many normal holds, mixed holds/errors, 256-root selection limit, detail/count truncation, deadline and partial-delete failure. Parameterize default/verbose output comparisons for summary/classification/exit equality.
3. Add typed reasons/results and a bounded summary projection reused by all output formats. Preserve cleanup eligibility/deletion policy. Avoid creating separate mutable counters that drift from final evidence; make aggregation responsibility explicit.
4. Bump report/event schema 8 to 9 once, update strict readers/schema/goldens as needed and package floor to 0.14.0 consistently in Cargo.toml, pyproject.toml, Cargo.lock, uv.lock and fuzz/Cargo.lock. Preserve old fixtures only as rejection tests. Task 5 adds status to the same unpublished schema 9. Add an explicit compatibility notice and useful user description now; final integration reconciles combined docs.
5. Five implementation reviews: reason completeness, bounded retention, partial work counters, default diagnostic visibility, wire/default/verbose agreement. Run focused cleanup/report/schema/CLI checks and five validation reviews (behavior sensitivity, one-case AAA, default/verbose invariants, skipped-work honesty, resource limits). Commit and report.

**Report:** `task-4-report.md` adjacent to brief; include precise field semantics, limits, schema footprint and nonclaims.

## Task 5: Derived status in reports and complete human result lines (#1010)

**Files:** `crates/hoimin-core/src/report.rs`, outcome projection, `crates/hoimin-cli/src/report/human.rs`, common report readers, report schemas/goldens/tests and user/skill documentation in the repository. Use Task 2 escaping helper and Task 4 unpublished report/event schema 9.

**Required behavior:** JSON mutants[] and JSONL mutant_finished include status derived by the existing MutationOutcome::status(): killed, survived, timeout, out_of_memory, process_limit, error, not_run. Do not store a second mutable status in execution/persistence or classify again. Strict schema-9 readers reject status/outcome mismatch with exit 2. Summary/progress/resume still derive from outcome. Human result lines contain ID, location, operator, original→replacement and verdict together, escaped to one line under parallel runs. No report command, text alias or exit-5 policy change.

**Work:**
1. Read #1010 snapshot. Five requirements/design reviews: canonical classifier, projection-only state, deserialize consistency, self-contained human row, schemas/readers/consumer docs.
2. RED tests with each outcome, including Exit(1)+close_timed_out=error, cancellation/reuse/not_run, tampered status on JSON and JSONL, and control-bearing human records. Use literal expected classifications and generated parameter cases; verify missing/extra/contradictory wire data fails as intended.
3. Implement computed serialization and validated deserialization at the wire boundary without a separately mutable field. Reuse canonical classifier and escaping helper. Update current schema-9 schemas/goldens/reader fixtures; do not bump to 10 before publication.
4. Five implementation reviews: all mutant writer paths including spool/reused, reader duplicate/mismatch behavior, summary/resume authority, human line fields, no legacy format fallback.
5. Run focused outcome/report/progress/review/spool/CLI and JSON-schema validation. Five validation reviews: close timeout sensitivity, all variants, AAA/parameterization, real public exit 2, parallel row readability. Commit and report.

**Report:** `task-5-report.md` adjacent to brief; document schema changes and reused helper/type APIs.

## Task 6: Shared eager-import graph analysis (#1005)

**Files:** `crates/hoimin-python/src/analyzer/exception_hierarchy.rs` and a focused graph module if warranted, hierarchy test/property/performance fixtures, minimal development measurement notes. Do not mix #1006 line selection or #1008 visibility indexing into this work. Consult analyzer OKF contract and current hierarchy limits/cancellation.

**User-confirmed depth policy (Q1, 2026-10-19):** Use a name-order-independent conservative SCC-weighted bound, saturated at 256; keep all previous exclusions and allow additional exclusions. For SCC C, H(C) = max(cyclic(C) ? size(C) : 0, max(C→D) [size(C) + H(D)]). Reject acyclic modules at H >= 256 and cycle members unconditionally. Small-cycle prefixes remain eligible; document additional exclusions. The separate class ancestry limit is unchanged.

**Required behavior:** Build the eager-import graph once; replace per-module whole reachable traversal with shared SCC/cycle analysis and shared depth-limit reasoning. Mark cycle members, not every prefix reaching a cycle. Explicitly preserve conservative candidate/diagnostic/visibility behavior for DAG, self cycle, multi-node cycle, diamond, noncycle prefix, lazy imports, dynamic modules and depth boundaries. Keep cancellation checks during construction and traversal. Operation counts and release time/heap measurements demonstrate improvement; no timing-only regression assertions.

**Work:**
1. Read #1005 snapshot, map exact existing MAX_STEPS behavior and graph bounds. Five requirements/design reviews: cycle membership, depth rule, dynamic/lazy edges, cancellation, work/storage complexity. Present any observed order-dependent old depth behavior and proposed deterministic conservative rule to the controller before implementation; do not silently overreject cycle prefixes or relax the bound.
2. Add RED graph reference/property cases, deterministic cancel and operation-count regression. Use independently defined/reference expected sets on small graphs and literal boundary cases. Add the issue's three-layer N=16/32/64/128 DAG fixture and same-size missing-import control for release measurements.
3. Implement iterative/bounded graph analysis with shared results; avoid recursion proportional to module count and per-start graph searches. Keep the public candidate semantics under ordinary cases and state any necessary depth-rule ruling explicitly.
4. Five implementation reviews: node/edge identity, SCC correctness, depth/fail-closed semantics, cancel every bounded step, allocations and release cleanup.
5. Run hierarchy-focused CLI/python tests and properties, operation counts, release before/after/control time and heap measurements with fixed inputs and repeated runs. Build the baseline artifact without creating parallel huge targets (use a task-owned temporary source/target arrangement or preserve binary before changing code). Five validation reviews: reference independence, no false-cycle prefix, depth limits, measurement methodology, scope/nonclaims. Commit and report.

**Report:** `task-6-report.md` adjacent to brief; exact workload, environment, timings/counts/heap and limitations. Controller owns any policy ruling.

## Task 7: Integration, documentation and PR readiness

**Files:** Only necessary cross-task integration, current user/developer/compatibility docs, repository skills, relevant existing OKF concepts and source catalogs; append a durable execution/review record beside this plan (plans are outside source catalogs). No unrelated feature work.

**Work:**
1. Read completed task reports, ledger rulings and deferred findings. Five requirements reviews against every issue acceptance condition and the shared-interface table.
2. Reconcile report/event schema 9, version floor 0.14.0, old-format rejection notices, helper/API documentation and source evidence metadata. Keep plan/session/fingerprint versions unless an approved ruling changed them. Regenerate CLI reference only if definitions/help changed. Record native test coverage versus pending CI targets.
3. Run formatting, workspace clippy all targets/all features, workspace Rust tests, core/CLI contracts suites and impacted Python/JSON-schema/CLI-reference/compatibility checks using a frozen task-owned uv environment. Validate the updated OKF structure/references/content. Preserve the Creusot CI configuration and ensure source-only crate changes did not invalidate the proof scope. Do not rerun proof tools solely because unrelated Rust files changed.
4. Address integration failures through their owning implementer with focused covering tests. Five implementation review passes for type interfaces, formatting contracts, test fixtures, docs/schema coherence and resource/artifact ownership.
5. Record five validation reviews: full-check logs, requirement evidence, test hygiene, unverified platform statements, remaining risks. Commit integration changes. Controller dispatches one final whole-branch review and at most one final fix wave with scoped re-review.
6. Controller prepares the PR body from final behavior/test evidence using the repository template, pushes the issue-named branch and creates the PR referencing all six issues. Do not merge. Check actual branch/ref/PR state and CI results; repair justified failures within scope. Archive the plan-owned report/ledger evidence before cleaning its temporary workspace; remove only task-owned unnecessary build artifacts.

**Report:** `task-7-report.md` adjacent to brief; exact successful and pending checks, docs/OKF consulted/updated, remaining limitations, final artifact paths.
