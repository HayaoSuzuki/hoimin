#[cfg(test)]
mod tests {
    use camino::Utf8Path;
    #[cfg(unix)]
    use camino::Utf8PathBuf;

    use super::{
        COORDINATOR_FILE, MANAGED_DIR, MAX_MANAGED_CHILDREN, ManagedRootCoordinator,
        ManagedRunRoot, OwnerKind, ensure_direct_child_capacity,
    };

    fn outer_depth_guard_active() -> bool {
        std::env::var_os("HOIMIN_FOCUSED_MUTATION_OUTER_DEPTH_GUARD").is_some()
    }

    #[test]
    fn direct_child_limit_rejects_the_first_child_beyond_the_cap() {
        assert!(ensure_direct_child_capacity(MAX_MANAGED_CHILDREN - 1).is_ok());

        let error = ensure_direct_child_capacity(MAX_MANAGED_CHILDREN).unwrap_err();
        assert!(matches!(
            error,
            super::WorkspaceError::OwnedWorkspaceLimit { planned, limit }
                if planned == 100_001 && limit == 100_000
        ));
    }

    #[test]
    fn platform_lock_contention_is_classified_as_retryable() {
        assert!(super::lock_error_is_busy(&fs2::lock_contended_error()));
        assert!(!super::lock_error_is_busy(&std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "hard lock failure",
        )));
    }

    #[test]
    fn coordinator_registry_lock_honors_the_bootstrap_deadline() {
        let registry = super::LocalCoordinatorRegistry::default();
        let _held = registry.lock().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(10);

        let error =
            super::lock_registry_until(&registry, Utf8Path::new("managed"), deadline).unwrap_err();

        assert!(error.to_string().contains("deadline"));
    }

    #[test]
    fn managed_root_creation_waits_for_a_short_coordinator_holder() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let holder = ManagedRootCoordinator::open(parent).unwrap();
        let creator = ManagedRootCoordinator::open(parent).unwrap();
        let held = super::CoordinatorLockGuard::acquire(&holder).unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            entered_tx.send(()).unwrap();
            ManagedRunRoot::create(&creator, OwnerKind::PublicExecution)
        });
        entered_rx.recv().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(25));

        assert!(!thread.is_finished(), "creator failed instead of waiting");
        drop(held);
        let root = thread.join().unwrap().unwrap();
        assert!(root.path().exists());
    }

    #[test]
    fn deferred_cleanup_keeps_the_exclusive_lease_guard() {
        use fs2::FileExt;

        let temp = tempfile::tempdir().unwrap();
        let lease_path = temp.path().join("lease");
        let lease = std::fs::File::create(&lease_path).unwrap();
        FileExt::try_lock_exclusive(&lease).unwrap();
        let pending = super::PendingCleanup {
            deleting_name: ".deleting-test".to_owned(),
            expected_identity: (1, 2),
            _lease_guard: lease,
        };
        let contender = std::fs::File::open(&lease_path).unwrap();

        assert!(FileExt::try_lock_exclusive(&contender).is_err());
        drop(pending);
        FileExt::try_lock_exclusive(&contender).unwrap();
        FileExt::unlock(&contender).unwrap();
    }

    #[test]
    fn diagnostic_details_have_count_and_utf8_byte_caps() {
        let mut report = super::ReclaimReport::default();
        for _ in 0..=super::MAX_DIAGNOSTIC_DETAILS {
            super::push_reclaim_detail(&mut report, "é".repeat(3_000));
        }

        assert_eq!(report.details.len(), super::MAX_DIAGNOSTIC_DETAILS);
        assert!(
            report
                .details
                .iter()
                .all(|detail| detail.len() <= super::MAX_DIAGNOSTIC_DETAIL_BYTES)
        );
        assert_eq!(report.truncated_detail_count, 256);
        assert_eq!(report.omitted_detail_count, 1);
    }

    #[test]
    fn published_root_has_a_canonical_name_and_drop_is_non_destructive() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let run_name = root.path().file_name().unwrap();
        let run_id = run_name.strip_prefix("run-").unwrap();
        uuid::Uuid::parse_str(run_id).unwrap();
        let child = root.create_child("worker-").unwrap();
        std::fs::write(child.path().join("retained.txt"), b"retained").unwrap();
        let published = root.path().to_owned();
        let child_path = child.path().to_owned();

        drop(child);
        drop(root);

        assert!(published.exists());
        assert_eq!(
            std::fs::read(child_path.join("retained.txt")).unwrap(),
            b"retained"
        );
    }

    #[test]
    fn cleanup_ready_retry_replaces_a_partial_marker_from_an_interrupted_write() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.dir.write(super::CLEANUP_READY_FILE, b"{").unwrap();

        root.mark_cleanup_ready().unwrap();

        let marker = super::read_cleanup_ready_marker(&root.dir).expect("complete marker");
        assert_eq!(marker.run_id, root.run_id);
        assert_eq!(marker.owner, root.owner);
    }

    #[test]
    fn every_root_constructor_failure_rolls_back_or_is_immediately_reclaimable() {
        for selected in [
            super::PublishBoundary::StagingCreated,
            super::PublishBoundary::StagingOpened,
            super::PublishBoundary::RenamedActive,
            super::PublishBoundary::ActiveOpened,
            super::PublishBoundary::CoordinatorOpened,
            super::PublishBoundary::CoordinatorCloned,
        ] {
            let parent = tempfile::tempdir().unwrap();
            let parent = Utf8Path::from_path(parent.path()).unwrap();
            let coordinator = ManagedRootCoordinator::open(parent).unwrap();

            let result = ManagedRunRoot::create_with_publish_hook(
                &coordinator,
                OwnerKind::PublicExecution,
                &|observed| {
                    if observed == selected {
                        Err(super::WorkspaceError::io(
                            "inject publish failure",
                            Utf8Path::new("managed-root"),
                            format!("injected at {selected:?}"),
                        ))
                    } else {
                        Ok(())
                    }
                },
            );

            assert!(result.is_err(), "boundary {selected:?} did not fail");
            let _ = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
            let residual = std::fs::read_dir(parent.join(super::MANAGED_DIR))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .filter(|name| name != super::COORDINATOR_FILE)
                .collect::<Vec<_>>();
            assert!(
                residual.is_empty(),
                "boundary {selected:?} left managed roots: {residual:?}"
            );
        }
    }

    #[test]
    fn every_child_constructor_failure_removes_the_new_directory() {
        for selected in [
            super::ChildCreationBoundary::Created,
            super::ChildCreationBoundary::Opened,
        ] {
            let parent = tempfile::tempdir().unwrap();
            let parent = Utf8Path::from_path(parent.path()).unwrap();
            let coordinator = ManagedRootCoordinator::open(parent).unwrap();
            let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();

            let result = root.create_child_with_hook("worker-", &|observed| {
                if observed == selected {
                    Err(super::WorkspaceError::io(
                        "inject child creation failure",
                        Utf8Path::new("managed-child"),
                        format!("injected at {selected:?}"),
                    ))
                } else {
                    Ok(())
                }
            });

            assert!(result.is_err(), "boundary {selected:?} did not fail");
            let residual = std::fs::read_dir(root.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .filter(|name| name.to_string_lossy().starts_with("worker-"))
                .collect::<Vec<_>>();
            assert!(
                residual.is_empty(),
                "boundary {selected:?} left managed children: {residual:?}"
            );
        }
    }

    #[test]
    fn active_locked_lease_is_preserved_even_past_the_stale_threshold() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 1);
        assert!(published.exists());
        drop(root);
    }

    #[test]
    fn unlocked_lease_with_a_heartbeat_at_least_twenty_four_hours_old_is_reclaimed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        drop(root);
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 1);
        assert_eq!(report.preserved_roots, 0);
        assert!(!published.exists());
    }

    #[test]
    fn coordinator_uses_the_fixed_two_slot_crc_layout() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let _coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let bytes = std::fs::read(parent.join(MANAGED_DIR).join(COORDINATOR_FILE)).unwrap();

        assert_eq!(bytes.len(), 1_025);
        assert_eq!(bytes[0], 0);
        for start in [1, 513] {
            let slot = &bytes[start..start + 512];
            assert_eq!(&slot[0..8], b"HMCUR001");
            assert_eq!(u32::from_le_bytes(slot[8..12].try_into().unwrap()), 1);
            assert_eq!(u64::from_le_bytes(slot[12..20].try_into().unwrap()), 0);
            assert_eq!(u16::from_le_bytes(slot[20..22].try_into().unwrap()), 0);
            assert!(slot[22..508].iter().all(|byte| *byte == 0));
            assert_eq!(
                u32::from_le_bytes(slot[508..512].try_into().unwrap()),
                crc32fast::hash(&slot[..508])
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn managed_root_and_coordinator_are_user_only() {
        use std::os::unix::fs::PermissionsExt;

        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();

        assert_eq!(
            std::fs::metadata(&coordinator.path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(coordinator.path.join(COORDINATOR_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_managed_leaf_is_rejected_without_touching_its_target() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let parent = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let outside_path = Utf8Path::from_path(outside.path()).unwrap();
        let before = std::fs::metadata(outside_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        symlink(outside_path, parent.join(MANAGED_DIR)).unwrap();

        assert!(ManagedRootCoordinator::open(parent).is_err());
        assert_eq!(
            std::fs::metadata(outside_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            before
        );
        assert!(!outside_path.join(COORDINATOR_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn managed_paths_use_the_canonical_parent_not_a_symlink_alias() {
        use std::os::unix::fs::symlink;

        let real_parent = tempfile::tempdir().unwrap();
        let alias_parent = tempfile::tempdir().unwrap();
        let real_parent = Utf8Path::from_path(real_parent.path()).unwrap();
        let alias = Utf8Path::from_path(alias_parent.path())
            .unwrap()
            .join("alias");
        symlink(real_parent, &alias).unwrap();

        let coordinator = ManagedRootCoordinator::open(&alias).unwrap();
        let canonical =
            Utf8PathBuf::from_path_buf(std::fs::canonicalize(real_parent).unwrap()).unwrap();

        assert_eq!(coordinator.path, canonical.join(MANAGED_DIR));
        assert!(!coordinator.path.starts_with(&alias));
    }

    #[test]
    fn concurrent_bootstrap_never_observes_a_partial_coordinator() {
        let parent = tempfile::tempdir().unwrap();
        let path = Utf8Path::from_path(parent.path()).unwrap().to_owned();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let handles = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    ManagedRootCoordinator::open(&path)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();

        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert_eq!(
            std::fs::metadata(path.join(MANAGED_DIR).join(COORDINATOR_FILE))
                .unwrap()
                .len(),
            1_025
        );
    }

    #[test]
    fn bootstrap_waits_for_a_visible_zero_length_coordinator_to_finish() {
        use std::io::Write;

        let parent = tempfile::tempdir().unwrap();
        let managed = parent.path().join(MANAGED_DIR);
        std::fs::create_dir(&managed).unwrap();
        let coordinator_path = managed.join(COORDINATOR_FILE);
        std::fs::File::create(&coordinator_path).unwrap();
        let writer_path = coordinator_path.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(25));
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(writer_path)
                .unwrap();
            fs2::FileExt::lock_exclusive(&file).unwrap();
            file.write_all(&super::initial_coordinator_bytes()).unwrap();
            file.sync_all().unwrap();
            fs2::FileExt::unlock(&file).unwrap();
        });

        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        writer.join().unwrap();

        assert_eq!(coordinator.file.metadata().unwrap().len(), 1_025);
    }

    #[test]
    fn fresh_process_bootstrap_child() {
        let Some(parent) = std::env::var_os("HOIMIN_BOOTSTRAP_TEST_PARENT") else {
            return;
        };
        let parent = camino::Utf8PathBuf::from_path_buf(parent.into()).unwrap();
        if std::env::var_os("HOIMIN_BOOTSTRAP_TEST_BARRIER").is_some() {
            let ready = parent.join(format!("ready-{}", std::process::id()));
            std::fs::write(ready, b"ready").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !parent.join("go").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "bootstrap barrier timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        ManagedRootCoordinator::open(&parent).unwrap();
    }

    #[test]
    fn two_fresh_processes_bootstrap_one_complete_coordinator() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let executable = std::env::current_exe().unwrap();
        let spawn = || {
            std::process::Command::new(&executable)
                .arg("--exact")
                .arg("workspace::owned::tests::fresh_process_bootstrap_child")
                .env("HOIMIN_BOOTSTRAP_TEST_PARENT", parent)
                .env("HOIMIN_BOOTSTRAP_TEST_BARRIER", "1")
                .spawn()
                .unwrap()
        };
        let mut first = spawn();
        let mut second = spawn();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let ready = std::fs::read_dir(parent)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("ready-"))
                .count();
            if ready == 2 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "child readiness timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::fs::write(parent.join("go"), b"go").unwrap();

        assert!(first.wait().unwrap().success());
        assert!(second.wait().unwrap().success());
        assert_eq!(
            std::fs::metadata(parent.join(MANAGED_DIR).join(COORDINATOR_FILE))
                .unwrap()
                .len(),
            1_025
        );
    }

    #[test]
    fn malformed_existing_coordinator_fails_closed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let path = coordinator.path.join(COORDINATOR_FILE);
        drop(coordinator);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[32] = 1;
        bytes[544] = 1;
        std::fs::write(&path, bytes).unwrap();

        assert!(ManagedRootCoordinator::open(parent).is_err());
    }

    #[test]
    fn one_corrupt_coordinator_slot_preserves_the_other_slot() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let path = coordinator.path.join(COORDINATOR_FILE);
        drop(coordinator);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[32] = 1;
        std::fs::write(&path, bytes).unwrap();

        ManagedRootCoordinator::open(parent).unwrap();
    }

    #[test]
    fn truncated_existing_coordinator_fails_closed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let path = coordinator.path.join(COORDINATOR_FILE);
        drop(coordinator);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(513)
            .unwrap();

        assert!(ManagedRootCoordinator::open(parent).is_err());
    }

    #[test]
    fn owner_cleanup_removes_only_its_published_root() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let first = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let second = ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery).unwrap();
        let first_path = first.path().to_owned();
        let second_path = second.path().to_owned();
        std::fs::write(
            first.create_child("worker-").unwrap().path().join("x"),
            b"x",
        )
        .unwrap();

        let record = first.cleanup(std::time::Duration::from_secs(1));

        assert_eq!(
            record.status,
            hoimin_core::DiskCleanupStatus::Clean,
            "{record:?}"
        );
        assert!(!first_path.exists());
        assert!(second_path.exists());
    }

    #[test]
    fn owner_cleanup_defers_until_all_managed_child_handles_are_closed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = root.create_child("worker-").unwrap();

        let deferred = root.cleanup(std::time::Duration::from_secs(1));

        assert_eq!(deferred.status, hoimin_core::DiskCleanupStatus::Deferred);
        assert!(root.path().exists());
        assert!(root.create_child("worker-").is_err());
        drop(child);

        let completed = root.cleanup(std::time::Duration::from_secs(5));
        assert_eq!(completed.status, hoimin_core::DiskCleanupStatus::Clean);
        assert!(!root.path().exists());
    }

    #[test]
    fn busy_coordinator_defers_owner_cleanup_and_does_not_block_startup_reclaim() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let held = super::CoordinatorLockGuard::acquire(&coordinator).unwrap();
        let started = std::time::Instant::now();

        let cleanup = root.cleanup(std::time::Duration::from_secs(60));
        let reclaim = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(cleanup.status, hoimin_core::DiskCleanupStatus::Deferred);
        assert_eq!(reclaim.reclaimed_roots, 0);
        assert!(!reclaim.details.is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        drop(held);
        assert_eq!(
            root.cleanup(std::time::Duration::from_secs(5)).status,
            hoimin_core::DiskCleanupStatus::Clean
        );
    }

    #[test]
    fn cleanup_removes_a_tree_deeper_than_the_meter_limit() {
        if outer_depth_guard_active() {
            return;
        }
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = root.create_child("worker-").unwrap();
        let mut nested = child.path().to_owned();
        for _ in 0..129 {
            nested.push("d");
            std::fs::create_dir(&nested).unwrap();
        }
        std::fs::write(nested.join("payload"), b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&nested, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let published = root.path().to_owned();
        drop(child);

        let record = root.cleanup(std::time::Duration::from_secs(10));

        assert_eq!(
            record.status,
            hoimin_core::DiskCleanupStatus::Clean,
            "{record:?}"
        );
        assert!(record.examined_entries >= 130);
        assert!(!published.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_step_honors_an_exhausted_entry_allowance_without_deleting() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = root.create_child("worker-").unwrap();
        std::fs::write(child.path().join("payload"), b"retained").unwrap();
        let name = child.path().file_name().unwrap();
        let identity = super::directory_identity(&child.dir).unwrap();

        let error = super::remove_one_claimed_entry(
            &root.dir,
            name,
            Some(identity),
            0,
            std::time::Instant::now(),
            std::time::Duration::from_secs(1),
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(
            std::fs::read(child.path().join("payload")).unwrap(),
            b"retained"
        );

        let deadline_error = super::remove_one_claimed_entry(
            &root.dir,
            name,
            Some(identity),
            1,
            std::time::Instant::now(),
            std::time::Duration::ZERO,
        )
        .unwrap_err();
        assert_eq!(deadline_error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(
            std::fs::read(child.path().join("payload")).unwrap(),
            b"retained"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_cursor_depth_and_byte_limits_are_exact() {
        assert_eq!(
            super::advance_cleanup_cursor(super::MAX_CLEANUP_DEPTH, 65_534, 1).unwrap(),
            65_536
        );
        assert!(super::advance_cleanup_cursor(super::MAX_CLEANUP_DEPTH + 1, 0, 1).is_err());
        assert!(super::advance_cleanup_cursor(1, 65_535, 1).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn partial_cleanup_preserves_lease_markers_and_owner_resume_completes() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        std::fs::write(root.path().join("first"), b"1").unwrap();
        std::fs::write(root.path().join("second"), b"2").unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        assert_eq!(
            super::claim_managed_child(
                &coordinator.dir,
                &active,
                &deleting,
                &root.run_id,
                root.owner,
                &root.dir,
                &root.lease,
            )
            .unwrap(),
            super::ClaimResult::Claimed
        );

        let slice = super::remove_one_claimed_entry(
            &coordinator.dir,
            &deleting,
            Some(super::directory_identity(&root.dir).unwrap()),
            100,
            std::time::Instant::now(),
            std::time::Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(slice.removed, 1);
        assert!(root.dir.symlink_metadata(super::LEASE_FILE).is_ok());
        let resumed = root.cleanup(std::time::Duration::from_secs(5));
        assert_eq!(
            resumed.status,
            hoimin_core::DiskCleanupStatus::Clean,
            "{resumed:?}"
        );
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn janitor_resumes_a_partially_removed_deleting_root() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.mark_cleanup_ready().unwrap();
        std::fs::write(root.path().join("first"), b"1").unwrap();
        std::fs::write(root.path().join("second"), b"2").unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        super::claim_managed_child(
            &coordinator.dir,
            &active,
            &deleting,
            &root.run_id,
            root.owner,
            &root.dir,
            &root.lease,
        )
        .unwrap();
        super::remove_one_claimed_entry(
            &coordinator.dir,
            &deleting,
            Some(super::directory_identity(&root.dir).unwrap()),
            100,
            std::time::Instant::now(),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        assert!(root.dir.symlink_metadata(super::LEASE_FILE).is_ok());
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[test]
    fn young_deleting_root_without_cleanup_ready_is_preserved() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        assert_eq!(
            super::claim_managed_child(
                &coordinator.dir,
                &active,
                &deleting,
                &root.run_id,
                root.owner,
                &root.dir,
                &root.lease,
            )
            .unwrap(),
            super::ClaimResult::Claimed
        );
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 0, "{report:?}");
        assert_eq!(report.preserved_roots, 1, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&deleting).is_ok());
    }

    #[test]
    fn lease_only_deleting_root_from_final_marker_cleanup_is_resumed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        assert_eq!(
            super::claim_managed_child(
                &coordinator.dir,
                &active,
                &deleting,
                &root.run_id,
                root.owner,
                &root.dir,
                &root.lease,
            )
            .unwrap(),
            super::ClaimResult::Claimed
        );
        root.dir.remove_file(super::HEARTBEAT_FILE).unwrap();
        assert!(super::deleting_has_only_lease(&root.dir));
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert_eq!(report.preserved_roots, 0, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[test]
    fn ready_deleting_root_after_heartbeat_cleanup_is_resumed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.mark_cleanup_ready().unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        assert_eq!(
            super::claim_managed_child(
                &coordinator.dir,
                &active,
                &deleting,
                &root.run_id,
                root.owner,
                &root.dir,
                &root.lease,
            )
            .unwrap(),
            super::ClaimResult::Claimed
        );
        root.dir.remove_file(super::HEARTBEAT_FILE).unwrap();
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert_eq!(report.preserved_roots, 0, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[test]
    fn empty_deleting_root_after_final_marker_unlink_is_resumed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let deleting = format!("{}{}", super::DELETING_PREFIX, uuid::Uuid::new_v4());
        coordinator.dir.create_dir(&deleting).unwrap();

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[test]
    fn young_unlocked_lease_is_preserved() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 1);
        assert!(published.exists());
    }

    #[test]
    fn malformed_heartbeat_is_preserved_even_after_the_stale_threshold() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        std::fs::write(published.join(super::HEARTBEAT_FILE), b"not json\n").unwrap();
        drop(root);
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 1);
        assert!(published.exists());
    }

    #[test]
    fn claim_rejects_a_replacement_with_copied_marker_values() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let original = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let replacement = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let original_name = original.path().file_name().unwrap().to_owned();
        let parked_name = format!("{}{}", super::STAGING_PREFIX, original.run_id);
        let deleting_name = format!("{}{}", super::DELETING_PREFIX, original.run_id);
        let replacement_name = replacement.path().file_name().unwrap().to_owned();
        coordinator
            .dir
            .rename(&original_name, &coordinator.dir, &parked_name)
            .unwrap();
        coordinator
            .dir
            .rename(&replacement_name, &coordinator.dir, &original_name)
            .unwrap();
        let copied_marker = super::LeaseMarker {
            schema: super::LEASE_SCHEMA,
            run_id: original.run_id.clone(),
            created_unix_seconds: 0,
            owner: original.owner,
        };
        let mut marker = coordinator
            .dir
            .open_dir(&original_name)
            .unwrap()
            .create(super::LEASE_FILE)
            .unwrap();
        marker.set_len(0).unwrap();
        serde_json::to_writer(&mut marker, &copied_marker).unwrap();
        marker.sync_all().unwrap();
        let claim = super::claim_managed_child(
            &coordinator.dir,
            &original_name,
            &deleting_name,
            &original.run_id,
            original.owner,
            &original.dir,
            &original.lease,
        );

        assert!(claim.is_err());
        assert!(coordinator.dir.symlink_metadata(&original_name).is_ok());
        assert!(coordinator.dir.symlink_metadata(&deleting_name).is_err());
    }

    #[test]
    fn managed_child_cleanup_rejects_a_same_name_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let original = root.create_child("worker-").unwrap();
        let replacement = root.create_child("worker-").unwrap();
        let original_name = original.path().file_name().unwrap().to_owned();
        let replacement_name = replacement.path().file_name().unwrap().to_owned();
        let parked_name = format!("parked-{}", uuid::Uuid::new_v4());
        root.dir
            .rename(&original_name, &root.dir, &parked_name)
            .unwrap();
        root.dir
            .rename(&replacement_name, &root.dir, &original_name)
            .unwrap();

        assert!(original.cleanup().is_err());
        assert!(root.dir.symlink_metadata(&original_name).is_ok());
        assert!(root.dir.symlink_metadata(&parked_name).is_ok());
    }

    #[test]
    fn cleanup_ready_unlocked_lease_is_reclaimed_without_waiting_a_day() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        root.mark_cleanup_ready().unwrap();
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1);
        assert!(!published.exists());
    }

    #[test]
    fn live_managed_child_keeps_the_root_lease_after_the_owner_is_dropped() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = root.create_child("worker-").unwrap();
        let published = root.path().to_owned();
        root.mark_cleanup_ready().unwrap();
        drop(root);

        let live = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(live.reclaimed_roots, 0);
        assert!(published.exists());

        drop(child);
        let settled = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
        assert_eq!(settled.reclaimed_roots, 1);
        assert!(!published.exists());
    }

    #[test]
    fn cleanup_ready_for_a_different_lease_identity_is_rejected() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        root.mark_cleanup_ready().unwrap();
        let ready = published.join(super::CLEANUP_READY_FILE);
        let mut marker: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&ready).unwrap()).unwrap();
        marker["lease_inode"] = serde_json::json!(u64::MAX);
        std::fs::write(&ready, serde_json::to_vec(&marker).unwrap()).unwrap();
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 1);
        assert!(published.exists());
    }

    #[test]
    fn retained_root_is_not_reclaimed_when_stale() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        root.retain().unwrap();
        drop(root);
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 1);
        assert!(published.exists());
    }

    #[test]
    fn janitor_cursor_reaches_the_two_hundred_fifty_seventh_root() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        for _ in 0..257 {
            let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
            root.mark_cleanup_ready().unwrap();
        }

        let first = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
        let second = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(first.reclaimed_roots, 256);
        assert_eq!(second.reclaimed_roots, 1);
    }

    #[test]
    fn stale_staging_root_is_reclaimed_only_with_expected_marker_contents() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let run_id = root.run_id.clone();
        let active = format!("{}{}", super::ACTIVE_PREFIX, run_id);
        let staging = format!("{}{}", super::STAGING_PREFIX, run_id);
        drop(root);
        coordinator
            .dir
            .rename(&active, &coordinator.dir, &staging)
            .unwrap();
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 1);
        assert!(coordinator.dir.symlink_metadata(&staging).is_err());
    }

    #[test]
    fn empty_staging_root_from_a_pre_lease_crash_is_reclaimed_after_a_day() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let staging = format!("{}{}", super::STAGING_PREFIX, uuid::Uuid::new_v4());
        coordinator.dir.create_dir(&staging).unwrap();
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&staging).is_err());
    }

    #[test]
    fn lease_only_staging_root_is_reclaimed_after_a_day() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let staging = format!("{}{}", super::STAGING_PREFIX, root.run_id);
        root.dir.remove_file(super::HEARTBEAT_FILE).unwrap();
        coordinator
            .dir
            .rename(&active, &coordinator.dir, &staging)
            .unwrap();
        drop(root);
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&staging).is_err());
    }
}
#[cfg(windows)]
#[path = "owned/windows.rs"]
mod windows;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use super::WorkspaceError;
use hoimin_core::DiskCleanupStatus;

