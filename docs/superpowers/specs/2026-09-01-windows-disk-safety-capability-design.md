# Windows disk-safety capability design

**Status:** Approved in chat on 2026-09-01

**Parent design:** `docs/superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md`

## Purpose

Complete the Windows implementation of disk-safe mutation execution without
weakening the parent design, excluding Windows tests, or replacing anchored
filesystem operations with path-based best effort.

The current branch has two independent Windows failures:

1. Rust creates a protected DACL without an explicit owner. An administrator
   token may therefore assign `BUILTIN\\Administrators` as the default owner,
   while the runtime immediately requires the owner to equal the token user.
2. The Python focused workflow represents a directory capability as a POSIX
   directory file descriptor and calls `dir_fd`, `fstatvfs`, and descriptor
   directory enumeration APIs that Python does not provide on Windows.

The first failure needs an owner-contract correction. The second needs a native
filesystem capability implementation. A test skip, a Windows CI exclusion, or
a path-only fallback is not a completion.

## Goals

- Give Rust-managed files and directories the current token user as their
  explicit owner on Windows while retaining the protected user/SYSTEM/
  Administrators DACL.
- Run the focused mutation workflow on Windows with the same lifecycle,
  metering, output, lease, cleanup, and bounded-work contracts as POSIX.
- Keep security decisions and lifecycle state transitions platform-neutral.
- Confine operating-system differences to one capability interface and its
  POSIX and Windows backends.
- Preserve the existing depth, entry, elapsed-time, byte, diagnostic, and open
  capability bounds.
- Keep all native handles owned, non-inheritable, and closed exactly once.
- Make namespace races, reparse points, identity changes, unsupported native
  operations, and close failures observable and fail closed.

## Non-goals

- Do not add a hard aggregate quota backend. Windows continues to use the
  portable owned-byte and free-space guard.
- Do not support arbitrary caller-selected deletion roots.
- Do not make a distinct same-credential process a security boundary.
- Do not restructure the mutation state machine or change report schemas.
- Do not rewrite working POSIX algorithms merely to make the native API look
  identical internally.
- Do not add recursive deletion to a destructor or finalizer.

## Rejected shortcuts

The implementation must not use any of these approaches:

- skipping Windows disk, lease, output, or cleanup tests;
- weakening an assertion only on Windows;
- returning a synthetic successful measurement on Windows;
- using `Path.resolve()` or a before/after `stat()` pair as the authority for a
  destructive operation;
- opening a child through an unpinned pathname after validating its parent;
- falling back from a failed native relative operation to `shutil.rmtree`,
  `Path.unlink`, `Path.rename`, or another unanchored path operation;
- scattering new `os.name == "nt"` branches through lifecycle code;
- repairing an owner mismatch by taking ownership of a pre-existing object;
- retaining both an owning native handle and an owning CRT descriptor for the
  same kernel handle;
- treating an unsupported or failed native API as a clean or absent result.

## Architecture

### Shared capability contract

Add `tools/focused_mutation_support/filesystem.py`. It defines the data and
ownership contracts used by `disk.py`, `lease.py`, and `store.py`:

```python
@dataclass(frozen=True)
class FileIdentity:
    volume: int
    file: int

@dataclass(frozen=True)
class FilesystemIdentity:
    volume: int
    discriminator_a: int = 0
    discriminator_b: int = 0

class EntryKind(StrEnum):
    DIRECTORY = "directory"
    REGULAR = "regular"
    REPARSE = "reparse"
    OTHER = "other"

@dataclass(frozen=True)
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

The module also defines the owning `FileCapability` and
`DirectoryCapability` classes plus the structural `FilesystemBackend`
protocol detailed below.

`FileCapability` and `DirectoryCapability` are owning context managers. Their
`close()` method is idempotent in state, reports a failed close without
discarding ownership, and marks the capability closed only after the native
close succeeds. They expose no public raw handle. A regular file may transfer
its ownership once to a CRT descriptor for `LeaseLock` or buffered output; the
capability becomes detached at that point. `entries_owned` moves a directory's
resource into a fresh iterator-owned capability and marks the caller's source
capability transferred, so an accidentally retained Python reference cannot
operate on or close the resource. `is_open` is true only in the owning open
state, and `owned_by(backend)` exposes backend identity without exposing the
native resource.

Every capability also retains the native create result as `created: bool`.
`OPEN_OR_CREATE` callers use that result to distinguish a descriptor applied at
creation from verification of an already-existing object; pathname existence
checks are never used as a substitute.

On Windows, the transfer method calls `msvcrt.open_osfhandle` while the
capability is still the sole owner. A failed conversion leaves the capability
owning and closable. A successful conversion atomically marks it detached; the
returned CRT descriptor is then the sole owner. If wrapping that descriptor in
a Python file object fails, the descriptor is closed by the caller-side helper.

The backend operations are:

```python
open_root(
    path: Path,
    share_policy: SharePolicy,
    security_domain: SecurityDomain = SecurityDomain.CALLER,
) -> DirectoryCapability
create_secure_root(
    parent: DirectoryCapability, name: str
) -> DirectoryCapability
reopen_directory(
    directory: DirectoryCapability,
    share_policy: SharePolicy | None = None,
) -> DirectoryCapability
open_directory(
    parent: DirectoryCapability, name: str, share_policy: SharePolicy
) -> DirectoryCapability
create_directory(
    parent: DirectoryCapability, name: str, share_policy: SharePolicy
) -> DirectoryCapability
open_file(
    parent: DirectoryCapability,
    name: str,
    *,
    access: FileAccess,
    disposition: CreateDisposition,
    share_policy: SharePolicy = SharePolicy.MUTATION,
) -> FileCapability
open_entry(
    parent: DirectoryCapability,
    name: str,
    share_policy: SharePolicy,
) -> FileCapability | DirectoryCapability
entry(parent: DirectoryCapability, name: str) -> DirectoryEntry | None
entries(parent: DirectoryCapability) -> DirectoryIterator
entries_owned(parent: DirectoryCapability) -> DirectoryIterator
rename(
    source: FileCapability | DirectoryCapability,
    destination_parent: DirectoryCapability,
    destination_name: str,
    *,
    replace: bool,
) -> None
delete(capability: FileCapability | DirectoryCapability) -> None
available_bytes(directory: DirectoryCapability) -> int
allocation_unit(directory: DirectoryCapability) -> int
touch(file: FileCapability) -> None
flush(file: FileCapability) -> None
final_path(directory: DirectoryCapability) -> Path
verify_managed_security(
    capability: FileCapability | DirectoryCapability,
    *,
    repair_dacl: bool,
) -> None
```

Every child name is one non-empty component, is not `.` or `..`, and contains
no NUL or separator active on the host (`os.sep` and non-null `os.altsep`). The
interface rejects invalid components before entering a native API. A backslash
therefore remains a valid POSIX payload character.

The Windows encoder additionally rejects both `/` and `\`, colon/alternate-
stream syntax, control characters, Win32-forbidden punctuation, trailing dot
or space, DOS device basenames, unpaired UTF-16 surrogates, and names whose
encoded byte length cannot fit `UNICODE_STRING.Length`. This stricter rule
stays in the Windows backend so arbitrary POSIX payload names do not become a
cross-platform regression.

`open_root` opens and pins an existing final directory. Code that may create a
managed root first opens its existing parent and passes exactly one validated
component to `create_secure_root`; root creation is therefore anchored too.
`reopen_directory` duplicates the already-open directory object and does not
smuggle `.` through the child-name interface.

`open_entry` opens the named entry itself without following it and returns the
capability kind reported by the opened object. Cleanup uses it for regular,
reparse, and other entries so deletion always acts on an opened object rather
than a pathname. Backends reject identity changes between `entry` and
`open_entry`; they do not reinterpret an unsupported object as absent.

`rename` and `delete` accept only a relative `PINNED` source capability. On
Windows that source has self-`DELETE` authority but denies delete sharing, so
the validated object cannot move before the native namespace operation.
`delete` is consuming: it retains parent/name evidence locally, unlinks or sets
disposition on that exact opened object, closes the capability, and only then
verifies absence relative to the live parent. This ordering is required on
Windows because disposition takes effect at handle close. A failed close
leaves the capability retryable and the delete fails; a same-name replacement
found during absence verification is reported but is never opened or deleted
as a fallback target.

Each directory capability records a `SecurityDomain`. `create_secure_root`
returns `MANAGED`; reopen and relative child opens preserve the parent's domain.
Creates below `MANAGED` use the secure descriptor and verify it. Creates below
`CALLER` retain caller ACL inheritance on Windows. POSIX creation keeps the
existing explicit private modes (`0700` directories and `0600` files) in both
domains; opening a caller root never chmods it. Output and capacity-only roots
are `CALLER`, so capability pinning cannot accidentally become ACL ownership.

`DirectoryIterator` is streaming and exposes its owned directory as a borrowed
`directory` property for relative child opens. `entries` first reopens the
supplied directory; `entries_owned` instead moves the supplied capability's
resource into the iterator and invalidates the source wrapper. The iterator
returns at most one decoded record at a time and closes its buffer and
capability on normal exhaustion, failure, or explicit close. DFS uses
`entries_owned`, so each stack frame accounts for exactly one directory
capability rather than a directory plus a duplicate iterator handle.

### POSIX backend

The POSIX backend delegates to the existing `openat2`/`openat`, `fstatat`,
`fdopendir`/`readdir`, `renameat`, `unlinkat`, `fstatvfs`, and descriptor lock
behavior. Existing Linux and macOS semantics remain unchanged.

The refactor moves existing low-level operations behind the capability
contract. It does not replace Linux `openat2` resolution flags or macOS
filesystem identity handling.

### Windows backend

Add `tools/focused_mutation_support/windows_filesystem.py`. It is the only new
module that calls Windows filesystem and security APIs.

It uses:

- `CreateFileW` for an initial absolute root open;
- `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory` for child opens and
  creates;
- `GetFileInformationByHandleEx(FileIdInfo)` for 128-bit file identity and
  `GetFileInformationByHandle` for basic attributes;
- `GetVolumeInformationByHandleW` for filesystem identity;
- `GetFileInformationByHandleEx` with `FileIdExtdDirectoryInfo` and
  `FileIdExtdDirectoryRestartInfo` for streaming 128-bit-ID enumeration;
- `GetFinalPathNameByHandleW` plus volume identity revalidation before and
  after `GetDiskFreeSpaceExW`;
- `NtSetInformationFile` with a destination root handle for rename;
- `SetFileInformationByHandle` disposition information for deletion;
- `FlushFileBuffers` for durable regular-file boundaries;
- token and security-descriptor APIs for explicit ownership and protected
  DACL verification.

All handles are opened non-inheritable and with
`FILE_FLAG_OPEN_REPARSE_POINT`. Directories additionally use
`FILE_FLAG_BACKUP_SEMANTICS` or the corresponding native create options.

The share policies mean:

| Policy | Requested authority | Share mode | Use |
| --- | --- | --- | --- |
| `SCAN` | enumerate/traverse/read attributes/synchronize | read, write, delete | meter and non-destructive inspection |
| `PINNED` | absolute root and typed file: no self-`DELETE`; relative directory and deletion-only `open_entry`: self-`DELETE` | read, write | output/marker pinning and exact relative rename/delete targets whose names must not move during validation and mutation |
| `MUTATION` | child mutation plus self-`DELETE` and write attributes | read, write, delete | shareable managed/destination parents and command roots |

Windows rename and delete require a relative `PINNED` source capability with
native self-`DELETE` authority. Relative pinned directories receive that right;
files are reopened through deletion-only `open_entry`, because ordinary pinned
typed-file handles for coordinator/lease/heartbeat must remain concurrently
openable for locking and reads. Omission of `FILE_SHARE_DELETE` prevents another
handle from moving or deleting the source between identity validation and the
native namespace call. `MUTATION` remains delete-sharing so separate Hoimin
processes can concurrently retain the managed parent; it is not sufficient as
the source proof for rename/delete.

Typed root, directory, and regular-file opens reject a reparse point, the wrong
kind, an unsupported or zero identity, a root-filesystem crossing, or an
identity change between enumeration and open. The deletion-only `open_entry`
operation may instead return a no-follow `REPARSE` capability so cleanup can
delete that exact opened object; no caller may traverse or read it as a normal
file. A vanished child is returned only as absence; access denial, malformed
native data, and other errors stay errors.

The Windows directory iterator validates every variable-length record,
including record offset, name-byte alignment, name length, buffer bounds, and
forward progress. `ERROR_NO_MORE_FILES` is the only successful end condition.

### Secure ownership and ACLs

Rust and Python use the same Windows descriptor semantics:

```text
O:<current-token-user-SID>
D:P
  (A;OICI;FA;;;<current-token-user-SID>)
  (A;OICI;FA;;;SY)
  (A;OICI;FA;;;BA)
