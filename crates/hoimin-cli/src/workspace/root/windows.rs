use std::ffi::{OsStr, OsString, c_void};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr;

use camino::Utf8Path;
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT,
    FILE_RENAME_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT, FileRenameInformation, NtCreateFile,
    NtSetInformationFile,
};
use windows_sys::Win32::Foundation::{
    ERROR_NO_MORE_FILES, HANDLE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FileDispositionInfo, FileDispositionInfoEx,
    FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx, GetVolumeInformationByHandleW, READ_CONTROL, SYNCHRONIZE,
    SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

use super::{
    WindowsCreateDisposition, WindowsFinalOperation, WorkspaceError, make_file_writable,
    windows_final_name_units_are_valid,
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

pub(super) fn snapshot(
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
) -> Result<(Vec<u8>, std::fs::Permissions), WorkspaceError> {
    let mut file = open_final(parent, name, logical_path, WindowsFinalOperation::Read)?;
    let permissions = file
        .metadata()
        .map_err(|error| WorkspaceError::io("verify restored file", logical_path, error))?
        .permissions();
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(|error| WorkspaceError::io("verify restored file", logical_path, error))?;
    Ok((contents, permissions))
}

pub(super) fn set_permissions(
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
    permissions: std::fs::Permissions,
) -> Result<(), WorkspaceError> {
    let file = open_final(
        parent,
        name,
        logical_path,
        WindowsFinalOperation::InspectForWrite,
    )?;
    file.set_permissions(permissions)
        .map_err(|error| WorkspaceError::io("restore worker permissions", logical_path, error))
}

pub(super) fn open_mutation_file(
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
) -> Result<File, WorkspaceError> {
    open_final(
        parent,
        name,
        logical_path,
        WindowsFinalOperation::InspectMutation,
    )
}

pub(super) fn reopen_mutation_file(
    inspected: &File,
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
) -> Result<File, WorkspaceError> {
    let writable = open_final(
        parent,
        name,
        logical_path,
        WindowsFinalOperation::WriteMutation,
    )?;
    if same_file_identity(inspected, &writable, logical_path)? {
        Ok(writable)
    } else {
        Err(WorkspaceError::InvalidPath {
            path: logical_path.to_owned(),
        })
    }
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

pub(super) fn remove_entry(
    parent: &File,
    name: &OsString,
    logical_path: &Utf8Path,
) -> Result<(), WorkspaceError> {
    remove_entry_io(parent, name)
        .map_err(|error| WorkspaceError::io("remove worker entry", logical_path, error))
}

pub(crate) fn remove_entry_io(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<()> {
    remove_entry_io_with_guard(parent, name, &|| Ok(()))
}

pub(crate) fn remove_entry_io_with_guard(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    before_next_operation: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    if !windows_final_name_units_are_valid(&encode_final_name(name)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid final path component",
        ));
    }
    before_next_operation()?;
    let file = open_final_handle(parent, name, WindowsFinalOperation::RemoveEntry)?;
    remove_open_entry_io_with_guard(&file, before_next_operation)
}

pub(crate) fn remove_entry_io_checked(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    expected_identity: (u64, u64),
) -> io::Result<()> {
    remove_entry_io_checked_with_guard(parent, name, expected_identity, &|| Ok(()))
}

pub(crate) fn remove_entry_io_checked_with_guard(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    expected_identity: (u64, u64),
    before_next_operation: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    if !windows_final_name_units_are_valid(&encode_final_name(name)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid final path component",
        ));
    }
    before_next_operation()?;
    let file = open_final_handle(parent, name, WindowsFinalOperation::RemoveEntry)?;
    before_next_operation()?;
    if file_identity_io(&file)? != expected_identity {
        return Err(io::Error::other("remove target identity changed"));
    }
    remove_open_entry_io_with_guard(&file, before_next_operation)
}

