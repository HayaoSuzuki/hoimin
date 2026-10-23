#[cfg(test)]
mod tests {
    use camino::Utf8Path;
    #[cfg(unix)]
    use camino::Utf8PathBuf;

    #[cfg(windows)]
    use super::{CLEANUP_READY_FILE, RETAIN_FILE};
    use super::{
        COORDINATOR_FILE, MANAGED_DIR, MAX_MANAGED_CHILDREN, ManagedRootCoordinator,
        ManagedRunRoot, OwnerKind, ensure_direct_child_capacity,
    };

    #[test]
    fn janitor_counts_active_holds_without_claiming_remaining_population() {
        // Arrange.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let roots = (0..3)
            .map(|_| ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap())
            .collect::<Vec<_>>();

        // Act.
        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        // Assert.
        assert_eq!(report.inspected_roots, 3);
        assert_eq!(report.preserved_roots, 3);
        #[cfg(unix)]
        let reason = hoimin_core::JanitorReason::ActiveLease;
        #[cfg(windows)]
        let reason = hoimin_core::JanitorReason::MutationBarrierActive;
        assert_eq!(report.preserved_by_reason[&reason], 3);
        assert_eq!(report.removed_entries, 0);
        assert!(roots.iter().all(|root| root.path().exists()));
    }

    #[cfg(unix)]
    #[test]
    fn marker_failure_preserves_acknowledged_earlier_deletion() {
        // Arrange: retain is removed before heartbeat fails as a directory.
        let parent = tempfile::tempdir().unwrap();
        let dir = cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
            .unwrap();
        dir.write(super::RETAIN_FILE, b"retained").unwrap();
        dir.create_dir(super::HEARTBEAT_FILE).unwrap();
        let mut capabilities = super::CleanupCapabilities::default();

        // Act.
        let result = super::remove_root_control_markers(
            &dir,
            std::time::Instant::now(),
            std::time::Duration::from_secs(5),
            &mut capabilities,
        );

        // Assert.
        assert!(result.is_err());
        assert!(!dir.exists(super::RETAIN_FILE));
        assert!(dir.exists(super::HEARTBEAT_FILE));
        assert_eq!(capabilities.progress.removed_entries, 1);
    }

