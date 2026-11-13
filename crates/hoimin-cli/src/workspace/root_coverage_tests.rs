use super::*;

#[rstest::rstest]
#[case::empty("")]
#[case::traversal("../outside")]
fn native_parent_open_rejects_invalid_relative_paths(#[case] path: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    assert!(matches!(
        root.open_native_parent(Path::new(path)),
        Err(WorkspaceError::InvalidPath { .. })
    ));
}

#[test]
fn native_parent_open_rejects_absolute_paths() {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    assert!(matches!(
        root.open_native_parent(directory.path()),
        Err(WorkspaceError::InvalidPath { .. })
    ));
}

#[cfg(unix)]
#[test]
fn native_parent_open_rejects_symlink_ancestors() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join("link")).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    assert!(matches!(
        root.open_native_parent(Path::new("link/target")),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn native_parent_open_reports_missing_directories() {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    assert!(matches!(
        root.open_native_parent(Path::new("missing/target")),
        Err(WorkspaceError::Io { .. })
    ));
}

#[test]
fn missing_entry_validation_requires_an_actual_directory_parent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file");
    std::fs::write(&path, b"existing").unwrap();
    let file = File::open(path).unwrap();
    let error =
        WorkerRoot::validate_missing_entry_parent(&file, Utf8Path::new("child")).unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Io { operation: "inspect worker entry", path, message } if path == "child" && message.contains("not a directory"))
    );
}

#[cfg(unix)]
#[test]
fn file_removal_rejects_replaced_identity_and_preserves_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    std::fs::write(directory.path().join("target"), b"original").unwrap();
    let name = OsString::from("target");
    let metadata =
        cap_primitives::fs::stat(root.handle(), Path::new(&name), FollowSymlinks::No).unwrap();
    std::fs::rename(
        directory.path().join("target"),
        directory.path().join("original"),
    )
    .unwrap();
    std::fs::write(directory.path().join("target"), b"replacement").unwrap();
    assert!(matches!(
        WorkerRoot::remove_non_directory(root.handle(), &name, Utf8Path::new("target"), &metadata),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(
        std::fs::read(directory.path().join("target")).unwrap(),
        b"replacement"
    );
    assert_eq!(
        std::fs::read(directory.path().join("original")).unwrap(),
        b"original"
    );
}

#[cfg(unix)]
#[test]
fn readonly_mutation_target_is_reopened_only_after_permission_repair() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target");
    std::fs::write(&path, b"original").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let mutation = root.open_mutation_file(Utf8Path::new("target")).unwrap();
    assert!(mutation.reopen.is_some());
    let mut writable = mutation.into_writable(Utf8Path::new("target")).unwrap();
    writable.write_all(b"modified").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"modified");
    assert_ne!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o200,
        0
    );
}

#[cfg(unix)]
#[test]
fn readonly_mutation_reopen_preserves_a_replacement_identity() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target");
    std::fs::write(&path, b"original").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let mutation = root.open_mutation_file(Utf8Path::new("target")).unwrap();
    std::fs::rename(&path, directory.path().join("original")).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    assert!(matches!(
        mutation.into_writable(Utf8Path::new("target")),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"replacement");
    assert_eq!(
        std::fs::read(directory.path().join("original")).unwrap(),
        b"original"
    );
}

#[cfg(unix)]
#[test]
fn mutation_open_reports_unreadable_files() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target");
    std::fs::write(&path, b"original").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let denied = root.open_mutation_file(Utf8Path::new("target"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        denied,
        Err(WorkspaceError::Io {
            operation: "open mutation target",
            ..
        })
    ));
}

#[test]
fn mutation_open_reports_directory_targets() {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    std::fs::create_dir(directory.path().join("directory")).unwrap();
    assert!(matches!(
        root.open_mutation_file(Utf8Path::new("directory")),
        Err(WorkspaceError::Io {
            operation: "open mutation target",
            ..
        })
    ));
}

#[test]
fn mutation_read_reports_real_file_read_errors_with_target_context() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target");
    std::fs::write(&path, b"payload").unwrap();
    let mut mutation = MutationFile {
        file: File::options().write(true).open(path).unwrap(),
        reopen: None,
    };
    let error = mutation
        .read_to_end(&mut Vec::new(), 7, Utf8Path::new("target"))
        .unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Io { operation: "read mutation target", path, .. } if path == "target")
    );
}

