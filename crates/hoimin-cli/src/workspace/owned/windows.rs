use std::ffi::{OsStr, c_void};
use std::fs::File;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr;

use camino::Utf8Path;
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER,
    ERROR_SUCCESS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree, OBJ_CASE_INSENSITIVE,
    RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT, SetSecurityInfo,
};
#[cfg(test)]
use windows_sys::Win32::Security::GetSecurityDescriptorOwner;
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, EqualSid, GetSecurityDescriptorControl,
    GetSecurityDescriptorDacl, GetTokenInformation, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD, FILE_DISPOSITION_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_TRAVERSE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FileDispositionInfo, OPEN_EXISTING,
    READ_CONTROL, SYNCHRONIZE, SetFileInformationByHandle, WRITE_DAC,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenEventW, OpenProcessToken,
};

struct LocalMemory(*mut c_void);

impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this pointer was allocated by a Windows API documented to require LocalFree.
            unsafe { LocalFree(self.0) };
        }
    }
}

struct TokenHandle(HANDLE);

impl Drop for TokenHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: OpenProcessToken returned this owned handle.
            unsafe { CloseHandle(self.0) };
        }
    }
}

struct UserToken {
    _handle: TokenHandle,
    buffer: Vec<usize>,
}

impl UserToken {
    fn open() -> io::Result<Self> {
        let mut handle = ptr::null_mut();
        // SAFETY: handle points to writable storage and the pseudo process handle is always valid.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let handle = TokenHandle(handle);
        let mut needed = 0_u32;
        // SAFETY: a null buffer with length zero is the documented size query.
        let queried = unsafe {
            GetTokenInformation(handle.0, TokenUser, ptr::null_mut(), 0, &raw mut needed)
        };
        if queried != 0
            || io::Error::last_os_error().raw_os_error()
                != Some(i32::try_from(ERROR_INSUFFICIENT_BUFFER).expect("error code fits i32"))
        {
            return Err(io::Error::last_os_error());
        }
        let length = usize::try_from(needed)
            .map_err(|_| io::Error::other("token information length overflow"))?;
        let words = length
            .checked_add(size_of::<usize>() - 1)
            .and_then(|bytes| bytes.checked_div(size_of::<usize>()))
            .ok_or_else(|| io::Error::other("token information allocation overflow"))?;
        let mut buffer = vec![0_usize; words];
        // SAFETY: buffer is writable for exactly `needed` bytes and remains alive afterward.
        if unsafe {
            GetTokenInformation(
                handle.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &raw mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            _handle: handle,
            buffer,
        })
    }

    fn sid(&self) -> PSID {
        // SAFETY: GetTokenInformation populated a TOKEN_USER at the start of this live buffer.
        unsafe { self.buffer.as_ptr().cast::<TOKEN_USER>().read().User.Sid }
    }

    fn sid_string(&self) -> io::Result<String> {
        let mut string = ptr::null_mut();
        // SAFETY: sid points into the live token buffer and string is writable output storage.
        if unsafe { ConvertSidToStringSidW(self.sid(), &raw mut string) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let allocation = LocalMemory(string.cast());
        let mut length = 0_usize;
        // SAFETY: ConvertSidToStringSidW returns a NUL-terminated allocation.
        while unsafe { *string.add(length) } != 0 {
            length = length
                .checked_add(1)
                .ok_or_else(|| io::Error::other("SID string length overflow"))?;
        }
        // SAFETY: the preceding scan found the allocation's terminating NUL.
        let units = unsafe { std::slice::from_raw_parts(string, length) };
        let result = String::from_utf16(units)
            .map_err(|_| io::Error::other("current-user SID is not valid UTF-16"));
        drop(allocation);
        result
    }
}

struct SecurityDescriptor {
    allocation: LocalMemory,
}

impl SecurityDescriptor {
    fn for_user(token: &UserToken, directory: bool) -> io::Result<Self> {
        let sddl = managed_security_sddl(&token.sid_string()?, directory);
        Self::from_sddl(&sddl)
    }

    fn for_mutation_barrier(token: &UserToken) -> io::Result<Self> {
        let sddl = mutation_barrier_sddl(&token.sid_string()?);
        Self::from_sddl(&sddl)
    }

