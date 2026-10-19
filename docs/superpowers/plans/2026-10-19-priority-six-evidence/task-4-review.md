### Spec Compliance

- ❌ Issues found: the Windows-only cleanup test migration introduces a compile error at `crates/hoimin-cli/src/workspace/owned.rs:1842`. The reviewed production changes otherwise match Task 4 and R9: janitor-owned counts, bounded diagnostics, runtime verbosity, required-nullable `disk.janitor`, schema 9 and package floor 0.14.0.
- ⚠️ Native Linux/Windows execution remains pending CI. The Windows finding below is established by source types, not a claimed native compiler run. Full integration and Task 5's completion of unpublished schema 9 remain controller-owned; neither is a missing Task 4 feature.
- Review scope: package `review-8720b41a..b199e83d.diff`, base `8720b41a0d08f375d98bda7cfa6202be779f3226`, head `b199e83d30f50e591fb7e75dd5e24cb13e0dbe36`, plus the binding brief/report and relevant approved design semantics. No product/index changes, git commands, builds, tests or subagents were used.

### Strengths

- `crates/hoimin-cli/src/workspace/owned/janitor.rs:33`: one ledger record follows each inspected root through pending rounds. `removal` consumes operation progress into an exclusive outcome; `finish` derives root/reason totals once. Acknowledged root deletion takes precedence over later diagnostics, while absence does not invent a deletion (`:52`, `:90`). The four-category property test uses an independent frequency oracle (`:202`).
- `crates/hoimin-cli/src/workspace/owned.rs:7603`, `:7665`, `:8114`: actual successful marker and root deletion boundaries update capability-owned progress before later verification can fail. That progress survives nested errors and is consumed by owner finalization (`:5441`). Windows pinned lease removal does not double-count the generic marker path (`:7575`); slice counters remain scheduling inputs rather than a second factual aggregate.
- `crates/hoimin-cli/src/workspace/owned.rs:5559`: selection, eligibility, claim/unlock ordering, optional-marker fallback and deferred-only round scheduling remain structurally preserved in the diff. Typed causes replace collapsed boolean/string-only reporting without adding new deletion eligibility. `reclaim_removal` keeps its existing complete/deferred/error decision (`:6103`), and `finish_owner_removal` preserves CleanupDisposition and capability-restoration branches (`:5443`).
- `crates/hoimin-cli/src/workspace/owned/janitor.rs:109`: selected-but-uninspected and observed selection omissions are derived separately. Selection retains its bounded name sets and records completed enumeration independently from cleanup (`crates/hoimin-cli/src/workspace/owned.rs:6350`). Exact 256/257 and controlled cleanup deadline cases test distinct observed/selected/inspected facts (`crates/hoimin-cli/src/workspace/owned/janitor.rs:226`, `:258`). Human output explicitly labels incomplete scan tails unknown (`crates/hoimin-cli/src/report/human.rs:261`).
- `crates/hoimin-cli/src/workspace/owned/janitor.rs:64`: the single detail store caps retained records at 256, prioritizes late issues over normal holds, and counts capacity loss separately from truncation. The sole variable string payload combines target/cause in `message` and passes through the existing 4096-byte UTF-8 truncator; there are no separate uncapped path/cause strings. Default suppression happens after factual aggregation (`crates/hoimin-core/src/report/janitor.rs:105`).
- `crates/hoimin-cli/src/report/mod.rs:122`: one projection runs before format dispatch; delivery preserves the runtime setting. CLI verbosity is extracted before checked-config construction (`crates/hoimin-cli/src/lib.rs:72`), and saved config/plan/fingerprint structures are not extended. Failed/deferred current cleanup, paths and measurement causes are rendered independently of normal janitor holds (`crates/hoimin-cli/src/report/human.rs:191`). Six generated presentation cases exercise one format/verbosity combination apiece with independent count/visibility expectations (`crates/hoimin-cli/tests/report_handler.rs:2855`).
- `crates/hoimin-core/src/report.rs:28`: custom field deserialization distinguishes absent `janitor` from explicit null. Tests cover both at the actual shared artifact reader and published-schema boundaries (`crates/hoimin-cli/src/artifacts/run_report.rs:526`, `crates/hoimin-cli/tests/report_handler.rs:2963`). Current fixtures and both schemas use 9; old v8 fixtures remain historical rejection inputs. The diff consistently raises workspace/Python/lockfile local package versions to 0.14.0 and provides the compatibility notice (`docs/compatibility.md:7`).

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- **Windows test target no longer compiles — `crates/hoimin-cli/src/workspace/owned.rs:1842`.** `child_handles_close_before_quiescence_is_published` is guarded by `#[cfg(windows)]` (`:1808`). Its `while_closing` value comes from owner `root.cleanup`, whose `CleanupRecord.evidence` remains `CleanupEvidence` (`:4186`) and whose `details` remains `Vec<String>` (`:4207`). The change to `detail.message.contains(...)` therefore accesses a nonexistent field on `&String`. Native Windows lib tests and all-target validation will fail to compile before exercising cleanup. Restore `detail.contains("live child handle")` in this owner-cleanup assertion; only janitor details use `JanitorDetail.message`. The macOS passes cannot detect this cfg-gated error. Validate the corrected test in the scheduled native Windows checks.

#### Minor (Nice to Have)

- None raised.

### Evidence and Review Limits

- Existing GREEN evidence inspected: `/tmp/hoimin-priority-six-task4-cleanup-green.log:80` (73 passed), `owned-final.log:94` (73 passed, three existing subprocess helpers ignored), `presentation-final.log:15` (8 passed), `reader.log:9` (2 passed), `core.log:10`/`:33`/`:71` (2/17/32), `report-cli.log:25`/`:116`/`:263` (16/84/141), `python-schema.log:2` (63 passed), and `commit.log:1` (four mandatory format/clippy hooks passed). No warnings/errors appeared in these successful logs. The RED required-nullable and owner-progress logs show the claimed behavioral failures; they are not outstanding failures.
- Changed-file context was reopened only to resolve cut-off relevant bodies/types: the Windows test plus `CleanupRecord`/`CleanupEvidence`; `finish_owner_removal`; `reclaim_removal`; single-entry/slice traversal; marker removal and the deletion helpers. One small diff segment was recovered after tool-output truncation. Named cross-boundary risks checked: (1) acknowledgement lost inside unchanged Windows helpers—`workspace/root/windows.rs:167` and `:188` return the delete-disposition result directly, with no fallible post-ack work; (2) changed ready-marker wrapper accidentally adding syscalls—`owned.rs:7119` confirms the old bounded reader already delegates through the same open/read helpers. These checks did not reveal additional defects.
- Source review and controlled/synthetic boundary tests do not establish every native iterator/coordinator failure or Windows delete-pending interleaving. The implementer's report makes that limitation explicit. No broader suite was rerun.

### Assessment

**Task quality: Needs fixes.**

**Reasoning:** Production ownership and evidence projection are coherent and the focused macOS evidence is useful. The concrete Windows-only type error blocks this gate until corrected; native behavior remains for scheduled CI after that correction.