pub(crate) fn remove_open_entry_io_with_guard(
    file: &File,
    before_next_operation: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    before_next_operation()?;
    let mut permissions = file.metadata()?.permissions();
    before_next_operation()?;
    if permissions.readonly() {
        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "Windows Permissions toggles only the FILE_ATTRIBUTE_READONLY bit"
        )]
        permissions.set_readonly(false);
        file.set_permissions(permissions)?;
        before_next_operation()?;
    }
    before_next_operation()?;
    mark_delete_by_handle(&file)
}

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

pub(crate) fn open_directory_shared(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    open_directory_with_access(
        parent,
        name,
        SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
        NativeSharePolicy::Shared,
    )
}

pub(crate) fn open_directory_for_rename(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    open_directory_with_access(
        parent,
        name,
        DELETE
            | SYNCHRONIZE
            | READ_CONTROL
            | FILE_READ_ATTRIBUTES
            | FILE_WRITE_ATTRIBUTES
            | FILE_LIST_DIRECTORY,
        NativeSharePolicy::Pinned,
    )
}

fn open_directory_with_access(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    desired_access: u32,
    share_policy: NativeSharePolicy,
) -> io::Result<File> {
    let file = open_relative(
        parent,
        name,
        desired_access,
        FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        share_policy,
    )?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other(
            "managed directory is not a real directory",
        ));
    }
    Ok(file)
}

pub(crate) fn open_entry_shared(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    open_relative(
        parent,
        name,
        SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        NativeSharePolicy::Shared,
    )
}

pub(crate) fn open_regular_file_shared(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    let file = open_relative(
        parent,
        name,
        SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_READ_DATA,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        NativeSharePolicy::Shared,
    )?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("managed entry is not a regular file"));
    }
    Ok(file)
}

#[cfg(test)]
pub(crate) fn open_regular_file_for_update_shared(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    let file = open_relative(
        parent,
        name,
        SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_READ_DATA | FILE_WRITE_ATTRIBUTES,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        NativeSharePolicy::Shared,
    )?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("managed entry is not a regular file"));
    }
    Ok(file)
}

pub(crate) fn open_regular_file_for_update_pinned(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    let file = open_relative(
        parent,
        name,
        SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_READ_DATA | FILE_WRITE_ATTRIBUTES,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        NativeSharePolicy::Pinned,
    )?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("managed entry is not a regular file"));
    }
    Ok(file)
}

pub(crate) fn open_regular_file_for_delete_pinned(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
) -> io::Result<File> {
    let file = open_relative(
        parent,
        name,
        DELETE
            | SYNCHRONIZE
            | READ_CONTROL
            | FILE_READ_ATTRIBUTES
            | FILE_WRITE_ATTRIBUTES
            | FILE_READ_DATA,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        NativeSharePolicy::Pinned,
    )?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("managed entry is not a regular file"));
    }
    Ok(file)
}

pub(crate) fn rename_open_entry_relative_with_guard(
    parent: &(impl AsRawHandle + ?Sized),
    source: &(impl AsRawHandle + ?Sized),
    destination: &OsStr,
    expected_identity: (u64, u64),
    before_next_operation: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    if !windows_final_name_units_are_valid(&encode_final_name(destination)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid destination path component",
        ));
    }
    before_next_operation()?;
    if file_identity_io(source)? != expected_identity {
        return Err(io::Error::other("rename source identity changed"));
    }
    before_next_operation()?;
    let destination = encode_final_name(destination);
    let name_bytes = destination
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or_else(|| io::Error::other("rename destination length overflow"))?;
    let header_len = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
    let buffer_bytes = header_len
        .checked_add(name_bytes)
        .ok_or_else(|| io::Error::other("rename buffer length overflow"))?;
    let mut buffer = vec![0_usize; buffer_bytes.div_ceil(size_of::<usize>())];
    let rename = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // SAFETY: buffer is suitably aligned and sized for the fixed header and destination name.
    unsafe {
        (*rename).Anonymous.ReplaceIfExists = false;
        (*rename).RootDirectory = parent.as_raw_handle() as HANDLE;
        (*rename).FileNameLength = u32::try_from(name_bytes)
            .map_err(|_| io::Error::other("rename destination length overflow"))?;
        ptr::copy_nonoverlapping(
            destination.as_ptr(),
            (*rename).FileName.as_mut_ptr(),
            destination.len(),
        );
    }
    before_next_operation()?;
    // SAFETY: source is live, io_status is writable, and buffer contains a correctly sized
    // FILE_RENAME_INFORMATION record whose target is anchored to the live parent handle.
    let status = unsafe {
        let mut io_status: IO_STATUS_BLOCK = zeroed();
        NtSetInformationFile(
            source.as_raw_handle() as HANDLE,
            ptr::from_mut(&mut io_status),
            buffer.as_ptr().cast(),
            u32::try_from(buffer_bytes)
                .map_err(|_| io::Error::other("rename buffer length overflow"))?,
            FileRenameInformation,
        )
    };
    if status < 0 {
        Err(io_error_from_ntstatus(status))
    } else {
        Ok(())
    }
}

