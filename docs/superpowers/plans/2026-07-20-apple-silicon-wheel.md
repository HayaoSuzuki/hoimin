# Apple Silicon wheel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apple Silicon Mac で macOS arm64 wheel を生成し、ローカルの wheel を `uvx --from` で実行できることを自動検証する。

**Architecture:** `tests/wheel_smoke.py` の platform 判定を小さな純粋関数に分離し、macOS では arm64 tag の wheel だけを選択する。通常 CI の Rust と wheel smoke matrix に GitHub-hosted Apple Silicon runner の `macos-14` を追加し、release workflow と publish の依存関係は変更しない。

**Tech Stack:** Python 3.14 standard library `unittest`、Maturin、uv/uvx、GitHub Actions、Rust 2024。

## Global Constraints

- macOS の実行プラットフォーム識別子は `sys.platform == "darwin"` とする。
- macOS では filename に `arm64` を含む wheel だけを smoke test の対象にする。
- Linux の `x86_64` と Windows の `win_amd64` の既存 wheel 選択条件は保持する。
- 通常 CI の `rust` と `wheel-smoke` だけに `macos-14` を追加する。
- release workflow、artifact upload、PyPI Trusted Publishing、PyPI 公開は変更しない。
- Apple Silicon のローカル検証は `uv run maturin build --release` と `uv run --frozen python tests/wheel_smoke.py` で行う。

---

## File structure

- Create: `tests/test_wheel_smoke.py` — platform と wheel filename の互換性判定を OS ごとに unit test する。
- Modify: `tests/wheel_smoke.py:32-42` — wheel 選択の判定を `is_compatible_wheel` として切り出し、macOS arm64 を受け入れる。
- Modify: `.github/workflows/ci.yml:27-48,87-107` — Rust と wheel smoke の matrix に `macos-14` を追加する。

### Task 1: macOS arm64 wheel を smoke test が選択するようにする

**Files:**
- Create: `tests/test_wheel_smoke.py`
- Modify: `tests/wheel_smoke.py:32-42`

**Interfaces:**
- Consumes: `Path` 形式の wheel filename と `sys.platform` 文字列。
- Produces: `is_compatible_wheel(wheel: Path, platform: str) -> bool`。`wheel_path()` はこの関数を使い、実行中の platform と互換な最新 wheel を返す。
- Invariant: `darwin` は arm64 wheel のみ、`win32` は `win_amd64` wheel のみ、Linux は `x86_64` wheel のみを選択する。

- [ ] **Step 1: platform ごとの互換性判定をテストする**

`tests/test_wheel_smoke.py` を作成し、生成済みファイルを必要としない filename ベースのテストを追加する。

```python
from pathlib import Path
import unittest

from wheel_smoke import is_compatible_wheel


class CompatibleWheelTests(unittest.TestCase):
    def test_accepts_only_arm64_wheels_on_macos(self) -> None:
        arm64 = Path("hoimin-0.1.0-py3-none-macosx_11_0_arm64.whl")
        x86_64 = Path("hoimin-0.1.0-py3-none-macosx_10_12_x86_64.whl")
        universal2 = Path("hoimin-0.1.0-py3-none-macosx_10_12_universal2.whl")

        self.assertTrue(is_compatible_wheel(arm64, "darwin"))
        self.assertFalse(is_compatible_wheel(x86_64, "darwin"))
        self.assertFalse(is_compatible_wheel(universal2, "darwin"))

    def test_preserves_linux_and_windows_wheel_selection(self) -> None:
        linux = Path("hoimin-0.1.0-py3-none-manylinux_2_17_x86_64.whl")
        windows = Path("hoimin-0.1.0-py3-none-win_amd64.whl")

        self.assertTrue(is_compatible_wheel(linux, "linux"))
        self.assertFalse(is_compatible_wheel(windows, "linux"))
        self.assertTrue(is_compatible_wheel(windows, "win32"))
        self.assertFalse(is_compatible_wheel(linux, "win32"))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: 新しい unit test が失敗することを確認する**

Run: `uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v`

Expected: FAIL。`wheel_smoke` から `is_compatible_wheel` を import できないことを示す `ImportError` が出る。

- [ ] **Step 3: macOS arm64 を含む互換性関数を実装する**

`tests/wheel_smoke.py` の `wheel_path()` の前に次の関数を追加する。

```python
def is_compatible_wheel(wheel: Path, platform: str) -> bool:
    return (
        (platform == "win32" and "win_amd64" in wheel.name)
        or (platform.startswith("linux") and "x86_64" in wheel.name)
        or (platform == "darwin" and "arm64" in wheel.name)
    )
```

`wheel_path()` の既存の `compatible` 代入を次に置き換える。

```python
    compatible = [wheel for wheel in wheels if is_compatible_wheel(wheel, sys.platform)]
```

- [ ] **Step 4: platform 判定の unit test が通ることを確認する**

Run: `uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v`

Expected: PASS。macOS arm64 の受け入れ、macOS x86_64/universal2 の拒否、Linux と Windows の既存条件がすべて通る。

- [ ] **Step 5: Apple Silicon 上で生成した wheel の end-to-end smoke を通す**

Run: `uv run maturin build --release && uv run --frozen python tests/wheel_smoke.py`

Expected: PASS。`target/wheels/` に `macosx_*_arm64.whl` があり、smoke script がそれを選択して `uvx --from`、venv への install、`hoimin run` を成功させる。

- [ ] **Step 6: wheel 選択の変更をコミットする**

```console
git add tests/wheel_smoke.py tests/test_wheel_smoke.py
git commit -m "test: support Apple Silicon wheel smoke"
```

### Task 2: 通常 CI で Apple Silicon wheel を検証する

**Files:**
- Modify: `.github/workflows/ci.yml:27-48`
- Modify: `.github/workflows/ci.yml:87-107`

**Interfaces:**
- Consumes: GitHub Actions matrix の `matrix.os`。
- Produces: `Rust (macos-14)` と `Wheel smoke (macos-14)` の CI jobs。各 job は既存の checkout、Python 3.14、uv、stable Rust の手順をそのまま実行する。
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

- [ ] **Step 3: Apple Silicon のローカル wheel smoke を再実行する**

Run: `uv run maturin build --release && uv run --frozen python tests/wheel_smoke.py`

Expected: PASS。ローカル macOS arm64 wheel を `uvx --from` と installed CLI の両方で実行できる。pull request では CI が `Rust (macos-14)` と `Wheel smoke (macos-14)` を追加で実行する。

- [ ] **Step 4: CI matrix の変更をコミットする**

```console
git add .github/workflows/ci.yml
git commit -m "ci: test Apple Silicon wheels"
```

## Final verification

- [ ] Run `uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v` and expect PASS.
- [ ] Run `uv run maturin build --release && uv run --frozen python tests/wheel_smoke.py` and expect PASS with an arm64 macOS wheel selected.
- [ ] Run `git diff --check` and expect no output.
- [ ] Run `test -z "$(git diff --name-only HEAD~2..HEAD -- .github/workflows/release.yml)"` and expect PASS, confirming neither task commit modified the release workflow.

## Self-review

- Spec coverage: Task 1 implements macOS arm64 selection and local `uvx --from` smoke validation. Task 2 adds `macos-14` only to the Rust and wheel smoke CI jobs while preserving the non-publication boundary.
- Placeholder scan: no deferred work, unqualified test instruction, or undefined interface is present.
- Type consistency: `is_compatible_wheel` has the same `Path`, `str`, and `bool` interface in the tests and in `wheel_path()`.
