use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::workspace::{
    CopyOptions, WorkerWorkspace, WorkspaceDiagnostic, WorkspaceError, WorkspaceHandler,
    WorkspacePlan, build_command_environment,
};
use hoimin_core::{
    ApplyMutation, BudgetLedger, ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateIdentity, EffectId,
    IntegrityCheckpoint, MutationCandidate, Preflight, RunBudgets, VerifyOriginals,
    reserve_workspace_copy, stable_mutant_id,
};
use tempfile::TempDir;

const SUPPORTED_WORKER_TREE_DEPTH: usize = 128;

fn outer_depth_guard_active() -> bool {
    std::env::var_os("HOIMIN_FOCUSED_MUTATION_OUTER_DEPTH_GUARD").is_some()
}

struct FixtureProject {
    temp: TempDir,
}

impl FixtureProject {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        write_file(temp.path(), "pkg/a.py", b"original\n");
        write_file(temp.path(), "pkg/b.py", b"second\n");
        Self { temp }
    }

    fn root(&self) -> &Utf8Path {
        Utf8Path::from_path(self.temp.path()).unwrap()
    }
}

fn write_file(root: &Path, path: &str, contents: &[u8]) {
    let destination = root.join(path);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, contents).unwrap();
}

fn create_nested_directories(root: &Path, depth: usize) -> String {
    let mut directory =
        cap_primitives::fs::open_ambient_dir(root, cap_primitives::ambient_authority()).unwrap();
    let mut logical_path = String::new();
    for index in 0..depth {
        let name = format!("d{index}");
        cap_primitives::fs::create_dir(
            &directory,
            Path::new(&name),
            &cap_primitives::fs::DirOptions::new(),
        )
        .unwrap();
        directory = cap_primitives::fs::open_dir_nofollow(&directory, Path::new(&name)).unwrap();
        if !logical_path.is_empty() {
            logical_path.push('/');
        }
        logical_path.push_str(&name);
    }
    logical_path
}

fn make_cleanup_wrapper_inaccessible(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o0);
    }
    #[cfg(windows)]
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).unwrap();
}

fn make_cleanup_wrapper_accessible(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o700);
    }
    #[cfg(windows)]
    {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
    }
    fs::set_permissions(path, permissions).unwrap();
}

fn preflight_plan(root: &Utf8Path, workers: u32, options: CopyOptions) -> WorkspacePlan {
    WorkspacePlan::preflight(root, EffectId(900), workers, options).unwrap()
}

fn grant_plan(plan: &WorkspacePlan, max_copy_size: u64) -> hoimin_core::WorkspaceCopyGrant {
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: max_copy_size,
        processes: 1,
    });
    reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap()
}

fn create_worker(root: &Utf8Path) -> WorkerWorkspace {
    let plan = preflight_plan(root, 1, CopyOptions::default());
    let grant = grant_plan(&plan, plan.aggregate_bytes());
    plan.create_worker(&grant.create_worker(EffectId(901), 0).unwrap())
        .unwrap()
}

#[cfg(unix)]
fn create_fifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;

    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::mkfifo(path.as_ptr(), 0o600) },
        0,
        "failed to create FIFO: {}",
        std::io::Error::last_os_error()
    );
}

#[cfg(unix)]
fn run_fifo_operation(
    worker: WorkerWorkspace,
    fifo: &Path,
    operation: impl FnOnce(&mut WorkerWorkspace) -> Result<(), WorkspaceError> + Send + 'static,
) -> (WorkerWorkspace, Result<(), WorkspaceError>) {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::FromRawFd;

    let cleanup_fifo = fifo.to_owned();
    let (sender, receiver) = sync_channel(1);
    let handle = thread::spawn(move || {
        let mut worker = worker;
        let result = operation(&mut worker);
        let _ = fs::remove_file(cleanup_fifo);
        sender.send((worker, result)).unwrap();
    });

    let returned_before_timeout = match receiver.recv_timeout(Duration::from_secs(1)) {
        Ok(completed) => {
            handle.join().unwrap();
            return completed;
        }
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => panic!("FIFO operation thread disconnected"),
    };

    let fifo = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    let rescue = unsafe { libc::open(fifo.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
    assert!(
        rescue >= 0,
        "failed to open FIFO rescue peer: {}",
        std::io::Error::last_os_error()
    );
    let rescue = unsafe { fs::File::from_raw_fd(rescue) };
    let completed = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("blocked FIFO operation did not recover");
    drop(rescue);
    handle.join().unwrap();
    assert!(
        returned_before_timeout,
        "worker operation blocked while opening a FIFO"
    );
    completed
}

fn link_created_or_platform_denied(result: std::io::Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            #[cfg(windows)]
            {
                assert!(
                    error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.kind() == std::io::ErrorKind::Unsupported
                        || error.raw_os_error() == Some(1314),
                    "unexpected Windows link setup failure: {error}"
                );
                false
            }
            #[cfg(not(windows))]
            panic!("link setup failed unexpectedly: {error}");
        }
    }
}

