# Normalized Plan Configuration Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject semantically invalid normalized plan configuration as `plan.manifest.invalid` before target resolution, analyzer discovery, or baseline execution.

**Architecture:** Add side-effect-free validation methods to normalized `PlanConfig` and `RunConfig`, backed by shared private selection, limit, and command validators in `hoimin-core`. Invoke `PlanConfig::validate` at the plan-manifest trust boundary before any project inspection, while preserving valid serialization and existing diagnostic codes.

**Tech Stack:** Rust 2024 (MSRV 1.85), serde/serde_json, thiserror, Tokio integration tests, cargo-mutants, uv-managed Python 3.14 test environment.

## Global Constraints

- Work only in `.worktrees/issue-27-plan-config-validation` on `fix/issue-27-plan-config-validation`.
- Keep `PLAN_SCHEMA_VERSION` at `1`; do not change valid plan JSON.
- Keep `hoimin-core` deterministic and free of filesystem/process dependencies.
- Preserve existing raw CLI validation and its error precedence.
- Invalid normalized configuration must become `plan.manifest.invalid` before target, source, fingerprint, analyzer, or baseline work.
- Do not redesign candidate discovery, fingerprint validation, session persistence, or report schemas.
- Use TDD and commit each independently reviewed task.
- Run focused Rust mutation only for the new validators and the plan verification boundary.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/hoimin-core/src/config.rs` | Shared semantic validators and public normalized validation entry points. |
| `crates/hoimin-core/tests/plan_config.rs` | Persistence-boundary and runtime normalized configuration contracts. |
| `crates/hoimin-cli/src/plan.rs` | Early manifest validation and stable plan error mapping. |
| `crates/hoimin-cli/tests/plan.rs` | Tampered-manifest rejection and no-project-execution evidence. |
| `docs/superpowers/specs/2026-07-25-plan-config-validation-design.md` | Approved design. |

### Task 1: Validate Normalized Core Configuration

**Files:**
- Modify: `crates/hoimin-core/src/config.rs`
- Test: `crates/hoimin-core/tests/plan_config.rs`

**Interfaces:**
- Consumes: existing `PlanConfig`, `RunConfig`, `RunLimits`, `Selection`, `ConfigError`.
- Produces:
  - `PlanConfig::validate(&self) -> Result<(), ConfigError>`
  - `RunConfig::validate(&self) -> Result<(), ConfigError>`
  - shared private selection, limit, and test-command validators.

- [ ] **Step 1: Add failing normalized persistence tests**

Extend `crates/hoimin-core/tests/plan_config.rs` with JSON mutation helpers and
table-driven plan validation:

```rust
use hoimin_core::ConfigError;

fn plan_value() -> serde_json::Value {
    serde_json::to_value(fixture_run_config().into_plan_config()).unwrap()
}

#[test]
fn plan_config_rejects_invalid_normalized_semantics() {
    let cases: &[(&str, fn(&mut serde_json::Value), ConfigError)] = &[
        (
            "missing selector",
            |value| {
                value["selection"]["sources"] = serde_json::json!([]);
                value["selection"]["files"] = serde_json::json!([]);
                value["selection"]["lines"] = serde_json::json!([]);
                value["selection"]["symbols"] = serde_json::json!([]);
                value["selection"]["changed"] = serde_json::json!(false);
            },
            ConfigError::MissingSelector,
        ),
        (
            "diff base without changed",
            |value| {
                value["selection"]["diff_base"] = serde_json::json!("main");
                value["selection"]["changed"] = serde_json::json!(false);
            },
            ConfigError::DiffBaseRequiresChanged,
        ),
        (
            "changed without source",
            |value| {
                value["selection"]["sources"] = serde_json::json!([]);
                value["selection"]["changed"] = serde_json::json!(true);
            },
            ConfigError::ChangedRequiresSource,
        ),
        (
            "symbol without source",
            |value| {
                value["selection"]["sources"] = serde_json::json!([]);
                value["selection"]["files"] = serde_json::json!([]);
                value["selection"]["symbols"] = serde_json::json!(["module::symbol"]);
            },
            ConfigError::SymbolRequiresSource,
        ),
        (
            "empty test argv",
            |value| value["test_argv"] = serde_json::json!([]),
            ConfigError::MissingTestArgv,
        ),
        (
            "zero analyzer duration",
            |value| value["limits"]["analyzer_timeout"] = serde_json::json!({"secs": 0, "nanos": 0}),
            ConfigError::InvalidLimit("analyzer_timeout"),
        ),
    ];

    for (name, mutate, expected) in cases {
        let mut value = plan_value();
        mutate(&mut value);
        let config: PlanConfig = serde_json::from_value(value).unwrap();
        assert_eq!(config.validate(), Err(expected.clone()), "{name}");
    }
}
```

Add separate numeric boundary tests because their values depend on the target
pointer width:

```rust
#[test]
fn normalized_limit_validation_rejects_cross_field_and_arithmetic_violations() {
    let mut value = plan_value();
    value["limits"]["jobs"] = serde_json::json!(2);
    value["limits"]["max_processes"] = serde_json::json!(1);
    let config: PlanConfig = serde_json::from_value(value).unwrap();
    assert_eq!(
        config.validate(),
        Err(ConfigError::JobsExceedsProcesses {
            jobs: 2,
            max_processes: 1,
        })
    );
}

