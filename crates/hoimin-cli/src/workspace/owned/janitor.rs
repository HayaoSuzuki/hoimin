use super::{MAX_DIAGNOSTIC_DETAILS, truncate_diagnostic_detail};
use hoimin_core::{JanitorDetail, JanitorReason, JanitorSummary};

/// One ledger entry per inspected candidate. Pending rounds retain its index.
#[derive(Default)]
pub(super) struct ReclaimLedger {
    pub(super) report: JanitorSummary,
    roots: Vec<RootEvidence>,
}

struct RootEvidence {
    outcome: RootOutcome,
    examined: u64,
    removed: u64,
}

enum RootOutcome {
    Reclaimed,
    Absent,
    Held(JanitorReason),
}

/// Transient operation evidence survives errors and is consumed at root finalization.
/// `root_removed` is never retained alongside a finalized root outcome.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DeletionProgress {
    pub(super) examined_entries: u64,
    pub(super) removed_entries: u64,
    pub(super) root_removed: bool,
}

impl ReclaimLedger {
    pub(super) fn begin(&mut self) -> usize {
        let index = self.roots.len();
        self.roots.push(RootEvidence {
            outcome: RootOutcome::Held(JanitorReason::CleanupIncomplete),
            examined: 0,
            removed: 0,
        });
        index
    }

    pub(super) fn hold(&mut self, index: usize, reason: JanitorReason, message: String) {
        self.roots[index].outcome = RootOutcome::Held(reason);
        self.detail(reason, message);
    }

    pub(super) fn absent(&mut self, index: usize) {
        self.roots[index].outcome = RootOutcome::Absent;
    }

    pub(super) fn removal(&mut self, index: usize, progress: DeletionProgress, absent: bool) {
        let root = &mut self.roots[index];
        root.examined = progress.examined_entries;
        root.removed = progress.removed_entries;
        if progress.root_removed {
            root.outcome = RootOutcome::Reclaimed;
        } else if absent {
            root.outcome = RootOutcome::Absent;
        }
    }

    pub(super) fn detail(&mut self, reason: JanitorReason, message: String) {
        let omissions = &mut self.report.detail_omissions;
        if self.report.details.len() == MAX_DIAGNOSTIC_DETAILS {
            if !reason.is_normal_hold()
                && let Some(index) = self
                    .report
                    .details
                    .iter()
                    .rposition(|detail| detail.reason.is_normal_hold())
            {
                self.report.details.remove(index);
                omissions.normal_details += 1;
            } else {
                if reason.is_normal_hold() {
                    omissions.normal_details += 1;
                } else {
                    omissions.issue_details += 1;
                }
                return;
            }
        }
        let (message, truncated) = truncate_diagnostic_detail(message);
        omissions.truncated_details += u64::from(truncated);
        self.report.details.push(JanitorDetail { reason, message });
    }