fn mutation_candidate(worker: &WorkerWorkspace) -> MutationCandidate {
    let mut candidate = MutationCandidate {
        id: "candidate".into(),
        sequence: 0,
        path: "pkg/a.py".into(),
        span: ByteSpan {
            start: 0,
            length: 8,
        },
        original: "original".into(),
        replacement: "mutated!".into(),
        operator: "test".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: worker
            .manifest()
            .entry(Utf8Path::new("pkg/a.py"))
            .unwrap()
            .blake3
            .to_hex()
            .to_string(),
    };
    candidate.id = stable_mutant_id(&CandidateIdentity {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        file_hash: candidate.file_hash.clone(),
        path: candidate.path.clone(),
        span: candidate.span,
        operator: candidate.operator.clone(),
        replacement: candidate.replacement.clone(),
    })
    .to_string();
    candidate
}

#[test]
fn reset_restores_changed_and_deleted_files_and_removes_new_files() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    worker.write("pkg/a.py", b"mutated\n").unwrap();
    worker.remove("pkg/b.py").unwrap();
    worker.write("generated.txt", b"new\n").unwrap();

    worker.reset().unwrap();

    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
    assert!(worker.exists("pkg/b.py").unwrap());
    assert!(!worker.exists("generated.txt").unwrap());
    assert_eq!(
        fs::read(project.root().join("pkg/a.py")).unwrap(),
        b"original\n"
    );
}

#[test]
fn reset_restores_same_size_arbitrary_worker_change() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    fs::write(worker.root().join("pkg/b.py"), b"xxxxxx\n").unwrap();

    worker.reset().unwrap();

    assert_eq!(worker.read("pkg/b.py").unwrap(), b"second\n");
}

#[cfg(target_os = "linux")]
#[test]
fn reset_removes_a_non_utf8_file_and_restores_manifest_content() {
    use std::os::unix::ffi::OsStringExt;

    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    worker.write("pkg/a.py", b"changed\n").unwrap();
    let native_name = OsString::from_vec(vec![b'g', 0x80]);
    let native_path = worker.root().as_std_path().join(&native_name);
    fs::write(&native_path, b"untracked\n").unwrap();

    worker.reset().unwrap();

    assert!(!native_path.exists());
    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
}

#[cfg(target_os = "linux")]
#[test]
fn reset_removes_nested_non_utf8_directories() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let outer = worker
        .root()
        .as_std_path()
        .join(OsString::from_vec(vec![b'd', 0x80]));
    let inner = outer.join(OsString::from_vec(vec![b'n', 0x81]));
    fs::create_dir(&outer).unwrap();
    fs::create_dir(&inner).unwrap();
    fs::write(inner.join(OsString::from_vec(vec![b'f', 0x82])), b"data").unwrap();
    symlink("missing", inner.join(OsString::from_vec(vec![b'l', 0x83]))).unwrap();
    create_fifo(&inner.join(OsString::from_vec(vec![b'p', 0x84])));

    worker.reset().unwrap();

    assert!(!outer.exists());
}