#[cfg(unix)]
#[test]
fn removal_directory_open_rejects_stale_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let path = directory.path().join("target");
    std::fs::create_dir(&path).unwrap();
    let name = OsString::from("target");
    let metadata =
        cap_primitives::fs::stat(root.handle(), Path::new(&name), FollowSymlinks::No).unwrap();
    std::fs::rename(&path, directory.path().join("original")).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(matches!(
        open_removal_directory(root.handle(), &name, Utf8Path::new("target"), &metadata),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(directory.path().join("original").is_dir());
}

#[cfg(unix)]
#[test]
fn removal_directory_open_rejects_non_directory_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let path = directory.path().join("target");
    std::fs::create_dir(&path).unwrap();
    let name = OsString::from("target");
    let metadata =
        cap_primitives::fs::stat(root.handle(), Path::new(&name), FollowSymlinks::No).unwrap();
    std::fs::rename(&path, directory.path().join("original")).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    assert!(matches!(
        open_removal_directory(root.handle(), &name, Utf8Path::new("target"), &metadata),
        Err(WorkspaceError::Io {
            operation: "open worker directory",
            ..
        })
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"replacement");
}

#[cfg(unix)]
#[test]
fn inaccessible_directory_repair_rejects_owner_accessible_metadata() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target");
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let name = OsString::from("target");
    let metadata =
        cap_primitives::fs::stat(root.handle(), Path::new(&name), FollowSymlinks::No).unwrap();
    let error = make_inaccessible_directory_accessible(
        root.handle(),
        &name,
        &metadata,
        Utf8Path::new("target"),
    )
    .unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Io { operation: "open worker directory", message, .. } if message.contains("despite owner access bits"))
    );
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[cfg(unix)]
#[test]
fn owner_inaccessible_directory_can_be_repaired_and_removed() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("nested"), b"payload").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    root.remove_any_if_exists(Utf8Path::new("target")).unwrap();
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn inaccessible_directory_identity_is_rechecked_after_permission_repair() {
    use std::os::unix::fs::PermissionsExt;
    struct ReplaceDuringRepair {
        root: PathBuf,
    }
    impl WorkspaceRaceHook for ReplaceDuringRepair {
        fn parent_opened(&self, operation: &'static str, _: &Utf8Path) {
            if operation == "cleanup-inaccessible-directory" {
                std::fs::set_permissions(
                    self.root.join("target"),
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap();
                std::fs::rename(self.root.join("target"), self.root.join("held")).unwrap();
                std::fs::create_dir(self.root.join("target")).unwrap();
                std::fs::write(self.root.join("target/preserved"), b"replacement").unwrap();
            }
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target");
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let hook = std::sync::Arc::new(ReplaceDuringRepair {
        root: directory.path().to_owned(),
    });
    let _guard = install_workspace_race_hook(hook);
    let result = root.remove_any_if_exists(Utf8Path::new("target"));
    std::fs::set_permissions(
        directory.path().join("held"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
    assert_eq!(
        std::fs::read(path.join("preserved")).unwrap(),
        b"replacement"
    );
}

#[test]
fn removal_depth_limit_preserves_the_unvisited_child() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("target"), b"preserved").unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let child = cap_primitives::fs::read_base_dir(root.handle())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let mut stack = vec![RemovalFrame {
        name: OsString::from("root"),
        logical_path: Some(Utf8PathBuf::new()),
        error_path: Utf8PathBuf::new(),
        directory: root.handle().try_clone().unwrap(),
        entries: cap_primitives::fs::read_base_dir(root.handle()).unwrap(),
        depth: MAX_WORKER_TREE_DEPTH,
    }];
    assert!(matches!(
        WorkerRoot::process_removal_child(&mut stack, &child),
        Err(WorkspaceError::TreeDepthExceeded {
            limit: MAX_WORKER_TREE_DEPTH,
            ..
        })
    ));
    assert_eq!(
        std::fs::read(directory.path().join("target")).unwrap(),
        b"preserved"
    );
}

#[cfg(unix)]
#[test]
fn root_access_checks_reject_links() {
    let directory = tempfile::tempdir().unwrap();
    let file_path = directory.path().join("target");
    std::fs::write(&file_path, b"preserved").unwrap();
    std::os::unix::fs::symlink("target", directory.path().join("link")).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    assert!(matches!(
        root.try_exists(Utf8Path::new("link")),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(std::fs::read(file_path).unwrap(), b"preserved");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::exists(|root: &WorkerRoot| root.try_exists(Utf8Path::new("target")).map(drop))]
#[case::entries(|root: &WorkerRoot| root.entries().map(drop))]
#[case::removal(|root: &WorkerRoot| root.remove_any_if_exists(Utf8Path::new("target")))]
#[case::snapshot(|root: &WorkerRoot| root.snapshot_matches_reader(
    Utf8Path::new("target"), &mut io::Cursor::new(b"preserved"), 9,
    crate::workspace::permission_fingerprint(&<std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600)),
).map(drop))]
fn root_access_checks_report_revoked_search_permissions(
    #[case] action: fn(&WorkerRoot) -> Result<(), WorkspaceError>,
) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let file_path = directory.path().join("target");
    std::fs::write(&file_path, b"preserved").unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o400)).unwrap();
    let result = action(&root);
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(result, Err(WorkspaceError::Io { .. })));
    assert_eq!(std::fs::read(file_path).unwrap(), b"preserved");
}

#[test]
fn snapshot_verification_reports_expected_stream_read_errors() {
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected expected snapshot read failure"))
        }
    }
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("target"), b"payload").unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let permissions = crate::workspace::permission_fingerprint(
        &std::fs::metadata(directory.path().join("target"))
            .unwrap()
            .permissions(),
    );
    let error = root
        .snapshot_matches_reader(Utf8Path::new("target"), &mut BrokenReader, 7, permissions)
        .unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Io { operation: "verify restored file", message, .. } if message.contains("injected expected snapshot read failure"))
    );
}

#[rstest::rstest]
#[case::metadata("worker.mutation.inspect", "inspect mutation target")]
#[case::allocation("worker.mutation.reserve", "read mutation target")]
fn mutation_read_preparation_failure_preserves_input_and_allows_retry(
    #[case] point: &'static str,
    #[case] operation: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target.py"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target.py");
    let mut file = root.open_mutation_file(logical).unwrap();
    let fault = crate::workspace::owned::io_faults::arm(point);
    let mut bytes = Vec::new();
    let result = file.read_to_end(&mut bytes, 8, logical);
    assert!(
        fault.triggered(),
        "mutation read preparation boundary was not checked"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: actual, message, ..
    }) if actual == operation && message.contains(point)));
    assert_eq!(bytes, b"");
    assert_eq!(std::fs::read(path.join("target.py")).unwrap(), b"original");
    drop(fault);
    file.read_to_end(&mut bytes, 8, logical).unwrap();
    assert_eq!(bytes, b"original");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::existing(
    "worker.write.inspect_existing",
    |path| std::fs::write(path, b"original"),
    b"original",
)]
#[case::created("worker.write.inspect_created", |_| Ok(()), b"")]
#[test]
fn worker_write_metadata_failure_keeps_a_retryable_file(
    #[case] point: &'static str,
    #[case] prepare: fn(camino::Utf8PathBuf) -> std::io::Result<()>,
    #[case] expected_after_failure: &[u8],
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    prepare(path.join("target.py")).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target.py");
    let fault = crate::workspace::owned::io_faults::arm(point);
    let result = root.write(logical, b"replacement");
    assert!(
        fault.triggered(),
        "worker metadata boundary was not checked: {point}"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "inspect worker file", message, ..
    }) if message.contains(point)));
    assert_eq!(root.read(logical).unwrap(), expected_after_failure);
    drop(fault);
    root.write(logical, b"replacement").unwrap();
    assert_eq!(root.read(logical).unwrap(), b"replacement");
}

#[cfg(unix)]
type RemoveWorkerEntry = fn(&WorkerRoot, &Utf8Path) -> Result<(), WorkspaceError>;

