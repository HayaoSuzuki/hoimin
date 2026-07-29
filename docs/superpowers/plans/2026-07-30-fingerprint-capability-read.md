# Fingerprint Capability Read Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent fingerprint inputs from escaping the root through linked parents or pathname races.

**Architecture:** Reuse the workspace module's tested capability-relative root reader through a narrow crate-private function. Route fingerprint hashing through that reader while preserving existing discovery, normalization, ordering, and error variants.

**Tech Stack:** Rust, cap-primitives, platform-specific root handles, Cargo test

## Global Constraints

- Reject symlink/reparse traversal in every parent and the final component.
- Hash bytes from the validated file handle.
- Preserve exact-file and glob error prefixes and output ordering.
- Avoid duplicating the workspace reader's Unix and Windows implementations.

---

### Task 1: Reproduce linked-parent escape

**Files:**
- Modify: `crates/hoimin-cli/tests/fingerprint_inputs.rs`

**Interfaces:**
- Consumes: `fingerprint_inputs::resolve`
- Produces: a Unix regression test for an exact file beneath a linked parent

- [x] **Step 1: Add the failing linked-parent test**

Create separate root and outside temporary directories, symlink
`root/linked` to the outside directory, and request `linked/secret.txt`.
Assert the exact unsupported-file error prefix.

- [x] **Step 2: Verify RED**

Run: `cargo test -p hoimin-cli --test fingerprint_inputs exact_file_rejects_symlinked_parent`

Expected: FAIL because the current metadata check and read follow the parent
symlink and return a hash.

### Task 2: Route hashing through capability-relative reads

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/fingerprint_inputs.rs`
- Test: `crates/hoimin-cli/tests/fingerprint_inputs.rs`

**Interfaces:**
- Produces: `workspace::read_root_relative(root, path) -> Result<Vec<u8>, WorkspaceError>`
- Consumes: normalized selected fingerprint paths

- [x] **Step 1: Expose a narrow shared reader**

Open a `WorkerRoot` for the supplied root and call its existing `read` method.
Do not expose the handle type publicly.

- [x] **Step 2: Use the reader for fingerprint hashing**

Replace `std::fs::read(root.join(path))` with the shared reader and preserve
the existing exact/glob error mapping.

- [x] **Step 3: Verify GREEN and existing behavior**

Run:

```console
cargo test -p hoimin-cli --test fingerprint_inputs
cargo test -p hoimin-cli workspace::root
```

Expected: all tests pass.

- [x] **Step 4: Run full verification**

Run:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all commands exit zero.

### Task 3: Remove ambient exact-file preflight

**Files:**
- Modify: `crates/hoimin-cli/src/fingerprint_inputs.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/manifest.rs`
- Test: `crates/hoimin-cli/tests/fingerprint_inputs.rs`

**Interfaces:**
- Consumes: `WorkspaceError::io_kind()`
- Produces: secure exact-file `not_found` classification without ambient path probing

- [x] **Step 1: Reproduce a missing file beneath a linked parent**

Assert the request is rejected as unsupported rather than probing the outside
directory and reporting `not_found`.

- [x] **Step 2: Preserve I/O kinds in workspace errors**

Record `std::io::ErrorKind` in `WorkspaceError::Io` and expose it
crate-privately for fingerprint error mapping. Wrap the one non-I/O manifest
metadata error as `ErrorKind::Other`.

- [x] **Step 3: Remove `symlink_metadata` exact preflight**

Normalize the exact path without touching the filesystem, then map a
capability-reader `NotFound` to the existing exact not-found variant.

- [x] **Step 4: Verify both linked-parent cases and exact missing behavior**

Run: `cargo test -p hoimin-cli --test fingerprint_inputs`

Expected: all tests pass.
