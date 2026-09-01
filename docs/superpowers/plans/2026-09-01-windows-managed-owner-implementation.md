# Windows managed-object owner implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every Rust-created Windows managed root, coordinator, run directory, managed child, and protocol marker explicitly owned by the current token user without taking ownership of any pre-existing object.

**Architecture:** Keep the existing protected DACL and native handle-relative lifecycle, but add `O:<TOKEN_USER>` to its security descriptor and use that descriptor in exclusive relative creates. Existing objects continue through the verify-owner-before-DACL-repair path, so an ownership mismatch is terminal and never repaired.

**Tech Stack:** Rust 1.88+, `windows-sys` 0.60, `cap-std` 4.0.2, `cap-fs-ext` 4.0.2, `fs2` 0.4.3.

**Spec:** `docs/superpowers/specs/2026-09-01-windows-disk-safety-capability-design.md`

## Global Constraints

- Work only in `.worktrees/disk-safe-mutation` on `feat/disk-safe-mutation`; keep `origin/feat/disk-safe-mutation` as the upstream.
- Use `superpowers:test-driven-development` for every behavior change and `superpowers:verification-before-completion` before any completion or push claim.
- The managed descriptor is exactly `O:<TOKEN_USER>D:P` followed by current-user, SYSTEM, and Administrators full-control ACEs. Directories use `OICI`; regular files do not.
- Named mutation-barrier objects retain their current DACL-only descriptor and are outside filesystem-owner validation.
- `FILE_CREATE` is mandatory for relative protocol-object creation. A create must never open and rewrite a pre-existing object.
- A pre-existing root or coordinator must pass `TOKEN_USER` owner verification before any DACL repair. No code path may call `SetSecurityInfo` with `OWNER_SECURITY_INFORMATION` for such an object.
- Hoimin-created protocol objects receive the explicit descriptor. Arbitrary worker payload remains protected by the managed-root DACL but is not relabeled or treated as a protocol object.
- Preserve all current deadline checks, identity comparisons, no-follow checks, share modes, rollback bounds, and close-error precedence.
- Do not add a dependency or a Windows-only test skip to make CI pass.

---

### Task 1: Put the token user in the managed security descriptor

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/owned/windows.rs:134-207`
- Test: `crates/hoimin-cli/src/workspace/owned/windows.rs:482-508`

**Interfaces:**

- Consumes: `UserToken::sid_string()` and the existing object-kind-specific DACL.
- Produces: `SecurityDescriptor::for_user(&UserToken, directory)` whose owner and DACL both name the token user; Tasks 2 and 3 pass this descriptor to native creates.

- [ ] **Step 1: Change the SDDL unit expectations and confirm RED**

Replace the two managed-descriptor expectations with exact owner-bearing strings:

```rust
#[test]
fn protected_directory_descriptor_has_token_user_owner_and_required_dacl() {
    assert_eq!(
        super::managed_security_sddl("S-1-5-21-123", true),
        "O:S-1-5-21-123D:P(A;OICI;FA;;;S-1-5-21-123)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
    );
}

#[test]
fn protected_file_descriptor_has_token_user_owner_without_inheritance_flags() {
    assert_eq!(
        super::managed_security_sddl("S-1-5-21-123", false),
        "O:S-1-5-21-123D:P(A;;FA;;;S-1-5-21-123)(A;;FA;;;SY)(A;;FA;;;BA)"
    );
}
```

Run:

```powershell
cargo test -p hoimin-cli workspace::owned::windows::tests::protected_ -- --nocapture
```

Expected: both tests fail because the current value starts with `D:P`.

- [ ] **Step 2: Rename and implement the owner-bearing formatter**

Use one formatter for both the `O:` field and current-user ACE; do not derive the owner from `TOKEN_OWNER`:

```rust
fn managed_security_sddl(user_sid: &str, directory: bool) -> String {
    let inheritance = if directory { "OICI" } else { "" };
    format!(
        "O:{user_sid}D:P(A;{inheritance};FA;;;{user_sid})(A;{inheritance};FA;;;SY)(A;{inheritance};FA;;;BA)"
    )
}
```

Update `SecurityDescriptor::for_user` to call `managed_security_sddl` and leave `mutation_barrier_sddl` unchanged.

- [ ] **Step 3: Prove the native descriptor reports the token user as owner**

Add an owner accessor that uses `GetSecurityDescriptorOwner`, then exercise it against the live token:

```rust
fn owner(&self) -> io::Result<PSID> {
    let mut owner = ptr::null_mut();
    let mut defaulted = 0;
    if unsafe {
        windows_sys::Win32::Security::GetSecurityDescriptorOwner(
            self.pointer(),
            &raw mut owner,
            &raw mut defaulted,
        )
    } == 0 {
        return Err(io::Error::last_os_error());
    }
    if owner.is_null() || defaulted != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "managed descriptor has no explicit owner",
        ));
    }
    Ok(owner)
}

