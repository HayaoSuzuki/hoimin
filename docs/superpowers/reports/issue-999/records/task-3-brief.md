### Task 3: Prove the complete production arithmetic on three targets

**Files:**
- Modify: `crates/hoimin-budget-math/{Cargo.toml,src/lib.rs,src/duration.rs,src/projection.rs}`, root/fuzz lockfiles as needed.
- Create: `formal/creusot/why3find.json`, `formal/creusot/proofs/` strategies as appropriate, `formal/creusot/obligations.json` required function/contract manifest.
- Update: `formal/creusot/README.md` and durable review record.

**Interfaces:**
- Consumes Task 1 production API and Task 2 pinned environment.
- Produces contracts on actual production constructors/accessors, auto timeout, waves, saturation/reconstruction, projection and shortfall covering all eight spec clauses.
- Produces reproducible generation/prove/replay commands, actual generated function/goal identifiers, and no-cache proof strategies suitable for Task 4's runner.

- [ ] Establish native Linux and both cross-target generation with current production crate; diagnose unsupported operations from the pinned Creusot source. Use equivalent provable production operations where needed, never verification-only alternate behavior.
- [ ] Add contracts and proof helpers for all eight mathematical spec clauses. Deny `creusot::trusted_code` in owned proof code; model Duration in integer nanoseconds and encode positive duration invariants. Do not assume a public projection was produced by its constructor.
- [ ] Generate fresh obligations and prove every goal for each required target sequentially, with solver 30 seconds/2048 MiB and maximum two concurrent processes. Record goal counts, required functions, elapsed time and logs, including final unproved goals if any.
- [ ] Perform sensitivity checks by temporarily changing production ceil, saturation, auto timeout and equality comparison one at a time; each must compile and produce an unproved relevant goal. Restore source after each and retain diagnostic evidence; compile errors are not proof-sensitivity evidence.
- [ ] Recheck stable Rust/all-features and focused arithmetic/Lean/runtime tests after proof-driven refactors. Record five implementation and five proof/test self-reviews, trust boundaries, repair effort and actual resource measurements; commit only correct production code and useful proof artifacts.



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
Proof research handoff (reference, not proof evidence): /tmp/hoimin-issue999-design-mnk8miii/proof-handoff.md

Task2 report and formal/creusot/README.md are the authority for actual environment commands after its review is completed. Use the final task-owned native Linux development image; preserve native x86_64 CI acceptance as a later mandatory gate.

Proof-only cfg attributes/imports/ghost code may be added, but actual production types and runtime function bodies must remain common. Investigate target cfg dependency handling and license declarations using the handoff; do not add local trust holes for unsupported library operations. Any equivalent implementation refactor must be covered by existing arithmetic properties and Lean oracle.

Task2 review fix establishes two commands: check-installation validates installed environment only; actual proof MUST use `python tools/creusot_environment.py check --manifest MANIFEST --project-root PROJECT`, requiring exact pinned PROJECT/why3find.json. Use actual Cargo/Why3 project root, not a template directory substituted for the proof project. See final Task2 report/API for paths.

Final Task2 fix image: `hoimin-issue999-creusot-arm64:fix1-local`, ID `sha256:f1eec1050faa3c0af5e540eabc114c165c308abfe4fcdc585e3437b1a23e8603`. The earlier `:local` image lacks the selected-project preflight fix. This image reuses verified tool layers and has the corrected final-stage configuration/checker. It passed actual selected-project validation plus all three target first/fresh-warm sample proofs; these are environment evidence only. Do not rebuild the tools without concrete need. Docker binary `/Users/hayao/.rd/bin/docker`; keep unrelated 13 containers intact.

Read the approved spec mathematical clauses and final Task2 report; do not read the whole multi-task plan. Task3 starts after Task2 scoped review is approved. Stable checks use existing venv PATH `/Users/hayao/RustroverProjects/hoimin/.venv/bin`, and task-owned targets/caches. Persist logs/commands under owned task3 evidence paths, summarize exact failures and fixes; report at least every meaningful milestone.
