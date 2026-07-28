# Reproducible Rust Test Shuffle CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an independent, reproducible nightly CI job that randomizes the complete Rust workspace test order while preserving all stable gates.

**Architecture:** Protect the workflow and replay documentation with a standard-library Python contract test. Add one pinned-nightly Ubuntu job that prepares the controlled Python environment and delegates shuffling to Rust's standard test harness.

**Tech Stack:** GitHub Actions, rustup, nightly libtest, Cargo, Python unittest

## Global Constraints

- Existing stable formatting, Clippy, Rust, contract, and platform jobs remain unchanged.
- Nightly is pinned to `nightly-2026-07-27`.
- The initial shuffle job runs only on `ubuntu-latest`.
- The complete workspace runs with `-Z unstable-options --shuffle`.
- CI-generated seeds vary between runs and remain visible in failure logs.
- Documentation includes exact `--shuffle-seed` replay syntax.
- No third-party Rust test framework or nextest dependency is added.
- Release workflows are not modified.

---

### Task 1: Add a failing workflow and documentation contract

**Files:**
- Create: `tests/test_ci_workflow.py`
- Read: `.github/workflows/ci.yml`
- Read: `docs/development.md`

**Interfaces:**
- Consumes: repository-relative workflow and development guide
- Produces: `ShuffleWorkflowContractTests` guarding stable coverage, job isolation, pinning, shuffle flags, and replay documentation

- [ ] **Step 1: Write the workflow block extractor and failing tests**

Create `tests/test_ci_workflow.py`:

```python
from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
DEVELOPMENT_GUIDE = ROOT / "docs" / "development.md"


def job_block(workflow: str, job_name: str) -> str:
    marker = f"  {job_name}:\n"
    start = workflow.index(marker)
    following = workflow[start + len(marker) :]
    next_job = re.search(r"^  [a-z0-9-]+:\n", following, re.MULTILINE)
    end = len(workflow) if next_job is None else start + len(marker) + next_job.start()
    return workflow[start:end]


class ShuffleWorkflowContractTests(unittest.TestCase):
    def test_shuffle_job_is_pinned_isolated_and_complete(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("cargo test --workspace", job_block(workflow, "rust"))
        shuffle = job_block(workflow, "rust-shuffle")
        self.assertIn("needs: quality", shuffle)
        self.assertIn("runs-on: ubuntu-latest", shuffle)
        self.assertIn("nightly-2026-07-27", shuffle)
        self.assertIn("uv sync --frozen", shuffle)
        self.assertIn(
            "cargo +nightly-2026-07-27 test --workspace -- "
            "-Z unstable-options --shuffle",
            shuffle,
        )

    def test_development_guide_documents_seed_replay(self) -> None:
        guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

        self.assertIn("--shuffle", guide)
        self.assertIn("--shuffle-seed <SEED>", guide)
        self.assertIn("nightly-2026-07-27", guide)
```

- [ ] **Step 2: Run the contract and verify RED**

Run:

```bash
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: two failures: missing `rust-shuffle` job and missing shuffle replay documentation.

- [ ] **Step 3: Commit the failing contract**

```bash
git add tests/test_ci_workflow.py
git commit -m "test: require randomized Rust CI"
```

### Task 2: Add the pinned nightly shuffle job

**Files:**
- Modify: `.github/workflows/ci.yml`
- Test: `tests/test_ci_workflow.py`

**Interfaces:**
- Consumes: existing `quality` job and controlled `.venv` convention
- Produces: independent `rust-shuffle` job on Ubuntu

- [ ] **Step 1: Add the independent job after the stable Rust job**

Add:

```yaml
  rust-shuffle:
    needs: quality
    name: Rust randomized order (nightly-2026-07-27)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10 # v6.0.3
      - uses: actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1 # v6.3.0
        with:
          python-version: '3.14'
      - uses: astral-sh/setup-uv@08807647e7069bb48b6ef5acd8ec9567f424441b # v8.1.0
        with:
          enable-cache: true
      - name: Install pinned nightly Rust
        run: rustup toolchain install nightly-2026-07-27 --profile minimal
      - run: uv sync --frozen
      - name: Run Rust tests in randomized order
        run: cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