#[cfg(unix)]
struct OwnedDirectoryEntries(rustix::fs::Dir);

#[cfg(windows)]
struct OwnedDirectoryEntries(super::root::windows::DirectoryEntries);

impl Iterator for OwnedDirectoryEntries {
    type Item = std::io::Result<OsString>;

    fn next(&mut self) -> Option<Self::Item> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;

            loop {
                let entry = self.0.next()?;
                match entry {
                    Ok(entry) => {
                        let name = entry.file_name().to_bytes();
                        if name != b"." && name != b".." {
                            return Some(Ok(OsString::from_vec(name.to_vec())));
                        }
                    }
                    Err(error) => return Some(Err(std::io::Error::from(error))),
                }
            }
        }
        #[cfg(windows)]
        {
            self.0
                .next()
                .map(|entry| entry.map(super::root::windows::DirectoryEntryInfo::into_name))
        }
    }
}

fn owned_directory_entries(dir: &cap_std::fs::Dir) -> std::io::Result<OwnedDirectoryEntries> {
    #[cfg(unix)]
    {
        rustix::fs::Dir::read_from(dir)
            .map(OwnedDirectoryEntries)
            .map_err(std::io::Error::from)
    }
    #[cfg(windows)]
    {
        let directory = dir.try_clone()?.into_std_file();
        super::root::windows::DirectoryEntries::open(directory).map(OwnedDirectoryEntries)
    }
}

const MANAGED_DIR: &str = "hoimin-workspaces-v1";
const STAGING_PREFIX: &str = ".staging-";
const ACTIVE_PREFIX: &str = "run-";
const DELETING_PREFIX: &str = ".deleting-";
const LEASE_FILE: &str = ".hoimin-lease.json";
const RETAIN_FILE: &str = ".hoimin-retain.json";
const HEARTBEAT_FILE: &str = ".hoimin-heartbeat.json";
const CLEANUP_READY_FILE: &str = ".hoimin-cleanup-ready.json";
const COORDINATOR_FILE: &str = ".hoimin-coordinator";
const COORDINATOR_BYTES: u64 = 1_025;
const COORDINATOR_LAYOUT_BYTES: usize = 1_025;
const COORDINATOR_SLOT_BYTES: usize = 512;
const COORDINATOR_CURSOR_BYTES: usize = 480;
type LocalCoordinatorLock = Arc<Mutex<()>>;
type LocalCoordinatorRegistry = Mutex<HashMap<(u64, u64), Weak<Mutex<()>>>>;
type LocalCoordinatorRegistryGuard<'a> =
    std::sync::MutexGuard<'a, HashMap<(u64, u64), Weak<Mutex<()>>>>;
static COORDINATOR_LOCAL_LOCKS: OnceLock<LocalCoordinatorRegistry> = OnceLock::new();
const COORDINATOR_MAGIC: &[u8; 8] = b"HMCUR001";
const COORDINATOR_SCHEMA: u32 = 1;
const LEASE_SCHEMA: u32 = 1;
const MAX_MARKER_BYTES: u64 = 64 * 1024;
const MAX_MANAGED_CHILDREN: usize = 100_000;
const MAX_RECLAIM_CANDIDATES: usize = 256;
const MAX_DIAGNOSTIC_DETAILS: usize = 256;
const MAX_DIAGNOSTIC_DETAIL_BYTES: usize = 4 * 1024;
const JANITOR_SELECTION_BUDGET: Duration = Duration::from_secs(5);
const MAX_CLEANUP_SLICE_ENTRIES: usize = 50_000;
const MAX_CLEANUP_SLICE_DURATION: Duration = Duration::from_secs(5);
const MAX_CLEANUP_DEPTH: usize = 4_096;
const OWNER_CLEANUP_BUDGET: Duration = Duration::from_secs(60);
const JANITOR_CLEANUP_BUDGET: Duration = Duration::from_secs(30);
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OwnerKind {
    PublicExecution,
    PublicDelivery,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseMarker {
    schema: u32,
    run_id: String,
    created_unix_seconds: u64,
    owner: OwnerKind,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupReadyMarker {
    schema: u32,
    run_id: String,
    owner: OwnerKind,
    lease_device: u64,
    lease_inode: u64,
}

#[derive(Debug)]
pub(crate) struct ManagedRootCoordinator {
    path: Utf8PathBuf,
    dir: cap_std::fs::Dir,
    file: File,
    local_lock: LocalCoordinatorLock,
}

struct CoordinatorLockGuard<'a> {
    file: &'a File,
    _local: std::sync::MutexGuard<'a, ()>,
    locked: bool,
}

impl<'a> CoordinatorLockGuard<'a> {
    fn acquire(coordinator: &'a ManagedRootCoordinator) -> Result<Self, WorkspaceError> {
        let local = match coordinator.local_lock.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(WorkspaceError::StatePoisoned);
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(WorkspaceError::io(
                    "lock coordinator",
                    &coordinator.path,
                    std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        "coordinator is busy in this process",
                    ),
                ));
            }
        };
        FileExt::try_lock_exclusive(&coordinator.file)
            .map_err(|error| WorkspaceError::io("lock coordinator", &coordinator.path, error))?;
        Ok(Self {
            file: &coordinator.file,
            _local: local,
            locked: true,
        })
    }

    fn acquire_until(
        coordinator: &'a ManagedRootCoordinator,
        deadline: std::time::Instant,
    ) -> Result<Self, WorkspaceError> {
        loop {
            ensure_bootstrap_deadline(deadline, &coordinator.path, "lock coordinator")?;
            match coordinator.local_lock.try_lock() {
                Ok(local) => match FileExt::try_lock_exclusive(&coordinator.file) {
                    Ok(()) => {
                        if let Err(error) = ensure_bootstrap_deadline(
                            deadline,
                            &coordinator.path,
                            "lock coordinator",
                        ) {
                            let _ = FileExt::unlock(&coordinator.file);
                            return Err(error);
                        }
                        return Ok(Self {
                            file: &coordinator.file,
                            _local: local,
                            locked: true,
                        });
                    }
                    Err(error) if lock_error_is_busy(&error) => drop(local),
                    Err(error) => {
                        return Err(WorkspaceError::io(
                            "lock coordinator",
                            &coordinator.path,
                            error,
                        ));
                    }
                },
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(WorkspaceError::StatePoisoned);
                }
                Err(std::sync::TryLockError::WouldBlock) => {}
            }
            if std::time::Instant::now() >= deadline {
                return Err(WorkspaceError::io(
                    "lock coordinator",
                    &coordinator.path,
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "coordinator lock deadline exceeded",
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn unlock(mut self, path: &Utf8Path) -> Result<(), WorkspaceError> {
        FileExt::unlock(self.file)
            .map_err(|error| WorkspaceError::io("unlock coordinator", path, error))?;
        self.locked = false;
        Ok(())
    }
}

impl Drop for CoordinatorLockGuard<'_> {
    fn drop(&mut self) {
        if self.locked {
            let _ = FileExt::unlock(self.file);
        }
    }
}

impl ManagedRootCoordinator {
    pub(crate) fn open(parent: &Utf8Path) -> Result<Self, WorkspaceError> {
        let bootstrap_deadline = std::time::Instant::now() + JANITOR_SELECTION_BUDGET;
        let canonical_parent = fs::canonicalize(parent).map_err(|error| {
            WorkspaceError::io("canonicalize managed-root parent", parent, error)
        })?;
        ensure_bootstrap_deadline(
            bootstrap_deadline,
            parent,
            "canonicalize managed-root parent",
        )?;
        let canonical_parent = Utf8PathBuf::from_path_buf(canonical_parent)
            .map_err(|_| WorkspaceError::NonUtf8Path)?;
        let path = canonical_parent.join(MANAGED_DIR);
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "open managed-root parent")?;
        let parent_dir = cap_std::fs::Dir::open_ambient_dir(
            canonical_parent.as_std_path(),
            cap_std::ambient_authority(),
        )
        .map_err(|error| {
            WorkspaceError::io("open managed-root parent", &canonical_parent, error)
        })?;
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "open managed-root parent")?;
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "create managed root")?;
        match create_managed_root_entry(&parent_dir, &path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(WorkspaceError::io("create managed root", &path, error)),
        }
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "create managed root")?;
        let metadata = parent_dir
            .symlink_metadata(MANAGED_DIR)
            .map_err(|error| WorkspaceError::io("inspect managed root", &path, error))?;
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "inspect managed root")?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(WorkspaceError::InvalidPath { path });
        }
        let dir = cap_fs_ext::DirExt::open_dir_nofollow(&parent_dir, MANAGED_DIR)
            .map_err(|error| WorkspaceError::io("open managed root", &path, error))?;
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "open managed root")?;
        secure_managed_root(&dir, &path)?;
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "secure managed root")?;
        let local_lock = shared_coordinator_lock(&dir, &path, bootstrap_deadline)?;
        ensure_bootstrap_deadline(bootstrap_deadline, &path, "register coordinator lock")?;
        let file = {
            let _guard = lock_local_until(&local_lock, bootstrap_deadline)?;
            ensure_bootstrap_deadline(bootstrap_deadline, &path, "initialize coordinator")?;
            let file = initialize_coordinator(&dir, &path, bootstrap_deadline)?;
            ensure_bootstrap_deadline(bootstrap_deadline, &path, "initialize coordinator")?;
            file
        };
        Ok(Self {
            path,
            dir,
            file,
            local_lock,
        })
    }
}

