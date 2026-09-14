# Issue 490 boundary contracts: evidence and reviews

Issue: https://github.com/tokyogas-tech/hoimin/issues/490.
Base: `7d5bdc5a643add4ba55e8541fa59a069d0ff5c5e` (enhancement/issue-467,
PR #538). Platform: macOS, repository Python environment, dedicated Cargo
target; debug/incremental disabled and two build jobs. No production Rust
behavior changed. This registry executes tests; source references alone do
not constitute test results.

## Actual outcomes

The final registry runs 10 strict entries and retains three unexecuted report
entries. The reader invokes the actual CLI on 282 existing Lean-generated
inputs in JSON v2, JSON v3 and JSONL v3: all 846 observations match. The full
reader oracle target has four passing tests, including the historical v2/v3
adapter and the minimal contradiction witness. The session oracle exercises
existing Lean-generated schedules against actual SQLite; the resource-policy
fixture additionally combines real CLI, session/resume, best-effort backend
and JSON/JSONL output. Independent literal source spans cover normal overflow
abort versus retained partial plan/verify execution. Four preparation stages
observe errors before the analyzer deadline and before baseline execution.

Ten runner tests pass and check zero-test success, typed semantic/infrastructure failures,
report preservation, duplicate/unknown rows and inherited corpus filters.
The final strict invocation deliberately inherits `HOIMIN_BOUNDARY_CASE` and
an invalid `HOIMIN_SESSION_ORACLE_CASE`; the runner strips both before spawn.
This checks that a minimal witness cannot silently replace the full corpus.

Machine-readable observations and guarded Lean resource measurements are in
[the verification artifact](2026-09-14-issue-490-boundary-contracts-verification.json).
Runtime logs are local temporary evidence, and CI uploads the complete logs
and strict/report JSON. This report does not claim a completed CI run.

## Three self-review passes at each stage

These are separate reviews by the implementing agent, not human approvals.

| Stage | Pass 1 | Pass 2 | Pass 3 |
| --- | --- | --- | --- |
| OKF | Read the workflow, development, boundary audit and progress-input contract; retained historical evidence counts. | Classified the executable cross-boundary registry as a separate audit concept; linked the existing reader/session contracts instead of duplicating their claims. | Checked source provenance, footnotes, concept reachability and complete design/report indexes; kept current observations separate from native gaps. |
| Design | Chose the existing Lean result corpus as independent expectations; rejected generating expected dispositions from the reader. | Distinguished JSON projection from a JSONL lifecycle, requiring valid starts without inventing a missing baseline; actual corpus permits all 282 encodings. | Mapped all six boundaries to premises, model, production settings, public observations, evidence and mode. Parent approved bounded scope with native and preparation-cancellation gaps. |
| Plan | Ordered corpus shape, runner classification, actual replay and final artifacts after Lean freshness/sensitivity. | Added literal overflow fixture so plan/verify partial execution is not compared against ordinary-run abort as if identical. | Reused real SQLite session and backend fixtures; serial guarded Lean and dedicated Cargo builds bound cost without expanding model state spaces. |
| Implementation | Shape test exposed an incorrect assumption that existing missing-baseline corpus rows contain mutants; inspected the four rows, found empty results, corrected fixture applicability count without changing expected semantics. | Classifier rejects zero executed tests and nested mismatch/infrastructure/unexecuted observations even if the outer process exits zero; report rejects duplicate and unknown-status rows. | Parent found inherited minimal-case filters could narrow strict coverage. Removed both reader and session filters, retained PATH, added a regression and replayed with hostile inherited filters. |
| Verification | Baseline reader witness passed; initial runner test failed before its module existed. Four preparation stages initially hit a compile error then an unsupported APFS filename fixture, both recorded as failed attempts. Replaced only the copy fixture with an unreadable file and an explicit infrastructure marker. | Root Lean sources matched but cached oleans were stale; rejected that evidence and rebuilt eight unchanged modules in a private cache. Fresh proof imports, both corpus freshness checks and both sensitivity checks passed under bounds. | Python mutation first stopped at the 10 GiB reserve, then selector mistakes failed before verification. After space recovery and correcting source-root/module syntax, a fresh plan/verify killed the selected equality mutant. Cleanup traps removed each exact temporary directory. |
| PR preparation | Compared issue acceptance to actual registry evidence; added existing Lean SQLite consumer explicitly and preserved three unexecuted rows. | Reviewed CI ordering and report-on-failure behavior; proof-gate failure prevents dependent job execution, while adapter failure inside the job yields all-case unexecuted reporting. No native or end-to-end cancellation claim added. | Checked formatting, focused Rust/Python checks, final hashes and changed-file scope. PR targets enhancement/issue-467 and states dependency #538; local environment link is excluded. |

## Lean provenance and bounds

No Lean source or corpus changed. Source bytes were compared with the root
checkout at `8b33167a049e3cae0fc05e96ccf2253c660b7023` against this
worktree at `7d5bdc5a643add4ba55e8541fa59a069d0ff5c5e` before consulting its cached imports; ten
compared source modules matched. That did not establish cache freshness:
ProgressInputProofs/freshness failed against stale root oleans. Root `.lake`
was never modified. The first guarded command also failed with
`monitor_error` because sandbox process observation was unavailable; that is
infrastructure failure, not proof evidence.

Rebuilt these eight unchanged modules serially in a private cache:
MutationScoreExitPolicyModel, ProgressDecisionModel, ProgressInputModel,
ProgressInputProofs, ProgressInputCases, ReportSequenceModel,
ReportSequenceProofs, ReportSequenceCases. Working directory was
`formal/HoiminOracle`, pinned Lean 4.32.2, with
`LEAN_PATH=/private/tmp/issue-490-lean-cache`. Each guarded command used
`lean -j1 -DElab.async=false -o <private-cache>/HoiminOracle/<module>.olean
HoiminOracle/<module>.lean`, timeout 20 s, RSS limit 768 MiB, 250 ms sampling.
Then both `ProgressInputAuditMain.lean` and `ReportSequenceAuditMain.lean`
ran with `--check corpus/<progress-input|report-sequence>.jsonl` and
`--sensitivity`. All four passed. The existing session corpus was replayed
locally, but its Lean generator was not rerun locally; the existing CI Lean
gate covers it. No arbitrary-trace, OS atomicity or TOCTOU proof is claimed.

## Limits and reproducible checks

Use the strict/report commands in `docs/development.md`. Focused checks:
`python3 -m unittest discover -s tests -p test_boundary_contracts.py -v`,
`cargo test -p hoimin-cli --test lean_progress_input_oracle`, the exact tests
recorded in the registry, `cargo fmt --all -- --check`, and focused Clippy.
Minimal witness: set `HOIMIN_BOUNDARY_CASE=result_killed_exit_zero_complete`
and run the shared reader matrix exact test directly. The runner itself
intentionally clears this filter.

The Python mutation plan targets `--source tools --symbol
boundary_contracts:classify --profile focused`, jobs 1, workspace 8 GiB and
reserve 10 GiB, with `tests/test_boundary_contracts.py` fingerprinted and the
normal unittest argv. One selected equality mutant is killed; this is a
sensitivity probe, not a mutation score or saturation claim.

Native Linux hard OOM/process limits and Windows PID/fault injection remain
unexecuted with links to #228/#229/#157/#162/#223. Preparation error precedence
does not prove responsive cancellation of manifest/fingerprint/copy or a
whole-verify deadline. The unreadable-copy fixture requires an unprivileged
POSIX reader and otherwise reports infrastructure failure. The external
runner kills its POSIX process group on deadline; detached descendants are
not covered by that statement. No full workspace test or benchmark is claimed.

Final focused Clippy initially found two missing semicolons and a non-octal
permission literal in the new preparation test. Corrected these to semicolons
and `0o0`, formatted the code, and reran the focused check successfully.
The literal selector fixture also checks rank/order, projected candidate identity,
selection scope, profile and inherited limits rather than IDs alone.

Strengthening candidate comparison first exposed an intentional projection:
plan-only rank/score/ranking_reasons are absent from process reports. The
fixture now asserts literal plan ranks and compares every identity field
after explicitly removing only those three plan annotations. This initial
failed fixture assertion did not indicate a production mismatch.

Final checks: ten Python tests, four reader oracle tests, ten strict registry
entries, three minimal encoding witnesses, formatting and focused Clippy pass.
OKF validation covers 20 pages, 775 local links, current issue-490 hashes,
source footnotes and complete design/report indexes. Three report rows remain
unexecuted. Clippy also flagged the single fixture length after stronger
assertions; a local allowance keeps this one cross-command scenario readable.
