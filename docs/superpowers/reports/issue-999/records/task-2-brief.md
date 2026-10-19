### Task 2: Build and measure the pinned Linux verification environment

**Files:**
- Create: `formal/creusot/toolchain.json`, `formal/creusot/Dockerfile`, `tools/creusot_environment.py`, `tests/test_creusot_environment.py`.
- Update: durable review record; `formal/creusot/README.md` for reproducible environment commands and measured prerequisites.

**Interfaces:**
- Produces manifest with exact source revisions, download hashes, executable versions and prover configuration for approved tools; no mutable unverified downloads.
- Produces `python tools/creusot_environment.py check --manifest formal/creusot/toolchain.json --project-root PROJECT`, exit zero only if all actual executables, installed configuration and selected project-local `why3find.json` match; actionable nonzero diagnostics otherwise. `check-installation` preserves image self-check before a project exists and cannot substitute for proof preflight.
- Produces Linux x86_64 Docker build/run commands and a native Linux ARM64 development variant using the same source pins and proof targets. CI may install directly from the same pinned recipe if that avoids unnecessary container build overhead. Local ARM64 measurements do not replace native Linux x86_64 cold/warm acceptance in Task 4. Do not publish an image.

- [ ] Read pinned upstream installation/prover CLI sources, resolve full revisions and checksums, and record their authoritative URLs. No unsupported guesses about CLI flags.
- [ ] Write parameterized failing tests for a missing solver, wrong version, wrong configuration and malformed manifest; successful version printing with mismatched content must fail. Use controlled executable fixtures without asserting implementation-internal command ordering.
- [ ] Implement isolated install recipe and strict preflight. Download only pinned sources/artifacts with hash verification, bound build parallelism, and arrange exact Rust target standard libraries. Keep installation inside task-owned container/cache paths.
- [ ] Begin local environment setup from cold with a 30-minute wall-clock deadline per attempt, recording elapsed time, peak memory where measurable, and disk delta. During repair, reuse successful tool layers and distinguish failed cold attempts from the final working warm setup. Check the actual toolchain and an ordinary crate verification invocation. Task 4 must measure the final recipe from cold and warm on native Linux x86_64, including setup and all proofs within the accepted job cap; local cached success does not satisfy that gate. Stop and report measured cap violations rather than increasing limits; diagnose target support with real compiler invocation.
- [ ] Run focused Python tests/lint/types (and relevant hoimin mutation testing per skill), record five implementation and five testing self-reviews and measured limitations, then commit. Task 3 owns actual arithmetic proofs, not this environment smoke check.


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
Environment research handoff: /tmp/hoimin-issue999-design-mnk8miii/environment-handoff.md

## Resource-enforcement clarification

Why3find calibrates its time parameter, so it alone is not a hard30s wall limit. The effective Linux prover commands use verified GNU timeout and prlimit(RLIMIT_AS=2GiB), with preflight checks and a real resource-limit smoke. Document the stricter address-space restriction distinctly from RSS and leave full runner/deadline/concurrency/freshness semantics to Task4. No relaxation of approved limits. See progress.md for ruling and cost.
