### Task 5: Complete distribution, documentation and branch acceptance

**Files:**
- Update: `formal/creusot/README.md`, approved spec status, durable review record, relevant `docs/knowledge` OKF contracts/catalog and release/proof documentation.
- Adjust packaging files/tests from Task 1 only if full acceptance exposes a concrete gap.

**Interfaces:**
- Consumes production crate, fresh proof runner, mandatory CI, and measurement evidence.
- Produces complete reproducible maintenance guidance, verified distribution and review record ready for PR.

- [ ] Document actual guarantees, exclusions/trust boundary, commands to regenerate/replay and repair proofs, pinned updates, cold/warm resource measurements, additional code/spec size and sensitivity results. Never claim Ledger/OS timing or unsupported targets are proved.
- [ ] Run repository-required workspace tests, all-features clippy/formatting, Python quality checks, Lean oracle/proof correspondence and runtime contracts. Reuse passing focused checks where unchanged; fix actual failures without repeated full-suite churn.
- [ ] Verify root/fuzz lock consistency and dependency policy, generated SBOM, sdist includes the math crate and proof annotations' normal-build dependencies, build from sdist and execute wheel. Record results with exact source commit.
- [ ] Validate relevant OKF artifacts, record five final implementation/testing self-reviews, archive task/review evidence durably, and commit. Controller then obtains whole-branch review, addresses findings, pushes the issue branch and creates PR closing #999 only when all accepted conditions have evidence.

## Plan self-review (five passes)

1. Coverage: all Q1–Q7 and eight arithmetic obligations map to Tasks 1–4; distribution/docs/PR map to Tasks 1 and 5. Actual cold setup remains a measured gate.
2. Steps: each task has failing behavioral tests or actual proof sensitivity, a concrete deliverable, verification and commit. No task ends at an unexecuted environment proposal.
3. Interfaces: Task 1 owns arithmetic API, Task 2 environment manifest/preflight, Task 3 goal identifiers/strategies, Task 4 runner/CI. The optional Serde feature does not alter proved arithmetic.
4. Failure classes: all five Review Focus items have explicit owners/tests; manually constructed output and missing solver are included.
5. Proportion/resources: five deliverables, sequential heavy builds, scoped reviews, no parallel tool installations or unapproved cap increases. Existing approved spec governs remaining implementation choices.



- Work only in `.worktrees/issue-999-creusot-budget-proofs` on `issue-999-creusot-budget-proofs`, starting at `b65f62834b35a9ff85795b041e9687b21e5c51ba`. Preserve other worktrees and untracked personal files.
- Product and verification use the same implementation and value types. No compatibility adapter, proof-only arithmetic, target-specific substitute, hoimin-authored `trusted`, external specification, or `assume` hole.
- Cover all public typed inputs: all `usize`, positive `NonZeroUsize`, all `Duration`, and positive fixed durations. Preserve observable results, serialized forms, config error behavior, and warning-only projection semantics.
- Required proof targets: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, generated and proved separately, sequentially, on Linux.
- Per solver process: 30 seconds, 2 GiB; maximum 2 concurrent solvers. Entire CI job, including cold environment setup: 30 minutes. Measure cold/warm duration, memory and disk. Escalate measured insufficiency; never silently relax caps or scope.
- Pin Creusot v0.14.0, nightly-2026-08-03, OCaml 5.3.0, Why3 `05bf4ddf55cb3042f34c3bc0ed3aff56a226978a`, Why3find `0f054b93c86ac7ba20bca7df5d1563a9a4434a30`, Z3 5.1.0, CVC5 1.4.1; verify artifact hashes and actual tools/configuration.
- Regenerate from current source; require nonempty obligations and named required functions/contracts; prohibit successful-result cache reuse. Final unproved obligations, missing tools, timeouts, and unintended skips fail CI.
- Stable Rust 1.99.0 and all-features checks remain supported; normal builds do not require proof tools. Preserve Lean oracle correspondence and runtime contracts.
- AAA, one case per test execution, parameterized finite cases and property tests for arithmetic domains. Record at least five self-review passes for requirements, design, implementation, and testing in durable review notes.
- One heavy build at a time, `CARGO_BUILD_JOBS=2`, task-owned shared `CARGO_TARGET_DIR=/tmp/hoimin-issue999-target`. Check disk before/after heavy work; clean owned obsolete artifacts only. Do not spawn nested agents.
- Complete issue update, implementation, actual verification, documentation, and PR. Do not merge, release, or publish a container image.



