# Rust Toolchain Reproducibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make unchanged commits reproducible under an exact Rust 1.98.0 ordinary-CI toolchain while preserving MSRV/nightly lanes and moving synchronous shell construction off Tokio's executor threads.

**Architecture:** The repository root selects Rust 1.98.0, every repository-toolchain job consumes that selection, and a separate weekly workflow probes moving `stable`. `ShellContext::new` awaits a non-generic blocking preparation result, then attaches caller-owned writers on the async task; test-only immediate futures satisfy Rust 1.98 without changing the production trait.

**Tech Stack:** Rust 2024, Tokio 1.53, rustup, Clippy, GitHub Actions, Python 3.14 `unittest`, PyYAML, uv

**Spec:** `docs/superpowers/specs/2026-08-24-rust-toolchain-reproducibility-design.md`

## Global Constraints

- Work only on `fix/rust-1.98-ci`, which was created from `origin/main` without an upstream; do not change any remote branch.
- Do not touch the existing untracked `.idea/` directory.
- Do not modify `crates/hoimin-cli/src/resource/windows.rs`, `crates/hoimin-cli/src/resource/suspended.rs`, timeout values, retries, or test scheduling. Reviewed exceptions are limited to the test-only junction fallback in `workspace/root.rs` and exactly two Rust 1.98 lint-only `is_ok_and` rewrites in `tests/run_e2e.rs`.
- Preserve the public `ShellContext::new(&RunConfig, Stdout, Stderr).await -> Result<Self, String>` contract and do not add `Send + 'static` bounds to `Stdout` or `Stderr`.
- Preserve `workspace.package.rust-version = "1.88"` and `nightly-2026-07-27`.
- Add no dependency, public configuration field, report format, or release publication path.
- Keep shell setup outside the run total-timeout interval.
- Ordinary stable CI uses exact Rust 1.98.0; only the canary uses moving `stable`.
- Treat “repository-toolchain job” as a compiler-selection category, not a claim about external GitHub branch-protection settings.
- Follow RED, GREEN, refactor, focused verification, then commit for every task.

## File Map

| File | Responsibility in this change |
| --- | --- |
| `crates/hoimin-cli/src/process/output.rs` | Express immediate test-double futures without async trait impl methods. |
| `crates/hoimin-cli/src/report/mod.rs` | Separate non-generic report preparation from generic writer attachment. |
| `crates/hoimin-cli/src/shell.rs` | Prepare synchronous shell infrastructure in `spawn_blocking` and test executor responsiveness. |
| `rust-toolchain.toml` | Declare exact Rust 1.98.0, minimal profile, Clippy, and rustfmt. |
| `.github/workflows/ci.yml` | Consume the repository toolchain in every ordinary stable job, including event-conditional jobs. |
| `.github/workflows/rust-stable-canary.yml` | Probe latest stable weekly without a PR/push trigger or dependency from `ci.yml`. |
| `tests/test_ci_workflow.py` | Enforce toolchain, ordinary-CI, canary, release, and documentation contracts. |
| `docs/development.md` | Explain pinned stable updates separately from MSRV updates. |
| `crates/hoimin-cli/src/workspace/root.rs` | Keep the linked-parent Windows regression active without requiring symlink privilege by using a junction fallback in the test fixture only. |
| `crates/hoimin-cli/tests/run_e2e.rs` | Apply exactly two mechanical `Result::is_ok_and` Rust 1.98 lint fixes; do not alter waits, deadlines, or shutdown behavior. |

## Spec-to-plan acceptance matrix

| Specification contract | Implementation task | Executable or review evidence |
| --- | --- | --- |
| Exact Rust 1.98.0 for ordinary development and repository-toolchain CI jobs | Task 4 | Parsed `rust-toolchain.toml`, exhaustive CI-job classification, install-before-use checks, Rustup resolution, Task 5 quality and test gates. |
| Separate MSRV and pinned-nightly compatibility lanes | Task 4 | Exact `+1.88` and `+nightly-2026-07-27` workflow contracts; Task 5 runs both lanes. |
| Moving stable is visible but cannot change ordinary-CI results | Task 4 | Exact canary trigger, permission, action, environment, and command structure; `ci.yml` contains no canary dependency or `+stable`. |
| Release wheels consume the repository pin without changing publication policy | Task 4 | Existing exact artifact-only release contract, pinned `maturin-action` SHA, absence of a release toolchain input, and the pinned action-source behavior recorded in the spec. |
| `ShellContext::new(...).await` remains public and keeps non-`Send`, non-`'static` writers | Task 3 | Compile-time borrowed-`Rc` writer characterization plus unchanged signature and focused constructor tests. |
| Synchronous setup leaves Tokio's executor thread | Task 3 | Current-thread heartbeat regression, join-failure regression, manual inline counterfactual, and focused generated mutations. |
| Setup errors remain distinct and every partially or fully prepared resource has one owner | Task 3 | Owned `PreparedShellSetup`, compiler-enforced `Send + 'static` blocking boundary, setup-panic classification, and existing constructor/drop integrations. |
| Report writers remain on the async caller while JSON state is prepared off-thread | Task 2 | `PreparedReport: Send`, format-state unit test, and existing public report integrations. |
| The immediate `FirstWriteFails` test double becomes Rust 1.98-clean without production-trait changes | Task 1 | Exact Clippy RED/GREEN and the first-write-error/finalize invariant regression. |
| Setup stays outside total timeout and Windows production-flake work stays excluded | Tasks 0, 3, and 5 | The existing deadline remains after `ShellContext::new`; final review rejects run-loop and Windows resource changes, while separately auditing the test-only junction fallback and two lint-only E2E expressions. |
| No dependency, configuration, report-format, retry, or release-publication expansion | Tasks 0-5 | File Map allowlist, unchanged manifests and release workflow, existing exact artifact-only contract, and final path/diff review. |

