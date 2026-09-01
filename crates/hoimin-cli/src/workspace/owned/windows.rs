use std::ffi::{OsStr, c_void};
use std::fs::File;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr;

use camino::Utf8Path;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER,
    ERROR_SUCCESS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT, SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, EqualSid, GetSecurityDescriptorControl,
    GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, GetTokenInformation,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSID, SE_DACL_PROTECTED,
    SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING, READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
};
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
    verify_owner(&secured, &token)?;
    let expected_descriptor = SecurityDescriptor::for_user(&token, directory)?;
    let expected_dacl = expected_descriptor.dacl()?;
    // SAFETY: secured has READ_CONTROL|WRITE_DAC and expected_dacl belongs to a live descriptor.
    let status = unsafe {
        SetSecurityInfo(
            secured.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            expected_dacl,
            ptr::null(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(i32::MAX),
        ));
    }
    verify_security(&secured, &token, expected_dacl)?;
    if file_identity(&secured)? != file_identity(expected)? {
        return Err(io::Error::other(
            "managed object identity changed while securing it",
        ));
    }
    Ok(())
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