#[test]
fn native_managed_descriptor_owner_equals_token_user() {
    let token = super::UserToken::open().unwrap();
    let descriptor = super::SecurityDescriptor::for_user(&token, true).unwrap();
    assert_ne!(unsafe { windows_sys::Win32::Security::EqualSid(descriptor.owner().unwrap(), token.sid()) }, 0);
}
```

Run:

```powershell
cargo test -p hoimin-cli workspace::owned::windows::tests::native_managed_descriptor_owner_equals_token_user -- --exact
```

Expected: PASS on Windows.

- [ ] **Step 4: Run the focused formatter/security tests**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p hoimin-cli workspace::owned::windows::tests:: -- --nocapture
```

Expected: all focused tests pass; the mutation-barrier expectation remains byte-for-byte unchanged.

- [ ] **Step 5: Commit the descriptor contract**

```powershell
git add crates/hoimin-cli/src/workspace/owned/windows.rs
git commit -m "fix: assign Windows managed owner explicitly"
```

---

### Task 2: Add exclusive relative creates with the managed descriptor

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/owned/windows.rs:1-198`
- Test: `crates/hoimin-cli/src/workspace/owned/windows.rs:482-521`

**Interfaces:**

- Consumes: Task 1 `SecurityDescriptor::for_user` and a live parent implementing `AsRawHandle`.
- Produces: `create_relative_managed_directory(parent, name) -> io::Result<File>` and `create_relative_managed_file(parent, name, access) -> io::Result<File>`; both use `FILE_CREATE`, no-follow native options, delete sharing, and the explicit owner descriptor.

- [ ] **Step 1: Add native RED tests for exclusive relative directory and file creation**

Add tests that create below an already-open parent, verify owner/security, and then retry the same name:

```rust
#[test]
fn relative_managed_creates_are_exclusive_and_owned_by_token_user() {
    use std::ffi::OsStr;

    let temporary = tempfile::tempdir().unwrap();
    let parent = cap_std::fs::Dir::open_ambient_dir(
        temporary.path(),
        cap_std::ambient_authority(),
    )
    .unwrap();

    let directory = super::create_relative_managed_directory(
        &parent,
        OsStr::new("managed-directory"),
    )
    .unwrap();
    super::verify_current_user_owner(&directory).unwrap();

    let file = super::create_relative_managed_file(
        &parent,
        OsStr::new("managed-file"),
        super::ManagedFileAccess::ReadWrite,
    )
    .unwrap();
    super::verify_current_user_owner(&file).unwrap();

    let nested = super::create_relative_managed_file(
        &directory,
        OsStr::new("nested-protocol-file"),
        super::ManagedFileAccess::Write,
    )
    .unwrap();
    super::verify_current_user_owner(&nested).unwrap();

    assert_eq!(
        super::create_relative_managed_directory(
            &parent,
            OsStr::new("managed-directory"),
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::AlreadyExists,
    );
    assert_eq!(
        super::create_relative_managed_file(
            &parent,
            OsStr::new("managed-file"),
            super::ManagedFileAccess::ReadWrite,
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::AlreadyExists,
    );
}
```

Run:

```powershell
cargo test -p hoimin-cli workspace::owned::windows::tests::relative_managed_creates_are_exclusive_and_owned_by_token_user -- --exact
```

Expected: compile failure because the creation API and access enum do not exist.

- [ ] **Step 2: Implement strict component encoding**

Reject empty, dot, dot-dot, separator, and NUL components before calling NT APIs:

```rust
fn encoded_component(name: &OsStr) -> io::Result<Vec<u16>> {
    let encoded = name.encode_wide().collect::<Vec<_>>();
    let dot = [u16::from(b'.')];
    let dot_dot = [u16::from(b'.'), u16::from(b'.')];
    if encoded.is_empty()
        || encoded == dot
        || encoded == dot_dot
        || encoded.last().is_some_and(|unit| matches!(*unit, 0x20 | 0x2e))
        || encoded.iter().any(|unit| {
            *unit < 0x20
                || matches!(*unit, 0x22 | 0x2a | 0x2f | 0x3a | 0x3c | 0x3e | 0x3f | 0x5c | 0x7c)
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed object name is not one direct component",
        ));
    }
    let text = String::from_utf16(&encoded)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "managed name is not valid UTF-16"))?;
    let basename = text.split('.').next().unwrap_or_default().to_uppercase();
    let reserved = matches!(
        basename.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    )
        || ["COM", "LPT"].iter().any(|prefix| {
            basename.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
            })
        });
    if reserved {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed object name is a reserved Windows device name",
        ));
    }
    Ok(encoded)
}
```

Add table-driven unit cases for `""`, `"."`, `".."`, `"a/b"`, `"a\\b"`, `"name:stream"`, `"bad*name"`, `"trailing."`, `"NUL.txt"`, `"COM¹"`, `"CONOUT$.log"`, and an embedded NUL; assert `InvalidInput` without filesystem changes.

- [ ] **Step 3: Implement the shared `NtCreateFile(FILE_CREATE)` primitive**

Extend the existing imports rather than declaring local FFI values. Import
`zeroed` beside `size_of`; `OBJECT_ATTRIBUTES` from
`windows_sys::Wdk::Foundation`; `FILE_CREATE`, `FILE_DIRECTORY_FILE`,
`FILE_NON_DIRECTORY_FILE`, `FILE_OPEN_REPARSE_POINT`,
`FILE_SYNCHRONOUS_IO_NONALERT`, and `NtCreateFile` from
`windows_sys::Wdk::Storage::FileSystem`; `OBJ_CASE_INSENSITIVE`,
`RtlNtStatusToDosError`, and `UNICODE_STRING` from
`windows_sys::Win32::Foundation`; `DELETE`, `FILE_ADD_FILE`,
`FILE_ADD_SUBDIRECTORY`, `FILE_DELETE_CHILD`, `FILE_DISPOSITION_INFO`,
`FILE_LIST_DIRECTORY`, `FILE_READ_ATTRIBUTES`, `FILE_READ_DATA`,
`FILE_TRAVERSE`, `FILE_WRITE_ATTRIBUTES`, `FILE_WRITE_DATA`,
`FileDispositionInfo`, and `SetFileInformationByHandle` from
`windows_sys::Win32::Storage::FileSystem`; and `IO_STATUS_BLOCK` from
`windows_sys::Win32::System::IO`. Reuse the module's existing `HANDLE`, share,
read-control, and synchronize imports.

Use an enum whose variants map only to the rights needed after creation:

```rust
#[derive(Clone, Copy, Debug)]
pub(super) enum ManagedFileAccess {
    ReadWrite,
    Write,
}