pub(crate) fn file_identity_io(file: &(impl AsRawHandle + ?Sized)) -> io::Result<(u64, u64)> {
    let volume_serial = supported_volume_serial(file)?;
    // SAFETY: information is writable and file owns a live handle.
    let information = unsafe {
        let mut information: BY_HANDLE_FILE_INFORMATION = zeroed();
        if GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &raw mut information) == 0 {
            return Err(io::Error::last_os_error());
        }
        information
    };
    if information.dwVolumeSerialNumber != volume_serial {
        return Err(io::Error::other(
            "volume identity changed while inspecting managed object",
        ));
    }
    Ok((
        u64::from(volume_serial),
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}

fn supported_volume_serial(file: &(impl AsRawHandle + ?Sized)) -> io::Result<u32> {
    let mut volume_serial = 0_u32;
    let mut filesystem_name = [0_u16; 32];
    // SAFETY: file owns a live handle and all supplied output buffers are writable.
    if unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle() as HANDLE,
            ptr::null_mut(),
            0,
            &raw mut volume_serial,
            ptr::null_mut(),
            ptr::null_mut(),
            filesystem_name.as_mut_ptr(),
            u32::try_from(filesystem_name.len()).expect("fixed filesystem-name capacity"),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let length = filesystem_name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(filesystem_name.len());
    if String::from_utf16_lossy(&filesystem_name[..length]).eq_ignore_ascii_case("refs") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "ReFS requires a 128-bit file identity and is not supported for managed workspaces",
        ));
    }
    Ok(volume_serial)
}

