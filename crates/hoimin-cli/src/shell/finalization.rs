//! Owns execution cleanup evidence shared by normal and outer finalization.

use std::collections::BTreeMap;
use std::io;

use camino::Utf8PathBuf;
use hoimin_core::disk::{DiskCleanupOutcome, DiskComponentState, DiskLifecycleEvent, DiskRootId};
use hoimin_core::{DiskLifecycleError, RunSummary};

use super::{
    CLEANUP_QUIESCENCE_UNPROVEN, CleanupThreadFailure, ShellDiskLifecycle,
    apply_execution_cleanup_evidence, cleanup_quiescence_proven, lifecycle_safety_succeeded,
    merge_lifecycle_stop,
};
use crate::workspace::{CleanupDisposition, CleanupEvidence, CleanupRecord, FilesystemKey};

/// Quiescence is independent of whether a joined scan produced a usable reading.
/// An unjoined scan remains unsafe even if a later observation succeeds.
enum FinalMeasurement {
    Quiescent(Option<u64>),
    Unjoined,
}

/// An attempt is recorded before lifecycle admission, including rejected requests.
/// A pre-attempt deferral can carry evidence without claiming removal was tried.
enum ExecutionCleanup {
    NotAttempted(Option<CleanupRecord>),
    Attempted(Option<CleanupRecord>),
    Completed(CleanupRecord),
}

enum EndAvailable {
    NotObserved,
    Unavailable,
    Available(BTreeMap<FilesystemKey, u64>),
}

#[derive(Clone, Copy)]
pub(super) struct DrainEvidence {
    pub(super) process_reaped: bool,
    pub(super) output_drained: bool,
    pub(super) monitor_joined: bool,
}

/// Keeps removal admission, actual observations and retained evidence together.
/// Lifecycle success is never inferred from a cleanup attempt or a joined task.
pub(super) struct ExecutionFinalization {
    lifecycle: ShellDiskLifecycle,
    measurement: FinalMeasurement,
    cleanup: ExecutionCleanup,
    end_available: EndAvailable,
}

impl ExecutionFinalization {
    pub(super) fn new() -> Result<Self, DiskLifecycleError> {
        Ok(Self {
            lifecycle: ShellDiskLifecycle::new([DiskRootId::Execution, DiskRootId::Delivery])?,
            measurement: FinalMeasurement::Quiescent(None),
            cleanup: ExecutionCleanup::NotAttempted(None),
            end_available: EndAvailable::NotObserved,
        })
    }

    pub(super) fn apply(&mut self, event: DiskLifecycleEvent) -> bool {
        self.lifecycle.apply(event)
    }

    pub(super) fn snapshot(&self) -> hoimin_core::DiskLifecycleSnapshot {
        self.lifecycle.snapshot()
    }

    pub(super) fn safety_proven(&self) -> bool {
        lifecycle_safety_succeeded(&self.lifecycle) && self.measurement_quiescent()
    }

    fn measurement_quiescent(&self) -> bool {
        matches!(self.measurement, FinalMeasurement::Quiescent(_))
    }

    pub(super) fn observe_measurement(
        &mut self,
        result: Result<io::Result<u64>, CleanupThreadFailure>,
    ) -> io::Result<()> {
        match result {
            Ok(Ok(bytes)) => {
                if let FinalMeasurement::Quiescent(reading) = &mut self.measurement {
                    *reading = Some(bytes);
                }
                Ok(())
            }
            Ok(Err(error)) => Err(error),
            Err(error) => {
                self.measurement = FinalMeasurement::Unjoined;
                Err(io::Error::new(io::ErrorKind::TimedOut, error.to_string()))
            }
        }
    }