```

Files omit directory inheritance flags. Named synchronization objects keep
their existing DACL; they do not participate in filesystem owner validation.

The implementation reads `TOKEN_USER`, places that SID in the explicit owner
field, and verifies both owner and the exact protected DACL. It does not compare
the filesystem owner with `TOKEN_OWNER`, whose administrator default may be
`BUILTIN\\Administrators`.

Every managed filesystem object receives this descriptor at creation, not only
the managed root. Rust passes it through `SECURITY_ATTRIBUTES` or
`OBJECT_ATTRIBUTES.SecurityDescriptor` for the root, coordinator file, and all
relative child creates. Python does the same for a secure root and every file
or directory created through a `MANAGED` capability. Caller-owned output files
and capacity-only roots stay in the `CALLER` domain and use their existing ACL
inheritance. For `OPEN_OR_CREATE`, the native creation result (`FILE_CREATED`
versus `FILE_OPENED`) is retained so the security descriptor is treated as a
creation default, never as authority to rewrite an existing owner.

A newly created object remains unpublished and reachable only through its
owned handle until owner and DACL verification succeeds. If a native API in
scope cannot accept a creation descriptor, the implementation may secure that
exact newly-created handle with `WRITE_OWNER`/`WRITE_DAC` before publication;
failure triggers bounded rollback through the same handle.

A pre-existing object follows a different rule: verify that its owner already
equals `TOKEN_USER` before any DACL repair, and fail closed on an owner mismatch.
The implementation never takes ownership of a pre-existing root, coordinator,
marker, lease, temporary, or child object. DACL repair is limited to an
owner-verified direct managed root or a protocol-recognized object reached from
that root. Arbitrary ancestors, unrecognized descendants, and caller-provided
output directories are never re-ACL'd.

Only recognized managed protocol objects call `verify_managed_security`: the
managed root, coordinator, lease, heartbeat, retention, cleanup-ready marker,
and Hoimin run directories. Output-owner markers and report temporaries remain
below the `CALLER` output root. Newly created command spools inherit the
`MANAGED` command-root creation descriptor, but an existing spool is never
accepted or DACL-repaired as a protocol object. Worker payload opened during
cleanup is never relabeled or repaired merely because Hoimin has opened it.

## Integration by subsystem

### Disk metering

`DiskGuard` retains one `DirectoryCapability` per configured root. A
`MeterRoot` may provide a zero-argument capability factory when the owner
already has an anchored root; `ManagedScratch` and `OwnedOutput` use that path
so metering cannot reopen their names during a replacement window. Capacity-
only roots without an existing owner still use `open_root`. `DiskGuard` calls
each factory once and solely owns the returned capability. Each sample reopens
an enumeration capability relative to that retained root and performs the
existing streaming DFS.

Directory and regular-file children are opened relative to the live parent.
The opened identity must equal the enumerated identity before the entry is
used. Hard-link deduplication uses `(volume, file)` across all owned roots.

Free-space measurement is tied to the retained root. Windows obtains a volume-
GUID final path from the handle, compares path-reported volume information with
the handle identity, queries free bytes, then repeats both handle and path-
volume checks. It fails closed when no volume-GUID path is available. A path
disappearance or replacement for an `exact_path` root remains a typed
measurement failure.

Reservation rounding uses `allocation_unit` on the same retained capability.
POSIX obtains `f_frsize` from `fstatvfs`; Windows derives the volume root from
the handle's GUID path, obtains sectors per cluster and bytes per sector there,
and repeats the same object/path-volume identity checks before returning their
positive product.

The existing limits remain exact:

- depth: 128;
- entries: 250,000;
- elapsed scan time: five seconds;
- active DFS-frame directory capabilities: at most 129 for metering, with the
  existing one retained capability per configured `MeterRoot` accounted
  separately;
- hard-link identities: at most the entry limit.

### Managed scratch, leases, and publication

Managed-root creation uses the secure descriptor above. Coordinator, lease,
heartbeat, retention, and cleanup-ready markers are regular-file capabilities
opened relative to the managed or run root.

Long-lived lease, heartbeat, and output-owner marker handles use `PINNED`
sharing, so another process cannot remove or rename the marker while its lock
or identity evidence is live. Windows publication/claim closes those marker
handles before renaming the containing directory, then reopens and revalidates
them before publishing the new state.

On Windows, staging creation acquires a relative `PINNED` capability before any
descendant marker is published. Marker capabilities close before publication.
The retained pinned capability renames staging to active relative to the
live managed-root capability. Markers are reopened, identity/content verified,
and the lease is reacquired before the coordinator is released.

The same handoff applies from active to deleting. Failure to reacquire or
revalidate leaves a hard cleanup failure with bounded secondary close details;
it never publishes quiescence or a clean result.

### Bounded cleanup and janitor

Cleanup walks through live directory capabilities. It never accepts a delete
path from a marker or report. It opens the candidate as a relative `PINNED`
target with `DELETE` authority before opening descendants. The bounded walker
moves the pin to the current directory, reopens cursor components with identity
checks, and reacquires an exact pinned capability before each directory delete;
no namespace mutation occurs from cursor text alone.

Files and empty directories are deleted by handle. Cursor reopen validates
every saved identity and filesystem boundary. The existing cleanup limits
remain exact:

- 50,000 examined entries per slice;
- five seconds per slice;
- 60 seconds for current-owner cleanup;
- 30 seconds for startup janitor work;
- 4,096 path components and 64 KiB encoded cursor;
- at most root, current parent, and current child directory capabilities;
- 256 retained diagnostics of at most 4 KiB each.

Readonly, ACL, identity, reparse, deadline, and close failures keep their
existing `failed` versus `deferred` classification. Windows does not convert a
sharing violation into absence.

### Output and command spools

`OwnedOutput` opens the output root with `PINNED`, which prevents another
process from renaming or deleting the root while path evidence is live. All
children are still created and opened relative to the capability.

Opening the output root does not alter any ancestor or the root's ACL. The
final opened object is rejected if it is a reparse point or not a directory,
and its identity is retained for every later capacity and child operation. Its
`CALLER` security domain propagates to the owner marker and report temporaries,
which preserve normal ACL inheritance while remaining anchored. Command spools
live below the managed command-root capability instead; they get the managed
descriptor only on exclusive creation and are never repaired in place as
trusted pre-existing protocol files.

The owner marker is created exclusively, flushed, locked, and identity
verified. Atomic report writing creates one deterministic temporary relative
to the output capability, flushes it, performs the post-flush guard, then
renames it relative to the same output capability with replacement enabled.

Command stdout and stderr spools use the same relative file capability API.
Reads remain bounded before decoding, and discard verifies absence relative to
the command-root capability.

The command root is not reopened from the `Path` returned by child creation.
`ManagedScratch.open_child(name, policy)` opens it relative to the retained run
root and compares the registered creation identity; `RunStore` takes ownership
of that capability while retaining the path only for reporting and subprocess
arguments. Constructor failure closes the transferred capability.

Abandoned-output recovery accepts only the canonical marker and its two
marker-derived temporary names. It locks and validates the marker before
deleting either temporary by handle.

### Focused workflow activation

The unconditional Windows rejection in `options_from_arguments` is removed
only after these native preflight operations are available:

- secure managed-root open/create and verification;
- output-root pinning and marker lock;
- initial owned-byte and free-space measurement;
- lease publication and bounded janitor selection.

Failure in any preflight operation returns exit code 2 before cargo-mutants or
a mutation test command is launched.

## Error and ownership contract

Native API failures are converted to `OSError` with the Windows error code,
operation label, and a bounded logical path or component. NTSTATUS values are
translated with `RtlNtStatusToDosError`.

The first operational failure remains primary. Unlock, flush, close, rollback,
and absence-verification failures are appended as bounded secondary details.
A close failure does not silently mark the capability closed; later cleanup
may retry it. A detached capability cannot be used or detached twice.

Constructors either return a fully owned, verified capability graph or perform
bounded rollback. Rollback uses only already-owned capabilities and exact
child names. It never discovers a new deletion target from a pathname.

Finalizers may close a handle. They may not rename, walk, or delete a tree.

## Testing

### Rust red-green tests

- The managed security descriptor contains an explicit current-user owner.
- A native Windows managed root created by an administrator-group user verifies
  as owned by the token user.
- Coordinator files and relative managed children created under the same token
  also verify as owned by the token user; the fix is not root-only.
- An owner-mismatched pre-existing root or child is rejected without changing
  its owner or DACL.
- The protected DACL still contains only current user, SYSTEM, and
  Administrators full-control entries.

### Python native primitive tests

- Root and relative child opens retain identity across pathname replacement.
- Reparse-point roots and typed children are rejected without following them;
  deletion-only `open_entry` returns the no-follow object for exact deletion.
- Directory enumeration is streaming, bounded, and rejects malformed records.
- Relative create is exclusive, rename is destination-root anchored, and
  deletion affects only the opened object.
- `SCAN` permits concurrent candidate deletion while `PINNED` blocks output-root
  rename/delete.
- Handle-to-CRT transfer closes exactly once and preserves close failures.
- Capacity queries reject final-path or volume-identity changes.
- Secure-root and managed-child creation and verification use the token user as
  owner, while an owner-mismatched pre-existing object is left unchanged.
- Security-domain propagation applies managed ACLs only below a managed root;
  opening or writing an output root does not rewrite caller ACLs.

### Integrated Windows tests

- Metering counts regular files, deduplicates hard links, and preserves depth,
  entry, descriptor, and deadline bounds.
- A swapped directory, junction, symlink, or same-name replacement cannot
  redirect metering or cleanup outside the owned root.
- Lease publication closes descendants, renames by retained capability,
  reopens and revalidates markers, and reacquires the lock.
- Live, abandoned, retained, malformed, cleanup-ready, staging, and deleting
  roots have the same outcomes as the POSIX contract.
- Owner cleanup and janitor cleanup remove only the validated leased root and
  resume within existing budgets.
- Output ownership, deterministic-temporary recovery, atomic replacement,
  command spool bounds, and concurrent initialization behave natively.
- The focused workflow completes a controlled fake cargo-mutants run on
  Windows instead of failing at argument parsing.

Existing cross-platform tests continue to run on Windows. Platform-specific
tests may express genuinely different primitives, but no disk-safety behavior
test is skipped merely because the platform is Windows.

### Gates

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features`
- full frozen Python `unittest` discovery on Windows, Linux, and macOS
- focused Python mutation verification for changed production symbols
- Rust MSRV, randomized-order, contracts, wheel-smoke, and dependency-purity
  jobs
- native Windows Rust and Python integration tests

## Documentation

After native gates pass, replace the Windows handoff section in
`docs/development.md` with supported behavior, limits, and failure semantics.
The PR platform note must no longer claim that Windows native behavior is an
unfinished follow-up.

## Acceptance criteria

The Windows completion is done only when all of these are true:

1. Rust explicitly assigns and verifies the current token user as owner for the
   root, coordinator, and relative children without taking ownership of any
   pre-existing mismatched object.
2. Python lifecycle code depends on the shared capability contract rather than
   new scattered Windows branches.
3. Meter, lease, marker, output, rename, cleanup, and janitor operations are
   anchored to live native capabilities.
4. No destructive path fallback exists.
5. Reparse, replacement, identity, volume, deadline, and close failures fail
   closed with bounded evidence.
6. Existing safety limits retain their exact values and meanings.
7. The Windows focused workflow passes a controlled end-to-end run.
8. Full Windows Rust and Python CI passes without disk-safety skips or workflow
   exclusions.
9. Linux and macOS gates remain green.
10. Documentation describes Windows as supported and removes the handoff.
