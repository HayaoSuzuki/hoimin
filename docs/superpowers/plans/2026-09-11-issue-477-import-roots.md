# Issue 477 implementation plan

Spec: ../specs/2026-09-11-issue-477-import-roots-design.md

Independent worktree from4adf809; no merge. Controller owns docs/superpowers and docs/knowledge. One architecture-level implementation task owns production/tests/current README/development and schema descriptions if required. No subagents or cargo-mutants. Every Cargo uses CARGO_INCREMENTAL=0 and CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target. Do not run Cargo concurrently with another implementer. CPython3.14.7 available through untracked .venv symlink.

## Plan self-review

1. Follow dependencies: reproduce actual imported path/value first, add config normalization and version framing, then workspace environment plumbing and actual run/plan/verify tests. Keep selection unchanged at every conversion.
2. Boundary coverage: ordered roots affect imports and fingerprints; duplicate normalization, traversal rejection, copied-root availability, old plan/session diagnostics, no-selector rejection and old report decoding each cover a distinct contract.
3. Architecture/resources: one task spans core configuration and CLI/workspace; use capable architecture implementer, no parallel Cargo. Run focused then full suite once after production stabilizes; repeat only affected tests/quality if test-only changes follow. Explicit support limits avoid runtime import-loader work.

## OKF self-review

1. Read architecture's copy contract, selection/plan/verify and session/report compatibility notes against actual implementation. Original source bytes are preserved even when Python imports the wrong path; the new source explains that difference.
2. Add issue-specific source to architecture, selection and session concepts plus design index, with actual spec SHA256; keep historical sources/revisions and audit evidence unchanged.
3. Validate reserved files/frontmatter, source-footnote pairs, links/reachability, source hashes and complete design index including displayed count161. Recheck after any design adjustment.

## Task 1: Add independent worker import roots end to end

Worktree `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-477`, base4adf809. Own necessary Rust code/tests in core and CLI, README.md, docs/development.md, applicable current JSON Schema descriptions. Controller owns all docs/superpowers and docs/knowledge. No delegation. Review spec and `/private/tmp/issue477-preflight-notes.md` before editing; report concrete design concerns and smallest sound alternative before broad deviation.

- [x] Capture old --file path-only `.pth` semantic failure with network-free regular-package/venv fixture: both phases import original file/value5, wrong survived. Keep an unrelated eligible file to prove selection scope.
- [x] Add repeatable run/plan --import-root; dedicated ordered normalized core field and CLI conversions. Project-root-relative dirs, allow `.`, normalize harmless components and stable duplicates, reject absolute/traversal escapes. Import roots alone are not mutation selectors. Validate deserialized normalized run/plan fields, preserve historical run-report default-empty decoding where needed.
- [x] Thread explicit roots separately into workspace environment: worker root, explicit roots, selected source roots, rewritten inherited PYTHONPATH. Retain existing OS-aware split/join/dedup. Validate explicit roots are directories in copied worker before baseline, through existing run-controlled workspace lifecycle; excluded/missing roots must not silently fall back to original `.pth`. No changes to selection/copy exclusions/import loaders/venv/source mutation delivery.
- [x] Persist roots in PlanConfig and restore on verify. Plan schema3→4 with pre-baseline old-version rejection/regenerate message; ranking stays3 on this independent branch. Fingerprint schema6→7, new framed ordered roots field; do not sort precedence away or silently reuse older sessions. Add exact compatibility tests and review extension-friendly normalized_config JSON schemas without gratuitous schema changes.
- [x] Config/environment/fingerprint tests: repeat/order/dedup/rootdot/invalidabsolute/escape, no-selector, forgednormalizedplan, copiedrootunavailable marker, defaults, source-root/PYTHONPATH precedence, change/order fingerprint mismatch, equivalent-normalization equality, old plan/session behavior.
- [x] Actual CLI .pth tests run --file and --line with import roots, logs inside worker baseline5/mutant-1, killed, only intended candidate(s), unchanged originals. Actual plan then CLI verify inherits roots without flag, checks serialized roots/ID/path/status and immutable plan/source. Existing PYTHONPATH workaround remains verified.
- [x] Current user docs include practical src-layout example, narrow selection, ordered precedence/path rules, unavailable copied-root diagnostic, plan regeneration/fingerprint7 new-session cost and bounded path-only `.pth` support. No claim for all editable finders or -E/-I. Existing Lean oracles may verify downstream contracts but cannot prove Python imports; no new Lean required unless meaningful changed semantics justify it.
- [x] Run focused suites then full workspace --all-features, fmt --check and all-target/all-feature Clippy -D warnings. Record logs `/private/tmp/issue477-*.log`, failures and followups accurately, no broad retries without diagnosed reason. Three implementation selfreviews and three test selfreviews with concrete outcomes. Commit only ownedfiles; no push/PR. Return full report at supplied scratch path.

