# Issue #339 portable Unix timeout implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Hoimin-installed `RLIMIT_CPU` from portable Unix children while preserving wall-clock timeout, process-group cleanup, inherited CPU policy, and native exit reporting.

**Architecture:** Delete CPU-limit setup from the two Unix `configure_command` branches. Keep Linux/other-Unix `RLIMIT_AS`, keep `setpgid` on Unix, and leave `ProcessHandler` deadline selection untouched. Prove deletion with a child-only pre-exec inheritance fixture, pin ambiguous exit behavior, and correct live diagnostics and README claims.

**Tech Stack:** Rust 2024, libc rlimits/signals, Tokio process supervision, Python 3.14 fixtures, cargo-mutants 27.1.0, Markdown.

**Spec:** `docs/superpowers/specs/2026-08-25-issue-339-remove-portable-rlimit-cpu-design.md`

## Global Constraints

- Work in `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-339-remove-portable-rlimit-cpu` on `fix/issue-339-remove-portable-rlimit-cpu`.
- Remove Hoimin `RLIMIT_CPU` reads and writes on each Unix cfg branch; preserve values held by the spawning Hoimin process.
- Keep Linux/other-Unix `setpgid` and `RLIMIT_AS`; keep macOS `setpgid`; keep Windows and fallback branches unchanged.
- Do not modify `ProcessHandler::run`, `select_process_result`, `terminate_and_reap`, output finalization, timeout formulas, schemas, or resource mode.
- Preserve cancellation before deadline before child-completion selection.
- Preserve explicit exit 152 and SIGXCPU as native `Exit` values.
- Follow RED/GREEN TDD. Never change the shared Rust test process's hard rlimit.
- Run focused native macOS/Linux and workspace cargo-mutants 27.1.0 without `--iterate`; store artifacts under `/tmp`.
- Update both live README CPU-limit claims; leave archival specs and plans unchanged.
- Execute this plan only after this plan file itself is committed; every verification and review record must name the tested `HEAD` SHA.

---

### Task 1: Prove CPU-limit inheritance and remove the portable override

**Files:**
- Modify: `crates/hoimin-cli/src/resource/portable.rs:300-443`

**Interfaces:**
- Consumes: `PortableBackend::prepare`, the returned `ProcessSupervisor::{attach, terminate}` dispatch interface, and Unix `configure_command`.
- Produces: `unix_tests::rlimit_cpu_probe_fixture` and `unix_tests::portable_setup_preserves_inherited_rlimit_cpu`.
- Preserves: the production supervisor ownership protocol: attach failure kills/reaps; success reaps then `terminate(false)`; timeout/wait error calls `terminate(true)` before reap.

- [ ] **Step 1: Add imports and a child-output reader in `unix_tests`**

Extend the module imports with `std::mem::MaybeUninit`, `std::os::unix::process::CommandExt`, `hoimin_core::EffectId`, `tokio::io::{AsyncRead, AsyncReadExt}`, `tokio::process::Child`, `crate::process::wait_after_termination`, `crate::resource::ProcessSupervisor`, and `super::PortableBackend`. Add:

```rust
const EXPECTED_CPU_SOFT: &str = "HOIMIN_TEST_EXPECTED_CPU_SOFT";
const EXPECTED_CPU_HARD: &str = "HOIMIN_TEST_EXPECTED_CPU_HARD";

async fn read_pipe<R>(mut pipe: R) -> Vec<u8>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes).await.unwrap();
    bytes
}

async fn finish_pipe(
    mut task: tokio::task::JoinHandle<Vec<u8>>,
    label: &str,
    errors: &mut Vec<String>,
) -> Vec<u8> {
    match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(error)) => {
            errors.push(format!("join {label}: {error}"));
            Vec::new()
        }
        Err(_) => {
            task.abort();
            let _ = task.await;
            errors.push(format!("read {label}: timed out and aborted"));
            Vec::new()
        }
    }
}

#[cfg(target_os = "macos")]
fn fixture_memory_limit() -> u64 {
    1024 * 1024 * 1024
}

#[cfg(not(target_os = "macos"))]
#[allow(
    clippy::unnecessary_fallible_conversions,
    clippy::useless_conversion,
    reason = "libc::rlim_t signedness and width vary across supported Unix targets"
)]
fn finite_rlimit_to_u64(value: libc::rlim_t) -> u64 {
    u64::try_from(value).expect("finite inherited RLIMIT_AS must fit u64")
}

#[cfg(not(target_os = "macos"))]
fn fixture_memory_limit() -> u64 {
    let mut inherited = MaybeUninit::<libc::rlimit>::uninit();
    // SAFETY: `inherited` points to writable storage for one rlimit value.
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_AS, inherited.as_mut_ptr()) }, 0);
    // SAFETY: getrlimit succeeded and initialized the value.
    let inherited = unsafe { inherited.assume_init() };
    let hard = if inherited.rlim_max == libc::RLIM_INFINITY {
        1024 * 1024 * 1024
    } else {
        finite_rlimit_to_u64(inherited.rlim_max)
    };
    assert!(hard >= 512 * 1024 * 1024, "inherited RLIMIT_AS has no safe fixture headroom");
    hard.min(1024 * 1024 * 1024)
}

async fn cleanup_probe(
    supervisor: &mut ProcessSupervisor,
    child: &mut Child,
    terminate_group: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    if terminate_group {
        if let Err(error) = supervisor.terminate(true) {
            errors.push(format!("terminate probe group: {error}"));
            if let Err(error) = child.start_kill() {
                errors.push(format!("kill probe root: {error}"));
            }
        }
    } else if let Err(error) = child.start_kill() {
        errors.push(format!("kill unattached probe root: {error}"));
    }
    if let Err(error) = wait_after_termination(EffectId(900), child).await {
        errors.push(format!("reap probe root: {error}"));
    }
    if child.id().is_none() && let Err(error) = supervisor.terminate(false) {
        errors.push(format!("disarm reaped probe supervisor: {error}"));
    }
    errors
}
```

- [ ] **Step 2: Add the ignored limit-probe fixture**

Add:

```rust
#[test]
#[ignore = "subprocess fixture for RLIMIT_CPU inheritance"]
fn rlimit_cpu_probe_fixture() {
    let expected_soft = std::env::var(EXPECTED_CPU_SOFT)
        .unwrap()
        .parse::<libc::rlim_t>()
        .unwrap();
    let expected_hard = std::env::var(EXPECTED_CPU_HARD)
        .unwrap()
        .parse::<libc::rlim_t>()
        .unwrap();
    let mut observed = MaybeUninit::<libc::rlimit>::uninit();
    // SAFETY: `observed` points to writable storage for one rlimit value.
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_CPU, observed.as_mut_ptr()) }, 0);
    // SAFETY: getrlimit succeeded and initialized the value.
    let observed = unsafe { observed.assume_init() };

    assert_eq!(observed.rlim_cur, expected_soft);
    assert_eq!(observed.rlim_max, expected_hard);
}
```

- [ ] **Step 3: Add the child-only inheritance regression**

Add a Tokio test that follows this structure:

```rust
#[tokio::test]
async fn portable_setup_preserves_inherited_rlimit_cpu() {
    let mut inherited = MaybeUninit::<libc::rlimit>::uninit();
    // SAFETY: `inherited` points to writable storage for one rlimit value.
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_CPU, inherited.as_mut_ptr()) }, 0);
    // SAFETY: getrlimit succeeded and initialized the value.
    let inherited = unsafe { inherited.assume_init() };
    let requested_hard = if inherited.rlim_max == libc::RLIM_INFINITY {
        31
    } else {
        inherited.rlim_max.min(31)
    };
    assert!(requested_hard >= 10, "inherited RLIMIT_CPU has no safe fixture headroom");
    let requested_soft = requested_hard - 1;

    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "resource::portable::unix_tests::rlimit_cpu_probe_fixture",
            "--test-threads=1",
        ])
        .env(EXPECTED_CPU_SOFT, requested_soft.to_string())
        .env(EXPECTED_CPU_HARD, requested_hard.to_string())
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: the closure changes only the soon-to-exec child and calls libc setrlimit.
    unsafe {
        command.as_std_mut().pre_exec(move || {
            let requested = libc::rlimit {
                rlim_cur: requested_soft,
                rlim_max: requested_hard,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, std::ptr::addr_of!(requested)) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let backend = PortableBackend::for_tests();
    let mut supervisor = backend
        .prepare(
            &mut command,
            ProcessLimits {
                timeout: Duration::from_secs(5),
                max_output_bytes: 64,
                max_memory_bytes: fixture_memory_limit(),
                max_processes: 8,
            },
        )
        .unwrap();
    let mut child = command.spawn().unwrap();
    let stdout_task = tokio::spawn(read_pipe(child.stdout.take().unwrap()));
    let stderr_task = tokio::spawn(read_pipe(child.stderr.take().unwrap()));
    if let Err(error) = supervisor.attach(&child) {
        let mut cleanup = cleanup_probe(&mut supervisor, &mut child, false).await;
        let stdout = finish_pipe(stdout_task, "probe stdout", &mut cleanup).await;
        let stderr = finish_pipe(stderr_task, "probe stderr", &mut cleanup).await;
        panic!(
            "attach RLIMIT_CPU probe: {error}; cleanup={cleanup:?}; stdout={} stderr={}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
        );
    }
    let mut diagnostics = Vec::new();
    let outcome = match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
        Ok(Ok(status)) => supervisor
            .terminate(false)
            .map(|()| status)
            .map_err(|error| format!("disarm RLIMIT_CPU supervisor: {error}")),
        Ok(Err(error)) => {
            diagnostics.extend(cleanup_probe(&mut supervisor, &mut child, true).await);
            Err(format!("wait for RLIMIT_CPU probe: {error}"))
        }
        Err(_) => {
            diagnostics.extend(cleanup_probe(&mut supervisor, &mut child, true).await);
            Err("RLIMIT_CPU probe timed out".to_owned())
        }
    };
    let stdout = finish_pipe(stdout_task, "probe stdout", &mut diagnostics).await;
    let stderr = finish_pipe(stderr_task, "probe stderr", &mut diagnostics).await;
    let status = outcome.unwrap_or_else(|error| {
        panic!(
            "{error}; diagnostics={diagnostics:?}; stdout={} stderr={}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
        )
    });
    assert!(diagnostics.is_empty(), "probe diagnostics: {diagnostics:?}");
    assert!(
        status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr),
    );
}
```

- [ ] **Step 4: Run the inheritance test and capture RED**

Run:

```console
cargo test -p hoimin-cli resource::portable::unix_tests::portable_setup_preserves_inherited_rlimit_cpu -- --exact --nocapture
```

Expected: FAIL. Current production replaces the unequal pair with the five-second equal pair.

Before adding production code, save the test-only RED delta and its base identity for final reconstruction:

```console
git rev-parse HEAD > /tmp/hoimin-issue-339-red-base.txt
git diff -- crates/hoimin-cli/src/resource/portable.rs > /tmp/hoimin-issue-339-red-test.patch
shasum -a 256 /tmp/hoimin-issue-339-red-test.patch > /tmp/hoimin-issue-339-red-test.patch.sha256
shasum -a 256 -c /tmp/hoimin-issue-339-red-test.patch.sha256
```

- [ ] **Step 5: Remove both production CPU-limit blocks**

In the non-macOS Unix function, keep `let memory = limits.max_memory_bytes`, `setpgid`, and `RLIMIT_AS`. Delete `cpu_seconds`, the `cpu` rlimit, and `setrlimit(RLIMIT_CPU, ...)`.

Replace the macOS function with:

```rust
#[cfg(target_os = "macos")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the shared command configuration API retains a fallible signature across target-specific implementations"
)]
fn configure_command(command: &mut Command, _limits: ProcessLimits) -> Result<(), ResourceError> {
    use std::os::unix::process::CommandExt;

    // SAFETY: this closure changes only the soon-to-exec child process.
    unsafe {
        command.as_std_mut().pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}
```

- [ ] **Step 6: Run focused portable tests and confirm GREEN**

Run:

```console
cargo test -p hoimin-cli resource::portable::unix_tests:: -- --nocapture
cargo test -p hoimin-cli --test process_handler portable::honors_requested_timeout -- --exact
cargo test -p hoimin-cli --test process_handler portable::timeout_terminates_descendants -- --exact
```