    fn create_test_owned_directory_entry(
        parent: &cap_std::fs::Dir,
        name: &str,
    ) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            rustix::fs::mkdirat(parent, name, rustix::fs::Mode::from_raw_mode(0o700))
                .map_err(std::io::Error::from)
        }
        #[cfg(windows)]
        {
            super::windows::create_relative_managed_directory(parent, std::ffi::OsStr::new(name))
                .map(drop)
        }
    }

    fn only_test_entry_with_prefix(path: &Utf8Path, prefix: &str) -> String {
        let names = std::fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.starts_with(prefix))
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 1, "entries with prefix {prefix:?}: {names:?}");
        names.into_iter().next().unwrap()
    }

    #[cfg(unix)]
    fn assert_primary_then_cleanup_unavailable(error: &super::WorkspaceError, primary: &str) {
        let message = error.to_string();
        let primary_offset = message.find(primary).expect("primary error is retained");
        let cleanup = "secondary new-directory rollback failure: bounded cleanup cannot proceed without the created directory identity; unverified name left untouched";
        let cleanup_offset = message
            .find(cleanup)
            .expect("one cleanup-unavailable secondary is retained");
        assert!(primary_offset < cleanup_offset, "{message}");
        assert_eq!(message.matches(cleanup).count(), 1, "{message}");
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

    proptest::proptest! {
        #[test]
        fn only_deferred_contention_is_retryable_independently_of_diagnostics(detail in ".{0,4096}") {
            for (disposition, expected) in [
                (super::CleanupDisposition::Clean, false),
                (super::CleanupDisposition::Failed, false),
                (super::CleanupDisposition::Deferred(None), false),
                (super::CleanupDisposition::Deferred(Some(super::CleanupRetryReason::CoordinatorContention)), true),
            ] {
                let mut record = super::CleanupRecord { disposition, evidence: super::CleanupEvidence::default() };
                record.evidence.push_detail(detail.clone());

                let retryable = record.retryable_contention();

                proptest::prop_assert_eq!(retryable, expected);
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bootstrap_returns_hard_lock_errors_without_retrying() {
        let directory = tempfile::tempdir().unwrap();
        let path = Utf8Path::from_path(directory.path())
            .unwrap()
            .join("coordinator");
        std::fs::write(&path, b"unchanged").unwrap();
        // O_PATH owns a valid descriptor that cannot perform file locking.
        let file = std::fs::File::from(
            rustix::fs::open(
                path.as_std_path(),
                rustix::fs::OFlags::PATH,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        assert!(matches!(
            file.try_lock(),
            Err(std::fs::TryLockError::Error(_))
        ));

        let result = super::lock_file_until(
            &file,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
            &path,
        );

        assert!(
            matches!(
                result,
                Err(super::WorkspaceError::Io {
                    operation: "lock coordinator",
                    ..
                })
            ),
            "{result:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");
    }

    #[cfg(windows)]
    #[test]
    fn every_windows_protocol_object_is_owned_by_the_token_user() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        super::windows::verify_current_user_owner(&coordinator.file).unwrap();

        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let root_file = root.dir.try_clone().unwrap().into_std_file();
        super::windows::verify_current_user_owner(&root_file).unwrap();
        super::windows::verify_current_user_owner(root.lease.lock().unwrap().as_deref().unwrap())
            .unwrap();
        super::windows::verify_current_user_owner(root.heartbeat.lock().unwrap().as_ref().unwrap())
            .unwrap();

        let child = root.create_child("owner-check-").unwrap();
        let child_file = child
            .dir
            .as_ref()
            .unwrap()
            .try_clone()
            .unwrap()
            .into_std_file();
        super::windows::verify_current_user_owner(&child_file).unwrap();

        root.retain().unwrap();
        let retained = crate::workspace::root::windows::open_regular_file_shared(
            &root.dir,
            std::ffi::OsStr::new(RETAIN_FILE),
        )
        .unwrap();
        super::windows::verify_current_user_owner(&retained).unwrap();

        {
            let lease = root.lease.lock().unwrap();
            super::write_cleanup_ready_marker(
                &root.dir,
                &root.path,
                &root.run_id,
                root.owner,
                lease.as_deref().unwrap(),
            )
            .unwrap();
        }
        let cleanup_ready = crate::workspace::root::windows::open_regular_file_shared(
            &root.dir,
            std::ffi::OsStr::new(CLEANUP_READY_FILE),
        )
        .unwrap();
        super::windows::verify_current_user_owner(&cleanup_ready).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn managed_root_capability_is_enumerable_on_linux() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        coordinator.dir.create_dir("run-entry").unwrap();

        let entries = super::owned_directory_entries(&coordinator.dir)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(entries.iter().any(|entry| entry == "run-entry"));
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
    fn diagnostic_details_have_count_and_utf8_byte_caps() {
        // Arrange.
        let mut ledger = super::ReclaimLedger::default();
        // Act.
        for _ in 0..=super::MAX_DIAGNOSTIC_DETAILS {
            ledger.detail(hoimin_core::JanitorReason::DeleteFailed, "é".repeat(3_000));
        }
        let report = ledger.finish();
        // Assert.
        assert_eq!(report.details.len(), 256);
        assert!(
            report
                .details
                .iter()
                .all(|detail| detail.message.len() <= 4096)
        );
        assert_eq!(report.detail_omissions.truncated_details, 256);
        assert_eq!(report.detail_omissions.issue_details, 1);
    }

    #[test]
    fn selection_deadline_after_eof_does_not_update_the_cursor() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let before = super::read_coordinator_state(
            &coordinator.file,
            &coordinator.path.join(COORDINATOR_FILE),
        )
        .unwrap();
        let base = std::time::Instant::now();
        let crossed = std::cell::Cell::new(false);

        let mut scan = hoimin_core::JanitorScan::default();
        let error = super::select_reclaim_candidates_locked_with_hooks(
            &coordinator,
            &mut scan,
            std::time::Duration::from_secs(5),
            &|| [base, base + std::time::Duration::from_secs(6)][usize::from(crossed.get())],
            &|| crossed.set(true),
        )
        .unwrap_err();

        assert!(error.to_string().contains("deadline"), "{error}");
        assert!(scan.directory_scan_complete);
        assert_eq!(
            scan.stop_reason,
            Some(hoimin_core::JanitorReason::SelectionDeadline)
        );
        assert_eq!(scan.selected_roots, 0);
        let after = super::read_coordinator_state(
            &coordinator.file,
            &coordinator.path.join(COORDINATOR_FILE),
        )
        .unwrap();
        assert_eq!(after, before);
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
        use std::io::Write;

        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let mut partial = super::create_owned_file(
            &root.dir,
            super::CLEANUP_READY_FILE,
            super::OwnedFileAccess::Write,
        )
        .unwrap();
        partial.write_all(b"{").unwrap();
        partial.sync_all().unwrap();
        drop(partial);

        root.mark_cleanup_ready().unwrap();

        let marker = super::read_cleanup_ready_marker(&root.dir).expect("complete marker");
        assert_eq!(marker.run_id, root.run_id);
        assert_eq!(marker.owner, root.owner);
    }

    fn inject_publish_failure(
        selected: super::PublishBoundary,
        observed: super::PublishBoundary,
    ) -> Result<(), super::WorkspaceError> {
        if observed == selected {
            Err(super::WorkspaceError::io(
                "inject publish failure",
                Utf8Path::new("managed-root"),
                format!("injected at {selected:?}"),
            ))
        } else {
            Ok(())
        }
    }

    fn inject_child_failure(
        selected: super::ChildCreationBoundary,
        observed: super::ChildCreationBoundary,
    ) -> Result<(), super::WorkspaceError> {
        if observed == selected {
            Err(super::WorkspaceError::io(
                "inject child creation failure",
                Utf8Path::new("managed-child"),
                format!("injected at {selected:?}"),
            ))
        } else {
            Ok(())
        }
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
                &|observed| inject_publish_failure(selected, observed),
            );

            assert!(result.is_err(), "boundary {selected:?} did not fail");
            let _ = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
            let residual = std::fs::read_dir(parent.join(MANAGED_DIR))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .filter(|name| name != COORDINATOR_FILE)
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
                inject_child_failure(selected, observed)
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

    #[cfg(unix)]
    #[test]
    fn round_five_staging_first_handle_identity_failure_preserves_a_same_name_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let parked_name = "parked-round-five-staging";
        let original_identity = std::cell::Cell::new(None);
        let replacement_identity = std::cell::Cell::new(None);

        let error = ManagedRunRoot::create_with_hooks(
            &coordinator,
            OwnerKind::PublicExecution,
            &|_| Ok(()),
            &|_| {
                let staging_name =
                    only_test_entry_with_prefix(&coordinator.path, super::STAGING_PREFIX);
                let original = super::metadata_identity(
                    &coordinator.dir.symlink_metadata(&staging_name).unwrap(),
                );
                original_identity.set(Some(original));
                coordinator
                    .dir
                    .rename(&staging_name, &coordinator.dir, parked_name)?;
                create_test_owned_directory_entry(&coordinator.dir, &staging_name)?;
                let replacement = super::metadata_identity(
                    &coordinator.dir.symlink_metadata(&staging_name).unwrap(),
                );
                assert_ne!(replacement, original);
                replacement_identity.set(Some(replacement));
                Err(std::io::Error::other("injected staging identity failure"))
            },
        )
        .unwrap_err();

        assert_primary_then_cleanup_unavailable(&error, "injected staging identity failure");
        let staging_name = only_test_entry_with_prefix(&coordinator.path, super::STAGING_PREFIX);
        assert_eq!(
            Some(super::metadata_identity(
                &coordinator.dir.symlink_metadata(&staging_name).unwrap()
            )),
            replacement_identity.get(),
        );
        assert_eq!(
            Some(super::metadata_identity(
                &coordinator.dir.symlink_metadata(parked_name).unwrap()
            )),
            original_identity.get(),
        );
    }

    #[cfg(unix)]
    #[test]
    fn round_five_child_first_handle_identity_failure_preserves_replacement_and_lifecycle() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let parked_name = "parked-round-five-child";
        let original_identity = std::cell::Cell::new(None);
        let replacement_identity = std::cell::Cell::new(None);

        let error = root
            .create_child_with_hooks("round-five-", &|_| Ok(()), &|_| {
                let child_name = only_test_entry_with_prefix(&root.path, "round-five-");
                let original =
                    super::metadata_identity(&root.dir.symlink_metadata(&child_name).unwrap());
                original_identity.set(Some(original));
                root.dir.rename(&child_name, &root.dir, parked_name)?;
                create_test_owned_directory_entry(&root.dir, &child_name)?;
                let replacement =
                    super::metadata_identity(&root.dir.symlink_metadata(&child_name).unwrap());
                assert_ne!(replacement, original);
                replacement_identity.set(Some(replacement));
                Err(std::io::Error::other("injected child identity failure"))
            })
            .unwrap_err();

        assert_primary_then_cleanup_unavailable(&error, "injected child identity failure");
        let child_name = only_test_entry_with_prefix(&root.path, "round-five-");
        assert_eq!(
            Some(super::metadata_identity(
                &root.dir.symlink_metadata(&child_name).unwrap()
            )),
            replacement_identity.get(),
        );
        assert_eq!(
            Some(super::metadata_identity(
                &root.dir.symlink_metadata(parked_name).unwrap()
            )),
            original_identity.get(),
        );
        assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    }

    #[test]
    fn round_five_explicit_rollback_runs_once_with_deadline_remainder_and_keeps_error_order() {
        let base = std::time::Instant::now();
        let cleanup_calls = std::cell::Cell::new(0_u32);
        let error = super::error_after_new_directory_rollback_with(
            std::io::Error::other("primary construction failure"),
            base + std::time::Duration::from_secs(10),
            || base + std::time::Duration::from_secs(4),
            |remaining| {
                cleanup_calls.set(cleanup_calls.get() + 1);
                assert_eq!(remaining, std::time::Duration::from_secs(6));
                Err(std::io::Error::other("bounded rollback failure"))
            },
            |primary, secondary| {
                std::io::Error::new(
                    primary.kind(),
                    format!("{primary}; secondary new-directory rollback failure: {secondary}"),
                )
            },
        );

        assert_eq!(cleanup_calls.get(), 1);
        assert_eq!(
            error.to_string(),
            "primary construction failure; secondary new-directory rollback failure: bounded rollback failure"
        );
        drop(error);
        assert_eq!(cleanup_calls.get(), 1, "drop repeated namespace cleanup");
    }

    #[cfg(windows)]
    #[test]
    fn round_five_staging_first_identity_failure_uses_exact_handle_rollback() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let replacement_was_blocked = std::cell::Cell::new(false);

        let error = ManagedRunRoot::create_with_hooks(
            &coordinator,
            OwnerKind::PublicExecution,
            &|_| Ok(()),
            &|_| {
                let staging_name =
                    only_test_entry_with_prefix(&coordinator.path, super::STAGING_PREFIX);
                let source = coordinator.path.join(&staging_name);
                let destination = coordinator.path.join("round-five-staging-replacement");
                replacement_was_blocked.set(std::fs::rename(source, destination).is_err());
                Err(std::io::Error::other("injected staging identity failure"))
            },
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("injected staging identity failure")
        );
        assert!(replacement_was_blocked.get());
        assert!(
            std::fs::read_dir(&coordinator.path)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .all(|name| !name.starts_with(super::STAGING_PREFIX)),
            "exact-handle rollback left the empty staging directory"
        );
    }

    #[cfg(windows)]
    #[test]
    fn round_five_child_first_identity_failure_uses_exact_handle_rollback_before_lifecycle() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let replacement_was_blocked = std::cell::Cell::new(false);

        let error = root
            .create_child_with_hooks("round-five-", &|_| Ok(()), &|_| {
                let child_name = only_test_entry_with_prefix(&root.path, "round-five-");
                let source = root.path.join(&child_name);
                let destination = root.path.join("round-five-child-replacement");
                replacement_was_blocked.set(std::fs::rename(source, destination).is_err());
                Err(std::io::Error::other("injected child identity failure"))
            })
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("injected child identity failure")
        );
        assert!(replacement_was_blocked.get());
        assert!(
            std::fs::read_dir(&root.path)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .all(|name| !name.starts_with("round-five-")),
            "exact-handle rollback left the empty managed child"
        );
        assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    }

    #[cfg(windows)]
    #[test]
    fn round_five_managed_child_cleanup_succeeds_after_pinned_creation_handoff() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = root.create_child("round-five-cleanup-").unwrap();
        let child_name = child.path().file_name().unwrap();
        let working_identity = super::directory_identity(child.dir.as_ref().unwrap()).unwrap();
        let namespace_identity =
            super::metadata_identity(&root.dir.symlink_metadata(child_name).unwrap());
        assert_eq!(working_identity, namespace_identity);
        std::fs::write(child.path().join("payload"), b"payload").unwrap();

        child.cleanup().unwrap();

        assert!(root.dir.symlink_metadata(child_name).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn round_five_published_markers_are_pinned_but_allow_intended_concurrent_opens() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let root_path = root.path.clone();
        let lease_path = root.path.join(super::LEASE_FILE);
        let heartbeat_path = root.path.join(super::HEARTBEAT_FILE);
        let moved_lease_path = root.path.join("round-five-moved-lease");
        let lease_reader = super::super::root::windows::open_regular_file_shared(
            &root.dir,
            std::ffi::OsStr::new(super::LEASE_FILE),
        )
        .unwrap();
        let (retained_lease_identity, retained_lease_len) = {
            let lease = root.lease.lock().unwrap();
            let lease = lease.as_deref().unwrap();
            (
                super::file_identity(lease).unwrap(),
                lease.metadata().unwrap().len(),
            )
        };
        let lease_metadata = lease_reader.metadata().unwrap();
        assert!(lease_metadata.is_file());
        assert_eq!(lease_metadata.len(), retained_lease_len);
        assert_eq!(
            super::file_identity(&lease_reader).unwrap(),
            retained_lease_identity
        );
        assert!(std::fs::File::try_lock(&lease_reader).is_err());
        let heartbeat_updater = super::super::root::windows::open_regular_file_for_update_shared(
            &root.dir,
            std::ffi::OsStr::new(super::HEARTBEAT_FILE),
        )
        .unwrap();
        super::refresh_file_modified_time(&heartbeat_updater).unwrap();

        assert!(
            std::fs::rename(&lease_path, &moved_lease_path).is_err(),
            "the reopened lease allowed an independent rename"
        );
        assert!(
            std::fs::remove_file(&heartbeat_path).is_err(),
            "the reopened heartbeat allowed an independent delete"
        );

        drop(lease_reader);
        drop(heartbeat_updater);
        drop(root);
        std::fs::rename(&lease_path, &moved_lease_path).unwrap();
        std::fs::remove_file(&moved_lease_path).unwrap();
        std::fs::remove_file(&heartbeat_path).unwrap();
        std::fs::remove_dir(root_path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn round_five_returned_root_dir_matches_the_retained_rename_capability() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let rename_handle = root.rename_handle.lock().unwrap();

        assert_eq!(
            super::directory_identity(&root.dir).unwrap(),
            super::directory_identity(rename_handle.as_ref().unwrap()).unwrap()
        );
    }

    #[cfg(windows)]
    fn replace_published_root(
        coordinator: &ManagedRootCoordinator,
        replacement_installed: &std::cell::Cell<bool>,
        boundary: super::PublishBoundary,
    ) {
        if boundary != super::PublishBoundary::RenamedActive {
            return;
        }
        let active_name = only_test_entry_with_prefix(&coordinator.path, super::ACTIVE_PREFIX);
        if coordinator
            .dir
            .rename(
                &active_name,
                &coordinator.dir,
                "parked-round-five-published-root",
            )
            .is_err()
        {
            return;
        }
        create_test_owned_directory_entry(&coordinator.dir, &active_name).unwrap();
        replacement_installed.set(true);
    }

    #[cfg(windows)]
    #[test]
    fn round_five_post_rename_replacement_never_builds_a_mixed_managed_root() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let replacement_installed = std::cell::Cell::new(false);

        let result = ManagedRunRoot::create_with_publish_hook(
            &coordinator,
            OwnerKind::PublicExecution,
            &|boundary| {
                replace_published_root(&coordinator, &replacement_installed, boundary);
                Ok(())
            },
        );

        let identities = result.as_ref().ok().map(|root| {
            let rename_handle = root.rename_handle.lock().unwrap();
            (
                super::directory_identity(&root.dir).unwrap(),
                super::directory_identity(rename_handle.as_ref().unwrap()).unwrap(),
            )
        });
        assert_eq!(
            identities.as_ref().map(|(opened, _)| opened),
            identities.as_ref().map(|(_, retained)| retained),
            "publication returned a path handle from a different object"
        );
        let expected = [Ok(()), Err(true)][usize::from(replacement_installed.get())];
        assert_eq!(
            result
                .map(|_| ())
                .map_err(|error| matches!(error, super::WorkspaceError::InvalidPath { .. })),
            expected,
            "replacement must cause a typed mismatch and blocked replacement must preserve a coherent root"
        );
    }

    #[test]
    fn active_locked_lease_is_preserved_even_past_the_stale_threshold() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        let future = std::time::SystemTime::now() + std::time::Duration::from_hours(25);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 1);
        assert!(published.exists());
        drop(root);
    }

    #[cfg(windows)]
    #[test]
    fn live_mutation_barrier_preserves_a_cleanup_ready_root_with_an_unlocked_lease() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        root.mark_cleanup_ready().unwrap();
        {
            let lease = root.lease.lock().unwrap();
            std::fs::File::unlock(lease.as_deref().unwrap()).unwrap();
        }

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 0, "{report:?}");
        assert_eq!(report.preserved_roots, 1, "{report:?}");
        assert!(published.exists());
        {
            let lease = root.lease.lock().unwrap();
            std::fs::File::try_lock(lease.as_deref().unwrap()).unwrap();
        }
        let cleanup = root.cleanup(std::time::Duration::from_secs(5));
        assert_eq!(
            cleanup.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{cleanup:?}"
        );
    }

    #[test]
    fn unlocked_lease_with_a_heartbeat_at_least_twenty_four_hours_old_is_reclaimed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let published = root.path().to_owned();
        // Model the shared file description retained by a concurrently forked child.
        // Closing the owner's descriptor alone must not establish this fixture's
        // unlocked premise: the child can hold its copy until exec.
        #[cfg(unix)]
        let _inherited_lease = root
            .lease
            .lock()
            .unwrap()
            .as_deref()
            .unwrap()
            .try_clone()
            .unwrap();
        {
            let lease = root.lease.lock().unwrap();
            std::fs::File::unlock(lease.as_deref().unwrap()).unwrap();
        }
        drop(root);
        let future = std::time::SystemTime::now() + std::time::Duration::from_hours(25);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert_eq!(report.preserved_roots, 0);
        assert!(!published.exists());
    }

    #[test]
    fn coordinator_uses_full_blake3_digests_in_fixed_slots() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let _coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let bytes = std::fs::read(parent.join(MANAGED_DIR).join(COORDINATOR_FILE)).unwrap();

        assert_eq!(bytes.len(), 1_025);
        assert_eq!(bytes[0], 0);
        for start in [1, 513] {
            let slot = &bytes[start..start + 512];
            assert_eq!(&slot[0..8], b"HMCUR001");
            assert_eq!(u32::from_le_bytes(slot[8..12].try_into().unwrap()), 2);
            assert_eq!(u64::from_le_bytes(slot[12..20].try_into().unwrap()), 0);
            assert_eq!(u16::from_le_bytes(slot[20..22].try_into().unwrap()), 0);
            assert!(slot[22..480].iter().all(|byte| *byte == 0));
            assert_eq!(&slot[480..512], blake3::hash(&slot[..480]).as_bytes());
        }
    }

    #[test]
    fn new_coordinator_namespace_leaves_old_workspaces_untouched() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let old = parent.join("hoimin-workspaces-v1");
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join(COORDINATOR_FILE), b"old coordinator").unwrap();
        std::fs::write(old.join("retained-source.py"), b"value = True\n").unwrap();

        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        assert_ne!(coordinator.path, old);
        assert_eq!(
            std::fs::read(old.join(COORDINATOR_FILE)).unwrap(),
            b"old coordinator"
        );
        assert_eq!(
            std::fs::read(old.join("retained-source.py")).unwrap(),
            b"value = True\n"
        );
    }

    #[test]
    fn old_coordinator_schema_is_rejected_without_rewriting() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let path = coordinator.path.join(COORDINATOR_FILE);
        drop(coordinator);
        // Schema 1's empty slot, with its CRC32 precomputed independently.
        let mut slot = [0_u8; 512];
        slot[..8].copy_from_slice(b"HMCUR001");
        slot[8..12].copy_from_slice(&1_u32.to_le_bytes());
        slot[508..].copy_from_slice(&[100, 205, 203, 59]);
        let mut bytes = vec![0];
        bytes.extend_from_slice(&slot);
        bytes.extend_from_slice(&slot);
        std::fs::write(&path, &bytes).unwrap();

        assert!(ManagedRootCoordinator::open(parent).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn owned_create_helpers_are_exclusive_and_apply_platform_policy() {
        let parent = tempfile::tempdir().unwrap();
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();

        let directory = super::create_owned_directory(&parent_dir, "owned-directory").unwrap();
        let file =
            super::create_owned_file(&parent_dir, "owned-file", super::OwnedFileAccess::ReadWrite)
                .unwrap();
        assert_eq!(
            super::create_owned_directory(&parent_dir, "owned-directory")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists,
        );
        assert_eq!(
            super::create_owned_file(&parent_dir, "owned-file", super::OwnedFileAccess::Write,)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists,
        );

        #[cfg(unix)]
        {
            use cap_std::fs::PermissionsExt as _;
            use std::os::unix::fs::PermissionsExt as _;

            assert_eq!(
                directory.metadata(".").unwrap().permissions().mode() & 0o777,
                0o700,
            );
            assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        }
        #[cfg(windows)]
        {
            let directory = directory.into_std_file();
            super::windows::verify_current_user_owner(&directory).unwrap();
            super::windows::verify_current_user_owner(&file).unwrap();
        }
    }

    #[test]
    fn owned_directory_metadata_failure_leaves_the_unverified_entry_untouched() {
        let parent = tempfile::tempdir().unwrap();
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();

        let error = super::create_owned_directory_with(
            &parent_dir,
            "metadata-failure-directory",
            create_test_owned_directory_entry,
            |_, _| Err(std::io::Error::other("injected directory metadata failure")),
            |parent, name| super::open_owned_directory(parent, name),
            super::rollback_new_owned_directory,
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(
            error.to_string(),
            "injected directory metadata failure; secondary created-directory rollback failure: bounded cleanup cannot proceed without the created directory identity; unverified name left untouched",
        );
        assert!(
            parent_dir
                .symlink_metadata("metadata-failure-directory")
                .unwrap()
                .is_dir()
        );
    }

    #[test]
    fn owned_directory_metadata_failure_does_not_adopt_post_failure_name_as_created_identity() {
        let parent = tempfile::tempdir().unwrap();
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();
        let name = "metadata-failure-replacement-directory";
        let original_identity = std::cell::Cell::new(None);
        let replacement_identity = std::cell::Cell::new(None);

        let error = super::create_owned_directory_with(
            &parent_dir,
            name,
            create_test_owned_directory_entry,
            |parent, name| {
                let original = super::metadata_identity(&parent.symlink_metadata(name).unwrap());
                original_identity.set(Some(original));
                parent.rename(name, parent, "parked-created-directory")?;
                create_test_owned_directory_entry(parent, name)?;
                let replacement = super::metadata_identity(&parent.symlink_metadata(name).unwrap());
                assert_ne!(replacement, original);
                replacement_identity.set(Some(replacement));
                Err(std::io::Error::other("injected directory metadata failure"))
            },
            |parent, name| super::open_owned_directory(parent, name),
            super::rollback_new_owned_directory,
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(
            error.to_string(),
            "injected directory metadata failure; secondary created-directory rollback failure: bounded cleanup cannot proceed without the created directory identity; unverified name left untouched",
        );
        assert_eq!(
            Some(super::metadata_identity(
                &parent_dir.symlink_metadata(name).unwrap()
            )),
            replacement_identity.get(),
        );
        assert_eq!(
            Some(super::metadata_identity(
                &parent_dir
                    .symlink_metadata("parked-created-directory")
                    .unwrap()
            )),
            original_identity.get(),
        );
    }

    #[test]
    fn owned_directory_open_failure_rolls_back_the_created_identity() {
        let parent = tempfile::tempdir().unwrap();
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();

        let error = super::create_owned_directory_with(
            &parent_dir,
            "rolled-back-directory",
            create_test_owned_directory_entry,
            |parent, name| parent.symlink_metadata(name),
            |_, _| Err(std::io::Error::other("injected directory open failure")),
            super::rollback_new_owned_directory,
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(error.to_string(), "injected directory open failure");
        assert_eq!(
            parent_dir
                .symlink_metadata("rolled-back-directory")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound,
        );
    }

    #[test]
    fn owned_directory_rollback_failure_keeps_the_open_error_first() {
        let parent = tempfile::tempdir().unwrap();
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();

        let error = super::create_owned_directory_with(
            &parent_dir,
            "rollback-failure-directory",
            create_test_owned_directory_entry,
            |parent, name| parent.symlink_metadata(name),
            |_, _| Err(std::io::Error::other("injected directory open failure")),
            |_, _, _, _| Err(std::io::Error::other("injected cleanup failure")),
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(
            error.to_string(),
            "injected directory open failure; secondary created-directory rollback failure: injected cleanup failure",
        );
        assert!(
            parent_dir
                .symlink_metadata("rollback-failure-directory")
                .unwrap()
                .is_dir()
        );
    }

    #[test]
    fn owned_directory_rollback_rejects_a_same_name_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();
        let name = "replacement-protected-directory";

        create_test_owned_directory_entry(&parent_dir, name).unwrap();
        let original_identity =
            super::metadata_identity(&parent_dir.symlink_metadata(name).unwrap());
        parent_dir
            .rename(name, &parent_dir, "parked-original-directory")
            .unwrap();
        create_test_owned_directory_entry(&parent_dir, name).unwrap();
        let replacement_identity =
            super::metadata_identity(&parent_dir.symlink_metadata(name).unwrap());
        assert_ne!(replacement_identity, original_identity);

        let error = super::rollback_new_owned_directory(
            &parent_dir,
            name,
            original_identity,
            super::OWNER_CLEANUP_BUDGET,
        )
        .unwrap_err();

        assert!(error.to_string().contains("identity changed"));
        assert_eq!(
            super::metadata_identity(&parent_dir.symlink_metadata(name).unwrap()),
            replacement_identity,
        );
    }

    #[test]
    fn initialize_new_coordinator_writes_valid_unlocked_fixed_layout() {
        let parent = tempfile::tempdir().unwrap();
        let parent_path = Utf8Path::from_path(parent.path()).unwrap();
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();
        let coordinator_path = parent_path.join("new-coordinator");
        let file = super::create_owned_file(
            &parent_dir,
            "new-coordinator",
            super::OwnedFileAccess::ReadWrite,
        )
        .unwrap();
        let identity = super::file_identity(&file).unwrap();

        let file = super::initialize_new_coordinator(
            &parent_dir,
            file,
            identity,
            &coordinator_path,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .unwrap();

        assert_eq!(file.metadata().unwrap().len(), 1_025);
        std::fs::File::try_lock(&file).unwrap();
        std::fs::File::unlock(&file).unwrap();
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
        let parent_dir =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();
        super::create_managed_root_entry(&parent_dir, Utf8Path::from_path(&managed).unwrap())
            .unwrap();
        let managed_dir = super::open_owned_directory(&parent_dir, MANAGED_DIR).unwrap();
        let coordinator_path = managed.join(COORDINATOR_FILE);
        let coordinator_file = super::create_owned_file(
            &managed_dir,
            COORDINATOR_FILE,
            super::OwnedFileAccess::ReadWrite,
        )
        .unwrap();
        drop(coordinator_file);
        drop(managed_dir);
        drop(parent_dir);
        let writer_path = coordinator_path.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(25));
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(writer_path)
                .unwrap();
            std::fs::File::lock(&file).unwrap();
            file.write_all(&super::initial_coordinator_bytes()).unwrap();
            file.sync_all().unwrap();
            std::fs::File::unlock(&file).unwrap();
        });

        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        writer.join().unwrap();

        assert_eq!(coordinator.file.metadata().unwrap().len(), 1_025);
    }

    fn bootstrap_parent_from_environment() -> camino::Utf8PathBuf {
        camino::Utf8PathBuf::from_path_buf(
            std::env::var_os("HOIMIN_BOOTSTRAP_TEST_PARENT")
                .expect("bootstrap subprocess parent")
                .into(),
        )
        .unwrap()
    }

    fn wait_for_bootstrap_release(parent: &Utf8Path) {
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

    #[test]
    #[ignore = "subprocess fixture for coordinator bootstrap"]
    fn fresh_process_bootstrap_child() {
        let parent = bootstrap_parent_from_environment();

        ManagedRootCoordinator::open(&parent).unwrap();
    }

    #[test]
    #[ignore = "subprocess fixture for concurrent coordinator bootstrap"]
    fn fresh_process_bootstrap_barrier_child() {
        let parent = bootstrap_parent_from_environment();
        wait_for_bootstrap_release(&parent);

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
                .arg("workspace::owned::tests::fresh_process_bootstrap_barrier_child")
                .arg("--ignored")
                .env("HOIMIN_BOOTSTRAP_TEST_PARENT", parent)
                .env("HOIMIN_BOOTSTRAP_TEST_BARRIER", "1")
                .spawn()
                .unwrap()
        };
        let mut first = spawn();
        let mut second = spawn();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::fs::read_dir(parent)
            .unwrap()
            .map(Result::unwrap)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("ready-"))
            .count()
            != 2
        {
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
    fn damaged_newest_digest_recovers_the_previous_cursor() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let path = coordinator.path.join(COORDINATOR_FILE);
        let first_cursor = "run-00000000-0000-0000-0000-000000000001";
        let next_cursor = "run-00000000-0000-0000-0000-000000000002";
        let guard = super::CoordinatorLockGuard::acquire(&coordinator).unwrap();
        let (slot, state) = super::read_coordinator_state(&coordinator.file, &path).unwrap();
        super::persist_coordinator_cursor(&coordinator.file, &path, slot, &state, first_cursor)
            .unwrap();
        let (slot, state) = super::read_coordinator_state(&coordinator.file, &path).unwrap();
        assert_eq!(
            (slot, state.generation, state.cursor.as_str()),
            (1, 1, first_cursor)
        );
        super::persist_coordinator_cursor(&coordinator.file, &path, slot, &state, next_cursor)
            .unwrap();
        let (slot, state) = super::read_coordinator_state(&coordinator.file, &path).unwrap();
        assert_eq!(
            (slot, state.generation, state.cursor.as_str()),
            (0, 2, next_cursor)
        );
        guard.unlock(&path).unwrap();
        drop(coordinator);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[481] ^= 1; // First slot's BLAKE3 digest; its payload is intact.
        std::fs::write(&path, &bytes).unwrap();

        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let (slot, state) = super::read_coordinator_state(&coordinator.file, &path).unwrap();
        assert_eq!(
            (slot, state.generation, state.cursor.as_str()),
            (1, 1, first_cursor)
        );
        drop(coordinator);
        bytes[993] ^= 1; // Damage the second digest too: neither slot is usable.
        std::fs::write(&path, &bytes).unwrap();
        assert!(ManagedRootCoordinator::open(parent).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
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
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{record:?}"
        );
        assert!(!first_path.exists());
        assert!(second_path.exists());
    }

    #[cfg(windows)]
    #[test]
    fn round_six_live_owner_cleanup_consumes_claimed_root_and_lease_pins() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);

        let record = root.cleanup(std::time::Duration::from_secs(5));

        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{record:?}"
        );
        assert!(coordinator.dir.symlink_metadata(&active).is_err());
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn root_delete_failure_after_lease_removal_preserves_owner_retry() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()))
            .unwrap();
        let identity = super::directory_identity(&root.dir).unwrap();
        let mut capabilities = root.take_owner_cleanup_capabilities().unwrap();
        let budget = std::time::Duration::from_secs(5);
        super::remove_root_control_markers(
            &root.dir,
            std::time::Instant::now(),
            budget,
            &mut capabilities,
        )
        .unwrap();
        assert!(capabilities.lease_guard.is_none());
        assert_eq!(
            root.dir
                .symlink_metadata(super::LEASE_FILE)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        // A late child makes the real deletion API fail after the lease is gone.
        root.dir.write("late-child", b"retry me").unwrap();

        let failed = super::remove_cleanup_root_checked(
            &coordinator.dir,
            std::ffi::OsStr::new(&deleting),
            Utf8Path::new(&deleting),
            identity,
            std::time::Instant::now(),
            budget,
            &mut capabilities,
        );

        assert!(failed.is_err());
        assert!(matches!(
            capabilities.root_authority,
            super::RootCleanupAuthority::Pinned(_)
        ));
        root.restore_owner_cleanup_capabilities(capabilities);
        assert!(root.rename_handle.lock().unwrap().is_some());
        assert!(root.lease.lock().unwrap().is_none());
        let resumed = root.cleanup(budget);
        assert_eq!(
            resumed.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{resumed:?}"
        );
        assert_eq!(
            coordinator
                .dir
                .symlink_metadata(&deleting)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[cfg(windows)]
    fn pinned_cleanup_directory(
        parent: &cap_std::fs::Dir,
        name: &str,
    ) -> super::CleanupCapabilities {
        let root =
            super::windows::create_relative_managed_directory(parent, std::ffi::OsStr::new(name))
                .unwrap();
        super::CleanupCapabilities {
            progress: super::DeletionProgress::default(),
            lease_guard: None,
            root_authority: super::RootCleanupAuthority::Pinned(cap_std::fs::Dir::from_std_file(
                root,
            )),
        }
    }

    #[cfg(windows)]
    #[test]
    fn root_delete_identity_failure_preserves_original_pin_for_retry() {
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let mut capabilities = pinned_cleanup_directory(&coordinator.dir, "identity-retry");
        let identity =
            super::metadata_identity(&coordinator.dir.symlink_metadata("identity-retry").unwrap());
        let budget = std::time::Duration::from_secs(5);

        let failed = super::remove_cleanup_root_checked(
            &coordinator.dir,
            std::ffi::OsStr::new("identity-retry"),
            Utf8Path::new("identity-retry"),
            (identity.0, identity.1 ^ 1),
            std::time::Instant::now(),
            budget,
            &mut capabilities,
        );

        assert!(failed.is_err());
        assert!(matches!(
            capabilities.root_authority,
            super::RootCleanupAuthority::Pinned(_)
        ));
        super::remove_claimed_tree_bounded_with_capabilities(
            &coordinator.dir,
            "identity-retry",
            Some(identity),
            budget,
            &mut capabilities,
        )
        .unwrap();
        assert_eq!(
            coordinator
                .dir
                .symlink_metadata("identity-retry")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[cfg(windows)]
    #[test]
    fn successful_root_delete_consumes_authority_despite_postcheck_failure() {
        // The pin deletes the original root; these names affect only the final lookup.
        for postcheck_name in ["invalid\0name", "survivor"] {
            let parent = tempfile::tempdir().unwrap();
            let coordinator =
                ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
            let mut capabilities = pinned_cleanup_directory(&coordinator.dir, "deleted");
            let identity =
                super::metadata_identity(&coordinator.dir.symlink_metadata("deleted").unwrap());
            create_test_owned_directory_entry(&coordinator.dir, "survivor").unwrap();
            std::fs::write(coordinator.path.join("survivor/sentinel"), b"untouched").unwrap();

            let result = super::remove_cleanup_root_checked(
                &coordinator.dir,
                std::ffi::OsStr::new(postcheck_name),
                Utf8Path::new("deleted"),
                identity,
                std::time::Instant::now(),
                std::time::Duration::from_secs(5),
                &mut capabilities,
            );

            let error = result.unwrap_err();
            assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
            assert!(matches!(
                capabilities.root_authority,
                super::RootCleanupAuthority::Consumed
            ));
            assert_eq!(
                coordinator
                    .dir
                    .symlink_metadata("deleted")
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
            assert_eq!(
                std::fs::read(coordinator.path.join("survivor/sentinel")).unwrap(),
                b"untouched"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn consumed_root_authority_rejects_same_name_replacement_with_matching_identity() {
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let mut capabilities = pinned_cleanup_directory(&coordinator.dir, "replacement");
        let identity =
            super::metadata_identity(&coordinator.dir.symlink_metadata("replacement").unwrap());
        let budget = std::time::Duration::from_secs(5);
        super::remove_claimed_tree_bounded_with_capabilities(
            &coordinator.dir,
            "replacement",
            Some(identity),
            budget,
            &mut capabilities,
        )
        .unwrap();
        create_test_owned_directory_entry(&coordinator.dir, "replacement").unwrap();
        std::fs::write(coordinator.path.join("replacement/sentinel"), b"untouched").unwrap();
        let replacement_identity =
            super::metadata_identity(&coordinator.dir.symlink_metadata("replacement").unwrap());

        let result = super::remove_claimed_tree_bounded_with_capabilities(
            &coordinator.dir,
            "replacement",
            Some(replacement_identity),
            budget,
            &mut capabilities,
        );

        assert!(result.is_err());
        assert!(matches!(
            capabilities.root_authority,
            super::RootCleanupAuthority::Consumed
        ));
        assert_eq!(
            std::fs::read(coordinator.path.join("replacement/sentinel")).unwrap(),
            b"untouched"
        );
    }

    #[cfg(windows)]
    #[test]
    fn consumed_root_removal_preserves_an_empty_same_name_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let mut capabilities = pinned_cleanup_directory(&coordinator.dir, "replacement");
        let identity =
            super::metadata_identity(&coordinator.dir.symlink_metadata("replacement").unwrap());
        let budget = std::time::Duration::from_secs(5);
        super::remove_claimed_tree_bounded_with_capabilities(
            &coordinator.dir,
            "replacement",
            Some(identity),
            budget,
            &mut capabilities,
        )
        .unwrap();
        // Consumed authority may confirm absence but has no authority over a new entry.
        assert!(
            super::remove_claimed_tree_bounded_with_capabilities(
                &coordinator.dir,
                "replacement",
                Some(identity),
                budget,
                &mut capabilities,
            )
            .unwrap()
            .complete
        );
        create_test_owned_directory_entry(&coordinator.dir, "replacement").unwrap();
        let replacement_identity =
            super::metadata_identity(&coordinator.dir.symlink_metadata("replacement").unwrap());

        let result = super::remove_cleanup_root_checked(
            &coordinator.dir,
            std::ffi::OsStr::new("replacement"),
            Utf8Path::new("replacement"),
            replacement_identity,
            std::time::Instant::now(),
            budget,
            &mut capabilities,
        );

        assert!(result.is_err());
        assert_eq!(
            super::metadata_identity(&coordinator.dir.symlink_metadata("replacement").unwrap()),
            replacement_identity
        );
    }

    #[cfg(windows)]
    #[test]
    fn round_six_review_lease_delete_pin_can_clear_readonly_lease() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let lease = root.path().join(super::LEASE_FILE);
        let mut permissions = std::fs::metadata(&lease).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&lease, permissions).unwrap();

        let record = root.cleanup(std::time::Duration::from_secs(5));

        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{record:?}"
        );
        assert!(record.evidence.remaining_root.is_none(), "{record:?}");
        assert!(!root.path().exists());
    }

    #[cfg(windows)]
    #[test]
    fn round_six_review_complete_state_precedes_zero_budget_deferral() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let first = root.cleanup(std::time::Duration::from_secs(5));
        assert_eq!(
            first.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{first:?}"
        );

        let second = root.cleanup(std::time::Duration::ZERO);

        assert_eq!(
            second.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{second:?}"
        );
        assert!(second.evidence.remaining_root.is_none(), "{second:?}");
    }

    #[test]
    fn owner_cleanup_defers_until_all_managed_child_handles_are_closed() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = root.create_child("worker-").unwrap();

        let deferred = root.cleanup(std::time::Duration::from_secs(1));

        assert_eq!(
            deferred.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert!(root.path().exists());
        assert!(root.create_child("worker-").is_err());
        drop(child);

        let completed = root.cleanup(std::time::Duration::from_secs(5));
        assert_eq!(
            completed.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean
        );
        assert!(!root.path().exists());
    }

    #[cfg(windows)]
    #[test]
    fn child_handles_close_before_quiescence_is_published() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = std::sync::Arc::new(
            ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap(),
        );
        let mut child = root.create_child("spool-").unwrap();
        let gate = std::sync::Arc::new(super::ManagedChildDropGate {
            before_closed_state: std::sync::Arc::new(std::sync::Barrier::new(2)),
            release: std::sync::Arc::new(std::sync::Barrier::new(2)),
            after_published_state: None,
            after_published_release: None,
        });
        child.drop_gate = Some(std::sync::Arc::clone(&gate));
        let drop_thread = std::thread::spawn(move || drop(child));

        gate.before_closed_state.wait();
        let while_closing = root.cleanup(std::time::Duration::from_secs(5));
        gate.release.wait();
        drop_thread.join().unwrap();

        assert_eq!(
            while_closing.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Deferred,
            "child handle closure was published too early: {while_closing:?}"
        );
        assert!(
            while_closing
                .evidence
                .details
                .iter()
                .any(|detail| detail.contains("live child handle")),
            "missing live-child evidence: {while_closing:?}"
        );
        assert_eq!(
            root.cleanup(std::time::Duration::from_secs(5))
                .disposition
                .report_status(),
            hoimin_core::DiskCleanupStatus::Clean
        );
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

        assert_eq!(
            cleanup.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert_eq!(reclaim.reclaimed_roots, 0);
        assert_ne!(reclaim.details, Vec::<hoimin_core::JanitorDetail>::new());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        drop(held);
        assert_eq!(
            root.cleanup(std::time::Duration::from_secs(5))
                .disposition
                .report_status(),
            hoimin_core::DiskCleanupStatus::Clean
        );
    }

    #[test]
    fn cleanup_retry_uses_contention_reason_even_when_details_change() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let held = super::CoordinatorLockGuard::acquire(&coordinator).unwrap();
        let mut cleanup = root.cleanup(std::time::Duration::from_secs(5));
        cleanup.evidence.details = vec!["translated diagnostic without lock wording".to_owned()];
        assert!(cleanup.retryable_contention());
        drop(held);
        let live_root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = live_root.create_child("live-").unwrap();
        let mut cleanup = live_root.cleanup(std::time::Duration::from_secs(5));
        cleanup.evidence.details =
            vec!["coordinator process lock is busy; coordinator lock failed".to_owned()];
        assert!(!cleanup.retryable_contention());
        drop(child);
        assert_eq!(
            live_root
                .cleanup(std::time::Duration::from_secs(5))
                .disposition
                .report_status(),
            hoimin_core::DiskCleanupStatus::Clean
        );
    }

    #[test]
    fn file_lock_contention_retries_without_diagnostic_details() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let competing =
            super::open_regular_file_nofollow(&coordinator.dir, COORDINATOR_FILE).unwrap();
        std::fs::File::try_lock(&competing).unwrap();

        let mut cleanup = root.cleanup(std::time::Duration::from_secs(5));
        cleanup.evidence.details.clear();

        assert_eq!(
            cleanup.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert!(cleanup.retryable_contention());
        assert!(root.path().exists());
        drop(competing);
        assert_eq!(
            root.cleanup(std::time::Duration::from_secs(5))
                .disposition
                .report_status(),
            hoimin_core::DiskCleanupStatus::Clean
        );
    }

    #[test]
    fn cleanup_removes_a_tree_deeper_than_the_meter_limit() {
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
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{record:?}"
        );
        assert!(record.evidence.examined_entries >= 130);
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
        let identity = super::directory_identity(child.dir.as_ref().unwrap()).unwrap();

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
            root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()),)
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
            resumed.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{resumed:?}"
        );
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_cleanup_reports_the_verified_deleting_root_with_bounded_detail() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        assert_eq!(
            root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()),)
                .unwrap(),
            super::ClaimResult::Claimed
        );

        let record = root.interrupted_cleanup_record(true, "x".repeat(8 * 1024));

        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Failed
        );
        assert_eq!(
            record.evidence.remaining_root,
            Some(root.path().parent().unwrap().join(&deleting))
        );
        assert_eq!(record.evidence.details.len(), 1);
        assert!(record.evidence.details[0].len() <= super::MAX_DIAGNOSTIC_DETAIL_BYTES);
        assert_eq!(record.evidence.omitted_detail_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_cleanup_records_unverifiable_namespace_evidence() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = root.path().file_name().unwrap();
        coordinator
            .dir
            .rename(active, &coordinator.dir, ".parked-interrupted-root")
            .unwrap();

        let record = root.interrupted_cleanup_record(true, "cleanup stopped".to_owned());

        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert!(record.evidence.remaining_root.is_none());
        assert!(
            record
                .evidence
                .details
                .iter()
                .any(|detail| { detail.contains("cleanup root namespace entry was not found") })
        );
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
        root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()))
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
            root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()),)
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
            root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()),)
                .unwrap(),
            super::ClaimResult::Claimed
        );
        root.dir.remove_file(super::HEARTBEAT_FILE).unwrap();
        assert!(super::deleting_has_only_lease(&root.dir));
        // Model a parallel test's fork retaining the lease description until exec.
        #[cfg(unix)]
        let _inherited_lease = root
            .lease
            .lock()
            .unwrap()
            .as_deref()
            .unwrap()
            .try_clone()
            .unwrap();
        // Establish an abandoned, unlocked fixture even while that copy stays open.
        {
            let lease = root.lease.lock().unwrap();
            std::fs::File::unlock(lease.as_deref().unwrap()).unwrap();
        }
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
            root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()),)
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
        drop(super::create_owned_directory(&coordinator.dir, &deleting).unwrap());

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
        let future = std::time::SystemTime::now() + std::time::Duration::from_hours(25);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 0);
        assert_eq!(
            report.unresolved_by_reason[&hoimin_core::JanitorReason::HeartbeatInvalid],
            1
        );
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
        assert_eq!(
            original
                .claim_with_handle_handoff(&coordinator.dir, &original_name, &parked_name, &|| Ok(
                    ()
                ),)
                .unwrap(),
            super::ClaimResult::Claimed
        );
        assert_eq!(
            replacement
                .claim_with_handle_handoff(
                    &coordinator.dir,
                    &replacement_name,
                    &original_name,
                    &|| Ok(()),
                )
                .unwrap(),
            super::ClaimResult::Claimed
        );
        let copied_marker = super::LeaseMarker {
            schema: super::LEASE_SCHEMA,
            run_id: original.run_id.clone(),
            created_unix_seconds: 0,
            owner: original.owner,
        };
        let replacement_lease = replacement.lease.lock().unwrap();
        let marker = replacement_lease.as_deref().unwrap();
        std::fs::File::unlock(marker).unwrap();
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).truncate(true);
        let mut marker_writer = replacement
            .dir
            .open_with(super::LEASE_FILE, &options)
            .unwrap();
        serde_json::to_writer(&mut marker_writer, &copied_marker).unwrap();
        marker_writer.sync_all().unwrap();
        drop(marker_writer);
        std::fs::File::try_lock(marker).unwrap();
        let claim = original.claim_with_handle_handoff(
            &coordinator.dir,
            &original_name,
            &deleting_name,
            &|| Ok(()),
        );

        assert!(claim.is_err());
        assert!(coordinator.dir.symlink_metadata(&original_name).is_ok());
        assert!(coordinator.dir.symlink_metadata(&deleting_name).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn failed_windows_claim_restores_the_exact_locked_lease_and_heartbeat() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active_name = root.path().file_name().unwrap().to_owned();
        let lease_identity = {
            let lease = root.lease.lock().unwrap();
            super::file_identity(lease.as_deref().unwrap()).unwrap()
        };
        let heartbeat_identity = {
            let heartbeat = root.heartbeat.lock().unwrap();
            super::file_identity(heartbeat.as_ref().unwrap()).unwrap()
        };

        let claim =
            root.claim_with_handle_handoff(&coordinator.dir, &active_name, "invalid/name", &|| {
                Ok(())
            });

        assert!(claim.is_err());
        assert!(coordinator.dir.symlink_metadata(&active_name).is_ok());
        root.refresh_heartbeat().unwrap();
        let lease = root.lease.lock().unwrap();
        assert_eq!(
            super::file_identity(lease.as_deref().unwrap()).unwrap(),
            lease_identity
        );
        let competing = super::open_regular_file_nofollow(&root.dir, super::LEASE_FILE).unwrap();
        let lock_error = std::fs::File::try_lock(&competing).unwrap_err();
        assert!(
            matches!(lock_error, std::fs::TryLockError::WouldBlock),
            "{lock_error}"
        );
        drop(competing);
        drop(lease);
        let heartbeat = root.heartbeat.lock().unwrap();
        assert_eq!(
            super::file_identity(heartbeat.as_ref().unwrap()).unwrap(),
            heartbeat_identity
        );
        drop(heartbeat);

        let cleanup = root.cleanup(std::time::Duration::from_secs(5));
        assert_eq!(
            cleanup.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Clean,
            "{cleanup:?}"
        );
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
        // A concurrent fork can retain the shared lease description until exec.
        // Establish the unlocked premise explicitly instead of relying on close.
        #[cfg(unix)]
        let _inherited_lease = root
            .lease
            .lock()
            .unwrap()
            .as_deref()
            .unwrap()
            .try_clone()
            .unwrap();
        {
            let lease = root.lease.lock().unwrap();
            std::fs::File::unlock(lease.as_deref().unwrap()).unwrap();
        }
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert_eq!(report.preserved_roots, 0, "{report:?}");
        assert!(!published.exists());
    }

    #[cfg(windows)]
    #[test]
    fn round_six_abandoned_cleanup_ready_active_root_consumes_claim_pins() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        root.mark_cleanup_ready().unwrap();
        drop(root);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert_eq!(report.preserved_roots, 0, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&active).is_err());
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[tokio::test]
    async fn live_managed_child_keeps_the_root_lease_after_the_owner_is_dropped() {
        const COMPLETED: &str = "hoimin managed-child lease assertions completed";
        const CHILD_MARKER: &str = "HOIMIN_ISOLATED_MANAGED_CHILD_LEASE_TEST";
        // Other parallel tests can fork while this fixture owns its lease.
        // Their inherited descriptor retains flock until exec, even after
        // our last local Arc<File> is dropped. Create the fixture only in
        // a dedicated process with no unrelated tests spawning children.
        let output = tokio::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("workspace::owned::tests::managed_child_lease_isolated_child")
            .arg("--ignored")
            .arg("--test-threads=1")
            .arg("--nocapture")
            .env(CHILD_MARKER, "1")
            .kill_on_drop(true)
            .output();
        let output = tokio::time::timeout(std::time::Duration::from_secs(30), output)
            .await
            .expect("isolated managed-child lease test timed out")
            .expect("isolated managed-child lease test failed to start");
        assert!(
            output.status.success(),
            "isolated managed-child lease test failed: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(COMPLETED),
            "isolated managed-child lease test did not complete its assertions: {output:?}",
        );
    }

    #[tokio::test]
    #[ignore = "subprocess fixture for managed child lease"]
    async fn managed_child_lease_isolated_child() {
        const COMPLETED: &str = "hoimin managed-child lease assertions completed";

        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let child = root.create_child("worker-").unwrap();
        let published = root.path().to_owned();
        root.mark_cleanup_ready().unwrap();
        drop(root);

        let live = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(live.reclaimed_roots, 0, "{live:?}");
        assert_eq!(live.preserved_roots, 1, "{live:?}");
        assert!(published.exists());

        drop(child);
        let settled = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
        assert_eq!(settled.reclaimed_roots, 1, "{settled:?}");
        assert!(!published.exists());
        println!("{COMPLETED}");
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
        let future = std::time::SystemTime::now() + std::time::Duration::from_hours(25);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.preserved_roots, 1);
        assert!(published.exists());
    }

    #[test]
    fn retained_deleting_root_is_preserved_even_when_cleanup_ready() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.retain().unwrap();
        root.mark_cleanup_ready().unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let deleting = format!("{}{}", super::DELETING_PREFIX, root.run_id);
        assert_eq!(
            root.claim_with_handle_handoff(&coordinator.dir, &active, &deleting, &|| Ok(()),)
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
        // Model a concurrent fork retaining the shared lease description until exec.
        #[cfg(unix)]
        let _inherited_lease = root
            .lease
            .lock()
            .unwrap()
            .as_deref()
            .unwrap()
            .try_clone()
            .unwrap();
        // Closing the owner's handle alone does not establish an unlocked fixture.
        {
            let lease = root.lease.lock().unwrap();
            std::fs::File::unlock(lease.as_deref().unwrap()).unwrap();
        }
        drop(root);
        coordinator
            .dir
            .rename(&active, &coordinator.dir, &staging)
            .unwrap();
        let future = std::time::SystemTime::now() + std::time::Duration::from_hours(25);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&staging).is_err());
    }

    macro_rules! young_pre_lease_projection_case {
        ($name:ident, $verbose:expr, $details:literal, $suppressed:literal) => {
            #[test]
            fn $name() {
                // Arrange: a crash after mkdir but before lease creation.
                let parent = tempfile::tempdir().unwrap();
                let coordinator =
                    ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap())
                        .unwrap();
                let staging = format!("{}{}", super::STAGING_PREFIX, uuid::Uuid::new_v4());
                drop(super::create_owned_directory(&coordinator.dir, &staging).unwrap());

                // Act.
                let mut report =
                    ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
                report.project_details($verbose);

                // Assert: the same ordinary hold is retained in either projection.
                assert_eq!(report.inspected_roots, 1);
                assert_eq!(report.preserved_roots, 1, "{report:?}");
                assert_eq!(
                    report.preserved_by_reason,
                    std::collections::BTreeMap::from([(hoimin_core::JanitorReason::Young, 1)])
                );
                assert_eq!(report.unresolved_roots, 0);
                assert!(report.unresolved_by_reason.is_empty());
                assert_eq!(report.reclaimed_roots, 0);
                assert_eq!(report.removed_entries, 0);
                assert_eq!(report.details.len(), $details);
                assert_eq!(
                    report.detail_omissions.suppressed_normal_details,
                    $suppressed
                );
                if $verbose {
                    assert_eq!(report.details[0].reason, hoimin_core::JanitorReason::Young);
                    assert!(report.details[0].message.contains(&staging));
                }
                let candidate = super::open_owned_directory(&coordinator.dir, &staging).unwrap();
                assert!(super::inspect_empty_directory(&candidate).unwrap());
            }
        };
    }
    young_pre_lease_projection_case!(
        young_pre_lease_staging_default_suppresses_normal_detail,
        false,
        0,
        1
    );
    young_pre_lease_projection_case!(
        young_pre_lease_staging_verbose_retains_normal_detail,
        true,
        1,
        0
    );

    macro_rules! invalid_pre_lease_case {
        ($name:ident, $prefix:expr, $entry:expr, $hours:expr) => {
            #[test]
            fn $name() {
                // Arrange: absent or malformed lease outside the recognized empty staging state.
                let parent = tempfile::tempdir().unwrap();
                let coordinator =
                    ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap())
                        .unwrap();
                let name = format!("{}{}", $prefix, uuid::Uuid::new_v4());
                let candidate = super::create_owned_directory(&coordinator.dir, &name).unwrap();
                if let Some(entry) = $entry {
                    candidate.create_dir(entry).unwrap();
                }
                drop(candidate);
                let now = std::time::SystemTime::now() + std::time::Duration::from_hours($hours);

                // Act.
                let mut report = ManagedRunRoot::reclaim_abandoned(&coordinator, now);
                report.project_details(false);

                // Assert: still unresolved and visible by default; no deletion.
                assert_eq!(report.inspected_roots, 1);
                assert_eq!(report.preserved_roots, 0);
                assert_eq!(report.unresolved_roots, 1, "{report:?}");
                assert_eq!(
                    report.unresolved_by_reason[&hoimin_core::JanitorReason::LeaseInvalid],
                    1
                );
                assert_eq!(report.reclaimed_roots, 0);
                assert_eq!(report.removed_entries, 0);
                assert!(report.details.iter().any(|detail| detail.reason
                    == hoimin_core::JanitorReason::LeaseInvalid
                    && detail.message.contains(&name)));
                assert!(coordinator.dir.symlink_metadata(&name).is_ok());
            }
        };
    }
    invalid_pre_lease_case!(
        young_nonempty_staging_without_lease_is_unresolved,
        super::STAGING_PREFIX,
        Some("payload"),
        0
    );
    invalid_pre_lease_case!(
        stale_nonempty_staging_without_lease_is_unresolved,
        super::STAGING_PREFIX,
        Some("payload"),
        25
    );
    invalid_pre_lease_case!(
        empty_active_root_without_lease_is_unresolved,
        super::ACTIVE_PREFIX,
        None::<&str>,
        0
    );
    invalid_pre_lease_case!(
        young_staging_with_directory_lease_is_unresolved,
        super::STAGING_PREFIX,
        Some(super::LEASE_FILE),
        0
    );

    #[cfg(unix)]
    #[test]
    fn inaccessible_empty_staging_is_unresolved() {
        use std::os::unix::fs::PermissionsExt;
        // Arrange: directory search permission is required to acquire the candidate.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let staging = format!("{}{}", super::STAGING_PREFIX, uuid::Uuid::new_v4());
        drop(super::create_owned_directory(&coordinator.dir, &staging).unwrap());
        let path = coordinator.path.join(&staging);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        // Act.
        let mut report =
            ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        report.project_details(false);

        // Assert: failed candidate inspection cannot masquerade as Young.
        assert_eq!(report.preserved_roots, 0, "{report:?}");
        assert_eq!(report.unresolved_roots, 1);
        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(report.removed_entries, 0);
        assert!(
            report.details.iter().any(
                |detail| detail.reason == hoimin_core::JanitorReason::CandidateInspectionFailed
            ),
            "{report:?}"
        );
        assert!(path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_staging_age_inspection_retains_typed_failure() {
        use std::os::unix::fs::PermissionsExt;
        // Arrange: retain a capability acquired before search permission is removed.
        let parent = tempfile::tempdir().unwrap();
        let candidate =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();
        let path = Utf8Path::from_path(parent.path()).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut ledger = super::ReclaimLedger::default();

        // Act.
        let result = super::directory_timestamp_is_stale(
            &candidate,
            std::time::SystemTime::now(),
            &mut ledger,
            path,
        );
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut report = ledger.finish();
        report.project_details(false);

        // Assert: callers receive the failure, and default output retains its cause.
        assert_eq!(
            result.err().unwrap().reason,
            hoimin_core::JanitorReason::AgeUnverifiable
        );
        assert_eq!(report.details.len(), 1);
        assert_eq!(
            report.details[0].reason,
            hoimin_core::JanitorReason::AgeUnverifiable
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_empty_staging_inspection_retains_issue_detail() {
        use std::os::unix::fs::PermissionsExt;
        // Arrange: keep the opened capability while removing directory read access.
        let parent = tempfile::tempdir().unwrap();
        let candidate =
            cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
                .unwrap();
        let path = Utf8Path::from_path(parent.path()).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o100)).unwrap();
        let mut ledger = super::ReclaimLedger::default();

        // Act.
        let empty = super::janitor_empty_probe(&candidate, &mut ledger, path);
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut report = ledger.finish();
        report.project_details(false);

        // Assert: no positive empty evidence, and the failure survives default projection.
        assert!(!empty);
        assert_eq!(report.details.len(), 1);
        assert_eq!(
            report.details[0].reason,
            hoimin_core::JanitorReason::CandidateInspectionFailed
        );
    }

    #[test]
    fn empty_staging_root_from_a_pre_lease_crash_is_reclaimed_after_a_day() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let staging = format!("{}{}", super::STAGING_PREFIX, uuid::Uuid::new_v4());
        drop(super::create_owned_directory(&coordinator.dir, &staging).unwrap());
        let future = std::time::SystemTime::now() + std::time::Duration::from_hours(25);

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&staging).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn round_six_empty_unmarked_deleting_root_consumes_candidate_pin() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let deleting = format!("{}{}", super::DELETING_PREFIX, uuid::Uuid::new_v4());
        drop(super::create_owned_directory(&coordinator.dir, &deleting).unwrap());

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert_eq!(report.preserved_roots, 0, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn round_six_review_candidate_delete_pin_can_clear_readonly_root() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let deleting = format!("{}{}", super::DELETING_PREFIX, uuid::Uuid::new_v4());
        drop(super::create_owned_directory(&coordinator.dir, &deleting).unwrap());
        let deleting_path = coordinator.path.join(&deleting);
        let mut permissions = std::fs::metadata(&deleting_path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&deleting_path, permissions).unwrap();

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());

        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert_eq!(report.preserved_roots, 0, "{report:?}");
        assert!(coordinator.dir.symlink_metadata(&deleting).is_err());
    }

    #[test]
    fn lease_only_staging_root_is_reclaimed_after_a_day() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = format!("{}{}", super::ACTIVE_PREFIX, root.run_id);
        let staging = format!("{}{}", super::STAGING_PREFIX, root.run_id);
        // Model a parallel test's fork retaining the shared lease description until exec.
        #[cfg(unix)]
        let _inherited_lease = root
            .lease
            .lock()
            .unwrap()
            .as_deref()
            .unwrap()
            .try_clone()
            .unwrap();
        // A crash closes the process-owned heartbeat capability before the janitor observes
        // the lease-only staging state.
        drop(root.heartbeat.lock().unwrap().take());
        root.dir.remove_file(super::HEARTBEAT_FILE).unwrap();
        // Establish the abandoned, unlocked fixture even while a fork retains its copy.
        {
            let lease = root.lease.lock().unwrap();
            std::fs::File::unlock(lease.as_deref().unwrap()).unwrap();
        }
        drop(root);
        coordinator
            .dir
            .rename(&active, &coordinator.dir, &staging)
            .unwrap();
        let future = std::time::SystemTime::now() + std::time::Duration::from_hours(25);

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
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use super::WorkspaceError;
use hoimin_core::{DiskCleanupStatus, JanitorReason, JanitorScan};
#[path = "owned/janitor.rs"]
mod janitor;
use janitor::{DeletionProgress, ReclaimLedger};
pub(crate) type ReclaimReport = hoimin_core::JanitorSummary;

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
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{Mode, OFlags, ResolveFlags};

        let readable = rustix::fs::openat2(
            dir,
            c".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
        )
        .map_err(std::io::Error::from)?;
        rustix::fs::Dir::new(readable)
            .map(OwnedDirectoryEntries)
            .map_err(std::io::Error::from)
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        rustix::fs::Dir::read_from(dir)
            .map(OwnedDirectoryEntries)
            .map_err(std::io::Error::from)
    }
    #[cfg(windows)]
    {
        let directory = dir.try_clone()?.into_std_file();
        Ok(OwnedDirectoryEntries(
            super::root::windows::DirectoryEntries::open(directory),
        ))
    }
}

