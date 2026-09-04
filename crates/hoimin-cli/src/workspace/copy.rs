use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(test)]
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, channel};
#[cfg(test)]
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{CreateWorker, EffectId, PreflightCompleted, ReservationId};

use super::manifest::build_manifest;
use super::{
    CopyOptions, DiskSnapshot, ManagedRunRoot, OwnedWorkspaceDirectory, SnapshotFile, WorkerRoot,
    WorkerWorkspace, WorkspaceDiagnostic, WorkspaceError, WorkspaceManifest,
};

struct PendingOwnedWorkspace(Option<OwnedWorkspaceDirectory>);

impl PendingOwnedWorkspace {
    fn new(owner: OwnedWorkspaceDirectory) -> Self {
        Self(Some(owner))
    }

    fn path(&self) -> &std::path::Path {
        self.0.as_ref().expect("pending owner is present").path()
    }

    fn finish(mut self) -> OwnedWorkspaceDirectory {
        self.0.take().expect("pending owner is present")
    }
}

impl Drop for PendingOwnedWorkspace {
    fn drop(&mut self) {
        if let Some(owner) = self.0.as_ref() {
            let _ = owner.try_cleanup();
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkspacePlan {
    preflight_id: EffectId,
    original_root: Utf8PathBuf,
    options: CopyOptions,
    requested_workers: u32,
    aggregate_bytes: u64,
    manifest: WorkspaceManifest,
    snapshot: Arc<DiskSnapshot>,
    diagnostics: Vec<WorkspaceDiagnostic>,
    allowance: Arc<CopyAllowance>,
    state: Arc<Mutex<PlanState>>,
    managed_root: Option<Arc<ManagedRunRoot>>,
    #[cfg(test)]
    materialization_metrics: Arc<MaterializationIoMetrics>,
    #[cfg(test)]
    initial_grant_hook: Option<Arc<InitialGrantHook>>,
    #[cfg(test)]
    materialization_pause: Option<MaterializationPause>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MaterializationIoSnapshot {
    original_manifest_builds: u64,
    original_bytes: u64,
    original_hash_bytes: u64,
    snapshot_bytes: u64,
    snapshot_hash_bytes: u64,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct MaterializationIoMetrics {
    original_manifest_builds: AtomicU64,
    original_bytes: AtomicU64,
    original_hash_bytes: AtomicU64,
    snapshot_bytes: AtomicU64,
    snapshot_hash_bytes: AtomicU64,
}

#[cfg(test)]
impl MaterializationIoMetrics {
    fn record_original_manifest(&self, bytes: u64) {
        self.original_manifest_builds
            .fetch_add(1, Ordering::Relaxed);
        self.original_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.original_hash_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    fn record_snapshot_read(&self, bytes: usize) {
        self.snapshot_bytes
            .fetch_add(u64::try_from(bytes).unwrap(), Ordering::Relaxed);
    }

    #[cfg(feature = "contracts")]
    fn record_snapshot_hash(&self, bytes: usize) {
        self.snapshot_hash_bytes
            .fetch_add(u64::try_from(bytes).unwrap(), Ordering::Relaxed);
    }

    fn snapshot(&self) -> MaterializationIoSnapshot {
        MaterializationIoSnapshot {
            original_manifest_builds: self.original_manifest_builds.load(Ordering::Relaxed),
            original_bytes: self.original_bytes.load(Ordering::Relaxed),
            original_hash_bytes: self.original_hash_bytes.load(Ordering::Relaxed),
            snapshot_bytes: self.snapshot_bytes.load(Ordering::Relaxed),
            snapshot_hash_bytes: self.snapshot_hash_bytes.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
#[derive(Debug)]
struct InitialGrantHook {
    entered: SyncSender<()>,
    release: Mutex<Receiver<()>>,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct MaterializationPause {
    worker: u32,
    entered: Sender<()>,
    release: Arc<Mutex<Receiver<()>>>,
}

#[cfg(test)]
pub(crate) struct MaterializationPauseController {
    entered: Receiver<()>,
    release: Option<Sender<()>>,
}

#[cfg(test)]
pub(crate) struct MaterializationRelease {
    release: Option<Sender<()>>,
}

#[cfg(test)]
impl std::fmt::Debug for MaterializationPause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MaterializationPause")
            .field("worker", &self.worker)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
impl MaterializationPause {
    pub(crate) fn new(worker: u32) -> (Self, MaterializationPauseController) {
        let (entered, entered_receiver) = channel();
        let (release, release_receiver) = channel();
        (
            Self {
                worker,
                entered,
                release: Arc::new(Mutex::new(release_receiver)),
            },
            MaterializationPauseController {
                entered: entered_receiver,
                release: Some(release),
            },
        )
    }

    fn pause(&self, worker: u32) {
        if self.worker != worker || self.entered.send(()).is_err() {
            return;
        }
        let release = self
            .release
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = release.recv();
    }
}

#[cfg(test)]
impl MaterializationPauseController {
    pub(crate) fn wait_until_entered(&self, timeout: Duration) -> Result<(), RecvTimeoutError> {
        self.entered.recv_timeout(timeout)
    }

    pub(crate) fn release_guard(&mut self) -> MaterializationRelease {
        MaterializationRelease {
            release: Some(
                self.release
                    .take()
                    .expect("materialization release guard already created"),
            ),
        }
    }
}

#[cfg(test)]
impl Drop for MaterializationRelease {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[derive(Debug)]
pub(crate) enum ValidatedPreflightError<E> {
    Workspace(WorkspaceError),
    Validation(E),
}

impl<E> From<WorkspaceError> for ValidatedPreflightError<E> {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

#[derive(Debug, Default)]
pub struct PlanState {
    pub(crate) reservation: Option<(ReservationId, u64)>,
    pub(crate) workers: BTreeSet<u32>,
}

impl PlanState {
    fn validate_grant(
        &self,
        request: &CreateWorker,
        preflight_id: EffectId,
        requested_workers: u32,
        aggregate_bytes: u64,
    ) -> Result<(), WorkspaceError> {
        if request.preflight_id() != preflight_id {
            return Err(WorkspaceError::PreflightMismatch {
                expected: preflight_id,
                received: request.preflight_id(),
            });
        }
        if request.worker() >= requested_workers {
            return Err(WorkspaceError::WorkerOutOfRange {
                worker: request.worker(),
                requested_workers,
            });
        }
        if request.granted_allowance() != aggregate_bytes {
            return Err(WorkspaceError::AllowanceMismatch {
                expected: aggregate_bytes,
                received: request.granted_allowance(),
            });
        }
        match self.reservation {
            Some((reservation, _allowance)) if reservation != request.reservation_id() => {
                Err(WorkspaceError::ReservationMismatch {
                    expected: reservation,
                    received: request.reservation_id(),
                })
            }
            Some((_reservation, allowance)) if allowance != request.granted_allowance() => {
                Err(WorkspaceError::AllowanceMismatch {
                    expected: allowance,
                    received: request.granted_allowance(),
                })
            }
            _ => Ok(()),
        }
    }

    fn accept_grant(
        &mut self,
        request: &CreateWorker,
        preflight_id: EffectId,
        requested_workers: u32,
        aggregate_bytes: u64,
    ) -> Result<bool, WorkspaceError> {
        self.validate_grant(request, preflight_id, requested_workers, aggregate_bytes)?;
        let newly_bound = self.reservation.is_none();
        if newly_bound {
            self.reservation = Some((request.reservation_id(), request.granted_allowance()));
        }
        if !self.workers.insert(request.worker()) {
            return Err(WorkspaceError::WorkerAlreadyExists {
                worker: request.worker(),
            });
        }
        Ok(newly_bound)
    }
}

#[derive(Debug)]
pub struct CopyAllowance {
    granted: AtomicU64,
    charged: AtomicU64,
}

impl CopyAllowance {
    fn set_grant(&self, granted: u64) {
        self.granted.store(granted, Ordering::Release);
    }

    fn charge(&self, amount: u64) -> Result<(), WorkspaceError> {
        let granted = self.granted.load(Ordering::Acquire);
        self.charged
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(amount).filter(|next| *next <= granted)
            })
            .map(|_| ())
            .map_err(|observed| WorkspaceError::CopyAllowanceExceeded {
                observed: observed.saturating_add(amount),
                allowance: granted,
            })
    }

    pub fn release(&self, amount: u64) {
        let previous = self.charged.fetch_sub(amount, Ordering::AcqRel);
        debug_assert!(previous >= amount);
    }

    fn charged(&self) -> u64 {
        self.charged.load(Ordering::Acquire)
    }
}

impl WorkspacePlan {
    /// Builds a copy plan after validating the source workspace and requested capacity.
    ///
    /// # Errors
    ///
    /// Returns an error when the root cannot be materialized, the manifest cannot be built,
    /// the worker count is zero, or the aggregate copy allowance overflows.
    pub fn preflight(
        root: &Utf8Path,
        preflight_id: EffectId,
        requested_workers: u32,
        options: CopyOptions,
    ) -> Result<Self, WorkspaceError> {
        match Self::preflight_validated(root, preflight_id, requested_workers, options, |_, _| {
            Ok::<_, Infallible>(())
        }) {
            Ok(plan) => Ok(plan),
            Err(ValidatedPreflightError::Workspace(error)) => Err(error),
            Err(ValidatedPreflightError::Validation(never)) => match never {},
        }
    }

    pub(crate) fn preflight_validated<E>(
        root: &Utf8Path,
        preflight_id: EffectId,
        requested_workers: u32,
        options: CopyOptions,
        validate: impl FnOnce(&Utf8Path, &WorkspaceManifest) -> Result<(), E>,
    ) -> Result<Self, ValidatedPreflightError<E>> {
        Self::preflight_validated_in(
            root,
            preflight_id,
            requested_workers,
            options,
            None,
            None,
            validate,
        )
    }

    pub(crate) fn preflight_validated_in<E>(
        root: &Utf8Path,
        preflight_id: EffectId,
        requested_workers: u32,
        options: CopyOptions,
        managed_root: Option<Arc<ManagedRunRoot>>,
        max_owned_bytes: Option<u64>,
        validate: impl FnOnce(&Utf8Path, &WorkspaceManifest) -> Result<(), E>,
    ) -> Result<Self, ValidatedPreflightError<E>> {
        if requested_workers == 0 {
            return Err(WorkspaceError::ZeroWorkers.into());
        }
        let canonical = fs::canonicalize(root)
            .map_err(|error| WorkspaceError::io("canonicalize root", root, error))?;
        let original_root =
            Utf8PathBuf::from_path_buf(canonical).map_err(|_| WorkspaceError::NonUtf8Path)?;
        let (manifest, diagnostics) = build_manifest(&original_root, &options)?;
        let owned_copies = u64::from(requested_workers)
            .checked_add(1)
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        let planned_owned_bytes = manifest
            .logical_bytes()
            .checked_mul(owned_copies)
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        if let Some(limit) = max_owned_bytes
            && planned_owned_bytes >= limit
        {
            return Err(WorkspaceError::OwnedWorkspaceLimit {
                planned: planned_owned_bytes,
                limit,
            }
            .into());
        }
        validate(&original_root, &manifest).map_err(ValidatedPreflightError::Validation)?;
        let snapshot = Arc::new(create_disk_snapshot(
            &original_root,
            &manifest,
            managed_root.as_ref(),
        )?);
        let (current, _) = build_manifest(&original_root, &options)?;
        if !manifest.content_matches(&current) {
            return Err(WorkspaceError::OriginalChanged {
                path: manifest
                    .first_content_difference(&current)
                    .unwrap_or_default(),
            }
            .into());
        }
        let aggregate_bytes = manifest
            .logical_bytes()
            .checked_mul(u64::from(requested_workers))
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        Ok(Self {
            preflight_id,
            original_root,
            options,
            requested_workers,
            aggregate_bytes,
            manifest,
            snapshot,
            diagnostics,
            allowance: Arc::new(CopyAllowance {
                granted: AtomicU64::new(0),
                charged: AtomicU64::new(0),
            }),
            state: Arc::new(Mutex::new(PlanState::default())),
            managed_root,
            #[cfg(test)]
            materialization_metrics: Arc::new(MaterializationIoMetrics::default()),
            #[cfg(test)]
            initial_grant_hook: None,
            #[cfg(test)]
            materialization_pause: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_materialization_pause(
        mut self,
        pause: Option<MaterializationPause>,
    ) -> Self {
        self.materialization_pause = pause;
        self
    }

    #[must_use]
    pub const fn completed(&self) -> PreflightCompleted {
        PreflightCompleted {
            id: self.preflight_id,
            per_worker_logical_bytes: self.manifest.logical_bytes(),
            requested_workers: self.requested_workers,
            aggregate_logical_bytes: self.aggregate_bytes,
            fingerprint: None,
        }
    }

    #[must_use]
    pub const fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[WorkspaceDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn aggregate_bytes(&self) -> u64 {
        self.aggregate_bytes
    }

    #[must_use]
    pub fn observed_copy_bytes(&self) -> u64 {
        self.allowance.charged()
    }

    #[must_use]
    pub fn materialized_workers(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.workers.len())
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn materialization_io_snapshot(&self) -> MaterializationIoSnapshot {
        self.materialization_metrics.snapshot()
    }

    #[must_use]
    pub fn reservation_id(&self) -> Option<ReservationId> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.reservation.map(|(id, _)| id))
    }

    /// Confirms that the source workspace still matches this plan's manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if the source cannot be scanned or its contents changed.
    pub fn verify_originals(&self) -> Result<(), WorkspaceError> {
        let (current, _) = build_manifest(&self.original_root, &self.options)?;
        #[cfg(test)]
        self.materialization_metrics
            .record_original_manifest(current.logical_bytes());
        if self.manifest.content_matches(&current) {
            Ok(())
        } else {
            Err(WorkspaceError::OriginalChanged {
                path: self
                    .manifest
                    .first_content_difference(&current)
                    .unwrap_or_default(),
            })
        }
    }

    /// Materializes a worker after validating and recording its granted copy allowance.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid grant or if worker materialization fails.
    pub fn create_worker(&self, request: &CreateWorker) -> Result<WorkerWorkspace, WorkspaceError> {
        self.accept_grant(request)?;
        match self.materialize_worker(request.worker()) {
            Ok(worker) => Ok(worker),
            Err(error) => {
                if let Ok(mut state) = self.state.lock() {
                    state.workers.remove(&request.worker());
                }
                Err(error)
            }
        }
    }

    pub(crate) fn validate_grant(&self, request: &CreateWorker) -> Result<(), WorkspaceError> {
        let state = self
            .state
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        state.validate_grant(
            request,
            self.preflight_id,
            self.requested_workers,
            self.aggregate_bytes,
        )
    }

    fn accept_grant(&self, request: &CreateWorker) -> Result<(), WorkspaceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkspaceError::StatePoisoned)?;
        let newly_bound = state.accept_grant(
            request,
            self.preflight_id,
            self.requested_workers,
            self.aggregate_bytes,
        )?;
        #[cfg(test)]
        if newly_bound && let Some(hook) = &self.initial_grant_hook {
            hook.entered.send(()).expect("grant hook receiver");
            hook.release
                .lock()
                .expect("grant hook lock")
                .recv()
                .expect("grant hook release");
        }
        if newly_bound {
            self.allowance.set_grant(request.granted_allowance());
        }
        drop(state);
        Ok(())
    }

    fn materialize_worker(&self, worker: u32) -> Result<WorkerWorkspace, WorkspaceError> {
        self.materialize_worker_with_root_opener(worker, WorkerRoot::open)
    }

    fn materialize_worker_with_root_opener(
        &self,
        worker: u32,
        open_root: impl FnOnce(Utf8PathBuf) -> Result<WorkerRoot, WorkspaceError>,
    ) -> Result<WorkerWorkspace, WorkspaceError> {
        let temp = if let Some(managed) = &self.managed_root {
            OwnedWorkspaceDirectory::Managed(managed.create_child("worker-")?)
        } else {
            OwnedWorkspaceDirectory::Temporary(
                tempfile::Builder::new()
                    .prefix("hoimin-worker-")
                    .tempdir()
                    .map_err(|error| {
                        WorkspaceError::io("create worker", &self.original_root, error)
                    })?,
            )
        };
        let temp = PendingOwnedWorkspace::new(temp);
        let root_path = temp.path().join("workspace");
        fs::create_dir(&root_path).map_err(|error| {
            WorkspaceError::io("create worker root", &self.original_root, error)
        })?;
        let root_path =
            Utf8PathBuf::from_path_buf(root_path).map_err(|_| WorkspaceError::NonUtf8Path)?;
        let root = open_root(root_path.clone())?;
        #[cfg(test)]
        if let Some(pause) = &self.materialization_pause {
            pause.pause(worker);
        }
        let mut charged = 0_u64;

        let result = (|| {
            for entry in self.manifest.entries() {
                let snapshot = self.snapshot.files.get(&entry.path).ok_or_else(|| {
                    WorkspaceError::WorkspaceRestore {
                        path: entry.path.clone(),
                        message: "shared snapshot is missing a manifest entry".to_owned(),
                    }
                })?;
                let bytes = self.snapshot.read(&entry.path)?;
                #[cfg(test)]
                self.materialization_metrics
                    .record_snapshot_read(bytes.len());
                let amount =
                    u64::try_from(bytes.len()).map_err(|_| WorkspaceError::CopySizeOverflow)?;
                self.allowance.charge(amount)?;
                charged = charged
                    .checked_add(amount)
                    .ok_or(WorkspaceError::CopySizeOverflow)?;

                #[cfg(feature = "contracts")]
                {
                    #[cfg(test)]
                    self.materialization_metrics
                        .record_snapshot_hash(bytes.len());
                    if amount != entry.size || blake3::hash(&bytes) != entry.blake3 {
                        return Err(WorkspaceError::WorkspaceRestore {
                            path: entry.path.clone(),
                            message: "shared snapshot does not match its manifest".to_owned(),
                        });
                    }
                }
                let destination = root_path.join(&entry.path);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        WorkspaceError::io("create worker directory", parent, error)
                    })?;
                }
                fs::write(&destination, &bytes)
                    .map_err(|error| WorkspaceError::io("copy worker file", &entry.path, error))?;
                fs::set_permissions(&destination, snapshot.permissions.clone()).map_err(
                    |error| WorkspaceError::io("copy worker permissions", &entry.path, error),
                )?;
            }
            Ok(())
        })();

        if let Err(error) = result {
            self.allowance.release(charged);
            return Err(error);
        }

        Ok(WorkerWorkspace::from_materialized(
            temp.finish(),
            root,
            self.manifest.clone(),
            Arc::clone(&self.snapshot),
            Arc::clone(&self.allowance),
            Arc::clone(&self.state),
            worker,
            charged,
        ))
    }
}

fn create_disk_snapshot(
    original_root: &Utf8Path,
    manifest: &WorkspaceManifest,
    managed_root: Option<&Arc<ManagedRunRoot>>,
) -> Result<DiskSnapshot, WorkspaceError> {
    let temp = if let Some(managed) = managed_root {
        OwnedWorkspaceDirectory::Managed(managed.create_child("snapshot-")?)
    } else {
        OwnedWorkspaceDirectory::Temporary(
            tempfile::Builder::new()
                .prefix("hoimin-snapshot-")
                .tempdir()
                .map_err(|error| {
                    WorkspaceError::io("create shared snapshot", original_root, error)
                })?,
        )
    };
    let temp = PendingOwnedWorkspace::new(temp);
    let root_path = temp.path().join("workspace");
    fs::create_dir(&root_path)
        .map_err(|error| WorkspaceError::io("create shared snapshot root", original_root, error))?;
    let root = Utf8PathBuf::from_path_buf(root_path).map_err(|_| WorkspaceError::NonUtf8Path)?;
    let mut files = BTreeMap::new();

    for entry in manifest.entries() {
        let source = original_root.join(&entry.path);
        let bytes = fs::read(&source)
            .map_err(|error| WorkspaceError::io("read original", &entry.path, error))?;
        let amount = u64::try_from(bytes.len()).map_err(|_| WorkspaceError::CopySizeOverflow)?;
        if amount != entry.size || blake3::hash(&bytes) != entry.blake3 {
            return Err(WorkspaceError::OriginalChanged {
                path: entry.path.clone(),
            });
        }
        let permissions = fs::metadata(&source)
            .map_err(|error| WorkspaceError::io("read original metadata", &entry.path, error))?
            .permissions();
        let destination = root.join(&entry.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                WorkspaceError::io("create shared snapshot directory", parent, error)
            })?;
        }
        match fs::symlink_metadata(&destination) {
            Ok(_) => {
                return Err(WorkspaceError::SnapshotPathCollision {
                    path: entry.path.clone(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(WorkspaceError::io(
                    "inspect shared snapshot destination",
                    &entry.path,
                    error,
                ));
            }
        }
        fs::write(&destination, bytes)
            .map_err(|error| WorkspaceError::io("write shared snapshot", &entry.path, error))?;
        files.insert(entry.path.clone(), SnapshotFile::new(permissions));
    }

    Ok(DiskSnapshot {
        _owner: temp.finish(),
        root,
        files,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use hoimin_core::{BudgetLedger, RunBudgets, reserve_workspace_copy};

    #[cfg(windows)]
    use crate::workspace::ManifestEntry;

    use super::*;
    use crate::workspace::{ManagedRootCoordinator, OwnerKind};

    #[test]
    fn owned_byte_limit_stops_before_snapshot_creation() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"12345").unwrap();
        let source = Utf8Path::from_path(source.path()).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());

        let error = WorkspacePlan::preflight_validated_in(
            source,
            EffectId(7),
            1,
            CopyOptions::default(),
            Some(Arc::clone(&root)),
            Some(10),
            |_, _| Ok::<_, Infallible>(()),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ValidatedPreflightError::Workspace(WorkspaceError::OwnedWorkspaceLimit {
                planned: 10,
                limit: 10
            })
        ));
        let names = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            names.len(),
            2,
            "snapshot child must not be created: {names:?}"
        );
    }

    #[test]
    fn snapshot_and_worker_are_materialized_below_the_leased_root() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"pass\n").unwrap();
        let source = Utf8Path::from_path(source.path()).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let plan = WorkspacePlan::preflight_validated_in(
            source,
            EffectId(7),
            1,
            CopyOptions::default(),
            Some(Arc::clone(&root)),
            Some(1_024),
            |_, _| Ok::<_, Infallible>(()),
        )
        .unwrap();
        let completed = plan.completed();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();

        let mut worker = plan
            .create_worker(&grant.create_worker(EffectId(8), 0).unwrap())
            .unwrap();

        assert!(worker.root().starts_with(root.path()));
        worker.try_cleanup().unwrap();
        drop((worker, plan));
        assert_eq!(
            root.cleanup(Duration::from_secs(1)).status,
            hoimin_core::DiskCleanupStatus::Clean
        );
    }

    fn plan_with_padding(
        workers: u32,
        padding_bytes: usize,
    ) -> (
        tempfile::TempDir,
        WorkspacePlan,
        hoimin_core::WorkspaceCopyGrant,
        u64,
    ) {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"original\n").unwrap();
        fs::write(source.path().join("padding.bin"), vec![b'x'; padding_bytes]).unwrap();
        let source_root = Utf8Path::from_path(source.path()).unwrap();
        let plan =
            WorkspacePlan::preflight(source_root, EffectId(7), workers, CopyOptions::default())
                .unwrap();
        let completed = plan.completed();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        (
            source,
            plan,
            grant,
            u64::try_from(padding_bytes + b"original\n".len()).unwrap(),
        )
    }

    #[test]
    fn materialization_io_counts_full_tree_work() {
        const WORKERS: u32 = 2;
        const PADDING_BYTES: usize = 1024 * 1024;
        let (_source, plan, grant, fixture_bytes) = plan_with_padding(WORKERS, PADDING_BYTES);

        let first = plan
            .create_worker(&grant.create_worker(EffectId(8), 0).unwrap())
            .unwrap();
        let second = plan
            .create_worker(&grant.create_worker(EffectId(9), 1).unwrap())
            .unwrap();
        plan.verify_originals().unwrap();

        let expected_snapshot_hash_bytes = if cfg!(feature = "contracts") {
            2 * fixture_bytes
        } else {
            0
        };
        assert_eq!(
            plan.materialization_io_snapshot(),
            MaterializationIoSnapshot {
                original_manifest_builds: 1,
                original_bytes: fixture_bytes,
                original_hash_bytes: fixture_bytes,
                snapshot_bytes: 2 * fixture_bytes,
                snapshot_hash_bytes: expected_snapshot_hash_bytes,
            }
        );
        assert_eq!(plan.observed_copy_bytes(), 2 * fixture_bytes);
        drop((first, second));
    }

    #[test]
    #[ignore = "manual before/after performance evidence"]
    fn benchmark_worker_materialization_io() {
        const WORKERS: u32 = 8;
        const PADDING_BYTES: usize = 8 * 1024 * 1024;
        let (_source, plan, grant, fixture_bytes) = plan_with_padding(WORKERS, PADDING_BYTES);
        let started = Instant::now();

        let materialized = (0..WORKERS)
            .map(|worker| {
                plan.create_worker(
                    &grant
                        .create_worker(EffectId(u64::from(worker) + 8), worker)
                        .unwrap(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        plan.verify_originals().unwrap();

        let metrics = plan.materialization_io_snapshot();
        eprintln!(
            "workers={WORKERS} fixture_bytes={fixture_bytes} original_manifest_builds={} original_bytes={} original_hash_bytes={} snapshot_bytes={} snapshot_hash_bytes={} copied_bytes={} elapsed_ms={}",
            metrics.original_manifest_builds,
            metrics.original_bytes,
            metrics.original_hash_bytes,
            metrics.snapshot_bytes,
            metrics.snapshot_hash_bytes,
            plan.observed_copy_bytes(),
            started.elapsed().as_millis(),
        );
        drop(materialized);
    }

    #[cfg(windows)]
    #[test]
    fn disk_snapshot_rejects_paths_that_alias_on_its_filesystem() {
        let source = tempfile::tempdir().unwrap();
        let source_root = Utf8PathBuf::from_path_buf(source.path().to_owned()).unwrap();
        let bytes = b"same source bytes";
        fs::write(source.path().join("TARGET.py"), bytes).unwrap();
        let entry = |path: &str| ManifestEntry {
            path: Utf8PathBuf::from(path),
            size: u64::try_from(bytes.len()).unwrap(),
            modified: None,
            blake3: blake3::hash(bytes),
        };
        let manifest =
            WorkspaceManifest::from_entries_for_test(vec![entry("TARGET.py"), entry("target.py")]);

        let error = create_disk_snapshot(&source_root, &manifest, None).unwrap_err();

        assert_eq!(
            error,
            WorkspaceError::SnapshotPathCollision {
                path: Utf8PathBuf::from("target.py"),
            }
        );
        assert_eq!(error.code(), "workspace.path.collision");
    }

    fn create_request(
        ledger: &mut BudgetLedger,
        completed: &PreflightCompleted,
        worker: u32,
    ) -> (CreateWorker, ReservationId) {
        let grant = reserve_workspace_copy(ledger, completed).unwrap();
        let reservation = grant.reservation_id();
        (
            grant.create_worker(EffectId(99), worker).unwrap(),
            reservation,
        )
    }

    #[test]
    fn worker_root_is_opened_before_copy_allowance_is_charged() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"copied bytes").unwrap();
        let source_root = Utf8Path::from_path(source.path()).unwrap();
        let plan =
            WorkspacePlan::preflight(source_root, EffectId(7), 1, CopyOptions::default()).unwrap();
        let completed = plan.completed();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let (request, _) = create_request(&mut ledger, &completed, 0);
        plan.accept_grant(&request).unwrap();
        let opened_path = Arc::new(Mutex::new(None));
        let captured_path = Arc::clone(&opened_path);

        let error = plan
            .materialize_worker_with_root_opener(0, move |path| {
                *captured_path.lock().unwrap() = Some(path.clone());
                Err(WorkspaceError::io(
                    "injected worker root open",
                    path,
                    "failure",
                ))
            })
            .unwrap_err();

        assert!(matches!(error, WorkspaceError::Io { .. }));
        assert_eq!(plan.observed_copy_bytes(), 0);
        let opened_path = opened_path.lock().unwrap().take().unwrap();
        assert!(!opened_path.exists());
    }

    #[test]
    fn managed_snapshot_failure_removes_its_partial_child() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"before\n").unwrap();
        let source_root = Utf8Path::from_path(source.path()).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());

        let error = WorkspacePlan::preflight_validated_in(
            source_root,
            EffectId(7),
            1,
            CopyOptions::default(),
            Some(Arc::clone(&root)),
            Some(1_024),
            |source, _| fs::write(source.join("target.py"), b"after\n"),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ValidatedPreflightError::Workspace(WorkspaceError::OriginalChanged { .. })
        ));
        let names = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 2, "partial snapshot leaked: {names:?}");
    }

    #[test]
    fn managed_worker_open_failure_removes_its_partial_child() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"payload\n").unwrap();
        let source_root = Utf8Path::from_path(source.path()).unwrap();
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let plan = WorkspacePlan::preflight_validated_in(
            source_root,
            EffectId(7),
            1,
            CopyOptions::default(),
            Some(Arc::clone(&root)),
            Some(1_024),
            |_, _| Ok::<_, Infallible>(()),
        )
        .unwrap();

        let error = plan
            .materialize_worker_with_root_opener(0, |path| {
                Err(WorkspaceError::io(
                    "injected worker root open",
                    path,
                    "failure",
                ))
            })
            .unwrap_err();

        assert!(matches!(error, WorkspaceError::Io { .. }));
        let names = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 3, "partial worker leaked: {names:?}");
    }

    #[test]
    fn failed_snapshot_copy_rolls_back_only_the_slot_and_keeps_the_bound_reservation() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"original\n").unwrap();
        let source_root = Utf8Path::from_path(source.path()).unwrap();
        let plan =
            WorkspacePlan::preflight(source_root, EffectId(14), 1, CopyOptions::default()).unwrap();
        let completed = plan.completed();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: 2 * completed.aggregate_logical_bytes,
            processes: 1,
        });
        let winner = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        let foreign = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        let winner_request = winner.create_worker(EffectId(15), 0).unwrap();
        let foreign_request = foreign.create_worker(EffectId(16), 0).unwrap();
        let snapshot_path = plan.snapshot.root.join("target.py");
        fs::remove_file(&snapshot_path).unwrap();

        let failed = plan.create_worker(&winner_request).unwrap_err();

        let WorkspaceError::Io {
            operation, path, ..
        } = failed
        else {
            panic!("expected snapshot-read failure, got {failed:?}");
        };
        assert_eq!(operation, "read shared snapshot");
        assert_eq!(path, Utf8Path::new("target.py"));
        assert_eq!(plan.materialized_workers(), 0);
        assert_eq!(plan.observed_copy_bytes(), 0);
        assert_eq!(plan.reservation_id(), Some(winner.reservation_id()));

        assert_eq!(
            plan.create_worker(&foreign_request).unwrap_err(),
            WorkspaceError::ReservationMismatch {
                expected: winner.reservation_id(),
                received: foreign.reservation_id(),
            }
        );

        fs::write(snapshot_path, b"original\n").unwrap();
        let worker = plan.create_worker(&winner_request).unwrap();
        assert_eq!(plan.materialized_workers(), 1);
        assert_eq!(
            plan.observed_copy_bytes(),
            completed.per_worker_logical_bytes
        );
        assert_eq!(worker.read("target.py").unwrap(), b"original\n");
    }

    #[test]
    fn initial_grant_is_published_before_another_worker_can_materialize() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("target.py"), b"copied bytes").unwrap();
        let source_root = Utf8Path::from_path(source.path()).unwrap();
        let mut plan =
            WorkspacePlan::preflight(source_root, EffectId(7), 2, CopyOptions::default()).unwrap();
        let completed = plan.completed();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        let first = grant.create_worker(EffectId(8), 0).unwrap();
        let second = grant.create_worker(EffectId(9), 1).unwrap();
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        plan.initial_grant_hook = Some(Arc::new(InitialGrantHook {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }));
        let plan = Arc::new(plan);
        let (result_tx, result_rx) = mpsc::channel();

        let first_plan = Arc::clone(&plan);
        let first_result = result_tx.clone();
        let first_thread = thread::spawn(move || {
            first_result.send(first_plan.create_worker(&first)).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let second_plan = Arc::clone(&plan);
        let second_thread = thread::spawn(move || {
            result_tx.send(second_plan.create_worker(&second)).unwrap();
        });
        assert!(result_rx.recv_timeout(Duration::from_millis(50)).is_err());

        release_tx.send(()).unwrap();
        let workers = [
            result_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            result_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        ];
        first_thread.join().unwrap();
        second_thread.join().unwrap();

        assert!(workers.iter().all(Result::is_ok));
        assert_eq!(plan.materialized_workers(), 2);
        assert_eq!(
            plan.observed_copy_bytes(),
            completed.aggregate_logical_bytes
        );
        drop(workers);
    }

    #[test]
    fn dropping_materialization_pause_controller_releases_waiting_worker() {
        let (pause, controller) = MaterializationPause::new(0);
        let (completed_tx, completed_rx) = mpsc::sync_channel(0);
        let worker = thread::spawn(move || {
            pause.pause(0);
            completed_tx.send(()).unwrap();
        });
        controller
            .wait_until_entered(Duration::from_secs(1))
            .expect("worker did not reach the materialization pause");

        drop(controller);

        completed_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("dropping the controller must release the worker");
        worker.join().unwrap();
    }

    #[test]
    fn materialization_pause_entry_wait_is_bounded() {
        let (_pause, controller) = MaterializationPause::new(0);

        assert_eq!(
            controller.wait_until_entered(Duration::ZERO),
            Err(mpsc::RecvTimeoutError::Timeout)
        );
    }

    #[test]
    fn plan_state_atomically_binds_one_initial_reservation_and_worker_slot() {
        let completed = PreflightCompleted {
            id: EffectId(7),
            per_worker_logical_bytes: 9,
            requested_workers: 2,
            aggregate_logical_bytes: 18,
            fingerprint: None,
        };
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes * 2,
            processes: 1,
        });
        let (first, first_reservation) = create_request(&mut ledger, &completed, 0);
        let (second, second_reservation) = create_request(&mut ledger, &completed, 1);
        let state = Arc::new(Mutex::new(PlanState::default()));
        let barrier = Arc::new(Barrier::new(2));
        let attempts = [
            (first, first_reservation, 0),
            (second, second_reservation, 1),
        ]
        .map(|(request, reservation, worker)| {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let result = state.lock().unwrap().accept_grant(
                    &request,
                    completed.id,
                    2,
                    completed.aggregate_logical_bytes,
                );
                (reservation, worker, result)
            })
        })
        .map(|thread| thread.join().unwrap());
        let winner = attempts.iter().find(|attempt| attempt.2.is_ok()).unwrap();
        let loser = attempts.iter().find(|attempt| attempt.2.is_err()).unwrap();

        assert_eq!(
            attempts.iter().filter(|attempt| attempt.2.is_ok()).count(),
            1
        );
        assert_eq!(
            loser.2,
            Err(WorkspaceError::ReservationMismatch {
                expected: winner.0,
                received: loser.0,
            })
        );
        let state = state.lock().unwrap();
        assert_eq!(
            state.reservation,
            Some((winner.0, completed.aggregate_logical_bytes))
        );
        assert_eq!(state.workers, BTreeSet::from([winner.1]));
        drop(state);
    }
}
