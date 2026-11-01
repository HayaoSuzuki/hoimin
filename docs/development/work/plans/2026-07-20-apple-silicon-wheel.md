# Apple Silicon wheel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apple Silicon Mac で macOS arm64 wheel を生成し、`uvx --from` で `hoimin run` まで実行できることをローカルと通常 CI で検証する。

**Architecture:** macOS の portable backend を Darwin で使える process group と `RLIMIT_CPU` に限定し、`RLIMIT_AS` は設定しない。これを明示的な best-effort resource policy として扱ったうえで、wheel smoke の Darwin arm64 選択と `macos-14` CI coverage を追加する。release workflow と PyPI 公開は変更しない。

**Tech Stack:** Rust 2024、Tokio process、libc Unix APIs、Python 3.14 standard library `unittest`、Maturin、uv/uvx、GitHub Actions。

## Global Constraints

- 対象は Apple Silicon macOS (`aarch64-apple-darwin`) のみであり、Intel macOS と universal2 wheel は対象外とする。
- macOS resource mode は `best_effort` であり、`--max-memory` は強制しない。
- macOS の通常実行は `--allow-best-effort-memory` を必須にし、diagnostic は process group と `RLIMIT_CPU` だけを使うことを明示する。
- macOS の child process 設定では `setpgid` と `RLIMIT_CPU` を使い、`RLIMIT_AS` を設定しない。
- Linux の `RLIMIT_AS`/`RLIMIT_CPU` portable path と cgroup v2 backend、および Windows Job Object backend は変更しない。
- macOS wheel は filename に `macosx` と `arm64` を含むものだけを smoke test の対象にする。
- 通常 CI の `rust` と `wheel-smoke` だけに `macos-14` を追加する。
- release workflow、artifact upload、PyPI Trusted Publishing、PyPI 公開は変更しない。

---

## File structure

- Modify: `crates/hoimin-cli/src/resource/portable.rs` — Darwin の best-effort policy と `pre_exec` resource setup を定義する。
- Modify: `crates/hoimin-cli/tests/process_handler.rs` — macOS policy と `RLIMIT_AS` なしの child spawn を regression test する。
- Create: `tests/test_wheel_smoke.py` — OS/architecture と wheel filename の互換性判定を unit test する。
- Modify: `tests/wheel_smoke.py` — Darwin arm64 wheel を選択して既存の `uvx --from` smoke を実行する。
- Modify: `.github/workflows/ci.yml` — Rust と wheel smoke を `macos-14` で実行する。

### Task 1: macOS portable backend を実行可能な best-effort policy にする

**Files:**
- Modify: `crates/hoimin-cli/src/resource/portable.rs:13-26,135-180`
- Modify: `crates/hoimin-cli/tests/process_handler.rs:400-720`

**Interfaces:**
- Consumes: `PortableBackend::new(allow_best_effort_memory: bool) -> Result<PortableBackend, ResourceError>` と `ProcessLimits`。
- Produces: macOS では `PortableBackend::new(false)` が `ResourceError::BestEffortNotAllowed` を返し、`PortableBackend::new(true)` が `ResourceMode::BestEffort` と macOS diagnostic を返す。macOS child は `setpgid` と `RLIMIT_CPU` を設定して spawn する。
- Invariant: Darwin の `pre_exec` は `libc::RLIMIT_AS` を呼ばない。Linux と Windows の設定経路は変えない。

- [ ] **Step 1: macOS policy と spawn の失敗テストを書く**

`crates/hoimin-cli/tests/process_handler.rs` の `mod portable` に、次の macOS-only tests を追加する。diagnostic の文字列は production code と完全に一致させる。

```rust
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_portable_backend_requires_explicit_best_effort_opt_in() {
        let error = PortableBackend::new(false).unwrap_err();
        assert!(error.to_string().contains("--allow-best-effort-memory"));

        let backend = PortableBackend::new(true).unwrap();
        assert_eq!(backend.mode(), ResourceMode::BestEffort);
        assert_eq!(
            backend.diagnostic(),
            Some("macOS uses process groups and RLIMIT_CPU; max-memory is not enforced"),
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_portable_backend_spawns_without_virtual_memory_rlimit() {
        let output = tempfile::tempdir().unwrap();
        let handler = portable_handler(Utf8Path::from_path(output.path()).unwrap());

        let event = handler
            .handle(run_python(
                70,
                "raise SystemExit(0)",
                limits(Duration::from_secs(5), 64),
            ))
            .await
            .unwrap();

        assert_eq!(event.termination, ProcessTermination::Exit(0));
    }
```

- [ ] **Step 2: 新しい spawn test が現在の macOS 実装で失敗することを確認する**

Run: `cargo test -p hoimin-cli --test process_handler macos_portable_backend_spawns_without_virtual_memory_rlimit -- --exact`

Expected: FAIL。`process.spawn` の原因が `Invalid argument (os error 22)` である。現在の `#[cfg(unix)]` path は Darwin で `RLIMIT_AS` を設定しようとするためである。

