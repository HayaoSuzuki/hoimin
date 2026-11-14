//! Identify the directory entry replaced by metrics' atomic rename, not its referent.
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::io_faults::checked_io;
use crate::metrics::MetricsOutput;
use cap_std::fs::Dir;

use hoimin_core::{EffectFailed, EffectId, RunConfig, TargetSlice};

#[derive(Default)]
pub(crate) enum MetricsDestination {
    #[default]
    Disabled,
    Ready(MetricsOutput),
    Unavailable(String),
}

struct Entry {
    path: PathBuf,
    exists: bool,
    parent: Option<Arc<Dir>>,
}

#[derive(Default)]
struct EntryInspector {
    #[cfg(unix)]
    directories: std::collections::BTreeMap<(u64, u64), DirectoryEntries>,
}

#[cfg(unix)]
struct DirectoryEntries {
    names: std::collections::BTreeSet<OsString>,
    aliases: Option<std::collections::BTreeMap<(u64, u64), Vec<OsString>>>,
}

pub(crate) fn validate_metrics_destination(
    config: &RunConfig,
    targets: &[TargetSlice],
    protected_inputs: &[PathBuf],
    id: EffectId,
) -> Result<MetricsDestination, EffectFailed> {
    let Some(output) = &config.output.metrics else {
        return Ok(MetricsDestination::Disabled);
    };
    match inspect(config, targets, protected_inputs, output.as_std_path()) {
        Ok(Ok(path)) => Ok(MetricsDestination::Ready(path)),
        Ok(Err(protected)) => Err(EffectFailed::other(
            id,
            "metrics.destination.collision",
            format!(
                "metrics destination {output} would replace protected input {}",
                protected.display()
            ),
        )),
        Err(error) => Ok(MetricsDestination::Unavailable(format!(
            "metrics destination {output} was not authorized: {error}"
        ))),
    }
}

fn directory_only_syntax(path: &Path, windows_separators: bool) -> bool {
    // Inspect raw syntax before Path::components/absolute removes terminal `.`.
    // Stripping a directory requirement could allow persist to replace its symlink.
    let terminal = path
        .as_os_str()
        .as_encoded_bytes()
        .rsplit(|byte| *byte == b'/' || (windows_separators && *byte == b'\\'))
        .next()
        .unwrap_or_default();
    terminal.is_empty() || terminal == b"." || terminal == b".."
}

fn inspect(
    config: &RunConfig,
    targets: &[TargetSlice],
    protected_inputs: &[PathBuf],
    output: &Path,
) -> io::Result<Result<MetricsOutput, PathBuf>> {
    if directory_only_syntax(output, cfg!(windows)) {
        return Err(unknown("metrics destination requires a directory"));
    }
    let mut inspector = EntryInspector::default();
    let destination = inspector.entry(output)?;
    let mut protected = targets
        .iter()
        .map(|target| config.selection.root.join(&target.path).into_std_path_buf())
        .collect::<Vec<_>>();
    protected.extend(
        config
            .fingerprint_inputs
            .iter()
            .map(|input| config.selection.root.join(&input.path).into_std_path_buf()),
    );
    protected.extend_from_slice(protected_inputs);
    let mut lock_trees = Vec::new();
    let mut uncertainty = None;
    if let Some(session) = config.session.config() {
        protected.push(session.path.as_std_path().to_owned());
        match session_artifacts(session.path.as_std_path(), &mut inspector) {
            Ok((files, trees)) => {
                protected.extend(files);
                lock_trees = trees;
            }
            Err(error) => uncertainty = Some(error),
        }
    }
    // Do not let an unrelated uncertain comparison hide a confirmed collision.
    for path in protected {
        match inspector
            .entry(&path)
            .and_then(|input| same_entry(&destination, &input))
        {
            Ok(true) => return Ok(Err(path)),
            Ok(false) => {}
            Err(error) => uncertainty = Some(error),
        }
    }
    for tree in lock_trees {
        match inspector.entry(&tree).and_then(|tree| {
            if same_entry(&destination, &tree)? {
                return Ok(true);
            }
            let Some(parent) = &destination.parent else {
                // A missing parent can prove a prospective lock-tree collision,
                // but can never authorize output. Preserve collision precedence.
                for ancestor in destination.path.ancestors() {
                    if same_entry(&inspector.entry(ancestor)?, &tree)? {
                        return Ok(true);
                    }
                }
                return Ok(false);
            };
            let directory = match Dir::open_ambient_dir(&tree.path, cap_std::ambient_authority()) {
                Ok(directory) => directory,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error),
            };
            let protected = directory_identity(&directory)?;
            let mut ancestor = parent.try_clone()?;
            loop {
                let identity = directory_identity(&ancestor)?;
                if identity == protected {
                    return Ok(true);
                }
                let next = ancestor.open_parent_dir(cap_std::ambient_authority())?;
                if directory_identity(&next)? == identity {
                    return Ok(false);
                }
                ancestor = next;
            }
        }) {
            Ok(true) => return Ok(Err(tree)),
            Ok(false) => {}
            Err(error) => uncertainty = Some(error),
        }
    }
    if let Some(error) = uncertainty {
        return Err(error);
    }
    let parent = destination
        .parent
        .ok_or_else(|| unknown("metrics parent does not exist"))?;
    Ok(Ok(MetricsOutput::from_parent(parent, destination.path)?))
}

