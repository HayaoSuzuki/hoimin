# Approved acceptance reconciliation

Reviewed the cached approved Issue bodies #971–#979 against their implementation
commits, original task reports and scoped reviews. Numbering below follows each
issue's acceptance-list order, including the unboxed lists in #977/#978.
A listed historical focused pass is not a new Task10 execution. Integrated gates
check the final combined implementation; native Windows/Linux claims remain
limited as noted in the root maintenance record.

## #971: common raw CLI arguments (Task5, `f6e00d71`)

| Condition | Evidence |
| --- | --- |
| 1. Remove duplicate raw field declaration/full transfer | Composed `RunArgs`; no legacy type/alias; source review in [Task5](task-5-report.md) |
| 2. Equivalent common run/plan normalization | `cli_config` shared-option equivalence, core plan/config suites |
| 3. Missing selector/argv and invalid duration/bytes diagnostics | 75 CLI cases preserve exact individual diagnostic categories and validation boundaries |
| 4. Native argv, invalid UTF-8 selectors/paths | Real Unix native-byte parsing tests; literal Windows code-unit tests exist but native Windows execution is unverified |
| 5. Plan excludes session/resume/metrics | Named CLI rejection cases; plan config conversion checks |
| 6. CLI/help/completion/reference checks | 75 CLI + 36 core + 7 generator cases; four real shell completion commands; generator freshness |

Task6 intentionally changes timeout ownership after this syntax-preserving
refactor. Its explicit mixed-invalid diagnostic ruling does not retroactively
weaken Task5's preservation checks.

## #972: fixed analyzer session settings (Task7, `787fc4f3`)

| Condition | Evidence |
| --- | --- |
| 1. No per-file operator/profile/count changes | Constructor's private fixed fields; `AnalyzeFile` only id/target/final_target, obsolete fields rejected |
| 2. Cross-file cap, sequence, empty final target, truncation | 30 `analyzer_handler` cases and bounded positive-limit property |
| 3. New empty store after finalize with same settings | Separate normal/truncated restart cases; separate-handler settings case |
| 4. Cancellation/read/transfer failures preserve ownership | Nonempty pre-transfer retention, post-transfer failure/reuse, detached managed-owner tests in 323 analyzer cases |
| 5. Hierarchy/import roots and lazy I/O | 22 hierarchy cases and `shell_context_construction_performs_no_project_io` |
| 6. Handler/state/oracle adapters execute | 30 handler + 95 machine + 5 effects + 5 actual bounded-discovery Rust adapter cases |

[Task7](task-7-report.md) distinguishes cancelled waiting from worker exit; Task9
adds shared payload leases without changing the managed-spool lifetime contract.

## #973: preflight fingerprint (Task4, `4e99249c`)

| Condition | Evidence |
| --- | --- |
| 1. Remove meaningless constructor/helper branch | `RunState::with_fingerprint` removed; current guide corrected; no alias |
| 2. Fresh BeginSession uses preflight value | Bounded arbitrary-fingerprint characterization compares actual effect payload |
| 3. Resume LoadSession and Fresh fallback reuse that value | Named six-Fresh-reason cases compare both effects |
| 4. Missing enabled-session fingerprint fails | Named Fresh/Resume `MissingFingerprint` assertions |
| 5. Disabled mode and preflight stop | Disabled reaches worker creation without fingerprint; deadline observes no session effects before preflight |
| 6. Existing tests/current API docs | 133 contracts-focused passes, final five preflight cases; guide constructor listing and Task10 session concept |

[Task4](task-4-report.md) records characterization passing before the pure removal;
no manufactured behavioral RED or fingerprint format change is attributed to it.

## #974: shared-inode protection (Task1, `0202878e`)

| Condition | Evidence |
| --- | --- |
| 1. Equal-content hardlink after baseline/reset fails before write/chmod | Named original and sentinel mutation tests inspect `workspace.mutation.shared` |
| 2. Original and external sentinel bytes/permissions stay unchanged | Independent content and permission assertions, including readonly sentinel |
| 3. Independent, readonly and mismatch controls | Existing mutation and integration tests; 12 focused pass/1 platform ignored and 40 integration passes |
| 4. Retained handles, symlink/reparse and cleanup | Single descriptor boundary; shared readonly cleanup avoids external chmod; bounded alias property |
| 5. Platform identity/link information and regressions | Unix descriptor nlink and Windows handle link count; native macOS executed, Windows/Linux runtime unverified |

[Task1](task-1-report.md) records the cleanup ruling: safe unlink may fail rather
than modifying external readonly permissions. This is protection of hoimin's own
writes, not a sandbox or a concurrent hostile-writer guarantee.

## #975: regular-file opening (Task2, `1fb0409b`)

| Condition | Evidence |
| --- | --- |
| 1. Writerless FIFO plan/report rejected with bounded child cleanup | Five child-process cases RED 0/5 then GREEN 5/5; timeout/kill/reap in harness |
| 2. FIFO source unavailable; existing-store protection does not wait | Source freshness and existing-store protected-input FIFO cases |
| 3. Descriptor and 64 MiB initial/growth limits | Shared opened-descriptor check; artifact unit/property tests retain read caps |
| 4. Input symlinks work; store policy separate | Regular-file symlink byte control; store symlink rejection cases |
| 5. Caller diagnostics remain separate | Missing/alias errors and optional input handling remain at callers; 111 focused passes |
| 6. No general I/O timing guarantee | Current developer/review concepts scope `O_NONBLOCK` to FIFO writer waiting |

