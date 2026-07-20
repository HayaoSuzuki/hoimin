# Apple Silicon macOS Support Implementation Plan

> **Superseded:** Do not execute this plan. The verified Darwin behavior rejects
> `RLIMIT_AS`; the active no-release plan is
> [`2026-07-20-apple-silicon-wheel.md`](2026-07-20-apple-silicon-wheel.md),
> which uses process groups and `RLIMIT_CPU` without claiming enforced macOS
> memory limits.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support Apple Silicon macOS (`aarch64-apple-darwin`) with an installable native wheel, CI coverage, and an explicitly opt-in best-effort resource policy.

**Architecture:** macOS continues to use the existing Unix portable process backend: a separate process group plus `RLIMIT_AS` and `RLIMIT_CPU`. It is not a hard, run-wide resource controller, so it must require `--allow-best-effort-memory` and report `best_effort` with a macOS-specific diagnostic. Windows Job Objects and Linux cgroup v2 behavior remain unchanged.

**Tech Stack:** Rust 2024/MSRV 1.85, Tokio process spawning, `libc` Unix APIs, Python 3.14, Maturin native binary wheels, GitHub Actions, pytest wheel smoke test.

## Global Constraints

- Support only Apple Silicon macOS: `aarch64-apple-darwin`; Intel macOS is explicitly out of scope.
- macOS resource control is `best_effort`, never `hard`.
- macOS runs must require `--allow-best-effort-memory`, matching the documented Unix policy.
- Do not add a macOS-only hard-limit backend or new native dependency.
- Preserve the native argv/no-shell execution contract.
- Keep Windows Job Object and Linux cgroup-v2 selection behavior unchanged.
- Release output must contain exactly one macOS wheel target: `aarch64-apple-darwin`.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/hoimin-cli/src/resource/portable.rs` | Enforce opt-in and attach a platform-specific best-effort diagnostic for macOS. |
| `crates/hoimin-cli/tests/process_handler.rs` | Exercise portable-backend policy and descendant cleanup on macOS. |
| `tests/wheel_smoke.py` | Select Apple Silicon macOS wheels when the smoke test runs on Darwin. |
| `.github/workflows/ci.yml` | Run portable-path quality, Rust, and wheel-smoke jobs on an Apple Silicon macOS runner. |
| `.github/workflows/release.yml` | Build, smoke-test, upload, and publish the ARM64 macOS wheel. |
| `pyproject.toml` | Advertise macOS support in package classifiers. |
| `README.md` | State supported macOS architecture and its best-effort resource policy. |
| `docs/development.md` | Describe the macOS local verification command and required flag. |

### Task 1: Make macOS best-effort resource control an explicit opt-in

**Files:**
- Modify: `crates/hoimin-cli/src/resource/portable.rs:13-26`
- Modify: `crates/hoimin-cli/tests/process_handler.rs:724-734`
- Test: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Consumes: `PortableBackend::new(allow_best_effort_memory: bool) -> Result<PortableBackend, ResourceError>`.
- Produces: macOS `ResourceError::BestEffortNotAllowed` without the flag; `ResourceBackend::Portable` with `ResourceMode::BestEffort` and a nonempty macOS diagnostic with the flag.

- [ ] **Step 1: Add Darwin-only failing policy assertions**

  In `crates/hoimin-cli/tests/process_handler.rs`, change the existing Linux-only opt-in test gate to `#[cfg(any(target_os = "linux", target_os = "macos"))]`, keep the Linux assertions, and add a Darwin-specific diagnostic assertion:

  ```rust
  #[cfg(target_os = "macos")]
  assert_eq!(
      PortableBackend::new(true).unwrap().diagnostic(),
      Some("macOS uses per-process RLIMIT_AS/RLIMIT_CPU and process groups"),
  );
  ```

  Add a Darwin-only asynchronous regression next to the existing portable descendant-cancellation tests. Start a Python parent that writes its child PID to a marker, launches a sleeping child, and waits; cancel the `ProcessHandler` request; then assert `wait_until_process_stops(child_pid).await`.

- [ ] **Step 2: Run the policy test and verify it fails on a macOS ARM64 machine**

  Run:

  ```console
  cargo test -p hoimin-cli --test process_handler normal_linux_portable_backend_requires_explicit_opt_in -- --exact
  ```

  Expected: on macOS, failure because `PortableBackend::new(false)` currently succeeds and `PortableBackend::new(true)` has no diagnostic.