fn session_artifacts(
    configured: &Path,
    inspector: &mut EntryInspector,
) -> io::Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    #[cfg(not(windows))]
    let _ = inspector;
    // Use the same canonical database and companion paths as SessionHandler.
    let artifacts = crate::session::SessionArtifacts::resolve(configured)?;
    let files = artifacts.files().to_vec();
    // Also retain protection of the configured Windows namespace when the
    // supplied final entry is an alias of the database opened by SessionHandler.
    #[cfg(windows)]
    let files = {
        let mut files = files;
        let configured = inspector.entry(configured)?.path;
        let companions = crate::artifacts::SqliteCompanions::new(&configured);
        for path in [companions.wal, companions.shm, companions.journal] {
            if !files.contains(&path) {
                files.push(path);
            }
        }
        files
    };
    let trees = artifacts.lock_trees()?;
    Ok((files, trees))
}

fn unknown(message: &str) -> io::Error {
    io::Error::other(message)
}

impl EntryInspector {
    fn entry(&mut self, path: &Path) -> io::Result<Entry> {
        #[cfg(windows)]
        reject_unsupported_windows_names(path)?;
        // Preserve errors in the original spelling, e.g. a regular file followed by `/`.
        // Stripping that separator first could authorize a different rename operation.
        if let Err(error) = checked_io!("metrics.entry.metadata", std::fs::symlink_metadata(path))
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err(error);
        }
        let absolute = checked_io!("metrics.entry.absolute", std::path::absolute(path))?;
        let Some(name) = absolute.file_name() else {
            return Ok(Entry {
                path: std::fs::canonicalize(absolute)?,
                exists: true,
                parent: None,
            });
        };
        let parent = prospective_parent(
            absolute
                .parent()
                .ok_or_else(|| unknown("destination has no parent"))?,
        )?;
        let path = parent.join(name);
        // Retain the actual parent before inspecting the leaf. A canonical path
        // is display/prospective-comparison data, never the write capability.
        let directory = match checked_io!(
            "metrics.entry.open",
            Dir::open_ambient_dir(&parent, cap_std::ambient_authority())
        ) {
            Ok(directory) => Arc::new(directory),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Entry {
                    path,
                    exists: false,
                    parent: None,
                });
            }
            Err(error) => return Err(error),
        };
        match checked_io!("metrics.entry.leaf", directory.symlink_metadata(name)) {
            Ok(metadata) => {
                let path = self.existing_entry(&path, &directory, &metadata)?;
                Ok(Entry {
                    path,
                    exists: true,
                    parent: Some(directory),
                })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Entry {
                path,
                exists: false,
                parent: Some(directory),
            }),
            Err(error) => Err(error),
        }
    }
}