#[test]
fn run_config_validation_keeps_runtime_only_resume_dependency() {
    let mut config = fixture_run_config();
    config.resume = true;
    config.session = None;
    assert_eq!(config.validate(), Err(ConfigError::ResumeRequiresSession));
}
```

Complete the table with cases for `jobs > MAX_JOBS`,
`max_processes > u32::MAX`, zero baseline/mutant/total durations, and
baseline-timeout `checked_mul(2).checked_add(1s)` overflow. Use serde JSON
mutation to construct values that public constructors intentionally reject.

- [ ] **Step 2: Run the focused tests to verify RED**

Run:

```bash
cargo test -p hoimin-core --test plan_config -- --nocapture
```

Expected: compilation fails because `PlanConfig::validate` and
`RunConfig::validate` do not exist.

- [ ] **Step 3: Implement shared normalized validators**

In `crates/hoimin-core/src/config.rs`, add private helpers with the existing
constructor error ordering:

```rust
fn validate_selection(selection: &Selection) -> Result<(), ConfigError> {
    let has_selector = !selection.sources.is_empty()
        || !selection.files.is_empty()
        || !selection.lines.is_empty()
        || !selection.symbols.is_empty()
        || selection.changed;
    if !has_selector {
        return Err(ConfigError::MissingSelector);
    }
    if selection.diff_base.is_some() && !selection.changed {
        return Err(ConfigError::DiffBaseRequiresChanged);
    }
    if selection.changed && selection.sources.is_empty() {
        return Err(ConfigError::ChangedRequiresSource);
    }
    if !selection.symbols.is_empty() && selection.sources.is_empty() {
        return Err(ConfigError::SymbolRequiresSource);
    }
    Ok(())
}

fn validate_test_argv(test_argv: &[CommandArg]) -> Result<(), ConfigError> {
    if test_argv.is_empty() {
        Err(ConfigError::MissingTestArgv)
    } else {
        Ok(())
    }
}

fn validate_limits(limits: &RunLimits) -> Result<(), ConfigError> {
    if limits.jobs.get() > MAX_JOBS {
        return Err(ConfigError::JobsExceedsMaximum {
            jobs: limits.jobs.get(),
            maximum: MAX_JOBS,
        });
    }
    if u32::try_from(limits.max_processes.get()).is_err() {
        return Err(ConfigError::InvalidLimit("max_processes"));
    }
    if limits.jobs.get() > limits.max_processes.get() {
        return Err(ConfigError::JobsExceedsProcesses {
            jobs: limits.jobs.get(),
            max_processes: limits.max_processes.get(),
        });
    }
    for (name, duration) in [
        ("analyzer_timeout", limits.analyzer_timeout.get()),
        ("baseline_timeout", limits.baseline_timeout.get()),
        ("total_timeout", limits.total_timeout.get()),
    ] {
        if duration.is_zero() {
            return Err(ConfigError::InvalidLimit(name));
        }
    }
    if let MutantTimeout::Fixed(duration) = limits.mutant_timeout
        && duration.get().is_zero()
    {
        return Err(ConfigError::InvalidLimit("mutant_timeout"));
    }
    if limits
        .baseline_timeout
        .get()
        .checked_mul(2)
        .and_then(|value| value.checked_add(Duration::from_secs(1)))
        .is_none()
    {
        return Err(ConfigError::InvalidLimit("baseline_timeout"));
    }
    Ok(())
}
```

Add the public methods:

```rust
impl PlanConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_selection(&self.selection)?;
        validate_test_argv(&self.test_argv)?;
        validate_limits(&self.limits)
    }
}