- [ ] **Step 3: Implement macOS policy and diagnostic**

  Replace the Linux-specific constructor guard in `crates/hoimin-cli/src/resource/portable.rs` with target-specific branches that preserve Linux behavior and add macOS behavior:

  ```rust
  #[cfg(target_os = "macos")]
  {
      if !allow_best_effort_memory {
          return Err(ResourceError::BestEffortNotAllowed(
              "macOS uses per-process RLIMIT_AS/RLIMIT_CPU and process groups".into(),
          ));
      }
      return Ok(Self::with_diagnostic(
          "macOS uses per-process RLIMIT_AS/RLIMIT_CPU and process groups".into(),
      ));
  }
  ```

  Retain the existing Linux rejection text and the existing non-Linux/non-macOS no-op binding:

  ```rust
  #[cfg(not(any(target_os = "linux", target_os = "macos")))]
  let _ = allow_best_effort_memory;
  ```

  Do not alter `configure_command`: its existing `#[cfg(unix)]` `setpgid`, `RLIMIT_AS`, and `RLIMIT_CPU` setup is the macOS mechanism.

- [ ] **Step 4: Run focused portable-path tests on macOS**

  Run:

  ```console
  cargo test -p hoimin-cli --test process_handler portable -- --nocapture
  cargo test -p hoimin-cli --test process_handler normal_linux_portable_backend_requires_explicit_opt_in -- --exact
  ```

  Expected: all selected tests pass; the second command name remains historical but runs under its widened `cfg` gate.

- [ ] **Step 5: Commit the resource policy change**

  ```console
  git add crates/hoimin-cli/src/resource/portable.rs crates/hoimin-cli/tests/process_handler.rs
  git commit -m "feat: require best-effort opt-in on macOS"
  ```

### Task 2: Make the wheel smoke test recognize ARM64 macOS artifacts

**Files:**
- Modify: `tests/wheel_smoke.py:34-42`
- Test: `tests/wheel_smoke.py`

**Interfaces:**
- Consumes: `wheel_path() -> Path` and a Maturin wheel in `target/wheels`.
- Produces: on `sys.platform == "darwin"` with ARM64 Python, selection of a wheel whose filename contains both `macosx` and `arm64`.

- [ ] **Step 1: Add a failing unit-testable compatibility predicate**

  Extract the platform test from `wheel_path` so it can be exercised without building a wheel:

  ```python
  def compatible_wheel_name(name: str, *, platform: str, machine: str) -> bool:
      if platform == "win32":
          return "win_amd64" in name
      if platform.startswith("linux"):
          return "x86_64" in name
      if platform == "darwin":
          return machine == "arm64" and "macosx" in name and "arm64" in name
      return False
  ```

  Add `tests/test_wheel_smoke.py` with assertions for `win_amd64`, Linux `x86_64`, macOS `macosx_11_0_arm64`, and rejection of `macosx_10_12_x86_64` on ARM64.

- [ ] **Step 2: Run the new unit test and verify it fails**

  Run:

  ```console
  uv run --frozen python -m pytest tests/test_wheel_smoke.py -q
  ```

  Expected: FAIL because `compatible_wheel_name` does not yet exist.

- [ ] **Step 3: Implement platform-aware wheel selection**

  In `tests/wheel_smoke.py`, import `platform` and implement `compatible_wheel_name`. Replace the current inline list condition with:

  ```python
  compatible = [
      wheel
      for wheel in wheels
      if compatible_wheel_name(
          wheel.name, platform=sys.platform, machine=platform.machine().lower()
      )
  ]
  ```

  The `HOIMIN_WHEEL` override remains authoritative and must not be filtered.

- [ ] **Step 4: Run wheel selection tests and the full macOS wheel smoke test**

  Run:

  ```console
  uv run --frozen python -m pytest tests/test_wheel_smoke.py -q
  uvx maturin build --release --target aarch64-apple-darwin
  uv run --frozen python tests/wheel_smoke.py
  ```

  Expected: unit tests pass; the wheel smoke test installs and executes the generated `macosx_*_arm64` wheel successfully.

- [ ] **Step 5: Commit wheel smoke support**

  ```console
  git add tests/wheel_smoke.py tests/test_wheel_smoke.py
  git commit -m "test: recognize Apple Silicon macOS wheels"
  ```

### Task 3: Add Apple Silicon macOS to continuous integration and releases

**Files:**
- Modify: `.github/workflows/ci.yml:11-107`
- Modify: `.github/workflows/release.yml:24-87`
- Test: `.github/workflows/ci.yml`
- Test: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: a GitHub-hosted Apple Silicon macOS runner and the existing `maturin-action` build configuration.
- Produces: CI coverage for macOS portable execution and a `wheels-macos-aarch64` release artifact consumed by the existing publish job.