---

### Task 0: Keep the Windows baseline unfiltered without symlink privilege

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/root.rs` test module only.

The existing linked-parent regression failed before product verification with
Windows error 1314 because this host has neither symlink privilege nor Developer
Mode. Reuse the repository's established fixture strategy: attempt
`symlink_dir`, and only for error 1314 create a directory junction with
`cmd /c mklink /J`. Keep the same production resolver and rejection assertion.

RED is the unfiltered focused test failing at fixture construction. GREEN is the
same test passing through the junction while still rejecting the linked parent.
Then run the complete unfiltered Rust 1.98 workspace suite. Commit separately as
`fix(test): fall back to Windows junctions`.

---

### Task 1: Make the immediate `OutputSink` test double Rust 1.98-clean

**Files:**
- Modify: `crates/hoimin-cli/src/process/output.rs:228-264`

**Interfaces:**
- Consumes: the existing private `OutputSink` async trait.
- Produces: `FirstWriteFails::write_ring` as an immediate `Ready` future and `FirstWriteFails::finalize` as a deferred `poll_fn` invariant failure, both through return-position `impl Future`; no production interface changes.

- [ ] **Step 1: Install the review toolchains before the repository pin exists**

Run:

```console
rustup toolchain install 1.98.0 --profile minimal --component rustfmt --component clippy
rustup toolchain install 1.88 --profile minimal
```

Expected: both exact toolchains are installed. This task cannot rely on `rust-toolchain.toml`, which is deliberately introduced only after the Rust 1.98 source diagnostics are green in Task 4.

- [ ] **Step 2: Reproduce the exact lint failure**

Run:

```console
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit 101 with `clippy::unused_async_trait_impl` at `output.rs` for `write_ring` and `finalize`, plus the independent `ShellContext::new` diagnostic.

- [ ] **Step 3: Replace only the test-double method bodies and syntax**

Use this implementation; keep the production trait and `FileOutputSink` unchanged:

```rust
impl OutputSink for FirstWriteFails {
    fn write_ring(
        &mut self,
        _capacity: u64,
        _position: u64,
        _chunk: &[u8],
    ) -> impl std::future::Future<Output = std::io::Result<u64>> {
        std::future::ready(Err(std::io::Error::other(format!(
            "lean-error-{}",
            self.code
        ))))
    }

    fn finalize(
        &mut self,
        _capacity: u64,
        _position: u64,
        _truncated: bool,
    ) -> impl std::future::Future<Output = std::io::Result<()>> {
        std::future::poll_fn(|_| panic!("a failed sink must not be finalized"))
    }
}
```

Do not wrap `finalize` in an `async` block: Rust 1.98 reports `clippy::manual_async_fn` for that counterproposal.

- [ ] **Step 4: Run the behavioral regression**

Run:

```console
cargo +1.98.0 test -p hoimin-cli --lib process::output::tests::collector_drains_to_eof_and_keeps_first_write_error -- --exact
```

Expected: PASS, proving the first write error remains `lean-error-11` and `finalize` is not polled.

- [ ] **Step 5: Confirm the two output diagnostics are gone**

Run:

```console
cargo +1.98.0 clippy -p hoimin-cli --all-targets --all-features -- -D warnings
```

Expected: the command can still fail at `ShellContext::new`, but it must contain no diagnostic for `process/output.rs` and no `manual_async_fn` diagnostic.

- [ ] **Step 6: Confirm the syntax remains within MSRV**

Run:

```console
cargo +1.88 check -p hoimin-cli --all-targets --all-features --locked
```

Expected on the Ubuntu MSRV lane: PASS. On this Windows host the command reaches
the pre-existing Windows-only `if let` guard in `resource/windows.rs` and fails
with E0658 under Rust 1.88. Compile the exact changed async-trait implementation
shape with Rust 1.88 as local syntax evidence, record the host limitation, and
do not modify Windows production code to make this branch's local command pass.

- [ ] **Step 7: Commit the isolated test-only repair**

```console
git add crates/hoimin-cli/src/process/output.rs
git commit -m "fix(test): satisfy Rust 1.98 async trait lint"
```

---

### Task 2: Separate report preparation from writer attachment

**Files:**
- Modify: `crates/hoimin-cli/src/report/mod.rs:1-70`

**Interfaces:**
- Consumes: private `JsonReport::new` and existing `OutputFormat`.
- Produces: `pub(crate) struct PreparedReport`, `PreparedReport::new(format, spool_dir) -> io::Result<Self>`, and `PreparedReport::attach(stdout, stderr) -> ReportHandler<Stdout, Stderr>`.
- Preserves: public `ReportHandler::new` and `ReportHandler::with_mutant_spool` signatures and behavior.

- [ ] **Step 1: Add compile-time and format-state tests before the type exists**