fn prospective_parent(path: &Path) -> io::Result<PathBuf> {
    match checked_io!("metrics.parent.canonicalize", std::fs::canonicalize(path)) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A missing ancestor followed by `..` is not a usable rename path.
            if !matches!(
                path.components().next_back(),
                Some(std::path::Component::Normal(_))
            ) {
                return Err(error);
            }
            let name = path.file_name().ok_or(error)?;
            Ok(
                prospective_parent(path.parent().ok_or_else(|| unknown("missing parent"))?)?
                    .join(name),
            )
        }
        Err(error) => Err(error),
    }
}

fn same_entry(left: &Entry, right: &Entry) -> io::Result<bool> {
    if left.path == right.path {
        return Ok(true);
    }
    if !same_parent(left, right)? {
        return Ok(false);
    }
    if left.path.file_name() == right.path.file_name() {
        return Ok(true);
    }
    if left.exists || right.exists {
        // Native lookup has already resolved aliases of every existing entry.
        return Ok(false);
    }
    let (Some(left_name), Some(right_name)) = (
        left.path.file_name().and_then(|name| name.to_str()),
        right.path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Err(unknown(
            "cannot compare prospective non-Unicode entry names",
        ));
    };
    if !left_name.is_ascii() || !right_name.is_ascii() {
        return Err(unknown("cannot compare prospective non-ASCII entry names"));
    }
    #[cfg(windows)]
    if left_name.contains('~') || right_name.contains('~') {
        return Err(unknown(
            "cannot compare prospective Windows short-name aliases",
        ));
    }
    if !left_name.eq_ignore_ascii_case(right_name) {
        return Ok(false);
    }
    Ok(!case_sensitive(
        left.parent
            .as_deref()
            .ok_or_else(|| unknown("missing parent"))?,
    )?)
}

fn same_parent(left: &Entry, right: &Entry) -> io::Result<bool> {
    match (&left.parent, &right.parent) {
        (Some(left), Some(right)) => Ok(directory_identity(left)? == directory_identity(right)?),
        (None, None) => Ok(left.path.parent() == right.path.parent()),
        _ => Ok(false),
    }
}