const MANAGED_DIR: &str = "hoimin-workspaces-v2";
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
// Full BLAKE3 digest occupies the final 32 bytes of each fixed-size slot.
const COORDINATOR_DIGEST_OFFSET: usize = COORDINATOR_SLOT_BYTES - blake3::OUT_LEN;
const COORDINATOR_CURSOR_BYTES: usize = COORDINATOR_DIGEST_OFFSET - 22;
type LocalCoordinatorLock = Arc<Mutex<()>>;
type LocalCoordinatorRegistry = Mutex<HashMap<(u64, u64), Weak<Mutex<()>>>>;
type LocalCoordinatorRegistryGuard<'a> =
    std::sync::MutexGuard<'a, HashMap<(u64, u64), Weak<Mutex<()>>>>;
static COORDINATOR_LOCAL_LOCKS: OnceLock<LocalCoordinatorRegistry> = OnceLock::new();
const COORDINATOR_MAGIC: &[u8; 8] = b"HMCUR001";
const COORDINATOR_SCHEMA: u32 = 2;
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
const STALE_AFTER: Duration = Duration::from_hours(24);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OwnerKind {
    PublicExecution,
    PublicDelivery,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
        Self::try_acquire(coordinator)?.ok_or_else(|| {
            WorkspaceError::io(
                "lock coordinator",
                &coordinator.path,
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "coordinator is busy in this process or another process",
                ),
            )
        })
    }

    fn try_acquire(
        coordinator: &'a ManagedRootCoordinator,
    ) -> Result<Option<Self>, WorkspaceError> {
        let local = match coordinator.local_lock.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(WorkspaceError::StatePoisoned);
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                return Ok(None);
            }
        };
        match File::try_lock(&coordinator.file) {
            Ok(()) => Ok(Some(Self {
                file: &coordinator.file,
                _local: local,
                locked: true,
            })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(WorkspaceError::io(
                "lock coordinator",
                &coordinator.path,
                error,
            )),
        }
    }

    fn acquire_until(
        coordinator: &'a ManagedRootCoordinator,
        deadline: std::time::Instant,
    ) -> Result<Self, WorkspaceError> {
        loop {
            ensure_bootstrap_deadline(deadline, &coordinator.path, "lock coordinator")?;
            match coordinator.local_lock.try_lock() {
                Ok(local) => match File::try_lock(&coordinator.file) {
                    Ok(()) => {
                        if let Err(error) = ensure_bootstrap_deadline(
                            deadline,
                            &coordinator.path,
                            "lock coordinator",
                        ) {
                            let _ = File::unlock(&coordinator.file);
                            return Err(error);
                        }
                        return Ok(Self {
                            file: &coordinator.file,
                            _local: local,
                            locked: true,
                        });
                    }
                    Err(std::fs::TryLockError::WouldBlock) => drop(local),
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
        File::unlock(self.file)
            .map_err(|error| WorkspaceError::io("unlock coordinator", path, error))?;
        self.locked = false;
        Ok(())
    }
}

impl Drop for CoordinatorLockGuard<'_> {
    fn drop(&mut self) {
        if self.locked {
            let _ = File::unlock(self.file);
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct MarkerEvidence {
    identity: (u64, u64),
    marker: LeaseMarker,
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

#[derive(Clone, Copy)]
enum OwnedFileAccess {
    ReadWrite,
    Write,
}

#[cfg(unix)]
fn create_owned_directory(
    parent: &cap_std::fs::Dir,
    name: &str,
) -> std::io::Result<cap_std::fs::Dir> {
    create_owned_directory_with(
        parent,
        name,
        |parent, name| {
            rustix::fs::mkdirat(parent, name, rustix::fs::Mode::from_raw_mode(0o700))
                .map_err(std::io::Error::from)
        },
        |parent, name| parent.symlink_metadata(name),
        |parent, name| open_owned_directory(parent, name),
        rollback_new_owned_directory,
    )
}

#[cfg(any(unix, test))]
fn create_owned_directory_with(
    parent: &cap_std::fs::Dir,
    name: &str,
    create: impl FnOnce(&cap_std::fs::Dir, &str) -> std::io::Result<()>,
    inspect: impl FnOnce(&cap_std::fs::Dir, &str) -> std::io::Result<cap_std::fs::Metadata>,
    open: impl FnOnce(&cap_std::fs::Dir, &str) -> std::io::Result<cap_std::fs::Dir>,
    rollback: impl FnOnce(&cap_std::fs::Dir, &str, (u64, u64), Duration) -> std::io::Result<()>,
) -> std::io::Result<cap_std::fs::Dir> {
    create(parent, name)?;
    let rollback_deadline = std::time::Instant::now() + OWNER_CLEANUP_BUDGET;
    let mut expected_identity = None;
    let result: std::io::Result<cap_std::fs::Dir> = (|| {
        let metadata = inspect(parent, name)?;
        let identity = metadata_identity(&metadata);
        expected_identity = Some(identity);
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::other(
                "new owned directory changed before it could be opened",
            ));
        }
        let directory = open(parent, name)?;
        let opened_identity = directory_identity(&directory)?;
        if opened_identity != identity {
            return Err(std::io::Error::other(
                "new owned directory identity changed while opening it",
            ));
        }
        Ok(directory)
    })();
    match result {
        Ok(directory) => Ok(directory),
        Err(primary) => Err(error_after_created_directory_rollback_with(
            parent,
            name,
            expected_identity,
            rollback_deadline,
            primary,
            rollback,
        )),
    }
}

fn error_after_new_directory_rollback_with<E>(
    primary: E,
    deadline: std::time::Instant,
    now: impl FnOnce() -> std::time::Instant,
    rollback: impl FnOnce(Duration) -> std::io::Result<()>,
    append_secondary: impl FnOnce(E, std::io::Error) -> E,
) -> E {
    let remaining = deadline.saturating_duration_since(now());
    let cleanup = if remaining.is_zero() {
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "new-directory rollback deadline exhausted before cleanup",
        ))
    } else {
        rollback(remaining)
    };
    match cleanup {
        Ok(()) => primary,
        Err(secondary) => append_secondary(primary, secondary),
    }
}

fn created_directory_identity_unavailable() -> std::io::Error {
    std::io::Error::other(
        "bounded cleanup cannot proceed without the created directory identity; unverified name left untouched",
    )
}

#[cfg(any(unix, test))]
fn error_after_created_directory_rollback_with(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_identity: Option<(u64, u64)>,
    rollback_deadline: std::time::Instant,
    primary: std::io::Error,
    rollback: impl FnOnce(&cap_std::fs::Dir, &str, (u64, u64), Duration) -> std::io::Result<()>,
) -> std::io::Error {
    error_after_new_directory_rollback_with(
        primary,
        rollback_deadline,
        std::time::Instant::now,
        |remaining| match expected_identity {
            Some(expected_identity) => rollback(parent, name, expected_identity, remaining),
            None => Err(created_directory_identity_unavailable()),
        },
        |primary, secondary| created_directory_rollback_error(&primary, &secondary),
    )
}

#[cfg(any(unix, test))]
fn created_directory_rollback_error(
    primary: &std::io::Error,
    secondary: &std::io::Error,
) -> std::io::Error {
    std::io::Error::new(
        primary.kind(),
        format!("{primary}; secondary created-directory rollback failure: {secondary}"),
    )
}

fn rollback_new_owned_directory(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_identity: (u64, u64),
    budget: Duration,
) -> std::io::Result<()> {
    let removal = remove_claimed_tree_bounded(parent, name, Some(expected_identity), budget)?;
    if removal.complete {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "bounded cleanup did not remove the newly created directory",
        ))
    }
}