fn ensure_bootstrap_deadline(
    deadline: std::time::Instant,
    path: &Utf8Path,
    operation: &'static str,
) -> Result<(), WorkspaceError> {
    if std::time::Instant::now() < deadline {
        Ok(())
    } else {
        Err(WorkspaceError::io(
            operation,
            path,
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "managed-root bootstrap deadline exceeded",
            ),
        ))
    }
}

#[cfg(unix)]
fn create_managed_root_entry(parent: &cap_std::fs::Dir, _path: &Utf8Path) -> std::io::Result<()> {
    rustix::fs::mkdirat(parent, MANAGED_DIR, rustix::fs::Mode::from_raw_mode(0o700))
        .map_err(std::io::Error::from)
}

#[cfg(windows)]
fn create_managed_root_entry(_parent: &cap_std::fs::Dir, path: &Utf8Path) -> std::io::Result<()> {
    windows::create_managed_directory(path)
}

#[cfg(target_os = "linux")]
fn open_owned_directory(
    parent: &cap_std::fs::Dir,
    name: impl AsRef<std::path::Path>,
) -> std::io::Result<cap_std::fs::Dir> {
    use rustix::fs::{Mode, OFlags, ResolveFlags};

    let directory = rustix::fs::openat2(
        parent,
        name.as_ref(),
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
    )
    .map_err(std::io::Error::from)?;
    let directory = cap_std::fs::Dir::from_std_file(directory.into());
    verify_current_user_owned_directory(&directory)?;
    Ok(directory)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn open_owned_directory(
    parent: &cap_std::fs::Dir,
    name: impl AsRef<std::path::Path>,
) -> std::io::Result<cap_std::fs::Dir> {
    let directory = cap_fs_ext::DirExt::open_dir_nofollow(parent, name)?;
    verify_current_user_owned_directory(&directory)?;
    Ok(directory)
}

#[cfg(windows)]
fn open_owned_directory(
    parent: &cap_std::fs::Dir,
    name: impl AsRef<std::path::Path>,
) -> std::io::Result<cap_std::fs::Dir> {
    let directory = super::root::windows::open_directory_shared(parent, name.as_ref().as_os_str())?;
    windows::verify_current_user_owner(&directory)?;
    Ok(cap_std::fs::Dir::from_std_file(directory))
}

#[cfg(unix)]
fn verify_current_user_owned_metadata(metadata: &cap_std::fs::Metadata) -> std::io::Result<()> {
    use cap_fs_ext::OsMetadataExt;

    let current_uid = unsafe { libc::geteuid() };
    if metadata.uid() == current_uid {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "managed object is not owned by the current user",
        ))
    }
}

#[cfg(unix)]
fn verify_current_user_owned_directory(dir: &cap_std::fs::Dir) -> std::io::Result<()> {
    verify_current_user_owned_metadata(&dir.metadata(".")?)
}

#[cfg(windows)]
fn verify_current_user_owned_directory(dir: &cap_std::fs::Dir) -> std::io::Result<()> {
    windows::verify_current_user_owner_handle(dir)
}

#[cfg(unix)]
fn rename_owned_directory(
    parent: &cap_std::fs::Dir,
    source: &str,
    destination: &str,
    expected_identity: (u64, u64),
) -> std::io::Result<()> {
    rename_owned_directory_with_guard(parent, source, destination, expected_identity, &|| Ok(()))
}

#[cfg(unix)]
fn rename_owned_directory_with_guard(
    parent: &cap_std::fs::Dir,
    source: &str,
    destination: &str,
    expected_identity: (u64, u64),
    before_next_operation: &impl Fn() -> Result<(), WorkspaceError>,
) -> std::io::Result<()> {
    before_next_operation().map_err(std::io::Error::other)?;
    let metadata = parent.symlink_metadata(source)?;
    before_next_operation().map_err(std::io::Error::other)?;
    if metadata_identity(&metadata) != expected_identity
        || !metadata.is_dir()
        || metadata.file_type().is_symlink()
    {
        return Err(std::io::Error::other(
            "rename source identity changed before claim",
        ));
    }
    before_next_operation().map_err(std::io::Error::other)?;
    parent.rename(source, parent, destination)
}

#[cfg(windows)]
fn rename_owned_directory(
    parent: &cap_std::fs::Dir,
    source: &str,
    destination: &str,
    expected_identity: (u64, u64),
) -> std::io::Result<()> {
    rename_owned_directory_with_guard(parent, source, destination, expected_identity, &|| Ok(()))
}

#[cfg(windows)]
fn rename_owned_directory_with_guard(
    parent: &cap_std::fs::Dir,
    source: &str,
    destination: &str,
    expected_identity: (u64, u64),
    before_next_operation: &impl Fn() -> Result<(), WorkspaceError>,
) -> std::io::Result<()> {
    super::root::windows::rename_entry_relative_with_guard(
        parent,
        std::ffi::OsStr::new(source),
        std::ffi::OsStr::new(destination),
        expected_identity,
        &|| before_next_operation().map_err(std::io::Error::other),
    )
}

fn shared_coordinator_lock(
    dir: &cap_std::fs::Dir,
    path: &Utf8Path,
    deadline: std::time::Instant,
) -> Result<LocalCoordinatorLock, WorkspaceError> {
    let identity = directory_identity(dir)
        .map_err(|error| WorkspaceError::io("identify managed root", path, error))?;
    let registry = COORDINATOR_LOCAL_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = lock_registry_until(registry, path, deadline)?;
    registry.retain(|_, lock| lock.strong_count() != 0);
    if let Some(lock) = registry.get(&identity).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    let lock = Arc::new(Mutex::new(()));
    registry.insert(identity, Arc::downgrade(&lock));
    Ok(lock)
}

