# Rust Toolchain Reproducibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make unchanged commits reproducible under an exact Rust 1.98.0 required-CI toolchain while preserving MSRV/nightly lanes and moving synchronous shell construction off Tokio's executor threads.

**Architecture:** The repository root selects Rust 1.98.0, required jobs consume that selection, and a separate weekly workflow probes moving `stable`. `ShellContext::new` awaits a non-generic blocking preparation result, then attaches caller-owned writers on the async task; test-only immediate futures satisfy Rust 1.98 without changing the production trait.

**Tech Stack:** Rust 2024, Tokio 1.53, rustup, Clippy, GitHub Actions, Python 3.14 `unittest`, PyYAML, uv

**Spec:** `docs/superpowers/specs/2026-08-24-rust-toolchain-reproducibility-design.md`

## Global Constraints

- Work only on `fix/rust-1.98-ci`, which was created from `origin/main` without an upstream; do not change any remote branch.
- Do not touch the existing untracked `.idea/` directory.
- Do not modify `crates/hoimin-cli/src/resource/windows.rs`, `crates/hoimin-cli/src/resource/suspended.rs`, `crates/hoimin-cli/tests/run_e2e.rs`, timeout values, retries, or test scheduling.
- Preserve the public `ShellContext::new(&RunConfig, Stdout, Stderr).await -> Result<Self, String>` contract and do not add `Send + 'static` bounds to `Stdout` or `Stderr`.
- Preserve `workspace.package.rust-version = "1.88"` and `nightly-2026-07-27`.
- Add no dependency, public configuration field, report format, or release publication path.
- Keep shell setup outside the run total-timeout interval.
- Required CI uses exact Rust 1.98.0; only the canary uses moving `stable`.
- Follow RED, GREEN, refactor, focused verification, then commit for every task.

## File Map

| File | Responsibility in this change |
| --- | --- |
| `crates/hoimin-cli/src/process/output.rs` | Express immediate test-double futures without async trait impl methods. |
| `crates/hoimin-cli/src/report/mod.rs` | Separate non-generic report preparation from generic writer attachment. |
| `crates/hoimin-cli/src/shell.rs` | Prepare synchronous shell infrastructure in `spawn_blocking` and test executor responsiveness. |
| `rust-toolchain.toml` | Declare exact Rust 1.98.0, minimal profile, Clippy, and rustfmt. |
| `.github/workflows/ci.yml` | Consume the repository toolchain in every ordinary required job. |
| `.github/workflows/rust-stable-canary.yml` | Probe latest stable weekly without becoming a required check. |
| `tests/test_ci_workflow.py` | Enforce toolchain, required-CI, canary, release, and documentation contracts. |
| `docs/development.md` | Explain pinned stable updates separately from MSRV updates. |

---

### Task 1: Make the immediate `OutputSink` test double Rust 1.98-clean

**Files:**
- Modify: `crates/hoimin-cli/src/process/output.rs:228-264`

**Interfaces:**
- Consumes: the existing private `OutputSink` async trait.
- Produces: `FirstWriteFails::write_ring` as an immediate `Ready` future and `FirstWriteFails::finalize` as a deferred `poll_fn` invariant failure, both through return-position `impl Future`; no production interface changes.

- [ ] **Step 1: Reproduce the exact lint failure**

Run:

```console
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit 101 with `clippy::unused_async_trait_impl` at `output.rs` for `write_ring` and `finalize`, plus the independent `ShellContext::new` diagnostic.

- [ ] **Step 2: Replace only the test-double method bodies and syntax**

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

- [ ] **Step 3: Run the behavioral regression**

Run:

```console
cargo +1.98.0 test -p hoimin-cli process::output::tests::collector_drains_to_eof_and_keeps_first_write_error -- --exact
```

Expected: PASS, proving the first write error remains `lean-error-11` and `finalize` is not polled.

- [ ] **Step 4: Confirm the two output diagnostics are gone**

Run:

```console
cargo +1.98.0 clippy -p hoimin-cli --all-targets --all-features -- -D warnings
```

Expected: the command can still fail at `ShellContext::new`, but it must contain no diagnostic for `process/output.rs` and no `manual_async_fn` diagnostic.

- [ ] **Step 5: Confirm the syntax remains within MSRV**

Run:

```console
cargo +1.88 check -p hoimin-cli --all-targets --all-features --locked
```

Expected: PASS.

- [ ] **Step 6: Commit the isolated test-only repair**

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
cargo +1.98.0 test -p hoimin-cli report::tests::prepared_report_is_send_and_encodes_format_state -- --exact
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
cargo +1.98.0 test -p hoimin-cli report::tests::prepared_report_is_send_and_encodes_format_state -- --exact
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
- Produces: private `PreparedShellSetup`, `ShellSetupStartHook`, `prepare_shell_setup`, and `prepare_shell_setup_sync`.
- Preserves: the public generic constructor and its writer bounds.

- [ ] **Step 1: Import the prepared report type**

Change the report import to:

```rust
use crate::report::{PreparedReport, ReportHandler};
```

- [ ] **Step 2: Add the current-thread responsiveness regression before implementation**

Add this test inside `shell.rs`'s existing `tests` module:

```rust
#[tokio::test(flavor = "current_thread")]
async fn shell_setup_does_not_block_the_async_runtime() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
    let config = crate::cli::parse_config_from([
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
    .unwrap();
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
    let before_start: ShellSetupStartHook = Box::new(move || {
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
```

- [ ] **Step 3: Run the new test and observe RED**

Run:

```console
cargo +1.98.0 test -p hoimin-cli shell::tests::shell_setup_does_not_block_the_async_runtime -- --exact
```

Expected: compile failure because `ShellSetupStartHook` and `prepare_shell_setup` do not exist.

- [ ] **Step 4: Add the non-generic preparation result and blocking functions**

Place these definitions immediately before `ShellContext`:

```rust
type ShellSetupStartHook = Box<dyn FnOnce() + Send + 'static>;

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
    before_start: ShellSetupStartHook,
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

- [ ] **Step 5: Rebuild `ShellContext::new` from the prepared value**

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

Do not move `stdout` or `stderr` into the blocking closure and do not add bounds beyond `Write`.

- [ ] **Step 6: Add a join-failure classification test**

Add beside the responsiveness regression:

```rust
#[tokio::test]
async fn shell_setup_panic_is_reported_as_a_setup_join_failure() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
    let config = crate::cli::parse_config_from([
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
    .unwrap();

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

- [ ] **Step 7: Run focused shell and constructor regressions**

Run:

```console
cargo +1.98.0 test -p hoimin-cli shell::tests::shell_setup_does_not_block_the_async_runtime -- --exact
cargo +1.98.0 test -p hoimin-cli shell::tests::shell_setup_panic_is_reported_as_a_setup_join_failure -- --exact
cargo +1.98.0 test -p hoimin-cli --test run_e2e shell_context_construction_performs_no_project_io -- --exact
```

Expected: all PASS. The existing E2E test continues to prove that construction creates neither a missing project root nor its configured session database; do not edit `run_e2e.rs`.

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

### Task 4: Pin required Rust and add an isolated latest-stable canary

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

- [ ] **Step 1: Add failing path constants and contract tests**

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

Add a new test class:

```python
class RustToolchainWorkflowContractTests(unittest.TestCase):
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
        self.assertEqual(
            set(declaration["components"]),
            {"clippy", "rustfmt"},
        )

    def test_required_jobs_install_only_the_repository_toolchain(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        self.assertNotIn("RUSTUP_TOOLCHAIN", workflow)
        self.assertNotIn("rustup override", workflow)
        for job_name in REPOSITORY_RUST_JOBS:
            job = job_block(workflow, job_name)
            self.assertEqual(job.count("rustup toolchain install"), 1, job_name)
            self.assertRegex(
                job,
                r"(?m)^        run: rustup toolchain install$",
            )
            self.assertNotIn("rustup default", job, job_name)
            self.assertNotRegex(job, r"cargo \+[^\s]+")

        msrv = job_block(workflow, "msrv")
        shuffle = job_block(workflow, "rust-shuffle")
        self.assertIn("cargo +1.88 check", msrv)
        self.assertIn("cargo +nightly-2026-07-27 test", shuffle)

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
        uses = [step["uses"] for step in job["steps"] if "uses" in step]
        self.assertEqual(
            uses,
            [CHECKOUT_ACTION, SETUP_PYTHON_ACTION, SETUP_UV_ACTION],
        )
        self.assertIn("python-version: '3.14'", workflow)
        commands = [step["run"] for step in job["steps"] if "run" in step]
        self.assertEqual(
            commands,
            [
                "rustup toolchain install stable --profile minimal "
                "--component rustfmt --component clippy",
                "uv sync --frozen",
                "cargo +stable fmt --all -- --check",
                "cargo +stable clippy --workspace --all-targets "
                "--all-features -- -D warnings",
                "cargo +stable test --workspace",
            ],
        )
        self.assertNotIn("rust-stable-canary", CI_WORKFLOW.read_text(encoding="utf-8"))

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
```

- [ ] **Step 2: Run the workflow tests and observe RED**

Run:

```console
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: failures for the missing `rust-toolchain.toml`, missing canary, rolling stable installs, and missing development-guide section; existing MSRV, nightly, trigger, and release tests still pass.

- [ ] **Step 3: Add the exact repository toolchain declaration**

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.98.0"
profile = "minimal"
components = ["clippy", "rustfmt"]
```

- [ ] **Step 4: Make ordinary required jobs consume the repository declaration**

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

Do not add `rustup default`, `RUSTUP_TOOLCHAIN`, `cargo +stable`, or a release-workflow override.

- [ ] **Step 5: Create the isolated canary workflow**

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

- [ ] **Step 6: Document pinned stable separately from MSRV**

Insert this section before `## Minimum supported Rust version` in `docs/development.md`:

```markdown
## Pinned Rust toolchain

The repository root `rust-toolchain.toml` pins the Rust 1.98.0 compiler and the
Clippy and rustfmt components used by local development, required CI, and
release builds. Run ordinary `cargo` commands from the repository root; do not
set a global default toolchain for this project.

Updating the pin is a reviewed compatibility change. Change the exact channel
in `rust-toolchain.toml`, resolve formatter, compiler, and Clippy diagnostics,
then run the complete quality and test commands in this guide in the same pull
request. The weekly latest-stable canary reports when such an update is needed.
Updating the pinned compiler does not raise the minimum supported Rust version.
The MSRV remains the separate contract below.
```

Change the final sentence of the MSRV update paragraph from “Stable CI remains required” to “The pinned stable CI gate remains required” so it cannot be read as moving stable.

- [ ] **Step 7: Run all executable workflow contracts**

Run:

```console
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: all tests PASS, including existing exact release-workflow and trigger-reachability contracts.

- [ ] **Step 8: Verify Rustup resolution and all explicit exceptions**

Run:

```console
rustup show active-toolchain
rustc --version
rg -n "rustup default|rustup toolchain install stable|RUSTUP_TOOLCHAIN|cargo \+stable" .github/workflows/ci.yml .github/workflows/release.yml
rg -n "1\.88|nightly-2026-07-27" .github/workflows/ci.yml Cargo.toml docs/development.md tests/test_ci_workflow.py
```

Expected: the first two commands report Rust 1.98.0 from the repository override; the first `rg` has no matches; the second shows the unchanged MSRV and pinned-nightly contracts.

- [ ] **Step 9: Commit the reproducible CI boundary**

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

- [ ] **Step 1: Run formatting, exact quality, MSRV, and workflow contracts**

Run:

```console
cargo +1.98.0 fmt --all -- --check
cargo +1.98.0 clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.88 check --workspace --all-targets --all-features --locked
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: all PASS.

- [ ] **Step 2: Run the full workspace and independent E2E test processes**

Run as two separate Cargo processes:

```console
cargo +1.98.0 test --workspace
cargo +1.98.0 test -p hoimin-cli --test run_e2e
```

Expected: both PASS. The second command is intentionally independent, matching required CI; it is not a retry of a failed command.

- [ ] **Step 3: Prove the blocking-boundary counterfactual is detected**

Temporarily replace only `prepare_shell_setup` with this inline counterfactual using `apply_patch`:

```rust
async fn prepare_shell_setup(
    config: RunConfig,
    before_start: ShellSetupStartHook,
) -> Result<PreparedShellSetup, String> {
    before_start();
    prepare_shell_setup_sync(config)
}
```

Run:

```console
cargo +1.98.0 test -p hoimin-cli shell::tests::shell_setup_does_not_block_the_async_runtime -- --exact
```

Expected: FAIL after the controller releases the gate, with `shell setup blocked the runtime until its blocking work was released`; it must not hang or reach the six-second total timeout. Restore the exact `spawn_blocking` implementation from Task 3 with `apply_patch`, rerun the same command, and expect PASS. Verify `git diff` contains no counterfactual residue before continuing.

- [ ] **Step 4: Run focused generated mutation testing for the production setup boundary**

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

Expected: baseline passes. Explicit symbols receive the runner's highest ranking; do not add `--file`, which would raise every function in `shell.rs` and waste the bounded budget. Inspect `$mutationOutput\run.json` and `$mutationOutput\report.md`; every generated viable mutation belonging to the two selected functions is killed. The runner may also inventory lower-ranked functions changed from `origin/main`; their presence does not broaden this task's mutation claim. The manual counterfactual in Step 3, rather than cargo-mutants' generated inventory, is the required proof for removing `spawn_blocking`. Do not treat `not_run`, `pending`, `timeout`, `unviable`, or `error` for either selected function as passes.

- [ ] **Step 5: Fail closed on incomplete mutation evidence**

If `report.md` contains a survivor or `run.json` contains `not_run`, `pending`, `timeout`, `unviable`, or `error` for either selected function, do not claim mutation verification and do not add a speculative test. Record the exact generated mutation and outcome, then return it for design review. Continue only when the selected viable inventory is fully killed; an equivalent mutant requires an exact expression-level justification in the execution report.

- [ ] **Step 6: Prove forbidden Windows and IDE paths are absent**

Run:

```console
git diff --name-only origin/main
git diff --check origin/main
git status --short --branch
```

Expected: changed paths are limited to the spec, this plan, the eight implementation files in the File Map, and any focused Rust regression added in `shell.rs` or `report/mod.rs`. There are no changes to Windows resource files or `run_e2e.rs`; `.idea/` remains the sole unrelated untracked path; the status line is `## fix/rust-1.98-ci` with no upstream annotation.

- [ ] **Step 7: Confirm the final commit set without creating a verification-only commit**

Run:

```console
git log --oneline origin/main..HEAD
```

Expected: the two design-document commits, the implementation-plan and plan-review commits, and the four implementation commits from Tasks 1-4. Verification must not create an empty commit.