- [ ] **Step 2: Run the workflow contract**

Run:

```bash
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: workflow test passes; documentation test remains failing.

- [ ] **Step 3: Check the workflow diff and stable-job invariants**

Run:

```bash
git diff --check
git diff -- .github/workflows/ci.yml
```

Expected: only one new job; existing stable job commands are unchanged.

- [ ] **Step 4: Commit the CI job**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: randomize Rust test order"
```

### Task 3: Document deterministic seed replay

**Files:**
- Modify: `docs/development.md`
- Test: `tests/test_ci_workflow.py`

**Interfaces:**
- Consumes: pinned nightly command from Task 2
- Produces: copyable random run and deterministic replay commands

- [ ] **Step 1: Add a concise randomized-order section**

After the normal development commands, document:

````markdown
### Reproduce randomized Rust test order

CI supplements the stable cross-platform suite with Rust's standard nightly test harness in
randomized order:

```console
rustup toolchain install nightly-2026-07-27 --profile minimal
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

The harness prints the generated seed. Replay a failing order exactly with:

```console
cargo +nightly-2026-07-27 test --workspace -- \
  -Z unstable-options --shuffle-seed <SEED>
```

The nightly job supplements rather than replaces the stable Ubuntu, Windows, and macOS
test jobs.
````

- [ ] **Step 2: Run the complete workflow contract**

Run:

```bash
uv run --frozen python -m unittest tests/test_ci_workflow.py -v
```

Expected: 2 passed.

- [ ] **Step 3: Run all Python contracts**

Run:

```bash
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: all tests pass, including the two new workflow contracts.

- [ ] **Step 4: Commit the documentation**

```bash
git add docs/development.md
git commit -m "docs: explain Rust shuffle seed replay"
```

### Task 4: Execute the real pinned-nightly shuffle

**Files:**
- Inspect: `.github/workflows/ci.yml`
- Inspect: `docs/development.md`

**Interfaces:**
- Consumes: pinned nightly job command
- Produces: local evidence that the exact CI command is supported and the current suite is order-independent for one generated seed

- [ ] **Step 1: Install the exact pinned nightly toolchain**

Run:

```bash
rustup toolchain install nightly-2026-07-27 --profile minimal
```

Expected: exit 0.

- [ ] **Step 2: Confirm the shuffle option is available**

Run:

```bash
cargo +nightly-2026-07-27 test -p hoimin-core --test contracts -- \
  -Z unstable-options --help
```

Expected: output documents `--shuffle` and `--shuffle-seed`.

- [ ] **Step 3: Run the exact CI shuffle command**

Run:

```bash
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
```

Expected: all workspace tests pass and output includes generated shuffle seeds.

### Task 5: Verify and review

**Files:**
- Inspect: `.github/workflows/ci.yml`
- Inspect: `docs/development.md`
- Inspect: `tests/test_ci_workflow.py`

**Interfaces:**
- Consumes: completed Tasks 1-4
- Produces: merge-ready Issue #44 branch

- [ ] **Step 1: Run stable quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: all commands pass.

- [ ] **Step 2: Confirm release workflows are untouched**

Run:

```bash
git diff main...HEAD -- .github/workflows/release.yml
```

Expected: no output.

- [ ] **Step 3: Inspect final branch state**

Run:

```bash
git diff main...HEAD --check
git status -sb
```

Expected: no whitespace errors and no uncommitted changes.

- [ ] **Step 4: Request independent code review**

Use `superpowers:requesting-code-review` against Issue #44. Resolve every verified Critical
or Important finding before opening a PR.