fn lock_registry_until<'a>(
    registry: &'a LocalCoordinatorRegistry,
    path: &Utf8Path,
    deadline: std::time::Instant,
) -> Result<LocalCoordinatorRegistryGuard<'a>, WorkspaceError> {
    loop {
        ensure_bootstrap_deadline(deadline, path, "lock coordinator registry")?;
        match registry.try_lock() {
            Ok(registry) => {
                ensure_bootstrap_deadline(deadline, path, "lock coordinator registry")?;
                return Ok(registry);
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(WorkspaceError::StatePoisoned);
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                if std::time::Instant::now() >= deadline {
                    return Err(WorkspaceError::io(
                        "lock coordinator registry",
                        path,
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "coordinator registry lock deadline exceeded",
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

fn initialize_coordinator(
    dir: &cap_std::fs::Dir,
    managed_path: &Utf8Path,
    deadline: std::time::Instant,
) -> Result<File, WorkspaceError> {
    let coordinator_path = managed_path.join(COORDINATOR_FILE);
    let mut create = cap_std::fs::OpenOptions::new();
    create.create_new(true).read(true).write(true);
    configure_secure_create(&mut create);
    ensure_bootstrap_deadline(deadline, &coordinator_path, "create coordinator")?;
    match dir.open_with(COORDINATOR_FILE, &create) {
        Ok(file) => {
            let mut file = file.into_std();
            let identity = file_identity(&file).map_err(|error| {
                WorkspaceError::io("identify new coordinator", &coordinator_path, error)
            })?;
            let initialized = (|| {
                ensure_bootstrap_deadline(deadline, &coordinator_path, "create coordinator")?;
                lock_file_until(&file, deadline, &coordinator_path)?;
                ensure_bootstrap_deadline(deadline, &coordinator_path, "initialize coordinator")?;
                let bytes = initial_coordinator_bytes();
                file.write_all(&bytes).map_err(|error| {
                    WorkspaceError::io("initialize coordinator", &coordinator_path, error)
                })?;
                ensure_bootstrap_deadline(deadline, &coordinator_path, "initialize coordinator")?;
                file.sync_all().map_err(|error| {
                    WorkspaceError::io("flush coordinator", &coordinator_path, error)
                })?;
                ensure_bootstrap_deadline(deadline, &coordinator_path, "flush coordinator")?;
                secure_coordinator_file(&file, &coordinator_path)?;
                ensure_bootstrap_deadline(deadline, &coordinator_path, "secure coordinator")?;
                validate_coordinator_file(&mut file, &coordinator_path)?;
                ensure_bootstrap_deadline(deadline, &coordinator_path, "validate coordinator")?;
                FileExt::unlock(&file).map_err(|error| {
                    WorkspaceError::io("unlock coordinator", &coordinator_path, error)
                })?;
                ensure_bootstrap_deadline(deadline, &coordinator_path, "unlock coordinator")
            })();
            match initialized {
                Ok(()) => Ok(file),
                Err(error) => {
                    let _ = FileExt::unlock(&file);
                    drop(file);
                    if let Err(cleanup) = remove_new_coordinator_if_identity(dir, identity) {
                        return Err(WorkspaceError::io(
                            "rollback coordinator initialization",
                            &coordinator_path,
                            format!("{error}; rollback failed: {cleanup}"),
                        ));
                    }
                    Err(error)
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            open_initialized_coordinator_until(dir, &coordinator_path, deadline)
        }
        Err(error) => Err(WorkspaceError::io(
            "create coordinator",
            &coordinator_path,
            error,
        )),
    }
}

fn remove_new_coordinator_if_identity(
    dir: &cap_std::fs::Dir,
    expected_identity: (u64, u64),
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let metadata = dir.symlink_metadata(COORDINATOR_FILE)?;
        if metadata_identity(&metadata) != expected_identity
            || !metadata.is_file()
            || metadata.file_type().is_symlink()
        {
            return Err(std::io::Error::other(
                "new coordinator identity changed before rollback",
            ));
        }
        dir.remove_file(COORDINATOR_FILE)
    }
    #[cfg(windows)]
    {
        super::root::windows::remove_entry_io_checked(
            dir,
            std::ffi::OsStr::new(COORDINATOR_FILE),
            expected_identity,
        )
    }
}

fn open_initialized_coordinator_until(
    dir: &cap_std::fs::Dir,
    coordinator_path: &Utf8Path,
    deadline: std::time::Instant,
) -> Result<File, WorkspaceError> {
    loop {
        ensure_bootstrap_deadline(deadline, coordinator_path, "open coordinator")?;
        let metadata = dir
            .symlink_metadata(COORDINATOR_FILE)
            .map_err(|error| WorkspaceError::io("inspect coordinator", coordinator_path, error))?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "inspect coordinator")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(WorkspaceError::InvalidPath {
                path: coordinator_path.to_owned(),
            });
        }
        let expected_identity = metadata_identity(&metadata);
        let mut open = cap_std::fs::OpenOptions::new();
        open.read(true).write(true);
        configure_no_follow(&mut open);
        let file = dir
            .open_with(COORDINATOR_FILE, &open)
            .map_err(|error| WorkspaceError::io("open coordinator", coordinator_path, error))?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "open coordinator")?;
        let mut file = file.into_std();
        if file_identity(&file)
            .map_err(|error| WorkspaceError::io("identify coordinator", coordinator_path, error))?
            != expected_identity
        {
            return Err(WorkspaceError::InvalidPath {
                path: coordinator_path.to_owned(),
            });
        }
        ensure_bootstrap_deadline(deadline, coordinator_path, "identify coordinator")?;
        lock_file_until(&file, deadline, coordinator_path)?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "lock coordinator")?;
        let is_uninitialized = file
            .metadata()
            .map_err(|error| WorkspaceError::io("inspect coordinator", coordinator_path, error))?
            .len()
            == 0;
        ensure_bootstrap_deadline(deadline, coordinator_path, "inspect coordinator")?;
        if is_uninitialized {
            FileExt::unlock(&file).map_err(|error| {
                WorkspaceError::io("unlock coordinator", coordinator_path, error)
            })?;
            if std::time::Instant::now() >= deadline {
                return Err(WorkspaceError::io(
                    "open coordinator",
                    coordinator_path,
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "coordinator initialization deadline exceeded",
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        secure_coordinator_file(&file, coordinator_path)?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "secure coordinator")?;
        validate_coordinator_file(&mut file, coordinator_path)?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "validate coordinator")?;
        FileExt::unlock(&file)
            .map_err(|error| WorkspaceError::io("unlock coordinator", coordinator_path, error))?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "unlock coordinator")?;
        return Ok(file);
    }
}

fn lock_local_until(
    lock: &Mutex<()>,
    deadline: std::time::Instant,
) -> Result<std::sync::MutexGuard<'_, ()>, WorkspaceError> {
    loop {
        ensure_bootstrap_deadline(
            deadline,
            Utf8Path::new(COORDINATOR_FILE),
            "lock coordinator bootstrap",
        )?;
        match lock.try_lock() {
            Ok(guard) => {
                ensure_bootstrap_deadline(
                    deadline,
                    Utf8Path::new(COORDINATOR_FILE),
                    "lock coordinator bootstrap",
                )?;
                return Ok(guard);
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(WorkspaceError::StatePoisoned);
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                if std::time::Instant::now() >= deadline {
                    return Err(WorkspaceError::io(
                        "lock coordinator bootstrap",
                        Utf8Path::new(COORDINATOR_FILE),
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "coordinator bootstrap lock deadline exceeded",
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

fn lock_file_until(
    file: &File,
    deadline: std::time::Instant,
    path: &Utf8Path,
) -> Result<(), WorkspaceError> {
    loop {
        ensure_bootstrap_deadline(deadline, path, "lock coordinator bootstrap")?;
        match FileExt::try_lock_exclusive(file) {
            Ok(()) => {
                if let Err(error) =
                    ensure_bootstrap_deadline(deadline, path, "lock coordinator bootstrap")
                {
                    let _ = FileExt::unlock(file);
                    return Err(error);
                }
                return Ok(());
            }
            Err(error) if lock_error_is_busy(&error) => {
                if std::time::Instant::now() >= deadline {
                    return Err(WorkspaceError::io(
                        "lock coordinator bootstrap",
                        path,
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "coordinator bootstrap file lock deadline exceeded",
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(WorkspaceError::io("lock coordinator", path, error)),
        }
    }
}

fn lock_error_is_busy(error: &std::io::Error) -> bool {
    let contended = fs2::lock_contended_error();
    match (error.raw_os_error(), contended.raw_os_error()) {
        (Some(actual), Some(expected)) => actual == expected,
        _ => error.kind() == contended.kind(),
    }
}

fn open_coordinator_file(
    dir: &cap_std::fs::Dir,
    managed_path: &Utf8Path,
) -> Result<File, WorkspaceError> {
    let coordinator_path = managed_path.join(COORDINATOR_FILE);
    let mut open = cap_std::fs::OpenOptions::new();
    open.read(true).write(true);
    configure_no_follow(&mut open);
    let file = dir
        .open_with(COORDINATOR_FILE, &open)
        .map_err(|error| WorkspaceError::io("open coordinator", &coordinator_path, error))?;
    let mut file = file.into_std();
    validate_coordinator_file(&mut file, &coordinator_path)?;
    Ok(file)
}

#[cfg(unix)]
fn configure_no_follow(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(unix)]
fn configure_secure_create(options: &mut cap_std::fs::OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(windows)]
fn configure_secure_create(_options: &mut cap_std::fs::OpenOptions) {}

#[cfg(windows)]
fn configure_no_follow(options: &mut cap_std::fs::OpenOptions) {
    use cap_fs_ext::OpenOptionsFollowExt;

    options.follow(cap_fs_ext::FollowSymlinks::No);
}

fn initial_coordinator_bytes() -> [u8; COORDINATOR_LAYOUT_BYTES] {
    let mut bytes = [0_u8; COORDINATOR_LAYOUT_BYTES];
    let slot = initial_coordinator_slot();
    bytes[1..=COORDINATOR_SLOT_BYTES].copy_from_slice(&slot);
    bytes[1 + COORDINATOR_SLOT_BYTES..].copy_from_slice(&slot);
    bytes
}

fn initial_coordinator_slot() -> [u8; COORDINATOR_SLOT_BYTES] {
    coordinator_slot_bytes(0, "").expect("empty initial cursor is valid")
}

fn coordinator_slot_bytes(
    generation: u64,
    cursor: &str,
) -> Result<[u8; COORDINATOR_SLOT_BYTES], WorkspaceError> {
    if cursor.len() > COORDINATOR_CURSOR_BYTES
        || (!cursor.is_empty() && !valid_managed_name(cursor))
    {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(cursor),
        });
    }
    let mut slot = [0_u8; COORDINATOR_SLOT_BYTES];
    slot[..8].copy_from_slice(COORDINATOR_MAGIC);
    slot[8..12].copy_from_slice(&COORDINATOR_SCHEMA.to_le_bytes());
    slot[12..20].copy_from_slice(&generation.to_le_bytes());
    slot[20..22].copy_from_slice(
        &u16::try_from(cursor.len())
            .expect("cursor bound fits u16")
            .to_le_bytes(),
    );
    slot[22..22 + cursor.len()].copy_from_slice(cursor.as_bytes());
    let crc = crc32fast::hash(&slot[..508]);
    slot[508..].copy_from_slice(&crc.to_le_bytes());
    Ok(slot)
}

fn validate_coordinator_file(file: &mut File, path: &Utf8Path) -> Result<(), WorkspaceError> {
    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect coordinator", path, error))?;
    if !metadata.is_file() || metadata.len() != COORDINATOR_BYTES {
        return Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        });
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| WorkspaceError::io("seek coordinator", path, error))?;
    let mut bytes = [0_u8; COORDINATOR_LAYOUT_BYTES];
    file.read_exact(&mut bytes)
        .map_err(|error| WorkspaceError::io("read coordinator", path, error))?;
    if bytes[0] != 0 {
        return Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        });
    }
    let first = decode_coordinator_slot(&bytes[1..513]);
    let second = decode_coordinator_slot(&bytes[513..]);
    match (first, second) {
        (Some(first), Some(second))
            if first.generation == second.generation && first.cursor != second.cursor =>
        {
            Err(WorkspaceError::InvalidPath {
                path: path.to_owned(),
            })
        }
        (None, None) => Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        }),
        _ => Ok(()),
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CoordinatorSlot {
    generation: u64,
    cursor: String,
}

fn decode_coordinator_slot(slot: &[u8]) -> Option<CoordinatorSlot> {
    let cursor_len = usize::from(u16::from_le_bytes([slot[20], slot[21]]));
    let valid = &slot[..8] == COORDINATOR_MAGIC
        && u32::from_le_bytes(slot[8..12].try_into().expect("fixed slot")) == COORDINATOR_SCHEMA
        && cursor_len <= COORDINATOR_CURSOR_BYTES
        && slot[22 + cursor_len..508].iter().all(|byte| *byte == 0)
        && u32::from_le_bytes(slot[508..512].try_into().expect("fixed slot"))
            == crc32fast::hash(&slot[..508]);
    if !valid {
        return None;
    }
    let cursor = std::str::from_utf8(&slot[22..22 + cursor_len]).ok()?;
    if !cursor.is_empty() && !valid_managed_name(cursor) {
        return None;
    }
    Some(CoordinatorSlot {
        generation: u64::from_le_bytes(slot[12..20].try_into().expect("fixed slot")),
        cursor: cursor.to_owned(),
    })
}

fn valid_managed_name(name: &str) -> bool {
    [ACTIVE_PREFIX, DELETING_PREFIX, STAGING_PREFIX]
        .into_iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok_and(|parsed| parsed.to_string() == id))
}

fn read_coordinator_state(
    file: &File,
    path: &Utf8Path,
) -> Result<(usize, CoordinatorSlot), WorkspaceError> {
    let mut file = file
        .try_clone()
        .map_err(|error| WorkspaceError::io("clone coordinator", path, error))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| WorkspaceError::io("seek coordinator", path, error))?;
    let mut bytes = [0_u8; COORDINATOR_LAYOUT_BYTES];
    file.read_exact(&mut bytes)
        .map_err(|error| WorkspaceError::io("read coordinator", path, error))?;
    let first = decode_coordinator_slot(&bytes[1..513]);
    let second = decode_coordinator_slot(&bytes[513..]);
    match (first, second) {
        (Some(first), Some(second)) if first.generation == second.generation => {
            if first.cursor == second.cursor {
                Ok((0, first))
            } else {
                Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                })
            }
        }
        (Some(first), Some(second)) if first.generation > second.generation => Ok((0, first)),
        (Some(_) | None, Some(second)) => Ok((1, second)),
        (Some(first), None) => Ok((0, first)),
        (None, None) => Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        }),
    }
}

fn persist_coordinator_cursor(
    file: &File,
    path: &Utf8Path,
    active_slot: usize,
    state: &CoordinatorSlot,
    cursor: &str,
) -> Result<(), WorkspaceError> {
    let generation = state.generation.checked_add(1).ok_or_else(|| {
        WorkspaceError::io("advance coordinator cursor", path, "generation overflow")
    })?;
    let slot = coordinator_slot_bytes(generation, cursor)?;
    let inactive_slot = 1_usize.saturating_sub(active_slot);
    let offset = 1_u64
        + u64::try_from(inactive_slot).expect("slot index fits u64")
            * u64::try_from(COORDINATOR_SLOT_BYTES).expect("slot size fits u64");
    let mut file = file
        .try_clone()
        .map_err(|error| WorkspaceError::io("clone coordinator", path, error))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| WorkspaceError::io("seek coordinator slot", path, error))?;
    file.write_all(&slot)
        .map_err(|error| WorkspaceError::io("write coordinator cursor", path, error))?;
    file.sync_all()
        .map_err(|error| WorkspaceError::io("flush coordinator cursor", path, error))
}

#[cfg(unix)]
fn secure_coordinator_file(file: &File, path: &Utf8Path) -> Result<(), WorkspaceError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect coordinator", path, error))?;
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        });
    }
    let mut permissions = metadata.permissions();
    if permissions.mode() & 0o777 != 0o600 {
        permissions.set_mode(0o600);
        file.set_permissions(permissions)
            .map_err(|error| WorkspaceError::io("secure coordinator", path, error))?;
    }
    let verified = file
        .metadata()
        .map_err(|error| WorkspaceError::io("verify coordinator", path, error))?;
    if verified.uid() != unsafe { libc::geteuid() }
        || verified.permissions().mode() & 0o777 != 0o600
    {
        return Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn secure_coordinator_file(file: &File, path: &Utf8Path) -> Result<(), WorkspaceError> {
    windows::secure_file(file, path)
        .map_err(|error| WorkspaceError::io("secure coordinator", path, error))
}

#[cfg(unix)]
fn secure_managed_root(dir: &cap_std::fs::Dir, path: &Utf8Path) -> Result<(), WorkspaceError> {
    use cap_fs_ext::OsMetadataExt;
    use cap_std::fs::PermissionsExt;

    let metadata = dir
        .metadata(".")
        .map_err(|error| WorkspaceError::io("inspect managed root", path, error))?;
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        });
    }
    if metadata.permissions().mode() & 0o777 != 0o700 {
        dir.set_permissions(".", cap_std::fs::Permissions::from_mode(0o700))
            .map_err(|error| WorkspaceError::io("secure managed root", path, error))?;
    }
    let verified = dir
        .metadata(".")
        .map_err(|error| WorkspaceError::io("verify managed root", path, error))?;
    if verified.uid() != unsafe { libc::geteuid() }
        || verified.permissions().mode() & 0o777 != 0o700
    {
        return Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn secure_managed_root(dir: &cap_std::fs::Dir, path: &Utf8Path) -> Result<(), WorkspaceError> {
    let file = dir
        .try_clone()
        .map_err(|error| WorkspaceError::io("clone managed root", path, error))?
        .into_std_file();
    windows::secure_directory(&file, path)
        .map_err(|error| WorkspaceError::io("secure managed root", path, error))
}

#[derive(Debug)]
pub(crate) struct ManagedRunRoot {
    path: Utf8PathBuf,
    dir: cap_std::fs::Dir,
    coordinator_dir: cap_std::fs::Dir,
    coordinator_file: File,
    coordinator_local_lock: Arc<Mutex<()>>,
    run_id: String,
    owner: OwnerKind,
    lease: Arc<File>,
    heartbeat: Mutex<File>,
    lifecycle: Arc<Mutex<RootLifecycle>>,
}

#[derive(Debug, Default)]
struct RootLifecycle {
    cleanup_started: bool,
    cleanup_ready: bool,
    live_children: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublishBoundary {
    StagingCreated,
    StagingOpened,
    RenamedActive,
    ActiveOpened,
    CoordinatorOpened,
    CoordinatorCloned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChildCreationBoundary {
    Created,
    Opened,
}

#[derive(Clone, Copy)]
struct StagingPublication<'a> {
    name: &'a str,
    path: &'a Utf8Path,
    run_id: &'a str,
    owner: OwnerKind,
    dir: &'a cap_std::fs::Dir,
}

struct NewDirectoryRollback<'a> {
    parent: &'a cap_std::fs::Dir,
    name: &'a str,
    expected_identity: Option<(u64, u64)>,
    armed: bool,
}

impl NewDirectoryRollback<'_> {
    fn expect_identity(&mut self, identity: (u64, u64)) {
        self.expected_identity = Some(identity);
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for NewDirectoryRollback<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let expected_identity = self.expected_identity.or_else(|| {
            self.parent
                .symlink_metadata(self.name)
                .ok()
                .filter(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                .map(|metadata| metadata_identity(&metadata))
        });
        let Some(expected_identity) = expected_identity else {
            return;
        };
        let _ = remove_claimed_tree_bounded(
            self.parent,
            self.name,
            Some(expected_identity),
            OWNER_CLEANUP_BUDGET,
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CleanupRecord {
    pub(crate) status: DiskCleanupStatus,
    pub(crate) examined_entries: u64,
    pub(crate) removed_entries: u64,
    pub(crate) details: Vec<String>,
    pub(crate) omitted_detail_count: u64,
    pub(crate) remaining_root: Option<Utf8PathBuf>,
}

impl CleanupRecord {
    pub(crate) fn push_detail(&mut self, detail: String) {
        if self.details.len() >= MAX_DIAGNOSTIC_DETAILS {
            self.omitted_detail_count = self.omitted_detail_count.saturating_add(1);
            return;
        }
        let (detail, truncated) = truncate_diagnostic_detail(detail);
        if truncated {
            self.omitted_detail_count = self.omitted_detail_count.saturating_add(1);
        }
        self.details.push(detail);
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReclaimReport {
    pub(crate) reclaimed_roots: u64,
    pub(crate) preserved_roots: u64,
    pub(crate) details: Vec<String>,
    pub(crate) omitted_detail_count: u64,
    pub(crate) truncated_detail_count: u64,
}

impl ManagedRunRoot {
    pub(crate) fn create(
        coordinator: &ManagedRootCoordinator,
        owner: OwnerKind,
    ) -> Result<Self, WorkspaceError> {
        Self::create_with_publish_hook(coordinator, owner, &|_| Ok(()))
    }

    fn create_with_publish_hook(
        coordinator: &ManagedRootCoordinator,
        owner: OwnerKind,
        publish_hook: &impl Fn(PublishBoundary) -> Result<(), WorkspaceError>,
    ) -> Result<Self, WorkspaceError> {
        let initialization_guard = CoordinatorLockGuard::acquire_until(
            coordinator,
            std::time::Instant::now() + JANITOR_SELECTION_BUDGET,
        )?;
        let run_id = uuid::Uuid::new_v4().to_string();
        let staging_name = format!("{STAGING_PREFIX}{run_id}");
        coordinator
            .dir
            .create_dir(&staging_name)
            .map_err(|error| WorkspaceError::io("create staging root", &coordinator.path, error))?;
        let staging_path = coordinator.path.join(&staging_name);
        let mut staging_rollback = NewDirectoryRollback {
            parent: &coordinator.dir,
            name: &staging_name,
            expected_identity: None,
            armed: true,
        };
        let staging_metadata = coordinator
            .dir
            .symlink_metadata(&staging_name)
            .map_err(|error| WorkspaceError::io("inspect staging root", &staging_path, error))?;
        if !staging_metadata.is_dir() || staging_metadata.file_type().is_symlink() {
            return Err(WorkspaceError::InvalidPath { path: staging_path });
        }
        let staging_identity = metadata_identity(&staging_metadata);
        staging_rollback.expect_identity(staging_identity);
        publish_hook(PublishBoundary::StagingCreated)?;
        let staging_dir = open_owned_directory(&coordinator.dir, &staging_name)
            .map_err(|error| WorkspaceError::io("open staging root", &staging_path, error))?;
        publish_hook(PublishBoundary::StagingOpened)?;
        let opened_staging_identity = directory_identity(&staging_dir)
            .map_err(|error| WorkspaceError::io("identify staging root", &staging_path, error))?;
        if opened_staging_identity != staging_identity {
            return Err(WorkspaceError::InvalidPath { path: staging_path });
        }
        let publication = StagingPublication {
            name: &staging_name,
            path: &staging_path,
            run_id: &run_id,
            owner,
            dir: &staging_dir,
        };
        let result =
            Self::publish_staging(coordinator, publication, initialization_guard, publish_hook);
        if result.is_err() {
            let _ = remove_claimed_tree_bounded(
                &coordinator.dir,
                &staging_name,
                Some(staging_identity),
                OWNER_CLEANUP_BUDGET,
            );
        } else {
            staging_rollback.disarm();
        }
        result
    }

    #[allow(
        clippy::too_many_lines,
        reason = "lease publication keeps rollback adjacent to every post-rename failure"
    )]
    fn publish_staging(
        coordinator: &ManagedRootCoordinator,
        publication: StagingPublication<'_>,
        initialization_guard: CoordinatorLockGuard<'_>,
        publish_hook: &impl Fn(PublishBoundary) -> Result<(), WorkspaceError>,
    ) -> Result<Self, WorkspaceError> {
        let StagingPublication {
            name: staging_name,
            path: staging_path,
            run_id,
            owner,
            dir: staging_dir,
        } = publication;
        let marker = LeaseMarker {
            schema: LEASE_SCHEMA,
            run_id: run_id.to_owned(),
            created_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| WorkspaceError::io("read system time", staging_path, error))?
                .as_secs(),
            owner,
        };
        let lease_path = staging_path.join(LEASE_FILE);
        let mut lease_options = cap_std::fs::OpenOptions::new();
        lease_options.create_new(true).read(true).write(true);
        configure_no_follow(&mut lease_options);
        let mut lease = staging_dir
            .open_with(LEASE_FILE, &lease_options)
            .map_err(|error| WorkspaceError::io("create lease marker", &lease_path, error))?
            .into_std();
        serde_json::to_writer(&mut lease, &marker)
            .map_err(|error| WorkspaceError::io("write lease marker", &lease_path, error))?;
        lease
            .write_all(b"\n")
            .map_err(|error| WorkspaceError::io("write lease marker", &lease_path, error))?;
        lease
            .sync_all()
            .map_err(|error| WorkspaceError::io("flush lease marker", &lease_path, error))?;
        FileExt::try_lock_exclusive(&lease)
            .map_err(|error| WorkspaceError::io("lock lease marker", &lease_path, error))?;
        let heartbeat_path = staging_path.join(HEARTBEAT_FILE);
        let mut heartbeat_options = cap_std::fs::OpenOptions::new();
        heartbeat_options.create_new(true).write(true);
        configure_no_follow(&mut heartbeat_options);
        let mut heartbeat = staging_dir
            .open_with(HEARTBEAT_FILE, &heartbeat_options)
            .map_err(|error| WorkspaceError::io("create heartbeat marker", &heartbeat_path, error))?
            .into_std();
        serde_json::to_writer(&mut heartbeat, &marker).map_err(|error| {
            WorkspaceError::io("write heartbeat marker", &heartbeat_path, error)
        })?;
        heartbeat.write_all(b"\n").map_err(|error| {
            WorkspaceError::io("write heartbeat marker", &heartbeat_path, error)
        })?;
        heartbeat.sync_all().map_err(|error| {
            WorkspaceError::io("flush heartbeat marker", &heartbeat_path, error)
        })?;
        initialization_guard.unlock(&coordinator.path)?;

        let active_name = format!("{ACTIVE_PREFIX}{run_id}");
        let publish_guard = CoordinatorLockGuard::acquire_until(
            coordinator,
            std::time::Instant::now() + JANITOR_SELECTION_BUDGET,
        )?;
        let staging_identity = directory_identity(staging_dir)
            .map_err(|error| WorkspaceError::io("identify staging root", staging_path, error))?;
        let publish = rename_owned_directory(
            &coordinator.dir,
            staging_name,
            &active_name,
            staging_identity,
        )
        .map_err(|error| WorkspaceError::io("publish managed root", staging_path, error));
        let unlock = publish_guard.unlock(&coordinator.path);
        publish?;
        if let Err(error) = unlock {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                staging_dir,
                &lease,
            );
            return Err(error);
        }
        if let Err(error) = publish_hook(PublishBoundary::RenamedActive) {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                staging_dir,
                &lease,
            );
            return Err(error);
        }
        let path = coordinator.path.join(&active_name);
        let dir = match open_owned_directory(&coordinator.dir, &active_name) {
            Ok(dir) => dir,
            Err(error) => {
                rollback_published_root(
                    coordinator,
                    &active_name,
                    run_id,
                    owner,
                    staging_dir,
                    &lease,
                );
                return Err(WorkspaceError::io("open published root", &path, error));
            }
        };
        if let Err(error) = publish_hook(PublishBoundary::ActiveOpened) {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                staging_dir,
                &lease,
            );
            return Err(error);
        }
        let coordinator_file = match open_coordinator_file(&coordinator.dir, &coordinator.path) {
            Ok(file) => file,
            Err(error) => {
                rollback_published_root(
                    coordinator,
                    &active_name,
                    run_id,
                    owner,
                    staging_dir,
                    &lease,
                );
                return Err(error);
            }
        };
        if let Err(error) = publish_hook(PublishBoundary::CoordinatorOpened) {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                staging_dir,
                &lease,
            );
            return Err(error);
        }
        let coordinator_dir = match coordinator.dir.try_clone() {
            Ok(dir) => dir,
            Err(error) => {
                rollback_published_root(
                    coordinator,
                    &active_name,
                    run_id,
                    owner,
                    staging_dir,
                    &lease,
                );
                return Err(WorkspaceError::io(
                    "clone managed root",
                    &coordinator.path,
                    error,
                ));
            }
        };
        if let Err(error) = publish_hook(PublishBoundary::CoordinatorCloned) {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                staging_dir,
                &lease,
            );
            return Err(error);
        }
        Ok(Self {
            path,
            dir,
            coordinator_dir,
            coordinator_file,
            coordinator_local_lock: Arc::clone(&coordinator.local_lock),
            run_id: run_id.to_owned(),
            owner,
            lease: Arc::new(lease),
            heartbeat: Mutex::new(heartbeat),
            lifecycle: Arc::new(Mutex::new(RootLifecycle::default())),
        })
    }

    #[allow(dead_code, reason = "Task 6 reports retained managed-root locations")]
    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub(crate) fn disk_capability(&self) -> std::io::Result<super::disk::RootCapability> {
        super::disk::RootCapability::from_dir(&self.dir, self.path.clone())
    }

    pub(crate) fn create_child(&self, prefix: &str) -> Result<ManagedChild, WorkspaceError> {
        self.create_child_with_hook(prefix, &|_| Ok(()))
    }

    fn create_child_with_hook(
        &self,
        prefix: &str,
        creation_hook: &impl Fn(ChildCreationBoundary) -> Result<(), WorkspaceError>,
    ) -> Result<ManagedChild, WorkspaceError> {
        if !valid_child_prefix(prefix) {
            return Err(WorkspaceError::InvalidPath {
                path: Utf8PathBuf::from(prefix),
            });
        }
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        if lifecycle.cleanup_started {
            return Err(WorkspaceError::io(
                "create managed child",
                &self.path,
                "managed root cleanup has started",
            ));
        }
        let child_parent = self
            .dir
            .try_clone()
            .map_err(|error| WorkspaceError::io("clone managed child parent", &self.path, error))?;
        let mut child_count = 0_usize;
        for entry in owned_directory_entries(&self.dir)
            .map_err(|error| WorkspaceError::io("count managed children", &self.path, error))?
        {
            entry
                .map_err(|error| WorkspaceError::io("count managed children", &self.path, error))?;
            child_count = child_count
                .checked_add(1)
                .ok_or(WorkspaceError::CopySizeOverflow)?;
            ensure_direct_child_capacity(child_count)?;
        }
        let name = format!("{prefix}{}", uuid::Uuid::new_v4());
        self.dir
            .create_dir(&name)
            .map_err(|error| WorkspaceError::io("create managed child", &self.path, error))?;
        let path = self.path.join(&name);
        let mut child_rollback = NewDirectoryRollback {
            parent: &self.dir,
            name: &name,
            expected_identity: None,
            armed: true,
        };
        creation_hook(ChildCreationBoundary::Created)?;
        let child_metadata = self
            .dir
            .symlink_metadata(&name)
            .map_err(|error| WorkspaceError::io("inspect managed child", &path, error))?;
        if !child_metadata.is_dir() || child_metadata.file_type().is_symlink() {
            return Err(WorkspaceError::InvalidPath { path });
        }
        let child_identity = metadata_identity(&child_metadata);
        child_rollback.expect_identity(child_identity);
        let dir = open_owned_directory(&self.dir, &name)
            .map_err(|error| WorkspaceError::io("open managed child", &path, error))?;
        creation_hook(ChildCreationBoundary::Opened)?;
        let opened_child_identity = directory_identity(&dir)
            .map_err(|error| WorkspaceError::io("identify managed child", &path, error))?;
        if opened_child_identity != child_identity {
            return Err(WorkspaceError::InvalidPath { path });
        }
        lifecycle.live_children = lifecycle
            .live_children
            .checked_add(1)
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        child_rollback.disarm();
        drop(child_rollback);
        Ok(ManagedChild {
            path,
            dir,
            parent: child_parent,
            name,
            lifecycle: Arc::clone(&self.lifecycle),
            _lease: Arc::clone(&self.lease),
        })
    }

    #[allow(dead_code, reason = "Task 6 wires explicit retained-result policy")]
    pub(crate) fn retain(&self) -> Result<(), WorkspaceError> {
        self.write_control_marker(RETAIN_FILE)
    }

    #[allow(
        dead_code,
        reason = "Task 6 wires post-drain cleanup-ready transitions"
    )]
    pub(crate) fn mark_cleanup_ready(&self) -> Result<(), WorkspaceError> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        if lifecycle.cleanup_ready {
            return Ok(());
        }
        write_cleanup_ready_marker(&self.dir, &self.path, &self.run_id, self.owner, &self.lease)?;
        lifecycle.cleanup_ready = true;
        Ok(())
    }

    #[allow(dead_code, reason = "shared by Task 6 lifecycle marker transitions")]
    fn write_control_marker(&self, name: &str) -> Result<(), WorkspaceError> {
        let marker = LeaseMarker {
            schema: LEASE_SCHEMA,
            run_id: self.run_id.clone(),
            created_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| WorkspaceError::io("read system time", &self.path, error))?
                .as_secs(),
            owner: self.owner,
        };
        self.write_json_marker(name, &marker)
    }

    fn write_json_marker(&self, name: &str, marker: &impl Serialize) -> Result<(), WorkspaceError> {
        let marker_path = self.path.join(name);
        let mut options = cap_std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        let mut file = self
            .dir
            .open_with(name, &options)
            .map_err(|error| WorkspaceError::io("create workspace marker", &marker_path, error))?;
        serde_json::to_writer(&mut file, &marker)
            .map_err(|error| WorkspaceError::io("write workspace marker", &marker_path, error))?;
        file.write_all(b"\n")
            .map_err(|error| WorkspaceError::io("write workspace marker", &marker_path, error))?;
        file.sync_all()
            .map_err(|error| WorkspaceError::io("flush workspace marker", &marker_path, error))
    }

    #[allow(
        dead_code,
        reason = "Task 6 disk monitor refreshes the open heartbeat identity"
    )]
    pub(crate) fn refresh_heartbeat(&self) -> Result<(), WorkspaceError> {
        let heartbeat = self
            .heartbeat
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        refresh_file_modified_time(&heartbeat)
            .map_err(|error| WorkspaceError::io("refresh workspace heartbeat", &self.path, error))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "cleanup records every fail-closed claim and bounded-removal outcome"
    )]
    pub(crate) fn cleanup(&self, budget: Duration) -> CleanupRecord {
        let started = std::time::Instant::now();
        let mut record = CleanupRecord {
            status: DiskCleanupStatus::Failed,
            examined_entries: 0,
            removed_entries: 0,
            details: Vec::new(),
            omitted_detail_count: 0,
            remaining_root: Some(self.path.clone()),
        };
        if budget.is_zero() {
            record.status = DiskCleanupStatus::Deferred;
            return record;
        }
        let Ok(mut lifecycle) = self.lifecycle.lock() else {
            record.push_detail("managed root lifecycle is poisoned".to_owned());
            return record;
        };
        lifecycle.cleanup_started = true;
        if lifecycle.live_children != 0 {
            record.status = DiskCleanupStatus::Deferred;
            record.push_detail(format!(
                "managed root still has {} live child handle(s)",
                lifecycle.live_children
            ));
            return record;
        }
        let expected_root_identity = match directory_identity(&self.dir) {
            Ok(identity) => identity,
            Err(error) => {
                record.push_detail(format!("managed root identity inspection failed: {error}"));
                return record;
            }
        };
        let expected_lease = &self.lease;
        let local_guard = match self.coordinator_local_lock.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                record.status = DiskCleanupStatus::Deferred;
                record.push_detail("coordinator process lock is busy".to_owned());
                return record;
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                record.push_detail("coordinator process lock is poisoned".to_owned());
                return record;
            }
        };
        if let Err(error) = FileExt::try_lock_exclusive(&self.coordinator_file) {
            if lock_error_is_busy(&error) {
                record.status = DiskCleanupStatus::Deferred;
            }
            record.push_detail(format!("coordinator lock failed: {error}"));
            return record;
        }
        let active_name = format!("{ACTIVE_PREFIX}{}", self.run_id);
        let deleting_name = format!("{DELETING_PREFIX}{}", self.run_id);
        let claim_deadline_expired = std::cell::Cell::new(false);
        let claim = claim_managed_child_with_guard(
            &self.coordinator_dir,
            &active_name,
            &deleting_name,
            &self.run_id,
            self.owner,
            &self.dir,
            expected_lease,
            &|| {
                if started.elapsed() < budget {
                    Ok(())
                } else {
                    claim_deadline_expired.set(true);
                    Err(WorkspaceError::io(
                        "claim managed workspace",
                        &self.path,
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "owner cleanup deadline exceeded before claim",
                        ),
                    ))
                }
            },
        );
        let unlock = FileExt::unlock(&self.coordinator_file);
        if let Err(error) = unlock {
            if claim == Ok(ClaimResult::Claimed) {
                record.remaining_root = Some(
                    self.path
                        .parent()
                        .expect("managed run has parent")
                        .join(&deleting_name),
                );
            }
            record.push_detail(format!("coordinator unlock failed: {error}"));
            return record;
        }
        drop(local_guard);
        match claim {
            Ok(ClaimResult::Absent) => {
                record.status = DiskCleanupStatus::Clean;
                record.remaining_root = None;
                return record;
            }
            Ok(ClaimResult::Claimed) => {}
            Err(error) => {
                if claim_deadline_expired.get() {
                    record.status = DiskCleanupStatus::Deferred;
                }
                record.push_detail(error.to_string());
                return record;
            }
        }
        if started.elapsed() >= budget {
            record.status = DiskCleanupStatus::Deferred;
            record.remaining_root = Some(
                self.path
                    .parent()
                    .expect("managed run has parent")
                    .join(&deleting_name),
            );
            return record;
        }
        match remove_claimed_tree_bounded(
            &self.coordinator_dir,
            &deleting_name,
            Some(expected_root_identity),
            budget.saturating_sub(started.elapsed()),
        ) {
            Ok(slice)
                if slice.complete && entry_is_absent(&self.coordinator_dir, &deleting_name) =>
            {
                record.status = DiskCleanupStatus::Clean;
                record.examined_entries = slice.examined;
                record.removed_entries = slice.removed;
                record.remaining_root = None;
            }
            Ok(slice) => {
                record.status = DiskCleanupStatus::Deferred;
                record.examined_entries = slice.examined;
                record.removed_entries = slice.removed;
                record.remaining_root = Some(
                    self.path
                        .parent()
                        .expect("managed run has parent")
                        .join(&deleting_name),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                record.status = DiskCleanupStatus::Clean;
                record.remaining_root = None;
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                record.status = DiskCleanupStatus::Deferred;
                record.push_detail(error.to_string());
                record.remaining_root = Some(
                    self.path
                        .parent()
                        .expect("managed run has parent")
                        .join(&deleting_name),
                );
            }
            Err(error) => record.push_detail(error.to_string()),
        }
        record
    }

    #[allow(
        dead_code,
        reason = "Task 6 wires unsafe-to-clean shutdown transitions"
    )]
    pub(crate) fn abandon_for_janitor(&self, reason: String) -> CleanupRecord {
        let mut record = CleanupRecord {
            status: DiskCleanupStatus::Deferred,
            examined_entries: 0,
            removed_entries: 0,
            details: Vec::new(),
            omitted_detail_count: 0,
            remaining_root: Some(self.path.clone()),
        };
        record.push_detail(reason);
        record
    }

    #[allow(
        clippy::too_many_lines,
        reason = "janitor validation and claim order is kept explicit for security review"
    )]
    pub(crate) fn reclaim_abandoned(
        coordinator: &ManagedRootCoordinator,
        now: SystemTime,
    ) -> ReclaimReport {
        let mut report = ReclaimReport::default();
        let mut pending_cleanup = VecDeque::new();
        let cleanup_started = std::time::Instant::now();
        let (candidates, cursor_error) = match select_reclaim_candidates(coordinator) {
            Ok(selection) => selection,
            Err(error) => {
                push_reclaim_detail(&mut report, error.to_string());
                return report;
            }
        };
        if let Some(error) = cursor_error {
            push_reclaim_detail(&mut report, error);
        }
        for name in candidates {
            if cleanup_started.elapsed() >= JANITOR_CLEANUP_BUDGET {
                break;
            }
            let name = name.as_str();
            let run_id = if let Some(run_id) = name.strip_prefix(ACTIVE_PREFIX) {
                run_id
            } else if let Some(run_id) = name.strip_prefix(DELETING_PREFIX) {
                run_id
            } else if let Some(run_id) = name.strip_prefix(STAGING_PREFIX) {
                run_id
            } else {
                continue;
            };
            let staging = name.starts_with(STAGING_PREFIX);
            let deleting = name.starts_with(DELETING_PREFIX);
            if uuid::Uuid::parse_str(run_id).is_err() {
                report.preserved_roots += 1;
                continue;
            }
            let Ok(candidate) = open_owned_directory(&coordinator.dir, name) else {
                report.preserved_roots += 1;
                continue;
            };
            let Ok(candidate_identity) = directory_identity(&candidate) else {
                report.preserved_roots += 1;
                continue;
            };
            let lease_metadata = candidate.symlink_metadata(LEASE_FILE);
            let unmarked_staging_is_reclaimable = staging
                && staging_is_empty(&candidate)
                && directory_timestamp_is_stale(&candidate, now);
            let empty_deleting_is_resumable =
                name.starts_with(DELETING_PREFIX) && staging_is_empty(&candidate);
            if matches!(&lease_metadata, Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                && (unmarked_staging_is_reclaimable || empty_deleting_is_resumable)
            {
                let local_guard = match coordinator.local_lock.try_lock() {
                    Ok(guard) => guard,
                    Err(std::sync::TryLockError::WouldBlock) => {
                        report.preserved_roots += 1;
                        continue;
                    }
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        report.preserved_roots += 1;
                        push_reclaim_detail(
                            &mut report,
                            "coordinator process lock is poisoned".to_owned(),
                        );
                        continue;
                    }
                };
                if let Err(error) = FileExt::try_lock_exclusive(&coordinator.file) {
                    report.preserved_roots += 1;
                    if !lock_error_is_busy(&error) {
                        push_reclaim_detail(
                            &mut report,
                            format!("coordinator lock failed: {error}"),
                        );
                    }
                    continue;
                }
                let removal = remove_empty_unmarked_directory(
                    &coordinator.dir,
                    name,
                    candidate_identity,
                    cleanup_started,
                    JANITOR_CLEANUP_BUDGET,
                );
                let unlock = FileExt::unlock(&coordinator.file);
                drop(local_guard);
                match (removal, unlock) {
                    (Ok(()), Ok(())) => report.reclaimed_roots += 1,
                    (Err(error), _) | (_, Err(error)) => {
                        report.preserved_roots += 1;
                        push_reclaim_detail(
                            &mut report,
                            format!("empty unmarked-root cleanup failed for {name}: {error}"),
                        );
                    }
                }
                continue;
            }
            let lease = match lease_metadata {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    let Ok(lease) = open_regular_file_nofollow(&candidate, LEASE_FILE) else {
                        report.preserved_roots += 1;
                        continue;
                    };
                    lease
                }
                Ok(_) | Err(_) => {
                    report.preserved_roots += 1;
                    continue;
                }
            };
            if let Err(error) = FileExt::try_lock_exclusive(&lease) {
                report.preserved_roots += 1;
                if !lock_error_is_busy(&error) {
                    push_reclaim_detail(&mut report, format!("lease lock failed: {error}"));
                }
                continue;
            }
            let Some(marker) = read_marker_file(&lease) else {
                report.preserved_roots += 1;
                continue;
            };
            if marker.schema != LEASE_SCHEMA || marker.run_id != run_id {
                report.preserved_roots += 1;
                continue;
            }
            if (!deleting && candidate.symlink_metadata(RETAIN_FILE).is_ok())
                || (staging && !staging_contents_are_safe(&candidate))
                || if staging {
                    !staging_candidate_is_stale(&candidate, &marker, now)
                } else {
                    !(deleting && deleting_has_only_lease(&candidate)
                        || candidate_is_reclaimable(&candidate, &marker, &lease, now))
                }
            {
                report.preserved_roots += 1;
                push_reclaim_detail(
                    &mut report,
                    format!("managed root is not yet reclaimable: {name}"),
                );
                continue;
            }
            let local_guard = match coordinator.local_lock.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    report.preserved_roots += 1;
                    continue;
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    report.preserved_roots += 1;
                    push_reclaim_detail(
                        &mut report,
                        "coordinator process lock is poisoned".to_owned(),
                    );
                    continue;
                }
            };
            if let Err(error) = FileExt::try_lock_exclusive(&coordinator.file) {
                report.preserved_roots += 1;
                if !lock_error_is_busy(&error) {
                    push_reclaim_detail(&mut report, format!("coordinator lock failed: {error}"));
                }
                continue;
            }
            let deleting_name = format!("{DELETING_PREFIX}{run_id}");
            let claim = claim_managed_child_with_guard(
                &coordinator.dir,
                name,
                &deleting_name,
                run_id,
                marker.owner,
                &candidate,
                &lease,
                &|| {
                    if cleanup_started.elapsed() < JANITOR_CLEANUP_BUDGET {
                        Ok(())
                    } else {
                        Err(WorkspaceError::io(
                            "claim abandoned workspace",
                            Utf8Path::new(name),
                            std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "janitor cleanup deadline exceeded before claim",
                            ),
                        ))
                    }
                },
            );
            let unlock = FileExt::unlock(&coordinator.file);
            drop(local_guard);
            if let Err(error) = unlock {
                report.preserved_roots += 1;
                push_reclaim_detail(&mut report, format!("coordinator unlock failed: {error}"));
                continue;
            }
            if claim != Ok(ClaimResult::Claimed) {
                report.preserved_roots += 1;
                continue;
            }
            record_or_queue_reclaim_removal(
                &mut report,
                &mut pending_cleanup,
                coordinator,
                &deleting_name,
                candidate_identity,
                lease,
                cleanup_started,
            );
        }
        while !pending_cleanup.is_empty() && cleanup_started.elapsed() < JANITOR_CLEANUP_BUDGET {
            let round_len = pending_cleanup.len();
            let mut round_removed = 0_u64;
            for _ in 0..round_len {
                if cleanup_started.elapsed() >= JANITOR_CLEANUP_BUDGET {
                    break;
                }
                let Some(pending) = pending_cleanup.pop_front() else {
                    break;
                };
                match reclaim_removal(
                    coordinator,
                    &pending.deleting_name,
                    pending.expected_identity,
                    cleanup_started,
                ) {
                    ReclaimRemoval::Complete => {
                        report.reclaimed_roots += 1;
                    }
                    ReclaimRemoval::Deferred { removed } => {
                        round_removed = round_removed.saturating_add(removed);
                        pending_cleanup.push_back(pending);
                    }
                    ReclaimRemoval::Failed(error) => {
                        report.preserved_roots += 1;
                        push_reclaim_detail(
                            &mut report,
                            format!("cleanup failed for {}: {error}", pending.deleting_name),
                        );
                    }
                }
            }
            if round_removed == 0 {
                break;
            }
        }
        for pending in pending_cleanup {
            report.preserved_roots += 1;
            push_reclaim_detail(
                &mut report,
                format!("cleanup deferred for {}", pending.deleting_name),
            );
        }
        report
    }
}

