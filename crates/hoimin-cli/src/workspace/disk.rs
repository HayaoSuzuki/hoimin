//! Bounded disk measurement for Hoimin-owned workspace roots.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};

const MAX_TREE_ENTRIES: usize = 250_000;
const MAX_TREE_DEPTH: usize = 128;
const MAX_SCAN_DURATION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct FilesystemKey(pub(crate) u64);

#[derive(Debug)]
pub(crate) struct RootCapability {
    pub(crate) dir: cap_std::fs::Dir,
    pub(crate) display_path: Utf8PathBuf,
}

impl RootCapability {
    #[cfg(all(test, unix))]
    pub(crate) fn open(path: &Utf8Path) -> io::Result<Self> {
        let dir = cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())?;
        Ok(Self {
            dir,
            display_path: path.to_owned(),
        })
    }

    #[cfg(all(test, windows))]
    pub(crate) fn open(path: &Utf8Path) -> io::Result<Self> {
        Ok(Self {
            dir: open_windows_meter_root(path, None)?,
            display_path: path.to_owned(),
        })
    }

    #[cfg(unix)]
    pub(crate) fn from_dir(dir: &cap_std::fs::Dir, display_path: Utf8PathBuf) -> io::Result<Self> {
        Ok(Self {
            dir: dir.try_clone()?,
            display_path,
        })
    }

    #[cfg(windows)]
    pub(crate) fn from_dir(dir: &cap_std::fs::Dir, display_path: Utf8PathBuf) -> io::Result<Self> {
        Ok(Self {
            dir: dir.try_clone()?,
            display_path,
        })
    }
}

pub(crate) trait AvailableSpace: Send + Sync {
    fn available(&self, root: &RootCapability) -> io::Result<u64>;
    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemAvailableSpace;

#[cfg(unix)]
impl AvailableSpace for SystemAvailableSpace {
    fn available(&self, root: &RootCapability) -> io::Result<u64> {
        use std::mem::MaybeUninit;
        use std::os::fd::AsRawFd;

        let file = root.dir.try_clone()?.into_std_file();
        let mut value = MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: value is a writable statvfs buffer and file remains open for this call.
        if unsafe { libc::fstatvfs(file.as_raw_fd(), value.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fstatvfs succeeded and initialized the entire structure.
        let value = unsafe { value.assume_init() };
        let available_blocks = statvfs_value_to_u64(value.f_bavail);
        let fragment_size = statvfs_value_to_u64(value.f_frsize);
        available_blocks
            .checked_mul(fragment_size)
            .ok_or_else(|| io::Error::other("available-space overflow"))
    }

    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey> {
        use cap_fs_ext::MetadataExt;

        root.dir
            .metadata(".")
            .map(|metadata| FilesystemKey(metadata.dev()))
    }
}

#[cfg(unix)]
#[allow(
    clippy::useless_conversion,
    reason = "libc statvfs fields vary between Unix ABIs"
)]
fn statvfs_value_to_u64<T: Into<u64>>(value: T) -> u64 {
    value.into()
}

#[cfg(windows)]
impl AvailableSpace for SystemAvailableSpace {
    fn available(&self, root: &RootCapability) -> io::Result<u64> {
        use std::os::windows::io::AsRawHandle;

        use windows_sys::Win32::Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetFinalPathNameByHandleW, GetVolumeInformationW,
            GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        };