Append a unit-test module to `report/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn prepared_report_is_send_and_encodes_format_state() {
        assert_send::<PreparedReport>();
        let spool_dir = tempfile::tempdir().unwrap();

        let json = PreparedReport::new(OutputFormat::Json, spool_dir.path())
            .unwrap()
            .attach(Vec::new(), Vec::new());
        let jsonl = PreparedReport::new(OutputFormat::Jsonl, spool_dir.path())
            .unwrap()
            .attach(Vec::new(), Vec::new());
        let human = PreparedReport::new(OutputFormat::Human, spool_dir.path())
            .unwrap()
            .attach(Vec::new(), Vec::new());

        assert!(json.json.is_some());
        assert!(jsonl.json.is_none());
        assert!(human.json.is_none());
        assert_eq!(json.format, OutputFormat::Json);
        assert_eq!(jsonl.format, OutputFormat::Jsonl);
        assert_eq!(human.format, OutputFormat::Human);
    }
}
```

- [ ] **Step 2: Run the test and observe RED**

Run:

```console
cargo +1.98.0 test -p hoimin-cli --lib report::tests::prepared_report_is_send_and_encodes_format_state -- --exact
```

Expected: compile failure because `PreparedReport` is not defined.

- [ ] **Step 3: Implement the non-generic prepared state**

Place this type before `ReportHandler`:

```rust
pub(crate) struct PreparedReport {
    format: OutputFormat,
    json: Option<JsonReport>,
}

impl PreparedReport {
    pub(crate) fn new(format: OutputFormat, spool_dir: impl AsRef<Path>) -> io::Result<Self> {
        let json = match format {
            OutputFormat::Json => Some(JsonReport::new(spool_dir.as_ref())?),
            OutputFormat::Jsonl | OutputFormat::Human => None,
        };
        Ok(Self { format, json })
    }

    pub(crate) fn attach<Stdout, Stderr>(
        self,
        stdout: Stdout,
        stderr: Stderr,
    ) -> ReportHandler<Stdout, Stderr>
    where
        Stdout: Write,
        Stderr: Write,
    {
        ReportHandler {
            format: self.format,
            stdout,
            stderr,
            json: self.json,
        }
    }
}
```

Refactor the existing public constructor to delegate without changing its signature:

```rust
pub fn new(
    format: OutputFormat,
    stdout: Stdout,
    stderr: Stderr,
    spool_dir: impl AsRef<Path>,
) -> io::Result<Self> {
    Ok(PreparedReport::new(format, spool_dir)?.attach(stdout, stderr))
}
```

- [ ] **Step 4: Run focused and public report tests**

Run:

```console
cargo +1.98.0 test -p hoimin-cli --lib report::tests::prepared_report_is_send_and_encodes_format_state -- --exact
cargo +1.98.0 test -p hoimin-cli --test report_handler
cargo +1.98.0 test -p hoimin-cli --test report_heap
```

Expected: all PASS. The public constructor, injected-spool constructor, and JSON heap behavior remain unchanged.

- [ ] **Step 5: Run formatter and focused Clippy**

Run:

```console
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 clippy -p hoimin-cli --lib --all-features -- -D warnings
```

Expected: formatting passes; Clippy may still report only the known `ShellContext::new` diagnostic.

- [ ] **Step 6: Commit the report boundary**

```console
git add crates/hoimin-cli/src/report/mod.rs
git commit -m "refactor(report): separate preparation from writers"
```

---

### Task 3: Move real shell setup to Tokio's blocking pool

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs:1-35, 684-855, 2748-end`

**Interfaces:**
- Consumes: `PreparedReport::new` and `PreparedReport::attach` from Task 2.
- Consumes: the existing private `OwnedStartHook = Box<dyn FnOnce() + Send + 'static>` used by other owned blocking operations in `shell.rs`.
- Produces: private `PreparedShellSetup`, `prepare_shell_setup`, and `prepare_shell_setup_sync`.
- Preserves: the public generic constructor and its writer bounds.

- [ ] **Step 1: Import the prepared report type**

Change the report import to:

```rust
use crate::report::{PreparedReport, ReportHandler};
```

- [ ] **Step 2: Characterize the public writer-bound contract before implementation**

Add this shared fixture, local writer, and compile-time characterization inside
`shell.rs`'s existing `tests` module:

```rust
fn shell_setup_test_config(project: &TempDir) -> RunConfig {
    std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
    crate::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("target.py"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        OsString::from("unused-test-command"),
    ])
    .unwrap()
}

struct BorrowedNonSendWriter<'a> {
    buffer: &'a mut Vec<u8>,
    _not_send: std::rc::Rc<()>,
}

impl std::io::Write for BorrowedNonSendWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn shell_context_constructor_accepts_borrowed_non_send_writers() {
    let project = tempfile::tempdir().unwrap();
    let config = shell_setup_test_config(&project);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let future = ShellContext::new(
        &config,
        BorrowedNonSendWriter {
            buffer: &mut stdout,
            _not_send: std::rc::Rc::new(()),
        },
        BorrowedNonSendWriter {
            buffer: &mut stderr,
            _not_send: std::rc::Rc::new(()),
        },
    );

    drop(future);
}
```

Run:

```console
cargo +1.98.0 test -p hoimin-cli --lib shell::tests::shell_context_constructor_accepts_borrowed_non_send_writers -- --exact
```

Expected: PASS against the current constructor. `Rc` makes each writer
non-`Send`, its borrowed buffer makes it non-`'static`, and dropping the
unpolled future leaves constructor setup unexecuted, so the test characterizes
only the public writer bounds.

- [ ] **Step 3: Add the responsiveness and join-failure regressions before implementation**

Add these tests beside the characterization:

```rust
#[tokio::test(flavor = "current_thread")]
async fn shell_setup_does_not_block_the_async_runtime() {
    let project = tempfile::tempdir().unwrap();
    let config = shell_setup_test_config(&project);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
    let (heartbeat_tx, heartbeat_rx) = std::sync::mpsc::channel();
    let controller = std::thread::spawn(move || {
        entered_rx.recv_timeout(Duration::from_secs(4)).unwrap();
        let observed = heartbeat_rx
            .recv_timeout(Duration::from_secs(1))
            .is_ok();
        release_tx.send(()).unwrap();
        observed
    });
    let before_start: OwnedStartHook = Box::new(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    let heartbeat = async move {
        tokio::task::yield_now().await;
        let _ = heartbeat_tx.send(());
    };

    let (prepared, ()) = tokio::time::timeout(Duration::from_secs(6), async {
        tokio::join!(prepare_shell_setup(config, before_start), heartbeat)
    })
    .await
    .expect("shell setup regression must remain bounded");

    drop(prepared.unwrap());
    assert!(
        controller.join().unwrap(),
        "shell setup blocked the runtime until its blocking work was released"
    );
}

#[tokio::test]
async fn shell_setup_panic_is_reported_as_a_setup_join_failure() {
    let project = tempfile::tempdir().unwrap();
    let config = shell_setup_test_config(&project);
    let result = prepare_shell_setup(
        config,
        Box::new(|| panic!("controlled shell setup panic")),
    )
    .await;
    let Err(error) = result else {
        panic!("controlled setup panic unexpectedly succeeded");
    };

    assert!(error.starts_with("shell setup task failed:"), "{error}");
    assert!(error.contains("controlled shell setup panic"), "{error}");
}
```

- [ ] **Step 4: Run the new behavioral tests and observe RED**

Run:

```console
cargo +1.98.0 test -p hoimin-cli --lib shell::tests::shell_setup_does_not_block_the_async_runtime -- --exact
```

Expected: compile failure because `prepare_shell_setup` does not exist; `OwnedStartHook` already exists in `shell.rs`.

- [ ] **Step 5: Add the non-generic preparation result and blocking functions**

Place these definitions immediately before `ShellContext`:

```rust
struct PreparedShellSetup {
    workspace: WorkspaceHandler,
    analyzer: AnalyzerHandler,
    process: Arc<ProcessHandler>,
    report: PreparedReport,
    spool_dir: Arc<TempDir>,
    config: RunConfig,
}

async fn prepare_shell_setup(
    config: RunConfig,
    before_start: OwnedStartHook,
) -> Result<PreparedShellSetup, String> {
    tokio::task::spawn_blocking(move || {
        before_start();
        prepare_shell_setup_sync(config)
    })
    .await
    .map_err(|error| format!("shell setup task failed: {error}"))?
}

fn prepare_shell_setup_sync(config: RunConfig) -> Result<PreparedShellSetup, String> {
    let spool_dir = Arc::new(tempfile::tempdir().map_err(|error| error.to_string())?);
    let spool_path = Utf8PathBuf::from_path_buf(spool_dir.path().to_owned())
        .map_err(|_| "temporary spool path is not UTF-8".to_owned())?;
    std::fs::create_dir_all(spool_path.join("report"))
        .map_err(|error| error.to_string())?;
    let requested_workers = u32::try_from(config.limits.jobs.get())
        .map_err(|_| "--jobs exceeds the supported worker count".to_owned())?;
    let workspace = WorkspaceHandler::new(
        config.root.clone(),
        config.selection.sources.clone(),
        requested_workers,
        CopyOptions {
            includes: config.selection.includes.clone(),
            excludes: config.selection.excludes.clone(),
        },
    );
    let backend = resource_backend(&config).map_err(|error| error.to_string())?;
    let process = Arc::new(ProcessHandler::new(
        backend.clone(),
        spool_path.join("process"),
    ));
    let analyzer = AnalyzerHandler::with_backend(
        config.root.clone(),
        backend,
        config.limits.max_memory.get(),
        u32::try_from(config.limits.max_processes.get())
            .map_err(|_| "--max-processes exceeds the supported process count".to_owned())?,
    )
    .map_err(|error| error.to_string())?
    .with_candidate_spool_owner(spool_dir.clone());
    let report = PreparedReport::new(config.output.format, spool_path.join("report"))
        .map_err(|error| error.to_string())?;

    Ok(PreparedShellSetup {
        workspace,
        analyzer,
        process,
        report,
        spool_dir,
        config,
    })
}
```

- [ ] **Step 6: Rebuild `ShellContext::new` from the prepared value**

Remove the `#[expect(clippy::unused_async)]` attribute and replace the constructor body with:

```rust
pub async fn new(config: &RunConfig, stdout: Stdout, stderr: Stderr) -> Result<Self, String> {
    let PreparedShellSetup {
        workspace,
        analyzer,
        process,
        report,
        spool_dir,
        config,
    } = prepare_shell_setup(config.clone(), Box::new(|| {})).await?;
    let session_path = config.session.as_ref().map(|value| value.path.clone());

    Ok(Self {
        workspace: Some(workspace),
        analyzer,
        process,
        report: report.attach(stdout, stderr),
        session: None,
        session_path,
        active_candidates: BTreeMap::new(),
        _spool_dir: spool_dir,
        resolved_targets: None,
        config,
        fingerprint_copy_inputs: BTreeSet::new(),
        report_versions: ReportVersions {
            os: std::env::consts::OS.to_owned(),
            hoimin: env!("CARGO_PKG_VERSION").to_owned(),
        },
    })
}
```

