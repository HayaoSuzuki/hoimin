# Result-Lifecycle Deterministic Stop Fixture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `stop_preserves_accepted` stop the real CLI only after its first killed result is durably committed, eliminating the CI load race without changing production semantics.

**Architecture:** Keep the existing Lean corpus and public observation path unchanged. In the Unix test adapter, replace the one-second total-timeout trigger with a bounded SQLite readiness poll followed by SIGINT; preserve the current total-timeout fixture on Windows. An intentional first-mutant delay makes the old elapsed-time trigger fail deterministically and proves the new condition-based trigger.

**Tech Stack:** Rust, Tokio process control, rusqlite, Unix `libc::SIGINT`, Cargo nightly shuffled tests, GitHub Actions.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-report-sequence-audit` on branch `audit/lean-report-sequence`.
- Modify only `crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs` plus the audit report; do not change production Rust, Lean models/corpora, or CI configuration.
- On Unix, the semantic stop condition is exactly one durable result with status `killed`; merely observing an execution marker is insufficient.
- Detect premature CLI exit while waiting and bound both readiness and graceful shutdown.
- Every error path must terminate/reap the process tree and retain stdout/stderr/cleanup diagnostics as an infrastructure error.
- Windows retains the existing `--total-timeout 1s` path.
- Do not weaken `stop_preserves_accepted` expectations or mismatch classification.
- Keep the worktree-local `.venv` symlink untracked and unstaged.

---

### Task 1: Deterministic Unix stop trigger

**Files:**
- Modify: `crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs:350-800`

**Interfaces:**
- Consumes: existing `Fixture`, `FixtureRun`, `cleanup_cli_process_tree`, `read_output`, `join_output`, and `run_cli` observation contract.
- Produces: `build_cli_command(...) -> Result<tokio::process::Command, String>`, Unix-only `wait_for_first_durable_kill(...) -> Result<(), String>`, Unix-only `run_cli_after_first_durable(...) -> Result<FixtureRun, String>`, and the unchanged `execute_strict(...) -> Result<(ImplementationObservation, bool), String>` behavior.

- [ ] **Step 1: Add an intentional delay that exposes the elapsed-time race**

In the two-mutant Python fixture, delay only the first mutated execution before imports:

```rust
let first_mutant_delay = if two_mutants && cfg!(unix) {
    "if mutated_first:\n    time.sleep(2)\n"
} else {
    ""
};
```

Insert `{first_mutant_delay}` after `mutated_first` is computed and before the
`try` block. Keep the second mutant's existing 20-second sleep. This makes the
current `--total-timeout 1s` path stop before `m0` is accepted.

- [ ] **Step 2: Run the unchanged strict case to verify RED**

Run:

```bash
HOIMIN_RESULT_LIFECYCLE_CASE=stop_preserves_accepted \
cargo +nightly-2026-07-27 test -p hoimin-cli \
  --test lean_result_lifecycle_oracle \
  result_lifecycle_oracle_correspondence -- \
  -Z unstable-options --shuffle-seed 1786600918309127514 --exact --nocapture
```

Expected: FAIL as a semantic mismatch. The actual observation has no executed
or durable `m0`, demonstrating that the existing elapsed-time trigger cannot
establish the case premise. If it fails for compilation, missing `.venv`, or
another infrastructure reason, repair setup and rerun until the intended
mismatch is observed.

- [ ] **Step 3: Extract command construction without changing normal behavior**

Move argument and `tokio::process::Command` construction out of `run_cli`:

```rust
fn build_cli_command(
    fixture: &Fixture,
    two_mutants: bool,
    resume: bool,
    metrics_path: &Path,
    total_timeout: bool,
) -> Result<tokio::process::Command, String> {
    let python = python_executable()?;
    let mut args = vec![
        OsString::from("run"),
        OsString::from("--root"),
        fixture.root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--operators"),
        OsString::from("binary_add_sub"),
        OsString::from("--jobs"),
        OsString::from("1"),
        OsString::from("--max-mutants"),
        OsString::from(if two_mutants { "2" } else { "1" }),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--metrics"),
        metrics_path.as_os_str().to_owned(),
    ];
    if fixture.session.exists() || resume {
        args.extend([
            OsString::from("--session"),
            fixture.session.as_os_str().to_owned(),
        ]);
    }
    if resume {
        args.push(OsString::from("--resume"));
    }
    if total_timeout {
        args.extend([
            OsString::from("--total-timeout"),
            OsString::from("1s"),
            OsString::from("--mutant-timeout"),
            OsString::from("30s"),
        ]);
    }
    args.extend([
        OsString::from("--"),
        python.into_os_string(),
        OsString::from("-c"),
        OsString::from(fixture.test_command(two_mutants)),
    ]);
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    Ok(command)
}
```

Make `run_cli` call `build_cli_command` and then the existing
`bounded_cli_output`. Run the focused non-stop case to prove extraction did
not change the ordinary path:

```bash
HOIMIN_RESULT_LIFECYCLE_CASE=session_complete \
cargo test -p hoimin-cli --test lean_result_lifecycle_oracle \
  result_lifecycle_oracle_correspondence -- --exact --nocapture