fn append_workspace_new_directory_rollback_error(
    primary: WorkspaceError,
    fallback_path: &Utf8Path,
    secondary: &std::io::Error,
) -> WorkspaceError {
    match primary {
        WorkspaceError::Io {
            operation,
            path,
            message,
        } => WorkspaceError::Io {
            operation,
            path,
            message: format!("{message}; secondary new-directory rollback failure: {secondary}"),
        },
        primary => WorkspaceError::io(
            "construct managed directory",
            fallback_path,
            format!("{primary}; secondary new-directory rollback failure: {secondary}"),
        ),
    }
}

fn error_after_identity_bound_new_directory_rollback(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_identity: (u64, u64),
    rollback_deadline: std::time::Instant,
    path: &Utf8Path,
    primary: WorkspaceError,
) -> WorkspaceError {
    error_after_new_directory_rollback_with(
        primary,
        rollback_deadline,
        std::time::Instant::now,
        |remaining| rollback_new_owned_directory(parent, name, expected_identity, remaining),
        |primary, secondary| {
            append_workspace_new_directory_rollback_error(primary, path, &secondary)
        },
    )
}

fn error_after_unpublished_directory_failure(
    parent: &cap_std::fs::Dir,
    name: &str,
    directory: cap_std::fs::Dir,
    expected_identity: Option<(u64, u64)>,
    rollback_deadline: std::time::Instant,
    path: &Utf8Path,
    primary: WorkspaceError,
) -> WorkspaceError {
    #[cfg(windows)]
    if expected_identity.is_none() {
        return error_after_new_directory_rollback_with(
            primary,
            rollback_deadline,
            std::time::Instant::now,
            |_remaining| windows::rollback_created(&directory),
            |primary, secondary| {
                append_workspace_new_directory_rollback_error(primary, path, &secondary)
            },
        );
    }

    drop(directory);
    error_after_new_directory_rollback_with(
        primary,
        rollback_deadline,
        std::time::Instant::now,
        |remaining| match expected_identity {
            Some(expected_identity) => {
                rollback_new_owned_directory(parent, name, expected_identity, remaining)
            }
            None => Err(created_directory_identity_unavailable()),
        },
        |primary, secondary| {
            append_workspace_new_directory_rollback_error(primary, path, &secondary)
        },
    )
}

#[cfg(windows)]
fn create_owned_directory(
    parent: &cap_std::fs::Dir,
    name: &str,
) -> std::io::Result<cap_std::fs::Dir> {
    let file = windows::create_relative_managed_directory(parent, std::ffi::OsStr::new(name))?;
    Ok(cap_std::fs::Dir::from_std_file(file))
}

#[cfg(unix)]
fn create_owned_file(
    parent: &cap_std::fs::Dir,
    name: &str,
    access: OwnedFileAccess,
) -> std::io::Result<File> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    if matches!(access, OwnedFileAccess::ReadWrite) {
        options.read(true);
    }
    configure_no_follow(&mut options);
    configure_secure_create(&mut options);
    parent
        .open_with(name, &options)
        .map(cap_std::fs::File::into_std)
}

