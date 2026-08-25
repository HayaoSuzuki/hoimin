# Issue #338 cgroup OOM classification implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Linux hard-resource classification report `OutOfMemory` only when the supervised root cgroup records a new `memory.events:oom_kill` event.

**Architecture:** Keep the parser, counter snapshots, and atomic sticky signal. Move the two bit constants into Linux/test scope, reduce the delta predicate to `oom_kill` and `pids.events:max`, and route final mapping through one pure truth-table helper. Add deterministic unit coverage, one delegated-cgroup correspondence fixture, and corrected README wording.

**Tech Stack:** Rust 2024, libc, Tokio, Linux cgroup v2, cargo-mutants 27.1.0, Markdown.

**Spec:** `docs/superpowers/specs/2026-08-25-issue-338-cgroup-oom-kill-design.md`

## Global Constraints

- Work in `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-338-cgroup-oom-kill` on `fix/issue-338-cgroup-oom-kill`.
- Preserve all `CgroupEventCounters` fields and `parse_cgroup_event_counters` behavior.
- Treat only a strict `oom_kill` increase as memory evidence and a strict `pids_max` increase as process evidence.
- Preserve memory precedence, ignore unknown bits, and preserve the incoming termination when no known bit is set.
- Keep cgroup setup, launcher migration, sticky `fetch_or`, atomic ordering, cleanup, schemas, and resource mode unchanged.
- Do not repair stale PR #377 fixtures or the per-root normalization diagnostic regression.
- Follow RED/GREEN TDD and commit only green tasks.
- Run focused and workspace cargo-mutants 27.1.0 without `--iterate`; store artifacts under `/tmp`.
- Execute this plan only after this plan file itself is committed; every verification and review record must name the tested `HEAD` SHA.

---

### Task 1: Encode terminal cgroup evidence as a tested truth table

**Files:**
- Modify: `crates/hoimin-cli/src/resource/linux.rs:5-12`
- Modify: `crates/hoimin-cli/src/resource/linux.rs:185-193`
- Modify: `crates/hoimin-cli/src/resource/linux.rs:431-440`
- Modify: `crates/hoimin-cli/src/resource/linux.rs:792-812`
- Test: `crates/hoimin-cli/src/resource/linux.rs:1736-1758`

**Interfaces:**
- Consumes: `CgroupEventCounters` and `ProcessTermination`.
- Produces: `MEMORY_VIOLATION`, `PROCESS_VIOLATION`, `BOTH_VIOLATIONS`, `violations_since`, and `classify_violations` in `cfg(any(target_os = "linux", test))` scope.
- Preserves: `RootSignal::violations` and `refresh_events` accumulation.

- [ ] **Step 1: Write failing delta tests**

Replace the current event-delta test with the following. Keep numeric expectations during RED so the test compiles before the constants move into shared scope:

```rust
#[test]
fn pressure_counters_do_not_create_terminal_memory_evidence() {
    let before = CgroupEventCounters {
        memory_max: 5,
        oom: 7,
        oom_kill: 11,
        oom_group_kill: 13,
        pids_max: 17,
    };
    let observations = [
        CgroupEventCounters { memory_max: 6, ..before },
        CgroupEventCounters { oom: 8, ..before },
        CgroupEventCounters { oom_group_kill: 14, ..before },
        CgroupEventCounters {
            memory_max: 6,
            oom: 8,
            oom_group_kill: 14,
            ..before
        },
    ];
    for after in observations {
        assert_eq!(violations_since(before, after), 0);
    }
}

#[test]
fn terminal_event_deltas_use_strict_monotonic_increases() {
    let before = CgroupEventCounters {
        memory_max: 5,
        oom: 7,
        oom_kill: 11,
        oom_group_kill: 13,
        pids_max: 17,
    };
    assert_eq!(violations_since(before, before), 0);
    assert_eq!(
        violations_since(before, CgroupEventCounters { oom_kill: 12, ..before }),
        1,
    );
    assert_eq!(
        violations_since(before, CgroupEventCounters { pids_max: 18, ..before }),
        2,
    );
    assert_eq!(
        violations_since(
            before,
            CgroupEventCounters { oom_kill: 12, pids_max: 18, ..before },
        ),
        3,
    );
    assert_eq!(
        violations_since(
            before,
            CgroupEventCounters { oom_kill: 10, pids_max: 18, ..before },
        ),
        2,
    );
    assert_eq!(
        violations_since(
            before,
            CgroupEventCounters { oom_kill: 12, pids_max: 16, ..before },
        ),
        1,
    );
}
```