#[cfg(unix)]
#[rstest::rstest]
#[case::file("worker.remove.inspect", WorkerRoot::remove_file)]
#[case::cleanup("worker.cleanup.inspect_file", WorkerRoot::remove_any_if_exists)]
#[test]
fn worker_removal_metadata_failure_preserves_the_file_until_retry(
    #[case] point: &'static str,
    #[case] remove: RemoveWorkerEntry,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target.py"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target.py");
    let fault = crate::workspace::owned::io_faults::arm(point);
    let result = remove(&root, logical);
    assert!(
        fault.triggered(),
        "removal metadata boundary was not checked: {point}"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "inspect worker file", message, ..
    }) if message.contains(point)));
    assert_eq!(root.read(logical).unwrap(), b"original");
    drop(fault);
    remove(&root, logical).unwrap();
    assert!(!root.try_exists(logical).unwrap());
}

#[cfg(unix)]
fn create_fifo(path: &Utf8Path) {
    let native = std::ffi::CString::new(path.as_str()).unwrap();
    // SAFETY: native is a live NUL-terminated pathname in the owned temporary directory.
    let result = unsafe { libc::mkfifo(native.as_ptr(), 0o600) };
    assert_eq!(result, 0, "{}", std::io::Error::last_os_error());
}

#[cfg(unix)]
#[test]
fn worker_write_rejects_a_fifo_replacement_after_initial_file_inspection() {
    use std::os::unix::fs::OpenOptionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target.py");
    let replacement = path.join("replacement.fifo");
    std::fs::write(&target, b"original").unwrap();
    create_fifo(&replacement);
    let mut reader = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&replacement)
        .unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm_action(
        "worker.write.inspect_existing",
        0,
        move || std::fs::rename(replacement, target),
    );
    let logical = Utf8Path::new("target.py");
    let result = root.write(logical, b"replacement");
    assert!(fault.triggered());
    assert!(matches!(result, Err(WorkspaceError::InvalidPath { path }) if path == logical));
    let mut bytes = [0; 16];
    assert_eq!(reader.read(&mut bytes).unwrap(), 0);
}

#[cfg(unix)]
#[test]
fn worker_write_rejects_an_existing_fifo_before_writing() {
    use std::os::unix::fs::FileTypeExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    create_fifo(&path.join("target.py"));
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target.py");
    assert!(matches!(root.write(logical, b"replacement"),
        Err(WorkspaceError::InvalidPath { path }) if path == logical));
    assert!(
        std::fs::symlink_metadata(path.join("target.py"))
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case::initial_links("worker.mutation.links", 0, "inspect mutation target links", false)]
#[case::permissions_metadata("worker.file.inspect", 0, "inspect worker file", false)]
#[case::permissions_write("worker.file.chmod", 0, "prepare worker file", false)]
#[case::original_identity(
    "worker.mutation.reopen.inspect_original",
    0,
    "inspect mutation target",
    true
)]
#[case::writable_identity(
    "worker.mutation.reopen.inspect_writable",
    0,
    "inspect mutation target",
    true
)]
#[case::final_links("worker.mutation.links", 1, "inspect mutation target links", true)]
#[test]
fn mutation_permission_repair_inspection_failure_preserves_bytes_and_can_retry(
    #[case] point: &'static str,
    #[case] skip: usize,
    #[case] operation: &str,
    #[case] expected_owner_write: bool,
) {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target.py");
    std::fs::write(&target, b"original").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target.py");
    let (parent, name) = root.open_parent(logical, false).unwrap();
    // An explicitly read-only descriptor exercises the permission-denied fallback
    // even when the test runner can bypass pathname permissions.
    let file = MutationFile {
        file: File::open(&target).unwrap(),
        reopen: Some((parent, name)),
    };
    let fault = crate::workspace::owned::io_faults::arm_after(point, skip);
    let result = file.into_writable(logical);
    assert!(
        fault.triggered(),
        "permission boundary was not checked: {point}"
    );
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: actual, message, .. })
        if actual == operation && message.contains(point))
    );
    assert_eq!(root.read(logical).unwrap(), b"original");
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o200 != 0,
        expected_owner_write
    );
    drop(fault);
    let mut retry = root
        .open_mutation_file(logical)
        .unwrap()
        .into_writable(logical)
        .unwrap();
    retry.write_all(b"replacement").unwrap();
    assert_eq!(root.read(logical).unwrap(), b"replacement");
}

#[rstest::rstest]
#[case::missing(|_| Ok(()))]
#[case::regular_file(|path| std::fs::write(path, b"preserved"))]
#[test]
fn worker_root_open_rejects_non_directories_with_path_context(
    #[case] prepare: fn(camino::Utf8PathBuf) -> std::io::Result<()>,
) {
    let directory = tempfile::tempdir().unwrap();
    let entry = Utf8Path::from_path(directory.path()).unwrap().join("entry");
    prepare(entry.clone()).unwrap();
    assert!(matches!(WorkerRoot::open(entry.clone()),
        Err(WorkspaceError::Io { operation: "open worker root", path, .. }) if path == entry));
}

type WorkerPathAction = fn(&WorkerRoot, &Utf8Path) -> Result<(), WorkspaceError>;

#[rstest::rstest]
#[case::read(|root: &WorkerRoot, path: &Utf8Path| root.read(path).map(drop))]
#[case::hash(|root: &WorkerRoot, path: &Utf8Path| root.hash(path).map(drop))]
#[case::read_handle(|root: &WorkerRoot, path: &Utf8Path| root.with_read_file(path, Ok::<File, WorkspaceError>).map(drop))]
#[case::mutation(|root: &WorkerRoot, path: &Utf8Path| root.open_mutation_file(path).map(drop))]
#[case::write_callback(|root: &WorkerRoot, path: &Utf8Path| root.with_write_file(path, write_replacement_payload))]
#[case::write(|root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"))]
#[case::remove(WorkerRoot::remove_file)]
#[case::exists(|root: &WorkerRoot, path: &Utf8Path| root.try_exists(path).map(drop))]
#[case::directory(WorkerRoot::ensure_directory)]
#[case::restore(|root: &WorkerRoot, path: &Utf8Path| root.restore(path, b"replacement",
    std::fs::metadata(root.path()).unwrap().permissions()))]
#[case::snapshot(|root: &WorkerRoot, path: &Utf8Path| root.snapshot_matches(path, b"original",
    crate::workspace::permission_fingerprint(&std::fs::metadata(root.path()).unwrap().permissions())).map(drop))]
