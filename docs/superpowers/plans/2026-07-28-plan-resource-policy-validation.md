# Plan Resource Policy Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject incompatible macOS resource settings during `hoimin plan` before project work or manifest output.

**Architecture:** Add a side-effect-free resource-policy validator with a platform-independent test seam, then call it at the beginning of plan creation. Preserve runtime backend selection and verify-side validation unchanged.

**Tech Stack:** Rust, Tokio, Clap, thiserror, Cargo integration tests

## Global Constraints

- On macOS, missing `--allow-best-effort-memory` must make `plan` return exit 2.
- Failed planning must write no manifest bytes to stdout.
- The error must name `--allow-best-effort-memory` and explain that max-memory is not enforced.
- Planning must not create, probe, or acquire a runtime backend.
- Linux and Windows plan behavior must remain unchanged.
- Verify must retain its existing host-side resource validation.
- Manifest schema version 2 must remain unchanged.

---

### Task 1: Add a pure planning resource-policy validator

**Files:**
- Modify: `crates/hoimin-cli/src/resource/mod.rs`
- Modify: `crates/hoimin-cli/src/resource/portable.rs`

**Interfaces:**
- Consumes: `allow_best_effort_memory: bool`
- Produces: `pub(crate) fn validate_plan_resource_policy(bool) -> Result<(), ResourceError>`
- Internal seam: `fn validate_plan_resource_policy_for(bool, Option<&str>) -> Result<(), ResourceError>`

- [ ] **Step 1: Add failing unit tests for the pure policy seam**

Add tests in `crates/hoimin-cli/src/resource/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{ResourceError, validate_plan_resource_policy_for};

    const DIAGNOSTIC: &str =
        "macOS uses process groups and RLIMIT_CPU; max-memory is not enforced";

    #[test]
    fn plan_policy_rejects_unapproved_best_effort_memory() {
        let error = validate_plan_resource_policy_for(false, Some(DIAGNOSTIC)).unwrap_err();

        assert!(matches!(
            &error,
            ResourceError::BestEffortNotAllowed(_)
        ));
        assert_eq!(
            error.to_string(),
            "portable resource limits require --allow-best-effort-memory: \
             macOS uses process groups and RLIMIT_CPU; max-memory is not enforced"
        );
    }

    #[test]
    fn plan_policy_accepts_explicit_best_effort_memory() {
        assert!(validate_plan_resource_policy_for(true, Some(DIAGNOSTIC)).is_ok());
    }

    #[test]
    fn plan_policy_leaves_hard_platforms_unchanged() {
        assert!(validate_plan_resource_policy_for(false, None).is_ok());
    }
}
```

- [ ] **Step 2: Run the new tests and confirm the missing interface fails**

Run:

```bash
cargo test -p hoimin-cli resource::tests::plan_policy --lib
```

Expected: compilation failure because `validate_plan_resource_policy_for` does not exist.

- [ ] **Step 3: Expose the macOS diagnostic within the resource module**

Change the existing declaration in `portable.rs` without changing its text:

```rust
#[cfg(target_os = "macos")]
pub(super) const MACOS_BEST_EFFORT_DIAGNOSTIC: &str =
    "macOS uses process groups and RLIMIT_CPU; max-memory is not enforced";
```

- [ ] **Step 4: Implement the pure seam and platform wrapper**

Add to `resource/mod.rs`:

```rust
fn validate_plan_resource_policy_for(
    allow_best_effort_memory: bool,
    best_effort_diagnostic: Option<&str>,
) -> Result<(), ResourceError> {
    if !allow_best_effort_memory
        && let Some(diagnostic) = best_effort_diagnostic
    {
        return Err(ResourceError::BestEffortNotAllowed(diagnostic.to_owned()));
    }
    Ok(())
}

pub(crate) fn validate_plan_resource_policy(
    allow_best_effort_memory: bool,
) -> Result<(), ResourceError> {
    #[cfg(target_os = "macos")]
    let diagnostic = Some(portable::MACOS_BEST_EFFORT_DIAGNOSTIC);
    #[cfg(not(target_os = "macos"))]
    let diagnostic = None;

    validate_plan_resource_policy_for(allow_best_effort_memory, diagnostic)
}
```