- [ ] **Step 2: Run the delta tests and capture RED**

Run:

```console
cargo test -p hoimin-cli resource::linux::tests::pressure_counters_do_not_create_terminal_memory_evidence -- --exact
cargo test -p hoimin-cli resource::linux::tests::terminal_event_deltas_use_strict_monotonic_increases -- --exact
```

Expected: the pressure test fails because current code returns memory evidence. The terminal test passes, proving the existing `oom_kill` and `pids_max` positive paths before production changes.

- [ ] **Step 3: Implement the minimal delta predicate and behavior-preserving classifier**

Move the constants beside `INTERNAL_LAUNCHER_ARG`, add a cfg-scoped top-level `hoimin_core::ProcessTermination` import for the parent helper, remove the constant copies inside `platform`, retain `platform`'s direct `ProcessTermination` import for its own type signatures, and import the constants into the test module. Replace the RED test's numeric `1`, `2`, and `3` expectations with the named constants:

```rust
#[cfg(any(target_os = "linux", test))]
const MEMORY_VIOLATION: u8 = 1;
#[cfg(any(target_os = "linux", test))]
const PROCESS_VIOLATION: u8 = 2;
#[cfg(any(target_os = "linux", test))]
const BOTH_VIOLATIONS: u8 = 3;

#[cfg(any(target_os = "linux", test))]
fn violations_since(before: CgroupEventCounters, after: CgroupEventCounters) -> u8 {
    let memory = after.oom_kill > before.oom_kill;
    let processes = after.pids_max > before.pids_max;
    match (memory, processes) {
        (false, false) => 0,
        (true, false) => MEMORY_VIOLATION,
        (false, true) => PROCESS_VIOLATION,
        (true, true) => BOTH_VIOLATIONS,
    }
}
```

At the same time, extract the existing `classify_root` conditional into this helper without changing its behavior:

```rust
#[cfg(any(target_os = "linux", test))]
fn classify_violations(termination: ProcessTermination, violations: u8) -> ProcessTermination {
    let memory = violations & MEMORY_VIOLATION != 0;
    let processes = violations & PROCESS_VIOLATION != 0;
    match (memory, processes) {
        (false, false) => termination,
        (true, false) | (true, true) => ProcessTermination::OutOfMemory,
        (false, true) => ProcessTermination::ProcessLimit,
    }
}
```

Import `classify_violations` into `platform`; do not import the three constants there. Replace the conditional in `classify_root` with:

```rust
let violations = signal.violations.load(Ordering::Acquire);
Ok(classify_violations(termination, violations))
```

- [ ] **Step 4: Run both delta tests and confirm GREEN**

Run both commands from Step 2. Expected: PASS.

- [ ] **Step 5: Add the exhaustive classifier regression table**

Import `ProcessTermination` and `classify_violations` into the test module, and add:

```rust
#[test]
fn known_violation_bits_map_with_memory_precedence() {
    let incoming = ProcessTermination::Exit(7);
    for (bits, expected) in [
        (0, incoming),
        (MEMORY_VIOLATION, ProcessTermination::OutOfMemory),
        (PROCESS_VIOLATION, ProcessTermination::ProcessLimit),
        (BOTH_VIOLATIONS, ProcessTermination::OutOfMemory),
        (4, incoming),
        (4 | MEMORY_VIOLATION, ProcessTermination::OutOfMemory),
        (4 | PROCESS_VIOLATION, ProcessTermination::ProcessLimit),
        (4 | BOTH_VIOLATIONS, ProcessTermination::OutOfMemory),
    ] {
        assert_eq!(classify_violations(incoming, bits), expected);
    }
}
```

- [ ] **Step 6: Run the classification table**

Run:

```console
cargo test -p hoimin-cli resource::linux::tests::known_violation_bits_map_with_memory_precedence -- --exact
```

