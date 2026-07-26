# Capability-Relative Workspace Operations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate check-then-use workspace path races by performing every worker-tree content and permission operation relative to stable, no-follow directory capabilities on Linux, macOS, and Windows.

**Architecture:** Add a private `WorkerRoot` filesystem boundary backed by `cap-primitives`, with component-by-component `open_dir_nofollow` traversal and one-component final operations. `WorkerWorkspace`, mutation, and reset keep logical `Utf8Path` values for errors but never use joined ambient paths as authority; unsupported capability initialization fails closed.

**Tech Stack:** Rust 1.85, `cap-primitives` 4.x, `cap-fs-ext` 4.x, `camino`, existing `hoimin-cli` workspace tests, `cargo-mutants` 27.1.0, GitHub Actions Linux/macOS/Windows jobs

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/issue-28-capability-relative-workspace` on `refactor/issue-28-capability-relative-workspace`.
- Preserve valid workspace behavior and public schemas on Linux, macOS, and Windows.
- Reject absolute, empty, `.`, `..`, symlink, junction, and reparse-point traversal; never add an ambient-path fallback.
- Keep worker paths for diagnostics and process `cwd` only, never as authority for workspace content or permission changes.
- Use deterministic synchronization for race tests; do not use probabilistic retry loops.
- Keep test hooks out of normal cross-platform builds.
- Run focused mutation testing only for path rejection, capability traversal, mutation write authorization, and reset decisions.
- Commit each completed task before beginning the next task.

---

### Task 1: Stable no-follow worker-root capability

**Files:**
- Modify: `crates/hoimin-cli/Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Test: `crates/hoimin-cli/src/workspace/root.rs`

**Interfaces:**
- Consumes: a materialized worker-root `Utf8PathBuf`.
- Produces: `WorkerRoot::open(path) -> Result<Self, WorkspaceError>`, `WorkerRoot::path() -> &Utf8Path`, `WorkerRoot::open_parent(path, create) -> Result<(File, OsString), WorkspaceError>`, and no-follow metadata/open helpers used by later tasks.

- [x] **Step 1: Add failing path and no-follow traversal unit tests**

Add tests in `root.rs` that open a temporary root, assert normal nested
components resolve, and assert `""`, `"."`, `".."`, `"../outside"`,
absolute paths, and a nested directory symlink/junction return
`WorkspaceError::InvalidPath`.

```rust
#[test]
fn rejects_non_normal_and_linked_parent_components() {
    let fixture = RootFixture::new();
    fixture.link_dir("outside", "linked").unwrap();
    let root = WorkerRoot::open(fixture.worker_path()).unwrap();

    for path in ["", ".", "..", "../outside"] {
        assert!(matches!(
            root.open_parent(Utf8Path::new(path), false),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }
    assert!(matches!(
        root.open_parent(Utf8Path::new("linked/secret.txt"), false),
        Err(WorkspaceError::InvalidPath { .. })
    ));
}
```

- [x] **Step 2: Run the focused unit tests and verify RED**

Run:

```bash
cargo test -p hoimin-cli workspace::root::tests -- --nocapture
```

Expected: compilation fails because `WorkerRoot` and `open_parent` do not yet
exist.

- [x] **Step 3: Add the capability dependency and root abstraction**

Add `cap-primitives = "4.0"` and `cap-fs-ext = "4.0"` to `hoimin-cli`.
Implement `WorkerRoot` with an
ambient root open performed once and a traversal that validates
`hoimin_core::normalized_relative_path`, splits the final component, and calls
`cap_primitives::fs::open_dir_nofollow` once per parent component.

```rust
#[derive(Debug)]
pub(crate) struct WorkerRoot {
    path: Utf8PathBuf,
    handle: std::fs::File,
}

impl WorkerRoot {
    pub(crate) fn open(path: Utf8PathBuf) -> Result<Self, WorkspaceError> {
        let handle = cap_primitives::fs::open_ambient_dir(
            path.as_std_path(),
            cap_primitives::ambient_authority(),
        )
        .map_err(|error| WorkspaceError::io("open worker root", &path, error))?;
        Ok(Self { path, handle })
    }

    fn components(path: &Utf8Path) -> Result<Vec<&str>, WorkspaceError> {
        if !hoimin_core::normalized_relative_path(path.as_str()) {
            return Err(WorkspaceError::InvalidPath { path: path.to_owned() });
        }
        Ok(path.components().map(|part| part.as_str()).collect())
    }
}
```