Expected: PASS.

- [ ] **Step 7: Format, inspect, and commit Task 1**

Run:

```console
cargo fmt --all
cargo fmt --all -- --check
git diff --check
git diff -- crates/hoimin-cli/src/resource/portable.rs
```

Confirm production `portable.rs` contains no `RLIMIT_CPU` or CPU-seconds conversion; the test fixture may contain `RLIMIT_CPU`. Commit:

```console
git add crates/hoimin-cli/src/resource/portable.rs
git commit -m "fix: preserve inherited portable Unix CPU limits"
```

---

### Task 2: Pin native exits and correct live resource-control claims

**Files:**
- Modify: `crates/hoimin-cli/src/resource/portable.rs:20-55`
- Modify: `crates/hoimin-cli/src/resource/mod.rs:215-231`
- Modify: `crates/hoimin-cli/tests/process_handler.rs:840-880`
- Modify: `crates/hoimin-cli/tests/process_handler.rs:1287-1310`
- Modify: `README.md:53-59`
- Modify: `README.md:119-123`

**Interfaces:**
- Consumes: existing `exit_termination` and `PortableSupervisor::classify`; neither changes.
- Produces: exact Linux/macOS best-effort wording and `portable::preserves_explicit_152_and_sigxcpu_as_native_exits`.
- Documents: Tokio wall deadlines and process groups without a Hoimin CPU-time claim.

- [ ] **Step 1: Write platform diagnostic expectations**

Change the generic policy test fixture `DIAGNOSTIC` in `resource/mod.rs` and its exact expected error to:

```rust
const DIAGNOSTIC: &str = "macOS uses process groups; max-memory is not enforced";
```

In `process_handler.rs`, make the Linux test require `RLIMIT_AS` and reject `RLIMIT_CPU`:

```rust
let error = PortableBackend::new(false).unwrap_err();
let message = error.to_string();
assert_eq!(
    message,
    "portable resource limits require --allow-best-effort-memory: portable Linux uses per-process RLIMIT_AS and process groups",
);
assert!(!message.contains("RLIMIT_CPU"));
```

Keep the existing `PortableBackend::new(true).unwrap().mode()` assertion unchanged.

Change the macOS exact diagnostic expectation to:

```rust
Some("macOS uses process groups; max-memory is not enforced")
```

- [ ] **Step 2: Run diagnostic tests and capture RED**

Run:

```console
cargo test -p hoimin-cli resource::tests::plan_policy_rejects_unapproved_best_effort_memory -- --exact
cargo test -p hoimin-cli --test process_handler portable::macos_portable_backend_requires_explicit_best_effort_opt_in -- --exact
```

Expected on macOS: the generic policy-fixture test passes because it validates forwarding, while `macos_portable_backend_requires_explicit_best_effort_opt_in` fails against the live production constant. On Linux, run `portable::normal_linux_portable_backend_requires_explicit_opt_in`; it fails after the stronger assertion is added. Record the platform-specific RED output.

- [ ] **Step 3: Add native-exit characterization**

Add inside `mod portable`:

```rust
#[cfg(unix)]
#[tokio::test]
async fn preserves_explicit_152_and_sigxcpu_as_native_exits() {
    let output = tempfile::tempdir().unwrap();
    let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());
    let explicit = handler
        .handle(run_python(
            71,
            "raise SystemExit(152)",
            limits(Duration::from_secs(5), 64),
        ))
        .await
        .unwrap();
    let signal = handler
        .handle(run_python(
            72,
            "import os,signal\nsignal.signal(signal.SIGXCPU, signal.SIG_DFL)\nsignal.pthread_sigmask(signal.SIG_UNBLOCK, {signal.SIGXCPU})\nos.kill(os.getpid(), signal.SIGXCPU)",
            limits(Duration::from_secs(5), 64),
        ))
        .await
        .unwrap();

    assert_eq!(explicit.termination, ProcessTermination::Exit(152));
    assert_eq!(
        signal.termination,
        ProcessTermination::Exit(128 + libc::SIGXCPU),
    );
}
```

- [ ] **Step 4: Run characterization against the current behavior**

Run:

```console
cargo test -p hoimin-cli --test process_handler portable::preserves_explicit_152_and_sigxcpu_as_native_exits -- --exact --nocapture
```

Expected: PASS before and after diagnostic changes. A timeout indicates the signal disposition or mask setup is wrong.

- [ ] **Step 5: Update production diagnostic strings**

Set:

```rust
#[cfg(target_os = "macos")]
pub(super) const MACOS_BEST_EFFORT_DIAGNOSTIC: &str =
    "macOS uses process groups; max-memory is not enforced";
```

Change the Linux `BestEffortNotAllowed` reason to:

```rust
"portable Linux uses per-process RLIMIT_AS and process groups"
```

- [ ] **Step 6: Correct both current README passages**

Replace the plan/verify macOS sentence with:

```markdown
On macOS, `--max-memory` is accepted for plan compatibility but is not enforced. Hoimin still enforces wall-clock deadlines and process-group cleanup; pass `--allow-best-effort-memory` to `plan` when this memory policy is acceptable.
```

Replace the limits paragraph's portable Unix/macOS claims with:

```markdown
When hard enforcement is unavailable, Unix uses best-effort process groups and non-macOS Unix also applies per-process `RLIMIT_AS`. Linux and macOS require explicit `--allow-best-effort-memory` approval for this policy. On macOS, the memory limit is not enforced. Hoimin uses monotonic wall-clock deadlines and process-group cleanup on portable Unix. Reports identify `hard` or `best_effort` resource mode.
```

- [ ] **Step 7: Run focused tests and reject live CPU-limit claims**

Run:

```console
cargo test -p hoimin-cli resource::tests::
cargo test -p hoimin-cli --test process_handler portable::
cargo fmt --all
cargo fmt --all -- --check
rg -n "CPU-time limits|RLIMIT_CPU" README.md crates/hoimin-cli/src/resource/mod.rs crates/hoimin-cli/src/resource/portable.rs
git diff --check
```

Expected: tests PASS. `rg` may find `RLIMIT_CPU` only in the new test fixture and historical material outside the listed live files; README and production diagnostic code contain no claim that Hoimin installs it.

- [ ] **Step 8: Review and commit Task 2**

Inspect:

```console
git diff -- crates/hoimin-cli/src/resource/portable.rs crates/hoimin-cli/src/resource/mod.rs crates/hoimin-cli/tests/process_handler.rs README.md
git status --short
```

Commit:

```console
git add crates/hoimin-cli/src/resource/portable.rs crates/hoimin-cli/src/resource/mod.rs crates/hoimin-cli/tests/process_handler.rs README.md
git commit -m "fix: describe portable Unix wall timeouts"
```

---

### Task 3: Verify both native paths and close mutation-test gaps

**Files:**
- Modify only for a demonstrated gap: `crates/hoimin-cli/src/resource/portable.rs`
- Modify only for a demonstrated gap: `crates/hoimin-cli/tests/process_handler.rs`
- Modify only for a reviewed exact equivalence: `.cargo/mutants.toml`
- Evidence outside repository: `/tmp/hoimin-issue-339-mutants.*`

**Interfaces:**
- Consumes: green Task 1 and Task 2 commits.
- Produces: native macOS evidence, Linux CI/manual evidence, and focused/full mutation reports tied to final SHAs.

- [ ] **Step 1: Run local quality and workspace gates on macOS**

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
git diff --check
```

Expected: each command exits 0, including the inheritance and signal tests.

- [ ] **Step 2: Run the native macOS focused mutation slice**

Run:

```console
output_dir="$(mktemp -d /tmp/hoimin-issue-339-mutants.XXXXXX)"
printf '%s\n' "$output_dir" > /tmp/hoimin-issue-339-focused-path.txt
filter='(replace configure_command ->| in configure_command$)'
test "$(cargo mutants --version)" = 'cargo-mutants 27.1.0'
cargo mutants \
  --file crates/hoimin-cli/src/resource/portable.rs \
  --re "$filter" --list --json > "$output_dir/inventory.json"
cargo mutants \
  --file crates/hoimin-cli/src/resource/portable.rs \
  --re "$filter" --output "$output_dir"