fn push_reclaim_detail(report: &mut ReclaimReport, detail: String) {
    if report.details.len() >= MAX_DIAGNOSTIC_DETAILS {
        report.omitted_detail_count = report.omitted_detail_count.saturating_add(1);
        return;
    }
    let (detail, truncated) = truncate_diagnostic_detail(detail);
    if truncated {
        report.truncated_detail_count = report.truncated_detail_count.saturating_add(1);
    }
    report.details.push(detail);
}

pub(crate) fn truncate_diagnostic_detail(mut detail: String) -> (String, bool) {
    if detail.len() <= MAX_DIAGNOSTIC_DETAIL_BYTES {
        return (detail, false);
    }
    let mut boundary = MAX_DIAGNOSTIC_DETAIL_BYTES;
    while !detail.is_char_boundary(boundary) {
        boundary -= 1;
    }
    detail.truncate(boundary);
    (detail, true)
}

enum ReclaimRemoval {
    Complete,
    Deferred { removed: u64 },
    Failed(std::io::Error),
}

struct PendingCleanup {
    deleting_name: String,
    expected_identity: (u64, u64),
    _lease_guard: File,
}

fn record_or_queue_reclaim_removal(
    report: &mut ReclaimReport,
    pending_cleanup: &mut VecDeque<PendingCleanup>,
    coordinator: &ManagedRootCoordinator,
    deleting_name: &str,
    expected_identity: (u64, u64),
    lease_guard: File,
    cleanup_started: std::time::Instant,
) {
    match reclaim_removal(
        coordinator,
        deleting_name,
        expected_identity,
        cleanup_started,
    ) {
        ReclaimRemoval::Complete => report.reclaimed_roots += 1,
        ReclaimRemoval::Deferred { .. } => {
            pending_cleanup.push_back(PendingCleanup {
                deleting_name: deleting_name.to_owned(),
                expected_identity,
                _lease_guard: lease_guard,
            });
        }
        ReclaimRemoval::Failed(error) => {
            report.preserved_roots += 1;
            push_reclaim_detail(
                report,
                format!("cleanup failed for {deleting_name}: {error}"),
            );
        }
    }
}

