# Task 4 integration report

Integration base: `f72134d76d28d87cd728305ed224f298df41af4a`, clean before checks. At dispatch, Task3 I1 and user D1 were open; R7 authorized independent frozen-source integration. During this task, the user explicitly accepted +0.1 ms under D1 and the scoped addendum resolved I1 (details below). Integration tests themselves do not establish timing acceptance. Remote main's later dependency-only PR1024 is outside this branch and these receipts.

## Requirements/design reviews (five distinct passes)

1. Selection and source coordinates: read the combined spec, private CandidateSelection implementation, Task1 report/review and analyzer sources. Admission uses prefix maxima once after parsing; source positions remain separate. Raw-empty All, reversed-only None, inclusive MAX and symbol conjunction remain explicit. Sorting offers phase-boundary cooperative cancellation, with no deadline claim.
2. Immutable workspace ownership: read manifest/copy boundaries and Task2 report/review. Arc stores only entries/directories/bytes, with structural equality and fresh content verification. Worker roots, cleanup authority, charge, detached/pending work and slot ownership remain independent; metadata heap savings imply no disk-charge reduction. Native Windows lifecycle evidence still belongs to final CI.
3. Lazy encoding and validation: read Task3 report/review/fix brief and candidate constructor/validation order. Only location indexes defer; raw/decoded limits, decode/hash and strict rejection order remain eager or unchanged. UTF8/BOM scalar coordinates differ from Latin1 raw-byte validation; successful Unicode still builds twice sequentially, while actual CLI max-live drops 2 to 1. OnceLock adds overhead and context stack grows 160 to 168 bytes.
4. Evidence and unresolved acceptance: retain Task1's full-CLI many-line increase, mixed Task2 filesystem times, all Task3 original/confirmation/R6 observations, exact measured hashes and scripts. R6 small upper +0.067791ms exceeds +0.05ms; tiny passes and requested heap is unchanged. Neither compliance nor causal slowdown follows for small ASCII. D1 has no inferred answer.
5. Integration/catalog scope: read OKF workflow/overview/index and analyzer/selection/runtime/performance concepts. Run required workspace/vendor lint, workspace/contracts, CLI reference, frozen Python and registry gates once, serialized/two jobs; capture exact head/diff, counts and skips. Add spec catalog entry and evidence links without refreshing historical revisions. Audit inherited missing paths/unavailable commits separately from changed references; check reachability, without implying URL/fragment checks.

## Execution

In progress. Required resource environment and per-command bounds live in `/tmp/hoimin-perf-three-integration/run.py`; each completed command records source head, tracked diff hash, argv, environment, elapsed seconds, exit and log hash.

## D1 acceptance update

During integration, the controller delivered the user's explicit choice: 「+0.1 ms以内を許容して採用する（推奨）」. The accepted criterion now permits the existing tiny −0.019834 ms / small +0.067791 ms upper bounds, retaining the no-increase requested whole-plan peak/retained heap criterion. This user decision follows observation; it is not a prespecified +0.1 ms experiment. The D1 independent addendum resolves I1 without changing product source or rerunning measurements. The original +0.05 ms failure, earlier adverse samples, stack 160→168 B and finite-workload/confidence assumptions remain archived. Current docs/spec distinguish this accepted decision from original R6 evidence.

## Implementation self-reviews (five distinct passes)

1. Current versus historical claims: reviewed each new paragraph against exact product sources and per-issue evidence. Current analyzer/selection/runtime concepts describe private ownership and unchanged coordinates/payload/charge; original reports and initial I1/fix verdicts remain untouched. D1 addendum supplies the subsequent acceptance decision.
2. API and scope: docs record only two intentional const removals, private selection/Arc/lazy context changes, existing synchronous cancellation boundaries, and version/schema/proof-input preservation. No product, test, Cargo, workflow or proof change has been made in Task4.
3. Evidence mapping: archive links existing per-issue raw collections instead of duplicating them. It adds concise complete decisive RED/GREEN/hook logs, source-path/log hashes and other log excerpts, baseline receipt, task/review reports, controller rulings and D1. The original measured helpers remain byte-identical; no formatting changed their provenance.
4. Catalog and references: added the combined spec to design catalog with modified-working-tree hash after D1 appendix, retaining read revision f72134d7. Added source entries hash their actual read content; inherited unavailable references remain historical and separately enumerated. Local checker handles YAML numeric revisions as strings and excludes external-source revisions from local Git availability checks.
5. Communication and proportionality: current prose distinguishes operation counts, requested live/peak heap, RSS, logical payload/disk and elapsed process time. It retains Task1's 9.257→11.657ms full-CLI increase, Task2 mixed filesystem times and Task3 two sequential builds/+8B stack. CI/native claims remain controller-owned; no generic speedup or universal non-regression statement was introduced.

## Observed integration environment failure

The first full `uv run --frozen --no-sync pytest` exited 1: 1197 passed, 11 skipped, four resource-guard failures. All four are in unchanged `tests/test_lean_resource_guard.py`: normal-child exit, descendant cleanup, RSS limit, timeout. Guards returned monitor_error 126 instead of expected normal/limit exits. The exact `ps -axo pid=,ppid=,pgid=,rss=` spawn independently raises PermissionError errno1 under the filesystem/process sandbox; this explains monitor_error/zero-RSS results before child workloads. Preserved the full failed Python log and a machine-readable ps permission probe. Reported to controller before any edit; no product change or suppression was made. Awaiting controller's scoped environment retry routing; the serialized runner stopped before performance gates.