#[test]
fn worker_file_operations_reject_parent_traversal_without_touching_sibling_files(
    #[case] action: WorkerPathAction,
) {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let worker = parent.join("worker");
    std::fs::create_dir(&worker).unwrap();
    let outside = parent.join("outside.txt");
    std::fs::write(&outside, b"outside sentinel").unwrap();
    let root = WorkerRoot::open(worker).unwrap();
    let logical = Utf8Path::new("../outside.txt");
    assert!(matches!(action(&root, logical),
        Err(WorkspaceError::InvalidPath { path }) if path == logical));
    assert_eq!(std::fs::read(outside).unwrap(), b"outside sentinel");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::write(0, |path: Utf8PathBuf| std::fs::write(path, b"original").unwrap(),
    |root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"), "target")]
#[case::remove(0, |path: Utf8PathBuf| std::fs::write(path, b"original").unwrap(),
    WorkerRoot::remove_file, "target")]
#[case::clear(0, |path: Utf8PathBuf| std::fs::write(path, b"original").unwrap(),
    |root: &WorkerRoot, _: &Utf8Path| root.clear_for_cleanup(), "target")]
#[case::directory(0, |path: Utf8PathBuf| {
    std::fs::create_dir_all(path.join("a")).unwrap();
    std::fs::write(path.join("a/leaf"), b"original").unwrap();
}, WorkerRoot::remove_any_if_exists, "target/a/leaf")]
#[case::nested_directory(1, |path: Utf8PathBuf| {
    std::fs::create_dir_all(path.join("a")).unwrap();
    std::fs::write(path.join("a/leaf"), b"original").unwrap();
}, WorkerRoot::remove_any_if_exists, "target/a/leaf")]
#[test]
fn worker_directory_metadata_failure_preserves_the_complete_tree(
    #[case] skip: usize,
    #[case] prepare: fn(Utf8PathBuf),
    #[case] action: WorkerPathAction,
    #[case] leaf: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    prepare(path.join("target"));
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let before = root.entries().unwrap();
    let fault = crate::workspace::owned::io_faults::arm_after("worker.directory.inspect", skip);
    let result = action(&root, Utf8Path::new("target"));
    assert!(
        fault.triggered(),
        "directory metadata boundary was not checked"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "inspect worker directory", message, ..
    }) if message.contains("worker.directory.inspect")));
    assert_eq!(root.entries().unwrap(), before);
    assert_eq!(root.read(Utf8Path::new(leaf)).unwrap(), b"original");
}

#[test]
fn native_cleanup_rejects_parent_traversal_with_retained_root_context() {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let worker = parent.join("worker");
    std::fs::create_dir(&worker).unwrap();
    let outside = parent.join("outside.txt");
    std::fs::write(&outside, b"outside sentinel").unwrap();
    let root = WorkerRoot::open(worker.clone()).unwrap();
    let result = root.remove_any_if_exists(Utf8Path::new("../outside.txt"));
    assert!(matches!(result, Err(WorkspaceError::InvalidPath { path }) if path == worker));
    assert_eq!(std::fs::read(outside).unwrap(), b"outside sentinel");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::unrepresentable_parent(None, b"child", None)]
#[case::non_utf8_name(Some("prefix"), b"\xff", None)]
#[case::child(Some("prefix"), b"child", Some("prefix/child"))]
#[case::root_child(Some(""), b"child", Some("child"))]
#[test]
fn native_child_logical_paths_require_both_utf8_components(
    #[case] parent: Option<&str>,
    #[case] name: &[u8],
    #[case] expected: Option<&str>,
) {
    use std::os::unix::ffi::OsStrExt;

    assert_eq!(
        WorkerRoot::logical_child_path_if_utf8(
            parent.map(Utf8Path::new),
            std::ffi::OsStr::from_bytes(name)
        ),
        expected.map(Utf8PathBuf::from),
    );
}

#[cfg(unix)]
#[test]
fn worker_directory_permission_failure_preserves_the_original_mode() {
    use std::os::unix::fs::PermissionsExt;

    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join("directory");
    std::fs::create_dir(&path).unwrap();
    let retained = open_retained_directory(path.as_std_path()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o500)).unwrap();
    let fault = crate::workspace::owned::io_faults::arm("worker.directory.chmod");
    let result = make_directory_writable(&retained, &path);
    let observed_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o700;
    // Restore traversable fixture permissions before any assertion can fail.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        fault.triggered(),
        "directory chmod boundary was not checked"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "prepare worker directory", message, ..
    }) if message.contains("worker.directory.chmod")));
    assert_eq!(observed_mode, 0o500);
}

#[cfg(unix)]
#[rstest::rstest]
#[case::mutation("worker.mutation.open.inspect", "inspect mutation target",
    |root: &WorkerRoot, path: &Utf8Path| root.open_mutation_file(path).map(drop))]
#[case::snapshot("worker.snapshot.inspect_opened", "verify restored file",
    |root: &WorkerRoot, path: &Utf8Path| root.snapshot_matches(path, b"original", 0o600).map(drop))]
#[case::snapshot_entry("worker.snapshot.inspect_entry", "verify restored file",
    |root: &WorkerRoot, path: &Utf8Path| root.snapshot_matches(path, b"original", 0o600).map(drop))]
#[case::snapshot_stream("worker.snapshot.compare", "verify restored file",
    |root: &WorkerRoot, path: &Utf8Path| root.snapshot_matches(path, b"original", 0o600).map(drop))]
#[test]
fn worker_file_io_failure_preserves_the_original_content(
    #[case] point: &'static str,
    #[case] operation: &str,
    #[case] action: WorkerPathAction,
) {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target.py");
    std::fs::write(&target, b"original").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm(point);
    let result = action(&root, Utf8Path::new("target.py"));
    assert!(
        fault.triggered(),
        "opened file metadata boundary was not checked: {point}"
    );
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: actual, message, .. })
        if actual == operation && message.contains(point))
    );
    assert_eq!(std::fs::read(target).unwrap(), b"original");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::mutation(|root: &WorkerRoot, path: &Utf8Path| root.open_mutation_file(path).map(drop))]