fn reclaim_removal(
    coordinator: &ManagedRootCoordinator,
    deleting_name: &str,
    expected_identity: (u64, u64),
    cleanup_started: std::time::Instant,
) -> ReclaimRemoval {
    match remove_claimed_tree_slice(
        &coordinator.dir,
        deleting_name,
        Some(expected_identity),
        JANITOR_CLEANUP_BUDGET.saturating_sub(cleanup_started.elapsed()),
    ) {
        Ok(slice) if slice.complete => match coordinator.dir.symlink_metadata(deleting_name) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ReclaimRemoval::Complete,
            Ok(_) => ReclaimRemoval::Deferred {
                removed: slice.removed,
            },
            Err(error) => ReclaimRemoval::Failed(error),
        },
        Ok(slice) => ReclaimRemoval::Deferred {
            removed: slice.removed,
        },
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            ReclaimRemoval::Deferred { removed: 0 }
        }
        Err(error) => ReclaimRemoval::Failed(error),
    }
}

fn entry_is_absent(parent: &cap_std::fs::Dir, name: &str) -> bool {
    matches!(
        parent.symlink_metadata(name),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

fn ensure_direct_child_capacity(existing_entries: usize) -> Result<(), WorkspaceError> {
    if existing_entries >= MAX_MANAGED_CHILDREN {
        return Err(WorkspaceError::OwnedWorkspaceLimit {
            planned: u64::try_from(existing_entries)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
            limit: u64::try_from(MAX_MANAGED_CHILDREN).expect("fixed child limit"),
        });
    }
    Ok(())
}

fn rollback_published_root(
    coordinator: &ManagedRootCoordinator,
    active_name: &str,
    run_id: &str,
    owner: OwnerKind,
    expected_dir: &cap_std::fs::Dir,
    expected_lease: &File,
) {
    let deleting_name = format!("{DELETING_PREFIX}{run_id}");
    let Ok(expected_root_identity) = directory_identity(expected_dir) else {
        return;
    };
    // From this point the active name is externally visible. Publish immediate-reclaim evidence
    // before attempting any serialized claim, so lock contention or a later rollback failure can
    // never degrade into a young unlocked root that is preserved for 24 hours.
    let published_path = coordinator.path.join(active_name);
    let _ =
        write_cleanup_ready_marker(expected_dir, &published_path, run_id, owner, expected_lease);
    let Ok(guard) = CoordinatorLockGuard::acquire_until(
        coordinator,
        std::time::Instant::now() + JANITOR_SELECTION_BUDGET,
    ) else {
        return;
    };
    let claim = claim_managed_child(
        &coordinator.dir,
        active_name,
        &deleting_name,
        run_id,
        owner,
        expected_dir,
        expected_lease,
    );
    let unlock = guard.unlock(&coordinator.path);
    if claim == Ok(ClaimResult::Claimed) && unlock.is_ok() {
        let _ = remove_claimed_tree_bounded(
            &coordinator.dir,
            &deleting_name,
            Some(expected_root_identity),
            OWNER_CLEANUP_BUDGET,
        );
    }
}

fn write_cleanup_ready_marker(
    dir: &cap_std::fs::Dir,
    path: &Utf8Path,
    run_id: &str,
    owner: OwnerKind,
    lease: &File,
) -> Result<(), WorkspaceError> {
    let (lease_device, lease_inode) = file_identity(lease)
        .map_err(|error| WorkspaceError::io("inspect workspace lease", path, error))?;
    let marker = CleanupReadyMarker {
        schema: LEASE_SCHEMA,
        run_id: run_id.to_owned(),
        owner,
        lease_device,
        lease_inode,
    };
    let marker_path = path.join(CLEANUP_READY_FILE);
    if let Some(existing) = read_cleanup_ready_marker(dir)
        && existing.schema == marker.schema
        && existing.run_id == marker.run_id
        && existing.owner == marker.owner
        && existing.lease_device == marker.lease_device
        && existing.lease_inode == marker.lease_inode
    {
        return Ok(());
    }
    match dir.symlink_metadata(CLEANUP_READY_FILE) {
        Ok(_) => {
            let existing =
                open_regular_file_nofollow(dir, CLEANUP_READY_FILE).map_err(|error| {
                    WorkspaceError::io("open partial workspace marker", &marker_path, error)
                })?;
            let identity = file_identity(&existing).map_err(|error| {
                WorkspaceError::io("identify partial workspace marker", &marker_path, error)
            })?;
            remove_cleanup_entry_checked(
                dir,
                std::ffi::OsStr::new(CLEANUP_READY_FILE),
                &marker_path,
                identity,
                std::time::Instant::now(),
                OWNER_CLEANUP_BUDGET,
            )
            .map_err(|error| {
                WorkspaceError::io("remove partial workspace marker", &marker_path, error)
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(WorkspaceError::io(
                "open partial workspace marker",
                &marker_path,
                error,
            ));
        }
    }
    let mut options = cap_std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    configure_no_follow(&mut options);
    let mut file = dir
        .open_with(CLEANUP_READY_FILE, &options)
        .map_err(|error| WorkspaceError::io("create workspace marker", &marker_path, error))?;
    serde_json::to_writer(&mut file, &marker)
        .map_err(|error| WorkspaceError::io("write workspace marker", &marker_path, error))?;
    file.write_all(b"\n")
        .map_err(|error| WorkspaceError::io("write workspace marker", &marker_path, error))?;
    file.sync_all()
        .map_err(|error| WorkspaceError::io("flush workspace marker", &marker_path, error))
}

fn select_reclaim_candidates(
    coordinator: &ManagedRootCoordinator,
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    let guard = CoordinatorLockGuard::acquire(coordinator)?;
    let selection = select_reclaim_candidates_locked(coordinator);
    let unlock = guard.unlock(&coordinator.path);
    match (selection, unlock) {
        (Ok(selection), Ok(())) => Ok(selection),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

fn select_reclaim_candidates_locked(
    coordinator: &ManagedRootCoordinator,
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    let coordinator_path = coordinator.path.join(COORDINATOR_FILE);
    let (active_slot, state) = read_coordinator_state(&coordinator.file, &coordinator_path)?;
    let started = std::time::Instant::now();
    let entries = owned_directory_entries(&coordinator.dir)
        .map_err(|error| WorkspaceError::io("enumerate managed roots", &coordinator.path, error))?;
    let mut examined = 0_usize;
    let mut after = BTreeSet::new();
    let mut wrapped = BTreeSet::new();
    for entry in entries {
        if started.elapsed() >= JANITOR_SELECTION_BUDGET {
            return Err(WorkspaceError::io(
                "select abandoned workspaces",
                &coordinator.path,
                "selection deadline exceeded",
            ));
        }
        let entry = entry.map_err(|error| {
            WorkspaceError::io("enumerate managed roots", &coordinator.path, error)
        })?;
        examined = examined
            .checked_add(1)
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        if examined > MAX_MANAGED_CHILDREN {
            return Err(WorkspaceError::io(
                "select abandoned workspaces",
                &coordinator.path,
                "managed child limit exceeded",
            ));
        }
        let Some(name) = entry.to_str().map(str::to_owned) else {
            continue;
        };
        if !(name.starts_with(ACTIVE_PREFIX)
            || name.starts_with(DELETING_PREFIX)
            || name.starts_with(STAGING_PREFIX))
            || !valid_managed_name(&name)
        {
            continue;
        }
        if name.as_str() > state.cursor.as_str() {
            insert_bounded_name(&mut after, name);
        } else {
            insert_bounded_name(&mut wrapped, name);
        }
    }
    let candidates = if after.is_empty() { wrapped } else { after }
        .into_iter()
        .collect::<Vec<_>>();
    let cursor_error = candidates.last().and_then(|cursor| {
        persist_coordinator_cursor(
            &coordinator.file,
            &coordinator_path,
            active_slot,
            &state,
            cursor,
        )
        .err()
        .map(|error| error.to_string())
    });
    Ok((candidates, cursor_error))
}

fn insert_bounded_name(names: &mut BTreeSet<String>, name: String) {
    names.insert(name);
    if names.len() > MAX_RECLAIM_CANDIDATES {
        names.pop_last();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClaimResult {
    Claimed,
    Absent,
}

fn remove_empty_unmarked_directory(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    ensure_cleanup_deadline(started, budget)?;
    let candidate = open_owned_directory(parent, name)?;
    ensure_cleanup_deadline(started, budget)?;
    if directory_identity(&candidate)? != expected_identity || !staging_is_empty(&candidate) {
        return Err(std::io::Error::other(
            "empty deleting root changed before removal",
        ));
    }
    ensure_cleanup_deadline(started, budget)?;
    remove_cleanup_directory_checked(
        parent,
        std::ffi::OsStr::new(name),
        Utf8Path::new(name),
        expected_identity,
        started,
        budget,
    )
}

fn claim_managed_child(
    parent: &cap_std::fs::Dir,
    current_name: &str,
    deleting_name: &str,
    run_id: &str,
    owner: OwnerKind,
    expected_dir: &cap_std::fs::Dir,
    expected_lease: &File,
) -> Result<ClaimResult, WorkspaceError> {
    claim_managed_child_with_guard(
        parent,
        current_name,
        deleting_name,
        run_id,
        owner,
        expected_dir,
        expected_lease,
        &|| Ok(()),
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "claim keeps expected directory, lease, marker, and deadline evidence adjacent"
)]
fn claim_managed_child_with_guard(
    parent: &cap_std::fs::Dir,
    current_name: &str,
    deleting_name: &str,
    run_id: &str,
    owner: OwnerKind,
    expected_dir: &cap_std::fs::Dir,
    expected_lease: &File,
    before_next_operation: &impl Fn() -> Result<(), WorkspaceError>,
) -> Result<ClaimResult, WorkspaceError> {
    before_next_operation()?;
    let (candidate, claimed_name) = match open_owned_directory(parent, current_name) {
        Ok(candidate) => (candidate, current_name),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if current_name == deleting_name {
                return Ok(ClaimResult::Absent);
            }
            match open_owned_directory(parent, deleting_name) {
                Ok(candidate) => (candidate, deleting_name),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(ClaimResult::Absent);
                }
                Err(error) => {
                    return Err(WorkspaceError::io(
                        "open cleanup candidate",
                        Utf8Path::new(deleting_name),
                        error,
                    ));
                }
            }
        }
        Err(error) => {
            return Err(WorkspaceError::io(
                "open cleanup candidate",
                Utf8Path::new(current_name),
                error,
            ));
        }
    };
    before_next_operation()?;
    let candidate_identity = directory_identity(&candidate).map_err(|error| {
        WorkspaceError::io(
            "inspect cleanup candidate identity",
            Utf8Path::new(claimed_name),
            error,
        )
    })?;
    let expected_identity = directory_identity(expected_dir).map_err(|error| {
        WorkspaceError::io(
            "inspect expected cleanup identity",
            Utf8Path::new(current_name),
            error,
        )
    })?;
    if candidate_identity != expected_identity {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(claimed_name),
        });
    }
    before_next_operation()?;
    let candidate_lease = open_regular_file_nofollow(&candidate, LEASE_FILE)?;
    let candidate_lease_identity = file_identity(&candidate_lease).map_err(|error| {
        WorkspaceError::io(
            "inspect cleanup lease identity",
            Utf8Path::new(claimed_name),
            error,
        )
    })?;
    let expected_lease_identity = file_identity(expected_lease).map_err(|error| {
        WorkspaceError::io(
            "inspect expected lease identity",
            Utf8Path::new(current_name),
            error,
        )
    })?;
    if candidate_lease_identity != expected_lease_identity {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(claimed_name),
        });
    }
    before_next_operation()?;
    let marker = read_marker_file(&candidate_lease).ok_or_else(|| WorkspaceError::InvalidPath {
        path: Utf8PathBuf::from(claimed_name),
    })?;
    if marker.schema != LEASE_SCHEMA || marker.run_id != run_id || marker.owner != owner {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(current_name),
        });
    }
    before_next_operation()?;
    if claimed_name != deleting_name {
        before_next_operation()?;
        rename_owned_directory_with_guard(
            parent,
            claimed_name,
            deleting_name,
            candidate_identity,
            before_next_operation,
        )
        .map_err(|error| {
            WorkspaceError::io(
                "claim managed workspace",
                Utf8Path::new(claimed_name),
                error,
            )
        })?;
    }
    Ok(ClaimResult::Claimed)
}