    fn drain_events(evidence: DrainEvidence) -> [(DiskLifecycleEvent, &'static str); 3] {
        [
            (
                if evidence.process_reaped {
                    DiskLifecycleEvent::ProcessDrainSucceeded
                } else {
                    DiskLifecycleEvent::ProcessDrainFailed
                },
                "process drain",
            ),
            (
                if evidence.output_drained {
                    DiskLifecycleEvent::OutputDrainSucceeded
                } else {
                    DiskLifecycleEvent::OutputDrainFailed
                },
                "output drain",
            ),
            (
                if evidence.monitor_joined {
                    DiskLifecycleEvent::MonitorJoinSucceeded
                } else {
                    DiskLifecycleEvent::MonitorJoinFailed
                },
                "monitor join",
            ),
        ]
    }

    pub(super) fn observe_inner_drains(&mut self, evidence: DrainEvidence) -> Result<bool, String> {
        for (event, component) in Self::drain_events(evidence) {
            if !self.apply(event) {
                return Err(format!("disk lifecycle rejected {component} completion"));
            }
        }
        Ok(self.safety_proven())
    }

    /// Settle only pending lifecycle components; prior failure remains authoritative.
    /// Fresh observations still gate removal even when a component settled earlier.
    pub(super) fn observe_outer_drains(
        &mut self,
        evidence: DrainEvidence,
        errors: &mut Vec<String>,
    ) -> bool {
        let snapshot = self.snapshot();
        let states = [
            snapshot.process_drain,
            snapshot.output_drain,
            snapshot.monitor_join,
        ];
        for ((event, component), state) in Self::drain_events(evidence).into_iter().zip(states) {
            if state == DiskComponentState::Pending && !self.apply(event) {
                errors.push(format!(
                    "disk lifecycle rejected outer {component} completion"
                ));
            }
        }
        cleanup_quiescence_proven([
            evidence.process_reaped,
            evidence.output_drained,
            evidence.monitor_joined,
            self.measurement_quiescent(),
        ]) && self.safety_proven()
    }

    pub(super) fn observe_end_available(
        &mut self,
        query: impl FnOnce() -> io::Result<BTreeMap<FilesystemKey, u64>>,
    ) -> io::Result<()> {
        if !matches!(self.end_available, EndAvailable::NotObserved) {
            return Ok(());
        }
        self.end_available = EndAvailable::Unavailable;
        self.end_available = EndAvailable::Available(query()?);
        Ok(())
    }

    pub(super) fn end_available_bytes(&self, key: FilesystemKey) -> Option<u64> {
        match &self.end_available {
            EndAvailable::Available(values) => values.get(&key).copied(),
            EndAvailable::NotObserved | EndAvailable::Unavailable => None,
        }
    }

    pub(super) fn cleanup_attempted(&self) -> bool {
        !matches!(self.cleanup, ExecutionCleanup::NotAttempted(_))
    }

    pub(super) fn cleaned(&self) -> bool {
        matches!(&self.cleanup, ExecutionCleanup::Completed(record) if matches!(record.disposition, CleanupDisposition::Clean))
    }

    fn begin_cleanup_attempt(&mut self) -> bool {
        if self.cleanup_attempted() {
            return false;
        }
        // Move the evidence with the attempt before admission can fail.
        let cleanup = std::mem::replace(&mut self.cleanup, ExecutionCleanup::Attempted(None));
        self.cleanup = match cleanup {
            ExecutionCleanup::NotAttempted(record) => ExecutionCleanup::Attempted(record),
            other => other,
        };
        true
    }

    /// Admit actual removal only after every owned source of I/O is quiescent.
    /// A refused attempt remains terminal even if later observations succeed.
    pub(super) fn request_cleanup(&mut self) -> bool {
        self.begin_cleanup_attempt()
            && self.safety_proven()
            && self.apply(DiskLifecycleEvent::CleanupRequested {
                root: DiskRootId::Execution,
            })
    }

    /// Record deferred cleanup without granting permission to touch the root.
    pub(super) fn defer_cleanup(&mut self, root: Utf8PathBuf) -> bool {
        self.begin_cleanup_attempt()
            && self.apply(DiskLifecycleEvent::CleanupRequested {
                root: DiskRootId::Execution,
            })
            && self.complete_cleanup(Self::deferred_record(root), CLEANUP_QUIESCENCE_UNPROVEN)
    }

    pub(super) fn complete_cleanup(&mut self, record: CleanupRecord, error: &str) -> bool {
        let outcome = match record.disposition {
            CleanupDisposition::Clean => DiskCleanupOutcome::Clean,
            CleanupDisposition::Failed => DiskCleanupOutcome::Failed(error.to_owned()),
            CleanupDisposition::Deferred(_) => DiskCleanupOutcome::Deferred(error.to_owned()),
        };
        let allowed =
            matches!(record.disposition, CleanupDisposition::Deferred(_)) || self.safety_proven();
        let accepted = allowed
            && self.apply(DiskLifecycleEvent::CleanupCompleted {
                root: DiskRootId::Execution,
                outcome,
            });
        self.cleanup = if accepted {
            ExecutionCleanup::Completed(record)
        } else {
            ExecutionCleanup::Attempted(Some(record))
        };
        accepted
    }

    pub(super) fn defer_without_attempt(&mut self, root: Utf8PathBuf) {
        debug_assert!(!self.cleanup_attempted());
        self.cleanup = ExecutionCleanup::NotAttempted(Some(Self::deferred_record(root)));
    }

    fn deferred_record(root: Utf8PathBuf) -> CleanupRecord {
        CleanupRecord {
            disposition: CleanupDisposition::Deferred(None),
            evidence: CleanupEvidence {
                examined_entries: 0,
                removed_entries: 0,
                details: vec![CLEANUP_QUIESCENCE_UNPROVEN.to_owned()],
                omitted_detail_count: 0,
                remaining_root: Some(root),
            },
        }
    }

    pub(super) fn project_summary(&self, summary: &mut RunSummary) {
        summary.disk.removed_logical_bytes = if self.cleaned() {
            match self.measurement {
                FinalMeasurement::Quiescent(reading) => reading,
                FinalMeasurement::Unjoined => None,
            }
        } else {
            None
        };
        let record = match &self.cleanup {
            ExecutionCleanup::NotAttempted(record) | ExecutionCleanup::Attempted(record) => {
                record.as_ref()
            }
            ExecutionCleanup::Completed(record) => Some(record),
        };
        if let Some(report) = summary
            .disk
            .cleanup
            .iter_mut()
            .find(|report| report.root_id == "execution")
        {
            apply_execution_cleanup_evidence(report, record);
        }
        if let Some(failure) = self.snapshot().stop {
            merge_lifecycle_stop(summary, failure);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const JOINED: DrainEvidence = DrainEvidence {
        process_reaped: true,
        output_drained: true,
        monitor_joined: true,
    };

    fn record(disposition: CleanupDisposition) -> CleanupRecord {
        CleanupRecord {
            disposition,
            evidence: CleanupEvidence {
                examined_entries: 7,
                removed_entries: 5,
                details: vec!["observed cleanup".to_owned()],
                omitted_detail_count: 0,
                remaining_root: None,
            },
        }
    }

    fn summary() -> RunSummary {
        let mut disk = hoimin_core::DiskRunSummary::unmeasured(8, 10);
        disk.cleanup.push(hoimin_core::DiskCleanupReport {
            root_id: "execution".to_owned(),
            owner: "hoimin".to_owned(),
            status: hoimin_core::DiskCleanupStatus::Retained,
            examined_entries: 0,
            removed_entries: 0,
            details: Vec::new(),
            omitted_detail_count: 0,
            remaining_root: None,
        });
        RunSummary {
            resolved_mutant_timeout: None,
            schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
            sequence: 1,
            run_id: "test".to_owned(),
            counts: hoimin_core::MutationSummary::default(),
            complete: true,
            exit_code: 0,
            disk,
            verification_selection: None,
        }
    }

    #[test]
    fn inner_and_outer_observations_require_each_drain() {
        for flags in 0..8 {
            let evidence = DrainEvidence {
                process_reaped: flags & 1 != 0,
                output_drained: flags & 2 != 0,
                monitor_joined: flags & 4 != 0,
            };
            let mut inner = ExecutionFinalization::new().unwrap();
            let mut outer = ExecutionFinalization::new().unwrap();
            let mut errors = Vec::new();
            assert_eq!(inner.observe_inner_drains(evidence).unwrap(), flags == 7);
            assert_eq!(
                outer.observe_outer_drains(evidence, &mut errors),
                flags == 7
            );
            assert_eq!(errors, Vec::<String>::new());
            assert_eq!(inner.snapshot(), outer.snapshot());
            // A later successful retry cannot erase an accepted failure.
            assert_eq!(inner.observe_outer_drains(JOINED, &mut errors), flags == 7);
            assert_eq!(errors, Vec::<String>::new());
        }
    }

    #[test]
    fn an_unjoined_scan_cannot_be_replaced_by_later_success() {
        let mut owner = ExecutionFinalization::new().unwrap();
        assert!(owner.observe_inner_drains(JOINED).unwrap());
        owner.observe_measurement(Ok(Ok(100))).unwrap();
        assert!(
            owner
                .observe_measurement(Err(CleanupThreadFailure::TimedOut))
                .is_err()
        );
        owner.observe_measurement(Ok(Ok(200))).unwrap();
        assert!(!owner.safety_proven());
        assert!(!owner.observe_outer_drains(JOINED, &mut Vec::new()));
        assert!(!owner.request_cleanup());
        assert!(!owner.complete_cleanup(record(CleanupDisposition::Clean), ""));
        assert!(!owner.cleaned());
        let mut summary = summary();
        owner.project_summary(&mut summary);
        assert_eq!(summary.disk.removed_logical_bytes, None);
    }

    #[test]
    fn deferred_admission_never_grants_unjoined_measurement_removal() {
        let mut owner = ExecutionFinalization::new().unwrap();
        assert!(
            owner
                .observe_measurement(Err(CleanupThreadFailure::TimedOut))
                .is_err()
        );
        assert!(!owner.observe_inner_drains(JOINED).unwrap());
        assert!(owner.defer_cleanup("retained/root".into()));
        assert!(!owner.safety_proven());
        assert!(!owner.request_cleanup());
        assert!(!owner.cleaned());
        let mut summary = summary();
        owner.project_summary(&mut summary);
        assert_eq!(
            summary.disk.cleanup[0].status,
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert_eq!(summary.disk.removed_logical_bytes, None);
    }

    #[test]
    fn a_joined_failed_measurement_is_quiescent_but_has_no_bytes() {
        let mut owner = ExecutionFinalization::new().unwrap();
        assert!(
            owner
                .observe_measurement(Ok(Err(io::Error::other("measurement failed"))))
                .is_err()
        );
        assert!(owner.observe_inner_drains(JOINED).unwrap());
        assert!(owner.request_cleanup());
        assert!(owner.complete_cleanup(record(CleanupDisposition::Clean), ""));
        let mut summary = summary();
        owner.project_summary(&mut summary);
        assert_eq!(summary.disk.removed_logical_bytes, None);
        assert!(owner.cleaned());
    }

    #[test]
    fn rejected_attempt_is_retained_after_outer_observations() {
        let mut owner = ExecutionFinalization::new().unwrap();
        owner.defer_without_attempt("retained/root".into());
        assert!(!owner.cleanup_attempted());
        assert!(!owner.request_cleanup());
        assert!(owner.cleanup_attempted());
        assert!(owner.observe_outer_drains(JOINED, &mut Vec::new()));
        assert!(!owner.request_cleanup());
        assert!(!owner.cleaned());
        let mut summary = summary();
        owner.project_summary(&mut summary);
        assert_eq!(
            summary.disk.cleanup[0].status,
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert_eq!(
            summary.disk.cleanup[0].remaining_root.as_deref(),
            Some("retained/root")
        );
    }

    #[test]
    fn normal_and_outer_cleanup_project_the_same_observed_record() {
        let mut summaries = Vec::new();
        for outer in [false, true] {
            let mut owner = ExecutionFinalization::new().unwrap();
            owner.observe_measurement(Ok(Ok(123))).unwrap();
            if outer {
                assert!(owner.observe_outer_drains(JOINED, &mut Vec::new()));
            } else {
                assert!(owner.observe_inner_drains(JOINED).unwrap());
            }
            assert!(owner.request_cleanup());
            assert!(owner.complete_cleanup(record(CleanupDisposition::Clean), ""));
            assert!(!owner.request_cleanup());
            let mut summary = summary();
            owner.project_summary(&mut summary);
            assert_eq!(summary.disk.removed_logical_bytes, Some(123));
            assert_eq!(summary.disk.cleanup[0].examined_entries, 7);
            summaries.push(summary);
        }
        assert_eq!(summaries[0], summaries[1]);
    }

    #[test]
    fn failed_free_space_observation_is_not_retried_or_reported_as_zero() {
        let mut owner = ExecutionFinalization::new().unwrap();
        let calls = Cell::new(0);
        let first = owner.observe_end_available(|| {
            calls.set(calls.get() + 1);
            Err(io::Error::other("free space unavailable"))
        });
        assert!(first.is_err());
        owner
            .observe_end_available(|| {
                calls.set(calls.get() + 1);
                Ok(BTreeMap::from([(FilesystemKey(1), 0)]))
            })
            .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(owner.end_available_bytes(FilesystemKey(1)), None);
    }
}
