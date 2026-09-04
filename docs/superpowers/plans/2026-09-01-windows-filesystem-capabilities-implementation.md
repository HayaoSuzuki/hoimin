# Windows filesystem capabilities implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the complete disk-safe focused mutation workflow on Windows through native, identity-checked filesystem capabilities while preserving the POSIX behavior and every bounded-work guarantee.

**Architecture:** Introduce one platform-neutral capability contract, retain the existing POSIX primitives behind a POSIX backend, and implement Windows roots, relative opens, enumeration, capacity, rename, deletion, and security in a native backend. `disk.py`, `lease.py`, and `store.py` keep lifecycle policy but no longer own raw directory descriptors or platform branches.

**Tech Stack:** Python 3.14 standard library (`ctypes`, `msvcrt`, `os`, `unittest`), Win32/NT native APIs, existing Rust `hoimin` plan/verify mutation tooling.

**Spec:** `docs/superpowers/specs/2026-09-01-windows-disk-safety-capability-design.md`

## Global Constraints

- Work only in `.worktrees/disk-safe-mutation` on `feat/disk-safe-mutation`; preserve all user changes and the Rust commits from `2026-09-01-windows-managed-owner-implementation.md`.
- Use `superpowers:test-driven-development` for every behavior change, `superpowers:systematic-debugging` for unexpected failures, `hoimin-mutation-testing` for changed Python behavior, and `superpowers:verification-before-completion` before commit/push or completion claims.
- Python remains `>=3.14,<3.15`; add no runtime dependency and do not use private third-party Windows packages.
- Keep OS selection in `filesystem.default_filesystem_backend()` and native calls in `posix_filesystem.py` or `windows_filesystem.py`. Do not add lifecycle-level `os.name == "nt"` branches.
- Never replace a failed capability operation with `Path.unlink`, `Path.rename`, `shutil.rmtree`, or another destructive path operation.
- A path may select an initial root. Every child open/create, rename, and delete after that point is relative to a live parent or acts on an already-open object.
- Reject reparse points from typed root/directory/file use, wrong object kinds, zero/unsupported identities, filesystem crossings, and enumeration/open identity mismatches. Deletion-only `open_entry` may return a no-follow `REPARSE` capability for the exact object. Only `FileNotFoundError` means absence.
- `SCAN` shares read/write/delete. `PINNED` shares read/write but not delete: absolute roots and typed files omit self-`DELETE`, while relative directories and deletion-only `open_entry` request it. This keeps coordinator/lease files concurrently openable while race-pinning rename/delete sources. `MUTATION` requests delete/write authority and shares read/write/delete for concurrently retained parent/destination capabilities; it is not a rename/delete source proof.
- A capability owns exactly one native resource. Failed close keeps it open; successful close is idempotent; successful `detach_to_fd` transfers sole ownership to the CRT descriptor; `entries_owned` moves a directory resource into an iterator-owned capability and invalidates the source wrapper; finalizers may close only.
- `SecurityDomain.MANAGED` applies the current-token-user descriptor to Hoimin-created protocol objects. `SecurityDomain.CALLER` preserves normal ACL inheritance for output and capacity-only roots.
- Existing owner mismatch is terminal. Never take ownership of a pre-existing root, marker, lease, temporary, coordinator, or protocol directory.
- Metering limits remain: depth 128, 250,000 entries, five seconds, at most 129 active DFS-frame directory capabilities plus the existing one retained capability per configured root, and at most 250,000 hard-link identities.
- Cleanup limits remain: 50,000 entries or five seconds per slice, 60 seconds owner total, 30 seconds janitor total, five seconds selection, depth 4,096, cursor 64 KiB, at most three cleanup directory capabilities, 256 diagnostics, and 4 KiB per diagnostic.
- Marker schemas/caps, coordinator layout (1,025 bytes; two 512-byte slots), output inventory cap (1,000 names), and report/spool byte limits remain unchanged.
- Existing platform-specific primitive tests may skip on the other platform, but every disk-safety behavior must have a backend-neutral test and a native Windows execution path. Do not skip behavior merely because `os.name == "nt"`.
- Use small fixtures. Never create GiB files or hundreds of thousands of real entries; inject counters/iterators at limits.
- Commit only the files named by each task and finish each task with a clean tracked worktree.

## File Structure

- Create `tools/focused_mutation_support/filesystem.py`: public data types, capability ownership state, backend protocol, component validation, and the single backend factory.
- Create `tools/focused_mutation_support/posix_filesystem.py`: current `openat2`/`openat`, `fdopendir`, `fstatat`, `renameat`, `unlinkat`, `fstatvfs`, macOS filesystem identity, and POSIX security-mode behavior.
- Create `tools/focused_mutation_support/windows_filesystem.py`: all new Win32/NT structures, bindings, security descriptors, handles, relative namespace operations, enumeration, capacity, rename, and deletion.
- Create `tests/test_focused_mutation_filesystem.py`: platform-neutral capability-state and backend-conformance tests plus POSIX-only primitive cases.
- Create `tests/test_focused_mutation_windows_filesystem.py`: native Windows sharing, reparse, identity, ACL, enumeration-parser, rename, delete, and CRT-transfer tests.
- Modify `tools/focused_mutation_support/disk.py`: retain disk policy/lifecycle and generic DFS; consume capabilities instead of POSIX descriptors.
- Modify `tools/focused_mutation_support/lease.py`: retain schemas/deadlines/state transitions; consume capabilities for root creation, publication, markers, cleanup, and janitor work.
- Modify `tools/focused_mutation_support/store.py`: retain bounded encoding/report policy; use pinned output and command-root capabilities.
- Modify `tools/focused_mutation.py`: remove the Windows rejection only after native preflight exists and pass one backend through setup.
- Modify `tests/test_focused_mutation_disk.py`, `tests/test_focused_mutation_runner.py`, `tests/test_focused_mutation_reporting.py`, and `tests/test_wheel_smoke.py`: replace descriptor assumptions at integration seams and prove Windows activation.
- Modify `docs/development.md` and `tests/test_focused_mutation_docs.py`: replace the unfinished Windows handoff with the supported native contract.

---

### Task 1: Define the shared capability and ownership contract

**Files:**

- Create: `tools/focused_mutation_support/filesystem.py`
- Create: `tests/test_focused_mutation_filesystem.py`

**Interfaces:**

- Consumes: no production filesystem module.
- Produces: `FileIdentity`, `FilesystemIdentity`, `DirectoryEntry`, `DirectoryIterator`, `EntryKind`, `SharePolicy`, `SecurityDomain`, `FileAccess`, `CreateDisposition`, `FileCapability`, `DirectoryCapability`, `FilesystemBackend`, `validate_component`, and `default_filesystem_backend`. Capabilities expose immutable identity/kind/filesystem/size/time/domain/share/create-result metadata plus the safe `owned_by(owner) -> bool` check; every later task uses these exact names.

- [ ] **Step 1: Write RED tests for identity values and child-name rejection**

Create `tests/test_focused_mutation_filesystem.py` with a table that exercises the platform-neutral validator:

```python
from __future__ import annotations

import os
import unittest

from tools.focused_mutation_support.filesystem import (
    FileIdentity,
    FilesystemIdentity,
    validate_component,
)


class FilesystemValueTests(unittest.TestCase):
    def test_identity_values_are_hashable_and_do_not_alias(self) -> None:
        self.assertNotEqual(FileIdentity(7, 11), FileIdentity(7, 12))
        self.assertEqual(
            {FilesystemIdentity(7, 1, 2), FilesystemIdentity(7, 1, 2)},
            {FilesystemIdentity(7, 1, 2)},
        )

    def test_child_component_rejects_namespace_escape(self) -> None:
        invalid = ["", ".", "..", f"a{os.sep}b", "a\0b"]
        if os.altsep is not None:
            invalid.append(f"a{os.altsep}b")
        for value in invalid:
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_component(value)
        self.assertEqual(validate_component(".hoimin-lease.json"), ".hoimin-lease.json")
        if os.name == "posix":
            self.assertEqual(validate_component("a\\b"), "a\\b")
```

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_filesystem.FilesystemValueTests -v
```

Expected: import failure because `filesystem.py` does not exist.

- [ ] **Step 2: Define the immutable public values and enums**

Implement the approved values exactly, using slots to keep per-entry memory bounded. Start the production module with postponed annotations so the protocol can refer to its own iterator type:

```python
from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import Protocol


@dataclass(frozen=True, slots=True)
class FileIdentity:
    volume: int
    file: int


@dataclass(frozen=True, slots=True)
class FilesystemIdentity:
    volume: int
    discriminator_a: int = 0
    discriminator_b: int = 0


class EntryKind(StrEnum):
    DIRECTORY = "directory"
    REGULAR = "regular"
    REPARSE = "reparse"
    OTHER = "other"


@dataclass(frozen=True, slots=True)
class DirectoryEntry:
    name: str
    kind: EntryKind
    identity: FileIdentity
    filesystem: FilesystemIdentity
    logical_size: int
    modified_ns: int


class SharePolicy(StrEnum):
    SCAN = "scan"
    PINNED = "pinned"
    MUTATION = "mutation"


class SecurityDomain(StrEnum):
    CALLER = "caller"
    MANAGED = "managed"


class FileAccess(StrEnum):
    READ = "read"
    WRITE = "write"
    READ_WRITE = "read_write"


class CreateDisposition(StrEnum):
    OPEN_EXISTING = "open_existing"
    CREATE_NEW = "create_new"
    OPEN_OR_CREATE = "open_or_create"
```

Implement `validate_component` by importing `os` and rejecting NUL, empty, dot,
dot-dot, `os.sep`, and non-null `os.altsep` before any native call. Do not reject
backslash on POSIX; the Windows backend owns its stricter namespace rules.

- [ ] **Step 3: Write RED tests for close and detach state transitions**

Use an internal test backend that owns integer tokens and can inject failures:

```python
class RecordingOwner:
    def __init__(self) -> None:
        self.closed: list[int] = []
        self.detached: list[tuple[int, int]] = []
        self.fail_close = False
        self.fail_detach = False

    def close_resource(self, resource: object) -> None:
        if self.fail_close:
            raise OSError("close failed")
        self.closed.append(int(resource))

    def detach_file_resource(self, resource: object, flags: int) -> int:
        if self.fail_detach:
            raise OSError("detach failed")
        self.detached.append((int(resource), flags))
        return int(resource) + 100


class CapabilityOwnershipTests(unittest.TestCase):
    def test_failed_close_keeps_ownership_for_retry(self) -> None:
        owner = RecordingOwner()
        capability = _test_file_capability(owner, 7)
        owner.fail_close = True
        with self.assertRaisesRegex(OSError, "close failed"):
            capability.close()
        self.assertFalse(capability.closed)
        owner.fail_close = False
        capability.close()
        capability.close()
        self.assertTrue(capability.closed)
        self.assertEqual(owner.closed, [7])

    def test_detach_transfers_once_and_failure_keeps_ownership(self) -> None:
        owner = RecordingOwner()
        capability = _test_file_capability(owner, 9)
        owner.fail_detach = True
        with self.assertRaisesRegex(OSError, "detach failed"):
            capability.detach_to_fd(3)
        self.assertFalse(capability.detached)
        owner.fail_detach = False
        self.assertEqual(capability.detach_to_fd(3), 109)
        self.assertTrue(capability.detached)
        with self.assertRaises(RuntimeError):
            capability.detach_to_fd(3)
        capability.close()
        self.assertEqual(owner.closed, [])

    def test_directory_move_invalidates_source_and_closes_replacement_once(self) -> None:
        owner = RecordingOwner()
        source = _test_directory_capability(owner, 13)
        self.assertTrue(source.owned_by(owner))
        self.assertFalse(source.owned_by(RecordingOwner()))
        moved = source._move_for(owner)
        self.assertTrue(source.transferred)
        with self.assertRaises(RuntimeError):
            source._resource_for(owner)
        source.close()
        self.assertEqual(owner.closed, [])
        moved.close()
        moved.close()
        self.assertEqual(owner.closed, [13])
```

The test-only `_test_file_capability` and `_test_directory_capability` helpers call the private constructors with fixed metadata; neither may expose the raw resource from production properties.

- [ ] **Step 4: Implement capabilities with four explicit states**

Use one private constructor shape for both concrete capability classes:

```python
def __init__(
    self,
    owner: object,
    resource: object,
    *,
    identity: FileIdentity,
    filesystem: FilesystemIdentity,
    kind: EntryKind,
    logical_size: int,
    modified_ns: int,
    security_domain: SecurityDomain,
    share_policy: SharePolicy,
    created: bool,
    path_hint: Path,
) -> None:
```

Validate directory/file kind in the concrete constructors, reject negative
logical size, require nonzero identity values, and store `owner` by object
identity rather than equality. Use `OPEN`, `DETACHED`, `TRANSFERRED`, and
`CLOSED`; do not clear the resource before a successful owner operation or
before a replacement capability has been constructed:

```python
class _CapabilityState(StrEnum):
    OPEN = "open"
    DETACHED = "detached"
    TRANSFERRED = "transferred"
    CLOSED = "closed"


class _Capability:
    def close(self) -> None:
        if self._state is not _CapabilityState.OPEN:
            return
        self._owner.close_resource(self._resource)
        self._state = _CapabilityState.CLOSED
        self._resource = None

    @property
    def closed(self) -> bool:
        return self._state is _CapabilityState.CLOSED

    @property
    def detached(self) -> bool:
        return self._state is _CapabilityState.DETACHED

    def __enter__(self) -> _Capability:
        if self._state is not _CapabilityState.OPEN:
            raise RuntimeError("filesystem capability is not open")
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()

    def __del__(self) -> None:
        try:
            self.close()
        except OSError:
            pass
```

`FileCapability.detach_to_fd(flags)` calls `owner.detach_file_resource` first and changes state only after success. `DirectoryCapability._move_for(owner)` first constructs a replacement capability with the same owner/resource and immutable metadata, then changes the source to `TRANSFERRED` and clears only the source resource reference; constructor failure leaves the source `OPEN`. Add read-only `identity`, `filesystem`, `kind`, `logical_size`, `modified_ns`, `security_domain`, `share_policy`, `created`, `path_hint`, `is_open`, and `transferred` metadata/state properties. `is_open` is true only for `OPEN`, not merely `not closed`. Add `owned_by(owner) -> bool`, which compares only owner object identity and exposes no resource. Keep `_resource_for(owner)` private to backend implementations; it rejects the wrong backend or every state except `OPEN`. `close()` is a no-op for `DETACHED` and `TRANSFERRED` because those wrappers no longer own a resource. `created` is the native create result, is `False` for every pure open/reopen, and is never inferred from a pathname check. `DirectoryCapability` has no CRT detach operation.

- [ ] **Step 5: Define the backend protocol and lazy factory**

The protocol must contain the approved methods plus explicit protocol-security verification and handle-path recovery:

```python
class DirectoryIterator(Protocol):
    @property
    def directory(self) -> DirectoryCapability:
        raise NotImplementedError

    def __iter__(self) -> DirectoryIterator:
        raise NotImplementedError

    def __next__(self) -> DirectoryEntry:
        raise NotImplementedError

    def close(self) -> None:
        raise NotImplementedError