- [ ] **Step 3: Darwin の opt-in policy と process setup を実装する**

`crates/hoimin-cli/src/resource/portable.rs` の module scope に次を追加する。

```rust
#[cfg(target_os = "macos")]
const MACOS_BEST_EFFORT_DIAGNOSTIC: &str =
    "macOS uses process groups and RLIMIT_CPU; max-memory is not enforced";
```

`PortableBackend::new` の body を target-specific な末尾 expression に置き換える。

```rust
        #[cfg(target_os = "linux")]
        {
            if !allow_best_effort_memory {
                return Err(ResourceError::BestEffortNotAllowed(
                    "portable Linux uses per-process RLIMIT_AS/RLIMIT_CPU and process groups".into(),
                ));
            }
            Ok(Self { diagnostic: None })
        }
        #[cfg(target_os = "macos")]
        {
            if !allow_best_effort_memory {
                return Err(ResourceError::BestEffortNotAllowed(
                    MACOS_BEST_EFFORT_DIAGNOSTIC.into(),
                ));
            }
            Ok(Self::with_diagnostic(
                MACOS_BEST_EFFORT_DIAGNOSTIC.into(),
            ))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = allow_best_effort_memory;
            Ok(Self { diagnostic: None })
        }
```

既存の `#[cfg(unix)] fn configure_command` を `#[cfg(all(unix, not(target_os = "macos")))]` に狭め、現行の `setpgid`、`RLIMIT_AS`、`RLIMIT_CPU` の実装をそのまま残す。その直後に macOS-only function を追加する。

```rust
#[cfg(target_os = "macos")]
fn configure_command(command: &mut Command, limits: ProcessLimits) -> Result<(), ResourceError> {
    use std::os::unix::process::CommandExt;

    let cpu_seconds = limits
        .timeout
        .as_secs()
        .saturating_add(u64::from(limits.timeout.subsec_nanos() != 0))
        .max(1);
    unsafe {
        command.as_std_mut().pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            let cpu = libc::rlimit {
                rlim_cur: cpu_seconds as libc::rlim_t,
                rlim_max: cpu_seconds as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, std::ptr::addr_of!(cpu)) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}
```

- [ ] **Step 4: macOS regression and portable-path tests を通す**

Run: `cargo test -p hoimin-cli --test process_handler macos_portable_backend_requires_explicit_best_effort_opt_in -- --exact && cargo test -p hoimin-cli --test process_handler portable -- --nocapture`

Expected: PASS。macOS は opt-in と diagnostic を要求し、portable tests は `process.spawn` の `os error 22` なしに通る。Linux-only cgroup tests はこの command の対象外である。

- [ ] **Step 5: Task 1 をコミットする**

```console
git add crates/hoimin-cli/src/resource/portable.rs crates/hoimin-cli/tests/process_handler.rs
git commit -m "fix: support portable processes on macOS"
```

### Task 2: wheel smoke が Apple Silicon macOS artifact を選択するようにする

**Files:**
- Create: `tests/test_wheel_smoke.py`
- Modify: `tests/wheel_smoke.py:1-42`

**Interfaces:**
- Consumes: `Path` の wheel filename、`sys.platform`、`platform.machine().lower()`。
- Produces: `is_compatible_wheel(wheel: Path, system: str, machine: str) -> bool`。`wheel_path()` はこの predicate を使い、現行 host と互換な最新 wheel を返す。
- Invariant: Darwin arm64 では `macosx` と `arm64` の両方を含む wheel だけを選択し、`HOIMIN_WHEEL` override は従来どおり無条件で使う。

- [ ] **Step 1: platform/architecture ごとの wheel 判定 test を書く**

`tests/test_wheel_smoke.py` を作成する。

```python
from pathlib import Path
import unittest

from wheel_smoke import is_compatible_wheel


class CompatibleWheelTests(unittest.TestCase):
    def test_accepts_only_arm64_macos_wheels_on_apple_silicon(self) -> None:
        arm64 = Path("hoimin-0.1.0-py3-none-macosx_11_0_arm64.whl")
        x86_64 = Path("hoimin-0.1.0-py3-none-macosx_10_12_x86_64.whl")
        universal2 = Path("hoimin-0.1.0-py3-none-macosx_10_12_universal2.whl")

        self.assertTrue(is_compatible_wheel(arm64, "darwin", "arm64"))
        self.assertFalse(is_compatible_wheel(x86_64, "darwin", "arm64"))
        self.assertFalse(is_compatible_wheel(universal2, "darwin", "arm64"))
        self.assertFalse(is_compatible_wheel(arm64, "darwin", "x86_64"))

    def test_preserves_linux_and_windows_wheel_selection(self) -> None:
        linux = Path("hoimin-0.1.0-py3-none-manylinux_2_17_x86_64.whl")
        windows = Path("hoimin-0.1.0-py3-none-win_amd64.whl")

        self.assertTrue(is_compatible_wheel(linux, "linux", "x86_64"))
        self.assertFalse(is_compatible_wheel(windows, "linux", "x86_64"))
        self.assertTrue(is_compatible_wheel(windows, "win32", "amd64"))
        self.assertFalse(is_compatible_wheel(linux, "win32", "amd64"))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: 新しい unit test が失敗することを確認する**

Run: `uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v`

Expected: FAIL。`wheel_smoke` から `is_compatible_wheel` を import できないことを示す `ImportError` が出る。

- [ ] **Step 3: platform-aware wheel predicate を実装する**

`tests/wheel_smoke.py` に `import platform` を追加し、`wheel_path()` の前に次を実装する。

```python
def is_compatible_wheel(wheel: Path, system: str, machine: str) -> bool:
    if system == "win32":
        return "win_amd64" in wheel.name
    if system.startswith("linux"):
        return "x86_64" in wheel.name
    if system == "darwin":
        return machine == "arm64" and "macosx" in wheel.name and "arm64" in wheel.name
    return False
