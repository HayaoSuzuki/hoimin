# Task 4: mandatory fresh Creusot verification

Status: native Linux x86_64 warm and genuine cold proof acceptance both passed at proof-source SHA `41c888f5` in https://github.com/HayaoSuzuki/hoimin/actions/runs/39191762043. Independent review at `4137a294` found two Important gaps, addressed in fix round 1; scoped re-review at `b5d920b5` found inherited SIGTERM masking, addressed in fix round 2 below. Final native current-lock CI and scoped re-review remain pending. First failed CI evidence is at https://github.com/HayaoSuzuki/hoimin/actions/runs/39190445969. Corrected non-Linux validation https://github.com/HayaoSuzuki/hoimin/actions/runs/39193858954 passed at the earlier test-portability SHA `4d4a3ec6`.

## Result and scope

`python3 tools/verify_creusot.py --output DIR` checks the approved eight-clause manifest against actual production source attributes, required logical definitions, structural models and named generated markers. On each of the three required targets, in sequence, it checks the pinned installed tools and exact selected project; cleans only the budget-math package target; removes the owned generated Coma tree; generates new nonempty Coma files; checks their timestamps, named contracts and inventory; installs target-specific strategies; checks the pinned environment again; and runs `cargo creusot --only=prove --no-cache --replay -j 2 --why3find-arg=--goals`. It requires every generated theory and every named goal to be proved, exactly once, and the final file count to match. Source, manifest, selected project and Cargo inputs are hashed before/after each target's proof. Each command is subject to the runner's 1800-second deadline. Deadline/interruption cleanup stops only the command's Linux session, including GNU `timeout`'s separately grouped solver children.

CI's selected `creusot` job is mandatory in aggregation. It runs on native `ubuntu-latest` x86_64 under `timeout-minutes: 30`; its image cache key includes the effective pinned Docker/tool/install/configuration inputs, including `why3.conf`; cache reuse is for the tool image only. Every CI attempt still regenerates Coma and replays all goals without successful-solver cache. Reports and diagnostics are uploaded on `always()` and `failure()`, respectively, with run ID and attempt in the artifact name. The pinned solver configuration/preflight enforces GNU timeout 30 seconds, `RLIMIT_AS` 2 GiB per solver process and at most two concurrent solvers; the runner independently limits Cargo build jobs to two. The address-space bound is not an environment-wide RSS limit.

Changed files: `tools/verify_creusot.py`, `tests/test_verify_creusot.py`, `tests/fixtures/creusot/`, `tests/test_creusot_environment.py`, `tools/ci_selection.py`, `tests/test_ci_selection.py`, `tests/test_ci_selection_workflow.py`, `tests/test_ci_workflow.py`, `pyproject.toml`, `.gitattributes`, `.github/workflows/ci.yml`, and the durable self-review section in `docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md`. No production arithmetic, formal pins, strategy, or manifest was changed.

Commit span from Task 4 base `deb729be` to current HEAD `62e3e390`: `09a1d1b5` mandatory runner/CI; `e248f5e8` tracked real parser fixtures; `41c888f5` Python 3.12 runtime compatibility; `4d4a3ec6` cross-platform test/EOL repair; `4137a294` durable native CI review evidence; `b5d920b5` scoped-review guard/SIGTERM repair; `62e3e390` child SIGTERM delivery and reaped-leader correction. Native warm/cold proof acceptance ran on `41c888f5`; the two intervening commits through review base `4137a294` changed only tests, `.gitattributes` and this review note. The `b5d920b5` runner has local three-target proof evidence below; the final `62e3e390` runner has its own later local proof and awaits native CI.

## Tests and failure evidence