## Final integration handoff (controller-maintained)

Read `/tmp/hoimin-issue999-design-mnk8miii/final-integration-handoff.md`, final Task 3/4 reports and review reports in this SDD directory. They contain exact evidence paths; do not repeat accepted suites unless new changes justify it. Task 5 BASE is `878be0d5496be231f0f15b6da1a8fe8f03659e6c` (clean feature branch after upstream merge). No Task 5 implementation has started yet.

### Current acceptance and remaining code work

- Task 3 production proof commit `68ea9956` independently approved. Three targets, each 25 Coma files/59 goals, fresh generation and no-cache replay; five compiling sensitivity cases rejected. Stable Rust: 2916 passed/0 failed/33 ignored; all-features Clippy and fuzz check passed. Same source bodies and structural equality contracts remain mandatory.
- Task 4 original native acceptance: source `41c888f5`, run `39191762043`. Warm attempt 1/job `117865951824`: entire CI success, cache hit, setup-inclusive 240 seconds, runner 153.348 seconds. Genuine cold attempt 2/job `117868832092`: cache miss after deleting only the owned feature cache, entire CI success, setup-inclusive 942 seconds, runner 152.438 seconds. Each target independently generated/proved 25/59. Exact artifacts, full logs, resource measurements and metadata are under `task-4-evidence/native-ci/{accepted-warm,accepted-cold}` and adjacent files. Initial failed cold attempt is separate evidence, never count it as passing.
- Task 4 review first found source-contract attribute bypass and missing SIGTERM cleanup. Fix `b5d920b5` addressed those, but scoped review found inherited SIGTERM blocking. Round 2 committed `62e3e390`, removed inherited masks with deferred launch handling and unreaped-PID ownership. Its scoped re-review approved all findings addressed/no new Important or Critical findings; see `task-4-fix2-review.md`. Final actual local Python3.12 proof was 35.851 seconds, 3×59 goals, under `task-4-evidence/fix2-py312`. Task 4 is complete with final current-lock native regression assigned to Task 5 integration. Root owns final native verification after integration. Source arithmetic, tool pins and image recipe remain unchanged through these fixes.
- Non-Linux run `39193858954` at `4d4a3ec6` completed SUCCESS/all nine jobs SUCCESS: Windows/macOS Rust, Clippy/quality and wheels, plus Windows purity/metrics/resources/performance. Final metadata: `task-4-evidence/native-ci/non-linux-accepted-run-metadata.json`; Windows Rust log `windows-rust-117872253152.log`; Windows wheel log `windows-wheel-117872253173.log` (1146 Python passed/47 skips). Earlier run `39192791500` failed and must not be described as passing.
- Concrete remaining test repair: earlier Windows Rust job `117869024599` failed `lean_result_lifecycle_oracle::result_lifecycle_oracle_correspondence`, scenario `stop_preserves_accepted`. Actual empty results/exit 4, expected one durable accepted kill. Log: `task-4-evidence/native-ci/windows-rust-117869024599.log`. Root focused read found Windows `execute_strict` calls `run_cli(...total_timeout=true)` with a one-second budget; Unix uses `run_cli_with_durable_timeout_premise` with 12 seconds and persisted-result polling. Its premise helpers are `cfg(unix)` although APIs appear portable; existing cleanup already has a Windows taskkill path. Diagnose using systematic-debugging/lean-test-oracle as relevant; preserve actual durable-result premise, bounded cleanup and oracle expectations. A later passing run does not erase the demonstrated timing weakness. Own a narrow test-only fix if confirmed; root will run actual Windows CI on the final checkpoint.
- Deferred Task 4 Minor finding: 393 unresolved-source warnings per target from relative Coma spans. Actual path is `verif/hoimin_budget_math_rlib/duration/crates/hoimin-budget-math/src/duration.rs`. See `task-4-review.md`. Investigate pinned `--span-mode absolute` as a narrow repair; check actual supported semantics and rerun actual proof if changing flags. Otherwise document the precise limitation and cost. Do not suppress unrelated warnings or erase historical fixture evidence. Carry disposition to final review.

### Upstream integration