fn candidate_is_stale(candidate: &cap_std::fs::Dir, run_id: &str, now: SystemTime) -> bool {
    let Ok(lease) = open_regular_file_nofollow(candidate, LEASE_FILE) else {
        return false;
    };
    let Some(lease_marker) = read_json_file::<LeaseMarker>(&lease) else {
        return false;
    };
    let Ok(heartbeat) = open_regular_file_nofollow(candidate, HEARTBEAT_FILE) else {
        return false;
    };
    let Some(heartbeat_marker) = read_json_file::<LeaseMarker>(&heartbeat) else {
        return false;
    };
    if lease_marker.schema != LEASE_SCHEMA
        || lease_marker.run_id != run_id
        || heartbeat_marker.schema != LEASE_SCHEMA
        || heartbeat_marker.run_id != run_id
        || heartbeat_marker.owner != lease_marker.owner
    {
        return false;
    }
    let Ok(metadata) = heartbeat.metadata() else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    modified
        .checked_add(STALE_AFTER)
        .is_some_and(|stale_at| now >= stale_at)
}

fn staging_candidate_is_stale(
    candidate: &cap_std::fs::Dir,
    marker: &LeaseMarker,
    now: SystemTime,
) -> bool {
    match candidate.symlink_metadata(HEARTBEAT_FILE) {
        Ok(_) => candidate_is_stale(candidate, &marker.run_id, now),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => UNIX_EPOCH
            .checked_add(Duration::from_secs(marker.created_unix_seconds))
            .and_then(|created| created.checked_add(STALE_AFTER))
            .is_some_and(|stale_at| now >= stale_at),
        Err(_) => false,
    }
}

fn directory_timestamp_is_stale(candidate: &cap_std::fs::Dir, now: SystemTime) -> bool {
    candidate
        .metadata(".")
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.into_std().checked_add(STALE_AFTER))
        .is_some_and(|stale_at| now >= stale_at)
}

fn candidate_is_reclaimable(
    candidate: &cap_std::fs::Dir,
    marker: &LeaseMarker,
    lease: &File,
    now: SystemTime,
) -> bool {
    if let Some(ready) = read_cleanup_ready_marker(candidate)
        && let Ok((lease_device, lease_inode)) = file_identity(lease)
        && ready.schema == LEASE_SCHEMA
        && ready.run_id == marker.run_id
        && ready.owner == marker.owner
        && ready.lease_device == lease_device
        && ready.lease_inode == lease_inode
    {
        return true;
    }
    candidate_is_stale(candidate, &marker.run_id, now)
}

fn staging_contents_are_safe(candidate: &cap_std::fs::Dir) -> bool {
    let Ok(entries) = owned_directory_entries(candidate) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return false;
        };
        let Some(name) = entry.to_str().map(str::to_owned) else {
            return false;
        };
        if name != LEASE_FILE && name != HEARTBEAT_FILE {
            return false;
        }
    }
    true
}

fn staging_is_empty(candidate: &cap_std::fs::Dir) -> bool {
    owned_directory_entries(candidate).is_ok_and(|mut entries| entries.next().is_none())
}

fn deleting_has_only_lease(candidate: &cap_std::fs::Dir) -> bool {
    let Ok(mut entries) = owned_directory_entries(candidate) else {
        return false;
    };
    entries
        .next()
        .is_some_and(|entry| entry.is_ok_and(|name| name == LEASE_FILE) && entries.next().is_none())
}

fn read_cleanup_ready_marker(candidate: &cap_std::fs::Dir) -> Option<CleanupReadyMarker> {
    read_bounded_json(candidate, CLEANUP_READY_FILE)
}

fn read_marker_file(file: &File) -> Option<LeaseMarker> {
    read_json_file(file)
}

fn read_json_file<T: serde::de::DeserializeOwned>(file: &File) -> Option<T> {
    let mut file = file.try_clone().ok()?;
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_MARKER_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).ok()? > MAX_MARKER_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

#[cfg(unix)]
fn open_regular_file_nofollow(dir: &cap_std::fs::Dir, name: &str) -> Result<File, WorkspaceError> {
    let expected = dir
        .symlink_metadata(name)
        .map_err(|error| WorkspaceError::io("inspect cleanup lease", Utf8Path::new(name), error))?;
    if !expected.is_file() || expected.file_type().is_symlink() {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(name),
        });
    }
    let expected_identity = metadata_identity(&expected);
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    configure_no_follow(&mut options);
    let file = dir
        .open_with(name, &options)
        .map_err(|error| WorkspaceError::io("open cleanup lease", Utf8Path::new(name), error))?
        .into_std();
    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect cleanup lease", Utf8Path::new(name), error))?;
    if !metadata.is_file()
        || file_identity(&file).map_err(|error| {
            WorkspaceError::io("identify cleanup lease", Utf8Path::new(name), error)
        })? != expected_identity
    {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(name),
        });
    }
    verify_current_user_owned_file(&file).map_err(|error| {
        WorkspaceError::io("verify cleanup lease owner", Utf8Path::new(name), error)
    })?;
    Ok(file)
}

#[cfg(windows)]
fn open_regular_file_nofollow(dir: &cap_std::fs::Dir, name: &str) -> Result<File, WorkspaceError> {
    let file = super::root::windows::open_regular_file_shared(dir, std::ffi::OsStr::new(name))
        .map_err(|error| WorkspaceError::io("open cleanup lease", Utf8Path::new(name), error))?;
    verify_current_user_owned_file(&file).map_err(|error| {
        WorkspaceError::io("verify cleanup lease owner", Utf8Path::new(name), error)
    })?;
    Ok(file)
}

#[cfg(unix)]
fn verify_current_user_owned_file(file: &File) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;

    let current_uid = unsafe { libc::geteuid() };
    if file.metadata()?.uid() == current_uid {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "managed object is not owned by the current user",
        ))
    }
}

#[cfg(windows)]
fn verify_current_user_owned_file(file: &File) -> std::io::Result<()> {
    windows::verify_current_user_owner(file)
}

#[cfg(unix)]
fn directory_identity(dir: &cap_std::fs::Dir) -> std::io::Result<(u64, u64)> {
    use cap_fs_ext::MetadataExt;

    dir.metadata(".")
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn directory_identity(dir: &cap_std::fs::Dir) -> std::io::Result<(u64, u64)> {
    super::root::windows::file_identity_io(dir)
}

fn read_bounded_json<T: serde::de::DeserializeOwned>(
    candidate: &cap_std::fs::Dir,
    name: &str,
) -> Option<T> {
    let file = open_regular_file_nofollow(candidate, name).ok()?;
    read_json_file(&file)
}

#[cfg(unix)]
fn file_identity(file: &File) -> std::io::Result<(u64, u64)> {
    use cap_fs_ext::MetadataExt;

    file.metadata()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(file: &File) -> std::io::Result<(u64, u64)> {
    windows::file_identity(file)
}

#[cfg(unix)]
fn refresh_file_modified_time(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;

    let times = [
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_NOW,
        },
    ];
    // SAFETY: file is open and times points to two initialized timespec values.
    if unsafe { libc::futimens(file.as_raw_fd(), times.as_ptr()) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn refresh_file_modified_time(file: &File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Storage::FileSystem::SetFileTime;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

    let mut now = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: now is a valid writable FILETIME.
    unsafe { GetSystemTimeAsFileTime(&raw mut now) };
    // SAFETY: file is open and the modification-time pointer is valid for this call.
    if unsafe {
        SetFileTime(
            file.as_raw_handle(),
            std::ptr::null(),
            std::ptr::null(),
            &raw const now,
        )
    } != 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[derive(Clone, Copy, Debug)]
struct RemovalSlice {
    complete: bool,
    examined: u64,
    removed: u64,
}

#[derive(Clone, Copy)]
enum CleanupEntry {
    Directory((u64, u64)),
    Other((u64, u64)),
}

#[cfg(unix)]
fn inspect_cleanup_entry(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
) -> std::io::Result<CleanupEntry> {
    let metadata = parent.symlink_metadata(name)?;
    verify_current_user_owned_metadata(&metadata)?;
    let identity = metadata_identity(&metadata);
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(CleanupEntry::Directory(identity))
    } else {
        Ok(CleanupEntry::Other(identity))
    }
}

#[cfg(windows)]
fn inspect_cleanup_entry(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
) -> std::io::Result<CleanupEntry> {
    use std::os::windows::fs::MetadataExt;

    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let entry = super::root::windows::open_entry_shared(parent, name)?;
    windows::verify_current_user_owner(&entry)?;
    let metadata = entry.metadata()?;
    let identity = super::root::windows::file_identity_io(&entry)?;
    if metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        Ok(CleanupEntry::Directory(identity))
    } else {
        Ok(CleanupEntry::Other(identity))
    }
}

fn remove_claimed_tree_slice(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_root_identity: Option<(u64, u64)>,
    budget: Duration,
) -> std::io::Result<RemovalSlice> {
    let started = std::time::Instant::now();
    let slice_budget = budget.min(MAX_CLEANUP_SLICE_DURATION);
    let mut examined = 0_u64;
    let mut removed = 0_u64;
    loop {
        if examined >= u64::try_from(MAX_CLEANUP_SLICE_ENTRIES).expect("fixed limit")
            || started.elapsed() >= slice_budget
        {
            if removed == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "cleanup slice budget exhausted before progress",
                ));
            }
            return Ok(RemovalSlice {
                complete: false,
                examined,
                removed,
            });
        }
        let progress = match remove_one_claimed_entry(
            parent,
            name,
            expected_root_identity,
            u64::try_from(MAX_CLEANUP_SLICE_ENTRIES).expect("fixed limit") - examined,
            started,
            slice_budget,
        ) {
            Ok(progress) => progress,
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut && removed != 0 => {
                return Ok(RemovalSlice {
                    complete: false,
                    examined,
                    removed,
                });
            }
            Err(error) => return Err(error),
        };
        examined = examined
            .checked_add(progress.examined)
            .ok_or_else(|| std::io::Error::other("cleanup entry count overflow"))?;
        removed = removed
            .checked_add(progress.removed)
            .ok_or_else(|| std::io::Error::other("cleanup removal count overflow"))?;
        if progress.complete {
            return Ok(RemovalSlice {
                complete: true,
                examined,
                removed,
            });
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "single-step deletion keeps every identity and cooperative-bound check adjacent"
)]
fn remove_one_claimed_entry(
    parent: &cap_std::fs::Dir,
    root_name: &str,
    expected_root_identity: Option<(u64, u64)>,
    max_examined: u64,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<RemovalSlice> {
    let mut current = match open_cleanup_directory(
        parent,
        std::ffi::OsStr::new(root_name),
        expected_root_identity,
        started,
        budget,
    ) {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RemovalSlice {
                complete: true,
                examined: 0,
                removed: 0,
            });
        }
        Err(error) => return Err(error),
    };
    #[cfg(target_os = "macos")]
    ensure_same_cleanup_mount(parent, &current, started, budget)?;
    let root_identity = directory_identity(&current)?;
    let mut components: Vec<(std::ffi::OsString, (u64, u64))> = Vec::new();
    let mut cursor_bytes = 0_usize;
    let mut examined = 0_u64;
    loop {
        if cleanup_deadline_reached(started, budget) || examined >= max_examined {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "cleanup slice cooperative limit reached",
            ));
        }
        make_cleanup_directory_accessible(&current, started, budget)?;
        ensure_cleanup_deadline(started, budget)?;
        let mut entries = owned_directory_entries(&current)?;
        ensure_cleanup_deadline(started, budget)?;
        let entry = loop {
            if examined >= max_examined {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "cleanup slice entry limit reached while preserving control markers",
                ));
            }
            match entries.next() {
                Some(Ok(entry)) => {
                    examined = examined
                        .checked_add(1)
                        .ok_or_else(|| std::io::Error::other("cleanup entry count overflow"))?;
                    if components.is_empty() && is_root_control_marker(&entry) {
                        ensure_cleanup_deadline(started, budget)?;
                        continue;
                    }
                    break Some(entry);
                }
                Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    ensure_cleanup_deadline(started, budget)?;
                }
                Some(Err(error)) => return Err(error),
                None => break None,
            }
        };
        drop(entries);
        ensure_cleanup_deadline(started, budget)?;
        let Some(entry) = entry else {
            let Some((leaf_name, leaf_identity)) = components.pop() else {
                let marker_progress = remove_root_control_markers(&current, started, budget)?;
                if marker_progress.deferred {
                    if marker_progress.removed == 0 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "cleanup deadline reached before final marker removal",
                        ));
                    }
                    return Ok(RemovalSlice {
                        complete: false,
                        examined,
                        removed: marker_progress.removed,
                    });
                }
                if cleanup_deadline_reached(started, budget) {
                    return Ok(RemovalSlice {
                        complete: false,
                        examined,
                        removed: marker_progress.removed,
                    });
                }
                drop(current);
                remove_cleanup_directory_checked(
                    parent,
                    std::ffi::OsStr::new(root_name),
                    Utf8Path::new(root_name),
                    root_identity,
                    started,
                    budget,
                )?;
                return Ok(RemovalSlice {
                    complete: true,
                    examined,
                    removed: marker_progress.removed.saturating_add(1),
                });
            };
            drop(current);
            let containing = reopen_cleanup_parent(
                parent,
                root_name,
                root_identity,
                &components,
                started,
                budget,
            )?;
            remove_cleanup_directory_checked(
                &containing,
                &leaf_name,
                Utf8Path::new(root_name),
                leaf_identity,
                started,
                budget,
            )?;
            return Ok(RemovalSlice {
                complete: false,
                examined,
                removed: 1,
            });
        };
        let child_name = entry;
        let entry = match inspect_cleanup_entry(&current, &child_name) {
            Ok(entry) => entry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        ensure_cleanup_deadline(started, budget)?;
        if let CleanupEntry::Directory(identity) = entry {
            if identity.0 != root_identity.0 {
                return Err(std::io::Error::other(
                    "cleanup refuses to cross a filesystem boundary",
                ));
            }
            let depth = components
                .len()
                .checked_add(1)
                .ok_or_else(|| std::io::Error::other("cleanup directory depth overflow"))?;
            cursor_bytes =
                advance_cleanup_cursor(depth, cursor_bytes, cleanup_component_bytes(&child_name))?;
            let child =
                open_cleanup_directory(&current, &child_name, Some(identity), started, budget)?;
            ensure_cleanup_deadline(started, budget)?;
            #[cfg(target_os = "macos")]
            ensure_same_cleanup_mount(&current, &child, started, budget)?;
            components.push((child_name, identity));
            drop(current);
            current = child;
        } else {
            let CleanupEntry::Other(identity) = entry else {
                unreachable!("directory branch was handled above")
            };
            remove_cleanup_entry_checked(
                &current,
                &child_name,
                Utf8Path::new(root_name),
                identity,
                started,
                budget,
            )?;
            return Ok(RemovalSlice {
                complete: false,
                examined,
                removed: 1,
            });
        }
    }
}