#[cfg(windows)]
#[test]
fn windows_reset_handles_or_explicitly_rejects_a_lone_surrogate_name() {
    use std::os::windows::ffi::OsStringExt;

    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let native_path = worker
        .root()
        .as_std_path()
        .join(OsString::from_wide(&[u16::from(b'x'), 0xD800]));
    match fs::write(&native_path, b"untracked") {
        Ok(()) => {
            worker.reset().unwrap();
            assert!(!native_path.exists());
        }
        Err(error) => assert!(
            error.kind() == std::io::ErrorKind::InvalidInput
                || error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(123),
            "unexpected Windows lone-surrogate setup failure: {error}"
        ),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cleanup_removes_read_only_non_utf8_entries() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::PermissionsExt;

    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let wrapper = worker.root().parent().unwrap().to_owned();
    let directory = worker
        .root()
        .as_std_path()
        .join(OsString::from_vec(vec![b'd', 0x80]));
    fs::create_dir(&directory).unwrap();
    let file = directory.join(OsString::from_vec(vec![b'f', 0x81]));
    fs::write(&file, b"data").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o400)).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();

    worker.try_cleanup().unwrap();

    assert!(!wrapper.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn handler_reset_and_cleanup_release_non_utf8_worker_state() {
    use std::os::unix::ffi::OsStringExt;

    let project = FixtureProject::new();
    let mut handler = WorkspaceHandler::new(
        project.root().to_owned(),
        Vec::new(),
        1,
        CopyOptions::default(),
    );
    let completed = handler
        .handle_preflight(Preflight { id: EffectId(910) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    handler
        .handle_create_worker(grant.create_worker(EffectId(911), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    fs::write(
        worker_root
            .as_std_path()
            .join(OsString::from_vec(vec![b'x', 0x80])),
        b"untracked",
    )
    .unwrap();

    handler
        .handle_reset_worker(hoimin_core::ResetWorker {
            id: EffectId(912),
            worker: 0,
        })
        .unwrap();
    let cleaned = handler
        .handle_cleanup(grant.cleanup(EffectId(913)))
        .unwrap();

    assert_eq!(cleaned.released_reservations, vec![grant.reservation_id()]);
    assert_eq!(handler.worker_count(), 0);
    assert_eq!(handler.pending_cleanup_count(), 0);
    assert!(!worker_root.exists());
}

#[test]
fn reset_handles_a_tree_at_the_supported_depth() {
    if outer_depth_guard_active() {
        return;
    }
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    create_nested_directories(worker.root().as_std_path(), SUPPORTED_WORKER_TREE_DEPTH);

    worker.reset().unwrap();

    assert!(!worker.exists("d0").unwrap());
}

#[test]
fn reset_reports_a_depth_error_beyond_the_supported_depth() {
    if outer_depth_guard_active() {
        return;
    }
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    create_nested_directories(worker.root().as_std_path(), SUPPORTED_WORKER_TREE_DEPTH + 1);

    let error = worker.reset().unwrap_err();

    assert!(matches!(
        error,
        WorkspaceError::TreeDepthExceeded {
            limit: SUPPORTED_WORKER_TREE_DEPTH,
            ..
        }
    ));
}

#[test]
fn cleanup_reports_the_same_depth_error_as_reset() {
    if outer_depth_guard_active() {
        return;
    }
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    create_nested_directories(worker.root().as_std_path(), SUPPORTED_WORKER_TREE_DEPTH + 1);

    let error = worker.try_cleanup().unwrap_err();

    assert!(matches!(
        error,
        WorkspaceError::TreeDepthExceeded {
            limit: SUPPORTED_WORKER_TREE_DEPTH,
            ..
        }
    ));
}

#[test]
fn cleanup_restores_access_to_the_temporary_wrapper() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let wrapper = worker.root().parent().unwrap().to_owned();
    make_cleanup_wrapper_inaccessible(wrapper.as_std_path());

    if let Err(error) = worker.try_cleanup() {
        make_cleanup_wrapper_accessible(wrapper.as_std_path());
        worker.try_cleanup().unwrap();
        panic!("cleanup did not restore wrapper permissions: {error}");
    }

    assert!(!wrapper.exists());
}

#[test]
fn file_apis_create_nested_replace_read_only_and_remove_files() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let read_only = worker.root().join("pkg/a.py");
    let mut permissions = fs::metadata(&read_only).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&read_only, permissions).unwrap();

    worker.write("generated/nested.txt", b"nested\n").unwrap();
    worker.write("pkg/a.py", b"replacement\n").unwrap();

    assert_eq!(worker.read("generated/nested.txt").unwrap(), b"nested\n");
    assert_eq!(worker.read("pkg/a.py").unwrap(), b"replacement\n");
    assert!(!worker.exists("missing.txt").unwrap());
    worker.remove("generated/nested.txt").unwrap();
    assert!(!worker.exists("generated/nested.txt").unwrap());
}

#[cfg(unix)]
#[test]
fn reading_a_fifo_returns_without_waiting_for_a_peer() {
    let project = FixtureProject::new();
    let worker = create_worker(project.root());
    let fifo = worker.root().join("planted");
    create_fifo(fifo.as_std_path());

    let (_worker, result) = run_fifo_operation(worker, fifo.as_std_path(), |worker| {
        worker.read("planted").map(|_| ())
    });

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
}

#[cfg(unix)]
#[test]
fn writing_a_fifo_returns_without_waiting_for_a_peer() {
    let project = FixtureProject::new();
    let worker = create_worker(project.root());
    let fifo = worker.root().join("planted");
    create_fifo(fifo.as_std_path());

    let (_worker, result) = run_fifo_operation(worker, fifo.as_std_path(), |worker| {
        worker.write("planted", b"replacement")
    });

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
}

#[cfg(unix)]
#[test]
fn removing_a_fifo_returns_without_waiting_for_a_peer() {
    let project = FixtureProject::new();
    let worker = create_worker(project.root());
    let fifo = worker.root().join("planted");
    create_fifo(fifo.as_std_path());

    let (_worker, result) = run_fifo_operation(worker, fifo.as_std_path(), |worker| {
        worker.remove("planted")
    });

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
}

#[cfg(unix)]
#[test]
fn reset_removes_a_fifo_without_waiting_for_a_peer() {
    let project = FixtureProject::new();
    let worker = create_worker(project.root());
    let fifo = worker.root().join("planted");
    create_fifo(fifo.as_std_path());

    let (worker, result) = run_fifo_operation(worker, fifo.as_std_path(), WorkerWorkspace::reset);

    result.unwrap();
    assert!(!worker.exists("planted").unwrap());
    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
}

#[test]
fn excludes_git_venv_and_caches() {
    let project = FixtureProject::new();
    for path in [
        ".git/config",
        ".venv/lib/site.py",
        "venv/lib/site.py",
        "pkg/__pycache__/a.pyc",
        ".pytest_cache/state",
        ".mypy_cache/state",
        ".ruff_cache/state",
        ".pyre/state",
        ".pytype/state",
        ".tox/state",
    ] {
        write_file(project.temp.path(), path, b"cache");
    }

    let plan = preflight_plan(project.root(), 1, CopyOptions::default());
    let paths = plan
        .manifest()
        .entries()
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec!["pkg/a.py", "pkg/b.py"]);
}

#[test]
fn broad_include_restores_gitignored_files_without_default_excluded_directories() {
    let project = FixtureProject::new();
    write_file(project.temp.path(), ".gitignore", b"ignored.txt\n");
    write_file(project.temp.path(), "ignored.txt", b"included");
    for path in [
        ".git/config",
        ".venv/lib/site.py",
        "venv/lib/site.py",
        "env/lib/site.py",
        "pkg/__pycache__/a.pyc",
        ".pytest_cache/state",
        ".mypy_cache/state",
        ".ruff_cache/state",
        ".pyre/state",
        ".pytype/state",
        ".tox/state",
        ".nox/state",
    ] {
        write_file(project.temp.path(), path, b"excluded");
    }

    let plan = preflight_plan(
        project.root(),
        1,
        CopyOptions {
            includes: vec!["**".into()],
            excludes: Vec::new(),
        },
    );
    let paths = plan
        .manifest()
        .entries()
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();

    assert!(paths.contains(&"ignored.txt"));
    for excluded in [
        ".git/config",
        ".venv/lib/site.py",
        "venv/lib/site.py",
        "env/lib/site.py",
        "pkg/__pycache__/a.pyc",
        ".pytest_cache/state",
        ".mypy_cache/state",
        ".ruff_cache/state",
        ".pyre/state",
        ".pytype/state",
        ".tox/state",
        ".nox/state",
    ] {
        assert!(
            !paths.contains(&excluded),
            "{excluded} entered the manifest"
        );
    }
}

#[test]
fn explicit_exclude_wins_over_include_and_gitignore() {
    let project = FixtureProject::new();
    write_file(project.temp.path(), ".gitignore", b"fixtures/\n");
    write_file(project.temp.path(), "fixtures/keep.txt", b"keep");
    write_file(project.temp.path(), "fixtures/secret.txt", b"secret");

    let plan = WorkspacePlan::preflight(
        project.root(),
        EffectId(902),
        1,
        CopyOptions {
            includes: vec!["fixtures/**".into()],
            excludes: vec!["fixtures/secret.txt".into()],
        },
    )
    .unwrap();
    let paths = plan
        .manifest()
        .entries()
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();

    assert!(paths.contains(&"fixtures/keep.txt"));
    assert!(!paths.contains(&"fixtures/secret.txt"));
}

#[test]
fn skips_symlink_with_typed_diagnostic() {
    let project = FixtureProject::new();
    let link = project.temp.path().join("linked.py");
    if !link_created_or_platform_denied(create_file_symlink(
        project.temp.path().join("pkg/a.py"),
        &link,
    )) {
        return;
    }

    let plan = preflight_plan(project.root(), 1, CopyOptions::default());

    assert_eq!(
        plan.diagnostics(),
        &[WorkspaceDiagnostic::SymlinkSkipped {
            path: Utf8PathBuf::from("linked.py")
        }]
    );
    assert!(plan.manifest().entry(Utf8Path::new("linked.py")).is_none());
}

#[cfg(unix)]
fn create_file_symlink(target: PathBuf, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: PathBuf, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[test]
fn rejects_aggregate_copy_over_allowance_before_materializing() {
    let project = FixtureProject::new();
    let per_worker = b"original\n".len() as u64 + b"second\n".len() as u64;

    let plan = preflight_plan(project.root(), 2, CopyOptions::default());
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: per_worker * 2 - 1,
        processes: 1,
    });

    let error = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap_err();

    assert_eq!(error.code(), "workspace.copy.limit");
    assert_eq!(plan.materialized_workers(), 0);
}