        let file = root.dir.try_clone()?.into_std_file();
        let expected_volume = super::root::windows::file_identity_io(&file)?.0;
        let handle = file.as_raw_handle();
        // SAFETY: a null buffer with zero length requests the required UTF-16 length.
        let required = unsafe { GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, 0) };
        if required == 0 {
            return Err(io::Error::last_os_error());
        }
        let capacity = required
            .checked_add(1)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| io::Error::other("final path length overflow"))?;
        let mut final_path = vec![0_u16; capacity];
        // SAFETY: final_path is writable for its declared capacity and handle remains open.
        let written =
            unsafe { GetFinalPathNameByHandleW(handle, final_path.as_mut_ptr(), required + 1, 0) };
        if written == 0 || written > required {
            return Err(io::Error::last_os_error());
        }
        final_path.truncate(
            usize::try_from(written).map_err(|_| io::Error::other("final path overflow"))?,
        );
        final_path.push(0);
        let mut volume_path = vec![0_u16; 32_768];
        // SAFETY: both UTF-16 buffers are NUL-terminated/writable for their declared lengths.
        if unsafe {
            GetVolumePathNameW(
                final_path.as_ptr(),
                volume_path.as_mut_ptr(),
                u32::try_from(volume_path.len()).expect("fixed Windows path capacity"),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut volume_name = [0_u16; 128];
        // SAFETY: volume_path is NUL-terminated and volume_name is writable.
        if unsafe {
            GetVolumeNameForVolumeMountPointW(
                volume_path.as_ptr(),
                volume_name.as_mut_ptr(),
                u32::try_from(volume_name.len()).expect("fixed volume-name capacity"),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let volume_serial = |path: *const u16| -> io::Result<u64> {
            let mut serial = 0_u32;
            // SAFETY: path is a NUL-terminated volume GUID and serial is writable.
            if unsafe {
                GetVolumeInformationW(
                    path,
                    std::ptr::null_mut(),
                    0,
                    &raw mut serial,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                )
            } == 0
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(u64::from(serial))
            }
        };
        if volume_serial(volume_name.as_ptr())? != expected_volume {
            return Err(io::Error::other(
                "capacity volume does not match the open managed-root handle",
            ));
        }
        let mut available = 0_u64;
        // SAFETY: volume_name is a NUL-terminated volume GUID and output is writable.
        if unsafe {
            GetDiskFreeSpaceExW(
                volume_name.as_ptr(),
                &raw mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if volume_serial(volume_name.as_ptr())? != expected_volume {
            return Err(io::Error::other(
                "capacity volume identity changed during the query",
            ));
        }
        Ok(available)
    }

    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey> {
        let file = root.dir.try_clone()?.into_std_file();
        super::root::windows::file_identity_io(&file).map(|(volume, _)| FilesystemKey(volume))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MeterReading {
    pub(crate) owned_bytes: u64,
    pub(crate) available_by_filesystem: BTreeMap<FilesystemKey, u64>,
    pub(crate) conservative_entries: bool,
    pub(crate) elapsed: Duration,
}

#[derive(Debug)]
pub(crate) struct DiskMeter<S> {
    roots: Vec<RootCapability>,
    space: S,
}

impl<S: AvailableSpace> DiskMeter<S> {
    pub(crate) fn new(roots: Vec<RootCapability>, space: S) -> Self {
        Self { roots, space }
    }

    pub(crate) fn measure(&self) -> io::Result<MeterReading> {
        let started = Instant::now();
        let mut state = WalkState::default();
        let mut available_by_filesystem = BTreeMap::new();
        for root in &self.roots {
            let key = self.space.filesystem_key(root)?;
            if let std::collections::btree_map::Entry::Vacant(entry) =
                available_by_filesystem.entry(key)
            {
                entry.insert(self.space.available(root)?);
            }
            measure_owned_tree(root, started, &mut state)?;
        }
        Ok(MeterReading {
            owned_bytes: state.owned_bytes,
            available_by_filesystem,
            conservative_entries: cfg!(windows),
            elapsed: started.elapsed(),
        })
    }
}

#[derive(Default)]
struct WalkState {
    owned_bytes: u64,
    entries: usize,
    identities: BTreeSet<(u64, u64)>,
}

#[cfg(unix)]
struct WalkFrame {
    entries: rustix::fs::Dir,
    depth: usize,
    display_path: Utf8PathBuf,
}

fn measure_owned_tree(
    root: &RootCapability,
    started: Instant,
    state: &mut WalkState,
) -> io::Result<()> {
    measure_owned_tree_with_elapsed(root, state, &|| started.elapsed())
}

fn measure_owned_tree_with_elapsed(
    root: &RootCapability,
    state: &mut WalkState,
    elapsed: &impl Fn() -> Duration,
) -> io::Result<()> {
    measure_owned_tree_with_hooks(root, state, elapsed, &|_| {}, &|_| {})
}

#[cfg(unix)]
fn measure_owned_tree_with_hooks(
    root: &RootCapability,
    state: &mut WalkState,
    elapsed: &impl Fn() -> Duration,
    before_directory_open: &impl Fn(&Utf8Path),
    observe_open_directories: &impl Fn(usize),
) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    use rustix::fs::{AtFlags, FileType};

    let root_dir = rustix::fs::Dir::read_from(&root.dir).map_err(io::Error::from)?;
    let root_device = rustix::fs::fstat(root_dir.fd().map_err(io::Error::from)?)
        .map_err(io::Error::from)?
        .st_dev;
    #[cfg(target_os = "macos")]
    let root_mount = macos_mount_identity(&root_dir.fd().map_err(io::Error::from)?)?;
    let mut stack = vec![WalkFrame {
        entries: root_dir,
        depth: 0,
        display_path: root.display_path.clone(),
    }];
    observe_open_directories(stack.len());
    while let Some(frame) = stack.last_mut() {
        if elapsed() >= MAX_SCAN_DURATION {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owned workspace scan exceeded five seconds",
            ));
        }
        let Some(entry) = frame.entries.next() else {
            stack.pop();
            continue;
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if io::Error::from(error).kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io::Error::from(error)),
        };
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        record_entry(state)?;
        let name_os = std::ffi::OsStr::from_bytes(name.to_bytes());
        let display = frame.display_path.join(
            name_os
                .to_str()
                .ok_or_else(|| io::Error::other("non-UTF-8 workspace entry"))?,
        );
        let directory = frame.entries.fd().map_err(io::Error::from)?;
        let metadata = match rustix::fs::statat(directory, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(metadata) => metadata,
            Err(error) if io::Error::from(error).kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io::Error::from(error)),
        };
        let file_type = FileType::from_raw_mode(metadata.st_mode);
        if file_type == FileType::Symlink {
            continue;
        }
        if file_type == FileType::Directory {
            if metadata.st_dev != root_device {
                return Err(io::Error::other(format!(
                    "owned workspace scan refuses to cross a filesystem boundary: {display}"
                )));
            }
            let child_depth = frame.depth + 1;
            if child_depth > MAX_TREE_DEPTH {
                return Err(io::Error::other(format!(
                    "owned workspace depth exceeds {MAX_TREE_DEPTH}: {display}"
                )));
            }
            before_directory_open(&display);
            let child = match open_meter_directory(&directory, name) {
                Ok(child) => child,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            #[cfg(target_os = "macos")]
            if macos_mount_identity(&child)? != root_mount {
                return Err(io::Error::other(format!(
                    "owned workspace scan refuses to cross a mount boundary: {display}"
                )));
            }
            let opened_metadata = rustix::fs::fstat(&child).map_err(io::Error::from)?;
            if unix_stat_identity(&metadata) != unix_stat_identity(&opened_metadata) {
                return Err(io::Error::other(format!(
                    "owned workspace directory identity changed while opening: {display}"
                )));
            }
            stack.push(WalkFrame {
                entries: rustix::fs::Dir::new(child).map_err(io::Error::from)?,
                depth: child_depth,
                display_path: display,
            });
            observe_open_directories(stack.len());
            continue;
        }
        if file_type == FileType::RegularFile && should_count_unix_stat(&metadata, state) {
            let length = u64::try_from(metadata.st_size)
                .map_err(|_| io::Error::other("negative workspace file length"))?;
            state.owned_bytes = state
                .owned_bytes
                .checked_add(length)
                .ok_or_else(|| io::Error::other("owned workspace byte count overflow"))?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
type MacosMountIdentity = [u8; std::mem::size_of::<libc::fsid_t>()];

#[cfg(target_os = "macos")]
fn macos_mount_identity(fd: &impl std::os::fd::AsFd) -> io::Result<MacosMountIdentity> {
    use std::os::fd::AsRawFd;

    // SAFETY: result points to writable storage and fd remains borrowed for the call.
    let result = unsafe {
        let mut result: libc::statfs = std::mem::zeroed();
        if libc::fstatfs(fd.as_fd().as_raw_fd(), &raw mut result) != 0 {
            return Err(io::Error::last_os_error());
        }
        result
    };
    let mut identity = [0_u8; std::mem::size_of::<libc::fsid_t>()];
    // SAFETY: identity has exactly the byte size of f_fsid and both ranges are valid/nonoverlap.
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&raw const result.f_fsid).cast::<u8>(),
            identity.as_mut_ptr(),
            identity.len(),
        );
    }
    Ok(identity)
}

#[cfg(target_os = "linux")]
fn open_meter_directory(
    directory: &impl std::os::fd::AsFd,
    name: &std::ffi::CStr,
) -> io::Result<std::os::fd::OwnedFd> {
    use rustix::fs::{Mode, OFlags, ResolveFlags};

    rustix::fs::openat2(
        directory,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
    )
    .map_err(io::Error::from)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn open_meter_directory(
    directory: &impl std::os::fd::AsFd,
    name: &std::ffi::CStr,
) -> io::Result<std::os::fd::OwnedFd> {
    use rustix::fs::{Mode, OFlags};

    rustix::fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)
}

#[cfg(windows)]
struct WindowsWalkFrame {
    entries: super::root::windows::DirectoryEntries,
    depth: usize,
    display_path: Utf8PathBuf,
}

#[cfg(windows)]
fn measure_owned_tree_with_hooks(
    root: &RootCapability,
    state: &mut WalkState,
    elapsed: &impl Fn() -> Duration,
    before_directory_open: &impl Fn(&Utf8Path),
    observe_open_directories: &impl Fn(usize),
) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };

    let root_dir = root.dir.try_clone()?.into_std_file();
    let mut stack = vec![WindowsWalkFrame {
        entries: super::root::windows::DirectoryEntries::open(root_dir)?,
        depth: 0,
        display_path: root.display_path.clone(),
    }];
    observe_open_directories(stack.len());
    while let Some(frame) = stack.last_mut() {
        if elapsed() >= MAX_SCAN_DURATION {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owned workspace scan exceeded five seconds",
            ));
        }
        let Some(entry) = frame.entries.next() else {
            stack.pop();
            continue;
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        record_entry(state)?;
        let attributes = entry.file_attributes();
        let entry_file_id = entry.file_id();
        let name = entry.into_name();
        let display = frame.display_path.join(
            name.to_str()
                .ok_or_else(|| io::Error::other("non-UTF-8 workspace entry"))?,
        );
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            continue;
        }
        let child_depth = frame.depth + 1;
        if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 && child_depth > MAX_TREE_DEPTH {
            return Err(io::Error::other(format!(
                "owned workspace depth exceeds {MAX_TREE_DEPTH}: {display}"
            )));
        }
        // At the deepest accepted frame, acquire only a non-directory handle. This pins the
        // enumerated file without increasing the directory-handle peak, and the file-id check
        // makes a same-name replacement fail closed.
        if frame.depth == MAX_TREE_DEPTH {
            let inspected = match super::root::windows::open_regular_file_shared(
                frame.entries.directory(),
                &name,
            ) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let identity = super::root::windows::file_identity_io(&inspected)?;
            if identity.1 != entry_file_id {
                return Err(io::Error::other(format!(
                    "owned workspace entry identity changed while opening: {display}"
                )));
            }
            state.owned_bytes = state
                .owned_bytes
                .checked_add(inspected.metadata()?.len())
                .ok_or_else(|| io::Error::other("owned workspace byte count overflow"))?;
            continue;
        }
        let inspected =
            match super::root::windows::open_entry_shared(frame.entries.directory(), &name) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
        let metadata = inspected.metadata()?;
        if super::root::windows::file_identity_io(&inspected)?.1 != entry_file_id {
            return Err(io::Error::other(format!(
                "owned workspace entry identity changed while opening: {display}"
            )));
        }
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            continue;
        }
        if metadata.is_dir() {
            before_directory_open(&display);
            observe_open_directories(child_depth + 1);
            let expected_identity = super::root::windows::file_identity_io(&inspected)?;
            // Keep the depth bound equal to the handle bound: release the inspection handle
            // before acquiring the one child handle that the next frame will own. The identity
            // retained above makes any replacement during this gap fail closed below.
            drop(inspected);
            let child =
                match super::root::windows::open_directory_shared(frame.entries.directory(), &name)
                {
                    Ok(child) => child,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
            if super::root::windows::file_identity_io(&child)? != expected_identity {
                return Err(io::Error::other(format!(
                    "owned workspace directory identity changed while opening: {display}"
                )));
            }
            stack.push(WindowsWalkFrame {
                entries: super::root::windows::DirectoryEntries::open(child)?,
                depth: child_depth,
                display_path: display,
            });
            observe_open_directories(stack.len());
        } else if metadata.is_file() {
            state.owned_bytes = state
                .owned_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| io::Error::other("owned workspace byte count overflow"))?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn open_windows_meter_root(
    path: &Utf8Path,
    expected_identity: Option<(u64, u64)>,
) -> io::Result<cap_std::fs::Dir> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::FromRawHandle;

    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, SYNCHRONIZE,
    };

    let wide = path
        .as_std_path()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: wide is NUL-terminated and no optional security/template pointers are used.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned one newly owned handle.
    let file = unsafe { std::fs::File::from_raw_handle(handle.cast()) };
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("meter root is not a real directory"));
    }
    let reopened_identity = super::root::windows::file_identity_io(&file)?;
    let dir = cap_std::fs::Dir::from_std_file(file);
    if expected_identity.is_some_and(|expected| reopened_identity != expected) {
        return Err(io::Error::other(
            "meter root identity changed while reopening",
        ));
    }
    Ok(dir)
}