#[cfg(windows)]
fn create_owned_file(
    parent: &cap_std::fs::Dir,
    name: &str,
    access: OwnedFileAccess,
) -> std::io::Result<File> {
    let access = match access {
        OwnedFileAccess::ReadWrite => windows::ManagedFileAccess::ReadWrite,
        OwnedFileAccess::Write => windows::ManagedFileAccess::Write,
    };
    windows::create_relative_managed_file(parent, std::ffi::OsStr::new(name), access)
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

#[cfg(windows)]
fn open_owned_directory_for_rename(
    parent: &cap_std::fs::Dir,
    name: impl AsRef<std::path::Path>,
) -> std::io::Result<cap_std::fs::Dir> {
    let directory =
        super::root::windows::open_directory_for_rename(parent, name.as_ref().as_os_str())?;
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
fn rename_open_owned_directory(
    parent: &cap_std::fs::Dir,
    source: &cap_std::fs::Dir,
    destination: &str,
    expected_identity: (u64, u64),
) -> std::io::Result<()> {
    rename_open_owned_directory_with_guard(parent, source, destination, expected_identity, &|| {
        Ok(())
    })
}

#[cfg(windows)]
fn rename_open_owned_directory_with_guard(
    parent: &cap_std::fs::Dir,
    source: &cap_std::fs::Dir,
    destination: &str,
    expected_identity: (u64, u64),
    before_next_operation: &impl Fn() -> Result<(), WorkspaceError>,
) -> std::io::Result<()> {
    super::root::windows::rename_open_entry_relative_with_guard(
        parent,
        source,
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
    ensure_bootstrap_deadline(deadline, &coordinator_path, "create coordinator")?;
    match create_owned_file(dir, COORDINATOR_FILE, OwnedFileAccess::ReadWrite) {
        Ok(file) => {
            let identity = file_identity(&file).map_err(|error| {
                WorkspaceError::io("identify new coordinator", &coordinator_path, error)
            })?;
            initialize_new_coordinator(dir, file, identity, &coordinator_path, deadline)
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

fn initialize_new_coordinator(
    dir: &cap_std::fs::Dir,
    mut file: File,
    identity: (u64, u64),
    coordinator_path: &Utf8Path,
    deadline: std::time::Instant,
) -> Result<File, WorkspaceError> {
    let initialized = (|| {
        ensure_bootstrap_deadline(deadline, coordinator_path, "create coordinator")?;
        lock_file_until(&file, deadline, coordinator_path)?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "initialize coordinator")?;
        let bytes = initial_coordinator_bytes();
        file.write_all(&bytes).map_err(|error| {
            WorkspaceError::io("initialize coordinator", coordinator_path, error)
        })?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "initialize coordinator")?;
        file.sync_all()
            .map_err(|error| WorkspaceError::io("flush coordinator", coordinator_path, error))?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "flush coordinator")?;
        secure_coordinator_file(&file, coordinator_path)?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "secure coordinator")?;
        validate_coordinator_file(&mut file, coordinator_path)?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "validate coordinator")?;
        File::unlock(&file)
            .map_err(|error| WorkspaceError::io("unlock coordinator", coordinator_path, error))?;
        ensure_bootstrap_deadline(deadline, coordinator_path, "unlock coordinator")
    })();
    match initialized {
        Ok(()) => Ok(file),
        Err(error) => {
            let _ = File::unlock(&file);
            drop(file);
            if let Err(cleanup) = remove_new_coordinator_if_identity(dir, identity) {
                return Err(WorkspaceError::io(
                    "rollback coordinator initialization",
                    coordinator_path,
                    format!("{error}; rollback failed: {cleanup}"),
                ));
            }
            Err(error)
        }
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
            File::unlock(&file).map_err(|error| {
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
        File::unlock(&file)
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
        match File::try_lock(file) {
            Ok(()) => {
                if let Err(error) =
                    ensure_bootstrap_deadline(deadline, path, "lock coordinator bootstrap")
                {
                    let _ = File::unlock(file);
                    return Err(error);
                }
                return Ok(());
            }
            Err(std::fs::TryLockError::WouldBlock) => {
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
    let digest = blake3::hash(&slot[..COORDINATOR_DIGEST_OFFSET]);
    slot[COORDINATOR_DIGEST_OFFSET..].copy_from_slice(digest.as_bytes());
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
        && slot[22 + cursor_len..COORDINATOR_DIGEST_OFFSET]
            .iter()
            .all(|byte| *byte == 0)
        && &slot[COORDINATOR_DIGEST_OFFSET..]
            == blake3::hash(&slot[..COORDINATOR_DIGEST_OFFSET]).as_bytes();
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
    lease: Mutex<Option<Arc<File>>>,
    heartbeat: Mutex<Option<File>>,
    lifecycle: Arc<Mutex<RootLifecycle>>,
    namespace: AtomicU8,
    #[cfg(windows)]
    rename_handle: Mutex<Option<cap_std::fs::Dir>>,
    #[cfg(windows)]
    cleanup_lease_evidence: MarkerEvidence,
    #[cfg(windows)]
    _mutation_barrier: windows::RootMutationBarrier,
}

#[derive(Debug, Default)]
struct RootLifecycle {
    cleanup_started: bool,
    cleanup_ready: bool,
    live_children: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum NamespaceLifecycle {
    Active = 0,
    Deleting = 1,
    Complete = 2,
}

impl NamespaceLifecycle {
    fn load(state: &AtomicU8) -> Self {
        match state.load(Ordering::Acquire) {
            0 => Self::Active,
            1 => Self::Deleting,
            2 => Self::Complete,
            _ => unreachable!("managed namespace lifecycle has an invalid value"),
        }
    }

    fn store(self, state: &AtomicU8) {
        state.store(self as u8, Ordering::Release);
    }
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

struct StagingPublication<'a> {
    name: &'a str,
    path: &'a Utf8Path,
    run_id: &'a str,
    owner: OwnerKind,
    dir: cap_std::fs::Dir,
    #[cfg(windows)]
    rename_handle: cap_std::fs::Dir,
}

struct PreRenamePublication<'a> {
    lease: File,
    heartbeat: File,
    publish_guard: CoordinatorLockGuard<'a>,
    staging_identity: (u64, u64),
    #[cfg(windows)]
    marker_evidence: (MarkerEvidence, MarkerEvidence),
    #[cfg(windows)]
    mutation_barrier: windows::RootMutationBarrier,
}

/// Workspace admission evidence, independent of bounded diagnostic details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CleanupRetryReason {
    CoordinatorContention,
}

/// Actual cleanup outcomes exclude report retention and delivery scheduling states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CleanupDisposition {
    Clean,
    Failed,
    Deferred(Option<CleanupRetryReason>),
}

impl CleanupDisposition {
    pub(crate) fn report_status(self) -> DiskCleanupStatus {
        match self {
            Self::Clean => DiskCleanupStatus::Clean,
            Self::Failed => DiskCleanupStatus::Failed,
            Self::Deferred(_) => DiskCleanupStatus::Deferred,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CleanupRecord {
    pub(crate) disposition: CleanupDisposition,
    pub(crate) evidence: CleanupEvidence,
}

impl CleanupRecord {
    /// Deadline, live-child and integrity deferrals remain janitor work.
    pub(crate) fn retryable_contention(&self) -> bool {
        matches!(
            self.disposition,
            CleanupDisposition::Deferred(Some(CleanupRetryReason::CoordinatorContention))
        )
    }
}

/// Evidence is independent of classification: an interrupted wait may not certify a path.
/// Display-only reports can retain this evidence without inventing a cleanup result.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CleanupEvidence {
    pub(crate) examined_entries: u64,
    pub(crate) removed_entries: u64,
    pub(crate) details: Vec<String>,
    pub(crate) omitted_detail_count: u64,
    pub(crate) remaining_root: Option<Utf8PathBuf>,
}

impl CleanupEvidence {
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
        Self::create_with_hooks(coordinator, owner, publish_hook, &directory_identity)
    }

    fn create_with_hooks(
        coordinator: &ManagedRootCoordinator,
        owner: OwnerKind,
        publish_hook: &impl Fn(PublishBoundary) -> Result<(), WorkspaceError>,
        created_identity: &impl Fn(&cap_std::fs::Dir) -> std::io::Result<(u64, u64)>,
    ) -> Result<Self, WorkspaceError> {
        let initialization_guard = CoordinatorLockGuard::acquire_until(
            coordinator,
            std::time::Instant::now() + JANITOR_SELECTION_BUDGET,
        )?;
        let run_id = uuid::Uuid::new_v4().to_string();
        let staging_name = format!("{STAGING_PREFIX}{run_id}");
        let staging_path = coordinator.path.join(&staging_name);
        let staging_dir = create_owned_directory(&coordinator.dir, &staging_name)
            .map_err(|error| WorkspaceError::io("create staging root", &coordinator.path, error))?;
        let rollback_deadline = std::time::Instant::now() + OWNER_CLEANUP_BUDGET;
        let mut staging_identity = None;
        #[cfg(windows)]
        let mut staging_work_dir = None;
        let preparation = (|| -> Result<(), WorkspaceError> {
            let identity = created_identity(&staging_dir).map_err(|error| {
                WorkspaceError::io("identify staging root", &staging_path, error)
            })?;
            staging_identity = Some(identity);
            publish_hook(PublishBoundary::StagingCreated)?;
            #[cfg(windows)]
            {
                staging_work_dir = Some(staging_dir.try_clone().map_err(|error| {
                    WorkspaceError::io("clone staging root capability", &staging_path, error)
                })?);
            }
            publish_hook(PublishBoundary::StagingOpened)?;
            #[cfg(unix)]
            let opened_staging_identity = directory_identity(&staging_dir);
            #[cfg(windows)]
            let opened_staging_identity = directory_identity(
                staging_work_dir
                    .as_ref()
                    .expect("Windows staging work capability was cloned"),
            );
            let opened_staging_identity = opened_staging_identity.map_err(|error| {
                WorkspaceError::io("identify staging root", &staging_path, error)
            })?;
            if opened_staging_identity != identity {
                return Err(WorkspaceError::InvalidPath {
                    path: staging_path.clone(),
                });
            }
            Ok(())
        })();
        if let Err(primary) = preparation {
            #[cfg(windows)]
            drop(staging_work_dir.take());
            return Err(error_after_unpublished_directory_failure(
                &coordinator.dir,
                &staging_name,
                staging_dir,
                staging_identity,
                rollback_deadline,
                &staging_path,
                primary,
            ));
        }
        let staging_identity = staging_identity.expect("staging preparation captured its identity");
        #[cfg(windows)]
        let staging_work_dir =
            staging_work_dir.expect("Windows staging work capability was cloned");
        let publication = StagingPublication {
            name: &staging_name,
            path: &staging_path,
            run_id: &run_id,
            owner,
            #[cfg(unix)]
            dir: staging_dir,
            #[cfg(windows)]
            dir: staging_work_dir,
            #[cfg(windows)]
            rename_handle: staging_dir,
        };
        Self::publish_staging(
            coordinator,
            publication,
            initialization_guard,
            rollback_deadline,
            staging_identity,
            publish_hook,
        )
    }

    #[allow(
        clippy::too_many_lines,
        reason = "lease publication keeps rollback adjacent to every post-rename failure"
    )]
    fn publish_staging(
        coordinator: &ManagedRootCoordinator,
        publication: StagingPublication<'_>,
        initialization_guard: CoordinatorLockGuard<'_>,
        rollback_deadline: std::time::Instant,
        staging_identity: (u64, u64),
        publish_hook: &impl Fn(PublishBoundary) -> Result<(), WorkspaceError>,
    ) -> Result<Self, WorkspaceError> {
        let StagingPublication {
            name: staging_name,
            path: staging_path,
            run_id,
            owner,
            dir: staging_dir,
            #[cfg(windows)]
            rename_handle,
        } = publication;
        let active_name = format!("{ACTIVE_PREFIX}{run_id}");
        let pre_rename = (|| -> Result<PreRenamePublication<'_>, WorkspaceError> {
            #[cfg(windows)]
            let mutation_barrier =
                windows::RootMutationBarrier::create(run_id).map_err(|error| {
                    WorkspaceError::io("create managed-root mutation barrier", staging_path, error)
                })?;
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
            let mut lease = create_owned_file(&staging_dir, LEASE_FILE, OwnedFileAccess::ReadWrite)
                .map_err(|error| WorkspaceError::io("create lease marker", &lease_path, error))?;
            serde_json::to_writer(&mut lease, &marker)
                .map_err(|error| WorkspaceError::io("write lease marker", &lease_path, error))?;
            lease
                .write_all(b"\n")
                .map_err(|error| WorkspaceError::io("write lease marker", &lease_path, error))?;
            lease
                .sync_all()
                .map_err(|error| WorkspaceError::io("flush lease marker", &lease_path, error))?;
            File::try_lock(&lease)
                .map_err(|error| WorkspaceError::io("lock lease marker", &lease_path, error))?;
            let heartbeat_path = staging_path.join(HEARTBEAT_FILE);
            let mut heartbeat =
                create_owned_file(&staging_dir, HEARTBEAT_FILE, OwnedFileAccess::Write).map_err(
                    |error| WorkspaceError::io("create heartbeat marker", &heartbeat_path, error),
                )?;
            serde_json::to_writer(&mut heartbeat, &marker).map_err(|error| {
                WorkspaceError::io("write heartbeat marker", &heartbeat_path, error)
            })?;
            heartbeat.write_all(b"\n").map_err(|error| {
                WorkspaceError::io("write heartbeat marker", &heartbeat_path, error)
            })?;
            heartbeat.sync_all().map_err(|error| {
                WorkspaceError::io("flush heartbeat marker", &heartbeat_path, error)
            })?;
            #[cfg(windows)]
            let marker_evidence = (
                MarkerEvidence {
                    identity: file_identity(&lease).map_err(|error| {
                        WorkspaceError::io("identify lease marker", &lease_path, error)
                    })?,
                    marker: marker.clone(),
                },
                MarkerEvidence {
                    identity: file_identity(&heartbeat).map_err(|error| {
                        WorkspaceError::io("identify heartbeat marker", &heartbeat_path, error)
                    })?,
                    marker: marker.clone(),
                },
            );
            initialization_guard.unlock(&coordinator.path)?;
            let publish_guard = CoordinatorLockGuard::acquire_until(
                coordinator,
                std::time::Instant::now() + JANITOR_SELECTION_BUDGET,
            )?;
            let observed_identity = directory_identity(&staging_dir).map_err(|error| {
                WorkspaceError::io("identify staging root", staging_path, error)
            })?;
            if observed_identity != staging_identity {
                return Err(WorkspaceError::InvalidPath {
                    path: staging_path.to_owned(),
                });
            }
            Ok(PreRenamePublication {
                lease,
                heartbeat,
                publish_guard,
                staging_identity,
                #[cfg(windows)]
                marker_evidence,
                #[cfg(windows)]
                mutation_barrier,
            })
        })();
        let PreRenamePublication {
            lease,
            heartbeat,
            publish_guard,
            staging_identity,
            #[cfg(windows)]
                marker_evidence: publication_marker_evidence,
            #[cfg(windows)]
            mutation_barrier,
        } = match pre_rename {
            Ok(pre_rename) => pre_rename,
            Err(primary) => {
                drop(staging_dir);
                #[cfg(windows)]
                drop(rename_handle);
                return Err(error_after_identity_bound_new_directory_rollback(
                    &coordinator.dir,
                    staging_name,
                    staging_identity,
                    rollback_deadline,
                    staging_path,
                    primary,
                ));
            }
        };
        #[cfg(windows)]
        {
            drop(lease);
            drop(heartbeat);
        }
        #[cfg(unix)]
        let publish = rename_owned_directory(
            &coordinator.dir,
            staging_name,
            &active_name,
            staging_identity,
        )
        .map_err(|error| WorkspaceError::io("publish managed root", staging_path, error));
        #[cfg(windows)]
        let publish = rename_open_owned_directory(
            &coordinator.dir,
            &rename_handle,
            &active_name,
            staging_identity,
        )
        .map_err(|error| WorkspaceError::io("publish managed root", staging_path, error));
        if let Err(primary) = publish {
            drop(publish_guard);
            #[cfg(unix)]
            {
                drop(lease);
                drop(heartbeat);
            }
            drop(staging_dir);
            #[cfg(windows)]
            {
                drop(rename_handle);
                drop(mutation_barrier);
            }
            return Err(error_after_identity_bound_new_directory_rollback(
                &coordinator.dir,
                staging_name,
                staging_identity,
                rollback_deadline,
                staging_path,
                primary,
            ));
        }
        #[cfg(windows)]
        if let Err(error) = publish_hook(PublishBoundary::RenamedActive) {
            let restored = rollback_closed_published_root(
                coordinator,
                staging_name,
                &active_name,
                &staging_dir,
                &rename_handle,
                staging_identity,
                &publication_marker_evidence.0,
            );
            if restored {
                drop(staging_dir);
                drop(rename_handle);
                drop(mutation_barrier);
                return Err(error_after_identity_bound_new_directory_rollback(
                    &coordinator.dir,
                    staging_name,
                    staging_identity,
                    rollback_deadline,
                    staging_path,
                    error,
                ));
            }
            return Err(error);
        }
        #[cfg(windows)]
        let (lease, heartbeat) = match reopen_published_markers(
            &staging_dir,
            &publication_marker_evidence,
            &coordinator.path.join(&active_name),
        ) {
            Ok(markers) => markers,
            Err(error) => {
                let restored = rollback_closed_published_root(
                    coordinator,
                    staging_name,
                    &active_name,
                    &staging_dir,
                    &rename_handle,
                    staging_identity,
                    &publication_marker_evidence.0,
                );
                if restored {
                    drop(staging_dir);
                    drop(rename_handle);
                    drop(mutation_barrier);
                    return Err(error_after_identity_bound_new_directory_rollback(
                        &coordinator.dir,
                        staging_name,
                        staging_identity,
                        rollback_deadline,
                        staging_path,
                        error,
                    ));
                }
                return Err(error);
            }
        };
        let unlock = publish_guard.unlock(&coordinator.path);
        if let Err(error) = unlock {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                &staging_dir,
                &lease,
            );
            return Err(error);
        }
        #[cfg(unix)]
        if let Err(error) = publish_hook(PublishBoundary::RenamedActive) {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                &staging_dir,
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
                    &staging_dir,
                    &lease,
                );
                return Err(WorkspaceError::io("open published root", &path, error));
            }
        };
        let active_identity = match directory_identity(&dir) {
            Ok(identity) => identity,
            Err(error) => {
                drop(dir);
                rollback_published_root(
                    coordinator,
                    &active_name,
                    run_id,
                    owner,
                    &staging_dir,
                    &lease,
                );
                return Err(WorkspaceError::io("identify published root", &path, error));
            }
        };
        if active_identity != staging_identity {
            drop(dir);
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                &staging_dir,
                &lease,
            );
            return Err(WorkspaceError::InvalidPath { path });
        }
        if let Err(error) = publish_hook(PublishBoundary::ActiveOpened) {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                &staging_dir,
                &lease,
            );
            return Err(error);
        }
        let coordinator_file = match coordinator.file.try_clone() {
            Ok(file) => file,
            Err(error) => {
                rollback_published_root(
                    coordinator,
                    &active_name,
                    run_id,
                    owner,
                    &staging_dir,
                    &lease,
                );
                return Err(WorkspaceError::io(
                    "clone coordinator",
                    &coordinator.path,
                    error,
                ));
            }
        };
        if let Err(error) = publish_hook(PublishBoundary::CoordinatorOpened) {
            rollback_published_root(
                coordinator,
                &active_name,
                run_id,
                owner,
                &staging_dir,
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
                    &staging_dir,
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
                &staging_dir,
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
            lease: Mutex::new(Some(Arc::new(lease))),
            heartbeat: Mutex::new(Some(heartbeat)),
            lifecycle: Arc::new(Mutex::new(RootLifecycle::default())),
            namespace: AtomicU8::new(NamespaceLifecycle::Active as u8),
            #[cfg(windows)]
            rename_handle: Mutex::new(Some(rename_handle)),
            #[cfg(windows)]
            cleanup_lease_evidence: publication_marker_evidence.0,
            #[cfg(windows)]
            _mutation_barrier: mutation_barrier,
        })
    }

    #[allow(dead_code, reason = "Task 6 reports retained managed-root locations")]
    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }

    #[cfg(test)]
    pub(crate) fn move_for_identity_replacement_test(
        &self,
        destination: &Utf8Path,
    ) -> Result<(), WorkspaceError> {
        if destination.parent() != self.path.parent() {
            return Err(WorkspaceError::InvalidPath {
                path: destination.to_owned(),
            });
        }
        let current_name = self
            .path
            .file_name()
            .ok_or_else(|| WorkspaceError::InvalidPath {
                path: self.path.clone(),
            })?;
        let destination_name =
            destination
                .file_name()
                .ok_or_else(|| WorkspaceError::InvalidPath {
                    path: destination.to_owned(),
                })?;
        let local_guard = self.coordinator_local_lock.try_lock().map_err(|error| {
            WorkspaceError::io("lock coordinator", &self.path, error.to_string())
        })?;
        File::try_lock(&self.coordinator_file)
            .map_err(|error| WorkspaceError::io("lock coordinator", &self.path, error))?;
        let claim = self.claim_with_handle_handoff(
            &self.coordinator_dir,
            current_name,
            destination_name,
            &|| Ok(()),
        );
        let unlock = File::unlock(&self.coordinator_file)
            .map_err(|error| WorkspaceError::io("unlock coordinator", &self.path, error));
        drop(local_guard);
        match (claim, unlock) {
            (Ok(ClaimResult::Claimed), Ok(())) => Ok(()),
            (Ok(ClaimResult::Absent), Ok(())) => Err(WorkspaceError::InvalidPath {
                path: self.path.clone(),
            }),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
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
        self.create_child_with_hooks(prefix, creation_hook, &directory_identity)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "exact child capability validation and identity-bound rollback ordering stay adjacent"
    )]
    fn create_child_with_hooks(
        &self,
        prefix: &str,
        creation_hook: &impl Fn(ChildCreationBoundary) -> Result<(), WorkspaceError>,
        created_identity: &impl Fn(&cap_std::fs::Dir) -> std::io::Result<(u64, u64)>,
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
        let path = self.path.join(&name);
        let dir = create_owned_directory(&self.dir, &name)
            .map_err(|error| WorkspaceError::io("create managed child", &self.path, error))?;
        let rollback_deadline = std::time::Instant::now() + OWNER_CLEANUP_BUDGET;
        let mut child_identity = None;
        #[cfg(windows)]
        let mut shared_dir = None;
        let construction = (|| -> Result<(Arc<File>, u64), WorkspaceError> {
            let identity = created_identity(&dir)
                .map_err(|error| WorkspaceError::io("identify managed child", &path, error))?;
            child_identity = Some(identity);
            creation_hook(ChildCreationBoundary::Created)?;
            let child_metadata = self
                .dir
                .symlink_metadata(&name)
                .map_err(|error| WorkspaceError::io("inspect managed child", &path, error))?;
            if !child_metadata.is_dir() || child_metadata.file_type().is_symlink() {
                return Err(WorkspaceError::InvalidPath { path: path.clone() });
            }
            if metadata_identity(&child_metadata) != identity {
                return Err(WorkspaceError::InvalidPath { path: path.clone() });
            }
            creation_hook(ChildCreationBoundary::Opened)?;
            let opened_child_identity = directory_identity(&dir)
                .map_err(|error| WorkspaceError::io("identify managed child", &path, error))?;
            if opened_child_identity != identity {
                return Err(WorkspaceError::InvalidPath { path: path.clone() });
            }
            #[cfg(windows)]
            {
                shared_dir = Some(open_owned_directory(&self.dir, &name).map_err(|error| {
                    WorkspaceError::io("open managed child working directory", &path, error)
                })?);
                let shared_identity = directory_identity(
                    shared_dir
                        .as_ref()
                        .expect("managed child shared directory was opened"),
                )
                .map_err(|error| {
                    WorkspaceError::io("identify managed child working directory", &path, error)
                })?;
                if shared_identity != identity {
                    return Err(WorkspaceError::InvalidPath { path: path.clone() });
                }
            }
            let lease = self
                .lease
                .lock()
                .map_err(|_| WorkspaceError::StatePoisoned)?
                .as_ref()
                .map(Arc::clone)
                .ok_or_else(|| {
                    WorkspaceError::io(
                        "create managed child",
                        &self.path,
                        "managed root lease is unavailable",
                    )
                })?;
            let next_live_children = lifecycle
                .live_children
                .checked_add(1)
                .ok_or(WorkspaceError::CopySizeOverflow)?;
            Ok((lease, next_live_children))
        })();
        let (lease, next_live_children) = match construction {
            Ok(constructed) => constructed,
            Err(primary) => {
                #[cfg(windows)]
                drop(shared_dir.take());
                return Err(error_after_unpublished_directory_failure(
                    &self.dir,
                    &name,
                    dir,
                    child_identity,
                    rollback_deadline,
                    &path,
                    primary,
                ));
            }
        };
        lifecycle.live_children = next_live_children;
        #[cfg(windows)]
        let dir = {
            let shared_dir = shared_dir.expect("managed child shared directory passed validation");
            drop(dir);
            shared_dir
        };
        Ok(ManagedChild {
            path,
            dir: Some(dir),
            parent: Some(child_parent),
            name,
            lifecycle: Arc::clone(&self.lifecycle),
            lease: Some(lease),
            #[cfg(all(test, windows))]
            drop_gate: None,
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
        let lease = self
            .lease
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        let lease = lease.as_deref().ok_or_else(|| {
            WorkspaceError::io(
                "mark workspace cleanup ready",
                &self.path,
                "managed root lease is unavailable",
            )
        })?;
        write_cleanup_ready_marker(&self.dir, &self.path, &self.run_id, self.owner, lease)?;
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
        let mut file = create_owned_file(&self.dir, name, OwnedFileAccess::Write)
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
        let heartbeat = heartbeat.as_ref().ok_or_else(|| {
            WorkspaceError::io(
                "refresh workspace heartbeat",
                &self.path,
                "managed root heartbeat is unavailable",
            )
        })?;
        refresh_file_modified_time(heartbeat)
            .map_err(|error| WorkspaceError::io("refresh workspace heartbeat", &self.path, error))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "exact root and lease handoff, lifecycle recording, and error restoration stay adjacent"
    )]
    fn claim_with_handle_handoff(
        &self,
        parent: &cap_std::fs::Dir,
        current_name: &str,
        deleting_name: &str,
        before_next_operation: &impl Fn() -> Result<(), WorkspaceError>,
    ) -> Result<ClaimResult, WorkspaceError> {
        let mut lease_slot = self
            .lease
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        let lease = lease_slot.take().ok_or_else(|| {
            WorkspaceError::io(
                "claim managed workspace",
                &self.path,
                "managed root lease is unavailable",
            )
        })?;
        let mut lease = match Arc::try_unwrap(lease) {
            Ok(lease) => Some(lease),
            Err(shared) => {
                *lease_slot = Some(shared);
                return Err(WorkspaceError::io(
                    "claim managed workspace",
                    &self.path,
                    "managed root still has a shared lease guard",
                ));
            }
        };
        #[cfg(windows)]
        let Ok(rename_slot) = self.rename_handle.lock() else {
            *lease_slot = lease.take().map(Arc::new);
            return Err(WorkspaceError::StatePoisoned);
        };
        #[cfg(windows)]
        let Some(rename_handle) = rename_slot.as_ref() else {
            *lease_slot = lease.take().map(Arc::new);
            return Err(WorkspaceError::io(
                "claim managed workspace",
                &self.path,
                "managed root deletion capability is unavailable",
            ));
        };
        #[cfg(windows)]
        let Ok(mut heartbeat_slot) = self.heartbeat.lock() else {
            *lease_slot = lease.take().map(Arc::new);
            return Err(WorkspaceError::StatePoisoned);
        };
        #[cfg(windows)]
        let heartbeat_evidence = match heartbeat_slot.as_ref() {
            Some(heartbeat) => {
                match inspect_marker_evidence(heartbeat, &self.path.join(HEARTBEAT_FILE)) {
                    Ok(evidence) => Some(evidence),
                    Err(error) => {
                        *lease_slot = lease.take().map(Arc::new);
                        return Err(error);
                    }
                }
            }
            None => None,
        };
        #[cfg(windows)]
        drop(heartbeat_slot.take());
        let records_deleting_state = deleting_name == format!("{DELETING_PREFIX}{}", self.run_id);

        let claim = claim_managed_child(
            parent,
            current_name,
            deleting_name,
            &self.run_id,
            self.owner,
            &self.dir,
            #[cfg(windows)]
            rename_handle,
            &mut lease,
            &|| {
                if records_deleting_state {
                    NamespaceLifecycle::Deleting.store(&self.namespace);
                }
            },
            before_next_operation,
        );
        *lease_slot = lease.map(Arc::new);
        #[cfg(windows)]
        if NamespaceLifecycle::load(&self.namespace) == NamespaceLifecycle::Active
            && !matches!(&claim, Ok(ClaimResult::Claimed | ClaimResult::Absent))
            && let Some(evidence) = heartbeat_evidence.as_ref()
        {
            match reopen_heartbeat(&self.dir, evidence, &self.path) {
                Ok(heartbeat) => *heartbeat_slot = Some(heartbeat),
                Err(restore_error) => {
                    let claim_error = claim
                        .as_ref()
                        .expect_err("only a failed claim restores the heartbeat");
                    return Err(WorkspaceError::io(
                        "restore workspace heartbeat after failed claim",
                        &self.path,
                        format!(
                            "claim failed: {claim_error}; heartbeat restore failed: {restore_error}"
                        ),
                    ));
                }
            }
        }
        claim
    }

    #[cfg(windows)]
    fn deleting_root_capability_was_consumed(
        &self,
        deleting_name: &str,
    ) -> Result<bool, WorkspaceError> {
        let root = self
            .rename_handle
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        if root.is_some() {
            return Ok(false);
        }
        match self.coordinator_dir.symlink_metadata(deleting_name) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(WorkspaceError::io(
                "verify consumed managed-root deletion capability",
                &self.path,
                error,
            )),
            Ok(_) => Err(WorkspaceError::io(
                "resume managed workspace cleanup",
                &self.path,
                "deleting root remains after its exact deletion capability was consumed",
            )),
        }
    }

    #[cfg(windows)]
    fn restore_missing_deleting_lease(&self, deleting_name: &str) -> Result<(), WorkspaceError> {
        let mut lease = self
            .lease
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        if lease.is_some() {
            return Ok(());
        }
        match self.dir.symlink_metadata(LEASE_FILE) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(WorkspaceError::io(
                "inspect deleting workspace lease",
                self.path.parent().unwrap().join(deleting_name),
                error,
            )),
            Ok(_) => {
                let reopened = reopen_locked_lease(
                    &self.dir,
                    &self.cleanup_lease_evidence,
                    &self.path.parent().unwrap().join(deleting_name),
                )?;
                *lease = Some(Arc::new(reopened));
                Ok(())
            }
        }
    }

    fn take_owner_cleanup_capabilities(&self) -> Result<CleanupCapabilities, WorkspaceError> {
        let mut lease_slot = self
            .lease
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        let lease_guard = match lease_slot.take() {
            Some(lease) => match Arc::try_unwrap(lease) {
                Ok(lease) => Some(lease),
                Err(shared) => {
                    *lease_slot = Some(shared);
                    return Err(WorkspaceError::io(
                        "resume managed workspace cleanup",
                        &self.path,
                        "managed root still has a shared lease guard",
                    ));
                }
            },
            None => None,
        };
        #[cfg(windows)]
        {
            let Ok(mut root_slot) = self.rename_handle.lock() else {
                *lease_slot = lease_guard.map(Arc::new);
                return Err(WorkspaceError::StatePoisoned);
            };
            let Some(root_pin) = root_slot.take() else {
                *lease_slot = lease_guard.map(Arc::new);
                return Err(WorkspaceError::io(
                    "resume managed workspace cleanup",
                    &self.path,
                    "managed root deletion capability is unavailable",
                ));
            };
            let Some(lease_guard) = lease_guard else {
                if self.dir.symlink_metadata(LEASE_FILE).is_ok() {
                    *root_slot = Some(root_pin);
                    return Err(WorkspaceError::io(
                        "resume managed workspace cleanup",
                        &self.path,
                        "managed lease deletion capability is unavailable",
                    ));
                }
                return Ok(CleanupCapabilities {
                    lease_guard: None,
                    root_authority: RootCleanupAuthority::Pinned(root_pin),
                    progress: DeletionProgress::default(),
                });
            };
            Ok(CleanupCapabilities::claimed(root_pin, lease_guard))
        }
        #[cfg(unix)]
        {
            Ok(CleanupCapabilities::with_lease(lease_guard))
        }
    }

    fn restore_owner_cleanup_capabilities(&self, mut capabilities: CleanupCapabilities) {
        if let Some(lease) = capabilities.lease_guard.take() {
            let mut slot = self
                .lease
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            debug_assert!(slot.is_none());
            *slot = Some(Arc::new(lease));
        }
        #[cfg(windows)]
        if let RootCleanupAuthority::Pinned(root) = capabilities.root_authority {
            let mut slot = self
                .rename_handle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            debug_assert!(slot.is_none());
            *slot = Some(root);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "cleanup records every fail-closed claim and bounded-removal outcome"
    )]
    pub(crate) fn cleanup(&self, budget: Duration) -> CleanupRecord {
        let started = std::time::Instant::now();
        let mut record = CleanupRecord {
            disposition: CleanupDisposition::Failed,
            evidence: CleanupEvidence {
                examined_entries: 0,
                removed_entries: 0,
                details: Vec::new(),
                omitted_detail_count: 0,
                remaining_root: Some(self.path.clone()),
            },
        };
        if NamespaceLifecycle::load(&self.namespace) == NamespaceLifecycle::Complete {
            record.disposition = CleanupDisposition::Clean;
            record.evidence.remaining_root = None;
            return record;
        }
        if budget.is_zero() {
            record.disposition = CleanupDisposition::Deferred(None);
            return record;
        }
        let Ok(mut lifecycle) = self.lifecycle.lock() else {
            record
                .evidence
                .push_detail("managed root lifecycle is poisoned".to_owned());
            return record;
        };
        lifecycle.cleanup_started = true;
        if lifecycle.live_children != 0 {
            record.disposition = CleanupDisposition::Deferred(None);
            record.evidence.push_detail(format!(
                "managed root still has {} live child handle(s)",
                lifecycle.live_children
            ));
            return record;
        }
        let active_name = format!("{ACTIVE_PREFIX}{}", self.run_id);
        let deleting_name = format!("{DELETING_PREFIX}{}", self.run_id);
        let expected_root_identity = match directory_identity(&self.dir) {
            Ok(identity) => identity,
            Err(error) => {
                record
                    .evidence
                    .push_detail(format!("managed root identity inspection failed: {error}"));
                return record;
            }
        };
        if NamespaceLifecycle::load(&self.namespace) == NamespaceLifecycle::Active {
            let local_guard = match self.coordinator_local_lock.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    record.disposition = CleanupDisposition::Deferred(Some(
                        CleanupRetryReason::CoordinatorContention,
                    ));
                    record
                        .evidence
                        .push_detail("coordinator process lock is busy".to_owned());
                    return record;
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    record
                        .evidence
                        .push_detail("coordinator process lock is poisoned".to_owned());
                    return record;
                }
            };
            if let Err(error) = File::try_lock(&self.coordinator_file) {
                if matches!(error, std::fs::TryLockError::WouldBlock) {
                    record.disposition = CleanupDisposition::Deferred(Some(
                        CleanupRetryReason::CoordinatorContention,
                    ));
                }
                record
                    .evidence
                    .push_detail(format!("coordinator lock failed: {error}"));
                return record;
            }
            let claim_deadline_expired = std::cell::Cell::new(false);
            let claim = self.claim_with_handle_handoff(
                &self.coordinator_dir,
                &active_name,
                &deleting_name,
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
            let unlock = File::unlock(&self.coordinator_file);
            if let Err(error) = unlock {
                if NamespaceLifecycle::load(&self.namespace) == NamespaceLifecycle::Deleting {
                    record.evidence.remaining_root = Some(
                        self.path
                            .parent()
                            .expect("managed run has parent")
                            .join(&deleting_name),
                    );
                }
                record
                    .evidence
                    .push_detail(format!("coordinator unlock failed: {error}"));
                return record;
            }
            drop(local_guard);
            match claim {
                Ok(ClaimResult::Absent) => {
                    NamespaceLifecycle::Complete.store(&self.namespace);
                    record.disposition = CleanupDisposition::Clean;
                    record.evidence.remaining_root = None;
                    return record;
                }
                Ok(ClaimResult::Claimed) => {}
                Err(error) => {
                    if claim_deadline_expired.get() {
                        record.disposition = CleanupDisposition::Deferred(None);
                    }
                    if NamespaceLifecycle::load(&self.namespace) == NamespaceLifecycle::Deleting {
                        record.evidence.remaining_root = Some(
                            self.path
                                .parent()
                                .expect("managed run has parent")
                                .join(&deleting_name),
                        );
                    }
                    record.evidence.push_detail(error.to_string());
                    return record;
                }
            }
        }
        record.evidence.remaining_root = Some(
            self.path
                .parent()
                .expect("managed run has parent")
                .join(&deleting_name),
        );
        if started.elapsed() >= budget {
            record.disposition = CleanupDisposition::Deferred(None);
            return record;
        }
        #[cfg(windows)]
        match self.deleting_root_capability_was_consumed(&deleting_name) {
            Ok(true) => {
                NamespaceLifecycle::Complete.store(&self.namespace);
                record.disposition = CleanupDisposition::Clean;
                record.evidence.remaining_root = None;
                return record;
            }
            Ok(false) => {}
            Err(error) => {
                record.evidence.push_detail(error.to_string());
                return record;
            }
        }
        #[cfg(windows)]
        if let Err(error) = self.restore_missing_deleting_lease(&deleting_name) {
            record.evidence.push_detail(error.to_string());
            return record;
        }
        let mut capabilities = match self.take_owner_cleanup_capabilities() {
            Ok(capabilities) => capabilities,
            Err(error) => {
                record.evidence.push_detail(error.to_string());
                return record;
            }
        };
        let removal = remove_claimed_tree_bounded_with_capabilities(
            &self.coordinator_dir,
            &deleting_name,
            Some(expected_root_identity),
            budget.saturating_sub(started.elapsed()),
            &mut capabilities,
        );
        self.finish_owner_removal(record, capabilities, &deleting_name, removal)
    }

    fn finish_owner_removal(
        &self,
        mut record: CleanupRecord,
        capabilities: CleanupCapabilities,
        deleting_name: &str,
        removal: std::io::Result<RemovalSlice>,
    ) -> CleanupRecord {
        record.evidence.examined_entries = capabilities.progress.examined_entries;
        record.evidence.removed_entries = capabilities.progress.removed_entries;
        match removal {
            Ok(slice)
                if slice.complete && entry_is_absent(&self.coordinator_dir, deleting_name) =>
            {
                NamespaceLifecycle::Complete.store(&self.namespace);
                record.disposition = CleanupDisposition::Clean;
                record.evidence.remaining_root = None;
            }
            Ok(_) => {
                record.disposition = CleanupDisposition::Deferred(None);
                self.restore_owner_cleanup_capabilities(capabilities);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if entry_is_absent(&self.coordinator_dir, deleting_name) {
                    NamespaceLifecycle::Complete.store(&self.namespace);
                    record.disposition = CleanupDisposition::Clean;
                    record.evidence.remaining_root = None;
                } else {
                    record.evidence.push_detail(error.to_string());
                    self.restore_owner_cleanup_capabilities(capabilities);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                record.disposition = CleanupDisposition::Deferred(None);
                record.evidence.push_detail(error.to_string());
                self.restore_owner_cleanup_capabilities(capabilities);
            }
            Err(error) => {
                record.evidence.push_detail(error.to_string());
                self.restore_owner_cleanup_capabilities(capabilities);
            }
        }
        record
    }

    fn verified_remaining_root(&self) -> Result<Option<Utf8PathBuf>, String> {
        let expected_identity = directory_identity(&self.dir)
            .map_err(|error| format!("inspect interrupted cleanup root identity: {error}"))?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "managed root has no parent".to_owned())?;
        for name in [
            format!("{ACTIVE_PREFIX}{}", self.run_id),
            format!("{DELETING_PREFIX}{}", self.run_id),
        ] {
            let metadata = match self.coordinator_dir.symlink_metadata(&name) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(format!("inspect interrupted cleanup entry {name}: {error}"));
                }
            };
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || metadata_identity(&metadata) != expected_identity
            {
                return Err(format!(
                    "interrupted cleanup entry {name} did not match the owned root identity"
                ));
            }
            let candidate = open_owned_directory(&self.coordinator_dir, &name)
                .map_err(|error| format!("open interrupted cleanup entry {name}: {error}"))?;
            let candidate_identity = directory_identity(&candidate)
                .map_err(|error| format!("identify interrupted cleanup entry {name}: {error}"))?;
            if candidate_identity != expected_identity {
                return Err(format!(
                    "opened interrupted cleanup entry {name} changed identity"
                ));
            }
            return Ok(Some(parent.join(name)));
        }
        Ok(None)
    }

    pub(crate) fn interrupted_cleanup_record(&self, failed: bool, reason: String) -> CleanupRecord {
        let (remaining_root, integrity_detail) = match self.verified_remaining_root() {
            Ok(remaining_root) => (remaining_root, None),
            Err(error) => (None, Some(error)),
        };
        let mut record = CleanupRecord {
            disposition: if failed && remaining_root.is_some() {
                CleanupDisposition::Failed
            } else {
                CleanupDisposition::Deferred(None)
            },
            evidence: CleanupEvidence {
                examined_entries: 0,
                removed_entries: 0,
                details: Vec::new(),
                omitted_detail_count: 0,
                remaining_root,
            },
        };
        record.evidence.push_detail(reason);
        if let Some(detail) = integrity_detail {
            record
                .evidence
                .push_detail(format!("cleanup integrity unverifiable: {detail}"));
        } else if record.evidence.remaining_root.is_none() {
            record
                .evidence
                .push_detail("cleanup root namespace entry was not found".to_owned());
        }
        record
    }

    #[allow(
        dead_code,
        reason = "Task 6 wires unsafe-to-clean shutdown transitions"
    )]
    pub(crate) fn abandon_for_janitor(&self, reason: String) -> CleanupRecord {
        let mut record = CleanupRecord {
            disposition: CleanupDisposition::Deferred(None),
            evidence: CleanupEvidence {
                examined_entries: 0,
                removed_entries: 0,
                details: Vec::new(),
                omitted_detail_count: 0,
                remaining_root: Some(self.path.clone()),
            },
        };
        record.evidence.push_detail(reason);
        record
    }

    pub(crate) fn reclaim_abandoned(
        coordinator: &ManagedRootCoordinator,
        now: SystemTime,
    ) -> ReclaimReport {
        Self::reclaim_abandoned_with_clock(coordinator, now, &|started| started.elapsed())
    }

    /// Cap the existing janitor ledger by the run's remaining preparation time.
    /// All deletion slices inherit the same shifted start, including their local budgets.
    pub(crate) fn reclaim_abandoned_controlled(
        coordinator: &ManagedRootCoordinator,
        now: SystemTime,
        deadline: tokio::time::Instant,
        stopped: &impl Fn() -> bool,
    ) -> ReclaimReport {
        let remaining = deadline
            .saturating_duration_since(tokio::time::Instant::now())
            .min(JANITOR_CLEANUP_BUDGET);
        let started = std::time::Instant::now();
        let started = started
            .checked_sub(JANITOR_CLEANUP_BUDGET.saturating_sub(remaining))
            .unwrap_or(started);
        let elapsed = |started: std::time::Instant| {
            if stopped() {
                JANITOR_CLEANUP_BUDGET
            } else {
                started.elapsed()
            }
        };
        let remaining = || JANITOR_CLEANUP_BUDGET.saturating_sub(elapsed(started));
        if remaining().is_zero() {
            let mut report = ReclaimLedger::default();
            report.report.scan.stop_reason = Some(JanitorReason::SelectionDeadline);
            return report.finish();
        }
        Self::reclaim_abandoned_with_clock_and_selection(
            coordinator,
            now,
            started,
            &elapsed,
            &|scan| select_reclaim_candidates_controlled(coordinator, scan, &remaining),
        )
    }

    fn reclaim_abandoned_with_clock(
        coordinator: &ManagedRootCoordinator,
        now: SystemTime,
        elapsed: &impl Fn(std::time::Instant) -> Duration,
    ) -> ReclaimReport {
        Self::reclaim_abandoned_with_clock_and_selection(
            coordinator,
            now,
            std::time::Instant::now(),
            elapsed,
            &|scan| select_reclaim_candidates(coordinator, scan),
        )
    }

    #[allow(
        clippy::too_many_lines,
        reason = "janitor validation and claim order stays explicit"
    )]
    fn reclaim_abandoned_with_clock_and_selection(
        coordinator: &ManagedRootCoordinator,
        now: SystemTime,
        cleanup_started: std::time::Instant,
        elapsed: &impl Fn(std::time::Instant) -> Duration,
        select: &impl Fn(&mut JanitorScan) -> Result<(Vec<String>, Option<String>), WorkspaceError>,
    ) -> ReclaimReport {
        let mut report = ReclaimLedger::default();
        let mut pending_cleanup = VecDeque::new();
        let (candidates, cursor_error) = match select(&mut report.report.scan) {
            Ok(selection) => selection,
            Err(error) => {
                let reason = *report
                    .report
                    .scan
                    .stop_reason
                    .get_or_insert(JanitorReason::SelectionFailed);
                report.detail(reason, error.to_string());
                return report.finish();
            }
        };
        report.report.scan.selected_roots = candidates.len() as u64;
        if let Some(error) = cursor_error {
            report.detail(JanitorReason::CursorPersistenceFailed, error);
        }
        for name in candidates {
            if elapsed(cleanup_started) >= JANITOR_CLEANUP_BUDGET {
                report.report.scan.stop_reason = Some(JanitorReason::CleanupIncomplete);
                break;
            }
            let index = report.begin();
            let name = name.as_str();
            let path = coordinator.path.join(name);
            let run_id = if let Some(run_id) = name.strip_prefix(ACTIVE_PREFIX) {
                run_id
            } else if let Some(run_id) = name.strip_prefix(DELETING_PREFIX) {
                run_id
            } else if let Some(run_id) = name.strip_prefix(STAGING_PREFIX) {
                run_id
            } else {
                report.hold(
                    index,
                    JanitorReason::InvalidCandidate,
                    format!("invalid managed name: {path}"),
                );
                continue;
            };
            let staging = name.starts_with(STAGING_PREFIX);
            let deleting = name.starts_with(DELETING_PREFIX);
            if uuid::Uuid::parse_str(run_id).is_err() {
                report.hold(
                    index,
                    JanitorReason::InvalidCandidate,
                    format!("invalid managed UUID: {path}"),
                );
                continue;
            }
            #[cfg(windows)]
            match windows::root_mutation_barrier_exists(run_id) {
                Ok(true) => {
                    report.hold(
                        index,
                        JanitorReason::MutationBarrierActive,
                        format!("active mutation barrier: {path}"),
                    );
                    continue;
                }
                Ok(false) => {}
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::MutationBarrierFailed,
                        format!("inspect managed-root mutation barrier failed for {path}: {error}"),
                    );
                    continue;
                }
            }
            #[cfg(windows)]
            let mut coordinator_guard = match CoordinatorLockGuard::try_acquire(coordinator) {
                Ok(Some(guard)) => Some(guard),
                Ok(None) => {
                    report.hold(
                        index,
                        JanitorReason::CoordinatorBusy,
                        format!("coordinator busy: {path}"),
                    );
                    continue;
                }
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::CoordinatorFailed,
                        format!("{path}: {error}"),
                    );
                    continue;
                }
            };
            #[cfg(unix)]
            let candidate = open_owned_directory(&coordinator.dir, name);
            #[cfg(windows)]
            let candidate = open_owned_directory_for_rename(&coordinator.dir, name);
            let candidate = match candidate {
                Ok(candidate) => candidate,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    report.absent(index);
                    continue;
                }
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::CandidateInspectionFailed,
                        format!("open candidate {path}: {error}"),
                    );
                    continue;
                }
            };
            let candidate_identity = match directory_identity(&candidate) {
                Ok(identity) => identity,
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::IdentityInspectionFailed,
                        format!("identify candidate {path}: {error}"),
                    );
                    continue;
                }
            };
            let lease_metadata = candidate.symlink_metadata(LEASE_FILE);
            let empty_staging_age = (staging
                && janitor_empty_probe(&candidate, &mut report, &path))
            .then(|| directory_timestamp_is_stale(&candidate, now, &mut report, &path));
            let unmarked_staging_is_reclaimable = matches!(empty_staging_age, Some(Ok(())));
            let empty_deleting_is_resumable =
                deleting && janitor_empty_probe(&candidate, &mut report, &path);
            if matches!(&lease_metadata, Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                && (unmarked_staging_is_reclaimable || empty_deleting_is_resumable)
            {
                #[cfg(unix)]
                let coordinator_guard = match CoordinatorLockGuard::try_acquire(coordinator) {
                    Ok(Some(guard)) => guard,
                    Ok(None) => {
                        report.hold(
                            index,
                            JanitorReason::CoordinatorBusy,
                            format!("coordinator busy: {path}"),
                        );
                        continue;
                    }
                    Err(error) => {
                        report.hold(
                            index,
                            JanitorReason::CoordinatorFailed,
                            format!("{path}: {error}"),
                        );
                        continue;
                    }
                };
                #[cfg(windows)]
                let coordinator_guard = coordinator_guard
                    .take()
                    .expect("coordinator acquired before opening marker");
                let mut progress = DeletionProgress::default();
                let removal = remove_empty_unmarked_directory(
                    &coordinator.dir,
                    name,
                    candidate,
                    candidate_identity,
                    cleanup_started,
                    CleanupBudget {
                        duration: JANITOR_CLEANUP_BUDGET,
                        stopped: Some(&|| elapsed(cleanup_started) >= JANITOR_CLEANUP_BUDGET),
                    },
                    &mut progress,
                );
                let unlock = coordinator_guard.unlock(&coordinator.path);
                match (removal, unlock) {
                    (Ok(()), Ok(())) => {}
                    (Err(error), _) => report.hold(
                        index,
                        JanitorReason::DeleteFailed,
                        format!("empty unmarked-root cleanup failed for {path}: {error}"),
                    ),
                    (_, Err(error)) => report.hold(
                        index,
                        JanitorReason::CoordinatorFailed,
                        format!("coordinator unlock failed for {path}: {error}"),
                    ),
                }
                report.removal(index, progress, false);
                continue;
            }
            // A pre-lease crash can leave an empty staging directory. Its positive
            // age evidence is an ordinary hold; failed inspection still falls through.
            if matches!(&lease_metadata, Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                && let Some(Err(failure)) = &empty_staging_age
                && failure.reason == JanitorReason::Young
            {
                report.hold(
                    index,
                    failure.reason,
                    format!("staging age for {path}: {}", failure.cause),
                );
                continue;
            }
            let lease = match lease_metadata {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    match open_regular_file_nofollow(&candidate, LEASE_FILE) {
                        Ok(lease) => lease,
                        Err(error) => {
                            report.hold(
                                index,
                                JanitorReason::LeaseInvalid,
                                format!("open lease for {path}: {error}"),
                            );
                            continue;
                        }
                    }
                }
                Ok(_) => {
                    report.hold(
                        index,
                        JanitorReason::LeaseInvalid,
                        format!("lease is not a regular non-symlink file: {path}"),
                    );
                    continue;
                }
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::LeaseInvalid,
                        format!("inspect lease for {path}: {error}"),
                    );
                    continue;
                }
            };
            if let Err(error) = File::try_lock(&lease) {
                let reason = if matches!(error, std::fs::TryLockError::WouldBlock) {
                    JanitorReason::ActiveLease
                } else {
                    JanitorReason::LeaseLockFailed
                };
                report.hold(index, reason, format!("lease lock for {path}: {error}"));
                continue;
            }
            let marker = match read_json_file_checked::<LeaseMarker>(&lease) {
                Ok(marker) => marker,
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::MarkerInvalid,
                        format!("lease marker for {path}: {error}"),
                    );
                    continue;
                }
            };
            if marker.schema != LEASE_SCHEMA || marker.run_id != run_id {
                report.hold(
                    index,
                    JanitorReason::MarkerInvalid,
                    format!("lease marker schema or identity mismatch: {path}"),
                );
                continue;
            }
            // Keep the original short-circuit order and fallback observations.
            let retained = match candidate.symlink_metadata(RETAIN_FILE) {
                Ok(_) => true,
                Err(error) => {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        report.detail(
                            JanitorReason::CandidateInspectionFailed,
                            format!("retain marker for {path}: {error}"),
                        );
                    }
                    false
                }
            };
            let hold = if retained {
                Some(EligibilityFailure::new(
                    JanitorReason::RetainMarker,
                    "retain marker exists",
                ))
            } else if staging {
                staging_contents_are_safe(&candidate)
                    .and_then(|()| staging_candidate_is_stale(&candidate, &marker, now))
                    .err()
            } else if deleting
                && inspect_deleting_contents(&candidate).unwrap_or_else(|error| {
                    report.detail(
                        JanitorReason::CandidateInspectionFailed,
                        format!("inspect deleting root {path}: {error}"),
                    );
                    false
                })
            {
                None
            } else {
                candidate_is_reclaimable(&candidate, &marker, &lease, now, &mut report, &path).err()
            };
            if let Some(failure) = hold {
                report.hold(
                    index,
                    failure.reason,
                    format!(
                        "managed root is not yet reclaimable: {path}: {}",
                        failure.cause
                    ),
                );
                continue;
            }
            #[cfg(unix)]
            let coordinator_guard = match CoordinatorLockGuard::try_acquire(coordinator) {
                Ok(Some(guard)) => guard,
                Ok(None) => {
                    report.hold(
                        index,
                        JanitorReason::CoordinatorBusy,
                        format!("coordinator busy: {path}"),
                    );
                    continue;
                }
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::CoordinatorFailed,
                        format!("{path}: {error}"),
                    );
                    continue;
                }
            };
            #[cfg(windows)]
            let coordinator_guard = coordinator_guard
                .take()
                .expect("coordinator acquired before opening marker");
            let deleting_name = format!("{DELETING_PREFIX}{run_id}");
            let mut lease = Some(lease);
            let claim = claim_managed_child(
                &coordinator.dir,
                name,
                &deleting_name,
                run_id,
                marker.owner,
                &candidate,
                #[cfg(windows)]
                &candidate,
                &mut lease,
                &|| {},
                &|| {
                    if elapsed(cleanup_started) < JANITOR_CLEANUP_BUDGET {
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
            let unlock = coordinator_guard.unlock(&coordinator.path);
            if let Err(error) = unlock {
                report.hold(
                    index,
                    JanitorReason::CoordinatorFailed,
                    format!("coordinator unlock failed for {path}: {error}"),
                );
                continue;
            }
            match claim {
                Ok(ClaimResult::Claimed) => {}
                Ok(ClaimResult::Absent) => {
                    report.absent(index);
                    continue;
                }
                Err(error) => {
                    report.hold(
                        index,
                        JanitorReason::ClaimFailed,
                        format!("claim {path}: {error}"),
                    );
                    continue;
                }
            }
            let Some(lease) = lease else {
                report.hold(
                    index,
                    JanitorReason::ClaimedLeaseUnavailable,
                    format!(
                        "claimed root lease could not be reopened: {}",
                        coordinator.path.join(&deleting_name)
                    ),
                );
                continue;
            };
            #[cfg(windows)]
            let capabilities = CleanupCapabilities::claimed(candidate, lease);
            #[cfg(unix)]
            let capabilities = {
                drop(candidate);
                CleanupCapabilities::with_lease(Some(lease))
            };
            record_or_queue_reclaim_removal(
                &mut report,
                &mut pending_cleanup,
                coordinator,
                &deleting_name,
                candidate_identity,
                capabilities,
                cleanup_started,
                index,
                elapsed,
            );
        }
        while !pending_cleanup.is_empty() && elapsed(cleanup_started) < JANITOR_CLEANUP_BUDGET {
            let round_len = pending_cleanup.len();
            let mut round_removed = 0_u64;
            for _ in 0..round_len {
                if elapsed(cleanup_started) >= JANITOR_CLEANUP_BUDGET {
                    break;
                }
                let Some(mut pending) = pending_cleanup.pop_front() else {
                    break;
                };
                match reclaim_removal(
                    coordinator,
                    &pending.deleting_name,
                    pending.expected_identity,
                    cleanup_started,
                    &mut pending.capabilities,
                    elapsed,
                ) {
                    ReclaimRemoval::Complete => {
                        report.removal(pending.index, pending.capabilities.progress, true);
                    }
                    ReclaimRemoval::Deferred { removed } => {
                        // Scheduling uses ONLY deferred-slice progress, as before.
                        round_removed = round_removed.saturating_add(removed);
                        pending_cleanup.push_back(pending);
                    }
                    ReclaimRemoval::Failed(error) => {
                        report.hold(
                            pending.index,
                            JanitorReason::DeleteFailed,
                            format!(
                                "cleanup failed for {}: {error}",
                                coordinator.path.join(&pending.deleting_name)
                            ),
                        );
                        report.removal(pending.index, pending.capabilities.progress, false);
                    }
                }
            }
            if round_removed == 0 {
                break;
            }
        }
        for pending in pending_cleanup {
            report.hold(
                pending.index,
                JanitorReason::CleanupIncomplete,
                format!(
                    "cleanup deferred for {}",
                    coordinator.path.join(&pending.deleting_name)
                ),
            );
            report.removal(pending.index, pending.capabilities.progress, false);
        }
        report.finish()
    }
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
    index: usize,
    deleting_name: String,
    expected_identity: (u64, u64),
    capabilities: CleanupCapabilities,
}

