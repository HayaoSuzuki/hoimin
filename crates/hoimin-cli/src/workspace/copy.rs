use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use camino::{Utf8Path, Utf8PathBuf};

use super::manifest::build_manifest;
use super::{
    CopyOptions, SnapshotFile, WorkerWorkspace, WorkspaceDiagnostic, WorkspaceError,
    WorkspaceLimits, WorkspaceManifest,
};

#[derive(Debug)]
pub struct WorkspacePlan {
    original_root: Utf8PathBuf,
    options: CopyOptions,
    limits: WorkspaceLimits,
    manifest: WorkspaceManifest,
    diagnostics: Vec<WorkspaceDiagnostic>,
    allowance: Arc<CopyAllowance>,
    materialized: Arc<AtomicUsize>,
}

#[derive(Debug)]
pub(crate) struct CopyAllowance {
    granted: u64,
    charged: AtomicU64,
}

impl CopyAllowance {
    fn charge(&self, amount: u64) -> Result<(), WorkspaceError> {
        self.charged
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(amount)
                    .filter(|next| *next <= self.granted)
            })
            .map(|_| ())
            .map_err(|observed| WorkspaceError::CopyAllowanceExceeded {
                observed: observed.saturating_add(amount),
                allowance: self.granted,
            })
    }

    pub(crate) fn release(&self, amount: u64) {
        let previous = self.charged.fetch_sub(amount, Ordering::AcqRel);
        debug_assert!(previous >= amount);
    }

    fn charged(&self) -> u64 {
        self.charged.load(Ordering::Acquire)
    }
}

impl WorkspacePlan {
    pub fn preflight(
        root: &Utf8Path,
        limits: WorkspaceLimits,
        options: CopyOptions,
    ) -> Result<Self, WorkspaceError> {
        if limits.workers == 0 {
            return Err(WorkspaceError::ZeroWorkers);
        }
        let canonical = fs::canonicalize(root)
            .map_err(|error| WorkspaceError::io("canonicalize root", root, error))?;
        let original_root =
            Utf8PathBuf::from_path_buf(canonical).map_err(|_| WorkspaceError::NonUtf8Path)?;
        let (manifest, diagnostics) = build_manifest(&original_root, &options)?;
        let workers =
            u64::try_from(limits.workers).map_err(|_| WorkspaceError::CopySizeOverflow)?;
        let aggregate = manifest
            .logical_bytes()
            .checked_mul(workers)
            .ok_or(WorkspaceError::CopySizeOverflow)?;
        if aggregate > limits.max_copy_size {
            return Err(WorkspaceError::AggregateCopyLimit {
                per_worker: manifest.logical_bytes(),
                workers: limits.workers,
                requested: aggregate,
                allowance: limits.max_copy_size,
            });
        }
        Ok(Self {
            original_root,
            options,
            limits,
            manifest,
            diagnostics,
            allowance: Arc::new(CopyAllowance {
                granted: aggregate,
                charged: AtomicU64::new(0),
            }),
            materialized: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }

    pub fn diagnostics(&self) -> &[WorkspaceDiagnostic] {
        &self.diagnostics
    }

    pub fn aggregate_bytes(&self) -> u64 {
        self.manifest
            .logical_bytes()
            .saturating_mul(self.limits.workers as u64)
    }

    pub fn observed_copy_bytes(&self) -> u64 {
        self.allowance.charged()
    }

    pub fn materialized_workers(&self) -> usize {
        self.materialized.load(Ordering::Acquire)
    }

    pub fn create_worker(&self) -> Result<WorkerWorkspace, WorkspaceError> {
        self.materialized
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < self.limits.workers).then_some(current + 1)
            })
            .map_err(|_| WorkspaceError::WorkerCountExceeded {
                workers: self.limits.workers,
            })?;

        match self.materialize_worker() {
            Ok(worker) => Ok(worker),
            Err(error) => {
                self.materialized.fetch_sub(1, Ordering::AcqRel);
                Err(error)
            }
        }
    }

    fn materialize_worker(&self) -> Result<WorkerWorkspace, WorkspaceError> {
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
        let mut snapshot = BTreeMap::new();
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
                let metadata = fs::metadata(&source).map_err(|error| {
                    WorkspaceError::io("read original metadata", &entry.path, error)
                })?;
                let permissions = metadata.permissions();
                let destination = root.join(&entry.path);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        WorkspaceError::io("create worker directory", parent, error)
                    })?;
                }
                fs::write(&destination, &bytes)
                    .map_err(|error| WorkspaceError::io("copy worker file", &entry.path, error))?;
                fs::set_permissions(&destination, permissions.clone()).map_err(|error| {
                    WorkspaceError::io("copy worker permissions", &entry.path, error)
                })?;
                snapshot.insert(entry.path.clone(), SnapshotFile { bytes, permissions });
            }

            let (current, _) = build_manifest(&self.original_root, &self.options)?;
            if !self.manifest.content_matches(&current) {
                return Err(WorkspaceError::OriginalChanged {
                    path: self
                        .manifest
                        .first_content_difference(&current)
                        .unwrap_or_default(),
                });
            }
            Ok(())
        })();

        if let Err(error) = result {
            self.allowance.release(charged);
            return Err(error);
        }

        Ok(WorkerWorkspace::from_materialized(
            temp,
            root,
            self.original_root.clone(),
            self.options.clone(),
            self.manifest.clone(),
            snapshot,
            Arc::clone(&self.allowance),
            Arc::clone(&self.materialized),
            charged,
        ))
    }
}
