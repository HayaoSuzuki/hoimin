//! Creation admission and reset recovery, shared by synchronous and owned tasks.

use camino::Utf8PathBuf;
use hoimin_core::CreateWorker;

use super::{WorkerWorkspace, WorkspaceError, WorkspacePlan, validate_copied_import_roots};

/// Success owns one active workspace. Failure owns at most one pending workspace;
/// absent pending ownership means cleanup completed or materialization never succeeded.
/// Keeping the alternatives together prevents publishing a rejected workspace.
pub(super) enum WorkerOperationOutcome {
    Active(Box<WorkerWorkspace>),
    Failed {
        error: WorkspaceError,
        pending: Option<Box<WorkerWorkspace>>,
    },
}

/// Validates authority before touching pending ownership, drops successfully cleaned
/// ownership before replacement, and returns rejected copied roots as pending cleanup.
/// Materialization failures release their own plan slot; retained workspaces keep theirs.
pub(super) fn create_worker(
    plan: &WorkspacePlan,
    request: &CreateWorker,
    import_roots: &[Utf8PathBuf],
    mut pending: Option<Box<WorkerWorkspace>>,
) -> WorkerOperationOutcome {
    if let Some(workspace) = pending.as_mut() {
        if let Err(error) = plan
            .validate_grant(request)
            .and_then(|()| workspace.try_cleanup())
        {
            return WorkerOperationOutcome::Failed { error, pending };
        }
        drop(pending.take());
    }
    match plan.create_worker(request) {
        Ok(workspace) => {
            let workspace = Box::new(workspace);
            if let Err(error) = validate_copied_import_roots(&workspace, import_roots) {
                WorkerOperationOutcome::Failed {
                    error,
                    pending: Some(workspace),
                }
            } else {
                WorkerOperationOutcome::Active(workspace)
            }
        }
        Err(error) => WorkerOperationOutcome::Failed {
            error,
            pending: None,
        },
    }
}

/// Restores the private snapshot or discards a failed worker. Failed discard retains
/// ownership and its copy allowance for retry. Depth failures remain primary even
/// when discard also fails; other failures retain both diagnostics in their old order.
pub(super) fn reset_worker(mut workspace: Box<WorkerWorkspace>) -> WorkerOperationOutcome {
    match workspace.reset() {
        Ok(()) => WorkerOperationOutcome::Active(workspace),
        Err(error) => {
            let discarded_root = workspace.root().to_owned();
            match workspace.try_cleanup() {
                Ok(()) => WorkerOperationOutcome::Failed {
                    error,
                    pending: None,
                },
                Err(cleanup_error) => {
                    let error = if matches!(error, WorkspaceError::TreeDepthExceeded { .. }) {
                        error
                    } else {
                        WorkspaceError::WorkspaceRestore {
                            path: discarded_root,
                            message: format!("{error}; discard cleanup failed: {cleanup_error}"),
                        }
                    };
                    WorkerOperationOutcome::Failed {
                        error,
                        pending: Some(workspace),
                    }
                }
            }
        }
    }
}