    /// Counts are derived once from final evidence, never maintained beside it.
    pub(super) fn finish(mut self) -> JanitorSummary {
        for root in self.roots {
            self.report.inspected_roots += 1;
            self.report.examined_entries += root.examined;
            self.report.removed_entries += root.removed;
            match root.outcome {
                RootOutcome::Reclaimed => self.report.reclaimed_roots += 1,
                RootOutcome::Absent => self.report.absent_roots += 1,
                RootOutcome::Held(reason) if reason.is_normal_hold() => {
                    self.report.preserved_roots += 1;
                    *self.report.preserved_by_reason.entry(reason).or_default() += 1;
                }
                RootOutcome::Held(reason) => {
                    self.report.unresolved_roots += 1;
                    *self.report.unresolved_by_reason.entry(reason).or_default() += 1;
                }
            }
        }
        self.report.scan.uninspected_selected_roots = self
            .report
            .scan
            .selected_roots
            .saturating_sub(self.report.inspected_roots);
        self.report.scan.selection_omitted_observed_entries = self
            .report
            .scan
            .observed_matching_entries
            .saturating_sub(self.report.scan.selected_roots);
        self.report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_failure_evicts_an_ordinary_detail_at_capacity() {
        // Arrange.
        let mut ledger = ReclaimLedger::default();
        for _ in 0..256 {
            ledger.detail(JanitorReason::Young, "ordinary root".into());
        }
        // Act.
        ledger.detail(JanitorReason::DeleteFailed, "failed root: denied".into());
        let mut report = ledger.finish();
        report.project_details(false);
        // Assert.
        assert_eq!(report.details.len(), 1);
        assert_eq!(report.details[0].message, "failed root: denied");
        assert_eq!(report.detail_omissions.normal_details, 1);
        assert_eq!(report.detail_omissions.suppressed_normal_details, 255);
    }

    #[test]
    fn acknowledged_root_stays_reclaimed_after_unlock_failure() {
        // Arrange.
        let mut ledger = ReclaimLedger::default();
        let index = ledger.begin();
        ledger.hold(
            index,
            JanitorReason::CoordinatorFailed,
            "unlock root: denied".into(),
        );
        // Act: the actual removal evidence is independent of that later failure.
        ledger.removal(
            index,
            DeletionProgress {
                removed_entries: 1,
                root_removed: true,
                ..Default::default()
            },
            false,
        );
        let report = ledger.finish();
        // Assert.
        assert_eq!(
            (
                report.inspected_roots,
                report.reclaimed_roots,
                report.unresolved_roots
            ),
            (1, 1, 0)
        );
        assert_eq!(report.removed_entries, 1);
        assert_eq!(report.details[0].reason, JanitorReason::CoordinatorFailed);
    }

    #[test]
    fn observed_absence_does_not_invent_a_root_deletion() {
        // Arrange.
        let mut ledger = ReclaimLedger::default();
        let index = ledger.begin();
        // Act.
        ledger.removal(index, DeletionProgress::default(), true);
        let report = ledger.finish();
        // Assert.
        assert_eq!(
            (
                report.absent_roots,
                report.reclaimed_roots,
                report.removed_entries
            ),
            (1, 0, 0)
        );
    }

    proptest::proptest! {
        #[test]
        fn final_counts_partition_observed_outcomes(kinds in proptest::collection::vec(0u16..4, 0..257)) {
            // Arrange: independent four-category oracle.
            let expected = [0u16, 1, 2, 3].map(|kind| kinds.iter().filter(|&&value| value == kind).count() as u64);
            let mut ledger = ReclaimLedger::default();
            for kind in kinds {
                let index = ledger.begin();
                match kind {
                    0 => ledger.removal(index, DeletionProgress { removed_entries: 2, root_removed: true, ..Default::default() }, true),
                    1 => ledger.absent(index),
                    2 => ledger.hold(index, JanitorReason::Young, "young".into()),
                    _ => ledger.hold(index, JanitorReason::DeleteFailed, "failed".into()),
                }
            }
            // Act.
            let report = ledger.finish();
            // Assert.
            proptest::prop_assert_eq!([report.reclaimed_roots, report.absent_roots, report.preserved_roots, report.unresolved_roots], expected);
            proptest::prop_assert_eq!(report.inspected_roots, expected.iter().sum::<u64>());
            proptest::prop_assert_eq!(report.removed_entries, expected[0] * 2);
        }
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::super::*;

    #[test]
    fn stopped_preparation_does_not_start_janitor_selection() {
        // Arrange.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        create_owned_directory(
            &coordinator.dir,
            &format!("run-{}", uuid::Uuid::from_u128(1)),
        )
        .unwrap();

        // Act.
        let report = ManagedRunRoot::reclaim_abandoned_controlled(
            &coordinator,
            SystemTime::now(),
            tokio::time::Instant::now() + Duration::from_secs(30),
            &|| true,
        );

        // Assert.
        assert!(!report.scan.directory_scan_complete);
        assert_eq!(report.scan.observed_matching_entries, 0);
        assert_eq!(report.inspected_roots, 0);
    }

    #[test]
    fn preparation_stop_after_first_janitor_entry_preserves_cursor() {
        // Arrange: stop exactly when the iterator has produced its first entry.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        for id in 1..=3 {
            create_owned_directory(
                &coordinator.dir,
                &format!("run-{}", uuid::Uuid::from_u128(id)),
            )
            .unwrap();
        }
        let before = std::fs::read(coordinator.path.join(COORDINATOR_FILE)).unwrap();
        let stopped = std::cell::Cell::new(false);
        let entries = std::cell::Cell::new(0);
        let mut scan = JanitorScan::default();

        // Act: remaining time becomes zero during enumeration, without sleeping.
        let result = select_reclaim_candidates_controlled_with_hooks(
            &coordinator,
            &mut scan,
            &|| {
                if stopped.get() {
                    Duration::ZERO
                } else {
                    Duration::from_secs(1)
                }
            },
            &|| {
                entries.set(entries.get() + 1);
                stopped.set(true);
            },
        );

        // Assert: traversal stops immediately and never persists a new cursor.
        assert!(
            result.is_err(),
            "stopped enumeration returned a completed selection"
        );
        assert_eq!(entries.get(), 1);
        assert!(!scan.directory_scan_complete);
        assert_eq!(scan.stop_reason, Some(JanitorReason::SelectionDeadline));
        assert_eq!(
            std::fs::read(coordinator.path.join(COORDINATOR_FILE)).unwrap(),
            before
        );
    }