#[case::remove(WorkerRoot::remove_file)]
#[test]
fn strict_file_operations_reject_fifo_targets_without_removing_them(
    #[case] action: WorkerPathAction,
) {
    use std::os::unix::fs::FileTypeExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    create_fifo(&path.join("target"));
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    assert!(matches!(action(&root, logical),
        Err(WorkspaceError::InvalidPath { path }) if path == logical));
    assert!(
        std::fs::symlink_metadata(path.join("target"))
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[cfg(unix)]
#[test]
fn native_cleanup_unlinks_fifo_entries() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    create_fifo(&path.join("target"));
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    root.remove_any_if_exists(logical).unwrap();
    assert!(!root.try_exists(logical).unwrap());
}

#[cfg(unix)]
#[test]
fn worker_snapshot_reports_a_file_removed_after_initial_inspection() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target");
    std::fs::write(&target, b"original").unwrap();
    let permissions = std::fs::metadata(&target).unwrap().permissions().mode();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    assert!(
        root.snapshot_matches(logical, b"original", permissions)
            .unwrap()
    );
    let fault =
        crate::workspace::owned::io_faults::arm_action("worker.snapshot.open", 0, move || {
            std::fs::remove_file(target)
        });
    let result = root.snapshot_matches(logical, b"original", permissions);
    assert!(fault.triggered(), "snapshot open boundary was not checked");
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "verify restored file", path, ..
    }) if path == logical));
    assert!(!root.try_exists(logical).unwrap());
}

#[cfg(unix)]
#[test]
fn worker_snapshot_rejects_file_growth_before_reading() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target");
    std::fs::write(&target, b"original").unwrap();
    let permissions = std::fs::metadata(&target).unwrap().permissions().mode();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    assert!(
        root.snapshot_matches(logical, b"original", permissions)
            .unwrap()
    );
    let changed = target.clone();
    let fault = crate::workspace::owned::io_faults::arm_action(
        "worker.snapshot.inspect_opened",
        0,
        move || std::fs::write(changed, b"grown payload"),
    );
    assert!(
        !root
            .snapshot_matches(logical, b"original", permissions)
            .unwrap()
    );
    assert!(fault.triggered());
    assert_eq!(std::fs::read(target).unwrap(), b"grown payload");
}

#[cfg(unix)]
#[test]
fn worker_snapshot_rejects_a_fifo_replacement_without_reading_from_it() {
    use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target");
    let replacement = path.join("replacement");
    std::fs::write(&target, b"").unwrap();
    let permissions = std::fs::metadata(&target).unwrap().permissions().mode();
    create_fifo(&replacement);
    // Keeping a writer open makes an erroneous nonblocking read fail with WouldBlock.
    let _writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&replacement)
        .unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    assert!(root.snapshot_matches(logical, b"", permissions).unwrap());
    let changed = target.clone();
    let fault =
        crate::workspace::owned::io_faults::arm_action("worker.snapshot.open", 0, move || {
            std::fs::rename(replacement, changed)
        });
    assert!(!root.snapshot_matches(logical, b"", permissions).unwrap());
    assert!(fault.triggered(), "snapshot open boundary was not checked");
    assert!(
        std::fs::symlink_metadata(target)
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[rstest::rstest]
#[case::walk_clone("worker.walk.clone", 0, "clone worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.entries().map(drop))]
#[case::walk_open("worker.walk.open", 0, "enumerate worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.entries().map(drop))]
#[case::walk_first_entry("worker.walk.next", 0, "enumerate worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.entries().map(drop))]
#[case::walk_later_entry("worker.walk.next", 1, "enumerate worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.entries().map(drop))]
#[case::walk_stat("worker.walk.stat_entry", 0, "inspect worker entry",
    |root: &WorkerRoot, _: &Utf8Path| root.entries().map(drop))]
#[case::walk_child_open("worker.walk.open_child", 0, "open worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.entries().map(drop))]
#[case::walk_child_read("worker.walk.read_child", 0, "enumerate worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.entries().map(drop))]
#[case::clear_clone("worker.clear.clone", 0, "clone worker root",
    |root: &WorkerRoot, _: &Utf8Path| root.clear_for_cleanup())]
#[case::clear_open("worker.clear.open", 0, "enumerate worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.clear_for_cleanup())]
#[case::clear_first_entry("worker.clear.next", 0, "enumerate worker directory",
    |root: &WorkerRoot, _: &Utf8Path| root.clear_for_cleanup())]
#[case::native_parent_clone(
    "worker.native_parent.clone",
    0,
    "clone worker root",
    WorkerRoot::remove_any_if_exists
)]
#[test]
fn worker_directory_io_failures_preserve_every_original_entry(
    #[case] point: &'static str,
    #[case] skip: usize,
    #[case] operation: &str,
    #[case] action: WorkerPathAction,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::create_dir_all(path.join("target/a")).unwrap();
    std::fs::write(path.join("target/a/leaf"), b"original").unwrap();
    std::fs::write(path.join("sibling"), b"sibling bytes").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let before = root.entries().unwrap();
    let fault = crate::workspace::owned::io_faults::arm_after(point, skip);
    let result = action(&root, Utf8Path::new("target"));
    assert!(
        fault.triggered(),
        "directory I/O boundary was not checked: {point}"
    );
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: actual, message, .. })
        if actual == operation && message.contains(point))
    );
    assert_eq!(root.entries().unwrap(), before);
    assert_eq!(
        root.read(Utf8Path::new("target/a/leaf")).unwrap(),
        b"original"
    );
    assert_eq!(
        root.read(Utf8Path::new("sibling")).unwrap(),
        b"sibling bytes"
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case::write_stat("worker.write.stat_existing", "write worker file",
    |root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"))]
#[case::write_inspection_open("worker.write.open_existing", "open worker file",
    |root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"))]
#[case::write_output_open("worker.write.open_output", "write worker file",
    |root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"))]
#[case::remove_open(
    "worker.remove.open_file",
    "remove worker file",
    WorkerRoot::remove_file
)]
#[case::remove_unlink(
    "worker.remove.unlink_file",
    "remove worker file",
    WorkerRoot::remove_file
)]
#[case::cleanup_open(
    "worker.cleanup.open_file",
    "open worker file",
    WorkerRoot::remove_any_if_exists
)]
#[case::cleanup_unlink(
    "worker.cleanup.unlink_file",
    "remove worker file",
    WorkerRoot::remove_any_if_exists
)]
#[test]
fn worker_file_io_failures_before_mutation_preserve_original_bytes(
    #[case] point: &'static str,
    #[case] operation: &str,
    #[case] action: WorkerPathAction,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    let fault = crate::workspace::owned::io_faults::arm(point);
    let result = action(&root, logical);
    assert!(
        fault.triggered(),
        "file I/O boundary was not checked: {point}"
    );
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: actual, message, .. })
        if actual == operation && message.contains(point))
    );
    assert_eq!(root.read(logical).unwrap(), b"original");
    drop(fault);
    action(&root, logical).unwrap();
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the write callback requires Result; fixture I/O failure must fail the test"
)]
fn write_replacement_payload(file: &mut File) -> Result<(), WorkspaceError> {
    file.write_all(b"replacement").unwrap();
    Ok(())
}