## Implementation review follow-up

Controller interim review found that checking `is_dir()` from command_environment would introduce filesystem reads on the async dispatcher. The implementer confirmed CreateWorker is already an owned blocking stage and will validate copied roots before WorkerCreated in both task and synchronous paths, retaining rejected workspaces in pending_cleanup. The initial environment-stat implementation is superseded. This implements the existing lifecycle requirement; no constructor or protocol compatibility waiver was granted. Follow-up checks: directory rejection before command marker, no successful worker publication, and cleanup ownership retained.

Design/OKF refinement reviews: (1) checked shell CreateWorker classification and WorkspaceTask ownership, (2) checked error cleanup cannot drop unowned created workspace, (3) updated all four source hashes and reran OKF validation.

## Test review follow-up

Controller review found that the first line-selection fixture had no eligible candidate outside the selected line in the same file. The test now adds an unused eligible expression elsewhere in calc.py for the line variant, retains the unrelated eligible file, and asserts exactly one selected mutant. Baseline Exit(0) is explicit. The new assertions and all five import regressions passed.

The first full suite stopped at documentation_contract because the new README command referred to src/pkg/calc.py, absent from the shared documentation fixture. Briefly expanding that fixture caused existing max-mutants1 commands to become incomplete, so the expansion was reverted and README examples use the established src/calc.py path. This preserves extracted-command validation and its existing budgets; the dedicated real `.pth` fixture still uses regular pkg/__init__.py. Focused report_handler21/import_roots5 and final fmt/Clippy passed before the justified full rerun.

## Committed implementation and validation record

Status: DONE (implementation committed; ready for controller task review).
Commit: `63509c8` — `fix(workspace): configure import roots independently of selection`.
Base: `4adf809`; branch: `fix/issue-477-import-roots`.
Worktree: `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-477`.

### Implemented behavior

- Repeatable run/plan `--import-root DIR` has its own ordered field through CLI, RawRunConfig, RunConfig, PlanConfig, fingerprint input, shell setup and WorkspaceHandler. It never contributes mutation selectors or selected source roots.
- Normalize harmless components and stable duplicates, retain first occurrence/order, allow `.`, reject empty/absolute/escaping paths. Normalized run and plan validation rejects forged unnormalized or duplicate fields as well as escapes.
- RunConfig and RawRunConfig retain missing-field default-empty serde compatibility. New PlanConfig requires the field; verify examines schema before deserialization, so real schema3 manifests without the field receive regeneration guidance.
- Plan schema is4; ranking remains3. Fingerprint schema is7; field9 frames list length and each root in precedence order. Different values/order or list boundaries differ, equivalent normalized roots match. Prior fingerprint schema6 cannot resume and receives existing start-new-session guidance.
- Existing WorkspaceHandler constructor and environment helper remain available with default-empty behavior; a dedicated builder carries explicit roots. Environment order is worker root, ordered explicit roots, existing source roots, rewritten inherited PYTHONPATH, with existing OS split/join/dedup behavior.
- Explicit copied directories are validated during owned blocking CreateWorker materialization, before WorkerCreated publication. The synchronous handler equivalent applies the same check. Failure uses `workspace.import_root.unavailable`, retains the created worker in pending_cleanup, and reports the root and copy-exclusion hint. Environment construction stays pure.
- README/development guide document narrow selectors, path/order/copy rules, plan/session compatibility costs, inherited PYTHONPATH workaround, and bounded regular-package path-only .pth support. No Python loader or source/venv/copy policy changes. Current JSON Schema normalized_config is already extensible: description only changed, no report-schema bump.