```

Expected: `cargo mutants --version` reports exactly 27.1.0; executed names account for the inventory. Applicable macOS mutants have zero missed, timeout, and error outcomes. Classify cfg-disabled definitions by exact name and source line.

- [ ] **Step 3: Resolve focused mutation findings**

Recover the exact artifact path with `output_dir="$(sed -n '1p' /tmp/hoimin-issue-339-focused-path.txt)"`; assert both JSON files exist, then inspect `$output_dir/inventory.json`, `$output_dir/mutants.out/outcomes.json`, and logs. Add the smallest behavioral assertion for any applicable adverse outcome, demonstrate RED with the mutant, return to GREEN, and rerun Step 2 without `--iterate`. The focused gate permits no accepted survivor, timeout, or tool error.

- [ ] **Step 4: Run the required full workspace mutation inventory**

Run:

```console
full_output="$(mktemp -d /tmp/hoimin-issue-339-full-mutants.XXXXXX)"
printf '%s\n' "$full_output" > /tmp/hoimin-issue-339-full-path.txt
cargo mutants --workspace --output "$full_output"
```

Recover the report with `full_output="$(sed -n '1p' /tmp/hoimin-issue-339-full-path.txt)"`. Expected: baseline PASS. Resolve timeouts and tool errors. Fix each missed mutant. For an exact equivalent mutant, add its anchored complete name to `.cargo/mutants.toml` as an `exclude_re` with a TOML reason comment, obtain independent review, and rerun a fresh full non-iterated inventory. Inspect unviable and platform-inapplicable results. For a non-equivalent survivor outside the Issue diff, use a same-host parent-SHA run to identify it as pre-existing, then block delivery until explicit scope approval permits its fix or a separate prerequisite fix lands; in either case rerun a fresh full inventory before continuing.

If that parent comparison is needed, keep the candidate branch checked out and run the parent in a disposable worktree on the same host and cargo-mutants version:

```console
set -e
parent_root="$(mktemp -d /tmp/hoimin-issue-339-parent-worktree.XXXXXX)"
parent_tree="$parent_root/worktree"
parent_output="$(mktemp -d /tmp/hoimin-issue-339-parent-mutants.XXXXXX)"
baseline_sha="$(git merge-base HEAD origin/main)"
cleanup_parent() {
  git worktree remove --force "$parent_tree" 2>/dev/null || true
  rmdir "$parent_root" 2>/dev/null || true
}
trap cleanup_parent EXIT
printf '%s\n' "$baseline_sha" > /tmp/hoimin-issue-339-parent-sha.txt
printf '%s\n' "$parent_output" > /tmp/hoimin-issue-339-parent-path.txt
git worktree add --detach "$parent_tree" "$baseline_sha"
test "$(git -C "$parent_tree" rev-parse HEAD)" = "$baseline_sha"
uv sync --frozen --directory "$parent_tree"
test "$(cargo mutants --version)" = 'cargo-mutants 27.1.0'
cargo mutants --manifest-path "$parent_tree/Cargo.toml" --workspace --output "$parent_output"
candidate_output="$(sed -n '1p' /tmp/hoimin-issue-339-full-path.txt)"
LC_ALL=C sort "$candidate_output/mutants.out/missed.txt" > /tmp/hoimin-issue-339-candidate-missed.sorted
LC_ALL=C sort "$parent_output/mutants.out/missed.txt" > /tmp/hoimin-issue-339-parent-missed.sorted
comm -12 /tmp/hoimin-issue-339-candidate-missed.sorted /tmp/hoimin-issue-339-parent-missed.sorted > /tmp/hoimin-issue-339-preexisting-mutant-names.txt
git worktree remove --force "$parent_tree"
rmdir "$parent_root"
trap - EXIT
test -s /tmp/hoimin-issue-339-preexisting-mutant-names.txt
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

Expected: each command exits 0.

- [ ] **Step 6: Obtain native Linux evidence**

On a Linux host at the same branch SHA, run:

```console
cargo test -p hoimin-cli resource::portable::unix_tests::portable_setup_preserves_inherited_rlimit_cpu -- --exact --nocapture
cargo test -p hoimin-cli --test process_handler portable::
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Repeat Step 2 there with a fresh output directory. Require zero adverse outcomes among Linux-applicable `configure_command` mutants. Ordinary GitHub CI does not run cargo-mutants, so archive the manual Linux report separately.

- [ ] **Step 7: Commit mutation-driven improvements, if present**

If Tasks 2-6 changed tracked files, rerun Step 1 and commit:

```console
git add crates/hoimin-cli/src/resource/portable.rs crates/hoimin-cli/tests/process_handler.rs .cargo/mutants.toml
git commit -m "test: strengthen portable Unix timeout mutation coverage"
```

If no file changed, record that Task 3 required no commit. Treat all Task 3 mutation and native-host results as exploratory until the clean final-SHA rerun in Task 4.

---

### Task 4: Review, push, and open the Issue #339 PR

**Files:**
- Review: every path changed from `origin/main`
- External evidence: macOS/Linux logs, focused mutation reports, full mutation report, GitHub checks

**Interfaces:**
- Consumes: a clean branch with Tasks 1-3 complete.
- Produces: pushed branch `fix/issue-339-remove-portable-rlimit-cpu` and a PR using `Closes #339`.

- [ ] **Step 1: Fetch and rebase before final evidence**

Run:

```console
git fetch origin
git rebase origin/main
```

Resolve only Issue #339 changes. Do not reuse pre-rebase verification or review evidence.

- [ ] **Step 2: Rerun every gate on the clean candidate SHA**

Use one shell block to record and guard the candidate SHA:

```console
set -e
candidate_sha="$(git rev-parse HEAD)"
test -z "$(git status --short)"
printf '%s\n' "$candidate_sha" > /tmp/hoimin-issue-339-candidate-sha.txt
```

Reconstruct the historical RED against the final base in a disposable worktree. Apply only `/tmp/hoimin-issue-339-red-test.patch`, record its SHA-256 and the final `origin/main` SHA, and rerun the exact Task 1 Step 4 command:

```console
set -e
red_parent="$(mktemp -d /tmp/hoimin-issue-339-final-red.XXXXXX)"
red_tree="$red_parent/worktree"
cleanup_red() {
  git worktree remove --force "$red_tree" 2>/dev/null || true
  rmdir "$red_parent" 2>/dev/null || true
}
trap cleanup_red EXIT
git rev-parse origin/main > /tmp/hoimin-issue-339-final-red-base.txt
shasum -a 256 -c /tmp/hoimin-issue-339-red-test.patch.sha256
git worktree add --detach "$red_tree" origin/main
git -C "$red_tree" apply /tmp/hoimin-issue-339-red-test.patch
uv sync --frozen --directory "$red_tree"
if cargo test --manifest-path "$red_tree/Cargo.toml" -p hoimin-cli resource::portable::unix_tests::portable_setup_preserves_inherited_rlimit_cpu -- --exact --nocapture > /tmp/hoimin-issue-339-final-red.log 2>&1; then
  red_failed=0
else
  red_failed=1
fi
git -C "$red_tree" diff --check
git worktree remove --force "$red_tree"
rmdir "$red_parent"
trap - EXIT
test "$red_failed" -eq 1
rg -F 'rlimit_cpu_probe_fixture' /tmp/hoimin-issue-339-final-red.log
rg -F 'assertion `left == right` failed' /tmp/hoimin-issue-339-final-red.log
```

The test must fail for the same unequal-pair reason. If the patch conflicts, update only its test code to the final base, record the replacement hash, and confirm the disposable worktree contains no production change before running it.

Rerun every Task 3 Step 1 and Step 5 command, including the explicit `run_e2e` test. Create fresh output directories and rerun the macOS focused mutation slice and full workspace mutation inventory without `--iterate`. Obtain the Task 3 Step 6 Linux test and focused-mutation evidence at the exact same candidate SHA. The focused macOS and Linux gates each require zero missed, timeout, and error outcomes. Apply the repository-owned anchored `exclude_re` policy to any exact full-workspace equivalence. If a candidate full-workspace outcome appears pre-existing, rerun the full inventory on final `origin/main` on the same host and block delivery until the non-equivalent survivor is fixed with explicit scope approval or a prerequisite fix lands. Finish with this self-contained SHA guard:

```console
set -e
candidate_sha="$(sed -n '1p' /tmp/hoimin-issue-339-candidate-sha.txt)"
test "$(git rev-parse HEAD)" = "$candidate_sha"
test -z "$(git status --short)"
```

- [ ] **Step 3: Request independent code reviews and commit every accepted fix**

Use `superpowers:requesting-code-review`. Give reviewers the spec, this plan, `origin/main`, `HEAD`, native test logs, and mutation reports. Require separate Unix-semantics, supervisor-lifecycle, and mutation/delivery reviews. Fix findings through TDD and commit each accepted fix. Any code, test, documentation, configuration, mutation-exclusion, review-driven, or CI-driven change invalidates the evidence and approvals: return to Task 4 Step 2, then repeat independent review until every reviewer approves the same `HEAD` SHA.

- [ ] **Step 4: Verify the final branch state**

Use `superpowers:verification-before-completion`, then run:

```console
set -e
test -z "$(git status --short)"
git diff --check origin/main...HEAD
git log --oneline origin/main..HEAD
candidate_sha="$(sed -n '1p' /tmp/hoimin-issue-339-candidate-sha.txt)"
test "$(git rev-parse HEAD)" = "$candidate_sha"
```

Expected: clean status, no whitespace error, and Issue #339-only commits.

- [ ] **Step 5: Push and create the PR**

Run:

```console
git push --set-upstream origin fix/issue-339-remove-portable-rlimit-cpu
```

If no PR exists for the branch, create it with `gh pr create`. If one already exists after a review/CI loop, push the new candidate SHA and update that PR instead of creating another. Its initial body must include:

- `Closes #339`;
- deletion of Hoimin-installed `RLIMIT_CPU` on Unix;
- exact inheritance and external-policy semantics;
- unchanged Tokio selection and cleanup paths;
- explicit exit 152 and deterministic SIGXCPU evidence;
- native macOS/Linux test and mutation reports;
- full workspace mutation results;
- both corrected README passages.
- PR checks marked pending.

- [ ] **Step 6: Watch every pull-request check**

Run:

```console
gh pr checks --watch
```

Expected: `quality` on Ubuntu, Windows, and macOS; `msrv`; `rust` on Ubuntu, Windows, and macOS; `rust-shuffle`; `contracts`; `core-dependency-purity` on Ubuntu and Windows; `wheel-smoke` on Ubuntu, Windows, and macOS; and `linux-best-effort` all pass. If a CI-driven change is required, implement it through TDD, commit it, and return to Task 4 Step 2 before pushing the new SHA.

After checks pass, verify the remote PR head and attach the check evidence:

```console
set -e
candidate_sha="$(sed -n '1p' /tmp/hoimin-issue-339-candidate-sha.txt)"
test "$(gh pr view --json headRefOid --jq .headRefOid)" = "$candidate_sha"
gh pr checks
```

Update the PR body or add a PR comment with the final SHA and complete check results; do not describe CI as complete before this step.

- [ ] **Step 7: Hand off the merge-order dependency**

Stop after the #339 PR and its checks are ready. Record that #339 must merge before #338 because both edit live README wording and may conditionally add mutation exclusions. Only after a maintainer or the user authorizes and completes that merge may work resume on #338: rebase it, preserve the union of any anchored `.cargo/mutants.toml` exclusions and reason comments, resolve the live README wording, and repeat #338 verification and review on its new SHA.

## Plan review record

- Self-review kept the RED child-only, preserved the production supervisor lifecycle, and separated inherited CPU policy from Hoimin's wall-clock deadline.
- Independent Unix review bounded root reap and pipe joins, added fallback root kill and PGID disarm, and made the fixture portable across differing `rlim_t` ABIs without tripping Linux Clippy.
- Independent semantic review fixed Linux `RLIMIT_AS` headroom, exact diagnostics, SIGXCPU determinism, and the scope of live README claims.
- Delivery review made the final-base RED reproducible by patch digest, required same-host macOS/Linux focused mutation evidence, enforced anchored mutation exclusions, and made all SHA and remote-head checks fail-closed.
- Four review rounds completed. The final independent Unix and delivery reviews reported no remaining blocker, high, or medium findings.