#[test]
fn worker_write_callback_persists_exactly_the_supplied_payload() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    root.with_write_file(logical, write_replacement_payload)
        .unwrap();
    assert_eq!(root.read(logical).unwrap(), b"replacement");
}

#[cfg(unix)]
#[test]
fn failed_worker_payload_write_can_be_retried_after_truncation() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    let fault = crate::workspace::owned::io_faults::arm("worker.write.bytes");
    let result = root.write(logical, b"replacement");
    assert!(fault.triggered(), "payload write boundary was not checked");
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "write worker file", message, ..
    }) if message.contains("worker.write.bytes")));
    assert_eq!(root.read(logical).unwrap(), b"");
    drop(fault);
    root.write(logical, b"replacement").unwrap();
    assert_eq!(root.read(logical).unwrap(), b"replacement");
}

#[cfg(unix)]
#[test]
fn mutation_reopen_reports_namespace_removal_after_permission_repair() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target");
    std::fs::write(&target, b"original").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
    let mut original = File::open(&target).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    let (parent, name) = root.open_parent(logical, false).unwrap();
    let mutation = MutationFile {
        file: File::open(&target).unwrap(),
        reopen: Some((parent, name)),
    };
    let fault = crate::workspace::owned::io_faults::arm_action(
        "worker.mutation.reopen.inspect_original",
        0,
        move || std::fs::remove_file(target),
    );
    let result = mutation.into_writable(logical);
    assert!(fault.triggered());
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "open mutation target", path, ..
    }) if path == logical));
    assert!(!root.try_exists(logical).unwrap());
    let mut bytes = Vec::new();
    original.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::write_before_truncate(0, |root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"), b"original")]
#[case::write_after_truncate(1, |root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"), b"")]
#[case::remove(0, WorkerRoot::remove_file, b"original")]
#[test]
fn worker_file_permission_inspection_errors_retain_the_correct_progress(
    #[case] skip: usize,
    #[case] action: WorkerPathAction,
    #[case] expected_bytes: &[u8],
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    let fault = crate::workspace::owned::io_faults::arm_after("worker.file.inspect", skip);
    let result = action(&root, logical);
    assert!(fault.triggered());
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "inspect worker file", message, ..
    }) if message.contains("worker.file.inspect")));
    assert_eq!(root.read(logical).unwrap(), expected_bytes);
    drop(fault);
    action(&root, logical).unwrap();
}

#[test]
fn directory_creation_rejects_parent_metadata_failure_before_creating_any_component() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm("worker.directory.inspect");
    let result = root.ensure_directory(Utf8Path::new("missing/nested"));
    assert!(fault.triggered());
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "inspect worker directory", message, ..
    }) if message.contains("worker.directory.inspect")));
    assert!(!root.try_exists(Utf8Path::new("missing")).unwrap());
    drop(fault);
    root.ensure_directory(Utf8Path::new("missing/nested"))
        .unwrap();
    assert!(
        std::fs::metadata(path.join("missing/nested"))
            .unwrap()
            .is_dir()
    );
}

#[test]
fn directory_creation_reports_syscall_errors_without_publishing_a_partial_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm("worker.parent.mkdir");
    let result = root.ensure_directory(Utf8Path::new("missing/nested"));
    assert!(
        fault.triggered(),
        "directory create boundary was not checked"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "open worker parent", message, ..
    }) if message.contains("worker.parent.mkdir")));
    assert!(!root.try_exists(Utf8Path::new("missing")).unwrap());
    drop(fault);
    root.ensure_directory(Utf8Path::new("missing/nested"))
        .unwrap();
    assert!(
        std::fs::metadata(path.join("missing/nested"))
            .unwrap()
            .is_dir()
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case::initial_read("worker.removal.read_root", 0, "enumerate worker directory")]
#[case::next_entry("worker.removal.next", 0, "enumerate worker directory")]
#[case::child_stat("worker.removal.stat_child", 0, "inspect worker entry")]
#[case::initial_open("worker.removal.open", 0, "open worker directory")]
#[case::child_open("worker.removal.open", 1, "open worker directory")]
#[case::initial_metadata("worker.removal.inspect", 0, "inspect worker directory")]
#[case::child_metadata("worker.removal.inspect", 1, "inspect worker directory")]
#[case::child_read("worker.removal.read_child", 0, "enumerate worker directory")]
#[test]
fn native_directory_removal_io_failures_preserve_unvisited_contents(
    #[case] point: &'static str,
    #[case] skip: usize,
    #[case] operation: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::create_dir_all(path.join("target/a")).unwrap();
    std::fs::write(path.join("target/a/leaf"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let before = root.entries().unwrap();
    let fault = crate::workspace::owned::io_faults::arm_after(point, skip);
    let result = root.remove_any_if_exists(Utf8Path::new("target"));
    assert!(
        fault.triggered(),
        "directory removal boundary was not checked: {point}"
    );
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: actual, message, .. })
        if actual == operation && message.contains(point))
    );
    assert_eq!(root.entries().unwrap(), before);
    assert_eq!(
        root.read(Utf8Path::new("target/a/leaf")).unwrap(),
        b"original"
    );
    drop(fault);
    root.remove_any_if_exists(Utf8Path::new("target")).unwrap();
    assert!(!root.try_exists(Utf8Path::new("target")).unwrap());
}

#[cfg(unix)]
#[test]
fn native_empty_directory_removal_failure_is_retryable() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::create_dir(path.join("target")).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let logical = Utf8Path::new("target");
    let fault = crate::workspace::owned::io_faults::arm("worker.removal.rmdir");
    let result = root.remove_any_if_exists(logical);
    assert!(
        fault.triggered(),
        "empty directory removal boundary was not checked"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "remove worker directory", message, ..
    }) if message.contains("worker.removal.rmdir")));
    assert!(root.try_exists(logical).unwrap());
    drop(fault);
    root.remove_any_if_exists(logical).unwrap();
    assert!(!root.try_exists(logical).unwrap());
}