### Semantic RED and GREEN evidence

- `/private/tmp/issue477-repro.py` creates a network-free `venv --without-pip`, regular `pkg/__init__.py`, original src path-only `.pth`, selected calc.py, and an unrelated eligible other.py. Old CLI `--file` baseline exited0 and logged original calc.py/value5; mutant also logged original/value5, survived, exit1. Raw output: `/private/tmp/issue477-semantic-red.log`; old build: `/private/tmp/issue477-red-build.log`.
- New CLI test initially failed on the unrecognized flag before production edits: `/private/tmp/issue477-test-red.log`.
- `/private/tmp/issue477-green-repro.py` reuses the semantic fixture with explicit src import root: baseline Exit0/value5 and mutant value-1, both worker calc.py, killed, exit0. Raw output: `/private/tmp/issue477-semantic-green.log`.
- Committed actual CLI tests also cover run --file, run --line, actual plan serialization followed by CLI verify --top1 without an import flag, unchanged source and saved-plan bytes, candidate ID/path/status correspondence, and inherited PYTHONPATH control. The line variant has a second eligible expression elsewhere in calc.py and asserts exactly one candidate at line2; other.py is eligible in every case and is never selected.

### Validation

All Cargo commands used `CARGO_INCREMENTAL=0` and `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-451/target`; no concurrent Cargo, additional targets, delegation, cargo-mutants, or Python installer/network. Repository Python is CPython3.14.7 via the existing untracked .venv symlink, excluded from commit.

- Core focused: config_json7, plan_config16, resume_policy20 passed. `/private/tmp/issue477-core-focused-2.log`.
- CLI focused: cli_config57, import_roots5, plan45 (1ignored), session_handler26 (1ignored), workspace_handler32 passed. `/private/tmp/issue477-cli-focused-2.log`.
- Owned blocking/synchronous rejection cleanup test:1passed. `/private/tmp/issue477-lifecycle-focused.log`.
- Strengthened line-scope and documentation contract focused: import_roots5 + report_handler21 passed. `/private/tmp/issue477-docs-line-focused-2.log`.
- Final `cargo test --workspace --all-features`: exit0, **1621passed,0failed,13ignored**,70 result blocks including doc-tests. `/private/tmp/issue477-workspace-all-features-2.log`.
- `cargo fmt --all --check`: exit0, empty `/private/tmp/issue477-fmt-check.log`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit0, `/private/tmp/issue477-clippy-3.log`.
- `git diff --check` and staged diff check: passed. No broad rerun after final full green.

### Diagnosed failures and corrections

1. Initial e2e used unsupported verify --all; corrected to actual --top1. File/line and inherited PYTHONPATH tests already passed on that run. `/private/tmp/issue477-import-green.log`.
2. Excluded-root negative fixture initially excluded its selected calc target, so target resolution correctly stopped before worker creation. Corrected to exclude an independent imports directory while retaining selected calc. `/private/tmp/issue477-cli-focused.log`.
3. Core focused compilation found one additional FingerprintInput property-generator literal; initialized its new field empty. `/private/tmp/issue477-core-focused.log`.
4. Controller lifecycle review identified synchronous directory stat in async worker_process_request. Moved validation from command_environment into existing owned blocking materialization and synchronous handler equivalent; added rejection/no-publication/pending-cleanup test. Initial environment-stat approach is superseded and is not final behavior.
5. Initial full run reached documentation_contract but new README src/pkg/calc.py example was absent from its shared fixture. Attempted fixture expansion added another eligible file and exposed its existing max-mutants1 budget (earlier source example exited4). Chose existing src/calc.py for README examples and restored shared fixture, retaining extracted-command execution and budget. Actual regular-package e2e remains unchanged. `/private/tmp/issue477-workspace-all-features.log`, `/private/tmp/issue477-docs-line-focused.log`; corrected focused/full runs passed.
6. Clippy rejected unused self in command helper and debug path formatting; making command associated exposed unused self in verify helper. Both are associated functions now; Display path formatting fixed. `/private/tmp/issue477-clippy.log`, `/private/tmp/issue477-clippy-2.log`; final Clippy passed.