The initial test import failure when the runner module did not exist was a collection RED, not behavioral evidence. Behavioral REDs included: a joint source-plus-manifest weakening that previously passed scope checking; CI selection/aggregation assertion failures before the new job was wired; a cache-key assertion missing `formal/creusot/why3.conf`; a live Linux child that survived the old process-group-only cleanup; and an ordering assertion that caught killing the child group before the dispatcher. The corresponding GREEN tests now check exact source/manifest scope, cache recipe coverage, session ownership and leader-first cleanup. Parameterized fixtures additionally reject missing/extra/weak contracts, missing functions/logical definitions, stale/empty/missing-marker Coma, missing or unproved named goals, missing/mismatched preflight tools, deadline expiration and incomplete target sequences. A solver timeout followed by a successful alternative is accepted only when the final named goals all pass. The three-target success fixture crosses the actual preflight, `cargo clean`, generation and proof subprocess boundaries and waits for all three target results; it does not implement a fake proof engine.

Initial focused command at checkpoint:

```text
/Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m pytest -q tests/test_verify_creusot.py tests/test_ci_selection.py tests/test_ci_selection_workflow.py tests/test_ci_workflow.py::test_rust_jobs_install_only_their_classified_toolchain
119 passed, 1 skipped in 1.69s
```

The same focused command passed again after fixture repair: `119 passed, 1 skipped in 1.53s`. The skip is the live Linux `/proc` process-group test on the macOS host. The equivalent separate-PGID child test ran inside the pinned Linux ARM64 container: before the cleanup change the owned child remained in state `S`; after the fix it reached state `Z` and did not execute further. The ordering test first failed when the child group was killed before the dispatcher and passed after reversing the order.