Do not move `stdout` or `stderr` into the blocking closure and do not add bounds
beyond `Write`; the Step 2 characterization must continue to compile.

- [ ] **Step 7: Run focused shell and constructor regressions**

Run:

```console
cargo +1.98.0 test -p hoimin-cli --lib shell::tests::shell_context_constructor_accepts_borrowed_non_send_writers -- --exact
cargo +1.98.0 test -p hoimin-cli --lib shell::tests::shell_setup_does_not_block_the_async_runtime -- --exact
cargo +1.98.0 test -p hoimin-cli --lib shell::tests::shell_setup_panic_is_reported_as_a_setup_join_failure -- --exact
cargo +1.98.0 test -p hoimin-cli --test run_e2e shell_context_construction_performs_no_project_io -- --exact
```

Expected: all PASS. The existing E2E test continues to prove that construction creates neither a missing project root nor its configured session database. The only permitted `run_e2e.rs` edits are the separately reviewed two mechanical `ok().is_some_and(...)` to `is_ok_and(...)` Rust 1.98 lint rewrites; no wait, deadline, or shutdown logic may change.

- [ ] **Step 8: Run the exact Rust 1.98 quality gate**

Run:

```console
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: both PASS with no `unused_async_trait_impl`, `unused_async`, or `manual_async_fn` allowance.

- [ ] **Step 9: Commit the production async boundary**

```console
git add crates/hoimin-cli/src/shell.rs
git commit -m "fix(shell): offload context setup"
```

---

### Task 4: Pin ordinary CI Rust and add an isolated latest-stable canary

**Files:**
- Create: `rust-toolchain.toml`
- Create: `.github/workflows/rust-stable-canary.yml`
- Modify: `.github/workflows/ci.yml:17-174`
- Modify: `tests/test_ci_workflow.py:1-280`
- Modify: `docs/development.md:1-36`

**Interfaces:**
- Consumes: Rust 1.98-clean source from Tasks 1-3.
- Produces: exact repository toolchain contract, ordinary-job install contract, latest-stable canary, and executable documentation/workflow tests.
- Preserves: explicit `+1.88`, explicit `+nightly-2026-07-27`, and the exact artifact-only release workflow.

- [ ] **Step 1: Add the exact repository-toolchain contract**

Add these constants to `tests/test_ci_workflow.py`:

```python
RUST_TOOLCHAIN = ROOT / "rust-toolchain.toml"
STABLE_CANARY_WORKFLOW = (
    ROOT / ".github" / "workflows" / "rust-stable-canary.yml"
)
REPOSITORY_RUST_JOBS = {
    "quality",
    "rust",
    "contracts",
    "core-dependency-purity",
    "wheel-smoke",
    "linux-best-effort",
    "linux-cgroup-v2-hard",
}
COMPATIBILITY_RUST_JOBS = {"msrv", "rust-shuffle"}
CHECKOUT_ACTION = (
    "actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10"
)
SETUP_PYTHON_ACTION = (
    "actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1"
)
SETUP_UV_ACTION = (
    "astral-sh/setup-uv@08807647e7069bb48b6ef5acd8ec9567f424441b"
)
```

Add the repository declaration contract:

```python
class RepositoryRustToolchainContractTests(unittest.TestCase):
    def test_repository_toolchain_is_exact_and_complete(self) -> None:
        toolchain = tomllib.loads(RUST_TOOLCHAIN.read_text(encoding="utf-8"))

        self.assertEqual(set(toolchain), {"toolchain"})
        declaration = toolchain["toolchain"]
        self.assertEqual(
            set(declaration),
            {"channel", "profile", "components"},
        )
        self.assertEqual(declaration["channel"], "1.98.0")
        self.assertEqual(declaration["profile"], "minimal")
        self.assertCountEqual(
            declaration["components"],
            ["clippy", "rustfmt"],
        )
```

- [ ] **Step 2: Add exhaustive Rust-job classification and ordering**

Add a separate contract class:

```python
class CiRustJobContractTests(unittest.TestCase):

    def test_rust_jobs_install_only_their_classified_toolchain(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)

        self.assertEqual(
            set(decoded["jobs"]),
            REPOSITORY_RUST_JOBS | COMPATIBILITY_RUST_JOBS,
        )
        self.assertNotIn("RUSTUP_TOOLCHAIN", workflow)
        self.assertNotIn("rustup override", workflow)
        self.assertNotIn("rustup default", workflow)
        self.assertNotIn("rustup run", workflow)
        self.assertNotIn("rustup update", workflow)
        all_steps = [
            step
            for job in decoded["jobs"].values()
            for step in job["steps"]
        ]
        self.assertFalse(
            any(
                "toolchain" in step.get("uses", "").lower()
                for step in all_steps
            )
        )
        install_commands = [
            line.strip()
            for step in all_steps
            for line in step.get("run", "").splitlines()
            if line.strip().startswith("rustup toolchain install")
        ]
        self.assertCountEqual(
            install_commands,
            ["rustup toolchain install"] * len(REPOSITORY_RUST_JOBS)
            + [
                "rustup toolchain install 1.88 --profile minimal",
                (
                    "rustup toolchain install nightly-2026-07-27 "
                    "--profile minimal"
                ),
            ],
        )
        self.assertCountEqual(
            re.findall(
                r"(?m)\b(?:cargo|rustc|rustdoc) \+([^\s]+)",
                workflow,
            ),
            ["1.88", "nightly-2026-07-27"],
        )
        for job_name in REPOSITORY_RUST_JOBS:
            job = job_block(workflow, job_name)
            steps = decoded["jobs"][job_name]["steps"]
            install_indexes = [
                index
                for index, step in enumerate(steps)
                if step.get("run") == "rustup toolchain install"
            ]
            rust_command_indexes = [
                index
                for index, step in enumerate(steps)
                if re.search(
                    r"(?m)^(?:cargo|rustc|rustdoc|uvx maturin)\b",
                    step.get("run", ""),
                )
            ]
            self.assertEqual(len(install_indexes), 1, job_name)
            self.assertTrue(rust_command_indexes, job_name)
            self.assertLess(
                install_indexes[0],
                min(rust_command_indexes),
                job_name,
            )
            self.assertNotRegex(
                job,
                r"(?:cargo|rustc|rustdoc) \+[^\s]+",
                job_name,
            )

        msrv = job_block(workflow, "msrv")
        shuffle = job_block(workflow, "rust-shuffle")
        self.assertIn("cargo +1.88 check", msrv)
        self.assertIn("cargo +nightly-2026-07-27 test", shuffle)