#[allow(
    clippy::too_many_arguments,
    reason = "first slice binds the root evidence index to its deletion capabilities before queueing"
)]
fn record_or_queue_reclaim_removal(
    report: &mut ReclaimLedger,
    pending_cleanup: &mut VecDeque<PendingCleanup>,
    coordinator: &ManagedRootCoordinator,
    deleting_name: &str,
    expected_identity: (u64, u64),
    mut capabilities: CleanupCapabilities,
    cleanup_started: std::time::Instant,
    index: usize,
    elapsed: &impl Fn(std::time::Instant) -> Duration,
) {
    match reclaim_removal(
        coordinator,
        deleting_name,
        expected_identity,
        cleanup_started,
        &mut capabilities,
        elapsed,
    ) {
        ReclaimRemoval::Complete => report.removal(index, capabilities.progress, true),
        ReclaimRemoval::Deferred { .. } => {
            pending_cleanup.push_back(PendingCleanup {
                index,
                deleting_name: deleting_name.to_owned(),
                expected_identity,
                capabilities,
            });
        }
        ReclaimRemoval::Failed(error) => {
            report.hold(
                index,
                JanitorReason::DeleteFailed,
                format!(
                    "cleanup failed for {}: {error}",
                    coordinator.path.join(deleting_name)
                ),
            );
            report.removal(index, capabilities.progress, false);
        }
    }
}

