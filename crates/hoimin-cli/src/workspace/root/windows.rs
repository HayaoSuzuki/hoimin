use std::ffi::{OsStr, OsString, c_void};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr;

use camino::Utf8Path;
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    HANDLE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
    FileDispositionInfo, FileDispositionInfoEx, SYNCHRONIZE, SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

use super::{
    WindowsCreateDisposition, WindowsFinalOperation, WorkspaceError, make_file_writable,
    windows_final_name_is_valid,
};

pub(super) fn read(
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
) -> Result<Vec<u8>, WorkspaceError> {
    let mut file = open_final(parent, name, logical_path, WindowsFinalOperation::Read)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(|error| WorkspaceError::io("read worker file", logical_path, error))?;
    Ok(contents)
}

pub(super) fn write(
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
    contents: &[u8],
) -> Result<(), WorkspaceError> {
    if let Some(file) = open_final_if_present(
        parent,
        name,
        logical_path,
        WindowsFinalOperation::InspectForWrite,
    )? {
        make_file_writable(&file, logical_path)?;
    }
    let mut file = open_final(parent, name, logical_path, WindowsFinalOperation::Write)?;
    file.set_len(0)
        .map_err(|error| WorkspaceError::io("truncate worker file", logical_path, error))?;
    file.write_all(contents)
        .map_err(|error| WorkspaceError::io("write worker file", logical_path, error))
}

pub(super) fn remove_file(
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
) -> Result<(), WorkspaceError> {
    let file = open_final(parent, name, logical_path, WindowsFinalOperation::Remove)?;
    make_file_writable(&file, logical_path)?;
    mark_delete_by_handle(&file)
        .map_err(|error| WorkspaceError::io("remove worker file", logical_path, error))
}

fn open_final(
    parent: &File,
    name: &OsStr,
    logical_path: &Utf8Path,
    operation: WindowsFinalOperation,
) -> Result<File, WorkspaceError> {
    validate_final_name(name, logical_path)?;
    match open_final_handle(parent, name, operation) {
        Ok(file) => validate_opened_file(file, logical_path),
        Err(error) => Err(WorkspaceError::io(
            operation_name(operation),
            logical_path,
            error,
        )),
    }
}

fn open_final_if_present(
    parent: &File,
    name: &OsStr,
    logical_path: &Utf8Path,
    operation: WindowsFinalOperation,
) -> Result<Option<File>, WorkspaceError> {
    validate_final_name(name, logical_path)?;
    match open_final_handle(parent, name, operation) {
        Ok(file) => validate_opened_file(file, logical_path).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(WorkspaceError::io(
            operation_name(operation),
            logical_path,
            error,
        )),
    }
}

fn open_final_handle(
    parent: &File,
    name: &OsStr,
    operation: WindowsFinalOperation,
) -> Result<File, io::Error> {
    let name = name.to_string_lossy();
    let mut wide = OsStr::new(name.as_ref()).encode_wide().collect::<Vec<_>>();
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
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: &unicode_name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut handle: HANDLE = ptr::null_mut();
    // SAFETY: `attributes` points to live stack values, `wide` remains allocated for the call,
    // the output handle pointer and IO status block are writable, and no optional buffers are used.
    let status = unsafe {
        let mut io_status: IO_STATUS_BLOCK = zeroed();
        NtCreateFile(
            &mut handle,
            desired_access(operation),
            &attributes,
            &mut io_status,
            ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            match operation.create_disposition() {
                WindowsCreateDisposition::Open => FILE_OPEN,
                WindowsCreateDisposition::OpenIf => FILE_OPEN_IF,
            },
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(io_error_from_ntstatus(status));
    }
    // SAFETY: successful `NtCreateFile` returns one newly owned kernel handle.
    Ok(unsafe { File::from_raw_handle(handle as _) })
}

fn validate_final_name(name: &OsStr, logical_path: &Utf8Path) -> Result<(), WorkspaceError> {
    if windows_final_name_is_valid(&name.to_string_lossy()) {
        Ok(())
    } else {
        Err(WorkspaceError::InvalidPath {
            path: logical_path.to_owned(),
        })
    }
}

fn validate_opened_file(file: File, logical_path: &Utf8Path) -> Result<File, WorkspaceError> {
    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect worker file", logical_path, error))?;
    use std::os::windows::fs::MetadataExt;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_file() {
        return Err(WorkspaceError::InvalidPath {
            path: logical_path.to_owned(),
        });
    }
    Ok(file)
}

const fn desired_access(operation: WindowsFinalOperation) -> u32 {
    let common = SYNCHRONIZE | FILE_READ_ATTRIBUTES;
    match operation {
        WindowsFinalOperation::InspectForWrite => common | FILE_WRITE_ATTRIBUTES,
        WindowsFinalOperation::Read => common | FILE_READ_DATA,
        WindowsFinalOperation::Write => common | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES,
        WindowsFinalOperation::Remove => common | DELETE | FILE_WRITE_ATTRIBUTES,
    }
}

const fn operation_name(operation: WindowsFinalOperation) -> &'static str {
    match operation {
        WindowsFinalOperation::InspectForWrite => "prepare worker file",
        WindowsFinalOperation::Read => "read worker file",
        WindowsFinalOperation::Write => "write worker file",
        WindowsFinalOperation::Remove => "remove worker file",
    }
}

fn io_error_from_ntstatus(status: i32) -> io::Error {
    // SAFETY: `RtlNtStatusToDosError` accepts every NTSTATUS value.
    io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32)
}

fn mark_delete_by_handle(file: &File) -> io::Result<()> {
    let extended = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    // SAFETY: the handle is valid for the call and the typed input buffer has the exact size
    // required by `FileDispositionInfoEx`.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileDispositionInfoEx,
            ptr::from_ref(&extended).cast::<c_void>(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    } != 0
    {
        return Ok(());
    }

    let legacy = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: this is a handle-only fallback with a correctly sized legacy disposition buffer.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileDispositionInfo,
            ptr::from_ref(&legacy).cast::<c_void>(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } != 0
    {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_profiles_match_operation_side_effects() {
        assert_eq!(
            desired_access(WindowsFinalOperation::InspectForWrite) & FILE_WRITE_DATA,
            0
        );
        assert_ne!(
            desired_access(WindowsFinalOperation::InspectForWrite) & FILE_WRITE_ATTRIBUTES,
            0
        );
        assert_eq!(desired_access(WindowsFinalOperation::Read) & DELETE, 0);
        assert_eq!(desired_access(WindowsFinalOperation::Write) & DELETE, 0);
        assert_ne!(desired_access(WindowsFinalOperation::Remove) & DELETE, 0);
        assert_ne!(
            desired_access(WindowsFinalOperation::Write) & FILE_WRITE_DATA,
            0
        );
    }
}