Expected: PASS. This is a regression table over the behavior-preserving helper extracted during GREEN; the genuine RED was the pressure-counter test.

- [ ] **Step 7: Run focused tests, format, and commit**

Run:

```console
cargo test -p hoimin-cli resource::linux::tests::
cargo test -p hoimin-cli --test process_handler linux_policy::
cargo fmt --all
cargo fmt --all -- --check
git diff --check
```

Expected: PASS. Inspect `git diff -- crates/hoimin-cli/src/resource/linux.rs`, then commit:

```console
git add crates/hoimin-cli/src/resource/linux.rs
git commit -m "fix: classify only cgroup OOM kills as memory violations"
```

---

### Task 2: Add delegated-kernel correspondence and status documentation

**Files:**
- Modify: `crates/hoimin-cli/tests/process_handler.rs:181-560`
- Modify: `README.md:290-299`

**Interfaces:**
- Consumes: `hard_handler`, `run_python`, `limits`, and the production hard-cgroup path.
- Produces: `cgroup_v2::hard_cgroup_classifies_one_root_oom_kill`.
- Documents: `out_of_memory` as a kernel OOM kill observed in the supervised root subtree.

- [ ] **Step 1: Add a single-root delegated OOM fixture**

Add this test inside `mod cgroup_v2`; leave the stale decimal and aggregate fixtures unchanged:

```rust
#[tokio::test]
async fn hard_cgroup_classifies_one_root_oom_kill() {
    let output = tempfile::tempdir().unwrap();
    let Some(handler) = hard_handler(
        Utf8Path::from_path(output.path()).unwrap(),
        512 * 1024 * 1024,
        16,
    ) else {
        return;
    };
    let mut process_limits = limits(Duration::from_secs(5), 64);
    process_limits.max_memory_bytes = 160 * 1024 * 1024;

    let event = handler
        .handle(run_python(
            210,
            "chunks=[]\nfor _ in range(12):\n chunk=bytearray(16*1024*1024)\n for page in range(0,len(chunk),4096): chunk[page]=1\n chunks.append(chunk)",
            process_limits,
        ))
        .await
        .unwrap();

    assert_eq!(event.resource_mode, ResourceMode::Hard);
    assert_eq!(event.termination, ProcessTermination::OutOfMemory);
    handler.close().unwrap();
}
```

The fixture dirties 192 MiB, enough to exceed the 160 MiB limit, but exits normally after a bounded allocation if cgroup attachment or enforcement regresses. In that failure mode the termination assertion fails without an unbounded host allocation.

- [ ] **Step 2: Run the exact fixture and record host capability**

Run:

```console
cargo test -p hoimin-cli --test process_handler cgroup_v2::hard_cgroup_classifies_one_root_oom_kill -- --exact --nocapture
```

Record one of three states: macOS/Windows compile out `cgroup_v2`, so the filter runs zero tests and the fixture is `not compiled`; Linux without delegation passes with `SKIP:`; delegated Linux passes without `SKIP:` and asserts `OutOfMemory`. Neither `not compiled` nor `SKIP:` is hard-cgroup evidence.

- [ ] **Step 3: Capture the documentation RED**

Run against the unchanged README:

```console
test "$(rg -Fxc -- '- `out_of_memory`: the kernel reported an OOM kill in the supervised root cgroup subtree;' README.md)" -eq 1
```

Expected: nonzero because the exact corrected definition is absent.

- [ ] **Step 4: Correct the current README status definition**

Replace the existing `out_of_memory` entry with:

```markdown
- `out_of_memory`: the kernel reported an OOM kill in the supervised root cgroup subtree;
```

Do not edit neighboring run-wide wording owned by the PR #377 documentation regression.

- [ ] **Step 5: Run integration and the exact documentation consumer**

Run:

```console
cargo test -p hoimin-cli --test process_handler linux_policy::
test "$(rg -Fxc -- '- `out_of_memory`: the kernel reported an OOM kill in the supervised root cgroup subtree;' README.md)" -eq 1
cargo fmt --all
cargo fmt --all -- --check
git diff --check
```

Expected: PASS. Record whether Task 2 Step 2 skipped.

- [ ] **Step 6: Review and commit Task 2**