    fn from_sddl(sddl: &str) -> io::Result<Self> {
        let wide = OsStr::new(&sddl)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut descriptor = ptr::null_mut();
        // SAFETY: wide is NUL-terminated and descriptor is writable output storage.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            allocation: LocalMemory(descriptor),
        })
    }

    fn pointer(&self) -> *mut c_void {
        self.allocation.0
    }

    fn dacl(&self) -> io::Result<*mut ACL> {
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = ptr::null_mut();
        // SAFETY: descriptor is live and all output pointers are writable.
        if unsafe {
            GetSecurityDescriptorDacl(
                self.pointer(),
                &raw mut present,
                &raw mut dacl,
                &raw mut defaulted,
            )
        } == 0
            || present == 0
            || dacl.is_null()
        {
            return Err(io::Error::last_os_error());
        }
        Ok(dacl)
    }

    #[cfg(test)]
    fn owner(&self) -> io::Result<PSID> {
        let mut owner = ptr::null_mut();
        let mut defaulted = 0;
        if unsafe { GetSecurityDescriptorOwner(self.pointer(), &raw mut owner, &raw mut defaulted) }
            == 0
        {
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
}

fn managed_security_sddl(user_sid: &str, directory: bool) -> String {
    let inheritance = if directory { "OICI" } else { "" };
    format!(
        "O:{user_sid}D:P(A;{inheritance};FA;;;{user_sid})(A;{inheritance};FA;;;SY)(A;{inheritance};FA;;;BA)"
    )
}

fn mutation_barrier_sddl(user_sid: &str) -> String {
    format!("D:P(A;;GA;;;{user_sid})(A;;GA;;;SY)(A;;GA;;;BA)")
}

fn mutation_barrier_name(token: &UserToken, run_id: &str) -> io::Result<Vec<u16>> {
    let name = format!(
        "Global\\hoimin-workspace-mutation-{}-{run_id}",
        token.sid_string()?
    );
    Ok(OsStr::new(&name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect())
}

#[derive(Debug)]
pub(super) struct RootMutationBarrier(isize);

impl RootMutationBarrier {
    pub(super) fn create(run_id: &str) -> io::Result<Self> {
        let token = UserToken::open()?;
        let descriptor = SecurityDescriptor::for_mutation_barrier(&token)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                .expect("SECURITY_ATTRIBUTES size fits u32"),
            lpSecurityDescriptor: descriptor.pointer(),
            bInheritHandle: 0,
        };
        let name = mutation_barrier_name(&token, run_id)?;
        // SAFETY: name is NUL-terminated and attributes references a live descriptor.
        let handle = unsafe { CreateEventW(&raw const attributes, 1, 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetLastError is the documented way to distinguish create from open after a
        // successful named CreateEventW call.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: CreateEventW returned one owned handle even for an existing event.
            unsafe { CloseHandle(handle) };
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "managed-root mutation barrier already exists",
            ));
        }
        Ok(Self(handle as isize))
    }
}

impl Drop for RootMutationBarrier {
    fn drop(&mut self) {
        // SAFETY: the barrier owns exactly one CreateEventW handle.
        unsafe { CloseHandle(self.0 as HANDLE) };
    }
}

pub(super) fn root_mutation_barrier_exists(run_id: &str) -> io::Result<bool> {
    let token = UserToken::open()?;
    let name = mutation_barrier_name(&token, run_id)?;
    // SAFETY: name is NUL-terminated and the returned handle is owned on success.
    let handle = unsafe { OpenEventW(SYNCHRONIZE, 0, name.as_ptr()) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        if error.raw_os_error()
            == Some(i32::try_from(ERROR_FILE_NOT_FOUND).expect("error code fits i32"))
        {
            return Ok(false);
        }
        return Err(error);
    }
    // SAFETY: OpenEventW returned one owned handle.
    unsafe { CloseHandle(handle) };
    Ok(true)
}