Map a no-follow failure caused by a link/reparse point to `InvalidPath`; map
ordinary I/O failures through `WorkspaceError::io` with the logical path.
Use `File::try_clone` for the root parent and retain every newly opened parent
handle until the next component has been opened.

- [x] **Step 4: Make worker construction fail closed**

Replace the `root: Utf8PathBuf` field with `root: WorkerRoot`, change
`from_materialized` from `const fn -> Self` to `fn -> Result<Self,
WorkspaceError>`, initialize the capability there, and update
`copy.rs::create_worker` to return that result directly.

```rust
pub(crate) fn from_materialized(/* existing arguments */)
    -> Result<Self, WorkspaceError>
{
    let root = WorkerRoot::open(root)?;
    Ok(Self { root, /* existing fields */ })
}

pub fn root(&self) -> &Utf8Path {
    self.root.path()
}
```

- [x] **Step 5: Run tests and commit**

Run:

```bash
cargo test -p hoimin-cli workspace::root::tests -- --nocapture
cargo test -p hoimin-cli --test workspace_handler --test workspace_recovery
```

Expected: root unit tests pass; both integration binaries retain all baseline
passes.

Commit:

```bash
git add Cargo.lock crates/hoimin-cli/Cargo.toml crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/src/workspace/mod.rs crates/hoimin-cli/src/workspace/copy.rs
git commit -m "refactor: establish worker root capabilities"
```

### Task 2: Capability-relative public file operations

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Test: `crates/hoimin-cli/tests/workspace_handler.rs`
- Test: `crates/hoimin-cli/tests/workspace_recovery.rs`

**Interfaces:**
- Consumes: `WorkerRoot` and normalized logical paths from Task 1.
- Produces: `WorkerRoot::{read, write, remove_file, try_exists}` with
  `Result<_, WorkspaceError>` signatures; `WorkerWorkspace` delegates its
  public operations to these methods.

- [x] **Step 1: Add characterization and external-sentinel tests**

Extend integration tests to cover nested creation, read-only replacement,
missing existence, removal, and rejection of final-component links. Assert an
outside sentinel is unchanged.

```rust
#[test]
fn file_apis_reject_final_link_without_touching_outside() {
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("sentinel.txt"), b"outside").unwrap();
    let mut worker = create_worker_with_link(
        "sentinel-link",
        outside.path().join("sentinel.txt"),
    );

    assert!(matches!(
        worker.write("sentinel-link", b"changed"),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(matches!(
        worker.remove("sentinel-link"),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(
        fs::read(outside.path().join("sentinel.txt")).unwrap(),
        b"outside"
    );
}
```

- [x] **Step 2: Run the new tests and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test workspace_handler file_apis_ -- --nocapture
cargo test -p hoimin-cli --test workspace_recovery root_relative_ -- --nocapture
```

Expected: final-link write/remove behavior fails under the pathname
implementation.

- [x] **Step 3: Implement handle-based read/write/remove/exists**

In `WorkerRoot`, open the stable parent first. For read, open the final name
with `FollowSymlinks::No`, verify regular-file metadata from the opened handle,
then `read_to_end`. For write, create missing parents component by component,
open the final file no-follow with read/write/create/truncate options, make
permissions writable through that file handle, then `write_all`. Remove and
exists inspect the one-component final entry without following it and call
capability-relative removal.

```rust
pub(crate) fn write(
    &self,
    path: &Utf8Path,
    contents: &[u8],
) -> Result<(), WorkspaceError> {
    let (parent, name) = self.open_parent(path, true)?;
    let mut options = cap_primitives::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    use cap_fs_ext::OpenOptionsFollowExt;
    options.follow(FollowSymlinks::No);
    let mut file = cap_primitives::fs::open(&parent, Path::new(&name), &options)
        .map_err(|error| self.map_entry_error("write worker file", path, error))?;
    make_file_writable(&file, path)?;
    file.write_all(contents)
        .map_err(|error| WorkspaceError::io("write worker file", path, error))
}
```

Remove `resolve_worker_path`. Move pathname `make_writable` logic to
file-handle and directory-handle helpers in `root.rs`. Keep ambient
`make_tree_writable` only for final temporary-directory cleanup, which is
outside the worker content authorization boundary and owns the `TempDir`.

- [x] **Step 4: Delegate `WorkerWorkspace` APIs and run tests**

```rust
pub fn read(&self, path: impl AsRef<Utf8Path>) -> Result<Vec<u8>, WorkspaceError> {
    self.root.read(path.as_ref())
}