#[test]
fn charges_every_worker_copy_against_one_allowance() {
    let project = FixtureProject::new();
    let per_worker = b"original\n".len() as u64 + b"second\n".len() as u64;
    let plan = preflight_plan(project.root(), 2, CopyOptions::default());
    let grant = grant_plan(&plan, per_worker * 2);

    let _first = plan
        .create_worker(&grant.create_worker(EffectId(910), 0).unwrap())
        .unwrap();
    assert_eq!(plan.observed_copy_bytes(), per_worker);
    let _second = plan
        .create_worker(&grant.create_worker(EffectId(911), 1).unwrap())
        .unwrap();
    assert_eq!(plan.observed_copy_bytes(), per_worker * 2);
    assert!(grant.create_worker(EffectId(912), 2).is_err());
}

#[test]
fn dropping_a_worker_releases_its_copy_charge_and_worker_slot() {
    let project = FixtureProject::new();
    let per_worker = b"original\n".len() as u64 + b"second\n".len() as u64;
    let plan = preflight_plan(project.root(), 1, CopyOptions::default());
    let grant = grant_plan(&plan, per_worker);
    let request = grant.create_worker(EffectId(920), 0).unwrap();
    let worker = plan.create_worker(&request).unwrap();
    assert_eq!(plan.materialized_workers(), 1);
    assert_eq!(plan.observed_copy_bytes(), per_worker);

    drop(worker);

    assert_eq!(plan.materialized_workers(), 0);
    assert_eq!(plan.observed_copy_bytes(), 0);
    let _replacement = plan.create_worker(&request).unwrap();
}