impl RunConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_selection(&self.selection)?;
        if self.resume && self.session.is_none() {
            return Err(ConfigError::ResumeRequiresSession);
        }
        validate_test_argv(&self.test_argv)?;
        validate_limits(&self.limits)
    }
}
```

Refactor `TryFrom<RawRunConfig> for RunConfig` only enough to reuse
`validate_selection` and `validate_test_argv` without changing the existing
precedence: selection errors, resume/session, test argv, operator parsing,
then limit construction. After constructing `RunLimits`, call
`validate_limits` as a defensive shared postcondition.

- [ ] **Step 4: Run core tests and contracts**

Run:

```bash
cargo test -p hoimin-core --test plan_config -- --nocapture
cargo test -p hoimin-core --test target_policy
cargo test -p hoimin-core --features contracts
```

Expected: all PASS. The plan-config target includes every normalized boundary,
and existing raw configuration error behavior remains green.

- [ ] **Step 5: Verify formatting and commit**

Run:

```bash
cargo fmt --all -- --check
cargo clippy -p hoimin-core --all-targets --all-features -- -D warnings
git diff --check
```

Expected: all PASS.

Commit:

```bash
git add crates/hoimin-core/src/config.rs crates/hoimin-core/tests/plan_config.rs
git commit -m "fix: validate normalized run configuration"
```

### Task 2: Reject Invalid Plan Configuration Before Project Work

**Files:**
- Modify: `crates/hoimin-cli/src/plan.rs`
- Test: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**
- Consumes: Task 1 `PlanConfig::validate(&self) -> Result<(), ConfigError>`.
- Produces: early `PlanError::ManifestInvalid` mapping before requested-ID and project validation.

- [ ] **Step 1: Add a failing table-driven tampered-manifest test**

In `crates/hoimin-cli/tests/plan.rs`, add:

```rust
#[tokio::test]
async fn verify_rejects_invalid_normalized_config_before_project_work() {
    let cases: &[(&str, fn(&mut serde_json::Value))] = &[
        ("empty argv", |value| {
            value["normalized_config"]["test_argv"] = serde_json::json!([]);
        }),
        ("jobs exceed processes", |value| {
            value["normalized_config"]["limits"]["jobs"] = serde_json::json!(2);
            value["normalized_config"]["limits"]["max_processes"] = serde_json::json!(1);
        }),
        ("zero total timeout", |value| {
            value["normalized_config"]["limits"]["total_timeout"] =
                serde_json::json!({"secs": 0, "nanos": 0});
        }),
        ("missing selector", |value| {
            let selection = &mut value["normalized_config"]["selection"];
            selection["sources"] = serde_json::json!([]);
            selection["files"] = serde_json::json!([]);
            selection["lines"] = serde_json::json!([]);
            selection["symbols"] = serde_json::json!([]);
            selection["changed"] = serde_json::json!(false);
        }),
    ];

    for (name, mutate) in cases {
        let project = Project::new();
        let (path, manifest, baseline_marker) = write_plan_manifest(&project, &[]).await;
        let requested = vec![manifest.candidates[0].id.clone()];
        let mut value = serde_json::to_value(manifest).unwrap();
        mutate(&mut value);
        write_json(&path, &value);

        std::fs::remove_file(project.path.join("src/calc.py")).unwrap();
        let error = prepare_verify(&path, &requested, OutputFormat::Json)
            .await
            .unwrap_err();

        assert_error_code(error, "plan.manifest.invalid");
        assert!(!baseline_marker.exists(), "{name}");
    }
}
```

Complete the table with `diff_base` without `changed`, `changed` without a
source, symbol without a source, `jobs > MAX_JOBS`,
`max_processes > u32::MAX`, and overflowing baseline timeout.

Deleting `src/calc.py` acts as the project-inspection sentinel: if target/source
work starts before validation, the observed error becomes
`plan.source.changed`. The baseline marker proves no test command ran.

- [ ] **Step 2: Run the new plan test to verify RED**

Run:

```bash
cargo test -p hoimin-cli --test plan \
  verify_rejects_invalid_normalized_config_before_project_work -- --exact --nocapture
```

Expected: FAIL because `prepare_verify` reaches target/source validation and
returns `plan.source.changed` for at least the semantically invalid cases.

- [ ] **Step 3: Add the early manifest validation boundary**

In `prepare_verify`, immediately after `validate_header(&manifest)?;`, add:

```rust
manifest
    .normalized_config
    .validate()
    .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
```

Keep candidate normalization and conversion below this call:

```rust
let candidate_ids = normalize_requested_ids(
    requested_ids,
    manifest.normalized_config.limits.max_mutants.get(),
)?;
let mut config = manifest
    .normalized_config
    .clone()
    .into_run_config(output_config(format));
```

Update `prepare_verify` rustdoc to name invalid normalized configuration as a
manifest error.

- [ ] **Step 4: Run plan and contracts regressions**

Run:

```bash
cargo test -p hoimin-cli --test plan \
  verify_rejects_invalid_normalized_config_before_project_work -- --exact --nocapture
