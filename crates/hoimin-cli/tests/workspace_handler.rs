use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::workspace::{
    CopyOptions, WorkerWorkspace, WorkspaceDiagnostic, WorkspaceError, WorkspaceHandler,
    WorkspaceLimits, WorkspacePlan, build_command_environment,
};
use hoimin_core::{
    ApplyMutation, ByteSpan, Cleanup, CreateWorker, EffectId, MutationCandidate, Preflight,
    ResetWorker,
};
use tempfile::TempDir;

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

fn limits(max_copy_size: u64, workers: usize) -> WorkspaceLimits {
    WorkspaceLimits {
        max_copy_size,
        workers,
    }
}

#[test]
fn reset_restores_changed_and_deleted_files_and_removes_new_files() {
    let project = FixtureProject::new();
    let mut worker = WorkerWorkspace::create(project.root(), limits(1_024, 1)).unwrap();
    worker.write("pkg/a.py", b"mutated\n").unwrap();
    worker.remove("pkg/b.py").unwrap();
    worker.write("generated.txt", b"new\n").unwrap();

    worker.reset().unwrap();

    assert_eq!(worker.read("pkg/a.py").unwrap(), b"original\n");
    assert!(worker.exists("pkg/b.py"));
    assert!(!worker.exists("generated.txt"));
    assert_eq!(
        fs::read(project.root().join("pkg/a.py")).unwrap(),
        b"original\n"
    );
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

    let plan =
        WorkspacePlan::preflight(project.root(), limits(1_024, 1), CopyOptions::default()).unwrap();
    let paths = plan
        .manifest()
        .entries()
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec!["pkg/a.py", "pkg/b.py"]);
}