fn open_relative(
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    desired_access: u32,
    create_options: u32,
    share_policy: NativeSharePolicy,
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
            FILE_OPEN,
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

const DIRECTORY_ENTRY_BUFFER_BYTES: usize = 64 * 1024;

pub(crate) struct DirectoryEntryInfo {
    name: OsString,
    file_attributes: u32,
    file_id: i64,
}

impl DirectoryEntryInfo {
    pub(crate) fn name(&self) -> &OsStr {
        &self.name
    }

    pub(crate) fn into_name(self) -> OsString {
        self.name
    }

    pub(crate) const fn file_attributes(&self) -> u32 {
        self.file_attributes
    }

    pub(crate) const fn file_id(&self) -> u64 {
        u64::from_ne_bytes(self.file_id.to_ne_bytes())
    }
}

pub(crate) struct DirectoryEntries {
    directory: File,
    buffer: Vec<usize>,
    offset: usize,
    needs_refill: bool,
    restart: bool,
    done: bool,
}

impl DirectoryEntries {
    pub(crate) fn open(directory: File) -> Self {
        Self {
            directory,
            buffer: vec![0; DIRECTORY_ENTRY_BUFFER_BYTES.div_ceil(size_of::<usize>())],
            offset: 0,
            needs_refill: true,
            restart: true,
            done: false,
        }
    }

    pub(crate) fn directory(&self) -> &File {
        &self.directory
    }

    fn refill(&mut self) -> io::Result<()> {
        self.buffer.fill(0);
        let byte_len = self
            .buffer
            .len()
            .checked_mul(size_of::<usize>())
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| io::Error::other("directory entry buffer length overflow"))?;
        let information_class = if self.restart {
            FileIdBothDirectoryRestartInfo
        } else {
            FileIdBothDirectoryInfo
        };
        // SAFETY: buffer is aligned and writable for byte_len, and directory owns a live handle.
        if unsafe {
            GetFileInformationByHandleEx(
                self.directory.as_raw_handle() as HANDLE,
                information_class,
                self.buffer.as_mut_ptr().cast(),
                byte_len,
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error()
                == Some(i32::try_from(ERROR_NO_MORE_FILES).expect("error code fits i32"))
            {
                self.done = true;
                return Ok(());
            }
            return Err(error);
        }
        self.offset = 0;
        self.needs_refill = false;
        self.restart = false;
        Ok(())
    }

    pub(crate) fn next_entry(&mut self) -> io::Result<Option<DirectoryEntryInfo>> {
        if self.done {
            return Ok(None);
        }
        if self.needs_refill {
            self.refill()?;
            if self.done {
                return Ok(None);
            }
        }
        let bytes = self.buffer.len() * size_of::<usize>();
        let header_len = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
        if self
            .offset
            .checked_add(header_len)
            .is_none_or(|end| end > bytes)
        {
            return Err(io::Error::other("invalid directory enumeration record"));
        }
        // SAFETY: offset and fixed header bounds were checked; read_unaligned accepts any offset.
        let record = unsafe {
            self.buffer
                .as_ptr()
                .cast::<u8>()
                .add(self.offset)
                .cast::<FILE_ID_BOTH_DIR_INFO>()
                .read_unaligned()
        };
        let name_bytes = usize::try_from(record.FileNameLength)
            .map_err(|_| io::Error::other("directory entry name length overflow"))?;
        if name_bytes % size_of::<u16>() != 0
            || self
                .offset
                .checked_add(header_len)
                .and_then(|start| start.checked_add(name_bytes))
                .is_none_or(|end| end > bytes)
        {
            return Err(io::Error::other("invalid directory entry name length"));
        }
        let name_units = name_bytes / size_of::<u16>();
        let name_start = self.offset + header_len;
        let mut name = Vec::with_capacity(name_units);
        for index in 0..name_units {
            // SAFETY: every two-byte unit lies within the validated variable-length name range.
            name.push(unsafe {
                self.buffer
                    .as_ptr()
                    .cast::<u8>()
                    .add(name_start + index * size_of::<u16>())
                    .cast::<u16>()
                    .read_unaligned()
            });
        }
        let next = usize::try_from(record.NextEntryOffset)
            .map_err(|_| io::Error::other("directory entry offset overflow"))?;
        if next == 0 {
            self.needs_refill = true;
        } else {
            if next < header_len || self.offset.checked_add(next).is_none_or(|end| end >= bytes) {
                return Err(io::Error::other("invalid next directory entry offset"));
            }
            self.offset += next;
        }
        Ok(Some(DirectoryEntryInfo {
            name: OsString::from_wide(&name),
            file_attributes: record.FileAttributes,
            file_id: record.FileId,
        }))
    }
}

impl Iterator for DirectoryEntries {
    type Item = io::Result<DirectoryEntryInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = match self.next_entry() {
                Ok(Some(entry)) => entry,
                Ok(None) => return None,
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            };
            if entry.name() != "." && entry.name() != ".." {
                return Some(Ok(entry));
            }
        }
    }
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
    parent: &(impl AsRawHandle + ?Sized),
    name: &OsStr,
    operation: WindowsFinalOperation,
) -> Result<File, io::Error> {
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
    // SAFETY: `attributes` points to live stack values, `wide` remains allocated for the call,
    // the output handle pointer and IO status block are writable, and no optional buffers are used.
    let status = unsafe {
        let mut io_status: IO_STATUS_BLOCK = zeroed();
        NtCreateFile(
            ptr::from_mut(&mut handle),
            desired_access(operation),
            ptr::from_ref(&attributes),
            ptr::from_mut(&mut io_status),
            ptr::null(),
            0,
            final_operation_share_policy(operation).access(),
            match operation.create_disposition() {
                WindowsCreateDisposition::Open => FILE_OPEN,
                WindowsCreateDisposition::OpenIf => FILE_OPEN_IF,
            },
            create_options(operation),
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(io_error_from_ntstatus(status));
    }
    // SAFETY: successful `NtCreateFile` returns one newly owned kernel handle.
    Ok(unsafe { File::from_raw_handle(handle.cast()) })
}

const fn final_operation_share_policy(operation: WindowsFinalOperation) -> NativeSharePolicy {
    match operation {
        WindowsFinalOperation::Remove | WindowsFinalOperation::RemoveEntry => {
            NativeSharePolicy::Pinned
        }
        WindowsFinalOperation::InspectForWrite
        | WindowsFinalOperation::InspectMutation
        | WindowsFinalOperation::WriteMutation
        | WindowsFinalOperation::Read
        | WindowsFinalOperation::Write => NativeSharePolicy::Shared,
    }
}

const fn create_options(operation: WindowsFinalOperation) -> u32 {
    let common = FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT;
    if matches!(operation, WindowsFinalOperation::RemoveEntry) {
        common
    } else {
        common | FILE_NON_DIRECTORY_FILE
    }
}

fn validate_final_name(name: &OsStr, logical_path: &Utf8Path) -> Result<(), WorkspaceError> {
    if windows_final_name_units_are_valid(&encode_final_name(name)) {
        Ok(())
    } else {
        Err(WorkspaceError::InvalidPath {
            path: logical_path.to_owned(),
        })
    }
}

fn encode_final_name(name: &OsStr) -> Vec<u16> {
    name.encode_wide().collect()
}

fn validate_opened_file(file: File, logical_path: &Utf8Path) -> Result<File, WorkspaceError> {
    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect worker file", logical_path, error))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_file() {
        return Err(WorkspaceError::InvalidPath {
            path: logical_path.to_owned(),
        });
    }
    Ok(file)
}

