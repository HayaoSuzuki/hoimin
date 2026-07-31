# Root Source Selects the Whole Project Implementation Plan

> **For Codex:** Use Superpowers subagent-driven development and test-driven development to execute this plan.

**Goal:** Define a source selector equal to the configured root as “all paths below root,” eliminating vacuous successful runs and false `FileOutsideSource` errors.

**Architecture:** Keep `normalize_logical` returning the empty logical path for root-equal selectors. Make that empty path the explicit root-prefix sentinel in `is_within`: every normalized discovered path is within it. This single containment rule then applies consistently to whole-source selection, file/line validation, symbol resolution, and `--changed` intersection.

**Tech Stack:** Rust, `camino`, Tokio integration tests, Cargo.

---

## Task 1: Define and verify the empty logical root prefix

**Files:**

- Modify: `crates/hoimin-core/src/target.rs`
- Modify: `crates/hoimin-core/tests/target_policy.rs`
- Modify: `crates/hoimin-cli/tests/target_handler.rs`
- Modify: `README.md`

### Step 1: Reproduce whole-project source failure in core

Add a table-driven core test covering root-equivalent source spellings:

- `""`
- `.`
- `./`
- `pkg/..`
- an absolute path exactly equal to `selection.root`

Use discovered root-level and nested Python files plus a regular non-Python file. For every spelling, assert that both Python files are returned in normalized order and the regular file is excluded.

Run:

```console
cargo test -p hoimin-core --test target_policy \
  root_equal_source_selects_every_python_file -- --exact
```

Expected RED: each root-equivalent source currently selects no targets.

### Step 2: Reproduce explicit selector validation failure

Add a core test with `sources: ["."]` and root-relative `--file` / `--line`
paths. Assert resolution succeeds rather than returning `FileOutsideSource`.
The source selector's whole-file union may dominate the narrower line selector;
the regression contract is successful containment.

Run the focused test and record RED.

### Step 3: Reproduce `--changed --source .`

Add a real-repository `target_handler` test:

1. Commit a root-level Python file and a nested Python file.
2. Modify a known line in each.
3. Resolve with `changed: true` and `sources: ["."]`.
4. Assert both files and their changed ranges are returned.

Run:

```console
cargo test -p hoimin-cli --test target_handler \
  changed_root_source_selects_changed_python_lines -- --exact
```

Expected RED: the explicit source set is empty and the handler returns no targets.

### Step 4: Make the empty normalized directory contain every path

At the start of `is_within`, add the platform-independent root sentinel:

```rust
if directory.as_str().is_empty() {
    return true;
}
```

Keep all existing Windows case folding and ordinary non-empty directory checks unchanged. Do not special-case callers independently.

### Step 5: Document the public contract

In the README selector description, state that `--source .` (or a source path
equal to `--root`) selects every discovered Python file below the root. Keep
the union and `--changed` intersection explanations intact.

### Step 6: Prove RED becomes GREEN

Run all new focused tests, then:

```console
cargo test -p hoimin-core --test target_policy
cargo test -p hoimin-cli --test target_handler
```

Expected: all tests pass.

### Step 7: Full verification

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
git diff --check
```

Expected: every command succeeds.

### Step 8: Commit

Stage only the containment fix, tests, README, and this plan:

```console
git add crates/hoimin-core/src/target.rs crates/hoimin-core/tests/target_policy.rs crates/hoimin-cli/tests/target_handler.rs README.md docs/superpowers/plans/2026-07-31-root-source-selects-project.md
git commit -m "fix: treat root source as whole project"
```

## Acceptance Checklist

- Relative and absolute root-equal source spellings select all discovered Python files.
- Root-equal sources accept root-relative file and line selectors.
- `--changed --source .` returns changed Python ranges rather than an empty set.
- Non-empty source roots retain their existing containment semantics.
- README documents the behavior.
- Full workspace and contracts test suites pass.
