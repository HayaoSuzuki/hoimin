mod copy;
mod manifest;
mod mutation;
mod reset;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    ApplyMutation, Cleanup, CleanupFinished, CreateWorker, EffectFailed, MutationApplied,
    MutationCandidate, Preflight, PreflightCompleted, ResetWorker, WorkerCreated, WorkerReset,
};

pub use copy::WorkspacePlan;
pub use manifest::{ManifestEntry, WorkspaceManifest};
use reset::make_writable;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CopyOptions {
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceLimits {
    pub max_copy_size: u64,
    pub workers: usize,
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
    #[error(
        "aggregate workspace copy requires {requested} bytes ({per_worker} x {workers}), allowance is {allowance}"
    )]
    AggregateCopyLimit {
        per_worker: u64,
        workers: usize,
        requested: u64,
        allowance: u64,
    },
    #[error("observed workspace copy reached {observed} bytes, allowance is {allowance}")]
    CopyAllowanceExceeded { observed: u64, allowance: u64 },
    #[error("all {workers} worker workspaces are already materialized")]
    WorkerCountExceeded { workers: usize },
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
    #[error("worker {index} does not exist")]
    WorkerMissing { index: usize },
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

    pub fn code(&self) -> &'static str {
        match self {
            Self::AggregateCopyLimit { .. }
            | Self::CopyAllowanceExceeded { .. }
            | Self::CopySizeOverflow => "workspace.copy.limit",
            Self::OriginalChanged { .. } => "workspace.original.changed",
            Self::WorkspaceRestore { .. } => "workspace.restore",
            Self::MutationTargetMissing { .. }
            | Self::MutationHashMismatch { .. }
            | Self::MutationSpanInvalid { .. }
            | Self::MutationOriginalMismatch { .. } => "workspace.mutation.invalid",
            Self::InvalidGlob(_) => "workspace.glob.invalid",
            _ => "workspace.io",
        }
    }
}

#[derive(Debug)]
pub(crate) struct SnapshotFile {
    bytes: Vec<u8>,
    permissions: fs::Permissions,
}

#[derive(Debug)]
pub struct WorkerWorkspace {
    temp: tempfile::TempDir,
    root: Utf8PathBuf,
    original_root: Utf8PathBuf,
    options: CopyOptions,
    manifest: WorkspaceManifest,
    snapshot: BTreeMap<Utf8PathBuf, SnapshotFile>,
    allowance: Arc<copy::CopyAllowance>,
    materialized: Arc<AtomicUsize>,
    charged: u64,
}

impl WorkerWorkspace {
    pub fn create(
        original_root: &Utf8Path,
        limits: WorkspaceLimits,
    ) -> Result<Self, WorkspaceError> {
        WorkspacePlan::preflight(original_root, limits, CopyOptions::default())?.create_worker()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_materialized(
        temp: tempfile::TempDir,
        root: Utf8PathBuf,
        original_root: Utf8PathBuf,
        options: CopyOptions,
        manifest: WorkspaceManifest,
        snapshot: BTreeMap<Utf8PathBuf, SnapshotFile>,
        allowance: Arc<copy::CopyAllowance>,
        materialized: Arc<AtomicUsize>,
        charged: u64,
    ) -> Self {
        Self {
            temp,
            root,
            original_root,
            options,
            manifest,
            snapshot,
            allowance,
            materialized,
            charged,
        }
    }

    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    pub fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }

    pub fn read(&self, path: impl AsRef<Utf8Path>) -> Result<Vec<u8>, WorkspaceError> {
        let path = path.as_ref();
        fs::read(self.root.join(path))
            .map_err(|error| WorkspaceError::io("read worker file", path, error))
    }

    pub fn write(
        &mut self,
        path: impl AsRef<Utf8Path>,
        contents: &[u8],
    ) -> Result<(), WorkspaceError> {
        let path = path.as_ref();
        let destination = self.root.join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| WorkspaceError::io("create worker directory", parent, error))?;
        }
        if destination.exists() {
            make_writable(&destination)?;
        }
        fs::write(destination, contents)
            .map_err(|error| WorkspaceError::io("write worker file", path, error))
    }

    pub fn remove(&mut self, path: impl AsRef<Utf8Path>) -> Result<(), WorkspaceError> {
        let path = path.as_ref();
        let destination = self.root.join(path);
        make_writable(&destination)?;
        fs::remove_file(destination)
            .map_err(|error| WorkspaceError::io("remove worker file", path, error))
    }

    pub fn exists(&self, path: impl AsRef<Utf8Path>) -> bool {
        self.root.join(path.as_ref()).exists()
    }
}