fn is_root_control_marker(name: &std::ffi::OsStr) -> bool {
    name == LEASE_FILE
        || name == HEARTBEAT_FILE
        || name == CLEANUP_READY_FILE
        || name == RETAIN_FILE
}

fn remove_root_control_markers(
    root: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<ControlMarkerProgress> {
    let mut removed = 0_u64;
    for marker in [RETAIN_FILE, HEARTBEAT_FILE, CLEANUP_READY_FILE, LEASE_FILE] {
        if cleanup_deadline_reached(started, budget) {
            return Ok(ControlMarkerProgress {
                removed,
                deferred: true,
            });
        }
        let marker_name = std::ffi::OsStr::new(marker);
        let marker_identity = match cleanup_entry_identity_if_owned(root, marker_name) {
            Ok(identity) => identity,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        match remove_cleanup_entry_checked(
            root,
            marker_name,
            Utf8Path::new(marker),
            marker_identity,
            started,
            budget,
        ) {
            Ok(()) => removed = removed.saturating_add(1),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(ControlMarkerProgress {
        removed,
        deferred: false,
    })
}

#[cfg(unix)]
fn cleanup_entry_identity_if_owned(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
) -> std::io::Result<(u64, u64)> {
    let metadata = parent.symlink_metadata(name)?;
    verify_current_user_owned_metadata(&metadata)?;
    Ok(metadata_identity(&metadata))
}

#[cfg(windows)]
fn cleanup_entry_identity_if_owned(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
) -> std::io::Result<(u64, u64)> {
    let entry = super::root::windows::open_entry_shared(parent, name)?;
    windows::verify_current_user_owner(&entry)?;
    super::root::windows::file_identity_io(&entry)
}

struct ControlMarkerProgress {
    removed: u64,
    deferred: bool,
}

fn advance_cleanup_cursor(
    depth: usize,
    cursor_bytes: usize,
    component_bytes: usize,
) -> std::io::Result<usize> {
    if depth > MAX_CLEANUP_DEPTH {
        return Err(std::io::Error::other(
            "cleanup directory depth exceeds 4096",
        ));
    }
    let cursor_bytes = cursor_bytes
        .checked_add(component_bytes)
        .and_then(|bytes| bytes.checked_add(1))
        .ok_or_else(|| std::io::Error::other("cleanup cursor size overflow"))?;
    if cursor_bytes > 64 * 1024 {
        return Err(std::io::Error::other("cleanup cursor exceeds 65536 bytes"));
    }
    Ok(cursor_bytes)
}

fn cleanup_deadline_reached(started: std::time::Instant, budget: Duration) -> bool {
    started.elapsed() >= budget
}

fn ensure_cleanup_deadline(started: std::time::Instant, budget: Duration) -> std::io::Result<()> {
    if cleanup_deadline_reached(started, budget) {
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "cleanup slice cooperative deadline reached",
        ))
    } else {
        Ok(())
    }
}

fn reopen_cleanup_parent(
    parent: &cap_std::fs::Dir,
    root_name: &str,
    root_identity: (u64, u64),
    components: &[(std::ffi::OsString, (u64, u64))],
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<cap_std::fs::Dir> {
    let mut current = open_cleanup_directory(
        parent,
        std::ffi::OsStr::new(root_name),
        Some(root_identity),
        started,
        budget,
    )?;
    ensure_cleanup_deadline(started, budget)?;
    #[cfg(target_os = "macos")]
    ensure_same_cleanup_mount(parent, &current, started, budget)?;
    for (name, identity) in components {
        ensure_cleanup_deadline(started, budget)?;
        let child = open_cleanup_directory(&current, name, Some(*identity), started, budget)?;
        ensure_cleanup_deadline(started, budget)?;
        #[cfg(target_os = "macos")]
        ensure_same_cleanup_mount(&current, &child, started, budget)?;
        drop(current);
        current = child;
    }
    Ok(current)
}

#[cfg(unix)]
fn open_cleanup_directory(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    expected_identity: Option<(u64, u64)>,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<cap_std::fs::Dir> {
    ensure_cleanup_deadline(started, budget)?;
    let metadata = parent.symlink_metadata(name)?;
    ensure_cleanup_deadline(started, budget)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::other(
            "cleanup entry is not a real directory",
        ));
    }
    let identity = metadata_identity(&metadata);
    if expected_identity.is_some_and(|expected| expected != identity) {
        return Err(std::io::Error::other(
            "cleanup directory identity changed before reopening",
        ));
    }
    ensure_cleanup_deadline(started, budget)?;
    let child = match open_owned_directory(parent, name) {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            ensure_cleanup_deadline(started, budget)?;
            make_cleanup_child_accessible(parent, name, &metadata, started, budget)?;
            ensure_cleanup_deadline(started, budget)?;
            let verified = parent.symlink_metadata(name)?;
            ensure_cleanup_deadline(started, budget)?;
            if metadata_identity(&verified) != identity
                || !verified.is_dir()
                || verified.file_type().is_symlink()
            {
                return Err(std::io::Error::other(
                    "cleanup directory identity changed while repairing permissions",
                ));
            }
            ensure_cleanup_deadline(started, budget)?;
            let reopened = open_owned_directory(parent, name)?;
            ensure_cleanup_deadline(started, budget)?;
            reopened
        }
        Err(error) => return Err(error),
    };
    ensure_cleanup_deadline(started, budget)?;
    if directory_identity(&child)? != identity {
        return Err(std::io::Error::other(
            "cleanup directory identity changed while opening",
        ));
    }
    ensure_cleanup_deadline(started, budget)?;
    Ok(child)
}

#[cfg(windows)]
fn open_cleanup_directory(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    expected_identity: Option<(u64, u64)>,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<cap_std::fs::Dir> {
    ensure_cleanup_deadline(started, budget)?;
    ensure_cleanup_deadline(started, budget)?;
    let child = super::root::windows::open_directory_shared(parent, name)?;
    ensure_cleanup_deadline(started, budget)?;
    let identity = super::root::windows::file_identity_io(&child)?;
    if expected_identity.is_some_and(|expected| expected != identity) {
        return Err(std::io::Error::other(
            "cleanup directory identity changed while opening",
        ));
    }
    ensure_cleanup_deadline(started, budget)?;
    Ok(cap_std::fs::Dir::from_std_file(child))
}

#[cfg(unix)]
fn make_cleanup_child_accessible(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    metadata: &cap_std::fs::Metadata,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    use cap_std::fs::PermissionsExt;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    ensure_cleanup_deadline(started, budget)?;
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| std::io::Error::other("cleanup name contains NUL"))?;
    ensure_cleanup_deadline(started, budget)?;
    let parent = parent.try_clone()?.into_std_file();
    ensure_cleanup_deadline(started, budget)?;
    let mode = libc::mode_t::try_from(metadata.permissions().mode() | 0o700)
        .map_err(|_| std::io::Error::other("cleanup mode conversion overflow"))?;
    // SAFETY: parent and name remain valid for the call; AT_SYMLINK_NOFOLLOW prevents escape.
    ensure_cleanup_deadline(started, budget)?;
    if unsafe {
        libc::fchmodat(
            parent.as_raw_fd(),
            name.as_ptr(),
            mode,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn make_cleanup_directory_accessible(
    dir: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure_cleanup_deadline(started, budget)?;
    let file = dir.try_clone()?.into_std_file();
    ensure_cleanup_deadline(started, budget)?;
    let metadata = file.metadata()?;
    ensure_cleanup_deadline(started, budget)?;
    if metadata.permissions().mode() & 0o700 != 0o700 {
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o700);
        ensure_cleanup_deadline(started, budget)?;
        file.set_permissions(permissions)?;
        ensure_cleanup_deadline(started, budget)?;
    }
    Ok(())
}

#[cfg(windows)]
fn make_cleanup_directory_accessible(
    dir: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    ensure_cleanup_deadline(started, budget)?;
    dir.metadata(".")?;
    ensure_cleanup_deadline(started, budget)
}

#[cfg(unix)]
fn cleanup_component_bytes(name: &std::ffi::OsStr) -> usize {
    use std::os::unix::ffi::OsStrExt;

    name.as_bytes().len()
}

#[cfg(windows)]
fn cleanup_component_bytes(name: &std::ffi::OsStr) -> usize {
    use std::os::windows::ffi::OsStrExt;

    name.encode_wide()
        .count()
        .saturating_mul(std::mem::size_of::<u16>())
}

#[cfg(unix)]
fn remove_cleanup_file(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    _logical_path: &Utf8Path,
) -> std::io::Result<()> {
    parent.remove_file(name)
}

#[cfg(unix)]
fn remove_cleanup_directory(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    _logical_path: &Utf8Path,
) -> std::io::Result<()> {
    parent.remove_dir(name)
}

#[cfg(unix)]
fn remove_cleanup_entry_checked(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    ensure_cleanup_deadline(started, budget)?;
    let metadata = parent.symlink_metadata(name)?;
    if metadata_identity(&metadata) != expected_identity {
        return Err(std::io::Error::other(
            "cleanup file identity changed before removal",
        ));
    }
    ensure_cleanup_deadline(started, budget)?;
    remove_cleanup_file(parent, name, logical_path)
}

#[cfg(unix)]
fn remove_cleanup_directory_checked(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    ensure_cleanup_deadline(started, budget)?;
    let metadata = parent.symlink_metadata(name)?;
    if metadata_identity(&metadata) != expected_identity
        || !metadata.is_dir()
        || metadata.file_type().is_symlink()
    {
        return Err(std::io::Error::other(
            "cleanup directory identity changed before removal",
        ));
    }
    ensure_cleanup_deadline(started, budget)?;
    remove_cleanup_directory(parent, name, logical_path)
}

#[cfg(windows)]
fn remove_cleanup_directory_checked(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    _logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    super::root::windows::remove_entry_io_checked_with_guard(
        parent,
        name,
        expected_identity,
        &|| ensure_cleanup_deadline(started, budget),
    )
}

#[cfg(windows)]
fn remove_cleanup_entry_checked(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    _logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    super::root::windows::remove_entry_io_checked_with_guard(
        parent,
        name,
        expected_identity,
        &|| ensure_cleanup_deadline(started, budget),
    )
}

fn metadata_identity(metadata: &cap_std::fs::Metadata) -> (u64, u64) {
    use cap_fs_ext::MetadataExt;

    (metadata.dev(), metadata.ino())
}

#[cfg(target_os = "macos")]
type MacosMountIdentity = [u8; std::mem::size_of::<libc::fsid_t>()];

#[cfg(target_os = "macos")]
fn macos_directory_mount_identity(dir: &cap_std::fs::Dir) -> std::io::Result<MacosMountIdentity> {
    use std::os::fd::AsRawFd;

    // SAFETY: result points to writable storage and dir owns a live descriptor for the call.
    let result = unsafe {
        let mut result: libc::statfs = std::mem::zeroed();
        if libc::fstatfs(dir.as_raw_fd(), &raw mut result) != 0 {
            return Err(std::io::Error::last_os_error());
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

#[cfg(target_os = "macos")]
fn ensure_same_cleanup_mount(
    parent: &cap_std::fs::Dir,
    child: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: Duration,
) -> std::io::Result<()> {
    ensure_cleanup_deadline(started, budget)?;
    let parent_mount = macos_directory_mount_identity(parent)?;
    ensure_cleanup_deadline(started, budget)?;
    let child_mount = macos_directory_mount_identity(child)?;
    ensure_cleanup_deadline(started, budget)?;
    if parent_mount == child_mount {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "cleanup refuses to cross a mount boundary",
        ))
    }
}

fn remove_claimed_tree_bounded(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_root_identity: Option<(u64, u64)>,
    budget: Duration,
) -> std::io::Result<RemovalSlice> {
    let started = std::time::Instant::now();
    let mut total = RemovalSlice {
        complete: false,
        examined: 0,
        removed: 0,
    };
    while started.elapsed() < budget {
        let slice = remove_claimed_tree_slice(
            parent,
            name,
            expected_root_identity,
            budget.saturating_sub(started.elapsed()),
        )?;
        total.examined = total.examined.saturating_add(slice.examined);
        total.removed = total.removed.saturating_add(slice.removed);
        if slice.complete {
            total.complete = true;
            return Ok(total);
        }
    }
    Ok(total)
}

fn valid_child_prefix(prefix: &str) -> bool {
    prefix.ends_with('-')
        && prefix.len() > 1
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[derive(Debug)]
pub(crate) struct ManagedChild {
    path: Utf8PathBuf,
    dir: cap_std::fs::Dir,
    parent: cap_std::fs::Dir,
    name: String,
    lifecycle: Arc<Mutex<RootLifecycle>>,
    // A detached blocking workspace task may outlive its owner future. Holding the original
    // locked file keeps startup janitors out until that task drops its managed child.
    _lease: Arc<File>,
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lifecycle.live_children = lifecycle.live_children.saturating_sub(1);
    }
}

impl ManagedChild {
    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub(crate) fn cleanup(&self) -> Result<(), WorkspaceError> {
        let identity = directory_identity(&self.dir)
            .map_err(|error| WorkspaceError::io("identify managed child", &self.path, error))?;
        let result = remove_claimed_tree_bounded(
            &self.parent,
            &self.name,
            Some(identity),
            OWNER_CLEANUP_BUDGET,
        )
        .map_err(|error| WorkspaceError::io("remove managed child", &self.path, error))?;
        if !result.complete {
            return Err(WorkspaceError::io(
                "remove managed child",
                &self.path,
                "cleanup budget exhausted",
            ));
        }
        match self.parent.symlink_metadata(&self.name) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(WorkspaceError::io(
                "verify managed child removal",
                &self.path,
                error,
            )),
            Ok(_) => Err(WorkspaceError::io(
                "verify managed child removal",
                &self.path,
                "path remains after cleanup",
            )),
        }
    }
}