Root fetched main `f4460fc00317ca6468577d4f075c9d8848264af0`, two commits after original base: `74bc2d24` updates root serde_json 1.0.151→1.0.154, Hypothesis 6.169.4→6.170.0 and uv.lock; `f4460fc0` updates C4 architecture docs. Root merged it without conflicts as `878be0d5`, preserving proof-source SHAs. Include the new math boundary in the updated C4 document as appropriate; preserve others' documentation. Sync a task-owned Python environment to new locked dependencies. Re-run appropriate integration after these changes; don't force unrelated fuzz versions merely to equal root's graph. Proof crate/tool pins did not change upstream, but the input lock hash changes and needs fresh proof execution.

### License/dependency policy and prepared audit tool

Final cargo-deny 0.20.2 audit of root and fuzz, all features and no target filtering, remains mandatory. Root prepared the official macOS ARM binary at:
`/tmp/hoimin-issue999-audit-tools/cargo-deny-0.20.2/unpacked/cargo-deny-0.20.2-aarch64-apple-darwin/cargo-deny`.
It prints 0.20.2. The official release API digest and published checksum both matched archive SHA256 `fe67d82a10d8597a3549364cb733a3f9cc1bfff9031b7ae46384a9f2a72090c3` from https://github.com/EmbarkStudios/cargo-deny/releases/tag/0.20.2. No system install needed.

Both exact `docs/dependency-policy.md` commands were run at graph source `4137a294`: root/fuzz exit 4, licenses FAILED/sources OK. Logs: `task-5-evidence/cargo-deny-root-before.log`, `cargo-deny-fuzz-before.log`. Reuse this RED rather than reinstall/repeat. Only `creusot-std` and `creusot-std-proc` 0.14.0 were rejected (LGPL-2.1-or-later); `pearlite-syn` 0.14.0 declares MIT OR Apache-2.0. Pinned Creusot root LICENSE blob is `8000a6faacf471c537530805ab29523c7732e11a` at source `65349025c67e69fcd7479f1f420fa5904f69bc27`.

Inspect complete graph, actual upstream licenses, proc macro/generated artifacts, sources and notices; use exact-version exceptions if warranted, never broaden the global allowlist. Normal all-features math dependency tree excludes proof crates, but that alone is not license compliance. Task 3 verified every Rust source of these three registry packages equals pinned upstream source; source equality is not a license conclusion. Existing `docs/licensing.md` and `licenses/contracts-MPL-2.0.txt` show the scoped notice pattern. Final root/fuzz audit, build-script inventory and actual generated shipping SBOM still required.

### Distribution, docs and evidence

Build the actual sdist, build from that sdist and execute the wheel; inspect the generated SBOM and packaged crate sources. Do not run release/publication workflows. Root owns GitHub push/CI/PR and can dispatch existing `ci.yml` and `non-linux-ci.yml` verification workflows. Send an early coherent checkpoint after any runner-span/Windows-fixture changes so remote verification can overlap docs/package work, without duplicate runs of superseded checkpoints.

Update relevant development/CI/licensing/dependency/toolchain/release docs and OKF via create-okf skill. Do not rewrite all historical source hashes. Update plan/spec statuses and durable reviews accurately. Record five implementation and five validation self-review passes; don't call unperformed checks successful.

Before root removes SDD scratch, retain task reports/reviews, useful actual execution evidence, exact source commits and ALL `Ruling:` decisions/costs durably. Task 2 archived evidence is under `evidence/task-2` with SHA256 manifest; Task 3/4 actual logs/summaries are in their evidence directories. Keep local ARM development measurements separate from native Linux x86_64 cold/warm acceptance, and preserve failed attempts where they explain repairs/cost. Archive relative to stable durable paths rather than pointing final docs only at ignored scratch.

Root owns Task 5 scoped review, one final strongest-model whole-branch review, one fix wave/scoped re-review, final push and PR closing #999. No broad review/PR/subagents from this implementer; no merge into main, release or container publication. User authorizations persist; no repeat permission questions.

Operational paths: `CARGO_TARGET_DIR=/tmp/hoimin-issue999-target`, `CARGO_BUILD_JOBS=2`, `UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache`. Existing host Python is main `.venv/bin/python`; worktree `.venv` currently symlinks to main, so use an isolated environment when syncing changed dependencies. OS process tests need permitted process access; restricted sandbox returns monitor_error 126. One heavy build at a time; about 98 GiB host free, target 18 GiB. Only clean owned obsolete artifacts, preserve unrelated worktrees and 13 containers.
