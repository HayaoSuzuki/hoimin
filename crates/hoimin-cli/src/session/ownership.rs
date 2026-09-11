use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

pub(super) enum AcquireError {
    Active,
    Io(io::Error),
}

pub(super) struct RunOwnership {
    file: File,
}

impl RunOwnership {
    pub(super) fn acquire(lock_directory: &Path, run_id: &str) -> Result<Self, AcquireError> {
        std::fs::create_dir_all(lock_directory).map_err(AcquireError::Io)?;
        let name = format!("{}.lock", blake3::hash(run_id.as_bytes()).to_hex());
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_directory.join(name))
            .map_err(AcquireError::Io)?;
        try_lock(&file)?;
        Ok(Self { file })
    }
}

impl Drop for RunOwnership {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

pub(super) fn lock_directory(database: &Path) -> PathBuf {
    let file_name = database
        .file_name()
        .expect("resolved database has a file name");
    let mut directory_name = OsString::from(".");
    directory_name.push(file_name);
    directory_name.push(".hoimin-locks");
    database.with_file_name(directory_name)
}

#[cfg(unix)]
fn try_lock(file: &File) -> Result<(), AcquireError> {
    use std::os::fd::AsRawFd;

    // SAFETY: `file` owns a valid descriptor for the duration of this call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    let errno = error.raw_os_error();
    if errno == Some(libc::EWOULDBLOCK) || errno == Some(libc::EAGAIN) {
        Err(AcquireError::Active)
    } else {
        Err(AcquireError::Io(error))
    }
}

#[cfg(unix)]
fn unlock(file: &File) {
    use std::os::fd::AsRawFd;

    // SAFETY: `file` still owns the descriptor and unlocking is best-effort during drop.
    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }
}

#[cfg(windows)]
fn try_lock(file: &File) -> Result<(), AcquireError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{ERROR_LOCK_VIOLATION, HANDLE};
    use windows_sys::Win32::Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    // SAFETY: zero is the documented offset/event initialization for a synchronous lock.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    // SAFETY: the handle remains owned by `file`, and `overlapped` lives through the call.
    let locked = unsafe {
        LockFileEx(
            file.as_raw_handle() as HANDLE,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &raw mut overlapped,
        )
    };
    if locked != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == i32::try_from(ERROR_LOCK_VIOLATION).ok() {
        Err(AcquireError::Active)
    } else {
        Err(AcquireError::Io(error))
    }
}

#[cfg(windows)]
fn unlock(file: &File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    // SAFETY: zero initializes the same lock range used by `try_lock`.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    // SAFETY: the handle remains owned by `file`; failure is best-effort during drop.
    unsafe {
        UnlockFileEx(
            file.as_raw_handle() as HANDLE,
            0,
            u32::MAX,
            u32::MAX,
            &raw mut overlapped,
        );
    }
}