fn same_file_identity(
    inspected: &File,
    writable: &File,
    logical_path: &Utf8Path,
) -> Result<bool, WorkspaceError> {
    Ok(file_identity(inspected, logical_path)? == file_identity(writable, logical_path)?)
}

fn file_identity(file: &File, logical_path: &Utf8Path) -> Result<(u32, u64), WorkspaceError> {
    let (volume, id) = file_identity_io(file)
        .map_err(|error| WorkspaceError::io("inspect mutation target", logical_path, error))?;
    Ok((
        u32::try_from(volume).expect("Windows volume serial is a u32"),
        id,
    ))
}

const fn desired_access(operation: WindowsFinalOperation) -> u32 {
    let common = SYNCHRONIZE | FILE_READ_ATTRIBUTES;
    match operation {
        WindowsFinalOperation::InspectForWrite => common | FILE_WRITE_ATTRIBUTES,
        WindowsFinalOperation::InspectMutation => common | FILE_READ_DATA | FILE_WRITE_ATTRIBUTES,
        WindowsFinalOperation::WriteMutation | WindowsFinalOperation::Write => {
            common | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES
        }
        WindowsFinalOperation::Read => common | FILE_READ_DATA,
        WindowsFinalOperation::Remove | WindowsFinalOperation::RemoveEntry => {
            common | DELETE | FILE_WRITE_ATTRIBUTES
        }
    }
}

const fn operation_name(operation: WindowsFinalOperation) -> &'static str {
    match operation {
        WindowsFinalOperation::InspectForWrite => "prepare worker file",
        WindowsFinalOperation::InspectMutation | WindowsFinalOperation::WriteMutation => {
            "open mutation target"
        }
        WindowsFinalOperation::Read => "read worker file",
        WindowsFinalOperation::Write => "write worker file",
        WindowsFinalOperation::Remove => "remove worker file",
        WindowsFinalOperation::RemoveEntry => "remove worker entry",
    }
}