```

`wheel_path()` の `compatible` 代入を次に置き換える。

```python
    compatible = [
        wheel
        for wheel in wheels
        if is_compatible_wheel(wheel, sys.platform, platform.machine().lower())
    ]
```

- [ ] **Step 4: unit test と Apple Silicon wheel smoke を通す**

Run: `uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v && uv run maturin build --release && uv run --frozen python tests/wheel_smoke.py`

Expected: PASS。`target/wheels/` の `macosx_*_arm64.whl` が選択され、`uvx --from`、venv install、`--allow-best-effort-memory` を含む `hoimin run` が成功する。

- [ ] **Step 5: Task 2 をコミットする**

```console
git add tests/wheel_smoke.py tests/test_wheel_smoke.py
git commit -m "test: support Apple Silicon wheel smoke"
```

### Task 3: 通常 CI で Apple Silicon wheel を検証する

**Files:**
- Modify: `.github/workflows/ci.yml:27-48`
- Modify: `.github/workflows/ci.yml:87-107`

**Interfaces:**
- Consumes: GitHub Actions matrix の `matrix.os`。
- Produces: `Rust (macos-14)` と `Wheel smoke (macos-14)` jobs。各 job は既存の checkout、Python 3.14、uv、stable Rust の手順をそのまま実行する。
- Invariant: `quality`、`contracts`、`core-dependency-purity`、Linux-specific jobs、release workflow は変更しない。

- [ ] **Step 1: macOS runner を Rust と wheel smoke の matrix に追加する**

`.github/workflows/ci.yml` の `rust` job と `wheel-smoke` job の `matrix.os` をそれぞれ次の値にする。

```yaml
        os: [ubuntu-latest, windows-latest, macos-14]
```

変更対象は 2 行だけである。`quality` job の matrix と `.github/workflows/release.yml` は編集しない。

- [ ] **Step 2: workflow 差分と matrix の対象数を確認する**

Run: `git diff --check && test "$(rg -F -c 'os: [ubuntu-latest, windows-latest, macos-14]' .github/workflows/ci.yml)" -eq 2 && test -z "$(git diff -- .github/workflows/release.yml)"`

Expected: PASS。空白エラーはなく、`macos-14` を含む matrix は `rust` と `wheel-smoke` の 2 個だけであり、release workflow に差分はない。

- [ ] **Step 3: Apple Silicon の local smoke を再実行する**

Run: `uv run maturin build --release && uv run --frozen python tests/wheel_smoke.py`

Expected: PASS。local macOS arm64 wheel を `uvx --from` と installed CLI の両方で実行できる。pull request では CI が `Rust (macos-14)` と `Wheel smoke (macos-14)` を追加で実行する。

- [ ] **Step 4: Task 3 をコミットする**

```console
git add .github/workflows/ci.yml
git commit -m "ci: test Apple Silicon wheels"
```

## Final verification

- [ ] Run `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace` and expect PASS.
- [ ] Run `uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v` and expect PASS.
- [ ] Run `uv run maturin build --release && uv run --frozen python tests/wheel_smoke.py` and expect PASS with an arm64 macOS wheel selected.
- [ ] Run `git diff --check` and expect no output.
- [ ] Run `test -z "$(git diff --name-only HEAD~3..HEAD -- .github/workflows/release.yml)"` and expect PASS, confirming no task commit modified the release workflow.

## Self-review

- Spec coverage: Task 1 removes the Darwin-incompatible `RLIMIT_AS` call while preserving explicit best-effort policy. Task 2 implements arm64 macOS wheel selection and end-to-end `uvx --from` smoke. Task 3 adds only the approved macOS CI jobs and leaves release publication untouched.
- Placeholder scan: every task includes exact changed files, commands, expected outcomes, and commit commands.
- Type consistency: `is_compatible_wheel` has the same `Path`, `str`, `str`, and `bool` interface in its tests and in `wheel_path()`.
