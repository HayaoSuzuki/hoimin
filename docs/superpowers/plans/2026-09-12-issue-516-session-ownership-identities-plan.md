# Issue 516: Shared session ownership-tree identities Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Prevent hoimin's own aliased session lock writes from causing `workspace.original.changed`, while retaining source protection and metrics collision rejection.

**Architecture:** Add a fallible ownership-tree accessor to `SessionArtifacts`. Workspace session preflight and metrics inspection consume its lexical and canonical trees at their existing I/O boundaries.

**Tech Stack:** Rust, std filesystem APIs, existing workspace literal exclusions, CLI integration tests, CPython 3.14.

**Spec:** [Approved design](../specs/2026-09-12-issue-516-session-ownership-identities-design.md).

## Global Constraints

- Root began execution on 2026-09-14 after #513/#514/#515 were merged. Do not overlap Cargo processes or delegate without authorization.
- Use the controller-approved shared target, `CARGO_INCREMENTAL=0`, existing CPython 3.14, bounded temporary fixtures, and existing timeouts. Do not create another target directory.
- Record real RED/GREEN, command exit codes, test counts, platform limitations, and three implementation plus three test self-reviews. Do not claim an unexecuted native check passed.
- The original preparation-agent publication restriction ended when root resumed this issue. Root owns commit and draft PR publication under the existing issue workflow; do not merge, run cargo-mutants, or increase Lean resource limits.

---
### Task 1: Capture the public regression and filesystem controls

**Files:** `crates/hoimin-cli/tests/run_e2e.rs`, `crates/hoimin-cli/tests/session_handler.rs`.

- [x] Reuse active-session fixtures and bounded Python helpers. Add a test named `active_session_aliased_lock_tree_is_excluded` for jobs 1 and 2: write `calc.py` with `value = True`, create `actual-locks`, symlink `.session.db.hoimin-locks` to it, and run with session `session.db`, file `calc.py`, and `boolean_literal`. Assert baseline Exit(0), complete true, expected candidate count/status, and unchanged source bytes. Capture current failure before edits: baseline absent and `workspace.original.changed` naming the actual lock file.
- [x] Run `cargo test -p hoimin-cli --test run_e2e active_session_aliased_lock_tree_is_excluded -- --nocapture` with the approved environment. Record RED as the actual assertion failure, not a compilation/setup failure.
- [x] Add direct accessor tests for a missing tree (no creation), existing ordinary tree (dedup), actual directory alias (both identities), and non-NotFound error propagation using a supported native fixture. Resolve actual filesystem paths independently in the oracle.

### Task 2: Share ownership-tree resolution at existing preflight boundaries

**Files:** `crates/hoimin-cli/src/session/mod.rs`, `crates/hoimin-cli/src/shell.rs`, `crates/hoimin-cli/src/metrics_destination.rs`.

- [x] Add documented `SessionArtifacts::lock_trees` beside `lock_directory`; preserve constructors and `lock_directory`:

```rust
pub fn lock_trees(&self) -> std::io::Result<Vec<PathBuf>> {
    let lexical = self.lock_directory();
    let mut trees = vec![lexical.clone()];
    match std::fs::canonicalize(&lexical) {
        Ok(actual) if actual != lexical => trees.push(actual),
        Ok(_) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(error),
    }
    Ok(trees)
}
```

- [x] In `prepare_session_artifacts`, resolve the trees with the existing string error boundary, then chain `trees.into_iter().map(|path| (path, true))` after the existing four exact files. Map each identity independently through `relative_inside` and existing UTF-8/portable-path validation. Do not first discard an outside-root lexical identity's canonical partner.
- [x] Replace private lock-tree canonicalization in metrics `session_artifacts` with `let trees = artifacts.lock_trees()?;`. Preserve configured DB entry and Windows companion protection, confirmed collisions, and withheld output on uncertainty.
- [x] Rerun Task 1 tests to GREEN. Inspect callers to ensure no extra tree I/O entered context constructors or session open, and actual ownership acquisition still uses the original lock namespace.

### Task 3: Check cross-feature boundaries without broad exemptions

**Files:** `crates/hoimin-cli/tests/run_e2e.rs`, `crates/hoimin-cli/tests/metrics_destinations.rs`, `crates/hoimin-cli/tests/session_handler.rs`; existing shell/workspace tests as needed.