#[test]
fn original_growth_does_not_change_snapshot_copy_charge() {
    let project = FixtureProject::new();
    let per_worker = b"original\n".len() as u64 + b"second\n".len() as u64;
    let plan = preflight_plan(project.root(), 1, CopyOptions::default());
    let grant = grant_plan(&plan, per_worker);
    fs::write(
        project.root().join("pkg/a.py"),
        b"much larger original contents\n",
    )
    .unwrap();

    let worker = plan
        .create_worker(&grant.create_worker(EffectId(930), 0).unwrap())
        .unwrap();

    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
    assert_eq!(plan.observed_copy_bytes(), per_worker);
    assert_eq!(plan.materialized_workers(), 1);
}

#[test]
fn resets_read_only_file() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    worker.write("pkg/a.py", b"mutated\n").unwrap();
    let path = worker.root().join("pkg/a.py");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();

    worker.reset().unwrap();

    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
}

#[test]
fn dropping_a_worker_removes_read_only_files() {
    let project = FixtureProject::new();
    let worker = create_worker(project.root());
    let root = worker.root().to_owned();
    let path = root.join("pkg/a.py");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();

    drop(worker);

    assert!(!root.exists());
}

#[test]
fn reset_restores_the_private_snapshot_after_original_change() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    fs::write(project.root().join("pkg/a.py"), b"changed outside\n").unwrap();
    worker.write("pkg/a.py", b"mutated\n").unwrap();

    worker.reset().unwrap();

    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
    assert_eq!(
        fs::read(project.root().join("pkg/a.py")).unwrap(),
        b"changed outside\n"
    );
}