pub(super) fn create_managed_directory(path: &Utf8Path) -> io::Result<()> {
    let token = UserToken::open()?;
    let descriptor = SecurityDescriptor::for_user(&token, true)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
            .expect("SECURITY_ATTRIBUTES size fits u32"),
        lpSecurityDescriptor: descriptor.pointer(),
        bInheritHandle: 0,
    };
    let wide = path
        .as_std_path()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: wide is NUL-terminated and attributes references a live descriptor.
    if unsafe { CreateDirectoryW(wide.as_ptr(), &raw const attributes) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn secure_directory(expected: &File, path: &Utf8Path) -> io::Result<()> {
    secure_object(expected, path, true)
}

pub(super) fn secure_file(expected: &File, path: &Utf8Path) -> io::Result<()> {
    secure_object(expected, path, false)
}

fn secure_object(expected: &File, path: &Utf8Path, directory: bool) -> io::Result<()> {
    let wide = path
        .as_std_path()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            0
        };
    // SAFETY: wide is NUL-terminated and no optional security/template pointers are used.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            READ_CONTROL | WRITE_DAC,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            flags,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned one newly owned handle.
    let secured = unsafe { File::from_raw_handle(handle.cast()) };
    let metadata = secured.metadata()?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.is_dir() != directory
        || file_identity(&secured)? != file_identity(expected)?
    {
        return Err(io::Error::other("managed object identity changed"));
    }

    let token = UserToken::open()?;
    let expected_descriptor = SecurityDescriptor::for_user(&token, directory)?;
    let expected_dacl = expected_descriptor.dacl()?;
    repair_existing_dacl(&secured, &token, expected_dacl)?;
    if file_identity(&secured)? != file_identity(expected)? {
        return Err(io::Error::other(
            "managed object identity changed while securing it",
        ));
    }
    Ok(())
}

fn repair_existing_policy(
    verify: impl FnOnce() -> io::Result<()>,
    repair: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    verify()?;
    repair()
}

fn repair_existing_dacl(file: &File, token: &UserToken, expected_dacl: *mut ACL) -> io::Result<()> {
    repair_existing_policy(
        || verify_owner(file, token),
        || {
            set_protected_dacl(file, expected_dacl)?;
            verify_security(file, token, expected_dacl)
        },
    )
}

fn set_protected_dacl(file: &File, expected_dacl: *mut ACL) -> io::Result<()> {
    // SAFETY: file has READ_CONTROL|WRITE_DAC and expected_dacl belongs to a live descriptor.
    let status = unsafe {
        SetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            expected_dacl,
            ptr::null(),
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(i32::MAX),
        ))
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(super) enum ManagedFileAccess {
    ReadWrite,
    Write,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum ManagedEntryKind {
    Directory,
    File(ManagedFileAccess),
}

#[allow(dead_code)]
pub(super) fn create_relative_managed_directory(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    create_relative_managed(parent, name, ManagedEntryKind::Directory)
}

#[allow(dead_code)]
pub(super) fn create_relative_managed_file(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    access: ManagedFileAccess,
) -> io::Result<File> {
    create_relative_managed(parent, name, ManagedEntryKind::File(access))
}

#[allow(dead_code)]
fn encoded_component(name: &OsStr) -> io::Result<Vec<u16>> {
    let encoded = name.encode_wide().collect::<Vec<_>>();
    let dot = [u16::from(b'.')];
    let dot_dot = [u16::from(b'.'), u16::from(b'.')];
    if encoded.is_empty()
        || encoded == dot
        || encoded == dot_dot
        || encoded
            .last()
            .is_some_and(|unit| matches!(*unit, 0x20 | 0x2e))
        || encoded.iter().any(|unit| {
            *unit < 0x20
                || matches!(
                    *unit,
                    0x22 | 0x2a | 0x2f | 0x3a | 0x3c | 0x3e | 0x3f | 0x5c | 0x7c
                )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed object name is not one direct component",
        ));
    }
    let text = String::from_utf16(&encoded).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed name is not valid UTF-16",
        )
    })?;
    let basename = text.split('.').next().unwrap_or_default().to_uppercase();
    let reserved = matches!(
        basename.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        basename.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
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

#[allow(dead_code)]
fn create_relative_managed(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    kind: ManagedEntryKind,
) -> io::Result<File> {
    create_relative_managed_with_verifier(parent, name, kind, verify_security)
}

#[allow(dead_code)]
fn create_relative_managed_with_verifier(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    kind: ManagedEntryKind,
    verifier: impl FnOnce(&File, &UserToken, *mut ACL) -> io::Result<()>,
) -> io::Result<File> {
    const FILE_CREATED_INFORMATION: usize = 2;

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
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>())
            .expect("OBJECT_ATTRIBUTES size fits u32"),
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: ptr::from_ref(&unicode),
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: descriptor.pointer().cast(),
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
    // SAFETY: all input pointers refer to live storage for the duration of this synchronous call.
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
    // SAFETY: successful NtCreateFile returns one newly owned kernel handle.
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    if io_status.Information != FILE_CREATED_INFORMATION {
        let primary = io::Error::new(
            io::ErrorKind::InvalidData,
            "exclusive managed create returned a non-created result",
        );
        return Err(error_after_created_rollback(&file, primary));
    }
    if let Err(primary) = verifier(&file, &token, expected_dacl) {
        return Err(error_after_created_rollback(&file, primary));
    }
    Ok(file)
}