class FilesystemBackend(Protocol):
    def open_root(
        self,
        path: Path,
        share_policy: SharePolicy,
        security_domain: SecurityDomain = SecurityDomain.CALLER,
    ) -> DirectoryCapability:
        raise NotImplementedError

    def create_secure_root(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryCapability:
        raise NotImplementedError

    def reopen_directory(
        self,
        directory: DirectoryCapability,
        share_policy: SharePolicy | None = None,
    ) -> DirectoryCapability:
        raise NotImplementedError

    def open_directory(
        self, parent: DirectoryCapability, name: str, share_policy: SharePolicy
    ) -> DirectoryCapability:
        raise NotImplementedError

    def create_directory(
        self, parent: DirectoryCapability, name: str, share_policy: SharePolicy
    ) -> DirectoryCapability:
        raise NotImplementedError

    def open_file(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        access: FileAccess,
        disposition: CreateDisposition,
        share_policy: SharePolicy = SharePolicy.MUTATION,
    ) -> FileCapability:
        raise NotImplementedError

    def open_entry(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> FileCapability | DirectoryCapability:
        raise NotImplementedError

    def entry(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryEntry | None:
        raise NotImplementedError

    def entries(self, parent: DirectoryCapability) -> DirectoryIterator:
        raise NotImplementedError

    def entries_owned(self, parent: DirectoryCapability) -> DirectoryIterator:
        raise NotImplementedError

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        raise NotImplementedError

    def delete(self, capability: FileCapability | DirectoryCapability) -> None:
        raise NotImplementedError

    def available_bytes(self, directory: DirectoryCapability) -> int:
        raise NotImplementedError

    def allocation_unit(self, directory: DirectoryCapability) -> int:
        raise NotImplementedError

    def touch(self, file: FileCapability) -> None:
        raise NotImplementedError

    def flush(self, file: FileCapability) -> None:
        raise NotImplementedError

    def final_path(self, directory: DirectoryCapability) -> Path:
        raise NotImplementedError

    def verify_managed_security(
        self,
        capability: FileCapability | DirectoryCapability,
        *,
        repair_dacl: bool,
    ) -> None:
        raise NotImplementedError
```

Import `cache` from `functools` and `Path` from `pathlib`. The `NotImplementedError` bodies make the structural contract explicit; production backends implement every method. Use an `_platform_name() -> str` seam that returns `os.name`, and decorate `default_filesystem_backend()` with `@cache`. The factory imports `WindowsFilesystemBackend` only when `_platform_name() == "nt"`, otherwise `PosixFilesystemBackend`; imports stay inside the function. Add `_reset_default_filesystem_backend_for_tests()` that calls `default_filesystem_backend.cache_clear()`, and require every selector-patching test to clear before and in `finally` after the assertion. No lifecycle module branches on the OS, and production never resets the cache.

Define `DirectoryIterator` as an iterator protocol with a borrowed read-only `directory: DirectoryCapability` property and `close() -> None`. `entries(parent)` calls `reopen_directory(parent)` and delegates to `entries_owned`; `entries_owned(parent)` calls `parent._move_for(self)`, stores only that replacement in the iterator, closes the replacement if iterator construction fails, and leaves the caller's source wrapper in `TRANSFERRED`. Any later native operation through the source raises `RuntimeError`. Iterator close failures retain ownership for retry just like capability close failures.

Document `delete` as consuming its capability: it retains parent/name evidence, performs the exact-object namespace mutation, closes the capability, then verifies absence through the parent. On a failed close the capability remains open/retryable and `delete` raises. Callers may invoke idempotent `close()` in cleanup, but they do not perform absence verification before the consuming close.

- [ ] **Step 6: Run contract tests and commit**

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_filesystem -v
uv run --frozen python -m compileall -q tools/focused_mutation_support/filesystem.py
git add tools/focused_mutation_support/filesystem.py tests/test_focused_mutation_filesystem.py
git commit -m "refactor: define filesystem capability contract"
```

Expected: tests pass and only the two named files are committed.

---

### Task 2: Move existing Unix primitives behind a POSIX backend

**Files:**

- Create: `tools/focused_mutation_support/posix_filesystem.py`
- Modify: `tests/test_focused_mutation_filesystem.py`
- Read for extraction: `tools/focused_mutation_support/disk.py:708-1064`
- Read for extraction: `tools/focused_mutation_support/lease.py:477-707`

**Interfaces:**

- Consumes: Task 1 contract.
- Produces: complete `PosixFilesystemBackend`; later lifecycle migration must not change Linux `openat2` flags, macOS filesystem identity, streaming `fdopendir`, or no-follow relative mutation semantics.

- [ ] **Step 1: Add POSIX backend conformance RED tests**

On POSIX, run one backend instance through root open, relative create/open, streaming enumeration, identity, rename, delete, capacity, and security-domain propagation:

```python
@unittest.skipUnless(os.name == "posix", "requires POSIX descriptor primitives")
class PosixBackendTests(unittest.TestCase):
    def test_relative_lifecycle_preserves_identity_and_domain(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            managed = backend.create_secure_root(root, "managed")
            child = backend.create_directory(managed, "child", SharePolicy.MUTATION)
            created = backend.open_file(
                child,
                "marker",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            self.assertTrue(created.created)
            identity = created.identity
            descriptor = created.detach_to_fd(os.O_RDWR)
            os.write(descriptor, b"x")
            os.fsync(descriptor)
            os.close(descriptor)
            reopened = backend.open_entry(child, "marker", SharePolicy.PINNED)
            self.assertFalse(reopened.created)
            self.assertEqual(reopened.identity, identity)
            self.assertIs(reopened.kind, EntryKind.REGULAR)
            self.assertEqual(reopened.security_domain, SecurityDomain.MANAGED)
            self.assertGreater(backend.allocation_unit(root), 0)
            backend.rename(reopened, child, "renamed", replace=False)
            backend.delete(reopened)
            self.assertIsNone(backend.entry(child, "renamed"))
            child.close()
            managed.close()
            root.close()

    def test_entries_owned_moves_the_source_and_closes_once(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.SCAN)
            source = backend.reopen_directory(root)
            iterator = backend.entries_owned(source)
            self.assertTrue(source.transferred)
            with self.assertRaises(RuntimeError):
                backend.entry(source, "unused")
            self.assertEqual(list(iterator), [])
            iterator.close()
            self.assertTrue(iterator.directory.closed)
            root.close()

    def test_backslash_payload_round_trips_as_one_posix_component(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            created = backend.open_file(
                root,
                r"back\slash",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            created.close()
            entries = backend.entries(root)
            try:
                self.assertIn(r"back\slash", {entry.name for entry in entries})
            finally:
                entries.close()
            source = backend.open_entry(root, r"back\slash", SharePolicy.PINNED)
            backend.rename(source, root, r"renamed\payload", replace=False)
            backend.delete(source)
            self.assertIsNone(backend.entry(root, r"renamed\payload"))
            root.close()
```

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_filesystem.PosixBackendTests -v
```

Expected: import failure because `posix_filesystem.py` does not exist.

- [ ] **Step 2: Implement roots, reopen, metadata, and filesystem identity**

Move the existing flags and identity logic without changing their values. Create capabilities only after type, no-follow, nonzero identity, and filesystem checks pass:

```python
def _metadata(fd: int) -> tuple[FileIdentity, FilesystemIdentity, int, int, int]:
    value = os.fstat(fd)
    identity = FileIdentity(value.st_dev, value.st_ino)
    if identity.volume == 0 or identity.file == 0:
        raise OSError("filesystem returned an unsupported zero identity")
    filesystem = _filesystem_identity(fd)
    return identity, filesystem, value.st_mode, value.st_size, value.st_mtime_ns
```

Linux child opens must keep `RESOLVE_NO_XDEV | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH` (`0x01 | 0x02 | 0x04 | 0x08`). macOS must keep the `fstatfs` FSID fields at offset 48. `reopen_directory` uses `openat(".")`, then compares both object and filesystem identity before returning.

Store `fd`, non-owning parent/name evidence, optional `DIR *`, and the file
access mode in `_PosixResource`. `close_resource` calls exactly one of
`closedir` or `os.close` and changes its sentinel only after success.
`detach_file_resource` rejects a directory stream, verifies
`flags & os.O_ACCMODE` is compatible with the access used for the native open,
moves the unchanged fd out of the resource, and returns it; POSIX ignores only
the Windows-specific binary/no-inherit bits that are zero or inapplicable on
the host. Add a conformance assertion that detach followed by capability close
closes the descriptor exactly once through the caller.

- [ ] **Step 3: Implement the streaming POSIX iterator**

Move `_DirectoryStream` into a private iterator backed by the capability it owns. `entries(parent)` first reopens a capability; `entries_owned(parent)` calls `_move_for(self)`, then converts the moved capability's `_PosixResource` from an fd to a `DIR *` exactly once with `fdopendir`. On success set the stored fd sentinel and make `_directory_fd(capability)` call `dirfd(stream)`; on `fdopendir` failure the moved capability still owns and closes the original fd while the source remains `TRANSFERRED`. Its close sequence is only the moved capability's `close_resource -> closedir`, never `closedir` plus `os.close`:

```python
class _PosixEntries:
    @property
    def directory(self) -> DirectoryCapability:
        return self._directory

    def __iter__(self) -> _PosixEntries:
        return self

    def __next__(self) -> DirectoryEntry:
        while True:
            name = self._stream.next_name()
            if name is None:
                self.close()
                raise StopIteration
            entry = self._backend.entry(self._directory, name)
            if entry is not None:
                return entry

    def close(self) -> None:
        if self._closed:
            return
        self._directory.close()
        self._closed = True
```

Set `_closed` only after `closedir` succeeds. If it fails, the capability/resource remains open and `close()` is retryable. The finalizer may retry close but may not suppress an explicit iteration/close error.

`entry` uses `os.stat(name, dir_fd=parent_fd, follow_symlinks=False)`, maps symlink to `REPARSE`, and treats only `FileNotFoundError` as `None`. When `st_dev == parent.identity.volume`, copy `parent.filesystem` into the record. When it differs, set `FilesystemIdentity(st_dev)` as explicit crossing evidence; the subsequent typed open must reject it against the retained root filesystem rather than silently borrowing the parent's identity.

- [ ] **Step 4: Implement relative create/open, rename, and delete**

All child operations call `validate_component`. Preserve the current explicit private modes for every Hoimin-created POSIX object: `0700` for directories and `0600` for files, subject only to a restrictive umask. `SecurityDomain.CALLER` means “do not chmod an existing caller root,” not “weaken newly-created marker/spool modes.” `CREATE_NEW` maps to `O_CREAT|O_EXCL`; `OPEN_EXISTING` has neither.

Implement `OPEN_OR_CREATE` without an existence precheck: try `O_CREAT|O_EXCL` first and return `created=True` on success. On `FileExistsError`, observe the no-follow entry and open without `O_CREAT`, returning `created=False` only after handle/observed identities match. If that existing entry vanishes, retry the pair at most eight times, then raise `OSError("open-or-create entry did not stabilize")`. `create_secure_root` uses the same exclusive-create/observed-existing distinction around `mkdirat`. Add tests for both create results and an injected repeated create/open disappearance that proves the ninth cycle is never attempted.

Require a `PINNED` source for POSIX rename or deletion as the shared cross-platform lifecycle contract, then compare the current `fstatat` identity with the opened capability identity. POSIX has no Windows-style delete-sharing lock, so the existing `*at` identity check remains mandatory. Use only the live parent and stored single component:

```python
def delete(self, capability: FileCapability | DirectoryCapability) -> None:
    if capability.share_policy is not SharePolicy.PINNED:
        raise ValueError("delete requires a pinned source capability")
    resource = self._resource(capability)
    parent = resource.parent
    name = resource.name
    if parent is None or name is None:
        raise OSError("root capability is not a deletable child")
    current = self.entry(parent, name)
    if current is None:
        raise FileNotFoundError(name)
    if current.identity != capability.identity:
        raise OSError("delete target identity changed")
    if capability.kind is EntryKind.DIRECTORY:
        os.rmdir(name, dir_fd=self._directory_fd(parent))
    else:
        os.unlink(name, dir_fd=self._directory_fd(parent))
    capability.close()
    if self.entry(parent, name) is not None:
        raise OSError("deleted entry still resolves through its parent")
```

`rename` applies the same `PINNED` precondition, calls `os.rename(source_name, destination_name, src_dir_fd=source_parent_fd, dst_dir_fd=destination_parent_fd)` or the equivalent `os.replace` call for `replace=True`, and updates only the capability's private parent/name/path hint after success.

- [ ] **Step 5: Implement capacity, path recovery, touch, flush, and managed security**

`available_bytes` uses `fstatvfs` and rejects nonpositive fragment size or negative available blocks. `allocation_unit` returns the same positive `f_frsize`. `final_path` retains `F_GETPATH` on macOS and `/proc/self/fd/<fd>` on Linux. `touch` uses descriptor-based `os.utime(fd, None)`; `flush` uses `os.fsync(fd)`.

`open_entry` first records `entry(parent, name)`. Regular files and directories reuse the typed relative open paths. For a symlink or other non-followable POSIX entry, reopen the parent descriptor as the capability-owned native resource and store only the validated component plus recorded entry identity; `delete` rechecks that identity immediately before `unlinkat`. It never follows the entry and never treats an open/type error as absence.

For an existing managed root or protocol object, require `st_uid == os.geteuid()` before `fchmod`; repair only `0700` directories or `0600` files and verify afterward. Caller-domain objects are never chmodded by `verify_managed_security`.

- [ ] **Step 6: Add race, close, and bounded-stream regression tests**

Add four named tests using small temporary fixtures and explicit private injection seams:

- `test_posix_open_rejects_entry_replaced_between_stat_and_open`: configure `_before_relative_open(parent, name)` to rename the observed entry and create a same-kind replacement, then assert `open_file` raises an identity-change `OSError` and both files retain their original bytes.
- `test_posix_delete_rejects_same_name_replacement`: open a capability, rename its entry, create a same-name sentinel, call `delete`, and assert the delete raises while both the opened original and replacement sentinel still exist.
- `test_posix_entries_close_reopened_descriptor_on_decode_error`: inject a stream whose first `next_name()` raises `UnicodeDecodeError`, assert iteration raises, and assert its reopened descriptor records exactly one `closedir` and no second `os.close`.
- `test_posix_managed_security_refuses_wrong_owner_before_chmod`: inject `_effective_uid()` to return a value different from the capability's recorded `st_uid` and `_chmod_resource()` to append calls; assert `PermissionError` and an empty chmod call list.

The production defaults for those seams call no hook, `os.geteuid`, and `os.fchmod`. The hooks live only in `PosixFilesystemBackend`; lifecycle modules do not receive test switches.

- [ ] **Step 7: Run POSIX backend tests and commit**

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_filesystem -v
uv run --frozen python -m compileall -q tools/focused_mutation_support/posix_filesystem.py
git add tools/focused_mutation_support/posix_filesystem.py tests/test_focused_mutation_filesystem.py
git commit -m "refactor: encapsulate POSIX filesystem capabilities"
```

Expected: PASS on POSIX. On Windows, value/ownership tests pass and POSIX-native tests report only their explicit primitive skip.

---

### Task 3: Build the Windows handle, open, identity, and CRT-transfer primitives

**Files:**

- Create: `tools/focused_mutation_support/windows_filesystem.py`
- Create: `tests/test_focused_mutation_windows_filesystem.py`

**Interfaces:**

- Consumes: Task 1 capability values and ownership callbacks.
- Produces: `_WindowsApi`, `_WindowsResource`, native root/relative open helpers, metadata extraction, and `WindowsFilesystemBackend.close_resource`/`detach_file_resource`. Task 4 adds enumeration/capacity; Task 5 completes namespace mutation/security and activates the backend factory.

- [ ] **Step 1: Add RED tests for pinned roots, relative identity, and CRT transfer**

Create native Windows tests that are skipped only when the host is not Windows:

```python
from __future__ import annotations

import os
from pathlib import Path
import tempfile
import unittest

from tools.focused_mutation_support.filesystem import (
    CreateDisposition,
    FileAccess,
    SharePolicy,
)
from tools.focused_mutation_support.windows_filesystem import WindowsFilesystemBackend


@unittest.skipUnless(os.name == "nt", "requires Windows native handles")
class WindowsOpenTests(unittest.TestCase):
    def test_relative_file_open_keeps_the_enumerated_identity(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "item").write_bytes(b"payload")
            root = backend.open_root(root_path, SharePolicy.SCAN)
            listed = backend.entry(root, "item")
            self.assertIsNotNone(listed)
            opened = backend.open_file(
                root,
                "item",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.SCAN,
            )
            assert listed is not None
            self.assertEqual(opened.identity, listed.identity)
            opened.close()
            root.close()

    def test_crt_transfer_has_one_owner(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            file = backend.open_file(
                root,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            descriptor = file.detach_to_fd(os.O_RDWR | os.O_BINARY)
            os.write(descriptor, b"payload")
            os.close(descriptor)
            file.close()
            self.assertEqual((Path(raw) / "item").read_bytes(), b"payload")
            root.close()
```

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_windows_filesystem.WindowsOpenTests -v
```

Expected: import failure because `windows_filesystem.py` does not exist.

- [ ] **Step 2: Define exact-width native structures and checked bindings**

At module import on Windows, load `kernel32`, `advapi32`, and `ntdll` with `use_last_error=True` where applicable. Define exact structures for `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `IO_STATUS_BLOCK`, `BY_HANDLE_FILE_INFORMATION`, `FILETIME`, `FILE_ID_128`, `FILE_ID_INFO`, `FILE_BASIC_INFO`, and variable-length rename/enumeration headers. Bind `GetSystemTimeAsFileTime`, `SetFileTime`, and `FlushFileBuffers` along with the open/identity APIs. Set every `argtypes` and `restype`; do not rely on ctypes defaults. Use fixed-width ctypes even when the module is imported on POSIX for pure parser tests; do not use host-width `ctypes.wintypes.WCHAR`, `LONG`, or `ULONG` in an on-disk/native layout.

Use pointer-sized types for handles and NT information fields:

```python
HANDLE = ctypes.c_void_p
NTSTATUS = ctypes.c_int32
ULONG_PTR = ctypes.c_size_t
USHORT = ctypes.c_uint16
ULONG = ctypes.c_uint32
WCHAR = ctypes.c_uint16


class UNICODE_STRING(ctypes.Structure):
    _fields_ = [
        ("Length", USHORT),
        ("MaximumLength", USHORT),
        ("Buffer", ctypes.POINTER(WCHAR)),
    ]


class OBJECT_ATTRIBUTES(ctypes.Structure):
    _fields_ = [
        ("Length", ULONG),
        ("RootDirectory", HANDLE),
        ("ObjectName", ctypes.POINTER(UNICODE_STRING)),
        ("Attributes", ULONG),
        ("SecurityDescriptor", ctypes.c_void_p),
        ("SecurityQualityOfService", ctypes.c_void_p),
    ]


class IO_STATUS_BLOCK(ctypes.Structure):
    _fields_ = [("Status", NTSTATUS), ("Information", ULONG_PTR)]
```

Add import-safe layout tests before any native call. On a 64-bit interpreter
require `sizeof(UNICODE_STRING) == 16`,
`sizeof(OBJECT_ATTRIBUTES) == 48`, and `IO_STATUS_BLOCK.Information.offset == 8`;
on 32-bit require 8, 24, and 4 respectively. On both widths require
`FILE_ID_INFO.FileId.offset == 8` and
`FILE_ID_EXTD_DIR_INFO.FileName.offset == 88`. Assert the variable rename
buffer uses `FILE_RENAME_INFORMATION.FileName.offset` from ctypes rather than a
hand-computed allocation length. A layout mismatch fails module tests before a
DLL function can receive the structure.

Add one `_error_from_win32(code, operation, component)` constructor used by
both `_raise_last_error` and `_raise_ntstatus`; the latter first calls
`RtlNtStatusToDosError`. Preserve the numeric `winerror` and Python `errno`
mapping, but map only `ERROR_FILE_NOT_FOUND` to `FileNotFoundError` for a
validated one-component leaf lookup. `ERROR_PATH_NOT_FOUND` means the retained
parent/path context failed and remains an `OSError`, so a walker cannot mistake
parent disappearance for a vanished child. Map `ERROR_ACCESS_DENIED` to
`PermissionError`; a sharing violation, invalid name, malformed buffer,
unsupported API, or any other code remains an `OSError`, never absence. Bound
every rendered component/path to 4 KiB before constructing the exception. Add
pure conversion tests for all categories and assert the original Win32 code
remains inspectable.

- [ ] **Step 3: Implement owned native resources and handle close semantics**

Represent a directory/file resource with its handle plus non-owning parent/name evidence. The capability, not this resource, stores the immutable `created` result:

```python
@dataclass(slots=True)
class _WindowsResource:
    handle: int
    parent: DirectoryCapability | None
    name: str | None
    delete_authority: bool


def close_resource(self, resource: object) -> None:
    native = self._checked_resource(resource)
    if not self._api.CloseHandle(native.handle):
        self._raise_last_error("close filesystem capability", native.name)
    native.handle = INVALID_OWNED_HANDLE


def detach_file_resource(self, resource: object, flags: int) -> int:
    native = self._checked_resource(resource)
    descriptor = msvcrt.open_osfhandle(native.handle, flags | os.O_NOINHERIT)
    native.handle = INVALID_OWNED_HANDLE
    return descriptor
```

The capability changes to `DETACHED` only after `detach_file_resource` returns. If `open_osfhandle` raises, leave `native.handle` untouched. A successful transfer sets the resource sentinel so a backend bug cannot close the same kernel handle later. In the native transfer test, assert `os.get_handle_inheritable(msvcrt.get_osfhandle(descriptor))` is false before closing the descriptor.

- [ ] **Step 4: Implement root and relative no-follow opens**

`open_root` calls `CreateFileW` with `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`, a null inheritable-security pointer, `OPEN_EXISTING`, and the share/access profile. `_open_relative` calls `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory` set to the live parent handle, `OBJ_CASE_INSENSITIVE`, and one validated UTF-16 component.

Before building `UNICODE_STRING`, `_encode_windows_component(name)` calls the shared `validate_component`, encodes with strict UTF-16, checks the byte length fits `USHORT`, and rejects `:`, code points below U+0020, `<`, `>`, `"`, `|`, `?`, `*`, a trailing dot/space, and case-insensitive DOS device basenames (`CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `COM1`-`COM9`, `LPT1`-`LPT9`, plus the documented superscript-digit `COM¹`-`COM³` and `LPT¹`-`LPT³`, including a suffix after the first dot). The native API call counter must remain zero for every rejection; include `CONOUT$.log` in the table.

Use these exact mappings:

```python
def _share_mode(policy: SharePolicy) -> int:
    common = FILE_SHARE_READ | FILE_SHARE_WRITE
    return common if policy is SharePolicy.PINNED else common | FILE_SHARE_DELETE


def _directory_access(policy: SharePolicy, *, relative_target: bool) -> int:
    scan = (
        SYNCHRONIZE
        | READ_CONTROL
        | FILE_READ_ATTRIBUTES
        | FILE_LIST_DIRECTORY
        | FILE_TRAVERSE
    )
    if policy is SharePolicy.SCAN:
        return scan
    child_mutation = (
        FILE_ADD_FILE
        | FILE_ADD_SUBDIRECTORY
        | FILE_DELETE_CHILD
    )
    if policy is SharePolicy.PINNED:
        self_delete = DELETE if relative_target else 0
        return scan | child_mutation | self_delete
    return scan | child_mutation | DELETE | FILE_WRITE_ATTRIBUTES


def _file_access(access: FileAccess, policy: SharePolicy) -> int:
    value = SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES
    if access in {FileAccess.READ, FileAccess.READ_WRITE}:
        value |= FILE_READ_DATA
    if access in {FileAccess.WRITE, FileAccess.READ_WRITE}:
        value |= FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES
    if policy is SharePolicy.MUTATION:
        value |= DELETE
    return value


def _entry_access(policy: SharePolicy) -> int:
    value = SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES
    if policy in {SharePolicy.PINNED, SharePolicy.MUTATION}:
        value |= DELETE
    return value
```

Add a table test over all three policies and both directory contexts. Assert `PINNED` omits `DELETE` for an absolute root and an ordinary typed file, includes it for a relative directory and `open_entry`, and always omits delete sharing; `SCAN` has neither child mutation nor self-delete; `MUTATION` includes both but shares delete. Deletion-only opens must not accidentally request file data or directory-list authority. `open_root` passes `relative_target=False`; every relative directory create/open passes `True`. `_WindowsResource.delete_authority` records whether the native desired-access mask included `DELETE`; it is never inferred from the enum alone. For this task, support existing root/directory/file opens, `open_entry`, and caller-domain file creation. Map `IO_STATUS_BLOCK.Information == FILE_CREATED` to `capability.created=True`; map `FILE_OPENED`, root opens, and duplicates to `False`, and reject every unexpected information value. `open_entry` uses `_entry_access`, requests `FILE_OPEN_REPARSE_POINT` without a file/directory type option, and returns the kind found on the opened handle. Task 5 adds managed descriptors and secure-root semantics before factory activation.

Implement `entry(parent, name)` with its own short-lived no-follow relative
observation handle: request only synchronize/read-control/read-attributes,
share read/write/delete, impose no file/directory type option, read the same
128-bit identity/kind/filesystem metadata, and close before returning the
immutable `DirectoryEntry`. Return `None` only for the leaf
`FileNotFoundError`; a close, sharing, access, parent-path, or metadata failure
raises. Do not implement point lookup by draining a directory enumeration, and
do not call a typed open that would erase reparse/other kind evidence.

- [ ] **Step 5: Implement and validate Windows identity metadata**

Call `GetFileInformationByHandleEx(FileIdInfo)` and convert the 16 ID bytes from the native `FILE_ID_128` buffer to one unsigned Python integer. Reject a zero 64-bit volume serial or all-zero 128-bit ID. Use `GetFileInformationByHandle` only for attributes, checked high/low size composition, and the same exact FILETIME-to-Unix-nanoseconds conversion used by Task 4; never use its 64-bit file index as identity. Call `GetVolumeInformationByHandleW`, require its 32-bit serial to equal the low 32 bits of `FILE_ID_INFO.VolumeSerialNumber`, and build `FilesystemIdentity(the_64_bit_volume_serial, maximum_component_length, filesystem_flags)`. Map attributes to `EntryKind` with reparse checked before directory/regular. A volume-management API failure, including an unsupported remote filesystem, fails closed before workflow launch rather than weakening identity.

For `open_directory`, `open_entry`, and `open_file(OPEN_EXISTING)`, capture the
non-follow parent entry before the native open, then require the opened handle
and a second parent entry to match that same kind, identity, and filesystem.
Typed directory/file opens reject reparse points; `open_entry` may retain a
`REPARSE` kind only for exact-object deletion. For `CREATE_NEW`, let
`FILE_CREATE` establish exclusivity, capture handle metadata, and require a
post-create parent entry to match it. For `OPEN_OR_CREATE`, retain the native
`FILE_CREATED`/`FILE_OPENED` result, compare any pre-open observation when one
existed, and always require the post-open entry to match the handle. Close the
new handle on every validation failure and attach a bounded close note without
replacing the primary error.

The replacement-hook test runs between the first observation and native open
for existing objects, and between native success and the post-open observation
for creates, so both race windows have a deterministic RED case.

- [ ] **Step 6: Add native replacement and invalid-component tests**

Add tests with an injected hook between parent-entry observation and `NtCreateFile`. The hook renames the original to `old` and replaces it with a same-kind object. Assert the open raises `OSError`, neither outside sentinel is changed, and all invalid components are rejected before `_WindowsApi.NtCreateFile` records a call.

Use a small recording wrapper around `_WindowsApi`:

```python
class OpenHookApi(_WindowsApi):
    def __init__(self, hook: Callable[[], None]) -> None:
        super().__init__()
        self._hook = hook

    def before_relative_open(self) -> None:
        self._hook()
```

The production `_WindowsApi.before_relative_open` is a no-op test seam; the backend invokes it immediately before `NtCreateFile`.

- [ ] **Step 7: Run native open tests and commit**

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_windows_filesystem.WindowsOpenTests -v
uv run --frozen python -m compileall -q tools/focused_mutation_support/windows_filesystem.py
git add tools/focused_mutation_support/windows_filesystem.py tests/test_focused_mutation_windows_filesystem.py
git commit -m "feat: add Windows filesystem handle primitives"
```

Expected: native open/identity/transfer tests pass on Windows; non-Windows discovery imports no Windows DLL and reports only the native test-class skip.

---

### Task 4: Add bounded Windows enumeration and handle-tied capacity

**Files:**

- Modify: `tools/focused_mutation_support/windows_filesystem.py`
- Modify: `tests/test_focused_mutation_windows_filesystem.py`

**Interfaces:**

- Consumes: Task 3 handles and metadata.
- Produces: `entries`, `reopen_directory`, `available_bytes`, `allocation_unit`, and `final_path` with bounded buffers and identity revalidation.

- [ ] **Step 1: Write RED tests for streaming records and malformed buffers**

Add a pure parser test with two valid synthetic records and one case for every malformed boundary:

```python
class WindowsDirectoryRecordTests(unittest.TestCase):
    def test_parser_yields_one_record_at_a_time(self) -> None:
        encoded = _directory_record("alpha", file_id=7, next_offset=128)
        encoded += _directory_record("beta", file_id=9, next_offset=0)
        parser = _DirectoryRecordParser(encoded)
        self.assertEqual(parser.next_record().name, "alpha")
        self.assertEqual(parser.next_record().name, "beta")
        self.assertIsNone(parser.next_record())

    def test_parser_rejects_invalid_offset_length_alignment_and_progress(self) -> None:
        cases = (
            _record_with_next_offset(2),
            _record_with_next_offset(64),
            _record_with_odd_name_length(),
            _record_with_name_outside_buffer(),
            _record_with_self_offset(),
            _record_with_zero_file_id_128(),
            _record_with_inconsistent_reparse_tag(),
        )
        for encoded in cases:
            with self.subTest(length=len(encoded)), self.assertRaises(OSError):
                _DirectoryRecordParser(encoded).next_record()
```

Run the class and expect missing parser/helpers.

- [ ] **Step 2: Implement a 64 KiB streaming directory iterator**

Use a fixed `64 * 1024` byte buffer. The first refill calls `GetFileInformationByHandleEx(FileIdExtdDirectoryRestartInfo)` and later refills use `FileIdExtdDirectoryInfo`; decode the `FILE_ID_EXTD_DIR_INFO` layout and convert its 16-byte ID to the same unsigned Python integer used by direct opens. Treat only `ERROR_NO_MORE_FILES` as clean exhaustion.

For every record verify:

- header fits before reading fields;
- `FileNameLength` is even and the name fits;
- nonzero `NextEntryOffset` is aligned, advances beyond this header/name, and stays within bytes returned;
- the last record has `NextEntryOffset == 0`;
- UTF-16 decoding succeeds without replacement;
- dot and dot-dot are filtered;
- the 128-bit file ID and retained parent volume are nonzero;
- `ReparsePointTag` is nonzero exactly when `FILE_ATTRIBUTE_REPARSE_POINT` is set.

`reopen_directory` uses `DuplicateHandle`, compares object and filesystem identity after duplication, and marks the duplicate `created=False`. A requested policy is accepted only when it has the same share mode and requires no authority absent from the source; otherwise it raises `ValueError`. `entries(parent)` calls `entries_owned(backend.reopen_directory(parent))`; `entries_owned(parent)` calls `_move_for(self)` and stores that moved capability in `_WindowsEntries` without duplicating its kernel handle. Its `directory` property borrows the iterator-owned replacement for relative child opens. It closes that capability on exhaustion, parser failure, explicit `close`, or finalization and retains only one decoded `DirectoryEntry` at a time.

For every decoded record, set `DirectoryEntry.filesystem` to the retained parent filesystem, reject a negative raw `LastWriteTime`, and convert its 100-nanosecond intervals since 1601 to Unix nanoseconds with exact Python integer arithmetic (`(ticks - 116_444_736_000_000_000) * 100`). Reject negative logical sizes; allow a valid pre-1970 result to remain negative just as POSIX `st_mtime_ns` can.

- [ ] **Step 3: Add native enumeration behavior tests**

Create regular files, a directory, and a reparse fixture. Assert names, kinds, logical sizes, and identities match direct opens. Delete one candidate concurrently after enumeration starts; because `SCAN` shares deletion, iteration either omits that vanished entry or reports its original record, but it must not convert a sharing/access error into absence.

Also instrument capability counts and assert the walker-owned root frame plus depth-128 child frames never exceeds 129 open directory capabilities. For one configured `MeterRoot`, the separately retained anchor makes the process total 130; report both counters so the retained anchor is neither hidden nor double-counted.

- [ ] **Step 4: Implement final-path recovery and capacity revalidation**

Call `GetFinalPathNameByHandleW` through one bounded helper parameterized by `VOLUME_NAME_DOS` or `VOLUME_NAME_GUID`. Start with 512 UTF-16 units, resize to the returned required length, and reject any result above 16 KiB. Public `final_path` uses the DOS form and requires an absolute `\\?\` path. Capacity uses the GUID form, requires a prefix shaped exactly as `\\?\Volume{GUID}\`, and calls `GetVolumePathNameW` to recover a trailing-backslash volume root. A missing GUID form (including unsupported network volumes) is a native preflight failure, not a DOS-path fallback.

`available_bytes` performs this sequence:

```python
before_object = directory.identity
before_filesystem = self._metadata(directory).filesystem
volume_path, volume_root = self._volume_paths(directory)
self._verify_path_volume(volume_root, before_filesystem)
available = self._api.get_disk_free_space_ex(volume_path)
self._verify_path_volume(volume_root, before_filesystem)
after_object = self._metadata(directory).identity
after_filesystem = self._metadata(directory).filesystem
if before_object != after_object or before_filesystem != after_filesystem:
    raise OSError("capacity root identity changed")
if available < 0:
    raise OSError("filesystem reported negative available bytes")
return available
```

The GUID path is only an argument required by `GetDiskFreeSpaceExW`; `_verify_path_volume` calls `GetVolumeInformationW(volume_root)` and requires its 32-bit serial to equal the low 32 bits of `FilesystemIdentity.volume`, and its maximum component length/filesystem flags to equal the two discriminators, before and after the query. The retained handle identities before and after remain authoritative. `allocation_unit` performs the same handle/path-volume checks around `GetDiskFreeSpaceW(volume_root)`, which requires a root rather than an arbitrary directory, multiplies positive `SectorsPerCluster` by positive `BytesPerSector` with an overflow check, and rejects zero values.

- [ ] **Step 5: Add sharing-policy and capacity-race tests**

On Windows, prove `SCAN` permits renaming a directory after its handle opens while `PINNED` makes the same rename fail with a sharing violation until close. Inject a fake capacity API that changes the reported volume identity after `GetDiskFreeSpaceExW`; assert `available_bytes` raises rather than returning the number.

- [ ] **Step 6: Run enumeration/capacity tests and commit**

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_windows_filesystem.WindowsDirectoryRecordTests `
  tests.test_focused_mutation_windows_filesystem.WindowsEnumerationTests `
  tests.test_focused_mutation_windows_filesystem.WindowsCapacityTests -v
git add tools/focused_mutation_support/windows_filesystem.py tests/test_focused_mutation_windows_filesystem.py
git commit -m "feat: enumerate and meter Windows capabilities"
```

Expected: all native tests pass on Windows and all buffers remain fixed/bounded.

---

### Task 5: Complete Windows security, relative rename, and deletion

**Files:**

- Modify: `tools/focused_mutation_support/windows_filesystem.py`
- Modify: `tools/focused_mutation_support/filesystem.py`
- Modify: `tests/test_focused_mutation_windows_filesystem.py`
- Modify: `tests/test_focused_mutation_filesystem.py`

**Interfaces:**

- Consumes: Tasks 1, 3, and 4.
- Produces: a complete `WindowsFilesystemBackend` selected by `default_filesystem_backend()` with secure root/child creation, explicit protocol-security verification, anchored rename, and handle deletion.

- [ ] **Step 1: Add RED security tests before native security code**

Add native tests that create a `MANAGED` root and children, obtain the token-user SID, and compare owner plus exact protected DACL. Also snapshot a caller output root's security descriptor before and after `open_root(output, SharePolicy.PINNED, SecurityDomain.CALLER)` with a read-only test helper `_security_descriptor_bytes_for_tests(path)` and assert it is byte-identical. The helper calls `GetNamedSecurityInfoW` for owner/group/DACL, obtains the exact allocation length with `GetSecurityDescriptorLength`, copies that self-relative descriptor before `LocalFree`, and is never used as mutation authority. Bind both functions with exact `argtypes`/`restype`; reject a null descriptor or zero length.

Add an injected policy-order test independent of local admin status:

```python
def test_existing_owner_mismatch_never_writes_owner_or_dacl(self) -> None:
    api = RecordingSecurityApi(owner_matches=False)
    backend = WindowsFilesystemBackend(api=api)
    capability = _security_test_capability(backend)
    with self.assertRaises(PermissionError):
        backend.verify_managed_security(capability, repair_dacl=True)
    self.assertEqual(api.owner_writes, 0)
    self.assertEqual(api.dacl_writes, 0)
```

Run the security test class and expect missing security bindings.

- [ ] **Step 2: Implement token-user and descriptor ownership**

Use `OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY)`, size-query `GetTokenInformation(TokenUser)`, and keep the returned buffer alive while its SID pointer is used. Convert it with `ConvertSidToStringSidW` and build:

```text
O:<SID>D:P(A;OICI;FA;;;<SID>)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)
```

for directories and the same string without `OICI` for files. Convert through `ConvertStringSecurityDescriptorToSecurityDescriptorW`; release all local allocations with `LocalFree` in owning wrappers.

To verify, call `GetSecurityInfo` for owner and DACL, `EqualSid` for owner, `GetSecurityDescriptorControl` for `SE_DACL_PROTECTED`, and compare `AclSize` bytes with the expected descriptor DACL. Owner mismatch returns `PermissionError` before any setter. DACL-only repair calls `SetSecurityInfo` with `DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION` and null owner/group pointers.

- [ ] **Step 3: Apply descriptors only at managed creation boundaries**

For `create_secure_root` use relative `NtCreateFile(FILE_OPEN_IF)` with `READ_CONTROL | WRITE_DAC` in addition to the normal mutation access, and retain `IO_STATUS_BLOCK.Information`. If it is `FILE_CREATED`, the descriptor must have been supplied in `OBJECT_ATTRIBUTES.SecurityDescriptor` and exact verification follows. If it is `FILE_OPENED`, verify token-user owner first, optionally repair only the DACL, and verify identity again.

`create_directory` and `open_file` inherit `parent.security_domain`. A new child below `MANAGED` receives the matching directory/file descriptor in `OBJECT_ATTRIBUTES`; a new child below `CALLER` receives a null descriptor and preserves normal inheritance. `create_directory` uses `FILE_CREATE`. For files, `CREATE_NEW` uses `FILE_CREATE`; `OPEN_OR_CREATE` uses `FILE_OPEN_IF` and adds `WRITE_DAC` only below `MANAGED`, because this is the only generic file-open boundary allowed to repair an owner-verified existing protocol object. An existing result never causes an owner write. Plain `OPEN_EXISTING`, `open_entry`, and payload-directory opens do not request `WRITE_DAC` and do not mutate security merely because the parent capability is managed.

Do not automatically require protocol ACLs when opening arbitrary worker payload during cleanup. Lifecycle code calls `verify_managed_security` only for the managed root, coordinator, lease, heartbeat, retention, cleanup-ready marker, and Hoimin run directories it recognizes. Output-owner markers and deterministic report temporaries remain below the caller-domain output root. Command spools are exclusively created below the managed command root and receive its creation descriptor, but an existing spool is never repaired or accepted as a managed protocol file. Arbitrary payload is never relabeled on open.

- [ ] **Step 4: Implement handle-relative rename**

Require `source.share_policy is SharePolicy.PINNED`, a non-null retained parent/name, and `_WindowsResource.delete_authority is True` before any rename API. Thus a file source must come from `open_entry(source_parent, source_name, SharePolicy.PINNED)`, while a relative pinned directory already has the right; a long-lived typed marker handle is never silently upgraded. Build `FILE_RENAME_INFORMATION` using its ctypes field offsets, set `RootDirectory` to the live destination parent, encode exactly one validated UTF-16 destination, and call `NtSetInformationFile(FileRenameInformation)`. Set `ReplaceIfExists` from the method argument. Immediately before the call, require the source handle and old parent entry to match. After success, require the handle identity to be unchanged, the destination entry to match it, and the old entry to be absent whenever the `(parent identity, name)` pair changed; only then update the capability's private parent/name evidence.

Add tests for same-parent staging-to-active rename, replacement-enabled report rename, destination escape rejection, and policy enforcement. In the native race case, an external same-name move attempted after the pinned source opens must fail with a sharing violation; after that assertion, the backend rename succeeds on the original identity. A `MUTATION` source is rejected before `NtSetInformationFile` is called. The outside sentinel remains unchanged in every rejected case.

- [ ] **Step 5: Implement delete disposition on the opened object**

Use `SetFileInformationByHandle(FileDispositionInfoEx)` with:

```python
flags = (
    FILE_DISPOSITION_FLAG_DELETE
    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE
)
```

Require `capability.share_policy is SharePolicy.PINNED`, a non-null retained parent/name, and `_WindowsResource.delete_authority is True` before setting disposition. If and only if the filesystem returns `ERROR_INVALID_PARAMETER` or `ERROR_NOT_SUPPORTED` for the extended class/flags, retry `FileDispositionInfo` on the same handle. This is a native compatibility path, not a pathname fallback; every other error remains primary. Reject nonempty directories as errors. Revalidate the current parent entry immediately before disposition, retain the live parent and validated component locally, set disposition on the exact pinned object, then call `capability.close()`; only after close succeeds may `entry(parent, name)` verify absence. If the original identity or any replacement remains, raise without opening or deleting it. A failed close keeps the disposition-bearing capability retryable. Add a native test that an external rename/delete fails while the pinned capability is open, plus recording tests that a `MUTATION` capability and a typed pinned file without delete authority are rejected before disposition. This sequence follows the native rule that deletion becomes effective when the disposition handle closes.

Complete the remaining file-capability primitives here. `touch` captures the
file identity, obtains a native `FILETIME` through `GetSystemTimeAsFileTime`,
passes it only as the last-write argument to `SetFileTime`, and requires the
identity to remain unchanged afterward. `flush` calls `FlushFileBuffers` on the
same live file handle and likewise rejects an identity change. Neither method
reopens a pathname or treats a sharing/access failure as success. Add a native
heartbeat test that observes a nondecreasing last-write time, a flush call, and
the same 128-bit identity.

- [ ] **Step 6: Run native mutation/security tests and activate the factory**

Run:

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_windows_filesystem.WindowsSecurityTests `
  tests.test_focused_mutation_windows_filesystem.WindowsMutationTests -v
uv run --frozen python -m unittest tests.test_focused_mutation_filesystem -v
```

After they pass, make the factory instantiate `WindowsFilesystemBackend` on Windows. Add a factory test that patches only the platform-selector helper and asserts lifecycle callers receive the correct backend without importing Windows DLLs on POSIX.

- [ ] **Step 7: Commit the complete Windows backend**

```powershell
git add `
  tools/focused_mutation_support/filesystem.py `
  tools/focused_mutation_support/windows_filesystem.py `
  tests/test_focused_mutation_filesystem.py `
  tests/test_focused_mutation_windows_filesystem.py
git commit -m "feat: complete native Windows filesystem capabilities"
```

Expected: the backend is complete and selected, but lifecycle modules still use their old POSIX helpers until Tasks 6-10.

---

### Task 6: Migrate disk metering and reservations to capabilities

**Files:**

- Modify: `tools/focused_mutation_support/disk.py:1-81`
- Modify: `tools/focused_mutation_support/disk.py:696-1064`
- Modify: `tools/focused_mutation_support/disk.py:1067-1488`
- Modify: `tests/test_focused_mutation_disk.py:64-184`
- Modify: `tests/test_focused_mutation_disk.py:443-1229`

**Interfaces:**

- Consumes: complete POSIX/Windows backend contract.
- Produces: `MeterRoot.capability_factory: Callable[[], DirectoryCapability] | None`, `DiskGuard(policy, roots, *, monotonic=time.monotonic, heartbeat=None, backend: FilesystemBackend | None = None)`, `_measure_capability`, and `reserve_additional_bytes(additional_bytes, *, filesystem: DirectoryCapability)`; later owners provide anchored root factories and store/runner code passes command-root capabilities.

- [ ] **Step 1: Add a native Windows capacity RED test while retaining the activation guard**

Keep `test_windows_fails_closed_before_disk_safe_runtime_setup` unchanged until Task 11; Tasks 6-10 do not activate a partially migrated workflow. Add only this native capacity assertion:

```python
@unittest.skipUnless(os.name == "nt", "requires Windows capacity API")
def test_windows_guard_samples_a_real_root(self) -> None:
    with tempfile.TemporaryDirectory() as raw:
        Path(raw, "payload").write_bytes(b"1234")
        guard = DiskGuard(
            DiskPolicy(max_disk_bytes=1024, min_free_bytes=1, scratch_root=Path(raw)),
            [MeterRoot(Path(raw), enforcement="owned:test")],
        )
        self.assertIsNone(guard.sample())
        self.assertEqual(guard.observations[-1].owned_bytes, 4)
        self.assertEqual(guard.close(), ())
```

Run this test. Expected: the native guard still calls unsupported POSIX APIs before the migration; the CLI activation guard remains independently green.

- [ ] **Step 2: Make scratch-root validation use the selected backend**

Change `canonical_scratch_root(value, *, backend=None)` to open the canonical root with `SCAN`, call `available_bytes`, and close in `finally`. Continue using strict `Path.resolve` only for initial root selection; no destructive action depends on it.

```python
selected = default_filesystem_backend() if backend is None else backend
capability = selected.open_root(canonical, SharePolicy.SCAN)
try:
    selected.available_bytes(capability)
finally:
    capability.close()
```

Convert open/capacity failures to the same `ValueError` messages currently exposed by `DiskPolicy`.

- [ ] **Step 3: Replace the POSIX descriptor walker with generic DFS**

Delete `_open_directory`, `_open_child_directory`, `_open_child_regular`, `_reopen_same_directory`, `_filesystem_identity`, and `_DirectoryStream` from `disk.py`; their POSIX implementation now lives in Task 2.

Implement `_measure_capability` with the current deadline/order/limit behavior. Reopen the retained root once, pass it to `entries_owned`, and store only that `DirectoryIterator` plus bounded path evidence in each stack frame; `iterator.directory` is the borrowed live parent for child opens. A child `DirectoryCapability` is immediately transferred to `entries_owned` before its frame is pushed. Thus root plus depth 128 is exactly 129 walker-owned directory capabilities, not a second handle per iterator. The core selection logic is:

```python
if entry.kind is EntryKind.REPARSE or entry.kind is EntryKind.OTHER:
    continue
if entry.kind is EntryKind.DIRECTORY:
    if depth > MAX_TREE_DEPTH:
        raise DiskMeasurementError(_bounded_depth_message(path_parts))
    child = backend.open_directory(current.directory, entry.name, SharePolicy.SCAN)
    if child.identity != entry.identity or child.filesystem != root.filesystem:
        child.close()
        raise DiskMeasurementError("owned scratch directory identity changed while opening")
    stack.append(_MeterFrame(backend.entries_owned(child), next_parts))
    continue
if entry.kind is EntryKind.REGULAR:
    file = backend.open_file(
        current.directory,
        entry.name,
        access=FileAccess.READ,
        disposition=CreateDisposition.OPEN_EXISTING,
        share_policy=SharePolicy.SCAN,
    )
    try:
        if file.identity != entry.identity or file.kind is not EntryKind.REGULAR:
            raise DiskMeasurementError("owned scratch file identity changed while opening")
        identity = file.identity
        size = file.logical_size
    finally:
        file.close()
```

Count each enumerated entry before kind dispatch; check the five-second deadline immediately after every native operation and every 256 entries. Preserve hard-link deduplication by `FileIdentity` and the conservative count for unsupported identities; Windows must reject zero identities before this function.

Wrap each child open so only `FileNotFoundError` from a concurrently vanished enumerated entry continues the scan; sharing, access, malformed-record, identity, filesystem, deadline, and close errors become `DiskMeasurementError`. On unwind, explicitly close every iterator/directory frame, retain the first operational error, and attach bounded close notes instead of silently discarding them.

- [ ] **Step 4: Store root capabilities and revalidate exact paths**

Add this defaulted field to `MeterRoot`:

```python
capability_factory: Callable[[], DirectoryCapability] | None = None
```

Replace `_root_capabilities: list[tuple[MeterRoot, int | None, str | None]]` with a small dataclass holding `DirectoryCapability | None`, open error, retained identity, and optional exact identity. For each configured `root`, call `root.capability_factory()` exactly once when present; otherwise call `backend.open_root(root.path, SharePolicy.SCAN)`. The factory transfers sole ownership of the returned capability to `DiskGuard`. Require `capability.owned_by(backend)`, `capability.is_open`, non-reparse directory kind, and a path hint consistent with `root.path`; on constructor failure close every capability already acquired. Lifecycle code never calls `_resource_for` or observes a native resource.

For `exact_path` on each sample, open a transient root, compare identity, use it for capacity, and close it. A missing path is acceptable only until the first exact identity has been recorded. A replacement or disappearance afterward is `DiskMeasurementError` with the existing enforcement label.

Deduplicate capacity by `FilesystemIdentity`; call `backend.available_bytes` exactly once per unique filesystem per sample.

- [ ] **Step 5: Convert reservation and close handling**

Change the reservation signature to:

```python
def reserve_additional_bytes(
    self,
    additional_bytes: int,
    *,
    filesystem: DirectoryCapability,
) -> DiskFailure | None:
```

Require `filesystem.owned_by(backend)` and `filesystem.is_open` before using `filesystem.filesystem` as the key and `backend.available_bytes(filesystem)` for the live value. Preserve block rounding through the Task 1 `backend.allocation_unit(filesystem)` contract; POSIX returns `f_frsize`, Windows returns `SectorsPerCluster * BytesPerSector` from `GetDiskFreeSpaceW` after the same final-path/identity revalidation. Reject a nonpositive unit.

`probe_close` reopens and closes each root capability; `_close_capabilities` calls capability `close()` and keeps a failed capability in the list so a later cleanup can retry it. Record every close error without short-circuiting.

- [ ] **Step 6: Port existing behavior tests to backend seams**

Keep the current test names and assertions for deadline boundaries, depth, entry count, hard links, capacity deduplication, replacement, disappearance, close failure, and sticky failure precedence. Replace patches of `_measure_fd`, `os.fstatvfs`, and `_DirectoryStream` with `_measure_capability`, `backend.available_bytes`, and recording backend iterators.

The four POSIX-parser tests at lines 443-589 may retain a POSIX-only marker, but add Windows-native equivalents in `test_focused_mutation_windows_filesystem.py` for each corresponding behavior. The generic `AnchoredDiskGuardTests` class must run on Windows without a class-level skip.

Add a recording factory test that returns a fixed-identity directory capability, assert it is called once, replace the reported pathname, and prove samples continue using the retained identity while exact-path policy reports the replacement. Assert `DiskGuard.close()` closes the factory capability exactly once.

- [ ] **Step 7: Run disk tests on the capability implementation**

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_filesystem `
  tests.test_focused_mutation_windows_filesystem `
  tests.test_focused_mutation_disk.DiskPolicyParserTests `
  tests.test_focused_mutation_disk.AnchoredDiskGuardTests -v
```

Expected: PASS on Windows; only tests for genuine POSIX parser layout skip.

- [ ] **Step 8: Commit disk migration**

```powershell
git add `
  tools/focused_mutation_support/filesystem.py `
  tools/focused_mutation_support/posix_filesystem.py `
  tools/focused_mutation_support/windows_filesystem.py `
  tools/focused_mutation_support/disk.py `
  tests/test_focused_mutation_filesystem.py `
  tests/test_focused_mutation_windows_filesystem.py `
  tests/test_focused_mutation_disk.py
git commit -m "refactor: meter disk through filesystem capabilities"
```

---

### Task 7: Migrate managed-root creation, markers, and publication

**Files:**

- Modify: `tools/focused_mutation_support/filesystem.py`
- Modify: `tools/focused_mutation_support/posix_filesystem.py`
- Modify: `tools/focused_mutation_support/windows_filesystem.py`
- Modify: `tools/focused_mutation_support/lease.py:1-1029`
- Modify: `tests/test_focused_mutation_disk.py:1230-2194`

**Interfaces:**

- Consumes: capability backends and `LeaseLock`'s CRT descriptor contract.
- Produces: capability-backed `_ensure_managed_root`, `_open_coordinator`, marker read/write, `ManagedScratch.create(parent, *, run_id=None, stale_cleanup=None, stale_diagnostics=None, backend=None)`, `ManagedScratch.open_child(name, share_policy)`, `ManagedScratch.reopen_for_meter()`, child creation, heartbeat refresh, staging-to-active publication, and exact rollback.

- [ ] **Step 1: Add backend-neutral RED tests for managed security and publication**

Retain existing constructor/publish race tests and add assertions that the managed root is `MANAGED`, the run root was created as a relative `PINNED` target with self-`DELETE`, and all protocol files are verified before publication. Use a recording backend event log:

```python
self.assertEqual(
    backend.events[:8],
    [
        "open-parent:mutation",
        "create-secure-root:hoimin-focused-v1",
        "verify-managed:hoimin-focused-v1",
        "open-or-create:.hoimin-coordinator",
        "verify-managed:.hoimin-coordinator",
        "create-directory:.staging-run-id:pinned",
        "create-new:.hoimin-lease.json",
        "create-new:.hoimin-heartbeat.json",
    ],
)
```

Add a Windows native test that reads both `TOKEN_USER` and `TOKEN_OWNER`, creates the root/markers, and always asserts their owners equal `TOKEN_USER`. When the executing account is an Administrators-group member and the two token SIDs differ, include that fact in the subtest and assert none of the objects use `TOKEN_OWNER`; do not skip or fail the universal owner assertion merely because a developer runs under a standard-user token.

- [ ] **Step 2: Add a backend feature for directory-rename handoff**

Extend the protocol with:

```python
@property
def directory_rename_requires_closed_descendants(self) -> bool:
    return False
```

`PosixFilesystemBackend` returns `False`; `WindowsFilesystemBackend` returns `True`. Lifecycle code branches on this semantic capability, never on `os.name`.

- [ ] **Step 3: Rewrite managed-root and coordinator bootstrap**

`_ensure_managed_root` opens the configured parent as `MUTATION`, calls `create_secure_root(parent, MANAGED_DIRECTORY)`, checks parent/root filesystem identity, verifies managed security, and closes the parent. It returns `(root_path, root_capability)`.

`_open_coordinator` accepts a `DirectoryCapability`; open/create the coordinator with `FileAccess.READ_WRITE`, `OPEN_OR_CREATE`, and `SharePolicy.PINNED`, call `verify_managed_security(coordinator, repair_dacl=True)`, then detach with `os.O_RDWR | getattr(os, "O_BINARY", 0)` and acquire `LeaseLock`. Preserve all current absolute deadline checks around open, lock, initialization, slot validation, flush, unlock, and close. The pinned binary CRT handle prevents coordinator replacement and text-mode translation while slot evidence is live.

Replace `os.pread`/`os.pwrite` with lock-protected portable helpers so Windows works:

```python
def _read_at(fd: int, size: int, offset: int) -> bytes:
    os.lseek(fd, offset, os.SEEK_SET)
    return os.read(fd, size)


def _write_at(fd: int, value: bytes, offset: int) -> int:
    os.lseek(fd, offset, os.SEEK_SET)
    return os.write(fd, value)
```

The coordinator lock serializes these seeks. Retain exact 1,025-byte validation and dual-slot CRC behavior.

- [ ] **Step 4: Introduce exact marker helpers**

Implement these exact helpers:

```python
def _create_marker(
    parent: DirectoryCapability,
    name: str,
    value: dict[str, object],
    backend: FilesystemBackend,
) -> tuple[FileIdentity, int]:
    """Create, verify, detach, durably write, and return identity plus owning fd."""


def _read_marker(
    parent: DirectoryCapability,
    name: str,
    backend: FilesystemBackend,
    *,
    expected_run_id: str,
    expected_lease_id: str | None = None,
    deadline: float | None = None,
) -> tuple[FileIdentity, dict[str, object]] | None:
    """Read one canonical marker; return None only when the entry is absent."""
```

Creation uses `FileAccess.READ_WRITE`, `CREATE_NEW`, and `SharePolicy.PINNED`, records the new identity, calls `verify_managed_security(marker, repair_dacl=False)`, and detaches with `os.O_RDWR | getattr(os, "O_BINARY", 0)`. It then performs bounded zero-progress-checked writes, `fsync`, and `fstat` size verification. If verification, transfer, or writing fails, whichever layer still owns the resource closes it first; after a successful close, rollback uses `backend.open_entry(parent, name, SharePolicy.PINNED)`, requires regular kind plus the recorded identity, consumes it through `delete`, and verifies absence. A close or rollback failure is bounded secondary evidence on the unchanged primary exception; no same-name replacement is deleted. Read uses `FileAccess.READ`, `OPEN_EXISTING`, and `SharePolicy.PINNED`, calls `verify_managed_security(marker, repair_dacl=False)`, detaches with `os.O_RDONLY | getattr(os, "O_BINARY", 0)`, enforces a `MARKER_CAPACITY + 1` bound before UTF-8/JSON decode, verifies schema plus the expected IDs/deadline, and returns both identity and parsed value. Only managed-root/coordinator `OPEN_OR_CREATE` paths pass `repair_dacl=True` after owner verification.

Only the canonical coordinator, lease, heartbeat, retention, and cleanup-ready names call managed-security verification. Unknown payload names are never relabeled.

- [ ] **Step 5: Rewrite staging creation and publication**

Create staging relative to the managed-root capability with `PINNED` before any marker. The returned relative directory handle includes self-`DELETE` but denies delete sharing, so its name cannot move between verification and publication. `_create_marker` returns the lease fd, which is wrapped directly by `LeaseLock`. For heartbeat, close the returned fd after flush and reopen a `FileCapability`; control-only markers close their returned fd immediately. Record all identities/content before publication.

When `directory_rename_requires_closed_descendants` is true:

1. close heartbeat and release/close the lease descriptor while the coordinator stays locked;
2. rename the retained staging capability to active relative to the managed root;
3. call `_read_marker` separately for lease and heartbeat to verify identity, owner/security, and complete content, closing those read capabilities;
4. reopen the lease as `READ_WRITE/PINNED`, compare its identity, detach it, acquire the lease lock, and reread the same bounded bytes through the locked descriptor;
5. reopen heartbeat as `WRITE/PINNED`, compare its identity/security, and retain that capability for `touch`;
6. release coordinator only after all checks pass.

When false, keep existing POSIX marker handles/lock across rename. A failure after Windows rename first attempts the capability-relative rename back to staging, then restores validated marker handles/lock; preserve the primary error and append rollback failures.

- [ ] **Step 6: Rewrite child creation and heartbeat refresh**

`ManagedScratch.create_child` validates/reports the path before calling `create_directory(self._root, name, PINNED)`, verifies identity via `entry`, records that identity under the component, then closes the child capability and returns its path. The temporary pin lets rollback delete only the exact newly-created capability and verify absence; normal later use reopens the registered child with the requested policy.

Add:

```python
def open_child(
    self,
    name: str,
    share_policy: SharePolicy,
) -> DirectoryCapability:
    """Open one registered child relative to the retained run root."""


def reopen_for_meter(self) -> DirectoryCapability:
    """Return a fresh backend-owned duplicate for DiskGuard."""
```

`open_child` validates the component, requires a registry entry, calls `backend.open_directory(self._root, name, share_policy)`, and compares kind, identity, filesystem, and current parent entry with the recorded creation identity before returning. It closes the new capability on any mismatch. `reopen_for_meter` calls `backend.reopen_directory(self._root)` with no policy conversion, preserving the run root's pinned share mode; the duplicate is iterator-only meter ownership and is not used as a namespace-mutation source.

Keep the heartbeat as a `FileCapability` after publication. `refresh_heartbeat` verifies the parent entry still equals the retained identity, calls `backend.touch` followed by `backend.flush`, checks the deadline after each native call, and verifies identity again. A replacement raises without touching the replacement; a flush failure remains a heartbeat failure rather than publishing a fresh in-memory timestamp as durable evidence.

- [ ] **Step 7: Convert `ManagedScratch` ownership and finalization fields**

Replace `_managed_root_fd` and `_root_fd` with `DirectoryCapability`; retain the lease as `LeaseLock` and heartbeat as `FileCapability`. `close_capabilities` attempts lease unlock/close, heartbeat close, root close, and managed-root close in order, collecting every error. `__del__` invokes only these closes and never cleanup or rename.

- [ ] **Step 8: Run bootstrap/publication tests and commit**

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_managed_root_stops_between_filesystem_identity_queries `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_constructor_failure_after_lease_publication_rolls_back_staging `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_constructor_never_replaces_preexisting_empty_active `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_heartbeat_refresh_refuses_replaced_marker `
  tests.test_focused_mutation_windows_filesystem.WindowsSecurityTests -v
git add `
  tools/focused_mutation_support/filesystem.py `
  tools/focused_mutation_support/posix_filesystem.py `
  tools/focused_mutation_support/windows_filesystem.py `
  tools/focused_mutation_support/lease.py `
  tests/test_focused_mutation_disk.py `
  tests/test_focused_mutation_windows_filesystem.py
git commit -m "refactor: publish managed scratch with capabilities"
```

Expected: named tests pass on Windows and POSIX; no production `dir_fd` remains in the migrated creation/publication section.

---

### Task 8: Migrate owner cleanup and child removal

**Files:**

- Modify: `tools/focused_mutation_support/lease.py:1029-2034`
- Modify: `tests/test_focused_mutation_disk.py:3242-4821`

**Interfaces:**

- Consumes: Task 7 `ManagedScratch` capabilities and backend rename/delete.
- Produces: capability-only `remove_child`, active-to-deleting claim, resumable `_remove_payload`, absence verification, and bounded current-owner cleanup.

- [ ] **Step 1: Make existing replacement/limit tests RED against capability seams**

Update tests that patch `_open_directory_at`, `_entry_identity`, `os.scandir`, `os.rename`, `os.unlink`, or `os.rmdir` to patch the corresponding backend methods. Keep their exact behavioral assertions. Add a native Windows case where a same-name replacement is installed before delete; assert cleanup fails/defer-classifies it and preserves the replacement sentinel.

- [ ] **Step 2: Claim active-to-deleting with the retained run capability**

Under the coordinator lock, compare the managed-root entry with `self._root.identity`. On Windows handoff, close descendant marker capabilities/lease, rename `self._root` to `.deleting-<run_id>`, reopen and validate markers, and relock before coordinator release. On POSIX retain the lock across rename.

If `.deleting-<run_id>` already exists, never replace it. Any identity mismatch is `FAILED`, not absence. If the absolute owner deadline expires before the next operation, return `DEFERRED` without starting it.

- [ ] **Step 3: Implement capability-backed bounded DFS**

Represent the cleanup cursor as the same validated component stack. Count the managed-root anchor as the first directory capability. Move the claimed run-root capability into the current iterator rather than retaining a second root wrapper. On descent, open at most one pinned child (the third capability), close the current iterator, and then transfer the child to the next iterator. Reopen a cursor path from the managed root one component at a time, closing each parent after the next pinned child has opened and validating every saved `FileIdentity` and `FilesystemIdentity`. On defer, close the iterator and retain only bounded cursor/identity evidence; resume reopens and revalidates rather than retaining a hidden handle chain. After a directory iterator finishes, close it and reopen that completed directory pinned from its validated parent before deletion. Thus the three-capability limit is exactly managed root, current parent, and just-opened child—never an uncounted claimed-root or iterator duplicate.

For a regular/reparse/other entry, call `backend.open_entry(current_parent, entry.name, SharePolicy.PINNED)`, require its kind and identity to equal the enumeration record, then call `backend.delete(opened)`. For a directory, call `backend.open_directory(current_parent, entry.name, SharePolicy.PINNED)` and descend only after identity/filesystem validation. `open_entry` opens a reparse point itself on Windows and retains an identity-checked parent resource for POSIX `unlinkat`; neither backend follows it. After an empty directory iterator closes, reopen that exact saved identity with `PINNED` and delete it by capability, never by a path discovered from marker data.

Maintain these checks in the loop:

```python
if examined_entries >= MAX_CLEANUP_SLICE_ENTRIES:
    return _deferred_slice("cleanup entry budget exhausted", cursor)
if monotonic() >= slice_deadline:
    return _deferred_slice("cleanup slice deadline exceeded", cursor)
if len(cursor.components) > MAX_CLEANUP_DEPTH:
    return _failed_slice("cleanup depth exceeds 4096", cursor)
if len(cursor.encode()) > MAX_CLEANUP_CURSOR_BYTES:
    return _failed_slice("cleanup cursor exceeds 64 KiB", cursor)
```

Increment examined/removed counts exactly where the current algorithm does. Preserve diagnostic caps and the distinction between a host filesystem timeout before Hoimin's deadline (`FAILED`) and Hoimin's own deadline (`DEFERRED`).

- [ ] **Step 4: Implement child removal through the same walker**

`remove_child` validates that the requested `Path` is exactly one registered child name, opens it relative to the retained run capability with `PINNED`, compares identity, and invokes `_remove_payload`. It never accepts the `Path` as deletion authority. After the child capability closes, verify `entry(root, name) is None` before reporting `CLEAN`.

- [ ] **Step 5: Preserve readonly and close-error classifications**

POSIX deletion relies on parent authority; Windows extended disposition ignores readonly attributes. If either backend reports ACL/sharing/readonly failure, retain it as an operational failure. Always attempt child, parent, root, marker, lease, and coordinator closes, attaching secondary details without overwriting the first failure.

- [ ] **Step 6: Run owner cleanup tests and commit**

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_cleanup_removes_only_the_leased_root `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_cleanup_of_deep_tree_keeps_only_three_directory_handles `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_cleanup_refuses_replacement_root_with_same_managed_name `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_remove_child_refuses_replacement_with_same_child_name `
  tests.test_focused_mutation_disk.ManagedScratchTests.test_owner_cleanup_propagates_one_absolute_deadline_to_all_helpers -v
git add tools/focused_mutation_support/lease.py tests/test_focused_mutation_disk.py
git commit -m "refactor: clean owned scratch through capabilities"
```

Expected: owner cleanup behavior passes natively on Windows and still respects all POSIX limits.

---

### Task 9: Migrate janitor selection, claim, and deferred resume

**Files:**

- Modify: `tools/focused_mutation_support/lease.py:2035-3089`
- Modify: `tests/test_focused_mutation_disk.py:2195-3241`
- Modify: `tests/test_focused_mutation_disk.py:4822-5029`

**Interfaces:**

- Consumes: Task 8 cleanup walker.
- Produces: capability-backed `_read_valid_marker_at`, `_reclaim_empty_unleased_candidate`, `reclaim_abandoned`, `_resume_deferred_cleanup`, and bounded empty/lease-only directory checks.

- [ ] **Step 1: Port marker and selection tests to capabilities and confirm RED**

Keep every current janitor test name. Replace raw descriptor fixtures with backend-created managed roots and protocol markers. Ensure live, retained, malformed, boolean-schema, fresh staging, old staging, cleanup-ready, stale heartbeat, deleting tail, cursor wrap, and 257th-candidate cases remain distinct.

Add a Windows test that holds a live lease CRT lock and confirms a second janitor preserves the root. Release it, publish cleanup-ready, rerun, and confirm only that root is reclaimed.

- [ ] **Step 2: Stream bounded selection from the managed-root capability**

Retain/open the managed root with `MUTATION` because coordinator open/create and cursor persistence mutate a recognized protocol file. Pass one `SCAN` duplicate to `entries_owned` for direct-child selection without materializing more than 256 candidates. Count at most 100,000 direct children and stop at the five-second selection deadline. Preserve cursor ordering and dual-slot persistence; the scan duplicate is closed before any candidate claim.

For each canonical candidate, retain `name`, `FileIdentity`, and parsed run ID. Unknown entries, reparse points, invalid names, and malformed markers produce bounded diagnostics and never become delete targets.

- [ ] **Step 3: Acquire mutation authority before descendant markers**

For each selected candidate, retain the managed root as `MUTATION` but open the relative candidate as `PINNED` before opening lease/heartbeat/retention/cleanup-ready. Compare selection identity with both the candidate handle and current managed-root entry. On Windows, a sharing violation at this pin step means a live owner still retains the run-root pin: preserve the candidate, record the bounded live/busy outcome, and do not open descendant markers. Access denial, malformed identity, and every non-sharing failure remain errors. This ordering pins the candidate name on Windows and is harmless on POSIX.

Open the lease, verify managed security/content/identity, detach to CRT fd, and acquire nonblocking `LeaseLock`. A busy lock preserves the candidate. After lock, reread marker content through the locking descriptor or a backend-safe duplicate and require exact equality.

- [ ] **Step 4: Claim and clean only validated candidates**

Under the coordinator, close descendants as required, rename candidate to `.deleting-<run_id>` through its retained relative `PINNED` capability, reopen/revalidate/relock, then construct `ManagedScratch` from the live capabilities. Use the remaining janitor deadline for one cleanup slice before fair second passes.

When a pre-lease staging or empty deleting tail qualifies, delete only after age, emptiness, identity, and coordinator checks. Empty/lease-only checks stream at most two records and honor the shared deadline.

- [ ] **Step 5: Rewrite deferred resume resource disposal**

Replace the integer resource dictionary with typed optional `DirectoryCapability`/`LeaseLock` fields. `_dispose_deferred_resources` clears a field only after moving its value to a local, then attempts every close. A failed capability close remains retryable until the function returns its bounded diagnostic.

Resume reopens managed root and candidate relative to the retained selection root, compares saved identity/filesystem, revalidates the lease after locking, then calls Task 8's walker. If identity changes, report a diagnostic and do not delete either generation.

- [ ] **Step 6: Run the complete janitor class and commit**

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_disk.ManagedScratchTests -v
git add tools/focused_mutation_support/lease.py tests/test_focused_mutation_disk.py
git commit -m "refactor: reclaim abandoned scratch with capabilities"
```

Expected: all managed-scratch tests pass on Windows. Remaining skips are only fixture-level POSIX primitives with explicit native Windows counterparts.

---

### Task 10: Migrate output ownership, atomic reports, and command spools

**Files:**

- Modify: `tools/focused_mutation_support/store.py:1-861`
- Modify: `tools/focused_mutation_support/runner.py:530-560`
- Modify: `tests/test_focused_mutation_disk.py:5030-5470`
- Modify: `tests/test_focused_mutation_runner.py:780-830`
- Modify: `tests/test_focused_mutation_reporting.py:1650-1785`

**Interfaces:**

- Consumes: `FilesystemBackend`, `DirectoryCapability`, `FileCapability`, and `LeaseLock`.
- Produces: `OwnedOutput.create(path, run_id, *, min_free_bytes=None, backend: FilesystemBackend | None = None)`, `OwnedOutput.reopen_for_meter()`, `RunStore(output, *, command_root=None, command_root_capability=None, backend=None)`, `CommandPaths.root: DirectoryCapability`, capability-relative output recovery/write/delete, and capability-backed command spool reservations.

- [ ] **Step 1: Add RED tests for pinned output and caller ACL preservation**

Keep every existing `OwnedOutputTests` behavior assertion and add native Windows assertions:

```python
@unittest.skipUnless(os.name == "nt", "requires Windows sharing and ACL APIs")
def test_output_root_is_pinned_without_rewriting_caller_acl(self) -> None:
    backend = WindowsFilesystemBackend()
    with tempfile.TemporaryDirectory() as raw:
        output = Path(raw) / "output"
        output.mkdir()
        before = _security_descriptor_bytes_for_tests(output)
        owner = OwnedOutput.create(output, str(uuid.uuid4()), backend=backend)
        try:
            with self.assertRaises(OSError):
                output.rename(Path(raw) / "replacement")
            self.assertEqual(_security_descriptor_bytes_for_tests(output), before)
        finally:
            owner.close(remove_marker=True)
```

Add a platform-neutral recording-backend test that the output-owner marker and every report temporary inherit `SecurityDomain.CALLER` from the output root. Add a separate assertion that command spools inherit `SecurityDomain.MANAGED` from the managed command-root capability, are always `CREATE_NEW`, and never enter an existing-object DACL-repair path.

- [ ] **Step 2: Replace `CommandPaths.root_fd` with a directory capability**

Use this dataclass shape:

```python
@dataclass(frozen=True)
class CommandPaths:
    stdout: Path
    stderr: Path
    root: DirectoryCapability
    stdout_name: str
    stderr_name: str
    backend: FilesystemBackend
    _root_token: int
    _release_root: Callable[[int], None] = field(repr=False, compare=False)
    _spool_identities: dict[str, FileIdentity] = field(
        default_factory=dict, repr=False, compare=False
    )
    capability_errors: list[str] = field(default_factory=list)
```

Import `field` beside `dataclass`, `Callable` from `collections.abc`, and `FileIdentity` from the capability module. The frozen dataclass prevents capability replacement; the private methods may mutate only the bounded two-entry identity map and diagnostic list. `RunStore` assigns a monotonic token to every duplicated command root and keeps a non-owning `set[int]` of live tokens. `_release_root` removes only that token and is idempotent. `CommandPaths.close()` calls it only after the capability close succeeds; a failed close leaves the token registered and retryable. This registry makes teardown ordering testable without giving `RunStore` a second owner of the capability.

`open_writer(stream_name)` accepts only the exact stdout/stderr selectors, creates the corresponding regular file with `FileAccess.WRITE`/`CREATE_NEW` and `SharePolicy.PINNED`, records its `FileIdentity` in `_spool_identities` before return, detaches once with `os.O_WRONLY | getattr(os, "O_BINARY", 0)`, and wraps it with `os.fdopen(descriptor, "wb", closefd=True)`. A duplicate selector or third identity is an internal error. If detach/`fdopen` fails, close whichever layer owns the file, reopen only that name through `open_entry`, delete only the recorded identity, remove the identity entry only after verified absence, and preserve the primary error. `read` requires a recorded identity, reopens with `FileAccess.READ`/`OPEN_EXISTING` and `PINNED`, compares regular kind and identity before detach with `os.O_RDONLY | getattr(os, "O_BINARY", 0)`, and retains the current capacity/tail bounds before decode. `discard` uses `backend.open_entry(root, spool_name, SharePolicy.PINNED)` to obtain self-`DELETE`, requires a regular kind and the recorded identity, and passes that exact capability to consuming `backend.delete`; remove the identity only after its internal post-close check verifies `backend.entry(root, spool_name) is None`.

`CommandPaths` owns its duplicated root capability. Add idempotent `close() -> tuple[str, ...]`; a failed capability close remains retryable and is appended to `capability_errors`, while a successful close calls `_release_root(_root_token)`. `discard()` attempts both spool deletions/absence checks, then `close()` in `finally`. `__del__` may call only `close()` and suppress its diagnostics; it never deletes spools. Every runner exit before materialization, every `_discard_command_spool` path, and command-record teardown must call `discard` or `close`, so `RunStore.close_command_root()` runs only after all per-command duplicates have been released.

`available_bytes` delegates to `backend.available_bytes(root)`. Change runner reservation to:

```python
reservation = disk_guard.reserve_additional_bytes(
    allowance,
    filesystem=paths.root,
)
```

- [ ] **Step 3: Pin and verify the output root**

`OwnedOutput.create` may create missing path components with `Path.mkdir(parents=True, exist_ok=True)` because creation is not a destructive authority. Immediately open the final root through `backend.open_root(path, PINNED, CALLER)`, reject reparse/wrong kind, recover abandoned output relative to that capability, enforce the 1,000-name inventory cap, and query capacity through the retained handle.

Record `directory.identity`. `_verify` reopens the requested path with `PINNED`, compares it with the retained identity/filesystem, closes the verification capability, and also confirms the retained capability remains live. Windows pinning prevents replacement; POSIX identity comparison detects it.

- [ ] **Step 4: Create and lock the output owner marker**

Create `.hoimin-output-owner` with `FileAccess.READ_WRITE`, `CREATE_NEW`, and `SharePolicy.PINNED` in the caller security domain. Record its identity, detach with `os.O_RDWR | getattr(os, "O_BINARY", 0)`, perform the existing bounded/zero-progress write and `fsync`, then acquire `LeaseLock(blocking=False)`. Store the lock plus identity, not a raw marker fd. `_verify` checks both the retained output-root capability and that `backend.entry(output, OUTPUT_OWNER_FILE)` is a regular entry whose identity equals the stored marker identity before every capacity query or report boundary.

Marker schema continues to encode `output_device=directory.identity.volume` and `output_inode=directory.identity.file`. On close, release/close the lock first. If `remove_marker=True`, reopen the marker with `backend.open_entry(output, OUTPUT_OWNER_FILE, SharePolicy.PINNED)` to obtain self-`DELETE`, require regular kind and the stored identity, delete by capability, and verify absence.

- [ ] **Step 5: Make atomic report replacement capability-relative**

Use this exact sequence:

1. verify output identity;
2. create deterministic temporary with `FileAccess.WRITE`/`CREATE_NEW`/`SharePolicy.PINNED` and retain its `FileIdentity`;
3. detach with `os.O_WRONLY | getattr(os, "O_BINARY", 0)`, wrap with `os.fdopen(descriptor, "wb", closefd=True)`, stream through `BoundedTextWriter`, flush, and `fsync`;
4. close the CRT descriptor;
5. verify output and run the post-flush guard;
6. reopen the temporary with `open_entry(output, temporary, PINNED)`, require regular kind and the original identity;
7. call `backend.rename(temp_capability, output, destination, replace=True)`;
8. close the source capability and reverify output/destination identity.

On failure, close whichever resource still owns the temporary, use `backend.open_entry(output, temporary, SharePolicy.PINNED)` on only the deterministic temporary name, delete it only if its kind and identity equal the originally created file, and attach rollback errors to the primary. Never unlink a different same-name replacement.

- [ ] **Step 6: Rewrite abandoned-output recovery**

Stream at most 1,001 names from `backend.entries(output)`. Open the canonical marker, enforce regular kind and 64 KiB cap, detach/lock, parse exact fields, compare output identity, and accept only the two UUID-derived temporary names.

For each accepted `temporary_name`, use `backend.open_entry(output, temporary_name, SharePolicy.PINNED)`, require regular kind, and delete by capability. Release/close the marker lock, reopen the exact marker through `backend.open_entry(output, OUTPUT_OWNER_FILE, SharePolicy.PINNED)`, delete it, and verify all three absences. Preserve foreign neighbors and reject noncanonical UUIDs, boolean schema values, copied markers, and identity changes.

- [ ] **Step 7: Store and close command-root capabilities**

Require `command_root` and `command_root_capability` to be both present or both absent. The workflow obtains the capability with `scratch.open_child("commands", SharePolicy.MUTATION)` and transfers ownership to `RunStore`; `_ensure_command_root` validates `command_root_capability.owned_by(backend)`, `command_root_capability.is_open`, directory kind, filesystem, and path hint against `command_root` without reopening the absolute path. `command_paths` gets a backend duplicate, registers its token before returning, and unregisters/rolls back the duplicate if construction fails. Constructor failure closes the transferred capability. `close_command_root` first checks the live-token registry; while any token remains it returns a bounded "command spool root still active" error and leaves the parent capability open. Once empty, it retries failed parent close and retains all accumulated spool errors. Add tests for successful unregister, failed-close retry, construction rollback, and refusal to close the parent ahead of a live `CommandPaths` owner.

`OwnedOutput.reopen_for_meter()` returns `backend.reopen_directory(self._directory)` so it preserves `PINNED` sharing and transfers the duplicate to `DiskGuard` without reopening the output pathname.

Keep path-based `RunStore` fallback behavior used outside the owned workflow unchanged; only the `OwnedOutput`/managed command-root path is safety-authoritative.

- [ ] **Step 8: Run output, runner, and report-boundary tests**

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_disk.OwnedOutputTests `
  tests.test_focused_mutation_runner.RunnerTests `
  tests.test_focused_mutation_reporting -v
```

Expected: output replacement/recovery/concurrency, spool bounds, report generation guards, and close precedence pass on Windows.

- [ ] **Step 9: Commit output/store migration**

```powershell
git add `
  tools/focused_mutation_support/store.py `
  tools/focused_mutation_support/runner.py `
  tests/test_focused_mutation_disk.py `
  tests/test_focused_mutation_runner.py `
  tests/test_focused_mutation_reporting.py
git commit -m "refactor: anchor Windows output and spools"
```

---

### Task 11: Activate the Windows workflow and document the supported contract

**Files:**

- Modify: `tools/focused_mutation.py:95-112`
- Modify: `tools/focused_mutation.py:275-350`
- Modify: `tools/focused_mutation.py:689-735`
- Modify: `tools/focused_mutation.py:1876-1920`
- Modify: `tests/test_focused_mutation_disk.py:64-184`
- Modify: `tests/test_focused_mutation_reporting.py:335-366`
- Modify: `tests/test_wheel_smoke.py`
- Modify: `docs/development.md:508-515`
- Modify: `tests/test_focused_mutation_docs.py`

**Interfaces:**

- Consumes: Tasks 6-10 and one complete backend.
- Produces: backend injection through `Dependencies`, unconditional supported option parsing, native preflight before launch, a controlled end-to-end Windows workflow test, and updated operator documentation.

- [ ] **Step 1: Inject one backend through workflow setup**

Add a defaulted dependency after the existing optional signal-restorer field:

```python
@dataclass(frozen=True)
class Dependencies:
    monotonic: Callable[[], float]
    utc_now: Callable[[], datetime]
    probe: CommandProbe
    runner: CommandRunner
    restore_signal_handlers: Callable[[], None] | None = None
    filesystem_backend: FilesystemBackend = field(
        default_factory=default_filesystem_backend
    )
```

In `main`, call `backend = default_filesystem_backend()` before `options_from_arguments`, then pass `filesystem_backend=backend` to `Dependencies`. The factory is cached, so `DiskPolicy.__post_init__` and `canonical_scratch_root` use that same default instance during option validation. Pass the dependency instance to `OwnedOutput.create`, `ManagedScratch.create`, `RunStore`, and every `DiskGuard`. Tests can inject recording/failing backends without patching the OS selector.

In `_prepare_runtime`, preserve the existing fake-runner branch but transfer only anchored capabilities:

```python
backend = dependencies.filesystem_backend
scratch = ManagedScratch.create(
    options.disk_policy.scratch_root,
    run_id=run_id,
    stale_cleanup=stale_cleanup,
    stale_diagnostics=stale_cleanup_diagnostics,
    backend=backend,
)
command_root = scratch.create_child("commands")
uses_command_spool = isinstance(dependencies.runner, CommandRunner)
command_root_capability = (
    scratch.open_child("commands", SharePolicy.MUTATION)
    if uses_command_spool
    else None
)
store = RunStore(
    owned_output,
    command_root=command_root if uses_command_spool else None,
    command_root_capability=command_root_capability,
    backend=backend,
)
guard = DiskGuard(
    options.disk_policy,
    [
        MeterRoot(
            scratch.path,
            enforcement="owned:scratch",
            capability_factory=scratch.reopen_for_meter,
        ),
        MeterRoot(
            options.output,
            enforcement="owned:output",
            capability_factory=owned_output.reopen_for_meter,
        ),
        MeterRoot(
            capacity_root,
            charge_owned_bytes=False,
            enforcement="capacity_only:cargo_home",
            exact_path=capacity_exact,
        ),
    ],
    monotonic=dependencies.monotonic,
    heartbeat=scratch.refresh_heartbeat,
    backend=backend,
)
```

The earlier output setup calls `OwnedOutput.create(options.output, run_id, min_free_bytes=options.disk_policy.min_free_bytes, backend=backend)`. If `RunStore` construction fails after accepting `command_root_capability`, its constructor closes that capability before re-raising.

- [ ] **Step 2: Remove the argument-time Windows rejection**

Delete only these lines from `options_from_arguments`:

```python
if os.name == "nt":
    raise ValueError(
        "disk-safe focused mutation requires the Windows native adapter"
    )
```

Do not add a platform warning, environment flag, CI-only bypass, or delayed synthetic success.

At this activation boundary, replace `test_windows_fails_closed_before_disk_safe_runtime_setup` with the platform-neutral parser assertion below. It must not patch `os.name`; only the host-specific scratch-path canonicalization is isolated, while option construction continues through the cached real backend selector:

```python
def test_options_accept_the_selected_native_backend(self) -> None:
    namespace = _parser().parse_args(
        ["--output", "out", "--file", "src/lib.rs"]
    )
    with mock.patch(
        "tools.focused_mutation_support.disk.canonical_scratch_root",
        return_value=Path("C:/scratch") if os.name == "nt" else Path("/scratch"),
    ):
        options = options_from_arguments(namespace, Path.cwd())
    self.assertEqual(options.disk_policy.jobs, 1)
```

- [ ] **Step 3: Preserve fail-before-launch native preflight**

Before any cargo-mutants call, require all four operations to succeed on the injected backend:

1. output root pin, recovery, marker create/flush/lock;
2. managed root/coordinator/run create and lease publication;
3. first owned-byte/free-space sample;
4. bounded janitor selection.

Add a test backend that fails each operation in turn and use `WorkflowRunner.calls` to assert it remains empty. The CLI maps each failure to exit code 2 with the native operation label and bounded path evidence.

- [ ] **Step 4: Add a native Windows full fake-workflow test**

Reuse `workflow_fixture`, which writes controlled cargo-mutants inventory/outcomes without launching cargo-mutants. On Windows run the complete workflow and assert:

```python
@unittest.skipUnless(os.name == "nt", "requires Windows native workflow")
def test_windows_native_fake_workflow_completes_and_cleans_scratch(self) -> None:
    with tempfile.TemporaryDirectory() as raw:
        options, dependencies, runner = workflow_fixture(raw)
        record = run_workflow(options, dependencies)
        self.assertEqual(record.state, RunState.COMPLETED)
        self.assertTrue((options.output / "run.json").is_file())
        self.assertTrue((options.output / "report.md").is_file())
        self.assertTrue(runner.calls)
        self.assertIsNotNone(record.scratch)
        assert record.scratch is not None
        scratch_path = record.scratch.get("path")
        self.assertIsInstance(scratch_path, str)
        assert isinstance(scratch_path, str)
        self.assertFalse(Path(scratch_path).exists())
        self.assertNotIn("native adapter", record.report_error or "")
```

Also keep the same fake workflow test unskipped on POSIX; the native Windows case is additional integration evidence, not the only behavior test.

- [ ] **Step 5: Add wheel/test-discovery assertions for Windows activation**

Update `tests/test_wheel_smoke.py` and CI-contract tests to assert source no longer contains the rejection text, Windows remains in both quality and wheel-smoke matrices, and no disk/lease/output test class is excluded by OS. Do not modify the matrices to conceal a failure.

- [ ] **Step 6: Replace the Windows handoff documentation**

Replace `docs/development.md` lines 508-515 with a supported section stating:

```markdown
### Windows filesystem safety

The focused wrapper uses pinned Win32 directory handles and NT handle-relative
child opens, rename, and delete operations on Windows. Meter handles share
deletion; output roots deny delete sharing while report evidence is live;
publication and cleanup acquire mutation authority before descendant marker
handles. Reparse points, identity or volume changes, unsupported native
identities, network volumes without a handle-derived volume-GUID/capacity
contract, sharing violations, and close failures fail closed before mutation
launch.

Windows uses the same 8 GiB owned-byte default, 10 GiB free-space reserve,
five-second/250,000-entry meter bound, 60-second owner cleanup bound, and
30-second bounded janitor as POSIX. Hoimin never falls back to pathname-based
recursive deletion. Caller output directories keep their inherited ACLs;
Hoimin-managed protocol objects use the current token user plus SYSTEM and
Administrators protected ACL.
```

Change docs tests to require this section and reject `Windows native disk-safety adapter remains unfinished`, `follow-up handoff`, and `fails closed on Windows before mutation setup`.

- [ ] **Step 7: Run activation/documentation tests and commit**

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_disk `
  tests.test_focused_mutation_reporting `
  tests.test_focused_mutation_docs `
  tests.test_wheel_smoke -v
git add `
  tools/focused_mutation.py `
  tests/test_focused_mutation_disk.py `
  tests/test_focused_mutation_reporting.py `
  tests/test_wheel_smoke.py `
  docs/development.md `
  tests/test_focused_mutation_docs.py
git commit -m "feat: enable disk-safe focused mutation on Windows"
```

---

### Task 12: Mutation-test, verify, push, and close PR 396

**Files:**

- Verification-only task: no repository file is modified when all gates pass. If a gate exposes a behavioral gap, return to the owning Task 1-11, add its RED regression there, make a new commit, and restart invalidated evidence.
- External update after same-SHA verification: PR `https://github.com/tokyogas-tech/hoimin/pull/396` body.

**Interfaces:**

- Consumes: all Rust-plan and Python-plan commits.
- Produces: one clean pushed SHA with normal, native Windows, mutation, Rust compatibility, wheel-smoke, and remote CI evidence; PR text no longer claims Windows is unfinished.

- [ ] **Step 1: Run the complete normal Python suite before mutation planning**

```powershell
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uv run --frozen python -m compileall -q tools
uv run --frozen mypy tools/focused_mutation.py tools/focused_mutation_support
uv run --frozen ty check tools/focused_mutation.py tools/focused_mutation_support
```

Expected: every command exits 0. Stop on a normal-test/type failure; do not generate a mutation plan against a failing baseline.

- [ ] **Step 2: Create one immutable mutation plan outside the repository**

Use the pre-Windows-design implementation SHA as the diff base and keep every input fixed for later verifies:

```powershell
$mutationEvidence = Join-Path $env:TEMP ("hoimin-windows-capabilities-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $mutationEvidence | Out-Null
$mutationPlan = Join-Path $mutationEvidence 'PLAN.json'
uv run hoimin plan --root . --source tools --changed --diff-base ece964e214401d69304039c10b1d12e4b25beb82 --profile focused --fingerprint-include pyproject.toml --include 'tests/**' --baseline-timeout 5m --mutant-timeout 2m --total-timeout 30m --max-mutants 10000 --max-candidates 10000 -- python -m unittest tests.test_focused_mutation_filesystem tests.test_focused_mutation_windows_filesystem tests.test_focused_mutation_disk tests.test_focused_mutation_runner tests.test_focused_mutation_reporting | Set-Content -Encoding utf8 $mutationPlan
$planExit = $LASTEXITCODE
if ($planExit -ne 0) { throw "complete mutation planning required; exit was $planExit" }
$parsedPlan = Get-Content -Raw $mutationPlan | ConvertFrom-Json
if ($parsedPlan.truncated -ne $false) { throw 'mutation plan is truncated' }
if (@($parsedPlan.candidates).Count -eq 0) { throw 'mutation plan retained no candidates' }
```

`verify` inherits the command's immutable five-minute baseline, two-minute per-mutant, and 30-minute total limits. Exit 4 is a useful partial manifest for exploratory work but is not completion evidence here; narrow the changed selectors and regenerate a complete plan instead of silently accepting undiscovered candidates. Exit 2 is not usable.

- [ ] **Step 3: Select and verify candidates for the safety-critical changed symbols**

Read `candidates` and select candidates whose `symbol` is one of these exact contracts:

```powershell
$requiredSymbols = @(
  'validate_component',
  '_Capability.close',
  'FileCapability.detach_to_fd',
  '_DirectoryRecordParser.next_record',
  'WindowsFilesystemBackend.rename',
  'WindowsFilesystemBackend.delete',
  '_measure_capability',
  '_remove_payload',
  'reclaim_abandoned',
  'OwnedOutput.write_atomic'
)
$manifest = Get-Content -Raw $mutationPlan | ConvertFrom-Json
$selected = @($manifest.candidates | Where-Object { $requiredSymbols -contains $_.symbol })
if ($selected.Count -eq 0) { throw 'mutation plan retained no safety-critical changed candidates' }
$symbolsWithoutCandidates = @(
  $requiredSymbols | Where-Object { $symbol = $_; -not ($selected | Where-Object symbol -EQ $symbol) }
)
$result = Join-Path $mutationEvidence 'verify-safety-critical.json'
$verifyArgs = @('run', 'hoimin', 'verify', $mutationPlan, '--format', 'json')
foreach ($candidate in $selected) {
  $verifyArgs += @('--candidate', [string]$candidate.id)
}
& uv @verifyArgs | Set-Content -Encoding utf8 $result
$verifyExit = $LASTEXITCODE
$report = Get-Content -Raw $result | ConvertFrom-Json
$expectedIds = @($selected.id | Sort-Object -Unique)
$reportedIds = @($report.mutants | ForEach-Object { $_.candidate.id } | Sort-Object -Unique)
if ($expectedIds.Count -ne $reportedIds.Count -or (Compare-Object $expectedIds $reportedIds)) {
  throw 'mutation verification report does not cover the selected candidate set'
}
if ($verifyExit -eq 1) {
  $survivors = @($report.mutants | Where-Object status -EQ 'survived' | ForEach-Object { $_.candidate.id })
  throw "mutation survivors: $($survivors -join ', ')"
}
if ($verifyExit -ne 0) { throw "mutation verify failed with exit $verifyExit" }
if ($report.summary.complete -ne $true -or $report.summary.counts.survived -ne 0) {
  throw 'mutation verification report is incomplete or contains a survivor'
}
$symbolsWithoutCandidates | Set-Content -Encoding utf8 (Join-Path $mutationEvidence 'symbols-without-candidates.txt')
```

The single repeated-`--candidate` invocation runs one baseline and is bounded by the saved 30-minute total timeout; run it as a yielded process and report progress/status at least once per minute rather than holding one silent tool call. Do not replace it with a per-candidate loop that reruns the baseline and can grow to hours. Review `symbols-without-candidates.txt` against the final source and plan: an entry is acceptable only when that exact function has no emitted mutation candidate, not when a symbol spelling is wrong. For a survivor (exit 1), invoke `hoimin-mutation-improvement`, identify the externally observable contract, add a failing test, regenerate the plan because source/fingerprint changed, and rerun the exact normal and selected verification gates. Never distort production behavior only to kill a mutant. Exit 2, 3, 4, or 130 requires diagnosis rather than test editing.

- [ ] **Step 4: Run all Rust and cross-language local gates on the final SHA**

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.88 check --workspace --all-targets --all-features --locked
cargo test --workspace --all-targets --all-features
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
cargo test -p hoimin-cli --test run_e2e
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
$coreTree = @(cargo tree -p hoimin-core --edges normal --prefix none)
$forbiddenCoreDependency = @($coreTree | Select-String -Pattern '(^| )(tokio|rusqlite|tempfile|windows-sys|libc|hoimin-cli)( |$)')
if ($forbiddenCoreDependency.Count -ne 0) { throw "hoimin-core dependency purity failed: $forbiddenCoreDependency" }
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
git diff --check origin/main...HEAD
```

Install the pinned nightly first with `rustup toolchain install nightly-2026-07-27 --profile minimal` if absent. Expected: all commands exit 0, the dependency scan is empty, and the worktree remains clean.

- [ ] **Step 5: Build and run the same Windows wheel-smoke gate as CI**

```powershell
$repositoryRoot = (Resolve-Path '.').Path
$wheelRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot 'target\wheels'))
if (Test-Path -LiteralPath $wheelRoot) {
  $wheelItem = Get-Item -LiteralPath $wheelRoot -Force
  if (-not [string]::Equals($wheelItem.FullName, $wheelRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to remove unexpected wheel directory: $($wheelItem.FullName)"
  }
  if ($wheelItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
    throw "refusing to remove reparse-point wheel directory: $wheelRoot"
  }
  Remove-Item -LiteralPath $wheelRoot -Recurse -Force
}
uvx maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

Expected: the checked path can only resolve to the repository's exact `target\wheels` directory; build and smoke exit 0 on Windows.

- [ ] **Step 6: Review the complete diff and push the verified branch**

```powershell
git status --short --branch
git diff --stat origin/main...HEAD
git diff --check origin/main...HEAD
git log --oneline origin/feat/disk-safe-mutation..HEAD
git push origin feat/disk-safe-mutation
```

Expected: clean worktree; push advances PR 396 from `ece964e214401d69304039c10b1d12e4b25beb82` to the locally verified SHA.

- [ ] **Step 7: Monitor every PR check to a terminal green state**

```powershell
gh pr checks 396
gh pr view 396 --json headRefOid,mergeStateStatus,statusCheckRollup
```

Run these as short, separate polling calls and send a user-visible status update at least once per minute while checks are pending; do not use one unbounded `--watch` process. Record a 45-minute polling deadline. If a check is still queued/running at that deadline, inspect its workflow/run state and runner availability instead of silently waiting for hours; a genuinely active check may continue under renewed short polls after that diagnosis. Expected: the remote head equals the verified local SHA and every required check, including Windows Rust, Windows quality, and Windows wheel-smoke, is successful. If CI exposes a real defect, add a local RED regression, fix it through TDD, invalidate old evidence, repeat affected mutation/normal gates on the new SHA, push, and poll again. Do not update the PR verification note while any check is pending or failed.

- [ ] **Step 8: Replace the exact stale PR platform note without losing other text**

Read the current body, require the old two bullets to exist, replace only those bullets, and write the full body back:

```powershell
$pr = gh pr view 396 --json body,headRefOid | ConvertFrom-Json
$localSha = (git rev-parse HEAD).Trim()
if ($pr.headRefOid -ne $localSha) { throw "PR head changed after verification: $($pr.headRefOid)" }
$old = @'
- verified on macOS; Windows-native behavior remains the documented follow-up handoff
- Rust mutation testing, cargo-mutants, and the focused mutation wrapper were intentionally not run
'@
$new = @'
- verified with native Windows handle-relative metering, lease/publication, output, cleanup, and controlled focused-wrapper coverage; Linux and macOS compatibility gates remain green
- Rust was covered by normal/MSRV/contracts gates; changed Python safety contracts were checked with manifest-driven focused mutation verification
'@
if (-not $pr.body.Contains($old.Trim())) { throw 'PR platform note changed; refusing broad body rewrite' }
$updated = $pr.body.Replace($old.Trim(), $new.Trim())
$prBodyFile = Join-Path $mutationEvidence 'pr-body.md'
Set-Content -Encoding utf8 -Path $prBodyFile -Value $updated
gh pr edit 396 --body-file $prBodyFile
$confirmed = gh pr view 396 --json body,headRefOid | ConvertFrom-Json
if ($confirmed.headRefOid -ne $localSha) { throw 'PR head changed while updating the body' }
if ($confirmed.body.Contains('follow-up handoff') -or $confirmed.body.Contains('focused mutation wrapper were intentionally not run')) {
  throw 'stale PR platform note remains after update'
}
```

The final reread is part of the command block; do not report completion without both head and body assertions.

---

## Spec Coverage Audit

| Approved requirement | Owning task and executable evidence |
| --- | --- |
| Explicit Windows token-user owner in Rust | Rust plan Tasks 1-3; descriptor, relative-create, lifecycle-owner, mismatch-order, and mechanical create-site tests |
| Shared capability values, state, move, close, and CRT ownership | Python Task 1; value/validator/state-transition tests |
| POSIX behavior retained behind the contract | Python Task 2; backend conformance plus Linux/macOS primitive and race tests |
| Windows root/relative opens, name validation, 128-bit identity, and error typing | Python Task 3; native open/replacement/component/transfer tests |
| Streaming enumeration, 129-frame meter bound, final paths, and capacity identity | Python Task 4; parser/native enumeration/capacity/share tests |
| Exact managed ACLs, pinned-source rename/delete, touch, and flush | Python Task 5; native security/policy/race/disposition/heartbeat tests |
| Metering, hard-link deduplication, capacity deduplication, and reservations | Python Task 6; backend-neutral guard tests plus native capacity test |
| Managed-root bootstrap, marker durability, publication handoff, and anchored child registry | Python Task 7; event-order, owner, rollback, publication, and heartbeat tests |
| Three-capability bounded owner cleanup and exact child removal | Python Task 8; replacement/depth/deadline/close-classification tests |
| Bounded janitor selection, live-lock preservation, claim, and deferred resume | Python Task 9; complete managed-scratch class plus native live-lease test |
| Caller-owned output ACLs, atomic reports, recovery, and command spools | Python Task 10; output/runner/report tests with separate caller/managed-domain assertions |
| Windows activation only after native preflight | Python Task 11; fail-before-launch matrix and full fake workflow |
| Documentation and wheel/CI matrix contracts | Python Task 11; docs and wheel-smoke tests |
| Normal, mutation, MSRV, contracts, randomized, purity, wheel, and remote CI gates | Python Task 12; immutable bounded mutation batch and same-SHA delivery evidence |
| PR 396 no longer claims Windows is unfinished | Python Task 12 Step 8; exact two-bullet replacement with before/after body and head-SHA assertions |

The implementation is not complete if any row loses its named RED test, bound,
or same-SHA verification. A failure found in Task 12 returns to the owning row;
Task 12 itself does not absorb production fixes.