    #[test]
    fn preparation_stop_after_first_deletion_preserves_remaining_entries() {
        // Arrange: the shared stop becomes true immediately after one payload file disappears.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let paths = ["a.txt", "b.txt", "c.txt"].map(|name| root.path().join(name));
        for path in &paths {
            std::fs::write(path, "payload").unwrap();
        }
        let stopped = || paths.iter().any(|path| !path.exists());
        let budget = CleanupBudget {
            duration: Duration::from_secs(5),
            stopped: Some(&stopped),
        };
        let mut capabilities = CleanupCapabilities::default();

        // Act: execute an ordinary deletion slice while its shared control changes.
        let result = remove_claimed_tree_slice_with_capabilities(
            &coordinator.dir,
            root.path().file_name().unwrap(),
            None,
            budget,
            &mut capabilities,
        );

        // Assert: acknowledged work remains reported, and no next payload deletion starts.
        assert!(result.is_err() || result.as_ref().is_ok_and(|slice| !slice.complete));
        assert_eq!(paths.iter().filter(|path| path.exists()).count(), 2);
        assert_eq!(capabilities.progress.removed_entries, 1);
        assert!(root.path().join(LEASE_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn controlled_janitor_propagates_stop_into_claimed_tree_deletion() {
        // Arrange: retain a directory capability so the stop observation survives claim rename.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let payload = ["a.txt", "b.txt", "c.txt"];
        for name in payload {
            std::fs::write(root.path().join(name), "payload").unwrap();
        }
        let observed_root = root.dir.try_clone().unwrap();
        root.mark_cleanup_ready().unwrap();
        std::fs::File::unlock(root.lease.lock().unwrap().as_deref().unwrap()).unwrap();
        drop(root);
        let stopped = || {
            payload
                .iter()
                .any(|name| observed_root.symlink_metadata(name).is_err())
        };

        // Act: cancellation becomes true immediately after the first real payload deletion.
        let report = ManagedRunRoot::reclaim_abandoned_controlled(
            &coordinator,
            SystemTime::now(),
            tokio::time::Instant::now() + Duration::from_secs(30),
            &stopped,
        );

        // Assert: the controlled public path retains acknowledged work and the remaining root.
        assert_eq!(report.removed_entries, 1, "{report:?}");
        assert_eq!(report.unresolved_roots, 1, "{report:?}");
        assert_eq!(report.reclaimed_roots, 0);
        assert_eq!(
            payload
                .iter()
                .filter(|name| observed_root.symlink_metadata(name).is_ok())
                .count(),
            2
        );
        assert!(observed_root.symlink_metadata(LEASE_FILE).is_ok());
    }

    macro_rules! selection_case {
        ($name:ident, $count:expr, $selected:expr, $omitted:expr) => {
            #[test]
            fn $name() {
                // Arrange: canonical names, no lease checks needed for selection.
                let parent = tempfile::tempdir().unwrap();
                let coordinator =
                    ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap())
                        .unwrap();
                for id in 1..=$count {
                    create_owned_directory(
                        &coordinator.dir,
                        &format!("run-{}", uuid::Uuid::from_u128(id)),
                    )
                    .unwrap();
                }
                // Act.
                let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
                // Assert.
                assert!(report.scan.directory_scan_complete);
                assert_eq!(report.scan.observed_matching_entries, $count as u64);
                assert_eq!(report.scan.selected_roots, $selected);
                assert_eq!(report.scan.selection_omitted_observed_entries, $omitted);
                assert_eq!(report.scan.selection_limited, $omitted != 0);
                assert_eq!(report.inspected_roots, $selected);
            }
        };
    }
    selection_case!(exact_selection_limit_does_not_invent_omissions, 256, 256, 0);
    selection_case!(selection_limit_reports_observed_extra_entry, 257, 256, 1);

    #[test]
    fn cleanup_deadline_reports_selected_but_uninspected_roots() {
        // Arrange: only first root is inspected, retaining the others as unknown work.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let roots = (0..3)
            .map(|_| ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap())
            .collect::<Vec<_>>();
        let checks = std::cell::Cell::new(0);
        // Act.
        let report =
            ManagedRunRoot::reclaim_abandoned_with_clock(&coordinator, SystemTime::now(), &|_| {
                let count = checks.get();
                checks.set(count + 1);
                if count == 0 {
                    Duration::ZERO
                } else {
                    JANITOR_CLEANUP_BUDGET
                }
            });
        // Assert.
        assert_eq!((report.inspected_roots, report.preserved_roots), (1, 1));
        assert_eq!(report.scan.uninspected_selected_roots, 2);
        assert_eq!(
            report.scan.stop_reason,
            Some(JanitorReason::CleanupIncomplete)
        );
        assert!(roots.iter().all(|root| root.path().exists()));
    }

    #[test]
    fn owner_deferred_slice_preserves_nested_acknowledgements() {
        // Arrange: a nested operation acknowledged a deletion before timing out.
        // The scheduler's partial slice excludes that interrupted operation.
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let capabilities = CleanupCapabilities {
            progress: DeletionProgress {
                examined_entries: 3,
                removed_entries: 2,
                root_removed: false,
            },
            ..CleanupCapabilities::default()
        };
        let record = CleanupRecord {
            disposition: CleanupDisposition::Failed,
            evidence: CleanupEvidence {
                remaining_root: Some(root.path().to_owned()),
                ..CleanupEvidence::default()
            },
        };
        // Act: preserve the existing deferred disposition and retained capabilities.
        let record = root.finish_owner_removal(
            record,
            capabilities,
            root.path().file_name().unwrap(),
            Ok(RemovalSlice {
                complete: false,
                examined: 1,
                removed: 1,
            }),
        );
        // Assert: use acknowledgement evidence, never the scheduling slice totals.
        assert!(matches!(
            record.disposition,
            CleanupDisposition::Deferred(None)
        ));
        assert_eq!(record.evidence.removed_entries, 2);
        assert_eq!(record.evidence.examined_entries, 3);
        assert!(root.path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn slice_failure_retains_deleted_markers_from_nested_operation() {
        // Arrange: control markers are skipped during traversal; the heartbeat directory
        // fails unlink after the retain marker was actually removed.
        let parent = tempfile::tempdir().unwrap();
        let dir = cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
            .unwrap();
        let root = create_owned_directory(&dir, "deleting").unwrap();
        root.write(RETAIN_FILE, b"retain").unwrap();
        root.create_dir(HEARTBEAT_FILE).unwrap();
        let identity = directory_identity(&root).unwrap();
        let mut capabilities = CleanupCapabilities::default();
        // Act.
        let result = remove_claimed_tree_bounded_with_capabilities(
            &dir,
            "deleting",
            Some(identity),
            Duration::from_secs(5),
            &mut capabilities,
        );
        // Assert.
        assert!(result.is_err());
        assert_eq!(capabilities.progress.removed_entries, 1);
        assert!(!capabilities.progress.root_removed);
        assert!(!root.exists(RETAIN_FILE));
        assert!(root.exists(HEARTBEAT_FILE));
    }
}