The first native CI workflow-lint job `117861860144` failed during setup with `105 passed, 24 errors`, and Wheel smoke job `117862232826` failed with `1,159 passed, 9 skipped, 24 errors`. All 48 errors were the same fixture lookup: tests referenced ignored `.superpowers/.../task-3-evidence/final` files absent from a clean checkout. This was a CI portability defect, also foreshadowed by the first hoimin mutation worker's baseline failure, not a proof-engine failure. The first CI quality and core-dependency-purity jobs succeeded. Repair `e248f5e8` tracks only the real Task 3 first-run parser fixtures: 25 Coma files byte-identical across all three targets, their three target-specific proof logs, and a provenance README. No saved `proof.json` strategy or successful solver-result cache was copied into the fixture; the production runner never reads it. A clean `git archive HEAD` without the ignored Task 3 evidence ran `tests/test_verify_creusot.py`: `29 passed, 1 skipped in 0.67s`. The exact workflow-lint job pytest selection passed locally after repair: `120 passed, 9 skipped in 1.45s`. A fresh hoimin plan/verify using `--include 'tests/fixtures/creusot/**'` killed the same guard-removal candidate, exit 0; its temporary workspace was removed. Workflow-lint and Wheel smoke later succeeded in the corrected native run at `41c888f5`.

The first native proof job `117861897121` built and saved the pinned native image, then failed before the runner could start: Python 3.12 rejected `except OSError, IndexError, ValueError:` at `tools/verify_creusot.py:318`. Host Ruff had rewritten the earlier parenthesized form under the project-wide `py314` target after the earlier local ARM proof. A new regression test first failed with `ast.parse(..., feature_version=(3,12))`; it passed after repair `41c888f5` restored the tuple and set a file-specific Ruff `py312` target in `pyproject.toml`. Ruff format and lint, type checking, and commit hooks passed. The exact workflow-lint pytest selection then passed locally: `121 passed, 9 skipped in 1.60s`. After this final commit, actual pinned container Python 3.12.3 ran `py_compile`, `--help`, and all three full production proofs successfully. The first native proof failure is preserved as failure evidence, not counted as cold acceptance.

The manual Windows wheel-smoke run https://github.com/HayaoSuzuki/hoimin/actions/runs/39192791500 failed at SHA `41c888f5`: `13 failed, 1,147 passed, 33 skipped`. Eight environment tests used POSIX `#!/bin/sh` fake executables, for which Windows `shutil.which("rustc")` returned `None`; three wrong-version cases and one PATH case also passed spuriously because a test-path substring or the earlier rustc PATH error satisfied their loose assertions. Those 12 POSIX fake-execution cases now explicitly skip on Windows; malformed manifest, receipt, hash and other portable validators continue to run. Two proof-runner tests failed because default Windows cp1252 decoded UTF-8 `✔` as `âœ”`; the UTF-8 bytes emitted back into the GitHub log rendered as a checkmark, disguising the actual mismatch. The fixture reads/writes now specify UTF-8, preserving the success-with-alternative and unresolved-third-target assertions across OSes. Two actual process-group tests now declare their POSIX/Linux dependency; no Windows proof-runtime process management was added. The last failure compared LF sdist metadata bytes to a CRLF Windows checkout of the new `crates/hoimin-budget-math/Cargo.toml`; root `.gitattributes` now pins that exact file to `text eol=lf`, leaving all other files' newline policy intact. Commit `4d4a3ec6` changes only `.gitattributes` and those two test modules. The Windows CI failure is retained as RED evidence; repair run https://github.com/HayaoSuzuki/hoimin/actions/runs/39193858954 completed with all nine jobs successful.

```text
/Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m pytest -q tests/test_creusot_environment.py tests/test_verify_creusot.py tests/test_sdist.py
100 passed, 1 skipped in 14.43s
git -c core.autocrlf=true checkout-index --prefix=<owned-temp>/ -- crates/hoimin-budget-math/Cargo.toml
598 bytes, 0 CRLF, 30 LF
```

The skipped local case is the live Linux session test on macOS. A direct byte-decoding check of the tracked proof log found 85 checkmarks with UTF-8 and zero with cp1252 (which instead produced 85 `âœ”` sequences). Ruff lint/format, ty and `git diff --check` passed after this repair; its commit hooks passed.

Full Python suite, before the final small cache-key and focused test additions:

```text
UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache /Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m pytest -q
1179 passed, 12 skipped in 44.98s
```

This command used an escalated process sandbox because four pre-existing Lean resource-guard process tests need OS process access. The first restricted run had seven failures: one newly changed CI job expectation (fixed), four exit-126 process-monitor restrictions (same tests passed in escalated targeted rerun), and two `uv` cache permission failures (passed with the task-owned `UV_CACHE_DIR`). None were suppressed or marked xfail. The final focused set was rerun after the additional tests and all passed. The earlier Task 3 Rust suite was not rerun because arithmetic is unchanged.

Final quality checks:

```text
ruff check tools/verify_creusot.py tools/ci_selection.py tests/test_verify_creusot.py tests/test_ci_selection.py tests/test_ci_selection_workflow.py tests/test_ci_workflow.py
All checks passed!
ruff format --check tools/verify_creusot.py tools/ci_selection.py tests/test_verify_creusot.py tests/test_ci_selection.py tests/test_ci_selection_workflow.py tests/test_ci_workflow.py
6 files already formatted
PATH=/tmp/hoimin-issue999-workflow-tools/bin:$PATH /Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m tools.workflow_lint
No findings to report. Good job! (3 ignored)
/Users/hayao/RustroverProjects/hoimin/.venv/bin/ty check tools/verify_creusot.py tools/ci_selection.py tests/test_verify_creusot.py tests/test_ci_selection.py tests/test_ci_selection_workflow.py tests/test_ci_workflow.py
All checks passed!
git diff --check
exit 0
```

The three ignored workflow-lint findings are the pre-existing `release.yml` Python-shell GITHUB_ENV evaluation warnings. Type checking used the pinned host environment. The relevant hoimin mutation candidate removed a runner guard (`tools/verify_creusot.py:86-89`, `condition_constant`, candidate `m1_abb215f94d37aa6f45b435ce10c6cde0b1645b7b02381c5c734f89d03f4b41de`). Its first `verify` failed baseline because ignored Task 3 fixture evidence was not copied to the worker; after a first `plan --include '.superpowers/sdd/2026-10-19-issue-999-creusot-adoption/task-3-evidence/final/**'`, `verify` exited 0 and killed the mutant. Following the tracked fixture repair, a new plan with `--include 'tests/fixtures/creusot/**'` and verification also exited 0 and killed it. The temporary mutation workspaces were removed. Planning used one worker, at most 8 GiB workspace and at least 10 GiB free disk; host had about 98 GiB free. The five requirements/design/implementation/testing self-review passes are in the durable review record.

```text
hoimin plan --root . --line tools/verify_creusot.py:86-89 --operators condition_constant --allow-best-effort-memory --jobs 1 --max-workspace-size 8GiB --min-free-space 10GiB --include 'tests/fixtures/creusot/**' -- /Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m pytest -q tests/test_verify_creusot.py > "$mutation_tmp/plan.json"
plan exit 0; 2 candidates
hoimin verify "$mutation_tmp/plan.json" --candidate 'm1_abb215f94d37aa6f45b435ce10c6cde0b1645b7b02381c5c734f89d03f4b41de' --format json > "$mutation_tmp/verify.json"
verify exit 0; selected mutant killed
```

## Actual local Linux ARM64 execution

Pinned local image: `hoimin-issue999-creusot-arm64:fix1-local`; task-owned container: `hoimin-issue999-task3`. Python inside container is 3.12.3, so these executions also verify the runner's actual runtime compatibility. The tools were already installed in the local image. For cold crate-cache execution only, `/tmp/hoimin-issue999-target` was cleared after checking free disk; no shared/unrelated container or artifact was removed.

```text
/Users/hayao/.rd/bin/docker exec hoimin-issue999-task3 bash -c 'cd /workspace && df -k . && rm -rf /tmp/hoimin-issue999-target && python3 tools/verify_creusot.py --output /tmp/task4-runner-cold'
exit 0; summary status passed; elapsed 81.379 s

/Users/hayao/.rd/bin/docker exec hoimin-issue999-task3 bash -c 'cd /workspace && python3 tools/verify_creusot.py --output /tmp/task4-runner-warm && df -k /workspace && du -sk /tmp/hoimin-issue999-target /tmp/task4-runner-cold /tmp/task4-runner-warm'
exit 0; summary status passed; elapsed 35.438 s
```

| Target | Cold generation / proof (s) | Warm generation / proof (s) | Both passes |
| --- | ---: | ---: | --- |
| `x86_64-unknown-linux-gnu` | 18.118 / 8.884 | 0.268 / 8.762 | 25 fresh nonempty Coma files, 59 proved named goals, 25 strategies |
| `aarch64-apple-darwin` | 13.537 / 9.166 | 0.217 / 9.922 | 25 / 59 / 25 |
| `x86_64-pc-windows-msvc` | 13.109 / 9.142 | 0.421 / 8.879 | 25 / 59 / 25 |

Cold maximum process-or-child RSS was 1,491,140 KiB during generation and 271,264 KiB during proof. Warm maxima were 186,572 KiB and 271,260 KiB. The output report size was 262,393 bytes cold and 261,976 bytes warm. After warm execution, `/workspace` had 23,736,656 KiB available; the task-owned target cache used 187,648 KiB, and each local evidence directory used about 384 KiB. Full local logs, GNU `/usr/bin/time -v` sidecars and JSON summaries are under `task-4-evidence/{cold,warm}`. These ARM64 runs start with preinstalled tools, so they are **not** evidence that the native Linux x86_64 cold image setup plus all proofs meets the 30-minute CI cap.

After the Python 3.12 repair and commit hooks, the final-source local run used:

```text
/Users/hayao/.rd/bin/docker exec hoimin-issue999-task3 bash -c 'cd /workspace && df -k /workspace && python3 tools/verify_creusot.py --output /tmp/task4-runner-final-py312 && df -k /workspace && du -sk /tmp/hoimin-issue999-target /tmp/task4-runner-final-py312'
exit 0; summary status passed; elapsed 37.827 s
each target: 25 fresh Coma files, 59 proved goals, 25 strategies
generation 0.269/0.270/0.271 s; proof 9.476/10.509/8.983 s
peak process-or-child RSS 187,464 KiB generation; 271,756 KiB proof
report 261,965 bytes; after-run disk available 23,665,368 KiB
```

The complete final-source logs and summary were copied to `task-4-evidence/final-py312/`.

## Native x86_64 CI acceptance

First, failed cold dispatch: https://github.com/HayaoSuzuki/hoimin/actions/runs/39190445969, workflow `.github/workflows/ci.yml`, feature ref `issue-999-creusot-budget-proofs`, checkpoint SHA `09a1d1b5`. Native job `117861897121` saw a cache miss for `creusot-amd64-c1c5dcc3fff807a83a5fcbfb453e70a4b03ef1c8eaddc3be9d46330d0a211c9b`, built image `sha256:485a192bceef8c69e7437df85855a6aac6950a58d90554d13f73130ff6ec5b62` (3,933,017,181 bytes), and saved that exact cache key. The tool-build stage's observed peak process RSS was 1,090,100 KiB. The saved tar was about 3.8 GiB; the job recorded 798 seconds through the Python syntax failure, with 81,464,232 KiB disk and 13,694,612 KiB memory available afterward. The full job log and `creusot-proof-report-39190445969-1` setup artifact are under `task-4-evidence/native-ci/attempt1-failed-cold/`; they are **not** successful cold proof evidence.

The native warm dispatch on final repair SHA `41c888f5` is https://github.com/HayaoSuzuki/hoimin/actions/runs/39191762043. Its Creusot job `117865951824` succeeded with `cache_hit=true`, full job elapsed 240 seconds, and runner elapsed 153.348 seconds. The runner regenerated and proved 25 Coma files, 59 named goals and 25 strategies separately for each of the three targets, with `--no-cache --replay` in each proof command. The per-target generation/proof times were x86 Linux 39.164/14.900 seconds, Apple ARM 30.500/14.248 seconds, and Windows 30.398/13.847 seconds. Peak process-or-child RSS was 1,510,704 KiB during generation and 274,444 KiB during proof. The runner report used 262,421 bytes. Afterward the native runner had 81,467,260 KiB disk and 14,716,636 KiB memory available. Complete target logs, `summary.json`, `setup.txt`, full job log and run metadata are preserved in `task-4-evidence/native-ci/accepted-warm/` and adjacent metadata files. The whole attempt-1 workflow finished successfully, including workflow-lint, Wheel smoke and mandatory aggregate; the conditional cgroup job was correctly skipped.

The exact task cache key `creusot-amd64-c1c5dcc3fff807a83a5fcbfb453e70a4b03ef1c8eaddc3be9d46330d0a211c9b` (cache ID `9139081989`) was removed under the feature-branch ref after saving the warm artifact; no other cache was deleted. The same SHA `41c888f5` Creusot job was then rerun as [attempt 2, job `117868832092`](https://github.com/HayaoSuzuki/hoimin/actions/runs/39191762043/job/117868832092). The job log shows `Cache not found` for the exact key, image build/save succeeded, and every proof command succeeded. The complete job took 942 seconds, within the unchanged 1,800-second cap; the proof runner took 152.438 seconds. The job metadata records `success`, from `03:02:03Z` to `03:17:52Z` (including GitHub step overhead). The pinned native image reported 3,933,017,305 bytes and its saved tar 3,976,204 KiB. Peak measured tool-build step RSS was 1,087,072 KiB. After completion, 81,462,316 KiB disk and 14,483,712 KiB memory remained available.

| Target | Cold generation / proof (s) | Generated and proved | Peak generation / proof process-or-child RSS (KiB) |
| --- | ---: | --- | ---: |
| `x86_64-unknown-linux-gnu` | 39.561 / 14.400 | 25 fresh Coma, 59 goals, 25 strategies | 1,504,228 / 273,260 |
| `aarch64-apple-darwin` | 30.590 / 14.349 | 25 fresh Coma, 59 goals, 25 strategies | 1,507,040 / 272,880 |
| `x86_64-pc-windows-msvc` | 30.691 / 14.351 | 25 fresh Coma, 59 goals, 25 strategies | 1,502,072 / 273,052 |

The cold runner reported `status=passed` and 262,429 output bytes. I rechecked both accepted artifacts: each has three target entries, each target's generation and proof commands, `--no-cache --replay --why3find-arg=--goals`, nonempty preflight logs, and `Proved (25 files) ✔` in its proof log. Warm `cache_hit=true` and cold cache-miss log are distinct; neither reuses successful solver results. The full cold job log, run metadata, setup, JSON summary, GNU time sidecars and target logs are under `task-4-evidence/native-ci/accepted-cold/` and adjacent `creusot-cold-117868832092.log`/`accepted-cold-run-metadata.json`. This is the required complete native setup-plus-proof acceptance, separate from the earlier failed cold build attempt.

## Review concerns

No solver time/address-space/concurrency limit, target scope, or 30-minute job cap was relaxed. The accepted warm job hit the tool-image cache; the accepted cold job missed after the exact task cache key was removed; both regenerated and replayed all three targets without solver-result cache. The complete cold job used 942 of 1,800 allowed seconds. The two commits after proof-source SHA `41c888f5` through review base `4137a294` touch only tests, `.gitattributes` and the durable review note, so native proof measurements correspond to identical runner/math/tools/workflow bytes at that review base. The corrected non-Linux integration run completed successfully; the earlier Windows Rust Lean result-lifecycle oracle mismatch remains Task 5 diagnosis. The scoped Task 4 review then found the two Important gaps repaired below; new native proof and re-review are pending.

## Independent-review fix round 1, base `4137a294`

The scoped review in `task-4-review.md` demonstrated that valid added attributes `#[cfg_attr(all(creusot), requires(selected@ < 100))]` and `#[cfg_attr(creusot, creusot_std::requires(selected@ < 100))]` were ignored by the source inventory. A nested `cfg_attr` was also ignored. Existing generated-file checking looks for approved postcondition markers and does not reject an added precondition in Coma; the review's temporary generated sensitivity check was parser-only, not a solver run. The fix now compares the complete multiset of attached outer attributes on each reviewed function against the exact manifest clauses wrapped in approved `#[cfg_attr(creusot, ...)]` syntax. Only built-in `#[must_use]` is exempt. Any alternative cfg predicate, qualified contract name, compound/nested attribute, added `#[cfg]`, or unsupported form fails closed. Extraction traverses line and nested block comments between attributes; an attribute on the function's own line is explicitly rejected rather than silently omitted. This is intentionally a narrow reviewed-source boundary, not a general Rust parser. Production arithmetic bodies, manifest and approved public input domains are unchanged.

The other finding reproduced default SIGTERM exit `-15` while the command's child session stayed live. The first repair installed a scoped SIGTERM handler that raises `ProofError`, traverses owned-session cleanup, and restores the previous handler. It also blocked SIGTERM around `Popen` to close the launch race; scoped re-review found that this mask was inherited by every child. Round 2 replaces that mask with deferred handler delivery while preserving dispatcher-first cleanup and leader reap. No unrelated process group is signaled, and the fixed solver limits and 1,800-second cap are unchanged. SIGKILL remains uncatchable. The minor unresolved Coma source-span warning is deferred to Task 5/final review as requested.

Behavioral RED: focused pytest against the original guard returned `DID NOT RAISE` for three alternate/nested preconditions (3 failures, 3 recognized-spelling controls passed, 1 Linux-only test skipped); two comment-separated attribute cases failed before scanner repair; an inline `#[cfg_attr(...)] pub fn` case failed before the same-line guard. The existing review separately reproduced exit `-15` and a live child before SIGTERM handling. GREEN: final `tests/test_verify_creusot.py` produced `37 passed, 2 skipped in 0.64s` on macOS; the skips are the two actual Linux session tests. An equivalent standalone Linux test in owned container `hoimin-issue999-task3` sent SIGTERM to a wrapper after a child entered a separate PGID; the wrapper exited `17` through `ProofError`, and `/proc/<child>/stat` showed `Z`. The tracked test asserts both results and cleans only its exact child PGID in `finally`. Existing deadline/separate-group, leader-first ordering, manifest, generated/proof and three-target tests remain green. A failure-path assertion checks that the prior SIGTERM handler is restored.

The host full Python suite initially had six sandbox-only failures outside Task 4: two release tests could not write the default `~/.cache/uv`, and four pre-existing Lean resource-guard subprocess tests returned monitor error `126`. The established process-permission setup and task-owned uv cache passed the unchanged full selection: `UV_CACHE_DIR=/tmp/hoimin-issue999-uv-cache /Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m pytest -q` → `1187 passed, 13 skipped in 45.61s`. That full run preceded the final small inline-attribute negative case and SIGTERM-handler restoration assertion; the final focused suite above covers both changes. Ruff lint/format, ty and `git diff --check` passed. A final-state `hoimin plan --line tools/verify_creusot.py:200-200 --operators compare_eq_ne --profile focused --allow-best-effort-memory --jobs 1 --max-workspace-size 8GiB --min-free-space 10GiB --include 'tests/fixtures/creusot/**' --fingerprint-include pyproject.toml` selected candidate `m1_6c3b209121819fef62a30c9e18725e8efef54087db2d1cf943d8803c02158537`; `verify` exit `0` with baseline exit `0` and mutant exit `1`. Both temporary mutation workspaces were removed; disk checks required >10 GiB free.

The final runner source SHA-256 was `986747a2d2a5d227029700982323ceba111036935ec570f04d2e5ac85ff11430` on both host and container. After the final source edit, pinned container Python `3.12.3` passed `py_compile`, `--help`, and the complete actual runner:

```text
/Users/hayao/.rd/bin/docker exec hoimin-issue999-task3 bash -c 'cd /workspace && python3 tools/verify_creusot.py --output /tmp/task4-runner-review-fix && df -k /workspace && du -sk /tmp/hoimin-issue999-target /tmp/task4-runner-review-fix'
exit 0; summary status=passed; elapsed=34.459 s; report=261,965 bytes
```

Each target freshly generated 25 Coma files, proved 59 named goals, and installed 25 strategies using `--no-cache --replay`. Generation/proof seconds were Linux x86_64 `0.235/8.592`, Apple ARM64 `0.220/9.247`, Windows x86_64 `0.275/8.551`; peak process-or-child RSS was `187,452/272,156 KiB`. Afterward container disk had `23,456,348 KiB` free; owned Cargo target used `187,648 KiB`, proof output `384 KiB`. Full logs/summary/time sidecars are preserved in `task-4-evidence/review-fix-py312/`. This ARM64 container run uses installed tools and does not substitute for root's pending native x86_64 fix-source CI. No stable Rust suite was repeated because production math and contracts did not change.

Five fix-round self-review passes: (1) compared every manifest function's complete outer-attribute multiset and explicit benign exemption, with alternate/compound/qualified cases; (2) checked scanner boundaries across whitespace, line/block comments, and same-line attributes, adding RED tests for missed valid placements; (3) checked SIGTERM handler lifetime and previous-handler restoration on command failure paths; (4) checked Linux launch masking, owned session identity, dispatcher-first order and separately grouped child termination with an actual process; (5) checked unchanged three-target/hard-cap recipe, parser/runtime compatibility, mutation sensitivity and final fresh no-cache proof execution. Native after-fix CI and independent scoped re-review remain outstanding at this local checkpoint.

## Scoped-review fix round 2, base `b5d920b5`

`task-4-fix1-review.md` marked both original Important findings addressed, then found a new Important regression: the parent's `pthread_sigmask(SIG_BLOCK, SIGTERM)` before `Popen` was inherited by `/usr/bin/time` and every proof-tool descendant. Unmasking only the parent after `Popen` did not restore child signal delivery. The final runner does not change the signal mask. Its scoped handler records SIGTERM received while `Popen` is in flight and returns; once `Popen` has assigned the owned PID, the pending stop raises through the existing cleanup. SIGTERM arriving after launch raises immediately. Repeated signals during cleanup are ignored. On an exception, cleanup runs only while `Popen.returncode is None`, so a signal after `wait()` has reaped the leader cannot target a potentially reused PID. Active cleanup still kills the dispatcher's group before separately grouped children in that session and reaps the leader. The previous handler is restored on all paths. No `preexec_fn`, solver timeout, 2 GiB address-space cap, two-prover limit, target, arithmetic, manifest or tool recipe changed.

Behavioral RED used the actual owned Linux container and source at `b5d920b5`: a separately grouped Python child installed a SIGTERM handler, then received `os.kill(child_pid, SIGTERM)`. Its handler marker remained absent, and the wrapper hit the 3-second runner deadline and exited `17` (`baseline_wrapper_exit 17 signal_received False`). This is OS-delivered child-signal evidence, distinct from the first repair's parent-SIGTERM test. On the host, two injected lifecycle cases failed before the fix: SIGTERM during a mocked `Popen` left no cleanup call because the handler raised before the PID was assigned; SIGTERM immediately after a mocked `wait()` reaped the leader still called `terminate_session(12345)`. Those tests exercise handler-entry transitions; the original Linux masking could defer real OS delivery during `Popen`, so they are not evidence that the old mask missed its launch-race purpose.

GREEN: with the corrected runner in the same Linux container, the real child received SIGTERM and wrote its marker; its wrapper returned `0` (`fixed_wrapper_exit 0 signal_received True`). A separate real parent SIGTERM test still returned `17` and left the owned separately grouped child in state `Z` (`fixed_parent_exit 17 child_state Z`). The tracked Linux child-delivery and parent-lifetime tests clean only their exact child groups on failure. Both injected lifecycle cases now pass: deferred launch cleanup runs once for PID `12345`, and no cleanup runs after a reaped leader. The final focused host command `/Users/hayao/RustroverProjects/hoimin/.venv/bin/python -m pytest -q tests/test_verify_creusot.py` returned `39 passed, 3 skipped in 0.62s`; the three skips require Linux process sessions or child delivery. Ruff lint and format check, ty and `git diff --check` passed. Per controller direction, the already passing full Python and stable Rust suites were not repeated.

The final host and container `tools/verify_creusot.py` SHA-256 was `74540a367029ea3c0cd9e76eed8042d342c85c577c7173288c3616cb870116e0`. Pinned container Python `3.12.3` passed `py_compile`, `--help`, and the complete production runner after the fix:

```text
/Users/hayao/.rd/bin/docker exec hoimin-issue999-task3 bash -c 'cd /workspace && python3 tools/verify_creusot.py --output /tmp/task4-runner-fix2-py312 && df -k /workspace && du -sk /tmp/hoimin-issue999-target /tmp/task4-runner-fix2-py312'
exit 0; summary status=passed; elapsed=35.851 s; report=261,970 bytes
```

Each target freshly generated 25 Coma files, proved 59 named goals, installed 25 strategies and used `--no-cache --replay`. Generation/proof seconds were Linux x86_64 `0.281/9.068`, Apple ARM64 `0.221/10.023`, Windows x86_64 `0.273/8.787`. Peak process-or-child RSS was `188,420 KiB` in generation and `271,200 KiB` in proof. Afterward container disk had `23,421,180 KiB` free; owned Cargo target used `187,648 KiB`, proof output `384 KiB`. Full logs, GNU time sidecars and summary are in `task-4-evidence/fix2-py312/`. This ARM64 container has preinstalled tools, so native Linux x86_64 current-lock setup-plus-proof CI and scoped re-review remain separate acceptance work.
