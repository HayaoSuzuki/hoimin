use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{CreateWorker, EffectId, PreflightCompleted, ReservationId};

use super::manifest::build_manifest;
use super::{
    CopyOptions, DiskSnapshot, SnapshotFile, WorkerWorkspace, WorkspaceDiagnostic, WorkspaceError,
    WorkspaceManifest,
};

#[derive(Debug)]
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
}

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
        if requested_workers == 0 {
            return Err(WorkspaceError::ZeroWorkers.into());
        }
        let canonical = fs::canonicalize(root)
            .map_err(|error| WorkspaceError::io("canonicalize root", root, error))?;
        let original_root =
            Utf8PathBuf::from_path_buf(canonical).map_err(|_| WorkspaceError::NonUtf8Path)?;
        let (manifest, diagnostics) = build_manifest(&original_root, &options)?;
        validate(&original_root, &manifest).map_err(ValidatedPreflightError::Validation)?;
        let snapshot = Arc::new(create_disk_snapshot(&original_root, &manifest)?);
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
        })
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
        drop(state);
        if newly_bound {
            self.allowance.set_grant(request.granted_allowance());
        }
        Ok(())
    }

    fn materialize_worker(&self, worker: u32) -> Result<WorkerWorkspace, WorkspaceError> {
        self.verify_originals_for_materialization()?;
        let temp = tempfile::Builder::new()
            .prefix("hoimin-worker-")
            .tempdir()
            .map_err(|error| WorkspaceError::io("create worker", &self.original_root, error))?;
        let root_path = temp.path().join("workspace");
        fs::create_dir(&root_path).map_err(|error| {
            WorkspaceError::io("create worker root", &self.original_root, error)
        })?;
        let root =
            Utf8PathBuf::from_path_buf(root_path).map_err(|_| WorkspaceError::NonUtf8Path)?;
        let mut charged = 0_u64;

        let result = (|| {
            for entry in self.manifest.entries() {
                let bytes = self.snapshot.read(&entry.path)?;
                let amount =
                    u64::try_from(bytes.len()).map_err(|_| WorkspaceError::CopySizeOverflow)?;
                self.allowance.charge(amount)?;
                charged = charged
                    .checked_add(amount)
                    .ok_or(WorkspaceError::CopySizeOverflow)?;
                if amount != entry.size || blake3::hash(&bytes) != entry.blake3 {
                    return Err(WorkspaceError::WorkspaceRestore {
                        path: entry.path.clone(),
                        message: "shared snapshot does not match its manifest".to_owned(),
                    });
                }
                let snapshot = self.snapshot.files.get(&entry.path).ok_or_else(|| {
                    WorkspaceError::WorkspaceRestore {
                        path: entry.path.clone(),
                        message: "shared snapshot is missing a manifest entry".to_owned(),
                    }
                })?;
                let destination = root.join(&entry.path);
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
            self.verify_originals()
        })();

        if let Err(error) = result {
            self.allowance.release(charged);
            return Err(error);
        }

        WorkerWorkspace::from_materialized(
            temp,
            root,
            self.original_root.clone(),
            self.options.clone(),
            self.manifest.clone(),
            Arc::clone(&self.snapshot),
            Arc::clone(&self.allowance),
            Arc::clone(&self.state),
            worker,
            charged,
        )
    }

    fn verify_originals_for_materialization(&self) -> Result<(), WorkspaceError> {
        let mut charged = 0_u64;
        let result = (|| {
            for entry in self.manifest.entries() {
                let source = self.original_root.join(&entry.path);
                let bytes = fs::read(&source)
                    .map_err(|error| WorkspaceError::io("read original", &entry.path, error))?;
                let amount =
                    u64::try_from(bytes.len()).map_err(|_| WorkspaceError::CopySizeOverflow)?;
                self.allowance.charge(amount)?;
                charged = charged
                    .checked_add(amount)
                    .ok_or(WorkspaceError::CopySizeOverflow)?;
                if amount != entry.size || blake3::hash(&bytes) != entry.blake3 {
                    return Err(WorkspaceError::OriginalChanged {
                        path: entry.path.clone(),
                    });
                }
                fs::metadata(&source).map_err(|error| {
                    WorkspaceError::io("read original metadata", &entry.path, error)
                })?;
            }
            self.verify_originals()
        })();
        self.allowance.release(charged);
        result
    }
}

fn create_disk_snapshot(
    original_root: &Utf8Path,
    manifest: &WorkspaceManifest,
) -> Result<DiskSnapshot, WorkspaceError> {
    let temp = tempfile::Builder::new()
        .prefix("hoimin-snapshot-")
        .tempdir()
        .map_err(|error| WorkspaceError::io("create shared snapshot", original_root, error))?;
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
        _temp: temp,
        root,
        files,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::thread;

    use hoimin_core::{BudgetLedger, RunBudgets, reserve_workspace_copy};

    use super::*;

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

        let error = create_disk_snapshot(&source_root, &manifest).unwrap_err();

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