fn io_error_from_ntstatus(status: i32) -> io::Error {
    // SAFETY: `RtlNtStatusToDosError` accepts every NTSTATUS value.
    let code = unsafe { RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(i32::try_from(code).unwrap_or(i32::MAX))
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
            u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>())
                .expect("FILE_DISPOSITION_INFO_EX size fits in u32"),
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
            u32::try_from(size_of::<FILE_DISPOSITION_INFO>())
                .expect("FILE_DISPOSITION_INFO size fits in u32"),
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
    fn final_name_encoding_preserves_a_lone_surrogate() {
        use std::os::windows::ffi::OsStringExt;

        let name = OsString::from_wide(&[u16::from(b'x'), 0xD800]);

        assert_eq!(encode_final_name(&name), vec![u16::from(b'x'), 0xD800]);
    }

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
        assert_ne!(
            desired_access(WindowsFinalOperation::InspectMutation) & FILE_READ_DATA,
            0
        );
        assert_eq!(
            desired_access(WindowsFinalOperation::InspectMutation) & FILE_WRITE_DATA,
            0
        );
        assert_ne!(
            desired_access(WindowsFinalOperation::InspectMutation) & FILE_WRITE_ATTRIBUTES,
            0
        );
        assert_ne!(
            desired_access(WindowsFinalOperation::WriteMutation) & FILE_WRITE_DATA,
            0
        );
        assert_eq!(desired_access(WindowsFinalOperation::Read) & DELETE, 0);
        assert_eq!(desired_access(WindowsFinalOperation::Write) & DELETE, 0);
        assert_ne!(desired_access(WindowsFinalOperation::Remove) & DELETE, 0);
        assert_ne!(
            desired_access(WindowsFinalOperation::RemoveEntry) & DELETE,
            0
        );
        assert_ne!(
            desired_access(WindowsFinalOperation::RemoveEntry) & FILE_WRITE_ATTRIBUTES,
            0
        );
        assert_ne!(
            desired_access(WindowsFinalOperation::Write) & FILE_WRITE_DATA,
            0
        );
        assert_eq!(
            create_options(WindowsFinalOperation::RemoveEntry) & FILE_NON_DIRECTORY_FILE,
            0
        );
        assert_ne!(
            create_options(WindowsFinalOperation::RemoveEntry) & FILE_OPEN_REPARSE_POINT,
            0
        );
    }

    #[test]
    fn shared_descendant_allows_late_delete_open_but_blocks_rename() {
        use std::os::windows::fs::OpenOptionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        parent.create_dir("source").unwrap();
        parent.write("source/child", b"content").unwrap();
        let child = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(temporary.path().join("source/child"))
            .unwrap();

        let source = open_directory_for_rename(&parent, OsStr::new("source")).unwrap();
        let expected_identity = file_identity_io(&source).unwrap();
        let blocked = rename_open_entry_relative_with_guard(
            &parent,
            &source,
            OsStr::new("destination"),
            expected_identity,
            &|| Ok(()),
        );
        assert!(
            blocked.is_err(),
            "Windows renamed a directory while a shared descendant handle was live"
        );
        drop(child);
        rename_open_entry_relative_with_guard(
            &parent,
            &source,
            OsStr::new("destination"),
            expected_identity,
            &|| Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn nonsharing_directory_handle_blocks_late_delete_open() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };

        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        parent.create_dir("source").unwrap();
        let inspection = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(temporary.path().join("source"))
            .unwrap();

        assert!(
            open_directory_for_rename(&parent, OsStr::new("source")).is_err(),
            "Windows granted DELETE access while the same directory denied delete sharing"
        );
        drop(inspection);
        open_directory_for_rename(&parent, OsStr::new("source")).unwrap();
    }

    #[test]
    fn round_five_relative_rename_source_open_is_pinned_but_still_inspectable() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        parent.create_dir("source").unwrap();
        let retained = open_directory_for_rename(&parent, OsStr::new("source")).unwrap();
        let inspection = open_directory_shared(&parent, OsStr::new("source")).unwrap();

        assert!(
            open_directory_for_rename(&parent, OsStr::new("source")).is_err(),
            "a retained rename-source capability admitted a second DELETE-capable open"
        );
        assert_eq!(
            file_identity_io(&retained).unwrap(),
            file_identity_io(&inspection).unwrap()
        );

        drop(inspection);
        drop(retained);
        open_directory_for_rename(&parent, OsStr::new("source")).unwrap();
    }

    #[test]
    fn retained_delete_handle_renames_with_an_inspection_handle_live() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        parent.create_dir("source").unwrap();
        let source = open_directory_for_rename(&parent, OsStr::new("source")).unwrap();
        let expected_identity = file_identity_io(&source).unwrap();
        let inspection = open_directory_shared(&parent, OsStr::new("source")).unwrap();

        rename_open_entry_relative_with_guard(
            &parent,
            &source,
            OsStr::new("destination"),
            expected_identity,
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(file_identity_io(&inspection).unwrap(), expected_identity);
        assert!(parent.symlink_metadata("source").is_err());
        assert!(parent.symlink_metadata("destination").unwrap().is_dir());
    }

    #[test]
    fn retained_delete_handle_waits_for_a_nonsharing_descendant_handle() {
        use std::os::windows::fs::OpenOptionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        parent.create_dir("source").unwrap();
        parent.write("source/child", b"content").unwrap();
        let source = open_directory_for_rename(&parent, OsStr::new("source")).unwrap();
        let expected_identity = file_identity_io(&source).unwrap();
        let child = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(temporary.path().join("source/child"))
            .unwrap();

        let blocked = rename_open_entry_relative_with_guard(
            &parent,
            &source,
            OsStr::new("destination"),
            expected_identity,
            &|| Ok(()),
        );

        assert!(
            blocked.is_err(),
            "Windows renamed a directory while a nonsharing descendant handle was live"
        );
        drop(child);
        rename_open_entry_relative_with_guard(
            &parent,
            &source,
            OsStr::new("destination"),
            expected_identity,
            &|| Ok(()),
        )
        .unwrap();
        assert!(parent.symlink_metadata("source").is_err());
        assert!(parent.symlink_metadata("destination").unwrap().is_dir());
    }

    #[test]
    fn round_six_checked_remove_entry_blocks_replacement_after_identity_validation() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        parent.write("source", b"original").unwrap();
        let original = open_entry_shared(&parent, OsStr::new("source")).unwrap();
        let original_identity = file_identity_io(&original).unwrap();
        drop(original);
        let hook_calls = std::cell::Cell::new(0_u8);
        let rename_blocked = std::cell::Cell::new(false);
        let replacement_installed = std::cell::Cell::new(false);

        remove_entry_io_checked_with_guard(
            &parent,
            OsStr::new("source"),
            original_identity,
            &|| {
                let call = hook_calls.get().saturating_add(1);
                hook_calls.set(call);
                if call == 3 {
                    match parent.rename("source", &parent, "parked") {
                        Ok(()) => {
                            parent.write("source", b"replacement")?;
                            replacement_installed.set(true);
                        }
                        Err(_) => rename_blocked.set(true),
                    }
                }
                Ok(())
            },
        )
        .unwrap();

        assert!(
            rename_blocked.get(),
            "the checked DELETE handle allowed rename"
        );
        assert!(
            !replacement_installed.get(),
            "a same-name replacement was installed during checked deletion"
        );
        assert!(parent.symlink_metadata("source").is_err());
        assert!(parent.symlink_metadata("parked").is_err());
    }

    #[test]
    fn round_six_remove_handle_is_pinned_but_shared_inspection_stays_compatible() {
        let temporary = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority())
                .unwrap();
        parent.write("source", b"content").unwrap();
        let removal =
            open_final_handle(&parent, OsStr::new("source"), WindowsFinalOperation::Remove)
                .unwrap();
        let inspection = open_entry_shared(&parent, OsStr::new("source")).unwrap();

        assert_eq!(
            file_identity_io(&removal).unwrap(),
            file_identity_io(&inspection).unwrap()
        );
        assert!(
            parent.rename("source", &parent, "destination").is_err(),
            "a live Remove handle allowed an independent rename"
        );
        assert!(
            parent.remove_file("source").is_err(),
            "a live Remove handle allowed an independent delete"
        );

        drop(inspection);
        drop(removal);
        parent.rename("source", &parent, "destination").unwrap();
        parent.remove_file("destination").unwrap();
    }
}