This wrapper deliberately does not probe Linux cgroups or construct any backend.

- [ ] **Step 5: Run the resource policy unit tests**

Run:

```bash
cargo test -p hoimin-cli resource::tests::plan_policy --lib
```

Expected: 3 passed.

- [ ] **Step 6: Commit the pure policy validator**

```bash
git add crates/hoimin-cli/src/resource/mod.rs crates/hoimin-cli/src/resource/portable.rs
git commit -m "feat: validate planning resource policy"
```

### Task 2: Reject incompatible plans before project work

**Files:**
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Consumes: `resource::validate_plan_resource_policy(config.allow_best_effort_memory)`
- Produces: `PlanError::ResourcePolicy(ResourceError)` displayed without losing the actionable resource message

- [ ] **Step 1: Add a macOS CLI regression test**

Add a `#[cfg(target_os = "macos")]` Tokio test to `crates/hoimin-cli/tests/plan.rs`.
Construct plan arguments without `--allow-best-effort-memory`, use the existing marker-file
test command, and invoke `hoimin_cli::run_with_io`:

```rust
#[cfg(target_os = "macos")]
#[tokio::test]
async fn plan_rejects_unapproved_best_effort_memory_before_project_work() {
    let project = Project::new();
    let marker = project.path.join("test-command-ran");
    let args = plan_args_without_best_effort(&project, &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(exit, 2);
    assert!(stdout.is_empty(), "failed plan emitted stdout");
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains("--allow-best-effort-memory"));
    assert!(stderr.contains("max-memory is not enforced"));
    assert!(!marker.exists(), "test command ran during plan validation");
}
```

Add a focused helper beside `plan_args` that supplies the same root, source, and marker
command but deliberately omits the opt-in flag.

- [ ] **Step 2: Run the regression test and verify current behavior fails**

Run on macOS:

```bash
cargo test -p hoimin-cli --test plan plan_rejects_unapproved_best_effort_memory_before_project_work
```

Expected: FAIL because current planning succeeds and emits a manifest.

- [ ] **Step 3: Add a transparent resource-policy plan error**

Import `crate::resource::{self, ResourceError}` in `plan.rs` and add:

```rust
#[error(transparent)]
ResourcePolicy(#[from] ResourceError),
```

This keeps the user-facing diagnostic identical to the resource error rather than adding a
second CLI-specific prefix.

- [ ] **Step 4: Validate policy at the start of plan creation**

Make the first operation in `plan::create`:

```rust
resource::validate_plan_resource_policy(config.allow_best_effort_memory)?;
```

Place it before `shell::prepare_run_config`, target resolution, source reads, and analyzer
discovery.

- [ ] **Step 5: Run the macOS regression and existing plan suite**

Run:

```bash
cargo test -p hoimin-cli --test plan
```

Expected on macOS: 22 passed. Existing helpers continue to opt in explicitly.

- [ ] **Step 6: Run existing runtime resource tests**

Run:

```bash
cargo test -p hoimin-cli --test process_handler
```

Expected: all tests pass, including verify/runtime portable backend policy tests.

- [ ] **Step 7: Commit the plan integration**

```bash
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/tests/plan.rs
git commit -m "feat: reject incompatible resource plans"
```

### Task 3: Verify cross-platform quality and public behavior

**Files:**
- Inspect: `crates/hoimin-cli/src/resource/mod.rs`
- Inspect: `crates/hoimin-cli/src/resource/portable.rs`
- Inspect: `crates/hoimin-cli/src/plan.rs`
- Inspect: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Consumes: completed Tasks 1 and 2
- Produces: merge-ready #43 implementation

- [ ] **Step 1: Check formatting and static analysis**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: both commands exit 0.

- [ ] **Step 2: Run the full Rust suite**

Run:

```bash
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 3: Run Python contract tests**

Run:

```bash
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: all tests pass.

- [ ] **Step 4: Inspect the final branch**

Run:

```bash
git diff main...HEAD --check
git status -sb
```

Expected: no whitespace errors and no uncommitted changes.

- [ ] **Step 5: Request code review**

Use `superpowers:requesting-code-review` to review the complete diff against Issue #43 and
resolve any verified findings before opening the PR.