Controller authorized the existing four cases to retry outside the sandbox, with no new user question and no product edit. `uv run --frozen --no-sync pytest tests/test_lean_resource_guard.py::test_propagates_normal_child_exit tests/test_lean_resource_guard.py::test_rejects_root_exit_that_leaves_a_descendant tests/test_lean_resource_guard.py::test_terminates_process_group_at_rss_limit tests/test_lean_resource_guard.py::test_terminates_process_group_at_timeout -ra` passed 4/4 in2.36s with process observation available. Exact receipt preserves require_escalated environment distinction and full command/log hash. Python aggregate is1201passed/11skipped across the original full run and four-case retry; it is NOT one all-green full run. Performance gates then started, with the original failed run preserved unchanged.

## Final integration results

All commands ran serialized with two Cargo jobs, shared target, incremental0, Rust test threads2 and the prescribed venv/cache/PATH. Per-command receipts record exact f72134d7 head, tracked diff hash, environment, bounded timeout, exit, elapsed seconds and full-log SHA256. Protected product/test/Cargo/tool/workflow/proof source is unchanged throughout Task4; tracked diffs during later checks contain only this task's docs. No benchmark was rerun or source provenance restyled.

| Check | Result |
| --- | --- |
| Workspace/vendor fmt | Both exit0 |
| Workspace all-target/all-feature clippy; locked vendor clippy | Both exit0, no compiler/linter warning |
| `cargo test --locked --workspace` | 3430 passed / 33 ignored; 179 top-level sections; 2 nested helper summaries excluded; 414.97s |
| Core contracts | 705 passed / 2 ignored; 23.85s |
| CLI contracts | 2228 passed / 28 ignored; 406.02s |
| CLI reference example tests and generated `--check` | 7 passed; check exit0 |
| Frozen Ruff format/lint and ty | All exit0 |
| Frozen full pytest + scoped unsandboxed environment retry | Full run1197 passed/11 skipped/4 environmental failures; retry4 passed; resolved aggregate1201 passed/11 skipped, NOT one all-green full run |
| Registered performance gate | 47/47 active gates passed, zero pending; 34.49s |

Rust counts from different commands overlap and are not summed as unique tests. Every harness-ignored case/reason lives in integration-summary.json. Python skips comprise3 Linux-only Creusot ownership/signal cases and8 real workflow-linter cases because actionlint/shellcheck/zizmor are absent in the preserved environment. python-resolution.json itemizes their source guards/parameters and confirms executable absence; compact pytest output reports module skip counts, not per-node reasons. No test skip or suppression was added. Native Linux/Windows and final CI remain controller-owned.

## Validation self-reviews (five distinct passes)

1. Actual integration outcomes: read each command's exit and final harness summaries; workspace3430/33ignored, corecontracts705/2, CLIcontracts2228/28, reference7, all lint/type checks pass, gate47/47. Count only final outer summaries, excluding2 nested helper summaries in workspace. Keep separate suites overlapping; no unique grand total or false single-green Python run.
2. Failure/skip honesty: inspected four full-pytest resource-guard failures and the exact ps PermissionError; escalated four-case retry passed without source edits. Preserve original failed full log, retry command/log/receipt and both random seeds. Itemize11 original skips from execution modules and unchanged guards/tool availability; retain macOS-only scope and controller CI limits.
3. Evidence/provenance: verify all5/12/9 primary source hashes at Tasks1/2/3 actual commits and original baseline binary SHA unchanged. Keep original per-issue measurements, failed initial+0.05ms result, D1 after-observation decision and final addendum. Protected839 input-file manifest has zero product/test/Cargo/tool/workflow/proof diff from integration head; formal inputs/pin diff from original base is empty.
4. OKF/references: current structure passes for50 pages, all50 reachable;11 added source hashes match. No new missing paths, unavailable local commit refs or footnote findings versus fresh f72134d7 baseline audit. Itemize inherited52 missing-path occurrences/12 target paths,67 local unavailable-commit references/24 commits and14 footnote findings; inherited catalog gaps31 designs/104 reports remain. Combined spec catalog omission fixed. External URLs/fragments and historical content hashes are outside the check.
5. Archive/review boundary: archive full task reports, all45 earlier self-reviews and15 Task4 passes, independent task/fix/D1 reviews, briefs/discovery/issue snapshots, controller R1–R7 costs and D1, concise decisive logs and execution receipts. Index60 passes and verify membership/current hashes before normal commit. No product/Cargo/feature/schema/version/proof/workflow change, push, CI or cleanup performed; controller still owes Task4 and whole-branch review, final review archival and publication/native CI.

## Files and remaining work

Updated analyzer, selection/plan, runtime and performance OKF explanations; development/performance guides; combined spec's explicit D1 decision; design-source catalog. Added docs/maintenance/2026-10-20-perf-three with indexed reports/records/rulings and concise receipts, linking original measurement collections. This is documentation/integration work; new application TDD cases were neither required nor added. Existing meaningful regression gates ran in the required integration sweep.

No product correctness doubt remains from integration. Material evidence limits: finite noisy timing fixtures and post-observation D1 decision, inherited OKF gaps/unavailable references,11 Python platform/tool skips, initial sandbox full-pytest failure with successful scoped retry, and native CI not yet run. Normal commit result will be appended after hooks; no hook bypass is permitted.