#[cfg(unix)]
fn directory_identity(directory: &Dir) -> io::Result<(u64, u64)> {
    use cap_std::fs::MetadataExt;
    let metadata = directory.dir_metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn directory_identity(directory: &Dir) -> io::Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: directory owns a live handle; info has the required writable size.
    if unsafe { GetFileInformationByHandle(directory.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful GetFileInformationByHandle initialized info.
    let info = unsafe { info.assume_init() };
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(unix)]
impl EntryInspector {
    fn existing_entry(
        &mut self,
        path: &Path,
        directory: &Dir,
        metadata: &cap_std::fs::Metadata,
    ) -> io::Result<PathBuf> {
        use cap_std::fs::MetadataExt;
        use std::collections::btree_map::Entry as MapEntry;
        let parent = path.parent().ok_or_else(|| unknown("missing parent"))?;
        let parent_metadata = checked_io!("metrics.index.metadata", directory.dir_metadata())?;
        let key = (parent_metadata.dev(), parent_metadata.ino());
        let index = match self.directories.entry(key) {
            MapEntry::Occupied(value) => value.into_mut(),
            MapEntry::Vacant(value) => value.insert(DirectoryEntries {
                names: checked_io!(
                    "metrics.index.collect",
                    checked_io!("metrics.index.entries", directory.entries())?
                        .map(|item| item.map(|item| item.file_name()))
                        .collect::<io::Result<_>>()
                )?,
                aliases: None,
            }),
        };
        let name = path.file_name().ok_or_else(|| unknown("missing name"))?;
        if index.names.contains(name) {
            return Ok(parent.join(name));
        }
        // Build the no-follow alias index once, only when an inexact spelling occurs.
        // Exact entries win even when several directory names share an inode.
        if index.aliases.is_none() {
            let mut aliases = std::collections::BTreeMap::<_, Vec<_>>::new();
            for name in &index.names {
                let candidate = directory.symlink_metadata(name)?;
                aliases
                    .entry((candidate.dev(), candidate.ino()))
                    .or_default()
                    .push(name.clone());
            }
            index.aliases = Some(aliases);
        }
        let aliases = index.aliases.as_ref().expect("initialized above");
        let Some(names) = aliases.get(&(metadata.dev(), metadata.ino())) else {
            return Err(unknown("final entry disappeared during inspection"));
        };
        if names.len() != 1 {
            return Err(unknown("ambiguous final entry alias"));
        }
        Ok(parent.join(&names[0]))
    }
}

#[cfg(windows)]
impl EntryInspector {
    #[allow(
        clippy::unused_self,
        reason = "Unix uses the same inspector API with a directory cache"
    )]
    fn existing_entry(
        &mut self,
        path: &Path,
        directory: &Dir,
        _: &cap_std::fs::Metadata,
    ) -> io::Result<PathBuf> {
        use cap_fs_ext::OpenOptionsFollowExt;
        use cap_std::fs::OpenOptionsExt;
        use std::os::windows::ffi::OsStringExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, GetFinalPathNameByHandleW,
        };
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .access_mode(0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .follow(cap_fs_ext::FollowSymlinks::No);
        let file = directory.open_with(
            path.file_name().ok_or_else(|| unknown("missing name"))?,
            &options,
        )?;
        let mut buffer = vec![0_u16; 32768];
        // SAFETY: file owns its handle and buffer is writable for the supplied length.
        let length = unsafe {
            GetFinalPathNameByHandleW(file.as_raw_handle(), buffer.as_mut_ptr(), 32768, 0)
        };
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length >= 32768 {
            return Err(unknown("final entry name exceeded inspection buffer"));
        }
        buffer.truncate(length as usize);
        Ok(PathBuf::from(OsString::from_wide(&buffer)))
    }
}

#[cfg(target_os = "macos")]
fn case_sensitive(directory: &Dir) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    // SAFETY: directory owns a valid descriptor; pathconf only queries its filesystem.
    match unsafe { libc::fpathconf(directory.as_raw_fd(), libc::_PC_CASE_SENSITIVE) } {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(unknown("filesystem case sensitivity is unavailable")),
    }
}

#[cfg(target_os = "linux")]
fn case_sensitive(directory: &Dir) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    const FS_CASEFOLD_FL: libc::c_int = 0x4000_0000;
    // Linux directory capabilities may use O_PATH, which rejects ioctl.
    // Reopen the retained directory itself, without resolving its old pathname.
    let directory = directory.open(".")?;
    // Only these filesystems expose directory casefold through FS_IOC_GETFLAGS.
    // A zero flag on an arbitrary filesystem is not evidence of sensitive lookup.
    let mut filesystem = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: directory owns the descriptor and filesystem is writable statfs storage.
    if unsafe { libc::fstatfs(directory.as_raw_fd(), filesystem.as_mut_ptr()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful fstatfs initialized the value.
    let filesystem = unsafe { filesystem.assume_init() };
    if ![libc::EXT4_SUPER_MAGIC, libc::F2FS_SUPER_MAGIC].contains(&filesystem.f_type) {
        return Err(unknown("filesystem case sensitivity is unavailable"));
    }
    let mut flags: libc::c_int = 0;
    // SAFETY: directory owns the descriptor; GETFLAGS writes an int despite encoding long in its request number.
    if unsafe { libc::ioctl(directory.as_raw_fd(), libc::FS_IOC_GETFLAGS, &raw mut flags) } == -1 {
        return Err(unknown("filesystem case sensitivity is unavailable"));
    }
    Ok(flags & FS_CASEFOLD_FL == 0)
}

#[cfg(windows)]
fn case_sensitive(directory: &Dir) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_CASE_SENSITIVE_INFO, FileCaseSensitiveInfo, GetFileInformationByHandleEx,
    };
    let mut info = FILE_CASE_SENSITIVE_INFO { Flags: 0 };
    // SAFETY: directory owns the handle; info and its size match the requested class.
    if unsafe {
        GetFileInformationByHandleEx(
            directory.as_raw_handle(),
            FileCaseSensitiveInfo,
            (&raw mut info).cast(),
            u32::try_from(std::mem::size_of::<FILE_CASE_SENSITIVE_INFO>())
                .expect("case information size fits u32"),
        )
    } == 0
    {
        return Err(unknown("filesystem case sensitivity is unavailable"));
    }
    Ok(info.Flags & 1 != 0)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn case_sensitive(_: &Dir) -> io::Result<bool> {
    Err(unknown("filesystem case sensitivity is unavailable"))
}