#[allow(dead_code)]
fn io_error_from_ntstatus(status: i32) -> io::Error {
    // SAFETY: RtlNtStatusToDosError accepts every NTSTATUS value.
    let code = unsafe { RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(i32::try_from(code).unwrap_or(i32::MAX))
}

#[allow(dead_code)]
fn rollback_created(file: &File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: file is a live handle and the disposition buffer has FileDispositionInfo's size.
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn error_after_created_rollback(file: &File, primary: io::Error) -> io::Error {
    error_after_created_rollback_with(file, primary, rollback_created)
}

fn verify_owner(file: &(impl AsRawHandle + ?Sized), token: &UserToken) -> io::Result<()> {
    let mut owner = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    // SAFETY: all output pointers are writable and file has READ_CONTROL access.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &raw mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(i32::MAX),
        ));
    }
    let _descriptor = LocalMemory(descriptor);
    if owner.is_null()
        // SAFETY: both SID pointers belong to live buffers/descriptors.
        || unsafe { EqualSid(owner, token.sid()) } == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed object is not owned by the current user",
        ));
    }
    Ok(())
}

pub(super) fn verify_current_user_owner(file: &File) -> io::Result<()> {
    let token = UserToken::open()?;
    verify_owner(file, &token)
}

fn verify_security(file: &File, token: &UserToken, expected_dacl: *mut ACL) -> io::Result<()> {
    let mut owner = ptr::null_mut();
    let mut actual_dacl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    // SAFETY: all output pointers are writable and file has READ_CONTROL access.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &raw mut owner,
            ptr::null_mut(),
            &raw mut actual_dacl,
            ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(i32::MAX),
        ));
    }
    let descriptor = LocalMemory(descriptor);
    if owner.is_null()
        || actual_dacl.is_null()
        // SAFETY: both SID pointers belong to live buffers/descriptors.
        || unsafe { EqualSid(owner, token.sid()) } == 0
    {
        return Err(io::Error::other("managed object owner or DACL is invalid"));
    }
    let mut control = 0_u16;
    let mut revision = 0_u32;
    // SAFETY: descriptor is live and control/revision are writable.
    if unsafe { GetSecurityDescriptorControl(descriptor.0, &raw mut control, &raw mut revision) }
        == 0
        || control & SE_DACL_PROTECTED == 0
    {
        return Err(io::Error::other("managed object DACL is not protected"));
    }
    // SAFETY: both ACL pointers belong to live descriptors and AclSize bounds each allocation.
    let expected = unsafe {
        std::slice::from_raw_parts(
            expected_dacl.cast::<u8>(),
            usize::from((*expected_dacl).AclSize),
        )
    };
    // SAFETY: actual_dacl belongs to the live descriptor returned by GetSecurityInfo.
    let actual = unsafe {
        std::slice::from_raw_parts(
            actual_dacl.cast::<u8>(),
            usize::from((*actual_dacl).AclSize),
        )
    };
    if actual != expected {
        return Err(io::Error::other(
            "managed object DACL does not match the user/SYSTEM/Administrators ACL",
        ));
    }
    Ok(())
}