pub fn write(&mut self, path: impl AsRef<Utf8Path>, contents: &[u8])
    -> Result<(), WorkspaceError>
{
    self.root.write(path.as_ref(), contents)
}
```

Run:

```bash
cargo test -p hoimin-cli --test workspace_handler --test workspace_recovery
```

Expected: all integration tests pass, including new final-link and sentinel
tests.

- [x] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/src/workspace/mod.rs crates/hoimin-cli/tests/workspace_handler.rs crates/hoimin-cli/tests/workspace_recovery.rs
git commit -m "refactor: route workspace file APIs through capabilities"
```

### Task 3: Bind mutation verification and writes to one opened object

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/mutation.rs`
- Test: `crates/hoimin-cli/tests/workspace_handler.rs`

**Interfaces:**
- Consumes: `WorkerRoot::open_regular_file(path, options)` from Tasks 1–2.
- Produces: mutation reads, validates, makes writable, truncates, and writes
  through one `std::fs::File`; no pathname is resolved after validation.

- [x] **Step 1: Add a mutation final-link regression test**

Create a valid candidate for `pkg/a.py`, replace the worker target with a link
to an outside file containing matching original bytes, invoke
`apply_mutation`, and assert `InvalidPath` plus an unchanged outside file.

```rust
#[test]
fn mutation_rejects_linked_target_with_matching_bytes() {
    let (mut worker, candidate) = worker_and_candidate();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), b"original\n").unwrap();
    replace_with_file_link(worker.root().join("pkg/a.py"), outside.path()).unwrap();

    assert!(matches!(
        worker.apply_mutation(&candidate),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(fs::read(outside.path()).unwrap(), b"original\n");
}
```

- [x] **Step 2: Run the mutation test and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test workspace_handler mutation_rejects_linked_target_with_matching_bytes -- --exact
```

Expected: the current read/check/write sequence does not satisfy the new
handle-bound contract.

- [x] **Step 3: Refactor mutation to retain the opened file**

Open the target once without following links and with read/write access. Read
bytes from the handle, perform the existing manifest hash, candidate hash,
span, and original-byte checks, build `mutated`, make the opened file writable,
seek to zero, truncate, write all bytes, and flush.

```rust
let mut file = self.root.open_mutation_file(&candidate.path)?;
let mut bytes = Vec::new();
file.read_to_end(&mut bytes)
    .map_err(|error| WorkspaceError::io("read mutation target", &candidate.path, error))?;
// Existing hash/span/original validation remains before any mutation.
make_file_writable(&file, &candidate.path)?;
file.seek(SeekFrom::Start(0))
    .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;
file.set_len(0)
    .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;
file.write_all(&mutated)
    .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;
```

- [x] **Step 4: Run mutation and workspace tests**

Run:

```bash
cargo test -p hoimin-cli --test workspace_handler mutation_ -- --nocapture
cargo test -p hoimin-cli --test workspace_recovery
```

Expected: mutation regressions and all recovery tests pass.

- [x] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/src/workspace/mutation.rs crates/hoimin-cli/tests/workspace_handler.rs
git commit -m "fix: bind mutation writes to verified file handles"
```

### Task 4: Capability-relative reset enumeration and restoration

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`
- Test: `crates/hoimin-cli/tests/workspace_recovery.rs`

**Interfaces:**
- Consumes: stable directory capabilities and file helpers from Tasks 1–3.
- Produces: `WorkerRoot::entries() -> Result<Vec<WorkerEntry>,
  WorkspaceError>`, recursive no-follow removal, snapshot comparison, content
  restoration, and permission restoration without ambient joins.

- [x] **Step 1: Add reset characterization and link-safety tests**

Cover extra nested files/directories, files replaced by directories, read-only
files, mode restoration on Unix, and an unexpected directory link/junction to
an outside sentinel.

```rust
#[test]
fn reset_removes_worker_link_without_traversing_outside() {
    let mut worker = create_worker();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("sentinel"), b"outside").unwrap();
    create_dir_link(outside.path(), worker.root().join("unexpected")).unwrap();

    worker.reset().unwrap();

    assert_eq!(fs::read(outside.path().join("sentinel")).unwrap(), b"outside");
    assert!(!worker.root().join("unexpected").exists());
    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
}
```

- [x] **Step 2: Run reset tests and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test workspace_recovery reset_ -- --nocapture
```

Expected: at least the link/junction safety or new characterization assertions
fail with the ambient `WalkBuilder` and pathname restore implementation.

- [x] **Step 3: Implement capability-relative enumeration and removal**

Define:

```rust
pub(crate) struct WorkerEntry {
    pub(crate) path: Utf8PathBuf,
    pub(crate) kind: WorkerEntryKind,
}

pub(crate) enum WorkerEntryKind {
    File,
    Directory,
    LinkOrReparse,
}
```

Enumerate each open directory with `cap_primitives::fs::read_base_dir`, classify
entries without following, recurse only after `open_dir_nofollow`, and sort by
component depth plus path. Implement recursive removal by holding the opened
parent, making actual files/directories writable through handles, and removing
one final component. Links/reparse points are removed as entries and never
opened as directories.

- [x] **Step 4: Rewrite reset and snapshot verification**

Replace `collect_worker_entries`, `remove_any`,
`remove_directory_if_empty`, and path-based `make_writable` with `WorkerRoot`
methods. Restore each `SnapshotFile` through capability-relative write and
handle-based `set_permissions`. Verify bytes, kind, and permission fingerprint
through opened handles.

```rust
for (path, snapshot) in &self.snapshot {
    if self.root.snapshot_matches(path, snapshot)? {
        continue;
    }
    self.root.remove_any_if_exists(path)?;
    self.root.restore(path, &snapshot.bytes, snapshot.permissions.clone())?;
}
```

- [x] **Step 5: Run reset tests and commit**

Run:

```bash
cargo test -p hoimin-cli --test workspace_recovery
cargo test -p hoimin-cli --test workspace_handler
```

Expected: all workspace and recovery tests pass.

Commit:

```bash
git add crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/src/workspace/reset.rs crates/hoimin-cli/tests/workspace_recovery.rs
git commit -m "refactor: restore workers through directory capabilities"
```

### Task 5: Deterministic parent-replacement race coverage

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/mutation.rs`
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`
- Test: `crates/hoimin-cli/src/workspace/root.rs`
- Test: `crates/hoimin-cli/src/workspace/mutation.rs`
- Test: `crates/hoimin-cli/src/workspace/reset.rs`

**Interfaces:**
- Consumes: all capability-relative operations from Tasks 1–4.
- Produces: a `cfg(test)` synchronization seam that pauses after
  parent acquisition, plus deterministic read/write/remove/mutation/reset race
  regressions on all supported OSes.

- [x] **Step 1: Add a test-only race hook**

Keep the hook private and compiled only for unit tests. It must pause after a
parent capability is opened but before the final entry operation.

```rust
#[cfg(test)]
trait WorkspaceRaceHook: Send + Sync {
    fn parent_opened(&self, operation: &'static str, path: &Utf8Path);
}
```

The production `WorkerRoot` layout has no hook field and exports no test
symbol. Place the race tests in the three workspace source modules so they can
install the private hook without enabling it in integration or all-feature
builds.

- [x] **Step 2: Add deterministic race tests**

For each operation, use two barriers: the operation signals that its parent is
open; the test renames the real parent and installs a symlink/junction or
alternate directory at the old pathname; the test releases the operation.
Assert it changes only the renamed, originally opened tree or returns an
error. Verify outside sentinel content, existence, and permissions.

```rust
let thread = spawn_paused_write(worker, hook.clone(), "swap/target", b"worker");
hook.wait_until_parent_opened();
fs::rename(worker_root.join("swap"), worker_root.join("held")).unwrap();
create_dir_link(outside.path(), worker_root.join("swap")).unwrap();
hook.resume();
let result = thread.join().unwrap();

assert!(result.is_ok() || matches!(result, Err(WorkspaceError::Io { .. })));
assert_eq!(fs::read(outside.path().join("target")).unwrap(), b"outside");
```

Cover `read`, `write`, `remove`, mutation, and reset. On Windows, use a
directory junction when symlink privileges are unavailable; if neither can be
created, exercise and assert the explicit fail-closed setup path instead of
silently returning from the test.

- [x] **Step 3: Run race tests and normal-build checks**

Run:

```bash
cargo test -p hoimin-cli parent_replacement_ -- --nocapture
cargo check -p hoimin-cli --all-targets
```

Expected: all race tests pass and the normal library/binary compile without
test-hook linkage.

- [x] **Step 4: Run the platform-relevant test suite and commit**

Run:

```bash
cargo test -p hoimin-cli
```

Expected: all tests pass.

Commit:

```bash
git add crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/src/workspace/mutation.rs crates/hoimin-cli/src/workspace/reset.rs
git commit -m "test: cover workspace parent replacement races"
```

### Task 6: Focused mutation and full verification

**Files:**
- Modify if survivors require tests: `crates/hoimin-cli/src/workspace/root.rs`
- Modify if survivors require tests: `crates/hoimin-cli/tests/workspace_handler.rs`
- Modify if survivors require tests: `crates/hoimin-cli/tests/workspace_recovery.rs`
- Modify if implementation details changed: `docs/superpowers/specs/2026-07-25-capability-relative-workspace-design.md`
- Modify: `docs/superpowers/plans/2026-07-25-capability-relative-workspace.md`

**Interfaces:**
- Consumes: completed Issue #28 implementation and tests.
- Produces: caught viable mutants in the security-critical target set, clean
  formatting/lints/tests, and recorded verification evidence.

- [x] **Step 1: Run focused mutation testing**

Target only the new root boundary plus mutation/reset decision functions:

```bash
cargo mutants -p hoimin-cli \
  --file crates/hoimin-cli/src/workspace/root.rs \
  --file crates/hoimin-cli/src/workspace/mutation.rs \
  --file crates/hoimin-cli/src/workspace/reset.rs \
  --timeout 120 \
  --jobs 2
```

Expected: every viable retained mutant is caught. Record unviable and timeout
mutants separately; do not broaden the target to unrelated CLI modules.

- [x] **Step 2: Close meaningful survivors with RED/GREEN tests**

For each meaningful survivor, add the smallest behavior assertion that fails
when the predicate is inverted or removed, verify the individual mutant is
missed before the test and caught afterward, then rerun the focused command.
Do not change production behavior merely to satisfy an unviable mutant.

Focused mutation retained the same 62-candidate set and test argv across
comparison runs. The initial run produced 36 caught, 20 missed, 2 unviable,
and 4 timeout outcomes. The final run produced 55 caught, 5 missed,
2 unviable, and 0 timeout outcomes. The remaining misses are one
filesystem-identity combination without a meaningful reachable scenario, one
non-`NotFound` final-stat injection that would require a mutant-only production
seam, and three Windows-only snapshot predicates not executable on the macOS
mutation host. Windows CI exercises the corresponding runtime behavior.

- [x] **Step 3: Run complete local verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace
git diff --check
```

Expected: every command exits zero.

- [x] **Step 4: Verify documentation consistency**

Compare the final implementation against every design section. Update the
design only for implementation facts that changed without weakening the
approved security contract. Check off completed plan steps and record the
focused mutation outcome without generated `mutants.out` artifacts.

- [x] **Step 5: Commit final verification changes**

```bash
git add crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/tests/workspace_handler.rs crates/hoimin-cli/tests/workspace_recovery.rs docs/superpowers/specs/2026-07-25-capability-relative-workspace-design.md docs/superpowers/plans/2026-07-25-capability-relative-workspace.md
git commit -m "test: strengthen capability workspace guarantees"
```

If no files changed after verification, do not create an empty commit.

- [ ] **Step 6: Push and open the Issue #28 PR**

```bash
git push -u origin refactor/issue-28-capability-relative-workspace
gh pr create \
  --base main \
  --head refactor/issue-28-capability-relative-workspace \
  --title "refactor: make workspace operations capability-relative" \
  --body-file /tmp/issue-28-pr-body.md
```

The PR body must include `Closes #28`, summarize the capability boundary and
race tests, list local verification and focused mutation results, and allow
the Linux/macOS/Windows CI jobs to run.