#[cfg(windows)]
fn reject_unsupported_windows_names(path: &Path) -> io::Result<()> {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if name.ends_with(['.', ' ']) || name.contains(':') {
                return Err(unknown(
                    "cannot establish Windows trailing-dot, trailing-space or stream entry identity",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &Path) -> io::Result<Entry> {
        EntryInspector::default().entry(path)
    }

    fn inspection_config(root: &Path, session: Option<&Path>) -> RunConfig {
        RunConfig::try_from(hoimin_core::RawRunConfig {
            root: camino::Utf8PathBuf::from_path_buf(root.to_owned()).unwrap(),
            files: vec!["source.py".into()],
            test_argv: vec![hoimin_core::CommandArg::Unix(b"test".to_vec())],
            session: session.map(|path| hoimin_core::SessionConfig {
                path: camino::Utf8PathBuf::from_path_buf(path.to_owned()).unwrap(),
            }),
            output: hoimin_core::OutputConfig {
                metrics: Some(
                    camino::Utf8PathBuf::from_path_buf(root.join("metrics.json")).unwrap(),
                ),
            },
            ..hoimin_core::RawRunConfig::default()
        })
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap()
    }

    #[test]
    fn an_unresolved_session_prevents_authorizing_a_metrics_destination() {
        let temp = tempfile::tempdir().unwrap();
        let config = inspection_config(temp.path(), Some(&temp.path().join("missing/session.db")));

        let error = inspect(&config, &[], &[], &temp.path().join("metrics.json"))
            .err()
            .unwrap();
        let actual_1 = std::fs::read_dir(temp.path());

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(actual_1.unwrap().count(), 0);
    }

    #[test]
    fn a_confirmed_collision_takes_precedence_over_an_unresolved_session() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("metrics.json");
        std::fs::write(&output, b"protected").unwrap();
        let config = inspection_config(temp.path(), Some(&temp.path().join("missing/session.db")));
        let collision = inspect(&config, &[], std::slice::from_ref(&output), &output)
            .unwrap()
            .err()
            .unwrap();

        let actual_1 = std::fs::read(&output);

        assert_eq!(collision, output);
        assert_eq!(actual_1.unwrap(), b"protected");
    }

    #[test]
    fn a_confirmed_collision_takes_precedence_over_an_uninspectable_input() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("metrics.json");
        let blocked = temp.path().join("file");
        std::fs::write(&output, b"protected").unwrap();
        std::fs::write(&blocked, b"obstruction").unwrap();
        let config = inspection_config(temp.path(), None);
        let collision = inspect(
            &config,
            &[],
            &[blocked.join("child"), output.clone()],
            &output,
        )
        .unwrap()
        .err()
        .unwrap();

        let actual_1 = std::fs::read(&output);
        let actual_2 = std::fs::read(blocked);

        assert_eq!(collision, output);
        assert_eq!(actual_1.unwrap(), b"protected");
        assert_eq!(actual_2.unwrap(), b"obstruction");
    }

    #[rstest::rstest]
    #[case::initial_metadata("metrics.entry.metadata")]
    #[case::absolute_path("metrics.entry.absolute")]
    #[case::canonical_parent("metrics.parent.canonicalize")]
    #[case::open_parent("metrics.entry.open")]
    #[case::leaf_metadata("metrics.entry.leaf")]
    #[case::index_metadata("metrics.index.metadata")]
    #[case::index_open("metrics.index.entries")]
    #[case::index_iteration("metrics.index.collect")]
    fn entry_io_errors_preserve_the_source_and_allow_retry(#[case] point: &'static str) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("metrics.json");
        std::fs::write(&path, b"original").unwrap();
        let fault = crate::io_faults::arm(point);
        let result = entry(&path);
        let triggered = fault.triggered();
        let error = result.err().unwrap();
        let retried = entry(&path).unwrap();

        assert!(triggered, "unchecked metrics I/O boundary: {point}");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(error.to_string().contains(point));
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        assert!(retried.exists);
    }

    #[cfg(unix)]
    #[test]
    fn cached_directory_index_rejects_an_entry_created_after_inspection() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        let added = temp.path().join("added");
        std::fs::write(&original, b"original").unwrap();
        let mut inspector = EntryInspector::default();
        let first = inspector.entry(&original).unwrap();
        std::fs::write(&added, b"added").unwrap();

        let error = inspector.entry(&added).err().unwrap();

        assert!(first.exists);
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            error.to_string(),
            "final entry disappeared during inspection"
        );
        assert_eq!(std::fs::read(original).unwrap(), b"original");
        assert_eq!(std::fs::read(added).unwrap(), b"added");
    }

    #[cfg(unix)]
    #[test]
    fn cached_directory_index_preserves_a_removed_entry_lookup_error() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        let added = temp.path().join("added");
        std::fs::write(&original, b"original").unwrap();
        let mut inspector = EntryInspector::default();
        let first = inspector.entry(&original).unwrap();
        std::fs::remove_file(&original).unwrap();
        std::fs::write(&added, b"added").unwrap();

        let error = inspector.entry(&added).err().unwrap();

        assert!(first.exists);
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(std::fs::read(added).unwrap(), b"added");
    }

    #[cfg(unix)]
    #[test]
    fn cached_directory_index_rejects_an_ambiguous_new_hardlink() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        let alias = temp.path().join("alias");
        let added = temp.path().join("added");
        std::fs::write(&original, b"original").unwrap();
        std::fs::hard_link(&original, &alias).unwrap();
        let mut inspector = EntryInspector::default();
        let first = inspector.entry(&original).unwrap();
        std::fs::hard_link(&original, &added).unwrap();

        let error = inspector.entry(&added).err().unwrap();

        assert!(first.exists);
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "ambiguous final entry alias");
        assert_eq!(std::fs::read(original).unwrap(), b"original");
        assert_eq!(std::fs::read(alias).unwrap(), b"original");
        assert_eq!(std::fs::read(added).unwrap(), b"original");
    }

    #[rstest::rstest]
    #[case::left("\u{00e9}.json", "metrics.json")]
    #[case::right("metrics.json", "\u{00e9}.json")]
    fn prospective_non_ascii_names_preserve_uncertainty(#[case] left: &str, #[case] right: &str) {
        let temp = tempfile::tempdir().unwrap();
        let left = entry(&temp.path().join(left)).unwrap();
        let right = entry(&temp.path().join(right)).unwrap();

        let error = same_entry(&left, &right).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            error.to_string(),
            "cannot compare prospective non-ASCII entry names"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn prospective_parent_rejects_a_missing_component_followed_by_parent_navigation() {
        let temp = tempfile::tempdir().unwrap();

        let error = prospective_parent(&temp.path().join("missing/..")).unwrap_err();
        let actual_1 = temp.path().join("missing").exists();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!actual_1);
    }

    #[test]
    fn entry_inspection_preserves_a_non_directory_parent_error() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("file");
        std::fs::write(&parent, b"original").unwrap();

        let error = entry(&parent.join("metrics.json")).err().unwrap();
        let actual_1 = std::fs::read(parent);

        assert_eq!(error.kind(), io::ErrorKind::NotADirectory);
        assert_eq!(actual_1.unwrap(), b"original");
    }

    #[test]
    fn root_entry_inspection_preserves_the_absence_of_a_parent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().ancestors().last().unwrap();

        let inspected = entry(root).unwrap();

        assert!(inspected.exists);
        assert!(inspected.parent.is_none());
        assert_eq!(inspected.path, std::fs::canonicalize(root).unwrap());
    }

    #[test]
    fn prospective_case_aliases_with_a_missing_parent_preserve_uncertainty() {
        let temp = tempfile::tempdir().unwrap();
        let left = entry(&temp.path().join("missing/metrics.json")).unwrap();
        let right = entry(&temp.path().join("missing/METRICS.JSON")).unwrap();

        let error = same_entry(&left, &right).unwrap_err();
        let actual_1 = std::fs::read_dir(temp.path());

        assert_eq!(error.to_string(), "missing parent");
        assert_eq!(actual_1.unwrap().count(), 0);
    }

    #[rstest::rstest]
    #[case::left_missing("missing/metrics.json", "metrics.json")]
    #[case::right_missing("metrics.json", "missing/metrics.json")]
    #[case::different_missing_parents("missing/metrics.json", "other/metrics.json")]
    fn missing_parents_do_not_alias_a_distinct_parent(#[case] left: &str, #[case] right: &str) {
        let temp = tempfile::tempdir().unwrap();
        let left = entry(&temp.path().join(left)).unwrap();
        let right = entry(&temp.path().join(right)).unwrap();

        let actual_0 = same_parent(&left, &right);
        let actual_1 = same_entry(&left, &right);
        let actual_2 = std::fs::read_dir(temp.path());

        assert!(!actual_0.unwrap());
        assert!(!actual_1.unwrap());
        assert_eq!(actual_2.unwrap().count(), 0);
    }

    #[rstest::rstest]
    #[case::alias("alias/", false, true)]
    #[case::alias_2("alias/.", false, true)]
    #[case::alias_3("alias/..", false, true)]
    #[case::value(".", false, true)]
    #[case::value_5("..", false, true)]
    #[case::alias_metrics("alias/.metrics", false, false)]
    #[case::alias_7("alias/...", false, false)]
    #[case::alias_metrics_json("alias/../metrics.json", false, false)]
    #[case::r_alias(r"alias\.", false, false)]
    #[case::r_alias_10(r"alias\.", true, true)]
    #[case::r_alias_child(r"alias\child/..", true, true)]
    #[case::r_alias_child_12(r"alias/child\.", true, true)]
    #[case::r_alias_child_13(r"alias/child\", true, true)]
    #[case::r_alias_child_metrics_json(r"alias/child\metrics.json", true, false)]
    fn directory_only_syntax_preserves_terminal_components_and_separator_policy(
        #[case] path: &str,
        #[case] windows: bool,
        #[case] expected: bool,
    ) {
        assert_eq!(
            directory_only_syntax(Path::new(path), windows),
            expected,
            "{path:?}, windows={windows}"
        );
    }

    #[test]
    fn native_lookup_keeps_distinct_hardlink_entries_and_resolves_existing_case_aliases() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Source.py");
        std::fs::write(&source, b"original").unwrap();
        let case_alias = temp.path().join("SOURCE.PY");

        assert_eq!(
            same_entry(&entry(&source).unwrap(), &entry(&case_alias).unwrap()).unwrap(),
            case_alias.exists(),
        );
        let sibling = temp.path().join("Sibling.py");
        std::fs::hard_link(&source, &sibling).unwrap();
        assert!(!same_entry(&entry(&source).unwrap(), &entry(&sibling).unwrap()).unwrap());
        #[cfg(unix)]
        if case_alias.exists() {
            assert!(
                entry(&case_alias).is_err(),
                "inexact names sharing an inode require withholding"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn shared_directory_uses_one_name_index_for_selected_entries() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["a.py", "b.py", "c.py"] {
            std::fs::write(temp.path().join(name), b"source").unwrap();
        }
        let mut inspector = EntryInspector::default();
        for name in ["a.py", "b.py", "c.py"] {
            assert!(inspector.entry(&temp.path().join(name)).unwrap().exists);
        }
        assert_eq!(inspector.directories.len(), 1);
        let directory = inspector.directories.values().next().unwrap();
        assert_eq!(directory.names.len(), 3);
        assert!(
            directory.aliases.is_none(),
            "exact names need no inode scan"
        );
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn prospective_names_follow_native_directory_case_behavior() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("Probe"), b"native case probe").unwrap();
        let insensitive = temp.path().join("PROBE").exists();
        let left = entry(&temp.path().join("session.db")).unwrap();
        let right = entry(&temp.path().join("SESSION.DB")).unwrap();

        assert!(!left.exists && !right.exists);
        assert_eq!(same_entry(&left, &right).unwrap(), insensitive);
        assert!(!temp.path().join("session.db").exists());
    }

    #[cfg(windows)]
    #[rstest::rstest]
    #[case::configured_db_1(0)]
    #[case::original_db_2(1)]
    fn windows_session_symlink_sidecars_protect_configured_and_canonical_entries_scenario_1(
        #[case] scenario_index: usize,
        #[values(0, 1, 2)] nested_scenario_index: usize,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("original.db");
        std::fs::write(&database, b"database entry").unwrap();
        let configured = temp.path().join("configured.db");
        if let Err(error) = std::os::windows::fs::symlink_file(&database, &configured) {
            assert!(
                std::env::var_os("HOIMIN_REQUIRE_WINDOWS_SYMLINKS").is_none(),
                "required Windows session symlink setup failed: {error}"
            );
            eprintln!("SKIP Windows session symlink namespace: symlink setup unavailable: {error}");
            return;
        }
        let (files, trees) =
            session_artifacts(&configured, &mut EntryInspector::default()).unwrap();
        let parent = std::fs::canonicalize(temp.path()).unwrap();
        assert!(files.contains(&parent.join("configured.db-wal")));

        let basename = ["configured.db", "original.db"]
            .into_iter()
            .nth(scenario_index)
            .unwrap();

        let suffix = ["-wal", "-shm", "-journal"]
            .into_iter()
            .nth(nested_scenario_index)
            .unwrap();

        assert!(files.contains(&parent.join(format!("{basename}{suffix}"))));
    }

    #[cfg(windows)]
    #[test]
    fn windows_session_symlink_sidecars_protect_configured_and_canonical_entries() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("original.db");
        std::fs::write(&database, b"database entry").unwrap();
        let configured = temp.path().join("configured.db");
        if let Err(error) = std::os::windows::fs::symlink_file(&database, &configured) {
            assert!(
                std::env::var_os("HOIMIN_REQUIRE_WINDOWS_SYMLINKS").is_none(),
                "required Windows session symlink setup failed: {error}"
            );
            eprintln!("SKIP Windows session symlink namespace: symlink setup unavailable: {error}");
            return;
        }
        let (files, trees) =
            session_artifacts(&configured, &mut EntryInspector::default()).unwrap();
        let parent = std::fs::canonicalize(temp.path()).unwrap();
        assert!(files.contains(&parent.join("configured.db-wal")));

        assert!(trees.contains(&parent.join(".original.db.hoimin-locks")));
    }
}