### Three implementation self-reviews

1. Selection/config correspondence: traced raw CLI conversion through plan round trip and shell builder; explicit roots never enter Selection.sources or satisfy selector requirements. Checked normalized strings rather than Path equality so hidden `.`/`..` components cannot evade normalized-config validation. Defaults preserve old environment behavior.
2. Compatibility/framing: actual plan3→4 and fingerprint6→7 checked against constants and real compatibility paths; ranking3 unchanged. Header rejection precedes PlanConfig serde; run-report default differs deliberately from required new plan field. Ordered field9 frames count and string lengths; no sorting erases import precedence. JSON Schema extension contract permits this optional normalized-run field.
3. Execution ownership: followed shell's CreateWorker blocking classification and owned WorkspaceTask completion route; validation precedes successful publication in both implementations. Rejected materialized workers remain pending for cleanup. New code does not stat directories on async process-dispatch path or change source mutation/copy policies.

### Three test self-reviews

1. Observable Python behavior: tests assert baseline Exit0/value5, mutant value-1/killed, equal worker module paths, selected candidate path/line, immutable source, actual plan candidate ID, persisted roots and unchanged plan bytes. External RED/GREEN logs independently record the mis-import and corrected import behavior; no loader monkeypatch substitutes for Python behavior.
2. Scope/negative controls: unrelated eligible file plus second eligible expression for line variant detect broadening. Existing inherited-PYTHONPATH route and default environment equivalence remain covered. Rootdot/order/dedup/harmless normalization and invalid path/no-selector cases cover distinct boundaries.
3. Failure/compatibility boundaries: missing, excluded and non-directory roots stop before external command marker; blocking and sync unit routes reject WorkerCreated, retain one pending worker, and remove it on close. Forged normalized plans and real old-schema/no-field plans reject before baseline. Fingerprint tests distinguish values/order/framing but equate normalized aliases, and session test explicitly rejects old schema6. Final workspace suite exercises downstream lifecycle/session/report oracles; none is claimed as a proof of Python imports.

### Ownership and limitations

Only16owned production/test/current-doc/schema-description files committed. Controller-owned docs/superpowers and docs/knowledge remain uncommitted by this task; .venv remains untracked. No push or PR. No unresolved implementation concerns. Support remains limited to Python honoring PYTHONPATH with ordinary path-only .pth/regular package imports; custom editable finders/loaders and -E/-I are outside the contract. Controller should persist this scratch report before cleanup and perform task/final review.

## PR self-review

1. Compared4adf809..63509c8 against the issue: dedicated ordered roots preserve selected files/lines, actual `.pth` execution imports worker bytes, and the chosen compatibility versions are explicit. Constructor and existing environment-helper defaults remain; no async filesystem-read waiver remains.
2. Reconciled raw logs independently:70 result blocks,1621 passed/0 failed/13 ignored; empty fmt log and final Clippy completion. Initial documentation-fixture failure and test-style/fixture corrections are disclosed, and final full validation uses the corrected code/tests.
3. Checked PR body, Closes #477, bounded support statement, no-merge branch scope, all source hashes and OKF links. Design index contains161 sources matching161 specs/displayed count. IDE MCP cannot access hoimin because that project is not open; passed native checks are macOS and do not claim Windows execution.

## Independent task review

Reviewer review477_task checked4adf809..63509c8 for spec compliance and architecture/code quality, including the existing plan version-validation boundary. Clean: no actionable findings. Ordered roots, framing, normalized persistence, worker publication/cleanup and public CLI correspondence satisfy the task. Passed tests were not repeated. No substantive rulings or deferred findings.