- [x] Extend public alias coverage with `--include '**'`, a readable ordinary fixture DB, and a similarly named directory. Baseline must observe ordinary fixtures; worker copies must omit the actual active lock tree. Verify candidates outside that tree remain.
- [x] Add both directions: lexical outside root / canonical inside, and lexical inside / canonical outside. Assert only root-contained active identities become exclusions. Add native case-insensitive existing uppercase lock-directory spelling when the filesystem supports it; explicitly report capability and unexecuted platforms.
- [x] Retain positive controls for missing/ordinary trees, DB-parent/root aliases and literal metacharacter DB names. Preserve root/ancestor exclusion validity: never authorize excluding every selected source. Test deterministic invalid cases at the existing preflight boundary.
- [x] Use existing bounded resume fixtures with a natural incomplete run containing a reusable killed result and retried timeout through the alias tree. Rerun existing ownership-contention and source/ordinary-fixture edit regressions; they must still detect real edits and competing ownership.
- [x] In metrics tests, assert lexical and canonical active-tree destinations reject before baseline with no output writes. Retain normal destination and distinct safe leaf-link positives. Do not turn safe replacement of a distinct symlink/hardlink directory entry into a new collision rule.
- [x] Run `cargo test -p hoimin-cli --test session_handler`, `cargo test -p hoimin-cli --test metrics_destinations`, and `cargo test -p hoimin-cli --test run_e2e active_session`, serially. Run `cargo test -p hoimin-cli --test workspace_handler literal_exclusions_distinguish_files_and_trees_and_reject_invalid_paths` and `cargo test -p hoimin-cli fingerprint_recheck_`, serially. Inspect the existing context-construction test and add an assertion that constructing a context leaves the configured session tree absent if that case is not already covered. The full session_handler module includes the existing ownership-contention tests.

### Task 4: Complete verification and reviewable handoff

**Files:** this plan/spec and `docs/knowledge/design/session-report.md`, `docs/knowledge/references/design-documents.md` for evidence updates only.