Inspect:

```console
git diff -- crates/hoimin-cli/tests/process_handler.rs README.md
git status --short
```

Commit:

```console
git add crates/hoimin-cli/tests/process_handler.rs README.md
git commit -m "test: cover kernel OOM kill classification"
```

---

### Task 3: Verify the branch and close mutation-test gaps

**Files:**
- Modify only for a demonstrated gap: `crates/hoimin-cli/src/resource/linux.rs`
- Modify only for a demonstrated gap: `crates/hoimin-cli/tests/process_handler.rs`
- Modify only for a reviewed exact equivalence: `.cargo/mutants.toml`
- Evidence outside repository: `/tmp/hoimin-issue-338-mutants.*`

**Interfaces:**
- Consumes: the green Task 1 and Task 2 commits.
- Produces: focused and full mutation reports plus final local verification tied to one SHA.

- [ ] **Step 1: Run the local quality and workspace gates**

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
git diff --check
```

Expected: each command exits 0.

- [ ] **Step 2: Run the focused non-iterated mutation slice**

Run:

```console
output_dir="$(mktemp -d /tmp/hoimin-issue-338-mutants.XXXXXX)"
artifact_record=/tmp/hoimin-issue-338-focused-path.txt
printf '%s\n' "$output_dir" > "$artifact_record"
filter='(violations_since|classify_violations)'
test "$(cargo mutants --version)" = 'cargo-mutants 27.1.0'
cargo mutants \
  --file crates/hoimin-cli/src/resource/linux.rs \
  --re "$filter" --list --json > "$output_dir/inventory.json"
cargo mutants \
  --file crates/hoimin-cli/src/resource/linux.rs \
  --re "$filter" --output "$output_dir"
```

Expected: `cargo mutants --version` reports exactly 27.1.0; inventory and execution names match; missed, timeout, and error counts are zero. Inspect each unviable result.

- [ ] **Step 3: Resolve focused mutation findings**

Recover the exact artifact path with `output_dir="$(sed -n '1p' /tmp/hoimin-issue-338-focused-path.txt)"`; assert both JSON files exist, then read `$output_dir/inventory.json`, `$output_dir/mutants.out/outcomes.json`, and the mutant logs. For any adverse focused outcome, add the smallest behavioral assertion, reproduce RED with that mutant, return to GREEN, and rerun Step 2 without `--iterate`. The focused gate permits no accepted survivor, timeout, or tool error.

- [ ] **Step 4: Run the required full workspace mutation inventory**

Run:

```console
full_output="$(mktemp -d /tmp/hoimin-issue-338-full-mutants.XXXXXX)"
printf '%s\n' "$full_output" > /tmp/hoimin-issue-338-full-path.txt
cargo mutants --workspace --output "$full_output"
```

Recover the report with `full_output="$(sed -n '1p' /tmp/hoimin-issue-338-full-path.txt)"`. Expected: baseline PASS. Resolve timeouts and tool errors. Fix each missed mutant. For an exact equivalent mutant, add its anchored complete name to `.cargo/mutants.toml` as an `exclude_re` with a TOML reason comment, obtain independent review, and rerun a fresh full non-iterated inventory. Inspect unviable and platform-inapplicable results. For a non-equivalent survivor outside the Issue diff, use a same-host parent-SHA run to identify it as pre-existing, then block delivery until explicit scope approval permits its fix or a separate prerequisite fix lands; in either case rerun a fresh full inventory before continuing.

If that parent comparison is needed, keep the candidate branch checked out and run the parent in a disposable worktree on the same host and cargo-mutants version:

```console
set -e
parent_root="$(mktemp -d /tmp/hoimin-issue-338-parent-worktree.XXXXXX)"
parent_tree="$parent_root/worktree"
parent_output="$(mktemp -d /tmp/hoimin-issue-338-parent-mutants.XXXXXX)"
baseline_sha="$(git merge-base HEAD origin/main)"
cleanup_parent() {
  git worktree remove --force "$parent_tree" 2>/dev/null || true
  rmdir "$parent_root" 2>/dev/null || true
}
trap cleanup_parent EXIT
printf '%s\n' "$baseline_sha" > /tmp/hoimin-issue-338-parent-sha.txt
printf '%s\n' "$parent_output" > /tmp/hoimin-issue-338-parent-path.txt
git worktree add --detach "$parent_tree" "$baseline_sha"
test "$(git -C "$parent_tree" rev-parse HEAD)" = "$baseline_sha"
uv sync --frozen --directory "$parent_tree"
test "$(cargo mutants --version)" = 'cargo-mutants 27.1.0'
cargo mutants --manifest-path "$parent_tree/Cargo.toml" --workspace --output "$parent_output"
candidate_output="$(sed -n '1p' /tmp/hoimin-issue-338-full-path.txt)"
LC_ALL=C sort "$candidate_output/mutants.out/missed.txt" > /tmp/hoimin-issue-338-candidate-missed.sorted
LC_ALL=C sort "$parent_output/mutants.out/missed.txt" > /tmp/hoimin-issue-338-parent-missed.sorted
comm -12 /tmp/hoimin-issue-338-candidate-missed.sorted /tmp/hoimin-issue-338-parent-missed.sorted > /tmp/hoimin-issue-338-preexisting-mutant-names.txt
git worktree remove --force "$parent_tree"
rmdir "$parent_root"
trap - EXIT
test -s /tmp/hoimin-issue-338-preexisting-mutant-names.txt
```

Review the complete names and corresponding candidate/parent outcomes. This comparison identifies provenance only; every non-equivalent name in the common list still blocks delivery.

- [ ] **Step 5: Run compatibility and randomized gates**

Run:

```console
cargo +1.88 check --workspace --all-targets --all-features --locked
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
uvx maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