cargo test -p hoimin-cli --test plan
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
```

Expected: all PASS. The focused table reports `plan.manifest.invalid`; all
existing valid and legacy manifest behavior remains unchanged.

- [ ] **Step 5: Verify formatting and commit**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Expected: all PASS.

Commit:

```bash
git add crates/hoimin-cli/src/plan.rs crates/hoimin-cli/tests/plan.rs
git commit -m "fix: reject invalid normalized plan manifests"
```

### Task 3: Focused Mutation, Final Verification, and Pull Request

**Files:**
- Modify only if mutation survivors reveal a missing behavioral assertion:
  - `crates/hoimin-core/tests/plan_config.rs`
  - `crates/hoimin-cli/tests/plan.rs`
- Do not commit cargo-mutants output.

**Interfaces:**
- Consumes: Tasks 1–2 validators and early plan boundary.
- Produces: mutation evidence, final verification evidence, and the Issue #27 PR.

- [ ] **Step 1: Inventory the focused mutation set**

Run with an external output directory:

```bash
test ! -e /private/tmp/issue27-mutants-core
test ! -e /private/tmp/issue27-mutants-plan
cargo mutants \
  --package hoimin-core \
  --file crates/hoimin-core/src/config.rs \
  --re 'validate_selection|validate_test_argv|validate_limits|PlanConfig::validate|RunConfig::validate' \
  --list
```

Record the exact retained candidate count. If the regular expression
prefix-matches another symbol, retain the scope and report it rather than
silently changing the requested target.

- [ ] **Step 2: Execute core validator mutation**

Run:

```bash
cargo mutants \
  --package hoimin-core \
  --jobs 4 \
  --file crates/hoimin-core/src/config.rs \
  --re 'validate_selection|validate_test_argv|validate_limits|PlanConfig::validate|RunConfig::validate' \
  --output /private/tmp/issue27-mutants-core \
  -- --test plan_config --test target_policy
```

Expected: baseline PASS; every viable mutant is caught.

- [ ] **Step 3: Mutate the plan boundary only if cargo-mutants inventories it**

Run:

```bash
cargo mutants \
  --package hoimin-cli \
  --file crates/hoimin-cli/src/plan.rs \
  --re 'prepare_verify' \
  --list
```

If the inventory contains a mutation that removes or bypasses
`normalized_config.validate`, run:

```bash
cargo mutants \
  --package hoimin-cli \
  --jobs 4 \
  --file crates/hoimin-cli/src/plan.rs \
  --re 'prepare_verify' \
  --output /private/tmp/issue27-mutants-plan \
  -- --test plan
```

If no mutation observes the call boundary, report that limitation and rely on
the sentinel integration test; do not broaden mutation to all of `plan.rs`.

- [ ] **Step 4: Improve tests for viable survivors**

For each viable survivor, inspect its original expression, replacement,
symbol, and line. Add the smallest assertion to the applicable test target,
run that normal test target, and rerun the same unchanged mutation scope.
Never alter production code solely to kill a mutant.

Expected final mutation outcome: zero viable missed and zero timeout. Report
caught, missed, unviable, and timeout separately for each inventory.

- [ ] **Step 5: Run fresh final verification**

Run:

```bash
uv sync --frozen
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
git diff --check
git status -sb
```

Expected: all commands PASS and tracked files are clean after commits.
`.serena/` is generated local project metadata and must not be staged.

- [ ] **Step 6: Commit any mutation-driven test improvements**

If Step 4 changed tests:

```bash
git add crates/hoimin-core/tests/plan_config.rs crates/hoimin-cli/tests/plan.rs
git commit -m "test: strengthen normalized plan validation"
```

If no tests changed, do not create an empty commit.

- [ ] **Step 7: Obtain independent review**

Review `main..HEAD` against:

- `docs/superpowers/specs/2026-07-25-plan-config-validation-design.md`;
- this plan;
- Issue #27 acceptance criteria;
- mutation outcome files in the external temporary directory.

Require no unresolved high, medium, or low findings before publishing the PR.

- [ ] **Step 8: Push and create the individual PR**

Run:

```bash
git push -u origin fix/issue-27-plan-config-validation
gh pr create \
  --base main \
  --head fix/issue-27-plan-config-validation \
  --title "fix: validate normalized plan configuration" \
  --body-file /private/tmp/issue27-pr-body.md
```

The PR body must summarize the early validation boundary, list normal and
mutation verification, and include `Closes #27`. Keep the worktree and branch
until PR feedback is resolved and the PR is merged.
