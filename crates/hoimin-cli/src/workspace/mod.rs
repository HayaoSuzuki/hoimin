mod copy;
mod manifest;
mod mutation;
mod reset;
mod root;

#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    ApplyMutation, Cleanup, CleanupFinished, CreateWorker, EffectFailed, EffectFailure,
    MutationApplied, MutationCandidate, OriginalsVerified, Preflight, PreflightCompleted,
    ResetWorker, VerifyOriginals, WorkerCreated, WorkerReset,
};

use copy::ValidatedPreflightError;
pub use copy::WorkspacePlan;
pub use manifest::{ManifestEntry, WorkspaceManifest};
use root::WorkerRoot;

pub(crate) fn build_validation_manifest(
    root: &Utf8Path,
    options: &CopyOptions,
) -> Result<WorkspaceManifest, WorkspaceError> {
    manifest::build_manifest(root, options).map(|(manifest, _)| manifest)
}

/// Each worker-tree level may retain a directory and iterator handle while it
/// is being visited. Keep enough headroom for the process's other open files.
pub(super) const MAX_WORKER_TREE_DEPTH: usize = 128;

#[derive(Debug, thiserror::Error)]
pub(crate) enum RootRelativeReadError {
    #[error("root-relative file was not found")]
    NotFound,
    #[error(transparent)]
    Other(#[from] WorkspaceError),
}

#[derive(Debug)]
pub(crate) struct RootRelativeReader {
    root: WorkerRoot,
}

impl RootRelativeReader {
    pub(crate) fn open(root: Utf8PathBuf) -> Result<Self, WorkspaceError> {
        WorkerRoot::open(root).map(|root| Self { root })
    }

