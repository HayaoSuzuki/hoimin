# Hosted CI Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make hosted Linux and Windows CI test the supported portable path and build the wheel reliably.

**Architecture:** E2E test helpers explicitly opt into the portable resource fallback. The delegated self-hosted cgroup test remains the sole hard-backend gate. CI obtains Maturin through `uvx`, keeping project and wheel dependencies empty.

**Tech Stack:** Rust, Cargo, GitHub Actions, uv, uvx, Maturin.

## Global Constraints

- GitHub-hosted Linux and Windows run the portable resource path explicitly.
- The delegated self-hosted cgroup-v2 hard job remains unchanged.
- Maturin must not become a project or wheel runtime dependency.
- Wheel smoke must continue to validate installed-wheel and `uvx` behavior.

---

### Task 1: Make Hosted E2E Explicitly Portable

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: CLI `--allow-best-effort-memory` option.
- Produces: every shared E2E run helper supplies the option before `--`.

- [ ] **Step 1: Add the option in the shared test argument builder**

Insert this value in `run_fixture_options_extra` before the command separator:

```rust
let mut args = vec![
    OsString::from("hoimin"),
    OsString::from("run"),
    OsString::from("--allow-best-effort-memory"),
    // existing root/source/file/format arguments
];
```

Apply the same option to `run_project` and direct `parse_config_from` E2E
builders that bypass the shared helper.

- [ ] **Step 2: Verify the hosted fallback contract**

Run: `cargo test -p hoimin-cli --test run_e2e`

Expected: PASS; E2E JSON reports are emitted even when cgroup controller
delegation is unavailable.

- [ ] **Step 3: Commit the E2E change**

```text
git add crates/hoimin-cli/tests/run_e2e.rs
git commit -m "test: allow portable resources in hosted E2E"
```

### Task 2: Acquire Maturin Through uvx in CI

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `uvx maturin build --release`.
- Produces: platform wheel in `target/wheels` for `tests/wheel_smoke.py`.

- [ ] **Step 1: Replace the wheel build command**

```yaml
- run: uvx maturin build --release
- run: uv run --frozen python tests/wheel_smoke.py
```

- [ ] **Step 2: Verify no dependency declaration changes are needed**

Run: `uv lock --check; rg -n 'maturin' pyproject.toml uv.lock`

Expected: the build-system requirement remains; no runtime project dependency
is added.

- [ ] **Step 3: Build and smoke-test locally**

Run: `uvx maturin build --release; uv run --frozen python tests/wheel_smoke.py`

Expected: both commands exit 0.

- [ ] **Step 4: Commit the CI wheel repair**

```text
git add .github/workflows/ci.yml
git commit -m "ci: build wheel with uvx maturin"
```

### Task 3: Validate the Hosted CI Contract

**Files:**
- Modify: `.github/workflows/ci.yml` only if verification reveals a CI-only command issue.

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces: complete local evidence matching hosted CI jobs.

- [ ] **Step 1: Run the complete relevant checks**

Run: `cargo fmt --all -- --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace; cargo test -p hoimin-cli --features contracts; uvx maturin build --release; uv run --frozen python tests/wheel_smoke.py`

Expected: every command exits 0.

- [ ] **Step 2: Confirm hard-backend isolation remains**

Run: `rg -n 'linux-cgroup-v2-hard|HOIMIN_CGROUP_V2_DELEGATED|allow-best-effort-memory' .github/workflows/ci.yml crates/hoimin-cli/tests/run_e2e.rs`

Expected: hosted E2E opt into best-effort; the delegated self-hosted hard job
still has its existing conditional runner.

- [ ] **Step 3: Commit any verification-driven CI correction**

```text
git add .github/workflows/ci.yml crates/hoimin-cli/tests/run_e2e.rs
git commit -m "ci: verify hosted portable test path"
```