#[cfg(unix)]
#[test]
fn cleanup_diagnostics_use_the_retained_ancestor_when_the_logical_prefix_is_unknown() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let child = cap_primitives::fs::read_base_dir(root.handle())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let retained = root.handle().try_clone().unwrap();
    let entries = cap_primitives::fs::read_base_dir(&retained).unwrap();
    let mut stack = vec![RemovalFrame {
        name: OsString::from("worker"),
        logical_path: None,
        error_path: path.to_owned(),
        directory: retained,
        entries,
        depth: 0,
    }];
    let fault = crate::workspace::owned::io_faults::arm("worker.cleanup.inspect_file");
    let result = WorkerRoot::process_removal_child(&mut stack, &child);
    assert!(fault.triggered());
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "inspect worker file", path: error_path, ..
    }) if error_path == path));
    assert_eq!(std::fs::read(path.join("target")).unwrap(), b"original");
    assert_eq!(stack.len(), 1);
}

#[cfg(unix)]
#[test]
fn worker_entries_classify_directory_symlinks_without_traversing_their_targets() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("protected"), b"outside sentinel").unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::os::unix::fs::symlink(outside.path(), path.join("link")).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let entries = root.entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, WorkerEntryKind::LinkOrReparse);
    assert_eq!(
        entries[0].logical_path.as_deref(),
        Some(Utf8Path::new("link"))
    );
    assert_eq!(
        std::fs::read(outside.path().join("protected")).unwrap(),
        b"outside sentinel"
    );
}

#[cfg(unix)]
#[test]
fn worker_entries_classify_fifo_entries_as_special_entries() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    create_fifo(&path.join("pipe"));
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let entries = root.entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, WorkerEntryKind::LinkOrReparse);
    assert_eq!(
        entries[0].logical_path.as_deref(),
        Some(Utf8Path::new("pipe"))
    );
}

#[cfg(unix)]
#[test]
fn recursive_native_cleanup_unlinks_symlinks_without_entering_outside_directories() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("protected"), b"outside sentinel").unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::create_dir(path.join("target")).unwrap();
    std::os::unix::fs::symlink(outside.path(), path.join("target/link")).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    root.remove_any_if_exists(Utf8Path::new("target")).unwrap();
    assert!(!root.try_exists(Utf8Path::new("target")).unwrap());
    assert_eq!(
        std::fs::read(outside.path().join("protected")).unwrap(),
        b"outside sentinel"
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case::mutation("open mutation target", |root: &WorkerRoot, path: &Utf8Path| root.open_mutation_file(path).map(drop))]
#[case::remove("remove worker file", WorkerRoot::remove_file)]
#[test]
fn strict_file_operations_report_missing_entries_without_creating_them(
    #[case] operation: &str,
    #[case] action: WorkerPathAction,
) {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let logical = Utf8Path::new("missing");
    assert!(matches!(action(&root, logical),
        Err(WorkspaceError::Io { operation: actual, path, .. }) if actual == operation && path == logical));
    assert!(!root.try_exists(logical).unwrap());
}

#[rstest::rstest]
#[case::write(|root: &WorkerRoot, path: &Utf8Path| root.write(path, b"replacement"))]
#[case::cleanup(WorkerRoot::remove_any_if_exists)]
#[test]
fn missing_entry_parent_inspection_failure_cannot_establish_safe_absence(
    #[case] action: WorkerPathAction,
) {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    let logical = Utf8Path::new("missing");
    let fault = crate::workspace::owned::io_faults::arm("worker.parent.inspect_missing");
    let result = action(&root, logical);
    assert!(
        fault.triggered(),
        "missing parent metadata boundary was not checked"
    );
    assert!(matches!(result, Err(WorkspaceError::Io {
        operation: "inspect worker parent", message, ..
    }) if message.contains("worker.parent.inspect_missing")));
    assert!(!root.try_exists(logical).unwrap());
}

#[cfg(unix)]
#[rstest::rstest]
#[case::symlink(|path: &Utf8Path| std::os::unix::fs::symlink("held", path).unwrap())]
#[case::directory(|path: &Utf8Path| std::fs::create_dir(path).unwrap())]
#[case::file(|path: &Utf8Path| std::fs::write(path, b"replacement").unwrap())]
fn cleanup_rechecks_identity_after_a_permission_denied_open(#[case] replace: fn(&Utf8Path)) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target");
    std::fs::write(&target, b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let replacement = target.clone();
    let held = path.join("held");
    let fault =
        crate::workspace::owned::io_faults::arm_action("worker.cleanup.open_file", 0, move || {
            std::fs::rename(&replacement, &held).unwrap();
            replace(&replacement);
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
    let result = root.remove_any_if_exists(Utf8Path::new("target"));
    assert!(fault.triggered());
    assert!(matches!(result, Err(WorkspaceError::InvalidPath { path }) if path == "target"));
    assert!(std::fs::symlink_metadata(&target).is_ok());
    assert_eq!(std::fs::read(path.join("held")).unwrap(), b"original");
}

#[cfg(unix)]
#[test]
fn cleanup_reports_an_entry_disappearing_after_a_permission_denied_open() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target");
    std::fs::write(&target, b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault =
        crate::workspace::owned::io_faults::arm_action("worker.cleanup.open_file", 0, move || {
            std::fs::rename(&target, target.with_file_name("held")).unwrap();
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
    let result = root.remove_any_if_exists(Utf8Path::new("target"));
    assert!(fault.triggered());
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "inspect worker file",
            ..
        })
    ));
    assert_eq!(std::fs::read(path.join("held")).unwrap(), b"original");
}

#[cfg(unix)]
#[test]
fn cleanup_can_unlink_the_unchanged_file_after_a_permission_denied_open() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault =
        crate::workspace::owned::io_faults::arm_action("worker.cleanup.open_file", 0, || {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
    root.remove_any_if_exists(Utf8Path::new("target")).unwrap();
    assert!(fault.triggered());
    assert!(!path.join("target").exists());
}

fn restore_test_stream(
    root: &WorkerRoot,
    mut reader: &mut dyn Read,
    permissions: std::fs::Permissions,
) -> Result<(), WorkspaceError> {
    root.restore_from(Utf8Path::new("target"), &mut reader, permissions)
}

#[rstest::rstest]
#[case::write("worker.restore.write", "write worker file", b"".as_slice())]
#[case::permissions("worker.restore.permissions", "restore worker permissions", b"replacement".as_slice())]
fn restore_reports_output_failures_with_the_target_path(
    #[case] point: &'static str,
    #[case] operation: &str,
    #[case] expected: &[u8],
) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let permissions = std::fs::metadata(path.join("target"))
        .unwrap()
        .permissions();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm(point);
    let result = restore_test_stream(&root, &mut io::Cursor::new(b"replacement"), permissions);
    assert!(fault.triggered(), "restore output boundary was not checked");
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: actual, path, message })
        if actual == operation && path == "target" && message.contains(point))
    );
    assert_eq!(std::fs::read(path.join("target")).unwrap(), expected);
}