- [ ] **Step 1: Add macOS to the failing CI matrices**

  In each of the `quality`, `rust`, `core-dependency-purity`, and `wheel-smoke` job matrices, extend the OS list:

  ```yaml
  os: [ubuntu-latest, windows-latest, macos-14]
  ```

  Keep `linux-best-effort` and `linux-cgroup-v2-hard` Linux-only. `macos-14` is the Apple Silicon runner choice; do not add an Intel macOS runner.

- [ ] **Step 2: Add a release job that initially fails publication dependency checks**

  Add `macos-arm64-wheel` after `linux-wheel`, patterned after the existing jobs, with:

  ```yaml
  runs-on: macos-14
  target: aarch64-apple-darwin
  name: wheels-macos-aarch64
  ```

  Include `uv run --frozen python tests/wheel_smoke.py` before upload. Add `macos-arm64-wheel` to `publish.needs`.

- [ ] **Step 3: Validate workflow syntax and inspect the release graph**

  Run:

  ```console
  ruby -e "require 'yaml'; %w[.github/workflows/ci.yml .github/workflows/release.yml].each { |p| YAML.load_file(p); puts \"OK #{p}\" }"
  git diff --check
  ```

  Expected: both workflow files parse and the diff has no whitespace errors. Push a branch and verify the PR runs all four matrix jobs on `macos-14`; create a disposable version tag only after ordinary CI is green to exercise the release workflow without publishing.

- [ ] **Step 4: Correct runner/action incompatibilities revealed by macOS CI**

  Keep changes limited to platform invocation details. In particular, retain shell-independent Rust/Python commands; do not add Bash-only syntax to matrix-wide `run` steps. If Maturin emits an ARM64 wheel with a different deployment tag, adjust only `compatible_wheel_name` to accept the filename pattern while still requiring `macosx` and `arm64`.

- [ ] **Step 5: Commit CI and release coverage**

  ```console
  git add .github/workflows/ci.yml .github/workflows/release.yml
  git commit -m "ci: build and test Apple Silicon macOS wheels"
  ```

### Task 4: Publish the macOS support contract in metadata and documentation

**Files:**
- Modify: `pyproject.toml:12-18`
- Modify: `README.md:57-60,127-131`
- Modify: `docs/development.md:3-20`
- Test: `tests/wheel_smoke.py`

**Interfaces:**
- Consumes: the implemented macOS `best_effort` policy and ARM64 release wheel.
- Produces: package metadata and documentation that accurately constrain macOS support to Apple Silicon and require explicit opt-in.

- [ ] **Step 1: Add macOS package metadata**

  Add this classifier to `[project].classifiers` in `pyproject.toml`:

  ```toml
  "Operating System :: MacOS :: MacOS X",
  ```

- [ ] **Step 2: Update README support and safety language**

  Amend the resource-limits paragraph to say that macOS uses the best-effort Unix process-group and rlimit path and requires `--allow-best-effort-memory`; only Windows Job Objects and delegated Linux cgroup v2 are hard backends. Amend the wheel section to list Windows x86_64, Linux x86_64, and Apple Silicon macOS ARM64 as release artifacts, and state Intel macOS is not published.

- [ ] **Step 3: Update local development verification**

  In `docs/development.md`, retain the shared commands and add this macOS-specific release-wheel check:

  ```console
  uvx maturin build --release --target aarch64-apple-darwin
  uv run --frozen python tests/wheel_smoke.py
  ```

  State that normal `hoimin run` invocations on macOS must include `--allow-best-effort-memory`.

- [ ] **Step 4: Verify metadata, documentation claims, and full local test suite on macOS**

  Run:

  ```console
  cargo fmt --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace
  uv run --frozen python -m pytest tests/test_wheel_smoke.py -q
  uvx maturin build --release --target aarch64-apple-darwin
  uv run --frozen python tests/wheel_smoke.py
  ```

  Expected: all commands exit 0. Inspect the generated wheel name to confirm it contains `macosx` and `arm64`; inspect `METADATA` through the smoke test to confirm the unchanged Python and dependency contract.

- [ ] **Step 5: Commit the support contract**

  ```console
  git add pyproject.toml README.md docs/development.md
  git commit -m "docs: document Apple Silicon macOS support"
  ```

## Final Verification and Release Readiness

- [ ] Run the exact macOS commands from Task 4 on Apple Silicon hardware.
- [ ] Confirm the macOS CI matrix is green for quality, Rust, core dependency purity, and wheel smoke.
- [ ] Confirm a tagged release uploads `wheels-macos-aarch64` and that the publish job receives it with the Windows and Linux artifacts.
- [ ] Install the published release with `uvx hoimin --help` on a clean Apple Silicon macOS environment, then run the README example with `--allow-best-effort-memory`.
- [ ] Do not claim hard memory/process enforcement on macOS in CLI output, README, package metadata, or release notes.