Expected: each command exits 0. Install a missing named toolchain, then rerun the same command.

- [ ] **Step 6: Commit mutation-driven improvements, if present**

If Steps 2-4 changed tracked files, rerun Step 1 and commit the reviewed delta:

```console
git add crates/hoimin-cli/src/resource/linux.rs crates/hoimin-cli/tests/process_handler.rs .cargo/mutants.toml
git commit -m "test: strengthen cgroup violation mutation coverage"
```

If no file changed, record that Task 3 required no commit. Treat all Task 3 mutation results as exploratory until the final clean-SHA rerun in Task 4.

---

### Task 4: Review, push, and open the Issue #338 PR

**Files:**
- Review: every path changed from `origin/main`
- External evidence: local logs, focused mutation report, full mutation report, GitHub checks

**Interfaces:**
- Consumes: a clean branch with Tasks 1-3 complete.
- Produces: pushed branch `fix/issue-338-cgroup-oom-kill` and a PR using `Refs #338`.

- [ ] **Step 1: Rebase after the maintainer-authorized Issue #339 merge**

Run:

```console
git fetch origin
git rebase origin/main
```

Do not open #338 until a maintainer or the user authorizes and completes the #339 merge. Resolve the current README overlap. If both branches added reviewed mutation exclusions, preserve the union of anchored names and reason comments in `.cargo/mutants.toml`; this invalidates earlier mutation evidence and therefore returns to the fresh full gate. Stop and escalate any unexpected source-code conflict.

- [ ] **Step 2: Rerun every gate on the clean candidate SHA**

Use one shell block to record and guard the candidate SHA:

```console
set -e
candidate_sha="$(git rev-parse HEAD)"
test -z "$(git status --short)"
printf '%s\n' "$candidate_sha" > /tmp/hoimin-issue-338-candidate-sha.txt
```

Rerun every Task 3 Step 1 and Step 5 command, including the explicit `run_e2e` test. On a Linux host at this exact SHA, also run and retain the output of:

```console
set -e
set -o pipefail
cargo test -p hoimin-cli --test process_handler cgroup_v2::hard_cgroup_classifies_one_root_oom_kill -- --exact --nocapture 2>&1 | tee /tmp/hoimin-issue-338-final-cgroup.log
rg -F 'running 1 test' /tmp/hoimin-issue-338-final-cgroup.log
rg -F 'cgroup_v2::hard_cgroup_classifies_one_root_oom_kill' /tmp/hoimin-issue-338-final-cgroup.log
```

