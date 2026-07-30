# Analyzer Capability-Relative Read Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure every analyzer source read stays beneath a stable project-root capability and rejects Unix links and Windows reparse points.

**Architecture:** Expose the existing hardened workspace `WorkerRoot` through a crate-private reusable `RootRelativeReader`. Lazily retain one reader from the first non-cancelled `AnalyzerHandler` request and create one per `discover_targets` invocation, while preserving no-I/O shell construction, analyzer errors, cancellation, ordering, and candidate generation.

**Tech Stack:** Rust 2024, Tokio, camino, cap-primitives, Windows `NtCreateFile`, Cargo tests

## Global Constraints

- Both analyzer read sites must use the same capability-relative no-follow implementation.
- Outside-root bytes must never enter candidate hashes, snippets, plan output, or reports.
- Unix symlinks and Windows reparse points must fail closed as `analyzer.source.read`.
- Candidate ordering, cancellation precedence, spool behavior, and UTF-8 errors must remain unchanged.
- No ambient canonicalization, metadata preflight, or duplicated platform-specific reader is permitted.

---

### Task 1: Reproduce analyzer parent replacement

**Files:**
- Modify: `crates/hoimin-cli/tests/analyzer_handler.rs`

**Interfaces:**
- Consumes: `AnalyzerHandler::new`, `AnalyzerHandler::handle`, and `discover_targets`
- Produces: platform-aware regressions for both analyzer source-read entry points

- [ ] **Step 1: Add platform link helpers**

Add `create_dir_link` implementations using
`std::os::unix::fs::symlink` and
`std::os::windows::fs::symlink_dir`. When Windows symlink privilege is
unavailable, create a directory junction with `mklink /J`; the regression must
not silently skip reparse-point coverage.

- [ ] **Step 2: Add the runtime-handler replacement test**

Create `root/src/calc.py`, construct `AnalyzerHandler`, rename `src`, replace it
with a link to an outside directory containing source with a distinct mutation,
then call `handle`. Assert the result is `analyzer.source.read`; do not accept a
candidate derived from the outside source.

- [ ] **Step 3: Add the in-memory discovery linked-parent test**

Create a project root whose selected `src` parent links to an outside source,
call `discover_targets`, and assert `analyzer.source.read`. Assert the outside
sentinel bytes remain unchanged.

- [ ] **Step 4: Verify RED**

Run:

```console
cargo test -p hoimin-cli --test analyzer_handler analyzer_rejects_replaced_source_parent -- --exact
cargo test -p hoimin-cli --test analyzer_handler discover_targets_rejects_linked_source_parent -- --exact
```

Expected: both fail because ambient `tokio::fs::read(root.join(path))` follows
the linked parent and successfully analyzes outside bytes.

### Task 2: Introduce a reusable root reader

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Test: `crates/hoimin-cli/tests/fingerprint_inputs.rs`
- Test: `crates/hoimin-cli/tests/workspace_recovery.rs`

**Interfaces:**
- Produces: `pub(crate) struct RootRelativeReader`
- Produces: `RootRelativeReader::open(root: Utf8PathBuf) -> Result<Self, WorkspaceError>`
- Produces: `RootRelativeReader::read(path: &Utf8Path) -> Result<Vec<u8>, RootRelativeReadError>`
- Preserves: `read_root_relative(root, path)` for fingerprint callers

- [ ] **Step 1: Wrap the existing `WorkerRoot`**

Store `WorkerRoot` inside `RootRelativeReader`. `open` acquires the stable root
handle once. `read` delegates to `WorkerRoot::read`; on failure it uses
`WorkerRoot::is_missing` to preserve the current `NotFound` classification.

- [ ] **Step 2: Delegate the one-shot helper**

Implement `read_root_relative` as
`RootRelativeReader::open(root)?.read(path)` with the existing
`RootRelativeReadError` mapping so fingerprint behavior does not change.

- [ ] **Step 3: Verify shared-reader regressions**

Run:

```console
cargo test -p hoimin-cli --test fingerprint_inputs
cargo test -p hoimin-cli --test workspace_recovery root_relative_file_apis
```

Expected: all tests pass.

### Task 3: Route both analyzer reads through the capability

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`
- Test: `crates/hoimin-cli/tests/analyzer_handler.rs`

**Interfaces:**
- Consumes: `workspace::RootRelativeReader`
- Preserves: `AnalyzerHandler::new(root) -> Result<Self, std::io::Error>`
- Preserves: `AnalyzerHandler::with_backend(...) -> Result<Self, std::io::Error>`
- Preserves: `discover_targets(...) -> Result<Discovery, EffectFailed>`

- [ ] **Step 1: Lazily store the reader in `AnalyzerHandler`**

Retain `root_path: Utf8PathBuf` and add `root: Option<RootRelativeReader>`.
Constructors perform no project I/O. On the first non-cancelled request, open
the reader and retain it for later targets.

- [ ] **Step 2: Read runtime sources through the stored capability**

Keep the biased cancellation branch first. Replace `tokio::fs::read` with a
lazy async branch that initializes the reader and calls
`reader.read(&request.target.path)`, mapping all reader failures to the
existing `analyzer.source.read` code.

- [ ] **Step 3: Read planned sources through one capability**

Open one `RootRelativeReader` at the start of `discover_targets`, map open/read
failures to `analyzer.source.read`, and reuse it for the complete target loop.

- [ ] **Step 4: Verify GREEN and cancellation**

Run:

```console
cargo test -p hoimin-cli --test analyzer_handler
cargo test -p hoimin-cli analyzer::tests::concrete_handler_cancellation_leaves_store_ready_for_final_spool
```

Expected: all tests pass, including the two RED regressions and the existing
cancellation/store test.

### Task 4: Cross-platform and repository verification

**Files:**
- Verify: `README.md`
- Verify: all changed files

**Interfaces:**
- Produces: reviewable evidence that #85 is fixed without changing CLI usage

- [ ] **Step 1: Check Windows compilation**

Run:

```console
rustup target add x86_64-pc-windows-gnu
cargo check -p hoimin-cli --target x86_64-pc-windows-gnu --all-features
```

Expected: pass when the local linker-independent dependency graph supports the
target. If the target cannot be installed or a dependency requires unavailable
host tooling, record the exact limitation and rely on Windows CI.

- [ ] **Step 2: Run repository gates**

Run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check
```

Expected: all commands exit zero.

- [ ] **Step 3: Confirm README scope**

Verify the diff introduces no new option, output format, or user workflow.
Leave `README.md` unchanged; if review identifies a necessary README-only
clarification, commit it separately with `[skip ci]`.

- [ ] **Step 4: Commit, review, push, and open the PR**

Commit implementation and tests, inspect `git diff origin/main...HEAD`, push
`fix/issue-85-analyzer-root-read`, and create a PR whose body includes
`Closes #85`, RED/GREEN evidence, Windows behavior, and verification commands.