    pub(crate) fn read(&self, path: &Utf8Path) -> Result<Vec<u8>, RootRelativeReadError> {
        match self.root.read(path) {
            Ok(bytes) => Ok(bytes),
            Err(error) => match self.root.is_missing(path) {
                Ok(true) => Err(RootRelativeReadError::NotFound),
                Ok(false) | Err(_) => Err(RootRelativeReadError::Other(error)),
            },
        }
    }
}

pub(crate) fn read_root_relative(
    root: &Utf8Path,
    path: &Utf8Path,
) -> Result<Vec<u8>, RootRelativeReadError> {
    RootRelativeReader::open(root.to_owned())?.read(path)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CopyOptions {
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WorkspaceDiagnostic {
    SymlinkSkipped { path: Utf8PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkspaceError {
    #[error("workspace root is not a directory: {0}")]
    RootNotDirectory(Utf8PathBuf),
    #[error("workspace worker count must be non-zero")]
    ZeroWorkers,
    #[error("workspace copy size overflow")]
    CopySizeOverflow,
    #[error("workspace copy grant {requested} does not match expected aggregate {expected}")]
    InvalidGrant { requested: u64, expected: u64 },
    #[error("workspace preflight {received:?} does not match plan preflight {expected:?}")]
    PreflightMismatch {
        expected: hoimin_core::EffectId,
        received: hoimin_core::EffectId,
    },
    #[error("worker {worker} is outside requested worker count {requested_workers}")]
    WorkerOutOfRange { worker: u32, requested_workers: u32 },
    #[error("workspace allowance {received} does not match plan allowance {expected}")]
    AllowanceMismatch { expected: u64, received: u64 },
    #[error("workspace reservation {received:?} does not match bound reservation {expected:?}")]
    ReservationMismatch {
        expected: hoimin_core::ReservationId,
        received: hoimin_core::ReservationId,
    },
    #[error("observed workspace copy reached {observed} bytes, allowance is {allowance}")]
    CopyAllowanceExceeded { observed: u64, allowance: u64 },
    #[error("worker {worker} already exists")]
    WorkerAlreadyExists { worker: u32 },
    #[error("original workspace changed: {path}")]
    OriginalChanged { path: Utf8PathBuf },
    #[error("mutation target is not in the manifest: {path}")]
    MutationTargetMissing { path: Utf8PathBuf },
    #[error("mutation target hash does not match: {path}")]
    MutationHashMismatch { path: Utf8PathBuf },
    #[error("mutation span is invalid: {path}")]
    MutationSpanInvalid { path: Utf8PathBuf },
    #[error("mutation original bytes do not match: {path}")]
    MutationOriginalMismatch { path: Utf8PathBuf },
    #[error("worker restore failed for {path}: {message}")]
    WorkspaceRestore { path: Utf8PathBuf, message: String },
    #[error("worker {worker} does not exist")]
    WorkerMissing { worker: u32 },
    #[error("workspace path is not a normalized root-relative path: {path}")]
    InvalidPath { path: Utf8PathBuf },
    #[error("workspace snapshot path collides on the destination filesystem: {path}")]
    SnapshotPathCollision { path: Utf8PathBuf },
    #[error("workspace tree depth exceeds limit {limit}: {path}")]
    TreeDepthExceeded { path: Utf8PathBuf, limit: usize },
    #[error("workspace plan state was poisoned")]
    StatePoisoned,
    #[error("invalid workspace include/exclude glob: {0}")]
    InvalidGlob(String),
    #[error("workspace walk failed: {0}")]
    Walk(String),
    #[error("workspace path is not valid UTF-8")]
    NonUtf8Path,
    #[error("workspace path escaped its root")]
    OutsideRoot,
    #[error("cannot construct PYTHONPATH: {0}")]
    PythonPath(String),
    #[error("{operation} failed for {path}: {message}")]
    Io {
        operation: &'static str,
        path: Utf8PathBuf,
        message: String,
    },
}

impl WorkspaceError {
    pub(crate) fn io(
        operation: &'static str,
        path: impl AsRef<Utf8Path>,
        error: impl fmt::Display,
    ) -> Self {
        Self::Io {
            operation,
            path: path.as_ref().to_owned(),
            message: error.to_string(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidGrant { .. }
            | Self::CopyAllowanceExceeded { .. }
            | Self::CopySizeOverflow => "workspace.copy.limit",
            Self::PreflightMismatch { .. } => "workspace.preflight.mismatch",
            Self::WorkerOutOfRange { .. } => "workspace.worker.out_of_range",
            Self::AllowanceMismatch { .. } => "workspace.allowance.mismatch",
            Self::ReservationMismatch { .. } => "workspace.grant.invalid",
            Self::OriginalChanged { .. } => "workspace.original.changed",
            Self::WorkspaceRestore { .. } => "workspace.restore",
            Self::MutationTargetMissing { .. }
            | Self::MutationHashMismatch { .. }
            | Self::MutationSpanInvalid { .. }
            | Self::MutationOriginalMismatch { .. } => "workspace.mutation.invalid",
            Self::InvalidGlob(_) => "workspace.glob.invalid",
            Self::InvalidPath { .. } => "workspace.path.invalid",
            Self::SnapshotPathCollision { .. } => "workspace.path.collision",
            Self::TreeDepthExceeded { .. } => "workspace.path.depth",
            _ => "workspace.io",
        }
    }
}

#[derive(Debug)]
pub(crate) struct SnapshotFile {
    permissions: fs::Permissions,
    permission_fingerprint: PermissionFingerprint,
    blake3: blake3::Hash,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResetIoMetrics {
    pub(crate) tree_walks: u64,
    pub(crate) snapshot_bytes: u64,
    pub(crate) worker_bytes: u64,
}

#[cfg(test)]
thread_local! {
    static RESET_IO_METRICS: Cell<ResetIoMetrics> = const { Cell::new(ResetIoMetrics {
        tree_walks: 0,
        snapshot_bytes: 0,
        worker_bytes: 0,
    }) };
}

#[cfg(test)]
pub(crate) fn reset_io_metrics() {
    RESET_IO_METRICS.with(|metrics| metrics.set(ResetIoMetrics::default()));
}

#[cfg(test)]
pub(crate) fn current_reset_io_metrics() -> ResetIoMetrics {
    RESET_IO_METRICS.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn record_reset_tree_walk() {
    RESET_IO_METRICS.with(|metrics| {
        let mut current = metrics.get();
        current.tree_walks += 1;
        metrics.set(current);
    });
}

#[cfg(test)]
pub(crate) fn record_reset_snapshot_bytes(bytes: usize) {
    RESET_IO_METRICS.with(|metrics| {
        let mut current = metrics.get();
        current.snapshot_bytes += bytes as u64;
        metrics.set(current);
    });
}

#[cfg(test)]
pub(crate) fn record_reset_worker_bytes(bytes: usize) {
    RESET_IO_METRICS.with(|metrics| {
        let mut current = metrics.get();
        current.worker_bytes += bytes as u64;
        metrics.set(current);
    });
}

impl SnapshotFile {
    fn new(permissions: fs::Permissions, blake3: blake3::Hash) -> Self {
        Self {
            permission_fingerprint: permission_fingerprint(&permissions),
            permissions,
            blake3,
        }
    }
}

#[derive(Debug)]
pub(crate) struct DiskSnapshot {
    _temp: tempfile::TempDir,
    root: Utf8PathBuf,
    files: BTreeMap<Utf8PathBuf, SnapshotFile>,
}

impl DiskSnapshot {
    fn read(&self, path: &Utf8Path) -> Result<Vec<u8>, WorkspaceError> {
        let bytes = fs::read(self.root.join(path))
            .map_err(|error| WorkspaceError::io("read shared snapshot", path, error))?;
        #[cfg(test)]
        record_reset_snapshot_bytes(bytes.len());
        Ok(bytes)
    }
}

#[cfg(windows)]
type PermissionFingerprint = bool;

#[cfg(unix)]
type PermissionFingerprint = u32;

#[cfg(windows)]
fn permission_fingerprint(permissions: &fs::Permissions) -> PermissionFingerprint {
    permissions.readonly()
}

#[cfg(unix)]
fn permission_fingerprint(permissions: &fs::Permissions) -> PermissionFingerprint {
    use std::os::unix::fs::PermissionsExt;
    permissions.mode()
}

fn make_tree_writable(root: &Path, error_path: &Utf8Path) -> Result<(), WorkspaceError> {
    let mut stack = vec![(root.to_owned(), 0_usize)];
    while let Some((path, depth)) = stack.pop() {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(WorkspaceError::io(
                    "inspect cleanup path",
                    error_path,
                    error,
                ));
            }
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        make_cleanup_entry_accessible(&path, &metadata, error_path)?;
        if metadata.is_dir() {
            let entries = fs::read_dir(&path)
                .map_err(|error| WorkspaceError::io("read cleanup directory", error_path, error))?;
            for entry in entries {
                let entry = entry
                    .map_err(|error| WorkspaceError::io("read cleanup entry", error_path, error))?;
                let child = entry.path();
                let child_depth = depth + 1;
                if child_depth > MAX_WORKER_TREE_DEPTH {
                    return Err(WorkspaceError::TreeDepthExceeded {
                        path: Utf8PathBuf::from_path_buf(child)
                            .unwrap_or_else(|_| error_path.to_owned()),
                        limit: MAX_WORKER_TREE_DEPTH,
                    });
                }
                stack.push((child, child_depth));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn make_cleanup_entry_accessible(
    path: &Path,
    metadata: &fs::Metadata,
    error_path: &Utf8Path,
) -> Result<(), WorkspaceError> {
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)
            .map_err(|error| WorkspaceError::io("prepare cleanup path", error_path, error))?;
    }
    Ok(())
}

#[cfg(unix)]
fn make_cleanup_entry_accessible(
    path: &Path,
    metadata: &fs::Metadata,
    error_path: &Utf8Path,
) -> Result<(), WorkspaceError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = metadata.permissions();
    let required = if metadata.is_dir() { 0o700 } else { 0o200 };
    if permissions.mode() & required != required {
        permissions.set_mode(permissions.mode() | required);
        fs::set_permissions(path, permissions)
            .map_err(|error| WorkspaceError::io("prepare cleanup path", error_path, error))?;
    }
    Ok(())
}

#[derive(Debug)]
pub struct WorkerWorkspace {
    temp: tempfile::TempDir,
    root: WorkerRoot,
    manifest: WorkspaceManifest,
    snapshot: Arc<DiskSnapshot>,
    allowance: Arc<copy::CopyAllowance>,
    plan_state: Arc<Mutex<copy::PlanState>>,
    worker: u32,
    charged: u64,
    cleanup_complete: bool,
}

impl WorkerWorkspace {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_materialized(
        temp: tempfile::TempDir,
        root: WorkerRoot,
        manifest: WorkspaceManifest,
        snapshot: Arc<DiskSnapshot>,
        allowance: Arc<copy::CopyAllowance>,
        plan_state: Arc<Mutex<copy::PlanState>>,
        worker: u32,
        charged: u64,
    ) -> Self {
        Self {
            temp,
            root,
            manifest,
            snapshot,
            allowance,
            plan_state,
            worker,
            charged,
            cleanup_complete: false,
        }
    }

    #[must_use]
    pub fn root(&self) -> &Utf8Path {
        self.root.path()
    }

    #[must_use]
    pub const fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }

    /// # Errors
    /// Returns `InvalidPath` or an I/O failure when the path cannot be read.
    pub fn read(&self, path: impl AsRef<Utf8Path>) -> Result<Vec<u8>, WorkspaceError> {
        self.root.read(path.as_ref())
    }

    /// # Errors
    /// Returns `InvalidPath` or an I/O failure when the file cannot be written.
    pub fn write(
        &mut self,
        path: impl AsRef<Utf8Path>,
        contents: &[u8],
    ) -> Result<(), WorkspaceError> {
        self.root.write(path.as_ref(), contents)
    }

    /// # Errors
    /// Returns `InvalidPath` or an I/O failure when the file cannot be removed.
    pub fn remove(&mut self, path: impl AsRef<Utf8Path>) -> Result<(), WorkspaceError> {
        self.root.remove_file(path.as_ref())
    }

    /// # Errors
    /// Returns `InvalidPath` when the supplied path is not root-relative.
    pub fn exists(&self, path: impl AsRef<Utf8Path>) -> Result<bool, WorkspaceError> {
        self.root.try_exists(path.as_ref())
    }

    /// # Errors
    /// Returns an I/O failure when the temporary workspace cannot be removed.
    pub fn try_cleanup(&mut self) -> Result<(), WorkspaceError> {
        if self.cleanup_complete {
            return Ok(());
        }
        self.root.close();
        let wrapper = self.temp.path();
        let wrapper_metadata = match fs::symlink_metadata(wrapper) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.cleanup_complete = true;
                return Ok(());
            }
            Err(error) => {
                let wrapper = Utf8Path::from_path(wrapper).unwrap_or(self.root.path());
                return Err(WorkspaceError::io(
                    "inspect cleanup wrapper",
                    wrapper,
                    error,
                ));
            }
        };
        let wrapper_error_path = Utf8Path::from_path(wrapper).unwrap_or(self.root.path());
        make_cleanup_entry_accessible(wrapper, &wrapper_metadata, wrapper_error_path)?;
        make_tree_writable(self.root.path().as_std_path(), self.root.path())?;
        match fs::remove_dir_all(self.temp.path()) {
            Ok(()) => {
                self.cleanup_complete = true;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.cleanup_complete = true;
                Ok(())
            }
            Err(error) => Err(WorkspaceError::io(
                "remove worker workspace",
                self.root.path(),
                error,
            )),
        }
    }
}

impl Drop for WorkerWorkspace {
    fn drop(&mut self) {
        if !self.cleanup_complete && self.try_cleanup().is_err() {
            return;
        }
        if fs::symlink_metadata(self.temp.path()).is_ok() {
            return;
        }
        self.allowance.release(self.charged);
        if let Ok(mut state) = self.plan_state.lock() {
            state.workers.remove(&self.worker);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandEnvironment {
    pub cwd: Utf8PathBuf,
    pub env: BTreeMap<OsString, OsString>,
}

/// # Errors
/// Returns `PythonPath` if the rewritten environment cannot be encoded.
pub fn build_command_environment(
    original_root: &Utf8Path,
    worker_root: &Utf8Path,
    source_roots: &[Utf8PathBuf],
    inherited: &BTreeMap<OsString, OsString>,
) -> Result<CommandEnvironment, WorkspaceError> {
    let mut env = inherited.clone();
    let inherited_pythonpath = take_pythonpath(&mut env);
    let mut paths = Vec::<PathBuf>::new();
    let mut seen = BTreeSet::<OsString>::new();

    push_unique(
        worker_root.as_std_path().to_path_buf(),
        &mut paths,
        &mut seen,
    );
    for source in source_roots {
        let path = rewrite_project_path(original_root, worker_root, source.as_std_path());
        push_unique(path, &mut paths, &mut seen);
    }
    for path in std::env::split_paths(&inherited_pythonpath) {
        let path = rewrite_project_path(original_root, worker_root, &path);
        push_unique(path, &mut paths, &mut seen);
    }
    let pythonpath = std::env::join_paths(paths)
        .map_err(|error| WorkspaceError::PythonPath(error.to_string()))?;
    env.insert(OsString::from("PYTHONPATH"), pythonpath);
    Ok(CommandEnvironment {
        cwd: worker_root.to_owned(),
        env,
    })
}

#[cfg(windows)]
// Keys must be owned before they can be removed from the same map.
#[allow(
    clippy::needless_collect,
    reason = "keys borrow the map before mutable removal"
)]
fn take_pythonpath(env: &mut BTreeMap<OsString, OsString>) -> OsString {
    let keys = env
        .keys()
        .filter(|key| key.to_string_lossy().eq_ignore_ascii_case("PYTHONPATH"))
        .cloned()
        .collect::<Vec<_>>();
    let paths = keys
        .into_iter()
        .filter_map(|key| env.remove(&key))
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    std::env::join_paths(paths).unwrap_or_default()
}

#[cfg(not(windows))]
fn take_pythonpath(env: &mut BTreeMap<OsString, OsString>) -> OsString {
    env.remove(std::ffi::OsStr::new("PYTHONPATH"))
        .unwrap_or_default()
}

fn rewrite_project_path(original_root: &Utf8Path, worker_root: &Utf8Path, path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        original_root.as_std_path().join(path)
    };
    let normalized = fs::canonicalize(&absolute).unwrap_or_else(|_| lexical_normalize(&absolute));
    let normalized_root = fs::canonicalize(original_root)
        .unwrap_or_else(|_| lexical_normalize(original_root.as_std_path()));
    relative_inside(&normalized, &normalized_root)
        .map(|relative| worker_root.as_std_path().join(relative))
        .unwrap_or(normalized)
}

fn relative_inside(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative.to_path_buf());
    }
    relative_inside_platform(path, root)
}

#[cfg(windows)]
fn relative_inside_platform(path: &Path, root: &Path) -> Option<PathBuf> {
    let path_components = path.components().collect::<Vec<_>>();
    let root_components = root.components().collect::<Vec<_>>();
    if root_components.len() > path_components.len()
        || !root_components
            .iter()
            .zip(&path_components)
            .all(|(left, right)| {
                left.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
            })
    {
        return None;
    }
    Some(
        path_components[root_components.len()..]
            .iter()
            .map(|component| component.as_os_str())
            .collect(),
    )
}

#[cfg(not(windows))]
fn relative_inside_platform(_path: &Path, _root: &Path) -> Option<PathBuf> {
    None
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn push_unique(path: PathBuf, paths: &mut Vec<PathBuf>, seen: &mut BTreeSet<OsString>) {
    let key = path_key(&path);
    if seen.insert(key) {
        paths.push(path);
    }
}

#[cfg(windows)]
fn path_key(path: &Path) -> OsString {
    OsString::from(lexical_normalize(path).to_string_lossy().to_lowercase())
}

#[cfg(not(windows))]
fn path_key(path: &Path) -> OsString {
    lexical_normalize(path).into_os_string()
}

#[derive(Debug)]
pub struct WorkspaceHandler {
    original_root: Utf8PathBuf,
    source_roots: Vec<Utf8PathBuf>,
    requested_workers: u32,
    options: CopyOptions,
    plan: Option<WorkspacePlan>,
    workers: BTreeMap<u32, WorkerWorkspace>,
    pending_cleanup: BTreeMap<u32, WorkerWorkspace>,
}

impl WorkspaceHandler {
    #[must_use]
    pub const fn new(
        original_root: Utf8PathBuf,
        source_roots: Vec<Utf8PathBuf>,
        requested_workers: u32,
        options: CopyOptions,
    ) -> Self {
        Self {
            original_root,
            source_roots,
            requested_workers,
            options,
            plan: None,
            workers: BTreeMap::new(),
            pending_cleanup: BTreeMap::new(),
        }
    }

    /// # Errors
    /// Returns `EffectFailed` for invalid workspace roots, manifests, or copy limits.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "event ownership is the workspace state-machine boundary"
    )]
    pub fn handle_preflight(
        &mut self,
        request: Preflight,
    ) -> Result<PreflightCompleted, EffectFailed> {
        let id = request.id;
        WorkspacePlan::preflight(
            &self.original_root,
            id,
            self.requested_workers,
            self.options.clone(),
        )
        .map(|plan| {
            let completed = plan.completed();
            self.plan = Some(plan);
            completed
        })
        .map_err(|error| effect_failed(id, error))
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "event ownership is the workspace state-machine boundary"
    )]
    pub(crate) fn handle_preflight_validated(
        &mut self,
        request: Preflight,
        validate: impl FnOnce(&Utf8Path, &WorkspaceManifest) -> Result<(), EffectFailed>,
    ) -> Result<PreflightCompleted, EffectFailed> {
        let id = request.id;
        match WorkspacePlan::preflight_validated(
            &self.original_root,
            id,
            self.requested_workers,
            self.options.clone(),
            validate,
        ) {
            Ok(plan) => {
                let completed = plan.completed();
                self.plan = Some(plan);
                Ok(completed)
            }
            Err(ValidatedPreflightError::Workspace(error)) => Err(effect_failed(id, error)),
            Err(ValidatedPreflightError::Validation(error)) => Err(error),
        }
    }

    /// # Errors
    /// Returns `EffectFailed` for missing plans, invalid grants, duplicate workers, or I/O failures.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "event ownership is the workspace state-machine boundary"
    )]
    pub fn handle_create_worker(
        &mut self,
        request: CreateWorker,
    ) -> Result<WorkerCreated, EffectFailed> {
        let id = request.id();
        let worker = request.worker();
        let reservation_id = request.reservation_id();
        if self.workers.contains_key(&worker) {
            return Err(effect_failed(
                id,
                WorkspaceError::WorkerAlreadyExists { worker },
            ));
        }
        if let Some(pending) = self.pending_cleanup.get_mut(&worker) {
            self.plan
                .as_ref()
                .ok_or(WorkspaceError::WorkerMissing { worker })
                .and_then(|plan| plan.validate_grant(&request))
                .map_err(|error| effect_failed(id, error))?;
            pending
                .try_cleanup()
                .map_err(|error| effect_failed(id, error))?;
            self.pending_cleanup.remove(&worker);
        }
        let result = self
            .plan
            .as_ref()
            .ok_or(WorkspaceError::WorkerMissing { worker })
            .and_then(|plan| plan.create_worker(&request));
        result
            .map(|workspace| {
                self.workers.insert(worker, workspace);
                WorkerCreated {
                    id,
                    worker,
                    reservation_id,
                }
            })
            .map_err(|error| effect_failed(id, error))
    }

    /// # Errors
    /// Returns `EffectFailed` for missing workers, changed originals, invalid mutations, or I/O failures.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "event ownership is the workspace state-machine boundary"
    )]
    pub fn handle_apply_mutation(
        &mut self,
        request: ApplyMutation,
        candidate: &MutationCandidate,
    ) -> Result<MutationApplied, EffectFailed> {
        let id = request.id;
        self.workers
            .get_mut(&request.worker)
            .ok_or(WorkspaceError::WorkerMissing {
                worker: request.worker,
            })
            .and_then(|workspace| workspace.apply_mutation(candidate))
            .map(|()| MutationApplied {
                id,
                worker: request.worker,
            })
            .map_err(|error| effect_failed(id, error))
    }

    /// # Errors
    /// Returns `EffectFailed` for missing workers, changed originals, or restoration failures.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "event ownership is the workspace state-machine boundary"
    )]
    pub fn handle_reset_worker(
        &mut self,
        request: ResetWorker,
    ) -> Result<WorkerReset, EffectFailed> {
        let id = request.id;
        let worker = request.worker;
        let result = self
            .workers
            .get_mut(&worker)
            .ok_or(WorkspaceError::WorkerMissing { worker })
            .and_then(WorkerWorkspace::reset);
        match result {
            Ok(()) => Ok(WorkerReset { id, worker }),
            Err(error) => {
                let reported_error = if let Some(mut discarded) = self.workers.remove(&worker) {
                    let discarded_root = discarded.root().to_owned();
                    match discarded.try_cleanup() {
                        Ok(()) => error,
                        Err(cleanup_error) => {
                            self.pending_cleanup.insert(worker, discarded);
                            if matches!(error, WorkspaceError::TreeDepthExceeded { .. }) {
                                error
                            } else {
                                WorkspaceError::WorkspaceRestore {
                                    path: discarded_root,
                                    message: format!(
                                        "{error}; discard cleanup failed: {cleanup_error}"
                                    ),
                                }
                            }
                        }
                    }
                } else {
                    error
                };
                Err(effect_failed(id, reported_error))
            }
        }
    }

    /// # Errors
    /// Returns `EffectFailed` for invalid reservations or worker cleanup failures.
    pub fn handle_cleanup(&mut self, request: Cleanup) -> Result<CleanupFinished, EffectFailed> {
        if let Some(bound) = self.plan.as_ref().and_then(WorkspacePlan::reservation_id) {
            match request.reservations.as_slice() {
                [received] if *received != bound => {
                    return Err(effect_failed(
                        request.id,
                        WorkspaceError::ReservationMismatch {
                            expected: bound,
                            received: *received,
                        },
                    ));
                }
                [received] if *received == bound => {}
                _ => {
                    return Err(effect_failed(
                        request.id,
                        WorkspaceError::InvalidGrant {
                            requested: request.reservations.len() as u64,
                            expected: 1,
                        },
                    ));
                }
            }
        }
        for workspace in self.workers.values_mut() {
            workspace
                .try_cleanup()
                .map_err(|error| effect_failed(request.id, error))?;
        }
        for workspace in self.pending_cleanup.values_mut() {
            workspace
                .try_cleanup()
                .map_err(|error| effect_failed(request.id, error))?;
        }
        self.workers.clear();
        self.pending_cleanup.clear();
        self.plan = None;
        Ok(CleanupFinished {
            id: request.id,
            released_reservations: request.reservations,
        })
    }

    /// # Errors
    /// Returns `EffectFailed` when preflight is absent or source contents changed.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "event ownership is the workspace state-machine boundary"
    )]
    pub fn handle_verify_originals(
        &self,
        request: VerifyOriginals,
    ) -> Result<OriginalsVerified, EffectFailed> {
        let id = request.id;
        self.plan
            .as_ref()
            .ok_or(WorkspaceError::WorkerMissing { worker: 0 })
            .and_then(WorkspacePlan::verify_originals)
            .map(|()| OriginalsVerified {
                id,
                checkpoint: request.checkpoint,
            })
            .map_err(|error| effect_failed(id, error))
    }

    #[must_use]
    pub fn worker(&self, worker: u32) -> Option<&WorkerWorkspace> {
        self.workers.get(&worker)
    }

    pub fn worker_mut(&mut self, worker: u32) -> Option<&mut WorkerWorkspace> {
        self.workers.get_mut(&worker)
    }

    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    #[must_use]
    pub fn pending_cleanup_count(&self) -> usize {
        self.pending_cleanup.len()
    }

    /// # Errors
    /// Returns the first worker cleanup failure.
    pub fn close(&mut self) -> Result<(), WorkspaceError> {
        let mut first_error = None;
        for workspace in self.workers.values_mut() {
            if let Err(error) = workspace.try_cleanup()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        for workspace in self.pending_cleanup.values_mut() {
            if let Err(error) = workspace.try_cleanup()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        self.workers.clear();
        self.pending_cleanup.clear();
        self.plan = None;
        Ok(())
    }

    pub fn observed_copy_bytes(&self) -> u64 {
        self.plan
            .as_ref()
            .map(WorkspacePlan::observed_copy_bytes)
            .unwrap_or_default()
    }

    pub fn materialized_worker_slots(&self) -> usize {
        self.plan
            .as_ref()
            .map(WorkspacePlan::materialized_workers)
            .unwrap_or_default()
    }

    /// # Errors
    /// Returns `WorkerMissing` or `PythonPath` when the environment cannot be built.
    pub fn command_environment(
        &self,
        worker: u32,
        inherited: &BTreeMap<OsString, OsString>,
    ) -> Result<CommandEnvironment, WorkspaceError> {
        let workspace = self
            .workers
            .get(&worker)
            .ok_or(WorkspaceError::WorkerMissing { worker })?;
        build_command_environment(
            &self.original_root,
            workspace.root(),
            &self.source_roots,
            inherited,
        )
    }
}

/// # Errors
/// Returns the corresponding workspace handler `EffectFailed`.
pub fn handle_preflight(
    handler: &mut WorkspaceHandler,
    request: Preflight,
) -> Result<PreflightCompleted, EffectFailed> {
    handler.handle_preflight(request)
}

/// # Errors
/// Returns the corresponding workspace handler `EffectFailed`.
pub fn handle_create_worker(
    handler: &mut WorkspaceHandler,
    request: CreateWorker,
) -> Result<WorkerCreated, EffectFailed> {
    handler.handle_create_worker(request)
}

/// # Errors
/// Returns the corresponding workspace handler `EffectFailed`.
pub fn handle_apply_mutation(
    handler: &mut WorkspaceHandler,
    request: ApplyMutation,
    candidate: &MutationCandidate,
) -> Result<MutationApplied, EffectFailed> {
    handler.handle_apply_mutation(request, candidate)
}

/// # Errors
/// Returns the corresponding workspace handler `EffectFailed`.
pub fn handle_reset_worker(
    handler: &mut WorkspaceHandler,
    request: ResetWorker,
) -> Result<WorkerReset, EffectFailed> {
    handler.handle_reset_worker(request)
}

/// # Errors
/// Returns the corresponding workspace handler `EffectFailed`.
pub fn handle_cleanup(
    handler: &mut WorkspaceHandler,
    request: Cleanup,
) -> Result<CleanupFinished, EffectFailed> {
    handler.handle_cleanup(request)
}

/// # Errors
/// Returns the corresponding workspace handler `EffectFailed`.
pub fn handle_verify_originals(
    handler: &WorkspaceHandler,
    request: VerifyOriginals,
) -> Result<OriginalsVerified, EffectFailed> {
    handler.handle_verify_originals(request)
}

fn effect_failed(id: hoimin_core::EffectId, error: WorkspaceError) -> EffectFailed {
    let code = error.code().to_owned();
    let message = error.to_string();
    let failure = match error {
        WorkspaceError::WorkspaceRestore { path, message } => {
            EffectFailure::WorkspaceRestore { path, message }
        }
        WorkspaceError::OriginalChanged { path } => EffectFailure::OriginalChanged { path },
        WorkspaceError::InvalidGrant {
            requested,
            expected,
        }
        | WorkspaceError::CopyAllowanceExceeded {
            observed: requested,
            allowance: expected,
        } => EffectFailure::CopyLimit {
            requested,
            allowance: expected,
        },
        WorkspaceError::ReservationMismatch { expected, received } => {
            EffectFailure::InvalidWorkspaceGrant { expected, received }
        }
        WorkspaceError::PreflightMismatch { expected, received } => {
            EffectFailure::WorkspacePreflightMismatch { expected, received }
        }
        WorkspaceError::WorkerOutOfRange {
            worker,
            requested_workers,
        } => EffectFailure::WorkspaceWorkerOutOfRange {
            worker,
            requested_workers,
        },
        WorkspaceError::AllowanceMismatch { expected, received } => {
            EffectFailure::WorkspaceAllowanceMismatch { expected, received }
        }
        WorkspaceError::InvalidPath { path } => EffectFailure::InvalidWorkspacePath { path },
        WorkspaceError::MutationTargetMissing { path }
        | WorkspaceError::MutationHashMismatch { path }
        | WorkspaceError::MutationSpanInvalid { path }
        | WorkspaceError::MutationOriginalMismatch { path } => EffectFailure::InvalidMutation {
            code,
            path,
            message,
        },
        WorkspaceError::WorkerMissing { worker }
        | WorkspaceError::WorkerAlreadyExists { worker } => EffectFailure::WorkerMissing { worker },
        WorkspaceError::Io {
            operation,
            path,
            message,
        } => EffectFailure::Io {
            code,
            operation: operation.to_owned(),
            path: Some(path),
            message,
        },
        _other => EffectFailure::Io {
            code,
            operation: "workspace".to_owned(),
            path: None,
            message,
        },
    };
    EffectFailed { id, failure }
}