```

- [ ] **Step 3: Add the isolated latest-stable canary contract**

Add the exact canary contract:

```python
class LatestStableCanaryContractTests(unittest.TestCase):

    def test_latest_stable_canary_is_isolated_and_environment_complete(self) -> None:
        workflow = STABLE_CANARY_WORKFLOW.read_text(encoding="utf-8")
        decoded = yaml.safe_load(workflow)

        self.assertEqual(
            set(decoded),
            {"name", True, "permissions", "jobs"},
        )
        self.assertEqual(decoded["name"], "Latest stable Rust canary")
        self.assertEqual(
            trigger_events(workflow),
            {"schedule", "workflow_dispatch"},
        )
        self.assertEqual(decoded["permissions"], {"contents": "read"})
        self.assertEqual(decoded[True]["schedule"], [{"cron": "0 3 * * 1"}])
        self.assertIsNone(decoded[True]["workflow_dispatch"])
        self.assertEqual(set(decoded["jobs"]), {"stable"})
        job = decoded["jobs"]["stable"]
        self.assertEqual(set(job), {"runs-on", "steps"})
        self.assertEqual(job["runs-on"], "ubuntu-latest")
        self.assertEqual(
            job["steps"],
            [
                {"uses": CHECKOUT_ACTION},
                {
                    "uses": SETUP_PYTHON_ACTION,
                    "with": {"python-version": "3.14"},
                },
                {
                    "uses": SETUP_UV_ACTION,
                    "with": {"enable-cache": True},
                },
                {
                    "name": "Install latest stable Rust tooling",
                    "run": (
                        "rustup toolchain install stable --profile minimal "
                        "--component rustfmt --component clippy"
                    ),
                },
                {"run": "uv sync --frozen"},
                {"run": "cargo +stable fmt --all -- --check"},
                {
                    "run": (
                        "cargo +stable clippy --workspace --all-targets "
                        "--all-features -- -D warnings"
                    ),
                },
                {"run": "cargo +stable test --workspace"},
            ],
        )
        self.assertNotIn("rust-stable-canary", CI_WORKFLOW.read_text(encoding="utf-8"))
```

- [ ] **Step 4: Add release and development-document preservation contracts**

Add the remaining preservation contracts:

```python
class ToolchainReleaseDocumentationContractTests(unittest.TestCase):

    def test_release_has_no_toolchain_override(self) -> None:
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        self.assertNotIn("RUSTUP_TOOLCHAIN", release)
        self.assertNotIn("rust-toolchain:", release)

    def test_development_guide_separates_pin_updates_from_msrv_updates(self) -> None:
        guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

        self.assertIn("## Pinned Rust toolchain", guide)
        self.assertIn("rust-toolchain.toml", guide)
        self.assertIn("1.98.0", guide)
        self.assertIn("does not raise the minimum supported Rust version", guide)
        expected_commands = [
            "cargo fmt --all -- --check",
            "cargo clippy --workspace --all-targets --all-features -- -D warnings",
            "cargo test --workspace",
            "cargo test -p hoimin-cli --test run_e2e",
            "cargo test -p hoimin-core --features contracts",
            "cargo test -p hoimin-cli --features contracts",
            "uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v",
            "uvx maturin build --release",
            "uv run --frozen python tests/wheel_smoke.py",
        ]
        fence = chr(96) * 3
        prefix = (
            "Run the Rust quality gate locally with the same commands used in CI:"
            f"\n\n{fence}console\n"
        )
        start = guide.index(prefix) + len(prefix)
        end = guide.index(f"\n{fence}", start)
        self.assertEqual(guide[start:end].splitlines(), expected_commands)
        self.assertNotIn("uv run maturin build --release", guide)
```

- [ ] **Step 5: Run the workflow tests and observe RED**

Run:

```console
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: failures for the missing `rust-toolchain.toml`, missing canary, rolling stable installs, and missing development-guide section; existing MSRV, nightly, trigger, and release tests still pass.

- [ ] **Step 6: Add the exact repository toolchain declaration**

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.98.0"
profile = "minimal"
components = ["clippy", "rustfmt"]
```

- [ ] **Step 7: Make repository-toolchain jobs consume the repository declaration**

In each job named in `REPOSITORY_RUST_JOBS`, replace the two-line rolling install/default block with:

```yaml
      - name: Install repository Rust toolchain
        run: rustup toolchain install