#[test]
fn mutation_checks_hash_exact_original_and_span() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let candidate = mutation_candidate(&worker);

    worker.apply_mutation(&candidate).unwrap();
    assert_eq!(worker.read("pkg/a.py").unwrap(), b"mutated!\n");
    let mut stale = candidate;
    stale.original = "wrong!!!".into();
    assert!(matches!(
        worker.apply_mutation(&stale),
        Err(WorkspaceError::MutationHashMismatch { .. }
            | WorkspaceError::MutationOriginalMismatch { .. })
    ));
}

#[test]
fn mutation_rejects_linked_target_with_matching_bytes() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let candidate = mutation_candidate(&worker);
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), b"original\n").unwrap();
    let target = worker.root().join("pkg/a.py");
    fs::remove_file(&target).unwrap();
    if !link_created_or_platform_denied(create_file_symlink(
        outside.path().to_path_buf(),
        target.as_std_path(),
    )) {
        return;
    }

    assert!(matches!(
        worker.apply_mutation(&candidate),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(fs::read(outside.path()).unwrap(), b"original\n");
}

#[test]
fn mutation_updates_a_read_only_regular_file() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let candidate = mutation_candidate(&worker);
    let target = worker.root().join("pkg/a.py");
    let mut permissions = fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&target, permissions).unwrap();

    worker.apply_mutation(&candidate).unwrap();

    assert_eq!(worker.read("pkg/a.py").unwrap(), b"mutated!\n");
}

#[test]
fn rejected_mutation_does_not_make_the_target_writable() {
    let project = FixtureProject::new();
    let mut worker = create_worker(project.root());
    let mut candidate = mutation_candidate(&worker);
    candidate.original = "mismatch".into();
    let target = worker.root().join("pkg/a.py");
    let mut permissions = fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&target, permissions).unwrap();

    assert!(matches!(
        worker.apply_mutation(&candidate),
        Err(WorkspaceError::MutationOriginalMismatch { .. })
    ));

    assert!(fs::metadata(&target).unwrap().permissions().readonly());
    assert_eq!(fs::read(&target).unwrap(), b"original\n");
}