#[test]
fn restore_test_stream_restores_the_full_payload_and_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let permissions = std::fs::metadata(path.join("target"))
        .unwrap()
        .permissions();
    let expected = crate::workspace::permission_fingerprint(&permissions);
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    restore_test_stream(&root, &mut io::Cursor::new(b"replacement"), permissions).unwrap();
    assert_eq!(std::fs::read(path.join("target")).unwrap(), b"replacement");
    assert_eq!(
        crate::workspace::permission_fingerprint(
            &std::fs::metadata(path.join("target"))
                .unwrap()
                .permissions()
        ),
        expected
    );
}

#[test]
fn restore_reports_snapshot_reader_failures_with_the_target_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let permissions = std::fs::metadata(path.join("target"))
        .unwrap()
        .permissions();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let mut reader = File::options()
        .write(true)
        .open(path.join("target"))
        .unwrap();
    let result = restore_test_stream(&root, &mut reader, permissions);
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: "read shared snapshot", path, .. }) if path == "target")
    );
    assert_eq!(std::fs::read(path.join("target")).unwrap(), b"");
}

#[test]
fn mutation_buffer_capacity_overflow_preserves_existing_contents() {
    let mut contents = b"existing".to_vec();
    // Vec<u8> rejects this capacity before calling the allocator: no large allocation is attempted.
    let error = MutationFile::reserve_contents(&mut contents, usize::MAX).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(contents, b"existing");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::inspect("worker.removal.inspect_repaired", "inspect worker directory")]
#[case::reopen("worker.removal.open_repaired", "open worker directory")]
fn repaired_directory_io_failures_preserve_the_unvisited_contents(
    #[case] point: &'static str,
    #[case] operation: &str,
) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("protected"), b"original").unwrap();
    let retained = File::open(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm(point);
    let result = root.remove_any_if_exists(Utf8Path::new("target"));
    retained
        .set_permissions(std::fs::Permissions::from_mode(0o700))
        .unwrap();
    assert!(
        fault.triggered(),
        "repaired directory boundary was not checked"
    );
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: actual, message, .. })
        if actual == operation && message.contains(point))
    );
    assert_eq!(
        std::fs::read(target.join("protected")).unwrap(),
        b"original"
    );
    root.remove_any_if_exists(Utf8Path::new("target")).unwrap();
    assert!(!target.exists());
}

#[cfg(unix)]
#[test]
fn native_parent_link_errors_are_classified_as_invalid_paths() {
    let path = Utf8Path::new("logical/target");
    let error = WorkerRoot::map_parent_error(path, io::Error::from_raw_os_error(libc::ELOOP));
    assert!(matches!(error, WorkspaceError::InvalidPath { path: actual } if actual == path));
}

#[cfg(unix)]
#[test]
fn restore_preserves_the_previous_file_when_removal_cannot_open_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let permissions = std::fs::metadata(path.join("target"))
        .unwrap()
        .permissions();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm("worker.cleanup.open_file");
    let result = restore_test_stream(&root, &mut io::Cursor::new(b"replacement"), permissions);
    assert!(fault.triggered());
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: "open worker file", path, .. }) if path == "target")
    );
    assert_eq!(std::fs::read(path.join("target")).unwrap(), b"original");
}

#[cfg(unix)]
#[test]
fn failed_symlink_unlink_preserves_the_link_and_the_outside_target() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("protected"), b"outside sentinel").unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::os::unix::fs::symlink(outside.path(), path.join("link")).unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let fault = crate::workspace::owned::io_faults::arm("worker.link.remove");
    let result = root.remove_any_if_exists(Utf8Path::new("link"));
    assert!(fault.triggered(), "symlink unlink boundary was not checked");
    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: "remove worker link", message, .. }) if message.contains("worker.link.remove"))
    );
    assert!(
        std::fs::symlink_metadata(path.join("link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read(outside.path().join("protected")).unwrap(),
        b"outside sentinel"
    );
    root.remove_any_if_exists(Utf8Path::new("link")).unwrap();
    assert!(std::fs::symlink_metadata(path.join("link")).is_err());
}

#[rstest::rstest]
#[case::one_parent("nested/target")]
#[case::two_parents("nested/deeper/target")]
fn native_parent_open_retains_the_requested_existing_ancestor(#[case] relative: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    let target = path.join(relative);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"original").unwrap();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let (parent, name) = root.open_native_parent(Path::new(relative)).unwrap();
    assert_eq!(name, "target");
    let mut options = cap_primitives::fs::OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = cap_primitives::fs::open(&parent, Path::new(&name), &options).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
}

#[rstest::rstest]
#[case::empty("")]
#[case::traversal("../outside")]
fn restore_rejects_invalid_paths_before_consuming_the_snapshot(#[case] invalid: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(path.join("target"), b"original").unwrap();
    let permissions = std::fs::metadata(path.join("target"))
        .unwrap()
        .permissions();
    let root = WorkerRoot::open(path.to_owned()).unwrap();
    let mut input = io::Cursor::new(b"replacement");
    let mut reader: &mut dyn Read = &mut input;
    let result = root.restore_from(Utf8Path::new(invalid), &mut reader, permissions);
    assert!(matches!(result, Err(WorkspaceError::InvalidPath { path }) if path == invalid));
    assert_eq!(input.position(), 0);
    assert_eq!(std::fs::read(path.join("target")).unwrap(), b"original");
}
