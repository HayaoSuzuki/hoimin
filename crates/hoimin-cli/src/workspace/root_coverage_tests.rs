use super::*;

#[test]
fn native_parent_open_rejects_empty_absolute_and_traversing_paths() {
    let directory = tempfile::tempdir().unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    for path in [Path::new(""), Path::new("../outside"), directory.path()] {
        assert!(matches!(
            root.open_native_parent(path),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }
}

#[cfg(unix)]
#[test]
fn native_parent_open_rejects_symlink_ancestors_and_missing_directories() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join("link")).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    assert!(matches!(
        root.open_native_parent(Path::new("link/target")),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(matches!(
        root.open_native_parent(Path::new("missing/target")),
        Err(WorkspaceError::Io { .. })
    ));
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
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
    std::fs::remove_file(directory.path().join("target")).unwrap();
    assert!(matches!(
        WorkerRoot::remove_non_directory(root.handle(), &name, Utf8Path::new("target"), &metadata),
        Err(WorkspaceError::Io {
            operation: "open worker file",
            ..
        })
    ));
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
fn mutation_open_reports_unreadable_files_and_directory_targets() {
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
fn removal_directory_open_rejects_stale_identity_and_non_directory_replacement() {
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
    std::fs::remove_dir(&path).unwrap();
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
fn root_access_checks_reject_links_and_revoked_search_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let file_path = directory.path().join("target");
    std::fs::write(&file_path, b"preserved").unwrap();
    std::os::unix::fs::symlink("target", directory.path().join("link")).unwrap();
    let root = WorkerRoot::open(Utf8Path::from_path(directory.path()).unwrap().to_owned()).unwrap();
    assert!(matches!(
        root.try_exists(Utf8Path::new("link")),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o400)).unwrap();
    let exists = root.try_exists(Utf8Path::new("target"));
    let entries = root.entries();
    let removal = root.remove_any_if_exists(Utf8Path::new("target"));
    let permissions = super::super::permission_fingerprint(&std::fs::Permissions::from_mode(0o600));
    let snapshot = root.snapshot_matches_reader(
        Utf8Path::new("target"),
        &mut io::Cursor::new(b"preserved"),
        9,
        permissions,
    );
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        exists,
        Err(WorkspaceError::Io {
            operation: "inspect worker file",
            ..
        })
    ));
    assert!(matches!(entries, Err(WorkspaceError::Io { .. })));
    assert!(matches!(
        removal,
        Err(WorkspaceError::Io {
            operation: "inspect worker entry",
            ..
        })
    ));
    assert!(matches!(
        snapshot,
        Err(WorkspaceError::Io {
            operation: "verify restored file",
            ..
        })
    ));
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
    let permissions = super::super::permission_fingerprint(
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