[Task2](task-2-report.md) preserves the initial pipe-backpressure harness mistake,
file-backed-capture correction and final harness RED against baseline.

## #976: source index (Task3, `bd7463b1`)

| Condition | Evidence |
| --- | --- |
| 1. One borrowed index per read | `BTreeMap` built after header validation; one recorded build |
| 2. Real work counts on many late sources | Reader counter changes 65,792 visits to 257 visits/256 membership queries |
| 3. Source position does not change result/order | Source permutation cases compare returned candidates |
| 4. Missing/hash/duplicate/ID errors and ID-first priority | Ten reader cases plus existing plan header tests; no reordering of candidate validation |
| 5. Review/freshness/joins; no format or syntax change | 257 focused passes/4 existing ignores; review CLI and source/ID joins; unrelated later Task6 version change documented separately |

[Task3](task-3-report.md) retains both original and aligned-feature timing records.
Use `task-3-review-before.jsonl` versus `task-3-review-after-default.jsonl` for the
aligned comparison; elapsed time is not the regression oracle.

## #977: selection before construction (Task8, `8d58c121`)

| Condition | Evidence |
| --- | --- |
| 1. Existing start-line/scope selection before building | Container/condition guards reuse existing selection rules |
| 2. Excluded parent still walks children and validates syntax/depth | Child-selected exact output/work counters; semantic/scope/depth regressions |
| 3. Zero off-selection construction; included/multiline/child controls | Actual builder count/bytes, ten RED work failures then 16 GREEN cases; two bounded 24-case properties |
| 4. Values/order/IDs/dedup/truncation/cancel; integration/oracle | 350 focused passes/3 benchmark ignores, container/condition integrations and bounded-discovery adapter |

[Task8](task-8-report.md) defines generated-byte counters as completed replacement
string lengths, distinct from retained bytes, allocations or RSS.

## #978: retained payload (Task9, `e4e3a1d4` + `4ebed445`)

| Condition | Evidence |
| --- | --- |
| 1. Document owner/counting/cross-producer scope/eviction | Shared 16 MiB RAII owner, lease transfer/drop; current analyzer/usage contract and Task9 accounting review |
| 2. Large fixture does not retain all oversized population | 8/16/32 elements: admissions 5/4/4, builders 6/5/5, charged peaks below limit, release to zero |
| 3. Exact/+1, eviction, producers, empty, cancel; no partial success | 33 named payload cases/two bounded properties; spool partial cleanup, detached-worker reuse and cross-file retention |
| 4. Values/IDs/order/dedup/truncation/2 MiB record cap | Existing discovery/store/candidate/plan tests; explicit budget error may precede selection/record rejection |
| 5. Existing suites and normal overhead | 1,822 focused passes/35 ignores before narrow follow-up; core 63/2; 27 aligned release matches; ten actual charged probes; final integrated gates cover final source |

[Task9](task-9-report.md) records the effect-ID correction, initial arithmetic
mistakes, and the scoped review's subscript early-stop finding. Fix regression:
1 unwanted helper entry/allocation RED, 0 GREEN; 59 covering executions and
required hooks passed. After measurements retain their actual `e4e3a1d4` revision.
The Task10 manifest fixture adjustment preserves lifetime coverage under the
approved resource contract, without changing production accounting.

## #979: discovery timeout ownership (Task6, `cf210e5c`)

| Condition | Evidence |
| --- | --- |
| 1. Direct run rejects option and help omits it | CLI run/root help and parser tests; plan-only option and four completions |
| 2. No runtime/saved/resume timeout residue | Strict typed config tests reject obsolete field; fingerprint schema12 omits timeout; no dummy placeholder |
| 3. Save positive timeout; one deadline across targets | Plan roundtrip and verify inheritance; controlled-clock/shared-deadline tests after review fix |
| 4. No manifest/baseline on timeout; diagnostics/ownership | Plan/verify marker tests and existing cancellation/managed-worker tests |
| 5. Preserve total/baseline/mutant and needed resume conditions | Core limit validation/fingerprint/resume suites; eight focused corrected cases |
| 6. Parsing/config/plan/verify/resume and generated outputs | 755 focused Rust passes/5 ignores; 72 Python; schema8 goldens added with 30 historical files unchanged; generator/completions |
| 7. Explain cooperative versus process deadlines | Usage/compatibility and updated OKF distinguish synchronous I/O/parser limits |

[Task6](task-6-report.md), [initial review](task-6-review.md) and
[fix review](task-6-fix1-review.md) retain the corrected fingerprint/SQLite prose,
finite case names and controlled clock. Actual bounded Lean proofs, corpus
freshness and sensitivity ran for Task6; later Rust adapters execute in the
integrated suites. No aggregate Lean build or native cross-platform proof is claimed.
