use std::fs::File;
use std::io;

#[cfg(unix)]
pub(crate) fn file_link_count(file: &File) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;

    Ok(file.metadata()?.nlink())
}

#[cfg(windows)]
pub(crate) fn file_link_count(file: &File) -> io::Result<u64> {
    use std::mem::zeroed;
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    // SAFETY: information is writable and file owns a live handle.
    let information = unsafe {
        let mut information: BY_HANDLE_FILE_INFORMATION = zeroed();
        if GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &raw mut information) == 0 {
            return Err(io::Error::last_os_error());
        }
        information
    };
    Ok(u64::from(information.nNumberOfLinks))
}