```

Keep these exceptions byte-for-byte in meaning:

```yaml
rustup toolchain install 1.88 --profile minimal
cargo +1.88 check --workspace --all-targets --all-features --locked
rustup toolchain install nightly-2026-07-27 --profile minimal
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

Do not add `rustup default`, `rustup override`, `rustup run`, `rustup update`,
`RUSTUP_TOOLCHAIN`, an alternate toolchain setup action, a new `+toolchain`
selector, or a release-workflow override.

- [ ] **Step 8: Create the isolated canary workflow**

Create `.github/workflows/rust-stable-canary.yml`:

```yaml
name: Latest stable Rust canary

on:
  schedule:
    - cron: '0 3 * * 1'
  workflow_dispatch:

permissions:
  contents: read

jobs:
  stable:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10 # v6.0.3
      - uses: actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1 # v6.3.0
        with:
          python-version: '3.14'
      - uses: astral-sh/setup-uv@08807647e7069bb48b6ef5acd8ec9567f424441b # v8.1.0
        with:
          enable-cache: true
      - name: Install latest stable Rust tooling
        run: rustup toolchain install stable --profile minimal --component rustfmt --component clippy
      - run: uv sync --frozen
      - run: cargo +stable fmt --all -- --check
      - run: cargo +stable clippy --workspace --all-targets --all-features -- -D warnings
      - run: cargo +stable test --workspace
```

- [ ] **Step 9: Document pinned stable separately from MSRV**

Insert this section before `## Minimum supported Rust version` in `docs/development.md`:

```markdown
## Pinned Rust toolchain

The repository root `rust-toolchain.toml` pins the Rust 1.98.0 compiler and the
Clippy and rustfmt components used by local development, ordinary CI, and
release builds. Run ordinary `cargo` commands from the repository root; do not
set a global default toolchain for this project.

Updating the pin is a reviewed compatibility change. Change the exact channel
in `rust-toolchain.toml`, resolve formatter, compiler, and Clippy diagnostics,
then run the complete quality and test commands in this guide in the same pull
request. The weekly latest-stable canary reports when such an update is needed.
Updating the pinned compiler does not raise the minimum supported Rust version.
The MSRV remains the separate contract below.
```

Replace the guide's opening command block with the primary repository-toolchain
and wheel-gate commands. The separate MSRV and nightly sections remain the
compatibility-lane instructions:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uvx maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

In the wheel-smoke paragraph, change its remaining `uv run maturin build --release` reference to `uvx maturin build --release`. Change the final sentence of the MSRV update paragraph from “Stable CI remains required” to “The pinned stable CI gate remains required” so it cannot be read as moving stable.

- [ ] **Step 10: Run all executable workflow contracts**

Run:

```console
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: all tests PASS, including existing exact release-workflow and trigger-reachability contracts.

- [ ] **Step 11: Verify Rustup resolution and all explicit exceptions**

Run:

```console
rustup show active-toolchain
rustc --version
rg -n "rustup (default|override|run|update)|rustup toolchain install stable|RUSTUP_TOOLCHAIN|(cargo|rustc|rustdoc) \+stable|uses:.*toolchain" .github/workflows/ci.yml .github/workflows/release.yml
rg -n "1\.88|nightly-2026-07-27" .github/workflows/ci.yml Cargo.toml docs/development.md tests/test_ci_workflow.py
```

Expected: the first two commands report Rust 1.98.0 from the repository override; the first `rg` has no matches; the second shows the unchanged MSRV and pinned-nightly contracts.

- [ ] **Step 12: Commit the reproducible CI boundary**

```console
git add rust-toolchain.toml .github/workflows/ci.yml .github/workflows/rust-stable-canary.yml tests/test_ci_workflow.py docs/development.md
git commit -m "ci: pin Rust toolchain and add stable canary"
```

---

### Task 5: Verify the combined change and mutation resistance

**Files:**
- Verify only.

**Interfaces:**
- Consumes: all deliverables from Tasks 1-4.
- Produces: local cross-toolchain, full-workspace, independent-E2E, workflow-contract, scope-isolation, and focused mutation evidence.

**Evidence boundary:** Steps 1-8 are evidence from the executor's current
Windows host plus static workflow contracts. They do not prove that hosted
Ubuntu/macOS matrices, the event-conditional self-hosted cgroup job, a tag-only
release, or the first scheduled canary actually ran. Record those later Actions
results separately and do not label them PASS from local evidence. The release
contract before a tag consists of the exact workflow test, the pinned-action
source audit in the spec, and the local wheel build/smoke result.

- [ ] **Step 1: Run formatting, exact quality, MSRV, and workflow contracts**

Run:

```console
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.88 check --workspace --all-targets --all-features --locked
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: formatting, Rust 1.98 quality, and workflow contracts PASS locally.
The complete MSRV check must PASS on its Ubuntu CI lane; on this Windows host,
record the known pre-existing E0658 at `resource/windows.rs` plus the separate
Rust 1.88 syntax check for the changed test-double implementation.

- [ ] **Step 2: Run the full workspace and independent E2E test processes**

Run as two separate Cargo processes:

```console
cargo +1.98.0 test --workspace
cargo +1.98.0 test -p hoimin-cli --test run_e2e
```

Expected: both PASS. The second command is intentionally independent, matching the ordinary test job; it is not a retry of a failed command.

- [ ] **Step 3: Run contracts and the pinned randomized-order lane**

Run:

```console
cargo +1.98.0 test -p hoimin-core --features contracts
cargo +1.98.0 test -p hoimin-cli --features contracts
rustup toolchain install nightly-2026-07-27 --profile minimal
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

Expected: all PASS. The nightly command supplements rather than replaces either stable test process.

- [ ] **Step 4: Run the complete Python and wheel-smoke lane**

Run:

```powershell
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
$repositoryRoot = (Resolve-Path '.').Path
$wheelDirectory = [System.IO.Path]::GetFullPath(
    (Join-Path $repositoryRoot 'target\wheels')
)
$expectedPrefix = $repositoryRoot + [System.IO.Path]::DirectorySeparatorChar
if (-not $wheelDirectory.StartsWith(
    $expectedPrefix,
    [System.StringComparison]::OrdinalIgnoreCase
)) {
    throw "Refusing to remove wheel artifacts outside repository: $wheelDirectory"
}
foreach ($candidate in @(
    (Join-Path $repositoryRoot 'target'),
    $wheelDirectory
)) {
    if (Test-Path -LiteralPath $candidate) {
        $item = Get-Item -LiteralPath $candidate -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Refusing to remove wheel artifacts through reparse point: $candidate"
        }
    }
}
if (Test-Path -LiteralPath $wheelDirectory) {
    Remove-Item -LiteralPath $wheelDirectory -Recurse -Force
}
uvx maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

Expected: all Python tests pass, the release wheel is rebuilt from an empty artifact directory with the repository-pinned Rust toolchain, and wheel smoke passes. Removing `target/wheels` is limited to ignored build artifacts created by this repository.

- [ ] **Step 5: Prove the blocking-boundary counterfactual is detected**

Temporarily replace only `prepare_shell_setup` with this inline counterfactual using `apply_patch`:

```rust
async fn prepare_shell_setup(
    config: RunConfig,
    before_start: OwnedStartHook,
) -> Result<PreparedShellSetup, String> {
    before_start();
    prepare_shell_setup_sync(config)
}
```

Run:

```console
cargo +1.98.0 test -p hoimin-cli --lib shell::tests::shell_setup_does_not_block_the_async_runtime -- --exact
```

Expected: FAIL after the controller releases the gate, with `shell setup blocked the runtime until its blocking work was released`; it must not hang or reach the six-second total timeout. Restore the exact `spawn_blocking` implementation from Task 3 with `apply_patch`, rerun the same command, and expect PASS. Verify `git diff` contains no counterfactual residue before continuing.

- [ ] **Step 6: Run focused generated mutation testing for the production setup boundary**

Run in PowerShell:

```powershell
$mutationOutput = Join-Path $env:TEMP ("hoimin-rust-1-98-mutation-" + [guid]::NewGuid())
uv run --frozen python tools/focused_mutation.py `
  --budget 30m `
  --base origin/main `
  --symbol prepare_shell_setup `
  --symbol prepare_shell_setup_sync `
  --output $mutationOutput
```

Expected: baseline passes. Explicit symbols receive the runner's highest ranking; do not add `--file`, which would raise every function in `shell.rs` and waste the bounded budget. Inspect `$mutationOutput\run.json` and `$mutationOutput\report.md`; every generated viable mutation belonging to the two selected functions is killed. The runner may also inventory lower-ranked functions changed from `origin/main`; their presence does not broaden this task's mutation claim. The manual counterfactual in Step 5, rather than cargo-mutants' generated inventory, is the required proof for removing `spawn_blocking`. Do not treat `not_run`, `pending`, `timeout`, `unviable`, or `error` for either selected function as passes.

- [ ] **Step 7: Fail closed on incomplete mutation evidence**

If `report.md` contains a survivor or `run.json` contains `not_run`, `pending`, `timeout`, `unviable`, or `error` for either selected function, do not claim mutation verification and do not add a speculative test. Record the exact generated mutation and outcome, then return it for design review. Continue only when the selected viable inventory is fully killed; an equivalent mutant requires an exact expression-level justification in the execution report.

- [ ] **Step 8: Prove forbidden Windows and IDE paths are absent**

Run:

```console
git diff --name-only origin/main
git diff --check origin/main
git diff --function-context origin/main -- crates/hoimin-cli/src/shell.rs
rg -n "ShellContext::new\(&config|let deadline = tokio::time::Instant::now\(\) \+ config\.limits\.total_timeout\.get\(\)" crates/hoimin-cli/src/shell.rs
git status --short --branch
```

Expected: changed paths are limited to the spec, this plan, and the ten implementation files in the File Map. The `shell.rs` function-context diff contains only the import, prepared setup, constructor, and focused tests. In `run_loop_prepared`, the `ShellContext::new` await remains before the single total-timeout deadline construction. There are no changes to the run-loop body or Windows resource production files. `workspace/root.rs` changes only its test fixture; `run_e2e.rs` contains exactly the two approved lint-only expressions and no timing or shutdown change. `.idea/` remains the sole unrelated untracked path; the status line is `## fix/rust-1.98-ci` with no upstream annotation.

- [ ] **Step 9: Confirm the final commit set without creating a verification-only commit**

Run:

```console
git log --oneline origin/main..HEAD
```

Expected: the log contains the reviewed specification/plan history followed by
the six implementation commits from Tasks 0-4: junction fixture, output test
double, report preparation, two E2E lint expressions, shell setup, and toolchain
CI. Every implementation commit remains inside the revised File Map. Do not
assert a fixed count for documentation-review
commits: this plan intentionally preserves their audit history. Verification
must not create an empty commit.
