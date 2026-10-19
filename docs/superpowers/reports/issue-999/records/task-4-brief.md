### Task 4: Enforce fresh complete verification in mandatory CI

**Files:**
- Create: `tools/verify_creusot.py`, `tests/test_verify_creusot.py`.
- Modify: `.github/workflows/ci.yml`, `tools/ci_selection.py`, `tests/{test_ci_selection,test_ci_selection_workflow}.py`, `formal/creusot/{README.md,obligations.json}` as needed.
- Update: durable review record.

**Interfaces:**
- Consumes environment preflight, required function/goal manifest, and proven generation/replay commands from Tasks 2–3.
- Produces `python tools/verify_creusot.py --output <directory>`: preflight, freshly generate each target, check function/contract coverage and nonempty goals, replay every goal without success cache, fail closed; emit JSON measurements/diagnostics and logs into the given directory.
- Produces mandatory CI job `creusot` with `timeout-minutes: 30`, including environment preparation, selected for source/spec/tools/dependency changes and required by CI aggregation when selected.

- [ ] Write parameterized failing runner tests: missing required function/contract, empty goals, old artifacts, one target unresolved, missing/mismatched tool, solver timeout recovered by a successful alternative, overall timeout, and success only after all three fresh target results. Test observable failure and subprocess boundaries with fixtures, not an alternate fake proof engine.
- [ ] Implement fail-closed orchestration using actual Task 3 commands and structured manifests; clear owned generated output before each generation, prohibit prove-only stale success and enforce caps/process counts. Identify required coverage by actual named contracts/functions, not total counts alone.
- [ ] Add CI selection/aggregate tests for changed math crate, proof specs/strategies, tool pins and runner, plus failed/cancelled/unintentionally skipped proof job. Integrate installation/cache without solver-success cache; upload diagnostics on failure.
- [ ] Execute full runner cold/warm on Linux and all three targets with actual tools; record elapsed time/memory/disk. Run focused tests, lint/types and relevant mutation checks, five implementation and five testing self-reviews, then commit.



## Global Constraints

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



Approved spec: docs/superpowers/specs/2026-10-19-issue-999-creusot-adoption-design.md

Task2 local development host is native Linux ARM64. Its repaired warm success and ordinary-crate smoke do not fulfill this task's native Linux x86_64 cold/warm full-production-proof acceptance. Read Task2 and Task3 final reports/README for actual commands and measurements. The controller can push the authorized issue branch and dispatch the existing CI workflow against it to obtain actual native measurements; final PR follows accepted evidence. Do not publish container images or alter branch protection. Tool/build caches are permitted, successful solver-result reuse is forbidden. Final cold timing includes every environment preparation step and all three target proofs within the unchanged30min job cap.

Effective solver commands enforce GNU timeout30s and prlimit(RLIMIT_AS=2GiB), with preflight checks on actual tools/configuration. This address-space bound is stricter than RSS and is not an environment-wide RSS guarantee. The runner remains responsible for fresh scoped coverage, maximum2 simultaneous solvers, overall deadline, diagnostic capture, no-cache proofs and correct final success/failure semantics.

Actual final local proof container has Python3.12.3 (Task2 executable check); the host project tests use Python3.14. If the runner executes inside this pinned Ubuntu environment, keep it3.12-compatible and validate its actual container execution. Do not infer compatibility from host-only tests.

Task2 review fix establishes two commands: check-installation validates installed environment only; actual proof MUST use `python tools/creusot_environment.py check --manifest MANIFEST --project-root PROJECT`, requiring exact pinned PROJECT/why3find.json. Use actual Cargo/Why3 project root, not a template directory substituted for the proof project. See final Task2 report/API for paths.

Task3 current interface decision (confirm final report before implementing): workspace-root why3find.json is the effective project configuration. Check actual committed project configuration before any staging; never overwrite a changed/missing input from the template and then claim drift detection. obligations.json maps all eight spec clauses to required normalized source contracts/logical definitions/invariant plus actual Coma paths and goal identifiers/translated markers. Verify both source contracts and generated named scope, not only counts. Task3 final report supplies exact schema/commands and saved strategies. Normal stable builds use cfg(creusot)-only annotation dependency and common runtime bodies.
