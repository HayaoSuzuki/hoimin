use std::num::NonZeroU64;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DISK_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

pub const WORKSPACE_SIZE_EXCEEDED: &str = "workspace.size.exceeded";
pub const FILESYSTEM_RESERVE_REACHED: &str = "filesystem.reserve.reached";
pub const DISK_MEASUREMENT_FAILED: &str = "disk.measurement.failed";
pub const PROCESS_LIFECYCLE_FAILED: &str = "process.failed";
pub const WORKSPACE_CLEANUP_FAILED: &str = "workspace.cleanup.failed";
pub const WORKSPACE_CLEANUP_DEFERRED: &str = "workspace.cleanup.deferred";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskPolicy {
    pub max_owned_bytes: NonZeroU64,
    pub min_free_bytes: NonZeroU64,
}

impl DiskPolicy {
    #[must_use]
    pub fn evaluate(self, observation: DiskObservation) -> DiskDecision {
        evaluate_disk_policy(self, observation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskObservation {
    pub owned_bytes: u64,
    pub available_bytes: u64,
    pub measured_in: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum DiskDecision {
    Continue,
    Stop(DiskFailure),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskFailure {
    pub code: String,
    pub reason: DiskStopReason,
    pub observation: Option<DiskObservation>,
    pub message: Option<String>,
    pub secondary: Vec<DiskSecondary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskStopReason {
    WorkspaceSizeExceeded,
    FilesystemReserveReached,
    MeasurementFailed,
    ProcessFailed,
}

impl DiskStopReason {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::WorkspaceSizeExceeded => WORKSPACE_SIZE_EXCEEDED,
            Self::FilesystemReserveReached => FILESYSTEM_RESERVE_REACHED,
            Self::MeasurementFailed => DISK_MEASUREMENT_FAILED,
            Self::ProcessFailed => PROCESS_LIFECYCLE_FAILED,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DiskSecondary {
    Observation {
        reason: DiskStopReason,
        value: DiskObservation,
    },
    Error {
        code: String,
        message: String,
    },
}

impl DiskSecondary {
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Observation { reason, .. } => reason.code(),
            Self::Error { code, .. } => code,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskRootId {
    Execution,
    Delivery,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DiskCleanupOutcome {
    Clean,
    Failed(String),
    Deferred(String),
    Retained,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DiskLifecycleError {
    #[error("duplicate disk root {0:?}")]
    DuplicateRoot(DiskRootId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiskLifecycleEvent {
    DispatchRequested,
    Observation {
        policy: DiskPolicy,
        value: DiskObservation,
    },
    MeasurementFailed {
        message: String,
    },
    ProcessDrainSucceeded,
    ProcessDrainFailed,
    OutputDrainSucceeded,
    OutputDrainFailed,
    MonitorJoinSucceeded,
    MonitorJoinFailed,
    ReportSucceeded,
    ReportFailed,
    CleanupRequested {
        root: DiskRootId,
    },
    CleanupCompleted {
        root: DiskRootId,
        outcome: DiskCleanupOutcome,
    },
    FinishRequested,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskComponentState {
    Pending,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiskLifecycleSnapshot {
    pub stop: Option<DiskFailure>,
    pub secondary: Vec<DiskSecondary>,
    pub active: u64,
    pub dispatched: u64,
    pub owned_roots: Vec<DiskRootId>,
    pub delivery_roots: Vec<DiskRootId>,
    pub cleanup_requested: Vec<DiskRootId>,
    pub cleanup_clean: Vec<DiskRootId>,
    pub cleanup_failed: Vec<DiskRootId>,
    pub cleanup_deferred: Vec<DiskRootId>,
    pub cleanup_retained: Vec<DiskRootId>,
    pub process_drain: DiskComponentState,
    pub output_drain: DiskComponentState,
    pub monitor_join: DiskComponentState,
    pub report: DiskComponentState,
    pub finished: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiskLifecycle {
    stop: Option<DiskFailure>,
    secondary: Vec<DiskSecondary>,
    active: u64,
    dispatched: u64,
    owned_roots: Vec<DiskRootId>,
    delivery_roots: Vec<DiskRootId>,
    cleanup_requested: Vec<DiskRootId>,
    cleanup_clean: Vec<DiskRootId>,
    cleanup_failed: Vec<DiskRootId>,
    cleanup_deferred: Vec<DiskRootId>,
    cleanup_retained: Vec<DiskRootId>,
    process_drain: DiskComponentState,
    output_drain: DiskComponentState,
    monitor_join: DiskComponentState,
    report: DiskComponentState,
    finished: bool,
}

impl DiskLifecycle {
    /// Creates a lifecycle with the complete finite set of owned roots.
    ///
    /// # Errors
    ///
    /// Returns [`DiskLifecycleError::DuplicateRoot`] when a logical root is listed twice.
    pub fn new(
        owned_roots: impl IntoIterator<Item = DiskRootId>,
    ) -> Result<Self, DiskLifecycleError> {
        let mut roots = Vec::new();
        for root in owned_roots {
            if roots.contains(&root) {
                return Err(DiskLifecycleError::DuplicateRoot(root));
            }
            roots.push(root);
        }
        let delivery_roots = roots
            .iter()
            .copied()
            .filter(|root| *root == DiskRootId::Delivery)
            .collect();
        Ok(Self {
            stop: None,
            secondary: Vec::new(),
            active: 0,
            dispatched: 0,
            owned_roots: roots,
            delivery_roots,
            cleanup_requested: Vec::new(),
            cleanup_clean: Vec::new(),
            cleanup_failed: Vec::new(),
            cleanup_deferred: Vec::new(),
            cleanup_retained: Vec::new(),
            process_drain: DiskComponentState::Pending,
            output_drain: DiskComponentState::Pending,
            monitor_join: DiskComponentState::Pending,
            report: DiskComponentState::Pending,
            finished: false,
        })
    }

    pub fn apply(&mut self, event: DiskLifecycleEvent) -> bool {
        apply_disk_lifecycle_event(self, event)
    }

    #[must_use]
    pub fn may_finish(&self) -> bool {
        self.active == 0
            && settled(self.process_drain)
            && settled(self.output_drain)
            && settled(self.monitor_join)
            && self.cleanup_deferred.is_empty()
            && self.report == DiskComponentState::Succeeded
            && self.owned_roots.iter().all(|root| {
                self.cleanup_clean.contains(root)
                    || self.cleanup_failed.contains(root)
                    || self.cleanup_retained.contains(root)
            })
            && self
                .delivery_roots
                .iter()
                .all(|root| self.cleanup_clean.contains(root))
    }

    #[must_use]
    pub fn snapshot(&self) -> DiskLifecycleSnapshot {
        let mut stop = self.stop.clone();
        if let Some(failure) = &mut stop {
            failure.secondary.clone_from(&self.secondary);
        }
        DiskLifecycleSnapshot {
            stop,
            secondary: self.secondary.clone(),
            active: self.active,
            dispatched: self.dispatched,
            owned_roots: self.owned_roots.clone(),
            delivery_roots: self.delivery_roots.clone(),
            cleanup_requested: self.cleanup_requested.clone(),
            cleanup_clean: self.cleanup_clean.clone(),
            cleanup_failed: self.cleanup_failed.clone(),
            cleanup_deferred: self.cleanup_deferred.clone(),
            cleanup_retained: self.cleanup_retained.clone(),
            process_drain: self.process_drain,
            output_drain: self.output_drain,
            monitor_join: self.monitor_join,
            report: self.report,
            finished: self.finished,
        }
    }
}

#[must_use]
pub fn evaluate_disk_policy(policy: DiskPolicy, observation: DiskObservation) -> DiskDecision {
    let reserve = observation.available_bytes <= policy.min_free_bytes.get();
    let size = observation.owned_bytes >= policy.max_owned_bytes.get();
    let Some(reason) = (if reserve {
        Some(DiskStopReason::FilesystemReserveReached)
    } else if size {
        Some(DiskStopReason::WorkspaceSizeExceeded)
    } else {
        None
    }) else {
        return DiskDecision::Continue;
    };
    let secondary = if reserve && size {
        vec![DiskSecondary::Observation {
            reason: DiskStopReason::WorkspaceSizeExceeded,
            value: observation,
        }]
    } else {
        Vec::new()
    };
    DiskDecision::Stop(DiskFailure {
        code: reason.code().to_owned(),
        reason,
        observation: Some(observation),
        message: None,
        secondary,
    })
}

pub fn apply_disk_lifecycle_event(
    lifecycle: &mut DiskLifecycle,
    event: DiskLifecycleEvent,
) -> bool {
    if lifecycle.finished {
        return false;
    }
    match event {
        DiskLifecycleEvent::DispatchRequested => {
            if lifecycle.stop.is_some() || lifecycle.process_drain != DiskComponentState::Pending {
                return false;
            }
            let Some(active) = lifecycle.active.checked_add(1) else {
                return false;
            };
            let Some(dispatched) = lifecycle.dispatched.checked_add(1) else {
                return false;
            };
            lifecycle.active = active;
            lifecycle.dispatched = dispatched;
        }
        DiskLifecycleEvent::Observation { policy, value } => {
            if let DiskDecision::Stop(failure) = evaluate_disk_policy(policy, value) {
                record_failure(lifecycle, failure);
            }
        }
        DiskLifecycleEvent::MeasurementFailed { message } => {
            record_failure(
                lifecycle,
                DiskFailure {
                    code: DISK_MEASUREMENT_FAILED.to_owned(),
                    reason: DiskStopReason::MeasurementFailed,
                    observation: None,
                    message: Some(message),
                    secondary: Vec::new(),
                },
            );
        }
        DiskLifecycleEvent::ProcessDrainSucceeded => {
            return complete_process_drain(lifecycle, DiskComponentState::Succeeded);
        }
        DiskLifecycleEvent::ProcessDrainFailed => {
            if !complete_process_drain(lifecycle, DiskComponentState::Failed) {
                return false;
            }
            record_failure(
                lifecycle,
                DiskFailure {
                    code: PROCESS_LIFECYCLE_FAILED.to_owned(),
                    reason: DiskStopReason::ProcessFailed,
                    observation: None,
                    message: Some("process drain failed".to_owned()),
                    secondary: Vec::new(),
                },
            );
            return true;
        }
        DiskLifecycleEvent::OutputDrainSucceeded => {
            return complete_component(&mut lifecycle.output_drain, DiskComponentState::Succeeded);
        }
        DiskLifecycleEvent::OutputDrainFailed => {
            return complete_component(&mut lifecycle.output_drain, DiskComponentState::Failed);
        }
        DiskLifecycleEvent::MonitorJoinSucceeded => {
            return complete_component(&mut lifecycle.monitor_join, DiskComponentState::Succeeded);
        }
        DiskLifecycleEvent::MonitorJoinFailed => {
            return complete_component(&mut lifecycle.monitor_join, DiskComponentState::Failed);
        }
        DiskLifecycleEvent::ReportSucceeded => {
            return complete_component(&mut lifecycle.report, DiskComponentState::Succeeded);
        }
        DiskLifecycleEvent::ReportFailed => {
            return complete_component(&mut lifecycle.report, DiskComponentState::Failed);
        }
        DiskLifecycleEvent::CleanupRequested { root } => {
            return request_cleanup(lifecycle, root);
        }
        DiskLifecycleEvent::CleanupCompleted { root, outcome } => {
            return complete_cleanup(lifecycle, root, outcome);
        }
        DiskLifecycleEvent::FinishRequested => {
            if !lifecycle.may_finish() {
                return false;
            }
            lifecycle.finished = true;
        }
    }
    true
}

fn complete_process_drain(lifecycle: &mut DiskLifecycle, state: DiskComponentState) -> bool {
    if !complete_component(&mut lifecycle.process_drain, state) {
        return false;
    }
    lifecycle.active = 0;
    true
}

fn complete_component(component: &mut DiskComponentState, state: DiskComponentState) -> bool {
    if *component != DiskComponentState::Pending {
        return false;
    }
    *component = state;
    true
}

fn request_cleanup(lifecycle: &mut DiskLifecycle, root: DiskRootId) -> bool {
    if !lifecycle.owned_roots.contains(&root)
        || lifecycle.cleanup_requested.contains(&root)
        || lifecycle.active != 0
        || !all_safety_settled(lifecycle)
        || (lifecycle.delivery_roots.contains(&root)
            && lifecycle.report == DiskComponentState::Pending)
    {
        return false;
    }
    lifecycle.cleanup_requested.push(root);
    true
}

fn complete_cleanup(
    lifecycle: &mut DiskLifecycle,
    root: DiskRootId,
    outcome: DiskCleanupOutcome,
) -> bool {
    if !lifecycle.cleanup_requested.contains(&root) || has_outcome(lifecycle, root) {
        return false;
    }
    match outcome {
        DiskCleanupOutcome::Clean => {
            if !all_safety_succeeded(lifecycle) {
                return false;
            }
            lifecycle.cleanup_clean.push(root);
        }
        DiskCleanupOutcome::Failed(message) => {
            if !all_safety_succeeded(lifecycle) {
                return false;
            }
            lifecycle.cleanup_failed.push(root);
            add_error(lifecycle, WORKSPACE_CLEANUP_FAILED, message);
        }
        DiskCleanupOutcome::Deferred(message) => {
            lifecycle.cleanup_deferred.push(root);
            add_error(lifecycle, WORKSPACE_CLEANUP_DEFERRED, message);
        }
        DiskCleanupOutcome::Retained => lifecycle.cleanup_retained.push(root),
    }
    true
}

fn record_failure(lifecycle: &mut DiskLifecycle, mut failure: DiskFailure) {
    if lifecycle.stop.is_none() {
        lifecycle.secondary.append(&mut failure.secondary);
        failure.secondary.clear();
        lifecycle.stop = Some(failure);
        return;
    }
    let mut values = Vec::with_capacity(1 + failure.secondary.len());
    if let Some(observation) = failure.observation {
        values.push(DiskSecondary::Observation {
            reason: failure.reason,
            value: observation,
        });
    } else if let Some(message) = failure.message {
        values.push(DiskSecondary::Error {
            code: failure.code,
            message,
        });
    }
    values.append(&mut failure.secondary);
    for value in values {
        if lifecycle
            .stop
            .as_ref()
            .is_some_and(|primary| failure_matches_secondary(primary, &value))
            || lifecycle.secondary.contains(&value)
        {
            continue;
        }
        lifecycle.secondary.push(value);
    }
}

fn add_error(lifecycle: &mut DiskLifecycle, code: &'static str, message: String) {
    let secondary = DiskSecondary::Error {
        code: code.into(),
        message,
    };
    if lifecycle
        .stop
        .as_ref()
        .is_some_and(|primary| failure_matches_secondary(primary, &secondary))
        || lifecycle.secondary.contains(&secondary)
    {
        return;
    }
    lifecycle.secondary.push(secondary);
}

fn failure_matches_secondary(failure: &DiskFailure, secondary: &DiskSecondary) -> bool {
    match secondary {
        DiskSecondary::Observation { reason, value } => {
            failure.reason == *reason && failure.observation.as_ref() == Some(value)
        }
        DiskSecondary::Error { code, message } => {
            failure.code == *code && failure.message.as_ref() == Some(message)
        }
    }
}

fn settled(state: DiskComponentState) -> bool {
    state != DiskComponentState::Pending
}

fn all_safety_settled(lifecycle: &DiskLifecycle) -> bool {
    settled(lifecycle.process_drain)
        && settled(lifecycle.output_drain)
        && settled(lifecycle.monitor_join)
}

fn all_safety_succeeded(lifecycle: &DiskLifecycle) -> bool {
    lifecycle.process_drain == DiskComponentState::Succeeded
        && lifecycle.output_drain == DiskComponentState::Succeeded
        && lifecycle.monitor_join == DiskComponentState::Succeeded
}

fn has_outcome(lifecycle: &DiskLifecycle, root: DiskRootId) -> bool {
    lifecycle.cleanup_clean.contains(&root)
        || lifecycle.cleanup_failed.contains(&root)
        || lifecycle.cleanup_deferred.contains(&root)
        || lifecycle.cleanup_retained.contains(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_dispatch_transition_is_accepted_and_counted() {
        let mut lifecycle = DiskLifecycle::new([]).unwrap();

        assert!(apply_disk_lifecycle_event(
            &mut lifecycle,
            DiskLifecycleEvent::DispatchRequested,
        ));

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.active, 1);
        assert_eq!(snapshot.dispatched, 1);
    }

    #[test]
    fn direct_dispatch_rejects_either_independent_stop_gate() {
        let mut lifecycle = DiskLifecycle::new([]).unwrap();
        assert!(apply_disk_lifecycle_event(
            &mut lifecycle,
            DiskLifecycleEvent::MeasurementFailed {
                message: "stop before dispatch".into(),
            },
        ));

        assert!(!apply_disk_lifecycle_event(
            &mut lifecycle,
            DiskLifecycleEvent::DispatchRequested,
        ));

        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.active, 0);
        assert_eq!(snapshot.dispatched, 0);
    }
}
