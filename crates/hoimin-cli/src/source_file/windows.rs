//! Native handle-relative opening shared by source reads and workspace operations.
use super::windows_final_name_units_are_valid;
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr;
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
    NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    HANDLE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeSharePolicy {
    Shared,
    Pinned,
}

impl NativeSharePolicy {
    pub(crate) const fn access(self) -> u32 {
        match self {
            Self::Shared => FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            Self::Pinned => FILE_SHARE_READ | FILE_SHARE_WRITE,
        }
    }
}

pub(crate) fn open_relative(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    desired_access: u32,
    create_options: u32,
    share_policy: NativeSharePolicy,
    disposition: u32,
) -> io::Result<File> {
    if !windows_final_name_units_are_valid(&encode_final_name(name)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid final path component",
        ));
    }
    let mut wide = encode_final_name(name);
    let byte_len = wide
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "file name is too long"))?;
    let unicode_name = UNICODE_STRING {
        Length: byte_len,
        MaximumLength: byte_len,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>())
            .expect("OBJECT_ATTRIBUTES size fits in u32"),
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: ptr::from_ref(&unicode_name),
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut handle: HANDLE = ptr::null_mut();
    // SAFETY: all pointers reference live stack storage and no optional buffers are used.
    let status = unsafe {
        let mut io_status: IO_STATUS_BLOCK = zeroed();
        NtCreateFile(
            ptr::from_mut(&mut handle),
            desired_access,
            ptr::from_ref(&attributes),
            ptr::from_mut(&mut io_status),
            ptr::null(),
            0,
            share_policy.access(),
            disposition,
            create_options,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(io_error_from_ntstatus(status));
    }
    // SAFETY: successful NtCreateFile returns one newly owned handle.
    Ok(unsafe { File::from_raw_handle(handle.cast()) })
}

pub(super) fn open_read(parent: &File, name: &OsStr) -> io::Result<File> {
    open_relative(
        parent,
        name,
        SYNCHRONIZE | FILE_READ_ATTRIBUTES | FILE_READ_DATA,
        FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT | FILE_NON_DIRECTORY_FILE,
        NativeSharePolicy::Shared,
        FILE_OPEN,
    )
}

pub(crate) fn encode_final_name(name: &OsStr) -> Vec<u16> {
    name.encode_wide().collect()
}

pub(crate) fn io_error_from_ntstatus(status: i32) -> io::Error {
    // SAFETY: RtlNtStatusToDosError accepts every NTSTATUS value.
    let code = unsafe { RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(i32::try_from(code).unwrap_or(i32::MAX))
}