#[test]
fn explicit_exclude_wins_over_include_and_gitignore() {
    let project = FixtureProject::new();
    write_file(project.temp.path(), ".gitignore", b"fixtures/\n");
    write_file(project.temp.path(), "fixtures/keep.txt", b"keep");
    write_file(project.temp.path(), "fixtures/secret.txt", b"secret");

    let plan = WorkspacePlan::preflight(
        project.root(),
        limits(1_024, 1),
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
    if create_file_symlink(project.temp.path().join("pkg/a.py"), &link).is_err() {
        return;
    }

    let plan =
        WorkspacePlan::preflight(project.root(), limits(1_024, 1), CopyOptions::default()).unwrap();

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

    let error = WorkspacePlan::preflight(
        project.root(),
        limits(per_worker * 2 - 1, 2),
        CopyOptions::default(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        WorkspaceError::AggregateCopyLimit {
            per_worker,
            workers: 2,
            requested: per_worker * 2,
            allowance: per_worker * 2 - 1,
        }
    );
}

#[test]
fn charges_every_worker_copy_against_one_allowance() {
    let project = FixtureProject::new();
    let per_worker = b"original\n".len() as u64 + b"second\n".len() as u64;
    let plan = WorkspacePlan::preflight(
        project.root(),
        limits(per_worker * 2, 2),
        CopyOptions::default(),
    )
    .unwrap();

    let _first = plan.create_worker().unwrap();
    assert_eq!(plan.observed_copy_bytes(), per_worker);
    let _second = plan.create_worker().unwrap();
    assert_eq!(plan.observed_copy_bytes(), per_worker * 2);
    assert_eq!(
        plan.create_worker().unwrap_err(),
        WorkspaceError::WorkerCountExceeded { workers: 2 }
    );
}

#[test]
fn dropping_a_worker_releases_its_copy_charge_and_worker_slot() {
    let project = FixtureProject::new();
    let per_worker = b"original\n".len() as u64 + b"second\n".len() as u64;
    let plan = WorkspacePlan::preflight(
        project.root(),
        limits(per_worker, 1),
        CopyOptions::default(),
    )
    .unwrap();
    let worker = plan.create_worker().unwrap();
    assert_eq!(plan.materialized_workers(), 1);
    assert_eq!(plan.observed_copy_bytes(), per_worker);

    drop(worker);

    assert_eq!(plan.materialized_workers(), 0);
    assert_eq!(plan.observed_copy_bytes(), 0);
    let _replacement = plan.create_worker().unwrap();
}

#[test]
fn stops_worker_creation_when_observed_bytes_exceed_grant() {
    let project = FixtureProject::new();
    let per_worker = b"original\n".len() as u64 + b"second\n".len() as u64;
    let plan = WorkspacePlan::preflight(
        project.root(),
        limits(per_worker, 1),
        CopyOptions::default(),
    )
    .unwrap();
    fs::write(
        project.root().join("pkg/a.py"),
        b"much larger original contents\n",
    )
    .unwrap();

    assert!(matches!(
        plan.create_worker().unwrap_err(),
        WorkspaceError::CopyAllowanceExceeded { .. }
    ));
    assert_eq!(plan.materialized_workers(), 0);
}

#[test]
fn resets_read_only_file() {
    let project = FixtureProject::new();
    let mut worker = WorkerWorkspace::create(project.root(), limits(1_024, 1)).unwrap();
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
    let worker = WorkerWorkspace::create(project.root(), limits(1_024, 1)).unwrap();
    let root = worker.root().to_owned();
    let path = root.join("pkg/a.py");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();

    drop(worker);

    assert!(!root.exists());
}

#[test]
fn detects_original_change() {
    let project = FixtureProject::new();
    let mut worker = WorkerWorkspace::create(project.root(), limits(1_024, 1)).unwrap();
    fs::write(project.root().join("pkg/a.py"), b"changed outside\n").unwrap();

    assert_eq!(
        worker.reset().unwrap_err(),
        WorkspaceError::OriginalChanged {
            path: Utf8PathBuf::from("pkg/a.py")
        }
    );
}

#[test]
fn mutation_checks_hash_exact_original_and_span() {
    let project = FixtureProject::new();
    let mut worker = WorkerWorkspace::create(project.root(), limits(1_024, 1)).unwrap();
    let hash = worker
        .manifest()
        .entry(Utf8Path::new("pkg/a.py"))
        .unwrap()
        .blake3
        .to_hex()
        .to_string();
    let candidate = MutationCandidate {
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
        file_hash: hash,
    };

    worker.apply_mutation(&candidate).unwrap();
    assert_eq!(worker.read("pkg/a.py").unwrap(), b"mutated!\n");
    let mut stale = candidate;
    stale.original = "wrong!!!".into();
    assert!(matches!(
        worker.apply_mutation(&stale),
        Err(WorkspaceError::MutationHashMismatch { .. })
            | Err(WorkspaceError::MutationOriginalMismatch { .. })
    ));
}

#[test]
fn rewrites_original_pythonpath_entries_and_deduplicates() {
    let project = FixtureProject::new();
    let worker = WorkerWorkspace::create(project.root(), limits(1_024, 1)).unwrap();
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
        limits(1_024, 1),
        CopyOptions::default(),
    );
    assert_eq!(
        handler
            .handle_preflight(Preflight { id: EffectId(1) })
            .unwrap()
            .id,
        EffectId(1)
    );
    assert_eq!(
        handler
            .handle_create_worker(CreateWorker { id: EffectId(2) })
            .unwrap()
            .id,
        EffectId(2)
    );
    let hash = handler
        .worker(0)
        .unwrap()
        .manifest()
        .entry(Utf8Path::new("pkg/a.py"))
        .unwrap()
        .blake3
        .to_hex()
        .to_string();
    let candidate = MutationCandidate {
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
        file_hash: hash,
    };
    assert_eq!(
        handler
            .handle_apply_mutation(ApplyMutation { id: EffectId(3) }, 0, &candidate)
            .unwrap()
            .id,
        EffectId(3)
    );
    assert_eq!(
        handler
            .handle_reset_worker(ResetWorker { id: EffectId(4) }, 0)
            .unwrap()
            .id,
        EffectId(4)
    );
    assert_eq!(
        handler
            .handle_cleanup(Cleanup { id: EffectId(5) })
            .unwrap()
            .id,
        EffectId(5)
    );

    let missing = Utf8PathBuf::from("definitely/missing/workspace/root");
    let mut failing =
        WorkspaceHandler::new(missing, Vec::new(), limits(1, 1), CopyOptions::default());
    assert_eq!(
        failing
            .handle_preflight(Preflight { id: EffectId(99) })
            .unwrap_err()
            .id,
        EffectId(99)
    );
}