Only after both log assertions pass, record the platform state: `not compiled` on non-Linux, `SKIP:` on non-delegated Linux, or no skip on delegated Linux. Retain a Linux log when Linux is available; otherwise retain an explicit `not run on Linux` record for reviewers. Create fresh output directories and rerun Task 3 Step 2 and Step 4 without `--iterate`. The focused gate requires zero missed, timeout, and error outcomes. Apply the repository-owned anchored `exclude_re` policy to any exact full-workspace equivalence. Finish with this self-contained SHA guard:

```console
set -e
candidate_sha="$(sed -n '1p' /tmp/hoimin-issue-338-candidate-sha.txt)"
test "$(git rev-parse HEAD)" = "$candidate_sha"
test -z "$(git status --short)"
```

- [ ] **Step 3: Request independent code reviews and commit every accepted fix**

Use `superpowers:requesting-code-review`. Give reviewers the spec, this plan, `origin/main`, candidate `HEAD`, final-SHA verification logs, final focused/full mutation reports, and either the exact Linux delegated-fixture log or the explicit Linux-unavailable record. Require separate kernel-semantics, lifecycle/error-precedence, and mutation/delivery reviews. Fix findings through TDD and commit each accepted fix. Any code, test, documentation, configuration, mutation-exclusion, review-driven, or CI-driven change invalidates the evidence and approvals: return to Task 4 Step 2, then repeat independent review until every reviewer approves the same `HEAD` SHA.

- [ ] **Step 4: Verify the final branch state**

Use `superpowers:verification-before-completion`, then run:

```console
set -e
test -z "$(git status --short)"
git diff --check origin/main...HEAD
git log --oneline origin/main..HEAD
candidate_sha="$(sed -n '1p' /tmp/hoimin-issue-338-candidate-sha.txt)"
test "$(git rev-parse HEAD)" = "$candidate_sha"
```

Expected: clean status, no whitespace error, and Issue #338-only commits.

- [ ] **Step 5: Push and create the PR**

Run:

```console
git push --set-upstream origin fix/issue-338-cgroup-oom-kill
```

Create the PR with `gh pr create`. Its initial body must contain `Refs #338`, the `oom_kill` contract, memory precedence, local/native mutation reports, delegated-test skip status, stale PR #377 limitations, pending-PR-check status, and no claim that the configured limit caused each OOM kill.

- [ ] **Step 6: Watch every pull-request check**

Run:

```console
gh pr checks --watch
```

Expected: `quality` on Ubuntu, Windows, and macOS; `msrv`; `rust` on Ubuntu, Windows, and macOS; `rust-shuffle`; `contracts`; `core-dependency-purity` on Ubuntu and Windows; `wheel-smoke` on Ubuntu, Windows, and macOS; and `linux-best-effort` all pass. The main-only `linux-cgroup-v2-hard` job is not a PR check. State that it and the stale PR #377 suite cannot supply pre-merge closure evidence. If a CI-driven change is required, implement it through TDD, commit it, and return to Task 4 Step 2 before pushing the new SHA.

After checks pass, verify the remote PR head and attach the check evidence:

```console
set -e
candidate_sha="$(sed -n '1p' /tmp/hoimin-issue-338-candidate-sha.txt)"
test "$(gh pr view --json headRefOid --jq .headRefOid)" = "$candidate_sha"
gh pr checks
```

Update the PR body or add a PR comment with the final SHA and complete check results; do not describe CI as complete before this step.

## Plan review record

- Self-review separated the genuine behavioral RED from helper extraction, made the `BOTH_VIOLATIONS` path explicit for mutation testing, and kept the change within the approved parser/classification boundary.
- Independent review replaced the unbounded OOM fixture with a 192 MiB dirty-page fixture, corrected module import ownership, and added non-Linux/Linux-skip/Linux-delegated evidence states.
- Delivery review made all mutation artifacts recoverable, required cargo-mutants 27.1.0, applied the repository's anchored-exclusion policy, and made non-equivalent survivors delivery blockers.
- Final review moved every native, compatibility, focused-mutation, full-mutation, and delegated-fixture result ahead of same-SHA independent approval; SHA and remote-head guards are fail-closed.
- Four review rounds completed. The final independent kernel and delivery reviews reported no remaining blocker, high, or medium findings.