fn reclaim_removal(
    coordinator: &ManagedRootCoordinator,
    deleting_name: &str,
    expected_identity: (u64, u64),
    cleanup_started: std::time::Instant,
    capabilities: &mut CleanupCapabilities,
    elapsed: &impl Fn(std::time::Instant) -> Duration,
) -> ReclaimRemoval {
    let stopped = || elapsed(cleanup_started) >= JANITOR_CLEANUP_BUDGET;
    let budget = CleanupBudget {
        duration: JANITOR_CLEANUP_BUDGET.saturating_sub(elapsed(cleanup_started)),
        stopped: Some(&stopped),
    };
    match remove_claimed_tree_slice_with_capabilities(
        &coordinator.dir,
        deleting_name,
        Some(expected_identity),
        budget,
        capabilities,
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

#[cfg(windows)]
fn rollback_closed_published_root(
    coordinator: &ManagedRootCoordinator,
    staging_name: &str,
    active_name: &str,
    expected_dir: &cap_std::fs::Dir,
    rename_handle: &cap_std::fs::Dir,
    expected_root_identity: (u64, u64),
    lease_evidence: &MarkerEvidence,
) -> bool {
    // No descendant handle owned by this process is live here, so first restore the unpublished
    // staging name. If another same-user actor prevents that rename, leave identity-bound
    // cleanup evidence on the anchored directory for the next serialized janitor pass.
    if rename_open_owned_directory(
        &coordinator.dir,
        rename_handle,
        staging_name,
        expected_root_identity,
    )
    .is_ok()
    {
        return true;
    }
    let published_path = coordinator.path.join(active_name);
    let marker = &lease_evidence.marker;
    let _ = write_cleanup_ready_marker_for_identity(
        expected_dir,
        &published_path,
        &marker.run_id,
        marker.owner,
        lease_evidence.identity,
    );
    false
}

fn rollback_published_root(
    coordinator: &ManagedRootCoordinator,
    active_name: &str,
    run_id: &str,
    owner: OwnerKind,
    expected_dir: &cap_std::fs::Dir,
    expected_lease: &File,
) {
    // From this point the active name is externally visible. Publish immediate-reclaim evidence
    // before attempting any serialized claim, so lock contention or a later rollback failure can
    // never degrade into a young unlocked root that is preserved for 24 hours.
    let published_path = coordinator.path.join(active_name);
    let _ =
        write_cleanup_ready_marker(expected_dir, &published_path, run_id, owner, expected_lease);

    // Windows cannot rename a directory while either marker file is open. The caller still owns
    // both handles here, so the cleanup-ready marker is the rollback: once construction unwinds
    // and closes those handles, the next serialized janitor pass can reclaim it immediately.
    #[cfg(windows)]
    return;

    #[cfg(unix)]
    {
        let deleting_name = format!("{DELETING_PREFIX}{run_id}");
        let Ok(expected_root_identity) = directory_identity(expected_dir) else {
            return;
        };
        let Ok(expected_lease) = expected_lease.try_clone() else {
            return;
        };
        let mut expected_lease = Some(expected_lease);
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
            &mut expected_lease,
            &|| {},
            &|| Ok(()),
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
}

fn write_cleanup_ready_marker(
    dir: &cap_std::fs::Dir,
    path: &Utf8Path,
    run_id: &str,
    owner: OwnerKind,
    lease: &File,
) -> Result<(), WorkspaceError> {
    let lease_identity = file_identity(lease)
        .map_err(|error| WorkspaceError::io("inspect workspace lease", path, error))?;
    write_cleanup_ready_marker_for_identity(dir, path, run_id, owner, lease_identity)
}

fn write_cleanup_ready_marker_for_identity(
    dir: &cap_std::fs::Dir,
    path: &Utf8Path,
    run_id: &str,
    owner: OwnerKind,
    lease_identity: (u64, u64),
) -> Result<(), WorkspaceError> {
    let (lease_device, lease_inode) = lease_identity;
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
    let mut file = create_owned_file(dir, CLEANUP_READY_FILE, OwnedFileAccess::Write)
        .map_err(|error| WorkspaceError::io("create workspace marker", &marker_path, error))?;
    serde_json::to_writer(&mut file, &marker)
        .map_err(|error| WorkspaceError::io("write workspace marker", &marker_path, error))?;
    file.write_all(b"\n")
        .map_err(|error| WorkspaceError::io("write workspace marker", &marker_path, error))?;
    file.sync_all()
        .map_err(|error| WorkspaceError::io("flush workspace marker", &marker_path, error))
}

fn select_reclaim_candidates_controlled(
    coordinator: &ManagedRootCoordinator,
    scan: &mut JanitorScan,
    remaining: &impl Fn() -> Duration,
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    select_reclaim_candidates_controlled_with_hooks(coordinator, scan, remaining, &|| {})
}

fn select_reclaim_candidates_controlled_with_hooks(
    coordinator: &ManagedRootCoordinator,
    scan: &mut JanitorScan,
    remaining: &impl Fn() -> Duration,
    after_entry: &impl Fn(),
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    let budget = remaining().min(JANITOR_SELECTION_BUDGET);
    let started = std::time::Instant::now();
    let deadline = started.checked_add(budget).unwrap_or(started);
    let now = || {
        if remaining().is_zero() {
            deadline
        } else {
            std::time::Instant::now()
        }
    };
    ensure_selection_deadline(coordinator, scan, deadline, &now)?;
    // Startup reclamation may defer when another creator owns the coordinator.
    // Waiting for that lock must not acquire an independent local time budget.
    let Some(guard) = CoordinatorLockGuard::try_acquire(coordinator)? else {
        scan.stop_reason = Some(JanitorReason::CoordinatorBusy);
        return Ok((Vec::new(), None));
    };
    let selection = select_reclaim_candidates_locked_until_with_hooks(
        coordinator,
        scan,
        deadline,
        &now,
        &|| {},
        after_entry,
    );
    let unlock = guard.unlock(&coordinator.path);
    match (selection, unlock) {
        (Ok(selection), Ok(())) => Ok(selection),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

fn select_reclaim_candidates(
    coordinator: &ManagedRootCoordinator,
    scan: &mut JanitorScan,
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    let guard = CoordinatorLockGuard::acquire(coordinator)?;
    let selection = select_reclaim_candidates_locked(coordinator, scan);
    let unlock = guard.unlock(&coordinator.path);
    match (selection, unlock) {
        (Ok(selection), Ok(())) => Ok(selection),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

fn select_reclaim_candidates_locked(
    coordinator: &ManagedRootCoordinator,
    scan: &mut JanitorScan,
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    select_reclaim_candidates_locked_with_hooks(
        coordinator,
        scan,
        JANITOR_SELECTION_BUDGET,
        &std::time::Instant::now,
        &|| {},
    )
}

fn select_reclaim_candidates_locked_with_hooks(
    coordinator: &ManagedRootCoordinator,
    scan: &mut JanitorScan,
    budget: Duration,
    now: &impl Fn() -> std::time::Instant,
    after_enumeration: &impl Fn(),
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    let deadline = now().checked_add(budget).ok_or_else(|| {
        WorkspaceError::io(
            "select abandoned workspaces",
            &coordinator.path,
            "selection deadline overflow",
        )
    })?;
    select_reclaim_candidates_locked_until_with_hooks(
        coordinator,
        scan,
        deadline,
        now,
        after_enumeration,
        &|| {},
    )
}

fn select_reclaim_candidates_locked_until_with_hooks(
    coordinator: &ManagedRootCoordinator,
    scan: &mut JanitorScan,
    deadline: std::time::Instant,
    now: &impl Fn() -> std::time::Instant,
    after_enumeration: &impl Fn(),
    after_entry: &impl Fn(),
) -> Result<(Vec<String>, Option<String>), WorkspaceError> {
    ensure_selection_deadline(coordinator, scan, deadline, now)?;
    let coordinator_path = coordinator.path.join(COORDINATOR_FILE);
    let (active_slot, state) = read_coordinator_state(&coordinator.file, &coordinator_path)?;
    ensure_selection_deadline(coordinator, scan, deadline, now)?;
    let mut entries = owned_directory_entries(&coordinator.dir)
        .map_err(|error| WorkspaceError::io("enumerate managed roots", &coordinator.path, error))?;
    ensure_selection_deadline(coordinator, scan, deadline, now)?;
    let mut examined = 0_usize;
    let mut after = BTreeSet::new();
    let mut wrapped = BTreeSet::new();
    loop {
        ensure_selection_deadline(coordinator, scan, deadline, now)?;
        let entry = entries.next();
        ensure_selection_deadline(coordinator, scan, deadline, now)?;
        let Some(entry) = entry else {
            break;
        };
        let entry = entry.map_err(|error| {
            WorkspaceError::io("enumerate managed roots", &coordinator.path, error)
        })?;
        after_entry();
        ensure_selection_deadline(coordinator, scan, deadline, now)?;
        examined = examined
            .checked_add(1)
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        if examined > MAX_MANAGED_CHILDREN {
            scan.stop_reason = Some(JanitorReason::SelectionChildLimit);
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
        scan.observed_matching_entries += 1;
        if name.as_str() > state.cursor.as_str() {
            scan.selection_limited |= after.len() >= MAX_RECLAIM_CANDIDATES;
            insert_bounded_name(&mut after, name);
        } else {
            scan.selection_limited |= wrapped.len() >= MAX_RECLAIM_CANDIDATES;
            insert_bounded_name(&mut wrapped, name);
        }
    }
    scan.directory_scan_complete = true;
    after_enumeration();
    ensure_selection_deadline(coordinator, scan, deadline, now)?;
    scan.cursor_deferred = !after.is_empty() && !wrapped.is_empty();
    let candidates = if after.is_empty() { wrapped } else { after }
        .into_iter()
        .collect::<Vec<_>>();
    ensure_selection_deadline(coordinator, scan, deadline, now)?;
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

fn ensure_selection_deadline(
    coordinator: &ManagedRootCoordinator,
    scan: &mut JanitorScan,
    deadline: std::time::Instant,
    now: &impl Fn() -> std::time::Instant,
) -> Result<(), WorkspaceError> {
    if now() < deadline {
        Ok(())
    } else {
        scan.stop_reason = Some(JanitorReason::SelectionDeadline);
        Err(WorkspaceError::io(
            "select abandoned workspaces",
            &coordinator.path,
            "selection deadline exceeded",
        ))
    }
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

// Nested filesystem operations keep the caller's stop predicate when capping or
// subtracting a local slice; a fresh duration alone would lose cancellation.
#[derive(Clone, Copy)]
struct CleanupBudget<'a> {
    duration: Duration,
    stopped: Option<&'a dyn Fn() -> bool>,
}

impl From<Duration> for CleanupBudget<'_> {
    fn from(duration: Duration) -> Self {
        Self {
            duration,
            stopped: None,
        }
    }
}

impl CleanupBudget<'_> {
    fn min(self, duration: Duration) -> Self {
        Self {
            duration: self.duration.min(duration),
            ..self
        }
    }
    fn saturating_sub(self, elapsed: Duration) -> Self {
        Self {
            duration: self.duration.saturating_sub(elapsed),
            ..self
        }
    }
}

fn remove_empty_unmarked_directory<'a>(
    parent: &cap_std::fs::Dir,
    name: &str,
    candidate: cap_std::fs::Dir,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
    progress: &mut DeletionProgress,
) -> std::io::Result<()> {
    let budget = budget.into();
    ensure_cleanup_deadline(started, budget)?;
    if directory_identity(&candidate)? != expected_identity || !staging_is_empty(&candidate) {
        return Err(std::io::Error::other(
            "empty deleting root changed before removal",
        ));
    }
    ensure_cleanup_deadline(started, budget)?;
    #[cfg(unix)]
    {
        drop(candidate);
        remove_cleanup_directory_checked(
            parent,
            std::ffi::OsStr::new(name),
            Utf8Path::new(name),
            expected_identity,
            started,
            budget,
        )?;
        progress.removed_entries += 1;
        progress.root_removed = true;
        Ok(())
    }
    #[cfg(windows)]
    {
        let candidate = candidate.into_std_file();
        super::root::windows::remove_open_entry_io_with_guard(&candidate, &|| {
            ensure_cleanup_deadline(started, budget)
        })?;
        progress.removed_entries += 1;
        progress.root_removed = true;
        drop(candidate);
        match parent.symlink_metadata(name) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(std::io::Error::other(
                "empty deleting root remains after exact-handle removal",
            )),
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "claim keeps expected directory, lease, marker, deadline, and rollback adjacent"
)]
fn claim_managed_child(
    parent: &cap_std::fs::Dir,
    current_name: &str,
    deleting_name: &str,
    run_id: &str,
    owner: OwnerKind,
    expected_dir: &cap_std::fs::Dir,
    #[cfg(windows)] rename_handle: &cap_std::fs::Dir,
    expected_lease: &mut Option<File>,
    on_deleting: &impl Fn(),
    before_next_operation: &impl Fn() -> Result<(), WorkspaceError>,
) -> Result<ClaimResult, WorkspaceError> {
    before_next_operation()?;
    let lease_path = Utf8PathBuf::from(current_name).join(LEASE_FILE);
    let expected_lease_evidence = inspect_marker_evidence(
        expected_lease.as_ref().ok_or_else(|| {
            WorkspaceError::io(
                "inspect expected lease",
                &lease_path,
                "managed root lease is unavailable",
            )
        })?,
        &lease_path,
    )?;
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
    if candidate_lease_identity != expected_lease_evidence.identity {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(claimed_name),
        });
    }
    before_next_operation()?;
    let marker = &expected_lease_evidence.marker;
    if marker.schema != LEASE_SCHEMA || marker.run_id != run_id || marker.owner != owner {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(current_name),
        });
    }
    before_next_operation()?;
    drop(candidate_lease);
    if claimed_name == deleting_name {
        on_deleting();
        #[cfg(windows)]
        drop(expected_lease.take());
    } else {
        before_next_operation()?;
        #[cfg(windows)]
        drop(expected_lease.take());
        #[cfg(unix)]
        let rename = rename_owned_directory_with_guard(
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
        });
        #[cfg(windows)]
        let rename = rename_open_owned_directory_with_guard(
            parent,
            rename_handle,
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
        });
        #[cfg(unix)]
        rename?;
        #[cfg(windows)]
        {
            if let Err(error) = rename {
                match reopen_locked_lease(
                    &candidate,
                    &expected_lease_evidence,
                    Utf8Path::new(current_name),
                ) {
                    Ok(lease) => {
                        *expected_lease = Some(lease);
                        return Err(error);
                    }
                    Err(restore_error) => {
                        return Err(WorkspaceError::io(
                            "restore workspace lease after failed claim",
                            Utf8Path::new(current_name),
                            format!("claim failed: {error}; lease restore failed: {restore_error}"),
                        ));
                    }
                }
            }
        }
        on_deleting();
    }
    #[cfg(windows)]
    {
        *expected_lease = Some(reopen_locked_lease(
            &candidate,
            &expected_lease_evidence,
            Utf8Path::new(deleting_name),
        )?);
    }
    Ok(ClaimResult::Claimed)
}

struct EligibilityFailure {
    reason: JanitorReason,
    cause: String,
}

impl EligibilityFailure {
    fn new(reason: JanitorReason, cause: impl std::fmt::Display) -> Self {
        Self {
            reason,
            cause: cause.to_string(),
        }
    }
}

fn candidate_is_stale(
    candidate: &cap_std::fs::Dir,
    lease_marker: &LeaseMarker,
    now: SystemTime,
) -> Result<(), EligibilityFailure> {
    let heartbeat = open_regular_file_nofollow(candidate, HEARTBEAT_FILE)
        .map_err(|error| EligibilityFailure::new(JanitorReason::HeartbeatInvalid, error))?;
    let heartbeat_marker = read_json_file_checked::<LeaseMarker>(&heartbeat)
        .map_err(|error| EligibilityFailure::new(JanitorReason::HeartbeatInvalid, error))?;
    if lease_marker.schema != LEASE_SCHEMA
        || heartbeat_marker.schema != LEASE_SCHEMA
        || heartbeat_marker.run_id != lease_marker.run_id
        || heartbeat_marker.owner != lease_marker.owner
    {
        return Err(EligibilityFailure::new(
            JanitorReason::HeartbeatInvalid,
            "heartbeat schema or identity mismatch",
        ));
    }
    let metadata = heartbeat
        .metadata()
        .map_err(|error| EligibilityFailure::new(JanitorReason::AgeUnverifiable, error))?;
    let modified = metadata
        .modified()
        .map_err(|error| EligibilityFailure::new(JanitorReason::AgeUnverifiable, error))?;
    stale_timestamp(modified.checked_add(STALE_AFTER), now)
}

fn stale_timestamp(
    stale_at: Option<SystemTime>,
    now: SystemTime,
) -> Result<(), EligibilityFailure> {
    match stale_at {
        Some(stale_at) if now >= stale_at => Ok(()),
        Some(_) => Err(EligibilityFailure::new(
            JanitorReason::Young,
            "stale threshold not reached",
        )),
        None => Err(EligibilityFailure::new(
            JanitorReason::AgeUnverifiable,
            "stale timestamp overflow",
        )),
    }
}

fn staging_candidate_is_stale(
    candidate: &cap_std::fs::Dir,
    marker: &LeaseMarker,
    now: SystemTime,
) -> Result<(), EligibilityFailure> {
    match candidate.symlink_metadata(HEARTBEAT_FILE) {
        Ok(_) => candidate_is_stale(candidate, marker, now),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => stale_timestamp(
            UNIX_EPOCH
                .checked_add(Duration::from_secs(marker.created_unix_seconds))
                .and_then(|created| created.checked_add(STALE_AFTER)),
            now,
        ),
        Err(error) => Err(EligibilityFailure::new(
            JanitorReason::HeartbeatInvalid,
            error,
        )),
    }
}

fn directory_timestamp_is_stale(
    candidate: &cap_std::fs::Dir,
    now: SystemTime,
    report: &mut ReclaimLedger,
    path: &Utf8Path,
) -> Result<(), EligibilityFailure> {
    let timestamp = candidate
        .metadata(".")
        .and_then(|metadata| metadata.modified());
    let result = timestamp
        .map_err(|error| EligibilityFailure::new(JanitorReason::AgeUnverifiable, error))
        .and_then(|modified| stale_timestamp(modified.into_std().checked_add(STALE_AFTER), now));
    if let Err(failure) = &result
        && failure.reason != JanitorReason::Young
    {
        report.detail(
            failure.reason,
            format!("staging age for {path}: {}", failure.cause),
        );
    }
    result
}

fn candidate_is_reclaimable(
    candidate: &cap_std::fs::Dir,
    marker: &LeaseMarker,
    lease: &File,
    now: SystemTime,
    report: &mut ReclaimLedger,
    path: &Utf8Path,
) -> Result<(), EligibilityFailure> {
    // An absent optional ready marker is ordinary. Invalid evidence still falls
    // back to heartbeat age, exactly as before, while retaining its typed cause.
    let ready = match inspect_regular_file_nofollow(candidate, CLEANUP_READY_FILE) {
        Ok(file) => match read_json_file_checked::<CleanupReadyMarker>(&file) {
            Ok(ready) => Some(ready),
            Err(error) => {
                report.detail(
                    JanitorReason::MarkerInvalid,
                    format!("cleanup-ready marker for {path}: {error}"),
                );
                None
            }
        },
        Err(error) if error.absent => None,
        Err(error) => {
            report.detail(
                JanitorReason::MarkerInvalid,
                format!("cleanup-ready marker for {path}: {}", error.error),
            );
            None
        }
    };
    if let Some(ready) = ready {
        match file_identity(lease) {
            Ok((lease_device, lease_inode)) => {
                if ready.schema == LEASE_SCHEMA
                    && ready.run_id == marker.run_id
                    && ready.owner == marker.owner
                    && ready.lease_device == lease_device
                    && ready.lease_inode == lease_inode
                {
                    return Ok(());
                }
                report.detail(
                    JanitorReason::MarkerInvalid,
                    format!("cleanup-ready marker identity mismatch: {path}"),
                );
            }
            Err(error) => report.detail(
                JanitorReason::IdentityInspectionFailed,
                format!("cleanup-ready lease identity for {path}: {error}"),
            ),
        }
    }
    candidate_is_stale(candidate, marker, now)
}

fn staging_contents_are_safe(candidate: &cap_std::fs::Dir) -> Result<(), EligibilityFailure> {
    let entries = owned_directory_entries(candidate)
        .map_err(|error| EligibilityFailure::new(JanitorReason::UnsafeStaging, error))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| EligibilityFailure::new(JanitorReason::UnsafeStaging, error))?;
        let Some(name) = entry.to_str().map(str::to_owned) else {
            return Err(EligibilityFailure::new(
                JanitorReason::UnsafeStaging,
                "non-UTF8 staging entry",
            ));
        };
        if name != LEASE_FILE && name != HEARTBEAT_FILE {
            return Err(EligibilityFailure::new(
                JanitorReason::UnsafeStaging,
                format!("unexpected staging entry {name}"),
            ));
        }
    }
    Ok(())
}

fn inspect_empty_directory(candidate: &cap_std::fs::Dir) -> std::io::Result<bool> {
    let mut entries = owned_directory_entries(candidate)?;
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(error)) => Err(error),
    }
}

fn staging_is_empty(candidate: &cap_std::fs::Dir) -> bool {
    inspect_empty_directory(candidate).unwrap_or(false)
}

fn janitor_empty_probe(
    candidate: &cap_std::fs::Dir,
    report: &mut ReclaimLedger,
    path: &Utf8Path,
) -> bool {
    inspect_empty_directory(candidate).unwrap_or_else(|error| {
        report.detail(
            JanitorReason::CandidateInspectionFailed,
            format!("inspect empty root {path}: {error}"),
        );
        false
    })
}

#[cfg(test)]
fn deleting_has_only_lease(candidate: &cap_std::fs::Dir) -> bool {
    inspect_deleting_contents(candidate).unwrap_or(false)
}

fn inspect_deleting_contents(candidate: &cap_std::fs::Dir) -> std::io::Result<bool> {
    let mut entries = owned_directory_entries(candidate)?;
    match entries.next().transpose()? {
        Some(name) if name == LEASE_FILE => Ok(entries.next().transpose()?.is_none()),
        _ => Ok(false),
    }
}