impl Drop for WorkerWorkspace {
    fn drop(&mut self) {
        self.allowance.release(self.charged);
        self.materialized.fetch_sub(1, Ordering::AcqRel);
        let _ = &self.temp;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandEnvironment {
    pub cwd: Utf8PathBuf,
    pub env: BTreeMap<OsString, OsString>,
}

pub fn build_command_environment(
    original_root: &Utf8Path,
    worker_root: &Utf8Path,
    source_roots: &[Utf8PathBuf],
    inherited: &BTreeMap<OsString, OsString>,
) -> Result<CommandEnvironment, WorkspaceError> {
    let mut env = inherited.clone();
    let inherited_pythonpath = env.remove(OsStr::new("PYTHONPATH")).unwrap_or_default();
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
    limits: WorkspaceLimits,
    options: CopyOptions,
    plan: Option<WorkspacePlan>,
    workers: Vec<WorkerWorkspace>,
}

impl WorkspaceHandler {
    pub fn new(
        original_root: Utf8PathBuf,
        source_roots: Vec<Utf8PathBuf>,
        limits: WorkspaceLimits,
        options: CopyOptions,
    ) -> Self {
        Self {
            original_root,
            source_roots,
            limits,
            options,
            plan: None,
            workers: Vec::new(),
        }
    }

    pub fn handle_preflight(
        &mut self,
        request: Preflight,
    ) -> Result<PreflightCompleted, EffectFailed> {
        let id = request.id;
        WorkspacePlan::preflight(&self.original_root, self.limits, self.options.clone())
            .map(|plan| {
                self.plan = Some(plan);
                PreflightCompleted { id }
            })
            .map_err(|error| effect_failed(id, error))
    }

    pub fn handle_create_worker(
        &mut self,
        request: CreateWorker,
    ) -> Result<WorkerCreated, EffectFailed> {
        let id = request.id;
        let result = self
            .plan
            .as_ref()
            .ok_or(WorkspaceError::WorkerMissing { index: 0 })
            .and_then(WorkspacePlan::create_worker);
        result
            .map(|worker| {
                self.workers.push(worker);
                WorkerCreated { id }
            })
            .map_err(|error| effect_failed(id, error))
    }

    pub fn handle_apply_mutation(
        &mut self,
        request: ApplyMutation,
        worker: usize,
        candidate: &MutationCandidate,
    ) -> Result<MutationApplied, EffectFailed> {
        let id = request.id;
        self.workers
            .get_mut(worker)
            .ok_or(WorkspaceError::WorkerMissing { index: worker })
            .and_then(|workspace| workspace.apply_mutation(candidate))
            .map(|()| MutationApplied { id })
            .map_err(|error| effect_failed(id, error))
    }

    pub fn handle_reset_worker(
        &mut self,
        request: ResetWorker,
        worker: usize,
    ) -> Result<WorkerReset, EffectFailed> {
        let id = request.id;
        self.workers
            .get_mut(worker)
            .ok_or(WorkspaceError::WorkerMissing { index: worker })
            .and_then(WorkerWorkspace::reset)
            .map(|()| WorkerReset { id })
            .map_err(|error| effect_failed(id, error))
    }

    pub fn handle_cleanup(&mut self, request: Cleanup) -> Result<CleanupFinished, EffectFailed> {
        self.workers.clear();
        self.plan = None;
        Ok(CleanupFinished { id: request.id })
    }

    pub fn worker(&self, index: usize) -> Option<&WorkerWorkspace> {
        self.workers.get(index)
    }

    pub fn command_environment(
        &self,
        worker: usize,
        inherited: &BTreeMap<OsString, OsString>,
    ) -> Result<CommandEnvironment, WorkspaceError> {
        let workspace = self
            .workers
            .get(worker)
            .ok_or(WorkspaceError::WorkerMissing { index: worker })?;
        build_command_environment(
            &self.original_root,
            workspace.root(),
            &self.source_roots,
            inherited,
        )
    }
}

pub fn handle_preflight(
    handler: &mut WorkspaceHandler,
    request: Preflight,
) -> Result<PreflightCompleted, EffectFailed> {
    handler.handle_preflight(request)
}

pub fn handle_create_worker(
    handler: &mut WorkspaceHandler,
    request: CreateWorker,
) -> Result<WorkerCreated, EffectFailed> {
    handler.handle_create_worker(request)
}

pub fn handle_apply_mutation(
    handler: &mut WorkspaceHandler,
    request: ApplyMutation,
    worker: usize,
    candidate: &MutationCandidate,
) -> Result<MutationApplied, EffectFailed> {
    handler.handle_apply_mutation(request, worker, candidate)
}

pub fn handle_reset_worker(
    handler: &mut WorkspaceHandler,
    request: ResetWorker,
    worker: usize,
) -> Result<WorkerReset, EffectFailed> {
    handler.handle_reset_worker(request, worker)
}

pub fn handle_cleanup(
    handler: &mut WorkspaceHandler,
    request: Cleanup,
) -> Result<CleanupFinished, EffectFailed> {
    handler.handle_cleanup(request)
}

fn effect_failed(id: hoimin_core::EffectId, error: WorkspaceError) -> EffectFailed {
    EffectFailed {
        id,
        message: format!("{}: {error}", error.code()),
    }
}