pub(super) fn file_identity(file: &File) -> io::Result<(u64, u64)> {
    super::super::root::windows::file_identity_io(file)
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::{OsStr, OsString},
        io,
        os::windows::ffi::OsStringExt,
    };

    use uuid::Uuid;

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

    #[test]
    fn native_managed_descriptor_owner_equals_token_user() {
        let token = super::UserToken::open().unwrap();
        let descriptor = super::SecurityDescriptor::for_user(&token, true).unwrap();
        assert_ne!(
            unsafe {
                windows_sys::Win32::Security::EqualSid(descriptor.owner().unwrap(), token.sid())
            },
            0
        );
    }

    #[test]
    fn relative_managed_creates_are_exclusive_and_owned_by_token_user() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();

        let directory =
            super::create_relative_managed_directory(&parent, OsStr::new("managed-directory"))
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
            super::create_relative_managed_directory(&parent, OsStr::new("managed-directory"),)
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
        assert!(
            super::create_relative_managed_file(
                &parent,
                OsStr::new("managed-directory"),
                super::ManagedFileAccess::Write,
            )
            .is_err()
        );
        assert!(
            super::create_relative_managed_directory(&parent, OsStr::new("managed-file")).is_err()
        );
        assert_eq!(
            super::encoded_component(OsStr::new("../escape"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput,
        );

        let link = temporary.path().join("managed-symlink");
        if std::os::windows::fs::symlink_file("not-a-target", &link).is_ok() {
            assert_eq!(
                super::create_relative_managed_file(
                    &parent,
                    OsStr::new("managed-symlink"),
                    super::ManagedFileAccess::Write,
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::AlreadyExists,
            );
        }
    }

    #[test]
    fn round_five_native_directory_create_is_a_pinned_rename_capability() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        let source_path = temporary.path().join("pinned-directory");
        let destination_path = temporary.path().join("moved-directory");
        let retained =
            super::create_relative_managed_directory(&parent, OsStr::new("pinned-directory"))
                .unwrap();
        let inspection = super::super::super::root::windows::open_directory_shared(
            &parent,
            OsStr::new("pinned-directory"),
        )
        .unwrap();

        assert!(
            std::fs::rename(&source_path, &destination_path).is_err(),
            "retained native directory capability allowed an independent rename"
        );
        assert!(
            std::fs::remove_dir(&source_path).is_err(),
            "retained native directory capability allowed an independent delete"
        );

        drop(inspection);
        drop(retained);
        std::fs::rename(&source_path, &destination_path).unwrap();
        std::fs::remove_dir(&destination_path).unwrap();
    }

    #[test]
    fn round_five_native_protocol_file_create_keeps_delete_but_denies_delete_sharing() {
        use std::io::Write as _;

        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        let source_path = temporary.path().join("pinned-protocol-file");
        let destination_path = temporary.path().join("moved-protocol-file");
        let mut retained = super::create_relative_managed_file(
            &parent,
            OsStr::new("pinned-protocol-file"),
            super::ManagedFileAccess::ReadWrite,
        )
        .unwrap();
        retained.write_all(b"protocol").unwrap();
        let inspection = super::super::super::root::windows::open_regular_file_shared(
            &parent,
            OsStr::new("pinned-protocol-file"),
        )
        .unwrap();

        assert!(
            std::fs::rename(&source_path, &destination_path).is_err(),
            "retained native protocol-file capability allowed an independent rename"
        );
        assert!(
            std::fs::remove_file(&source_path).is_err(),
            "retained native protocol-file capability allowed an independent delete"
        );

        drop(inspection);
        drop(retained);
        std::fs::rename(&source_path, &destination_path).unwrap();
        std::fs::remove_file(&destination_path).unwrap();
    }

    #[test]
    fn round_five_created_handle_identity_failure_rolls_back_the_exact_empty_directory() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        let source_path = temporary.path().join("identity-failure-directory");
        let replacement_path = temporary.path().join("replacement-attempt");
        let replacement_was_blocked = std::cell::Cell::new(false);

        let error = super::create_relative_managed_with_verifier(
            &parent,
            OsStr::new("identity-failure-directory"),
            super::ManagedEntryKind::Directory,
            |_, _, _| {
                let replacement = std::fs::rename(&source_path, &replacement_path);
                replacement_was_blocked.set(replacement.is_err());
                if replacement.is_ok() {
                    std::fs::rename(&replacement_path, &source_path).unwrap();
                }
                Err(io::Error::other("injected created-handle identity failure"))
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "injected created-handle identity failure"
        );
        assert!(
            replacement_was_blocked.get(),
            "the exact create handle did not pin the name during verification"
        );
        assert!(
            !source_path.exists(),
            "exact-handle rollback left the directory"
        );
        assert!(
            !replacement_path.exists(),
            "replacement attempt changed the name"
        );
    }

    #[test]
    fn relative_managed_component_rejects_non_components_and_device_names() {
        let invalid = [
            OsString::from(""),
            OsString::from("."),
            OsString::from(".."),
            OsString::from("a/b"),
            OsString::from("a\\b"),
            OsString::from("name:stream"),
            OsString::from("bad*name"),
            OsString::from("trailing."),
            OsString::from("NUL.txt"),
            OsString::from("COM¹"),
            OsString::from("CONOUT$.log"),
            OsString::from_wide(&[u16::from(b'a'), 0, u16::from(b'b')]),
        ];

        for name in invalid {
            assert_eq!(
                super::encoded_component(&name).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{name:?}",
            );
        }
    }

    #[test]
    fn relative_managed_wrappers_reject_invalid_names_without_changing_real_parent() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("sentinel"), b"unchanged").unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        let before = std::fs::read_dir(temporary.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();

        for name in [
            OsString::from(""),
            OsString::from(".."),
            OsString::from("a/b"),
            OsString::from("a\\b"),
            OsString::from("name:stream"),
            OsString::from("NUL.txt"),
        ] {
            assert_eq!(
                super::create_relative_managed_directory(&parent, &name)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput,
                "directory wrapper accepted {name:?}",
            );
            assert_eq!(
                super::create_relative_managed_file(
                    &parent,
                    &name,
                    super::ManagedFileAccess::Write,
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidInput,
                "file wrapper accepted {name:?}",
            );
        }

        let after = std::fs::read_dir(temporary.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(after, before);
        assert_eq!(
            std::fs::read(temporary.path().join("sentinel")).unwrap(),
            b"unchanged",
        );
    }

    #[test]
    fn relative_managed_verifier_failure_rolls_back_created_entries() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();

        for (name, kind) in [
            ("rolled-back-directory", super::ManagedEntryKind::Directory),
            (
                "rolled-back-file",
                super::ManagedEntryKind::File(super::ManagedFileAccess::Write),
            ),
        ] {
            let error = super::create_relative_managed_with_verifier(
                &parent,
                OsStr::new(name),
                kind,
                |_, _, _| Err(io::Error::other("injected verification failure")),
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert!(!temporary.path().join(name).exists(), "{name}");
        }
    }

    #[test]
    fn created_rollback_error_keeps_primary_failure_first() {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        let error = super::error_after_created_rollback_with(
            temporary.as_file(),
            io::Error::other("primary verification failure"),
            |_| Err(io::Error::other("secondary rollback failure")),
        );

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            error.to_string(),
            "primary verification failure; secondary created-object rollback failure: secondary rollback failure",
        );
    }

    #[test]
    fn existing_owner_mismatch_prevents_dacl_repair() {
        let mut dacl_writes = 0;
        let result = super::repair_existing_policy(
            || {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "owner mismatch",
                ))
            },
            || {
                dacl_writes += 1;
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(dacl_writes, 0);
    }

    #[test]
    fn mutation_barrier_dacl_is_restricted_to_the_required_trustees() {
        assert_eq!(
            super::mutation_barrier_sddl("S-1-5-21-123"),
            "D:P(A;;GA;;;S-1-5-21-123)(A;;GA;;;SY)(A;;GA;;;BA)"
        );
    }

    #[test]
    fn mutation_barrier_exists_exactly_while_its_owner_handle_is_live() {
        let run_id = Uuid::new_v4().to_string();
        assert!(!super::root_mutation_barrier_exists(&run_id).unwrap());

        let barrier = super::RootMutationBarrier::create(&run_id).unwrap();
        assert!(super::root_mutation_barrier_exists(&run_id).unwrap());

        drop(barrier);
        assert!(!super::root_mutation_barrier_exists(&run_id).unwrap());
    }
}