```

Expected: PASS.

- [ ] **Step 4: Implement the bounded durable-result readiness poll**

Add a Unix-only helper that never creates a missing database, tolerates schema
creation and transient SQLite busy errors, and checks for premature exit on
each iteration:

```rust
#[cfg(unix)]
async fn wait_for_first_durable_kill(
    child: &mut tokio::process::Child,
    session: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("inspect hoimin CLI while waiting for durable result: {error}"))?
        {
            return Err(format!(
                "hoimin CLI exited before the first durable killed result: {status}"
            ));
        }
        let ready = session.is_file()
            && Connection::open(session)
                .and_then(|connection| {
                    connection.query_row(
                        "SELECT COUNT(*) = 1 AND MIN(status) = 'killed' AND MAX(status) = 'killed' FROM results",
                        [],
                        |row| row.get::<_, bool>(0),
                    )
                })
                .unwrap_or(false);
        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("timed out waiting for the first durable killed result".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
```

Keep the 20 ms polling interval and use a 12-second readiness bound at the
call site. Do not treat a missing table or transient lock as semantic failure;
the deadline and premature-exit branch produce the bounded diagnosis.

- [ ] **Step 5: Implement SIGINT execution with cleanup-complete errors**

Add an Unix-only runner which starts the command without total timeout, takes
and drains both output pipes, waits for readiness, then signals the retained
root PID:

```rust
#[cfg(unix)]
async fn run_cli_after_first_durable(
    fixture: &Fixture,
    metrics_path: &Path,
) -> Result<FixtureRun, String> {
    let mut command = build_cli_command(fixture, true, false, metrics_path, false)?;
    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn hoimin CLI: {error}"))?;
    let stdout = child.stdout.take().ok_or_else(|| "hoimin CLI stdout was not piped".to_owned())?;
    let stderr = child.stderr.take().ok_or_else(|| "hoimin CLI stderr was not piped".to_owned())?;
    let stdout = tokio::spawn(read_output(stdout));
    let stderr = tokio::spawn(read_output(stderr));

    let trigger = async {
        wait_for_first_durable_kill(&mut child, &fixture.session, Duration::from_secs(12)).await?;
        let pid = child.id().and_then(|pid| i32::try_from(pid).ok())
            .ok_or_else(|| "hoimin CLI exited before SIGINT".to_owned())?;
        // SAFETY: pid is the retained CLI child, isolated into its own process group.
        if unsafe { libc::kill(pid, libc::SIGINT) } != 0 {
            return Err(format!("SIGINT failed: {}", std::io::Error::last_os_error()));
        }
        tokio::time::timeout(Duration::from_secs(12), child.wait())
            .await
            .map_err(|_| "hoimin CLI did not stop after SIGINT".to_owned())?
            .map_err(|error| format!("wait for hoimin CLI after SIGINT: {error}"))
    }
    .await;

    match trigger {
        Ok(status) => {
            let stdout = join_output(stdout, "stdout").await?;
            let stderr = join_output(stderr, "stderr").await?;
            fixture_run_from_output(Output { status, stdout, stderr })
        }
        Err(error) => {
            let cleanup = cleanup_cli_process_tree(&mut child).await;
            let stdout = join_output(stdout, "stdout").await;
            let stderr = join_output(stderr, "stderr").await;
            Err(format!(
                "condition-triggered stop failed: {error}; cleanup: {cleanup}; stdout: {stdout:?}; stderr: {stderr:?}"
            ))
        }
    }
}
```

Factor the final `Output -> FixtureRun` parsing into
`fixture_run_from_output(output: Output) -> Result<FixtureRun, String>` so
both runners preserve identical report parsing. Do not duplicate or relax the
JSON checks. If taking either configured output pipe unexpectedly fails,
invoke `cleanup_cli_process_tree`, drain any pipe already taken, and return an
infrastructure error containing the cleanup result; do not rely only on
`kill_on_drop`.

- [ ] **Step 6: Select the new runner only on Unix**

Replace only the `stop_preserves_accepted` match arm:

```rust
"stop_preserves_accepted" => {
    #[cfg(unix)]
    {
        run_cli_after_first_durable(&fixture, metrics_path).await?
    }
    #[cfg(windows)]
    {
        run_cli(&fixture, true, false, metrics_path, true).await?
    }
}
```

All other cases continue through `run_cli` unchanged.

- [ ] **Step 7: Run focused GREEN checks**

Run the formerly failing case three times, including the recorded seed:

```bash
for attempt in 1 2 3; do
  HOIMIN_RESULT_LIFECYCLE_CASE=stop_preserves_accepted \
  cargo +nightly-2026-07-27 test -p hoimin-cli \
    --test lean_result_lifecycle_oracle \
    result_lifecycle_oracle_correspondence -- \
    -Z unstable-options --shuffle-seed 1786600918309127514 --exact --nocapture || exit 1
done
```

Expected: all three runs PASS; `m0` is killed/executed/durable, `m1` is
`not_run`, and exit code is 4.

Then run the complete integration binary:

```bash
cargo +nightly-2026-07-27 test -p hoimin-cli \
  --test lean_result_lifecycle_oracle -- \
  -Z unstable-options --shuffle-seed 1786600918309127514
```

Expected: 6/6 PASS.

- [ ] **Step 8: Format, inspect the minimal diff, and commit**

Run:

```bash
cargo fmt --all -- --check
git diff --check
git diff -- crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs
```

If formatting is required, run `cargo fmt --all`, re-run the two checks, and
confirm the diff contains only the intentional fixture changes. Commit:

```bash
git add crates/hoimin-cli/tests/lean_result_lifecycle_oracle.rs
git commit -m "test: make result lifecycle stop fixture deterministic"
```

---

### Task 2: Audit documentation and complete verification

**Files:**
- Modify: `docs/superpowers/reports/2026-08-13-lean-report-sequence-audit.md`

**Interfaces:**
- Consumes: Task 1's deterministic fixture and verification evidence.
- Produces: an audit report that distinguishes the unrelated CI fixture race from report-sequence correspondence and records the final repair and checks.

- [ ] **Step 1: Record the CI follow-up without changing the audit conclusion**

Add a short `CI follow-up` paragraph under `Rust observations and
reconciliation` stating:

```markdown
After PR creation, the nightly shuffled workspace job exposed an unrelated
load race in the pre-existing result-lifecycle `stop_preserves_accepted`
fixture: its one-second total timeout could fire before the first accepted
result. The report-sequence suite passed. The Unix fixture now waits for the
first killed result to be durably committed before sending SIGINT; the Lean
expectation and production Rust are unchanged.
```

Add the recorded-seed and full shuffled workspace commands to verification.

- [ ] **Step 2: Run focused audit regression checks**

Run:

```bash
cargo test -p hoimin-core --test lean_report_sequence_oracle -- --nocapture
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-core --all-features --test report_policy
```

Expected: 5/5, 22/22, and 13/13 PASS respectively.

- [ ] **Step 3: Run repository quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
cargo +nightly-2026-07-27 test --workspace -- \
  -Z unstable-options --shuffle-seed 1786600918309127514
git diff --check
```

Expected: every command exits zero. The nightly command may emit existing
nightly deprecation/future-compatibility warnings, but no test failure.

- [ ] **Step 4: Self-review documentation and tracked state**

Run:

```bash
rg -n "T[B]D|T[O]DO|FIXME|implement lat[e]r|fill in" \
  docs/superpowers/specs/2026-08-13-result-lifecycle-stop-fixture-design.md \
  docs/superpowers/plans/2026-08-13-result-lifecycle-stop-fixture.md \
  docs/superpowers/reports/2026-08-13-lean-report-sequence-audit.md
rg -n "sorry|admit|axiom" \
  formal/HoiminOracle/HoiminOracle/ReportSequence\*.lean \
  formal/HoiminOracle/ReportSequenceAuditMain.lean
git diff --check
git status --short
```

Expected: no unresolved placeholders, no Lean proof escape hatches, no
whitespace errors, and only the report plus untracked `.venv` before commit.

- [ ] **Step 5: Commit documentation and push the repaired PR**

Run:

```bash
git add docs/superpowers/reports/2026-08-13-lean-report-sequence-audit.md
git commit -m "docs: record deterministic CI stop fixture"
git push
gh pr checks 304
```

Expected: push succeeds, PR #304 points at the new commits, and CI starts for
the updated head. Preserve the worktree for review follow-up.