- [x] After focused GREEN, run serial `cargo test --workspace --all-features`, `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Preserve exact failures if any; fix only within the approved scope.
- [x] Perform three implementation reviews: I/O phase/error contracts; alias identity and exclusion scope; metrics/ownership correspondence. Perform three test reviews: observed RED/oracle; negative controls/capability reporting; full-suite/platform evidence.
- [x] Update source hashes and evidence scope in the OKF draft after source changes. Record a handoff report with files, commands/counts, limitations and conservative cases. No Lean addition is planned: this change introduces no state transition and requires native filesystem identity evidence.

## Execution evidence (2026-09-14)

Before implementation, root honored the requested local cleanup: deleted 59 merged local branches (22 confirmed by ancestry, 37 by an exact merged PR head plus merge commit in main) and three completed worktrees. Preserved main and the uncommitted work in issues 516 and 517. Shared `.venv` contents and unrelated root files were preserved.

Updated this worktree from `61c654f` to main `6e78871` using autostash. The design index was the only conflict; retained sources and footnotes for both the integrated issues and issue 516. Existing pending tests and documentation were retained.

### Public RED/GREEN

All Cargo commands use `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target` and run serially from this worktree.

```sh
cargo test --offline -p hoimin-cli --test run_e2e active_session_aliased_lock_tree_is_excluded -- --nocapture
```

Before production edits: exit 101, two failed tests (jobs 1 and 2), baseline null, no mutants, incomplete report, exit code 2. The diagnostic named `workspace.original.changed` and `actual-locks/<hash>.lock`. After sharing the identity accessor: exit 0, both passed, baseline Exit(0), one killed boolean mutation and unchanged source.

The expanded helper uses an actual SQLite fixture opened read-only by the Python baseline, a similarly named backup directory, and an existing lock file whose bytes must remain unchanged. It requires the worker copy to omit the active tree and verifies a separate owned lock was created. `--include '**'` does not restore active artifacts. Both lexical-outside/canonical-inside and lexical-inside/canonical-outside cases pass. The native case test printed `native case-alias capability confirmed` on this macOS volume.

Natural resume uses an actual timed-out mutant and a killed mutant. It verifies the same run and candidate IDs, a reused killed result without reexecution, a retried timeout and unchanged source. The real source/ordinary fixture edit regression runs with both ordinary and aliased ownership trees. The metrics regression now checks both lexical and canonical paths into an in-root active tree and no output writes.

The first invalid-alias fixture used the wrong selected file (`calc.py` instead of the existing helper's `src/calc.py`) and failed at `target.resolve`; corrected the fixture before interpreting it as evidence. The next run showed the root-tree rejection is `workspace.path.invalid`, not `session.path`; retained production behavior and corrected the assertion. Self-referential symlinks use `session.path`. Both final negative cases reject before baseline and DB creation.

### Focused checks

- `cargo test --offline -p hoimin-cli --test session_handler`: 31 passed, 1 ignored; includes accessor error propagation, no creation, dedup, alias identity and ownership contention.
- `cargo test --offline -p hoimin-cli --test metrics_destinations`: 25 passed; includes collision preservation and safe distinct leaf replacement controls.
- `cargo test --offline -p hoimin-cli --test run_e2e active_session -- --nocapture`: 10 passed, including native case, root boundaries, real edits and natural resume.
- `cargo test --offline -p hoimin-cli --test workspace_handler literal_exclusions_distinguish_files_and_trees_and_reject_invalid_paths`: 1 passed.

### Three implementation reviews

1. I/O and errors: only the two existing blocking consumers call `lock_trees`. Constructors and SessionHandler opening do not gain tree resolution or creation. NotFound remains allowed and other native errors propagate. The accessor's self-link test compares the real OS error.
2. Identity and scope: both identities are mapped independently against the canonical project root; a lexical path outside root does not hide an in-root referent. Root-wide exclusion fails rather than removing selected sources. Exact files and trees do not become filename-prefix rules.
3. Metrics and ownership: both features share the same accessor; Windows configured companion protection and safe distinct entry semantics are unchanged. Actual ownership still acquires the original lock namespace and creates its lock while preserving preexisting bytes.

### Three test reviews

1. The original failure was a public runtime assertion on jobs 1 and 2, not a missing-method compile failure. Fixed runs verify baseline, mutation status, completeness, source bytes, copied fixture contents and an actual new ownership lock.
2. Negative controls preserve source/fixture integrity, ownership contention, alias errors, root-exclusion rejection and metrics collisions. Fixed two fixture expectations as described above without weakening production validation. Native case support is reported explicitly.
3. Affected suites exercise new/existing and resumed sessions, alias directions, literal metacharacters and DB/root aliases. Local platform claims are limited to macOS; CI and native Windows/Linux execution are not claimed here. There are no new Python production functions or formal state transitions requiring mutation/Lean additions.

### Documentation checks and tool feedback

- `cargo test --offline -p hoimin-cli fingerprint_recheck_`: five matching tests passed. Zero-test groups from other integration targets were not counted as coverage.
- `cargo fmt --all -- --check`: passed.
- Strict Clippy initially requested an `# Errors` section for the new public accessor and found the expanded CLI helper over the line limit. Added the error contract and compacted static argv construction without changing arguments. Final `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings` passed.
- `.venv/bin/python -m unittest discover -s tests -p 'test_*.py' -v`: 66 passed in 18.419 s with CPython 3.14.7. Ran outside the sandbox so the existing process-monitor tests can use `ps`.
- OKF review 1: safe-parsed all 16 knowledge Markdown documents and checked required type/version metadata; preserved draft status and unknown keys.
- OKF review 2: checked unique source IDs, actual issue 516 spec hashes, matching footnotes, resolving local links and every spec's presence in the design index.
- OKF review 3: replaced the pending-design claim with observed behavior and separated macOS evidence from unexecuted native Linux/Windows checks. No historical formal result is presented as a proof of the new filesystem behavior.

### Final verification

`cargo test --offline --workspace --all-features` exited 0: 1,766 passed, zero failed, 13 ignored across 76 result groups. This ran after the final ownership-lock assertion and CLI helper cleanup, and includes the no-project-I/O constructor regression. The root native case and all affected integration cases passed with the final production code. No native Linux/Windows job was run locally. The implementation is ready for a draft PR; merging remains a separate user action.