fn record_entry(state: &mut WalkState) -> io::Result<()> {
    state.entries = state
        .entries
        .checked_add(1)
        .ok_or_else(|| io::Error::other("owned workspace entry count overflow"))?;
    if state.entries > MAX_TREE_ENTRIES {
        return Err(io::Error::other(
            "owned workspace entry count exceeds 250000",
        ));
    }
    Ok(())
}

#[cfg(unix)]
#[allow(
    clippy::cast_sign_loss,
    clippy::unnecessary_cast,
    reason = "libc stat identity fields vary between Unix ABIs"
)]
fn unix_stat_identity(metadata: &rustix::fs::Stat) -> (u64, u64) {
    (metadata.st_dev as u64, metadata.st_ino as u64)
}

#[cfg(unix)]
fn should_count_unix_stat(metadata: &rustix::fs::Stat, state: &mut WalkState) -> bool {
    state.identities.insert(unix_stat_identity(metadata))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use camino::Utf8Path;

    use super::measure_owned_tree_with_hooks;
    use super::{
        AvailableSpace, DiskMeter, FilesystemKey, RootCapability, WalkState,
        measure_owned_tree_with_elapsed, record_entry,
    };

    #[derive(Debug)]
    struct FixedSpace(u64);

    impl AvailableSpace for FixedSpace {
        fn available(&self, _root: &RootCapability) -> std::io::Result<u64> {
            Ok(self.0)
        }

        fn filesystem_key(&self, _root: &RootCapability) -> std::io::Result<FilesystemKey> {
            Ok(FilesystemKey(7))
        }
    }

    fn capability(path: &Utf8Path) -> RootCapability {
        RootCapability::open(path).unwrap()
    }

    #[test]
    fn injected_deadline_and_entry_cap_fail_without_large_fixtures() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = capability(temp);
        let mut state = WalkState::default();

        let deadline = measure_owned_tree_with_elapsed(&root, &mut state, &|| {
            std::time::Duration::from_secs(5)
        })
        .unwrap_err();
        assert_eq!(deadline.kind(), std::io::ErrorKind::TimedOut);

        state.entries = super::MAX_TREE_ENTRIES;
        assert!(record_entry(&mut state).is_err());
        assert_eq!(state.entries, super::MAX_TREE_ENTRIES + 1);
    }

    #[cfg(unix)]
    #[test]
    fn directory_replaced_by_a_symlink_before_open_is_never_followed() {
        use std::cell::Cell;
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let outside = Utf8Path::from_path(outside.path()).unwrap();
        std::fs::create_dir(temp.join("victim")).unwrap();
        std::fs::write(outside.join("must-not-count"), vec![0_u8; 4096]).unwrap();
        let root = capability(temp);
        let swapped = Cell::new(false);
        let mut state = WalkState::default();

        let result = measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|display| {
                if display == temp.join("victim") && !swapped.replace(true) {
                    std::fs::rename(temp.join("victim"), temp.join("parked")).unwrap();
                    symlink(outside, temp.join("victim")).unwrap();
                }
            },
            &|_| {},
        );

        assert!(result.is_err());
        assert!(swapped.get());
        assert_eq!(state.owned_bytes, 0);
        assert!(outside.join("must-not-count").exists());
    }

    #[test]
    fn exact_depth_bound_uses_at_most_one_hundred_twenty_nine_directory_handles() {
        use std::cell::Cell;

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        std::fs::create_dir(&root).unwrap();
        let mut cursor = root.clone();
        for _ in 0..super::MAX_TREE_DEPTH {
            cursor.push("d");
            std::fs::create_dir(&cursor).unwrap();
        }
        let root = capability(&root);
        let mut state = WalkState::default();
        let peak = Cell::new(0_usize);

        measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|_| {},
            &|open| peak.set(peak.get().max(open)),
        )
        .unwrap();

        assert_eq!(peak.get(), super::MAX_TREE_DEPTH + 1);
        assert_eq!(peak.get(), 129);
    }

    #[cfg(unix)]
    fn open_file_descriptor_count() -> usize {
        #[cfg(target_os = "linux")]
        let directory = "/proc/self/fd";
        #[cfg(not(target_os = "linux"))]
        let directory = "/dev/fd";
        std::fs::read_dir(directory).unwrap().count()
    }

    #[cfg(unix)]
    #[test]
    fn directory_handle_bound_child() {
        use std::cell::Cell;

        if std::env::var_os("HOIMIN_DISK_HANDLE_BOUND_CHILD").is_none() {
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        std::fs::create_dir(&root).unwrap();
        let mut cursor = root.clone();
        for _ in 0..super::MAX_TREE_DEPTH {
            cursor.push("d");
            std::fs::create_dir(&cursor).unwrap();
        }
        let root = capability(&root);
        let baseline = open_file_descriptor_count();
        let peak = Cell::new(baseline);
        let mut state = WalkState::default();

        measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|_| {},
            &|_| peak.set(peak.get().max(open_file_descriptor_count())),
        )
        .unwrap();

        assert_eq!(peak.get().checked_sub(baseline), Some(129));
    }

    #[cfg(unix)]
    #[test]
    fn fresh_process_observes_the_real_directory_descriptor_bound() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("workspace::disk::tests::directory_handle_bound_child")
            .arg("--nocapture")
            .env("HOIMIN_DISK_HANDLE_BOUND_CHILD", "1")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[test]
    fn directory_that_vanishes_before_open_is_ignored() {
        use std::cell::Cell;

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir(temp.join("victim")).unwrap();
        let root = capability(temp);
        let removed = Cell::new(false);
        let mut state = WalkState::default();

        measure_owned_tree_with_hooks(
            &root,
            &mut state,
            &|| std::time::Duration::ZERO,
            &|display| {
                if display == temp.join("victim") && !removed.replace(true) {
                    std::fs::remove_dir(display).unwrap();
                }
            },
            &|_| {},
        )
        .unwrap();

        assert!(removed.get());
        assert_eq!(state.owned_bytes, 0);
    }

    #[test]
    fn owned_byte_overflow_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::write(temp.join("one-byte"), b"x").unwrap();
        let root = capability(temp);
        let mut state = WalkState {
            owned_bytes: u64::MAX,
            ..WalkState::default()
        };

        let error =
            measure_owned_tree_with_elapsed(&root, &mut state, &|| std::time::Duration::ZERO)
                .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(state.owned_bytes, u64::MAX);
    }

    #[test]
    fn aggregates_files_across_owned_roots_and_queries_a_filesystem_once() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir(temp.join("snapshot")).unwrap();
        std::fs::create_dir(temp.join("worker")).unwrap();
        std::fs::write(temp.join("snapshot/a"), b"abcd").unwrap();
        std::fs::write(temp.join("worker/b"), b"123456").unwrap();
        let roots = vec![
            capability(&temp.join("snapshot")),
            capability(&temp.join("worker")),
        ];

        let reading = DiskMeter::new(roots, FixedSpace(99)).measure().unwrap();

        assert_eq!(reading.owned_bytes, 10);
        assert_eq!(
            reading.available_by_filesystem,
            BTreeMap::from([(FilesystemKey(7), 99)])
        );
        assert_eq!(reading.conservative_entries, cfg!(windows));
    }

    #[test]
    fn repeated_measurements_restart_directory_enumeration() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir(temp.join("nested")).unwrap();
        std::fs::write(temp.join("nested/data"), b"1234567").unwrap();
        let meter = DiskMeter::new(vec![capability(temp)], FixedSpace(99));

        let first = meter.measure().unwrap();
        let second = meter.measure().unwrap();

        assert_eq!(first.owned_bytes, second.owned_bytes);
        assert_eq!(
            first.available_by_filesystem,
            second.available_by_filesystem
        );
        assert_eq!(first.conservative_entries, second.conservative_entries);
        assert_eq!(second.owned_bytes, 7);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks_and_counts_hard_links_once() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        let outside = temp.join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(root.join("data"), b"12345").unwrap();
        std::fs::hard_link(root.join("data"), root.join("alias")).unwrap();
        std::fs::write(outside.join("large"), vec![0_u8; 1_024]).unwrap();
        symlink(&outside, root.join("escape")).unwrap();

        let reading = DiskMeter::new(vec![capability(&root)], FixedSpace(99))
            .measure()
            .unwrap();

        assert_eq!(reading.owned_bytes, 5);
        assert!(!reading.conservative_entries);
    }

    #[test]
    fn rejects_a_tree_deeper_than_the_bound() {
        let temp = tempfile::tempdir().unwrap();
        let temp = Utf8Path::from_path(temp.path()).unwrap();
        let root = temp.join("root");
        std::fs::create_dir(&root).unwrap();
        let mut cursor = root.clone();
        for _ in 0..129 {
            cursor.push("d");
            std::fs::create_dir(&cursor).unwrap();
        }

        let error = DiskMeter::new(vec![capability(&root)], FixedSpace(99))
            .measure()
            .unwrap_err();

        assert!(error.to_string().contains("depth"));
    }
}