#[test]
fn rewrites_original_pythonpath_entries_and_deduplicates() {
    let project = FixtureProject::new();
    let worker = create_worker(project.root());
    let original_pkg = project.root().join("pkg");
    let outside = project.root().parent().unwrap().join("outside");
    let inherited_pythonpath = std::env::join_paths([
        project.root().as_std_path(),
        original_pkg.as_std_path(),
        outside.as_std_path(),
        original_pkg.as_std_path(),
    ])
    .unwrap();
    let inherited = BTreeMap::from([
        (OsString::from("PYTHONPATH"), inherited_pythonpath),
        (OsString::from("KEEP"), OsString::from("yes")),
    ]);

    let command = build_command_environment(
        project.root(),
        worker.root(),
        &[Utf8PathBuf::from("pkg")],
        &inherited,
    )
    .unwrap();
    let paths = std::env::split_paths(command.env.get(OsStr::new("PYTHONPATH")).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(command.cwd, worker.root());
    assert_eq!(
        paths,
        vec![
            worker.root().as_std_path().to_path_buf(),
            worker.root().join("pkg").into_std_path_buf(),
            outside.into_std_path_buf(),
        ]
    );
    assert_eq!(
        command.env.get(OsStr::new("KEEP")),
        Some(&OsString::from("yes"))
    );
}

#[test]
fn effect_handlers_preserve_original_ids_for_success_and_failure() {
    let project = FixtureProject::new();
    let mut handler = WorkspaceHandler::new(
        project.root().to_owned(),
        Vec::new(),
        1,
        CopyOptions::default(),
    );
    let completed = handler
        .handle_preflight(Preflight { id: EffectId(1) })
        .unwrap();
    assert_eq!(completed.id, EffectId(1));
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    assert_eq!(
        handler
            .handle_create_worker(grant.create_worker(EffectId(2), 0).unwrap())
            .unwrap()
            .id,
        EffectId(2)
    );
    let candidate = mutation_candidate(handler.worker(0).unwrap());
    assert_eq!(
        handler
            .handle_apply_mutation(
                ApplyMutation {
                    id: EffectId(3),
                    worker: 0,
                    candidate: candidate.clone(),
                },
                &candidate,
            )
            .unwrap()
            .id,
        EffectId(3)
    );
    assert_eq!(
        handler
            .handle_reset_worker(hoimin_core::ResetWorker {
                id: EffectId(4),
                worker: 0,
            })
            .unwrap()
            .id,
        EffectId(4)
    );
    assert_eq!(
        handler
            .handle_cleanup(grant.cleanup(EffectId(5)))
            .unwrap()
            .id,
        EffectId(5)
    );

    let missing = Utf8PathBuf::from("definitely/missing/workspace/root");
    let mut failing = WorkspaceHandler::new(missing, Vec::new(), 1, CopyOptions::default());
    assert_eq!(
        failing
            .handle_preflight(Preflight { id: EffectId(99) })
            .unwrap_err()
            .id,
        EffectId(99)
    );
}

#[test]
fn post_materialization_verification_rejects_changed_original() {
    const WORKERS: u32 = 2;
    let project = FixtureProject::new();
    let mut handler = WorkspaceHandler::new(
        project.root().to_owned(),
        Vec::new(),
        WORKERS,
        CopyOptions::default(),
    );
    let completed = handler
        .handle_preflight(Preflight { id: EffectId(30) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    for worker in 0..WORKERS {
        handler
            .handle_create_worker(
                grant
                    .create_worker(EffectId(u64::from(worker) + 31), worker)
                    .unwrap(),
            )
            .unwrap();
    }
    fs::write(project.root().join("pkg/a.py"), b"modified\n").unwrap();

    let error = handler
        .handle_verify_originals(VerifyOriginals {
            id: EffectId(40),
            checkpoint: IntegrityCheckpoint::PostMaterialization,
        })
        .unwrap_err();

    assert_eq!(error.failure.code(), "workspace.original.changed");
}

#[test]
fn explicit_close_removes_active_worker_before_handler_drop() {
    let project = FixtureProject::new();
    let mut handler = WorkspaceHandler::new(
        project.root().to_owned(),
        Vec::new(),
        1,
        CopyOptions::default(),
    );
    let completed = handler
        .handle_preflight(Preflight { id: EffectId(70) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    handler
        .handle_create_worker(grant.create_worker(EffectId(71), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();

    handler.close().unwrap();

    assert!(!worker_root.exists());
    assert_eq!(handler.worker_count(), 0);
    assert_eq!(handler.pending_cleanup_count(), 0);
}