fn read_cleanup_ready_marker(candidate: &cap_std::fs::Dir) -> Option<CleanupReadyMarker> {
    read_bounded_json(candidate, CLEANUP_READY_FILE)
}

fn read_marker_file(file: &File) -> Option<LeaseMarker> {
    read_json_file(file)
}

fn inspect_marker_evidence(file: &File, path: &Utf8Path) -> Result<MarkerEvidence, WorkspaceError> {
    let identity = file_identity(file)
        .map_err(|error| WorkspaceError::io("identify workspace lease", path, error))?;
    let marker = read_marker_file(file).ok_or_else(|| WorkspaceError::InvalidPath {
        path: path.to_owned(),
    })?;
    Ok(MarkerEvidence { identity, marker })
}

#[cfg(windows)]
fn reopen_locked_lease(
    dir: &cap_std::fs::Dir,
    expected: &MarkerEvidence,
    path: &Utf8Path,
) -> Result<File, WorkspaceError> {
    let lease_path = path.join(LEASE_FILE);
    let lease = super::root::windows::open_regular_file_for_delete_pinned(
        dir,
        std::ffi::OsStr::new(LEASE_FILE),
    )
    .map_err(|error| WorkspaceError::io("reopen workspace lease", &lease_path, error))?;
    verify_current_user_owned_file(&lease)
        .map_err(|error| WorkspaceError::io("verify workspace lease owner", &lease_path, error))?;
    if file_identity(&lease)
        .map_err(|error| WorkspaceError::io("identify workspace lease", &lease_path, error))?
        != expected.identity
    {
        return Err(WorkspaceError::InvalidPath { path: lease_path });
    }
    File::try_lock(&lease)
        .map_err(|error| WorkspaceError::io("lock workspace lease", &lease_path, error))?;
    if read_marker_file(&lease).as_ref() != Some(&expected.marker) {
        return Err(WorkspaceError::InvalidPath { path: lease_path });
    }
    Ok(lease)
}

#[cfg(windows)]
fn reopen_heartbeat(
    dir: &cap_std::fs::Dir,
    expected: &MarkerEvidence,
    path: &Utf8Path,
) -> Result<File, WorkspaceError> {
    let heartbeat_path = path.join(HEARTBEAT_FILE);
    let heartbeat = super::root::windows::open_regular_file_for_update_pinned(
        dir,
        std::ffi::OsStr::new(HEARTBEAT_FILE),
    )
    .map_err(|error| WorkspaceError::io("reopen workspace heartbeat", &heartbeat_path, error))?;
    verify_current_user_owned_file(&heartbeat).map_err(|error| {
        WorkspaceError::io("verify workspace heartbeat owner", &heartbeat_path, error)
    })?;
    if inspect_marker_evidence(&heartbeat, &heartbeat_path)? != *expected {
        return Err(WorkspaceError::InvalidPath {
            path: heartbeat_path,
        });
    }
    Ok(heartbeat)
}

#[cfg(windows)]
fn reopen_published_markers(
    dir: &cap_std::fs::Dir,
    expected: &(MarkerEvidence, MarkerEvidence),
    path: &Utf8Path,
) -> Result<(File, File), WorkspaceError> {
    let lease = reopen_locked_lease(dir, &expected.0, path)?;
    let heartbeat = reopen_heartbeat(dir, &expected.1, path)?;
    Ok((lease, heartbeat))
}

fn read_json_file<T: serde::de::DeserializeOwned>(file: &File) -> Option<T> {
    read_json_file_checked(file).ok()
}

fn read_json_file_checked<T: serde::de::DeserializeOwned>(file: &File) -> std::io::Result<T> {
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(MAX_MARKER_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(std::io::Error::other("marker exceeds 64 KiB"));
    }
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

struct MarkerOpenError {
    error: WorkspaceError,
    absent: bool,
}
impl From<WorkspaceError> for MarkerOpenError {
    fn from(error: WorkspaceError) -> Self {
        Self {
            error,
            absent: false,
        }
    }
}
fn open_regular_file_nofollow(dir: &cap_std::fs::Dir, name: &str) -> Result<File, WorkspaceError> {
    inspect_regular_file_nofollow(dir, name).map_err(|issue| issue.error)
}

#[cfg(unix)]
fn inspect_regular_file_nofollow(
    dir: &cap_std::fs::Dir,
    name: &str,
) -> Result<File, MarkerOpenError> {
    let expected = dir
        .symlink_metadata(name)
        .map_err(|error| MarkerOpenError {
            absent: error.kind() == std::io::ErrorKind::NotFound,
            error: WorkspaceError::io("inspect cleanup lease", Utf8Path::new(name), error),
        })?;
    if !expected.is_file() || expected.file_type().is_symlink() {
        return Err(WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(name),
        }
        .into());
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
        }
        .into());
    }
    verify_current_user_owned_file(&file).map_err(|error| {
        WorkspaceError::io("verify cleanup lease owner", Utf8Path::new(name), error)
    })?;
    Ok(file)
}

#[cfg(windows)]
fn inspect_regular_file_nofollow(
    dir: &cap_std::fs::Dir,
    name: &str,
) -> Result<File, MarkerOpenError> {
    let file = super::root::windows::open_regular_file_shared(dir, std::ffi::OsStr::new(name))
        .map_err(|error| MarkerOpenError {
            absent: error.kind() == std::io::ErrorKind::NotFound,
            error: WorkspaceError::io("open cleanup lease", Utf8Path::new(name), error),
        })?;
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

#[derive(Default)]
struct CleanupCapabilities {
    progress: DeletionProgress,
    lease_guard: Option<File>,
    #[cfg(windows)]
    root_authority: RootCleanupAuthority,
}

/// Whether cleanup may resolve a name or must use its originally claimed handle.
#[cfg(windows)]
#[derive(Default)]
enum RootCleanupAuthority {
    #[default]
    ByName,
    Pinned(cap_std::fs::Dir),
    /// The exact handle was consumed, not necessarily that the name is absent.
    /// Never reopen a same-name replacement to regain deletion authority.
    Consumed,
}

impl CleanupCapabilities {
    #[cfg(unix)]
    fn with_lease(lease_guard: Option<File>) -> Self {
        Self {
            lease_guard,
            progress: DeletionProgress::default(),
        }
    }

    #[cfg(windows)]
    fn claimed(root_pin: cap_std::fs::Dir, lease_guard: File) -> Self {
        Self {
            lease_guard: Some(lease_guard),
            root_authority: RootCleanupAuthority::Pinned(root_pin),
            progress: DeletionProgress::default(),
        }
    }
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
    let metadata = entry.metadata()?;
    let identity = super::root::windows::file_identity_io(&entry)?;
    if metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        Ok(CleanupEntry::Directory(identity))
    } else {
        Ok(CleanupEntry::Other(identity))
    }
}

fn remove_claimed_tree_slice_with_capabilities<'a>(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_root_identity: Option<(u64, u64)>,
    budget: impl Into<CleanupBudget<'a>>,
    capabilities: &mut CleanupCapabilities,
) -> std::io::Result<RemovalSlice> {
    let budget = budget.into();
    let started = std::time::Instant::now();
    let slice_budget = budget.min(MAX_CLEANUP_SLICE_DURATION);
    let mut examined = 0_u64;
    let mut removed = 0_u64;
    loop {
        if examined >= u64::try_from(MAX_CLEANUP_SLICE_ENTRIES).expect("fixed limit")
            || cleanup_deadline_reached(started, slice_budget)
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
        let progress = match remove_one_claimed_entry_with_capabilities(
            parent,
            name,
            expected_root_identity,
            u64::try_from(MAX_CLEANUP_SLICE_ENTRIES).expect("fixed limit") - examined,
            started,
            slice_budget,
            capabilities,
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

#[cfg(all(test, unix))]
#[allow(
    clippy::too_many_lines,
    reason = "single-step deletion keeps every identity and cooperative-bound check adjacent"
)]
fn remove_one_claimed_entry<'a>(
    parent: &cap_std::fs::Dir,
    root_name: &str,
    expected_root_identity: Option<(u64, u64)>,
    max_examined: u64,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<RemovalSlice> {
    let budget = budget.into();
    remove_one_claimed_entry_with_capabilities(
        parent,
        root_name,
        expected_root_identity,
        max_examined,
        started,
        budget,
        &mut CleanupCapabilities::default(),
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "single-step deletion keeps every identity and cooperative-bound check adjacent"
)]
fn remove_one_claimed_entry_with_capabilities<'a>(
    parent: &cap_std::fs::Dir,
    root_name: &str,
    expected_root_identity: Option<(u64, u64)>,
    max_examined: u64,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
    capabilities: &mut CleanupCapabilities,
) -> std::io::Result<RemovalSlice> {
    let budget = budget.into();
    let mut current = match open_cleanup_root(
        parent,
        std::ffi::OsStr::new(root_name),
        expected_root_identity,
        started,
        budget,
        capabilities,
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
    let mut components: Vec<(OsString, (u64, u64))> = Vec::new();
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
                    capabilities.progress.examined_entries += 1;
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
                let marker_progress =
                    remove_root_control_markers(&current, started, budget, capabilities)?;
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
                remove_cleanup_root_checked(
                    parent,
                    std::ffi::OsStr::new(root_name),
                    Utf8Path::new(root_name),
                    root_identity,
                    started,
                    budget,
                    capabilities,
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
            capabilities.progress.removed_entries += 1;
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
            capabilities.progress.removed_entries += 1;
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

fn remove_root_control_markers<'a>(
    root: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
    capabilities: &mut CleanupCapabilities,
) -> std::io::Result<ControlMarkerProgress> {
    let budget = budget.into();
    let mut removed = 0_u64;
    for marker in [RETAIN_FILE, HEARTBEAT_FILE, CLEANUP_READY_FILE, LEASE_FILE] {
        if cleanup_deadline_reached(started, budget) {
            return Ok(ControlMarkerProgress {
                removed,
                deferred: true,
            });
        }
        let marker_name = std::ffi::OsStr::new(marker);
        #[cfg(windows)]
        if marker == LEASE_FILE
            && !matches!(capabilities.root_authority, RootCleanupAuthority::ByName)
        {
            if remove_pinned_cleanup_lease(root, started, budget, capabilities)? {
                removed = removed.saturating_add(1);
            }
            continue;
        }
        let marker_identity = match cleanup_entry_identity_if_owned(root, marker_name) {
            Ok(identity) => identity,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if marker == LEASE_FILE {
                    drop(capabilities.lease_guard.take());
                }
                continue;
            }
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
            Ok(()) => {
                capabilities.progress.removed_entries += 1;
                if marker == LEASE_FILE {
                    drop(capabilities.lease_guard.take());
                }
                removed = removed.saturating_add(1);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if marker == LEASE_FILE {
                    drop(capabilities.lease_guard.take());
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(ControlMarkerProgress {
        removed,
        deferred: false,
    })
}

#[cfg(windows)]
fn remove_pinned_cleanup_lease<'a>(
    root: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
    capabilities: &mut CleanupCapabilities,
) -> std::io::Result<bool> {
    let budget = budget.into();
    ensure_cleanup_deadline(started, budget)?;
    let canonical_identity =
        match cleanup_entry_identity_if_owned(root, std::ffi::OsStr::new(LEASE_FILE)) {
            Ok(identity) => identity,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                drop(capabilities.lease_guard.take());
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
    let lease = capabilities.lease_guard.take().ok_or_else(|| {
        std::io::Error::other(
            "claimed cleanup lease remains after its exact deletion capability was consumed",
        )
    })?;
    let validation = (|| {
        ensure_cleanup_deadline(started, budget)?;
        verify_current_user_owned_file(&lease)?;
        if file_identity(&lease)? != canonical_identity {
            return Err(std::io::Error::other(
                "claimed cleanup lease capability changed identity before removal",
            ));
        }
        Ok(())
    })();
    if let Err(error) = validation {
        capabilities.lease_guard = Some(lease);
        return Err(error);
    }
    if let Err(error) = super::root::windows::remove_open_entry_io_with_guard(&lease, &|| {
        ensure_cleanup_deadline(started, budget)
    }) {
        capabilities.lease_guard = Some(lease);
        return Err(error);
    }
    capabilities.progress.removed_entries += 1;
    drop(lease);
    match root.symlink_metadata(LEASE_FILE) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
        Ok(_) => Err(std::io::Error::other(
            "claimed cleanup lease remains after exact-handle removal",
        )),
    }
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

fn cleanup_deadline_reached<'a>(
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> bool {
    let budget = budget.into();
    budget.stopped.is_some_and(|stopped| stopped()) || started.elapsed() >= budget.duration
}

fn ensure_cleanup_deadline<'a>(
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    let budget = budget.into();
    if cleanup_deadline_reached(started, budget) {
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "cleanup slice cooperative deadline reached",
        ))
    } else {
        Ok(())
    }
}

fn reopen_cleanup_parent<'a>(
    parent: &cap_std::fs::Dir,
    root_name: &str,
    root_identity: (u64, u64),
    components: &[(OsString, (u64, u64))],
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<cap_std::fs::Dir> {
    let budget = budget.into();
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

fn open_cleanup_root<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    expected_identity: Option<(u64, u64)>,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
    capabilities: &CleanupCapabilities,
) -> std::io::Result<cap_std::fs::Dir> {
    let budget = budget.into();
    #[cfg(windows)]
    match &capabilities.root_authority {
        RootCleanupAuthority::ByName => {}
        RootCleanupAuthority::Consumed => {
            return match parent.symlink_metadata(name) {
                Err(error) => Err(error),
                Ok(_) => Err(std::io::Error::other(
                    "claimed cleanup root remains after its exact deletion capability was consumed",
                )),
            };
        }
        RootCleanupAuthority::Pinned(root) => {
            ensure_cleanup_deadline(started, budget)?;
            let owner_verifier = root.try_clone()?.into_std_file();
            windows::verify_current_user_owner(&owner_verifier)?;
            drop(owner_verifier);
            let identity = directory_identity(root)?;
            if expected_identity.is_some_and(|expected| expected != identity) {
                return Err(std::io::Error::other(
                    "claimed cleanup root capability changed identity",
                ));
            }
            ensure_cleanup_deadline(started, budget)?;
            return root.try_clone();
        }
    }
    #[cfg(unix)]
    let _ = capabilities;
    open_cleanup_directory(parent, name, expected_identity, started, budget)
}

#[cfg(unix)]
fn open_cleanup_directory<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    expected_identity: Option<(u64, u64)>,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<cap_std::fs::Dir> {
    let budget = budget.into();
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
fn open_cleanup_directory<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    expected_identity: Option<(u64, u64)>,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<cap_std::fs::Dir> {
    let budget = budget.into();
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
fn make_cleanup_child_accessible<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    metadata: &cap_std::fs::Metadata,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    use cap_std::fs::PermissionsExt;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let budget = budget.into();

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
fn make_cleanup_directory_accessible<'a>(
    dir: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let budget = budget.into();

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
fn make_cleanup_directory_accessible<'a>(
    dir: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    let budget = budget.into();
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
fn remove_cleanup_entry_checked<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    let budget = budget.into();
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
fn remove_cleanup_directory_checked<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    let budget = budget.into();
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
fn remove_cleanup_directory_checked<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    _logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    let budget = budget.into();
    super::root::windows::remove_entry_io_checked_with_guard(
        parent,
        name,
        expected_identity,
        &|| ensure_cleanup_deadline(started, budget),
    )
}

#[cfg(windows)]
fn remove_cleanup_entry_checked<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    _logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    let budget = budget.into();
    super::root::windows::remove_entry_io_checked_with_guard(
        parent,
        name,
        expected_identity,
        &|| ensure_cleanup_deadline(started, budget),
    )
}

fn remove_cleanup_root_checked<'a>(
    parent: &cap_std::fs::Dir,
    name: &std::ffi::OsStr,
    logical_path: &Utf8Path,
    expected_identity: (u64, u64),
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
    capabilities: &mut CleanupCapabilities,
) -> std::io::Result<()> {
    let budget = budget.into();
    #[cfg(windows)]
    if !matches!(capabilities.root_authority, RootCleanupAuthority::ByName) {
        ensure_cleanup_deadline(started, budget)?;
        let RootCleanupAuthority::Pinned(root) = std::mem::replace(
            &mut capabilities.root_authority,
            RootCleanupAuthority::Consumed,
        ) else {
            return Err(std::io::Error::other(
                "claimed cleanup root deletion capability is unavailable",
            ));
        };
        let owner_verification = root
            .try_clone()
            .map(cap_std::fs::Dir::into_std_file)
            .and_then(|file| windows::verify_current_user_owner(&file));
        if let Err(error) = owner_verification {
            capabilities.root_authority = RootCleanupAuthority::Pinned(root);
            return Err(error);
        }
        match directory_identity(&root) {
            Ok(identity) if identity == expected_identity => {}
            Ok(_) => {
                capabilities.root_authority = RootCleanupAuthority::Pinned(root);
                return Err(std::io::Error::other(
                    "claimed cleanup root capability changed identity before removal",
                ));
            }
            Err(error) => {
                capabilities.root_authority = RootCleanupAuthority::Pinned(root);
                return Err(error);
            }
        }
        let root = root.into_std_file();
        if let Err(error) = super::root::windows::remove_open_entry_io_with_guard(&root, &|| {
            ensure_cleanup_deadline(started, budget)
        }) {
            capabilities.root_authority =
                RootCleanupAuthority::Pinned(cap_std::fs::Dir::from_std_file(root));
            return Err(error);
        }
        capabilities.progress.removed_entries += 1;
        capabilities.progress.root_removed = true;
        drop(root);
        // Successful deletion consumes authority even if the absence check fails.
        return match parent.symlink_metadata(name) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(std::io::Error::other(
                "claimed cleanup root remains after exact-handle removal",
            )),
        };
    }
    remove_cleanup_directory_checked(
        parent,
        name,
        logical_path,
        expected_identity,
        started,
        budget,
    )?;
    capabilities.progress.removed_entries += 1;
    capabilities.progress.root_removed = true;
    Ok(())
}

fn metadata_identity(metadata: &cap_std::fs::Metadata) -> (u64, u64) {
    use cap_fs_ext::MetadataExt;

    (metadata.dev(), metadata.ino())
}

#[cfg(target_os = "macos")]
type MacosMountIdentity = [u8; size_of::<libc::fsid_t>()];

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
    let mut identity = [0_u8; size_of::<libc::fsid_t>()];
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
fn ensure_same_cleanup_mount<'a>(
    parent: &cap_std::fs::Dir,
    child: &cap_std::fs::Dir,
    started: std::time::Instant,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<()> {
    let budget = budget.into();
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

fn remove_claimed_tree_bounded<'a>(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_root_identity: Option<(u64, u64)>,
    budget: impl Into<CleanupBudget<'a>>,
) -> std::io::Result<RemovalSlice> {
    let budget = budget.into();
    remove_claimed_tree_bounded_with_capabilities(
        parent,
        name,
        expected_root_identity,
        budget,
        &mut CleanupCapabilities::default(),
    )
}

fn remove_claimed_tree_bounded_with_capabilities<'a>(
    parent: &cap_std::fs::Dir,
    name: &str,
    expected_root_identity: Option<(u64, u64)>,
    budget: impl Into<CleanupBudget<'a>>,
    capabilities: &mut CleanupCapabilities,
) -> std::io::Result<RemovalSlice> {
    let budget = budget.into();
    let started = std::time::Instant::now();
    let mut total = RemovalSlice {
        complete: false,
        examined: 0,
        removed: 0,
    };
    while !cleanup_deadline_reached(started, budget) {
        let slice = remove_claimed_tree_slice_with_capabilities(
            parent,
            name,
            expected_root_identity,
            budget.saturating_sub(started.elapsed()),
            capabilities,
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
    dir: Option<cap_std::fs::Dir>,
    parent: Option<cap_std::fs::Dir>,
    name: String,
    lifecycle: Arc<Mutex<RootLifecycle>>,
    // A detached blocking workspace task may outlive its owner future. Holding the original
    // locked file keeps startup janitors out until that task drops its managed child.
    lease: Option<Arc<File>>,
    #[cfg(all(test, windows))]
    drop_gate: Option<Arc<ManagedChildDropGate>>,
}

#[cfg(all(test, windows))]
#[derive(Debug)]
struct ManagedChildDropGate {
    before_closed_state: Arc<std::sync::Barrier>,
    release: Arc<std::sync::Barrier>,
    after_published_state: Option<Arc<std::sync::Barrier>>,
    after_published_release: Option<Arc<std::sync::Barrier>>,
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        // Close every child-owned OS handle before publishing that no live child remains.
        // Cleanup may run on another thread as soon as the lifecycle count reaches zero; leaving
        // Rust's automatic field drop until after this method returns creates a Windows rename
        // race even though the logical child count is already quiescent.
        drop(self.dir.take());
        drop(self.parent.take());
        // Keep the lease locked until the child and parent directory handles are gone, so an
        // external janitor cannot begin a claim during this drop transition.
        drop(self.lease.take());
        #[cfg(all(test, windows))]
        if let Some(gate) = &self.drop_gate {
            gate.before_closed_state.wait();
            gate.release.wait();
        }
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lifecycle.live_children = lifecycle.live_children.saturating_sub(1);
        drop(lifecycle);
        #[cfg(all(test, windows))]
        if let Some(gate) = &self.drop_gate
            && let (Some(published), Some(release)) =
                (&gate.after_published_state, &gate.after_published_release)
        {
            published.wait();
            release.wait();
        }
    }
}

impl ManagedChild {
    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }

    #[cfg(all(test, windows))]
    pub(crate) fn pause_after_quiescence_for_test(
        &mut self,
    ) -> (Arc<std::sync::Barrier>, Arc<std::sync::Barrier>) {
        let published = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        self.drop_gate = Some(Arc::new(ManagedChildDropGate {
            before_closed_state: Arc::new(std::sync::Barrier::new(1)),
            release: Arc::new(std::sync::Barrier::new(1)),
            after_published_state: Some(Arc::clone(&published)),
            after_published_release: Some(Arc::clone(&release)),
        }));
        (published, release)
    }

    pub(crate) fn cleanup(&self) -> Result<(), WorkspaceError> {
        let dir = self
            .dir
            .as_ref()
            .expect("managed child directory is live before drop");
        let parent = self
            .parent
            .as_ref()
            .expect("managed child parent is live before drop");
        let identity = directory_identity(dir)
            .map_err(|error| WorkspaceError::io("identify managed child", &self.path, error))?;
        let result =
            remove_claimed_tree_bounded(parent, &self.name, Some(identity), OWNER_CLEANUP_BUDGET)
                .map_err(|error| WorkspaceError::io("remove managed child", &self.path, error))?;
        if !result.complete {
            return Err(WorkspaceError::io(
                "remove managed child",
                &self.path,
                "cleanup budget exhausted",
            ));
        }
        match parent.symlink_metadata(&self.name) {
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