enum ManagedEntryKind {
    Directory,
    File(ManagedFileAccess),
}
```

Implement `create_relative_managed` with these exact properties:

```rust
fn create_relative_managed(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    kind: ManagedEntryKind,
) -> io::Result<File> {
    let token = UserToken::open()?;
    let directory = matches!(kind, ManagedEntryKind::Directory);
    let descriptor = SecurityDescriptor::for_user(&token, directory)?;
    let expected_dacl = descriptor.dacl()?;
    let mut encoded = encoded_component(name)?;
    let byte_len = encoded
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "managed name is too long"))?;
    let unicode = UNICODE_STRING {
        Length: byte_len,
        MaximumLength: byte_len,
        Buffer: encoded.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).expect("OBJECT_ATTRIBUTES size fits u32"),
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: ptr::from_ref(&unicode),
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: descriptor.pointer(),
        SecurityQualityOfService: ptr::null_mut(),
    };
    let common = SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES;
    let (access, options) = match kind {
        ManagedEntryKind::Directory => (
            common
                | DELETE
                | FILE_LIST_DIRECTORY
                | FILE_TRAVERSE
                | FILE_ADD_FILE
                | FILE_ADD_SUBDIRECTORY
                | FILE_DELETE_CHILD
                | FILE_WRITE_ATTRIBUTES,
            FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        ),
        ManagedEntryKind::File(ManagedFileAccess::ReadWrite) => (
            common | DELETE | FILE_READ_DATA | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        ),
        ManagedEntryKind::File(ManagedFileAccess::Write) => (
            common | DELETE | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        ),
    };
    let mut handle: HANDLE = ptr::null_mut();
    let mut io_status: IO_STATUS_BLOCK = unsafe { zeroed() };
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            access,
            &raw const attributes,
            &raw mut io_status,
            ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_CREATE,
            options,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(io_error_from_ntstatus(status));
    }
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    const FILE_CREATED_INFORMATION: usize = 2;
    if io_status.Information != FILE_CREATED_INFORMATION {
        let primary = io::Error::new(
            io::ErrorKind::InvalidData,
            "exclusive managed create returned a non-created result",
        );
        return Err(error_after_created_rollback(&file, primary));
    }
    if let Err(primary) = verify_security(&file, &token, expected_dacl) {
        return Err(error_after_created_rollback(&file, primary));
    }
    Ok(file)
}
```

Implement rollback on the same newly created handle:

```rust
fn rollback_created(file: &File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileDispositionInfo,
            (&raw const disposition).cast(),
            u32::try_from(size_of::<FILE_DISPOSITION_INFO>())
                .expect("FILE_DISPOSITION_INFO size fits u32"),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn error_after_created_rollback_with(
    file: &File,
    primary: io::Error,
    rollback: impl FnOnce(&File) -> io::Result<()>,
) -> io::Error {
    match rollback(file) {
        Ok(()) => primary,
        Err(secondary) => io::Error::new(
            primary.kind(),
            format!("{primary}; secondary created-object rollback failure: {secondary}"),
        ),
    }
}

fn error_after_created_rollback(file: &File, primary: io::Error) -> io::Error {
    error_after_created_rollback_with(file, primary, rollback_created)
}
```

The `File` drops immediately after the returned error, making a successful
disposition effective. Factor the last verification call through a private
`create_relative_managed_with_verifier` helper whose final parameter is
`impl FnOnce(&File, &UserToken, *mut ACL) -> io::Result<()>`; production wrappers
pass `verify_security`. A native test passes an injected failing verifier and
asserts the just-created file and directory are absent after the error. Add a
unit test for `error_after_created_rollback_with` whose rollback closure fails;
require the primary verification message first and the bounded secondary
rollback detail second. Reuse the
existing NTSTATUS-to-Win32 conversion pattern; do not add an absolute-path
retry.

- [ ] **Step 4: Test no-follow type and invalid-name behavior**

Extend the native test so a pre-created directory cannot be opened as a file and an existing file cannot be opened as a directory. The nested-file assertion from Step 1 must also pass through the returned directory handle; this fixes `FILE_ADD_FILE`/traverse authority as part of the contract instead of merely checking the bit mask. Add a symlink fixture when Windows grants symlink creation and assert `FILE_CREATE` returns `AlreadyExists` without following it. The required assertions are:

```rust
assert!(super::create_relative_managed_file(
    &parent,
    OsStr::new("managed-directory"),
    super::ManagedFileAccess::Write,
).is_err());
assert!(super::create_relative_managed_directory(
    &parent,
    OsStr::new("managed-file"),
).is_err());
assert_eq!(
    super::encoded_component(OsStr::new("../escape")).unwrap_err().kind(),
    io::ErrorKind::InvalidInput,
);
```

Run:

```powershell
cargo test -p hoimin-cli workspace::owned::windows::tests:: -- --nocapture
```

Expected: all managed descriptor and relative-create tests pass.

- [ ] **Step 5: Commit the native creation primitive**

```powershell
git add crates/hoimin-cli/src/workspace/owned/windows.rs
git commit -m "feat: create Windows managed entries securely"
```

---

### Task 3: Route every Rust protocol-object create through the secure primitive

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/owned.rs:1749-1758`
- Modify: `crates/hoimin-cli/src/workspace/owned.rs:1943-2005`
- Modify: `crates/hoimin-cli/src/workspace/owned.rs:2597-2760`
- Modify: `crates/hoimin-cli/src/workspace/owned.rs:3005-3166`
- Modify: `crates/hoimin-cli/src/workspace/owned.rs:4050-4112`
- Test: `crates/hoimin-cli/src/workspace/owned.rs:1-1450`

**Interfaces:**

- Consumes: Task 2 relative directory/file creators.
- Produces: platform-neutral `create_owned_directory` and `create_owned_file` helpers used by every production protocol create; Unix retains `0700`/`0600`, Windows gets the explicit owner descriptor atomically.

- [ ] **Step 1: Add a Windows lifecycle RED test covering more than the root**

Inside the existing `owned.rs` test module, create a coordinator, run root, managed child, and retention marker, then verify each exact handle:

```rust
#[cfg(windows)]
#[test]
fn every_windows_protocol_object_is_owned_by_the_token_user() {
    let parent = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(parent.path()).unwrap();
    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    super::windows::verify_current_user_owner(&coordinator.file).unwrap();

    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    super::windows::verify_current_user_owner(root.lease.lock().unwrap().as_deref().unwrap()).unwrap();
    super::windows::verify_current_user_owner(root.heartbeat.lock().unwrap().as_ref().unwrap()).unwrap();

    let child = root.create_child("owner-check-").unwrap();
    let child_file = child.dir.as_ref().unwrap().try_clone().unwrap().into_std_file();
    super::windows::verify_current_user_owner(&child_file).unwrap();

    root.retain().unwrap();
    let retained = crate::workspace::root::windows::open_regular_file_shared(
        &root.dir,
        std::ffi::OsStr::new(RETAIN_FILE),
    )
    .unwrap();
    super::windows::verify_current_user_owner(&retained).unwrap();

    {
        let lease = root.lease.lock().unwrap();
        super::write_cleanup_ready_marker(
            &root.dir,
            &root.path,
            &root.run_id,
            root.owner,
            lease.as_deref().unwrap(),
        )
        .unwrap();
    }
    let cleanup_ready = crate::workspace::root::windows::open_regular_file_shared(
        &root.dir,
        std::ffi::OsStr::new(CLEANUP_READY_FILE),
    )
    .unwrap();
    super::windows::verify_current_user_owner(&cleanup_ready).unwrap();
}
```

Run:

```powershell
cargo test -p hoimin-cli workspace::owned::tests::every_windows_protocol_object_is_owned_by_the_token_user -- --exact
```

Expected: FAIL on an administrator-default-owner token for at least the coordinator, run root, child, or marker; this reproduces the non-root-only defect.

- [ ] **Step 2: Add platform-neutral exclusive create helpers**

Define helpers next to `create_managed_root_entry`:

```rust
#[derive(Clone, Copy)]
enum OwnedFileAccess {
    ReadWrite,
    Write,
}

#[cfg(unix)]
fn create_owned_directory(
    parent: &cap_std::fs::Dir,
    name: &str,
) -> std::io::Result<cap_std::fs::Dir> {
    rustix::fs::mkdirat(parent, name, rustix::fs::Mode::from_raw_mode(0o700))
        .map_err(std::io::Error::from)?;
    open_owned_directory(parent, name)
}

#[cfg(windows)]
fn create_owned_directory(
    parent: &cap_std::fs::Dir,
    name: &str,
) -> std::io::Result<cap_std::fs::Dir> {
    let file = windows::create_relative_managed_directory(parent, std::ffi::OsStr::new(name))?;
    Ok(cap_std::fs::Dir::from_std_file(file))
}
```

Implement `create_owned_file(parent, name, access)` likewise: Unix uses `create_new(true)`, no-follow, and mode `0600`; Windows maps to `ManagedFileAccess` and returns the native `File` directly.

- [ ] **Step 3: Replace directory creation sites while preserving rollback identities**

Replace the staging and managed-child `Dir::create_dir` calls with `create_owned_directory`. Keep the returned directory handle as the first opened identity source. On Windows, retain the staging handle with `DELETE` access for publication; clone it only for descendant work. The new sequence is:

```rust
let staging_dir = create_owned_directory(&coordinator.dir, &staging_name)
    .map_err(|error| WorkspaceError::io("create staging root", &coordinator.path, error))?;
let staging_identity = directory_identity(&staging_dir)
    .map_err(|error| WorkspaceError::io("identify staging root", &staging_path, error))?;
staging_rollback.expect_identity(staging_identity);
publish_hook(PublishBoundary::StagingCreated)?;
```

For `ManagedRunRoot::create_child_with_hook`, construct `ManagedChild.dir` from the returned secure directory and compare its handle identity with the parent entry before disarming rollback.

- [ ] **Step 4: Replace all protocol-file creation sites**

Use `create_owned_file` in these five paths and nowhere else:

1. coordinator initialization (`ReadWrite`);
2. lease marker (`ReadWrite`);
3. heartbeat marker (`Write`);
4. retention/control marker (`Write`);
5. cleanup-ready marker (`Write`).

For example, coordinator creation becomes:

```rust
match create_owned_file(dir, COORDINATOR_FILE, OwnedFileAccess::ReadWrite) {
    Ok(mut file) => {
        let identity = file_identity(&file).map_err(|error| {
            WorkspaceError::io("identify new coordinator", &coordinator_path, error)
        })?;
        initialize_new_coordinator(dir, &mut file, identity, &coordinator_path, deadline)
    }
    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
        open_initialized_coordinator_until(dir, &coordinator_path, deadline)
    }
    Err(error) => Err(WorkspaceError::io("create coordinator", &coordinator_path, error)),
}
```

Extract the current new-coordinator body into `initialize_new_coordinator` without changing lock, fixed-size write, flush, validation, rollback, or deadline ordering. Existing coordinator opens must still call `secure_coordinator_file`, which verifies the owner before repairing the DACL.

- [ ] **Step 5: Add an owner-mismatch regression at the policy seam**

Refactor only enough to make the existing-object ordering directly testable. Put this policy seam in `owned/windows.rs`:

```rust
fn repair_existing_policy(
    verify: impl FnOnce() -> io::Result<()>,
    repair: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    verify()?;
    repair()
}

fn repair_existing_dacl(
    file: &File,
    token: &UserToken,
    expected_dacl: *mut ACL,
) -> io::Result<()> {
    repair_existing_policy(
        || verify_owner(file, token),
        || {
            set_protected_dacl(file, expected_dacl)?;
            verify_security(file, token, expected_dacl)
        },
    )
}
```

Add a test seam with a recorder and assert an owner verification error prevents the DACL setter from running. The native production wrapper still invokes `verify_owner` and `SetSecurityInfo`; the test seam records the same order:

```rust
#[test]
fn existing_owner_mismatch_prevents_dacl_repair() {
    let mut dacl_writes = 0;
    let result = super::repair_existing_policy(
        || Err(io::Error::new(io::ErrorKind::PermissionDenied, "owner mismatch")),
        || {
            dacl_writes += 1;
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(dacl_writes, 0);
}
```

This pure ordering test is required even when a local non-administrator token cannot create a native mismatched-owner fixture.

- [ ] **Step 6: Run focused and workspace Rust gates**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p hoimin-cli workspace::owned:: -- --nocapture
cargo test --workspace --all-targets --all-features
```

Expected: all commands exit 0 on Windows. The focused test proves root, coordinator, run root, child, and markers use the token user.

- [ ] **Step 7: Commit the lifecycle wiring**

```powershell
git add crates/hoimin-cli/src/workspace/owned.rs crates/hoimin-cli/src/workspace/owned/windows.rs
git commit -m "fix: secure all Windows managed creations"
```

---

### Task 4: Verify Rust compatibility and preserve delivery evidence

**Files:**

- Verification-only task: no repository files are modified when all gates pass. If a gate exposes a real defect, return to Task 1, 2, or 3, add a RED regression there, fix the owning commit, and rerun this task from the beginning.

**Interfaces:**

- Consumes: Tasks 1-3.
- Produces: one clean Rust owner-fix SHA that the Python capability plan can include in PR-wide verification.

- [ ] **Step 1: Run formatting, lint, MSRV, contracts, and full tests on the same SHA**

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.88 check --workspace --all-targets --all-features --locked
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
cargo test --workspace --all-targets --all-features
cargo test -p hoimin-cli --test run_e2e
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
$coreTree = @(cargo tree -p hoimin-core --edges normal --prefix none)
$forbiddenCoreDependency = @($coreTree | Select-String -Pattern '(^| )(tokio|rusqlite|tempfile|windows-sys|libc|hoimin-cli)( |$)')
if ($forbiddenCoreDependency.Count -ne 0) { throw "hoimin-core dependency purity failed: $forbiddenCoreDependency" }
```

Install the pinned nightly first with `rustup toolchain install nightly-2026-07-27 --profile minimal` if absent. Expected: every command exits 0 and the dependency scan is empty. If an unexpected failure appears, invoke `superpowers:systematic-debugging`, add a failing regression first, and amend only the task commit that owns the defect.

- [ ] **Step 2: Audit all production creation sites mechanically**

Run:

```powershell
rg -n "create_new\(true\)|\.create_dir\(" crates/hoimin-cli/src/workspace/owned.rs
rg -n "create_owned_file|create_owned_directory|create_managed_root_entry" crates/hoimin-cli/src/workspace/owned.rs
```

Expected: every production coordinator, staging, child, lease, heartbeat, retention, and cleanup-ready create routes through an owner-bearing helper. Matches inside `#[cfg(test)]` fixtures may remain ordinary fixture creation.

- [ ] **Step 3: Confirm the worktree contains only intentional commits**

```powershell
git status --short
git log --oneline origin/feat/disk-safe-mutation..HEAD
```

Expected: clean status. Do not push yet; the Python plan performs the combined same-SHA verification and delivery.
