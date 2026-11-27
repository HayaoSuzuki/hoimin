use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use camino::Utf8Path;
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};
use serde::Deserialize;

use super::super::{CopyOptions, WorkerWorkspace, WorkspaceError, WorkspacePlan};
use crate::io_faults;

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../formal/HoiminOracle/corpus/workspace-restore.jsonl"
));
const REWIND: &str = "reset.snapshot.rewind";

#[derive(Deserialize)]
struct Case {
    schema: u32,
    id: String,
    unix_only: bool,
    snapshot: Vec<Entry>,
    worker: Vec<Entry>,
    fault: Option<Fault>,
    first: Observation,
    retry: Observation,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    #[serde(flatten)]
    kind: Kind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Kind {
    Directory,
    File { content: String, read_only: bool },
    Link,
}

#[derive(Deserialize)]
struct Fault {
    point: FaultPoint,
    skip: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FaultPoint {
    Rewind,
    Write,
    Permissions,
}

impl FaultPoint {
    const fn boundary(&self) -> &'static str {
        match self {
            Self::Rewind => REWIND,
            Self::Write => "worker.restore.write",
            Self::Permissions => "worker.restore.permissions",
        }
    }
}

#[derive(Deserialize)]
struct Observation {
    failed: bool,
    replaced: Vec<String>,
    tree: Vec<Entry>,
}

fn write_file(path: &Path, content: &str, read_only: bool) {
    fs::write(path, content).unwrap();
    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        fs::Permissions::from_mode(if read_only { 0o444 } else { 0o644 })
    };
    #[cfg(windows)]
    let permissions = {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(read_only);
        permissions
    };
    fs::set_permissions(path, permissions).unwrap();
}

fn populate(root: &Path, entries: &[Entry], sentinel: &Path) {
    for entry in entries {
        let path = root.join(&entry.path);
        match &entry.kind {
            Kind::Directory => fs::create_dir(path).unwrap(),
            Kind::File { content, read_only } => write_file(&path, content, *read_only),
            Kind::Link => {
                #[cfg(unix)]
                std::os::unix::fs::symlink(sentinel, path).unwrap();
                #[cfg(not(unix))]
                panic!("Unix fixture link to {}", sentinel.display());
            }
        }
    }
}

fn inventory(root: &Utf8Path) -> BTreeMap<String, Kind> {
    let mut result = BTreeMap::new();
    let mut directories = vec![root.to_owned()];
    while let Some(dir) = directories.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = camino::Utf8PathBuf::from_path_buf(entry.unwrap().path()).unwrap();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let kind = if metadata.is_symlink() {
                Kind::Link
            } else if metadata.is_dir() {
                directories.push(path.clone());
                Kind::Directory
            } else {
                assert!(metadata.is_file(), "unexpected entry {path}");
                Kind::File {
                    content: fs::read_to_string(&path).unwrap(),
                    read_only: metadata.permissions().readonly(),
                }
            };
            let key = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|part| part.as_str())
                .collect::<Vec<_>>()
                .join("/");
            assert!(result.insert(key, kind).is_none());
        }
    }
    result
}

fn expected_tree(entries: &[Entry]) -> BTreeMap<String, Kind> {
    entries
        .iter()
        .map(|entry| (entry.path.clone(), entry.kind.clone()))
        .collect()
}

fn observe_reset(
    worker: &mut WorkerWorkspace,
    case: &str,
    expected: &Observation,
    fault: Option<&Fault>,
) {
    let root = worker.root().to_owned();
    #[cfg(unix)]
    let original_files = inventory(&root)
        .into_iter()
        .filter(|(_, entry)| matches!(entry, Kind::File { .. }))
        .map(|(path, _)| {
            let file = fs::File::open(root.join(&path)).unwrap();
            (path, file)
        })
        .collect::<BTreeMap<_, _>>();

    let (boundary, skip) = fault.map_or((REWIND, expected.replaced.len()), |fault| {
        (fault.point.boundary(), fault.skip)
    });
    let probe = io_faults::arm_after(boundary, skip);
    let result = worker.reset();
    assert_eq!(probe.triggered(), expected.failed, "{case}: {result:?}");
    drop(probe);

    if expected.failed {
        assert!(
            matches!(&result, Err(WorkspaceError::WorkspaceRestore { path, message }) if *path == root && message.contains(boundary)),
            "{case}: {result:?}"
        );
    } else {
        result.unwrap_or_else(|error| panic!("{case}: {error}"));
    }
    assert_eq!(inventory(&root), expected_tree(&expected.tree), "{case}");

    #[cfg(unix)]
    for entry in &expected.tree {
        use std::os::unix::fs::MetadataExt;

        if let Kind::File { .. } = &entry.kind
            && let Some(file) = original_files.get(&entry.path)
        {
            // Hold the old file open so a removed inode cannot be reused.
            let old = file.metadata().unwrap();
            let new = fs::metadata(root.join(&entry.path)).unwrap();
            assert_eq!(
                (old.dev(), old.ino()) != (new.dev(), new.ino()),
                expected.replaced.contains(&entry.path),
                "{case}: {}",
                entry.path
            );
        }
    }
}

#[test]
fn worker_reset_matches_lean_oracle() {
    let mut cases_checked = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        if case.unix_only && !cfg!(unix) {
            continue;
        }
        cases_checked += 1;

        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("original");
        let sentinel = fixture.path().join("sentinel.txt");
        fs::create_dir(&project).unwrap();
        fs::write(&sentinel, b"outside worker").unwrap();
        populate(&project, &case.snapshot, &sentinel);
        let project = Utf8Path::from_path(&project).unwrap();
        let plan =
            WorkspacePlan::preflight(project, EffectId(1), 1, CopyOptions::default()).unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: plan.aggregate_bytes(),
            processes: 1,
        });
        let reservation = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
        let mut worker = plan
            .create_worker(&reservation.create_worker(EffectId(2), 0).unwrap())
            .unwrap();

        write_file(
            project.join("a.txt").as_std_path(),
            "edited original",
            false,
        );
        fs::set_permissions(
            project.join("pkg/b.txt"),
            fs::metadata(project.join("a.txt")).unwrap().permissions(),
        )
        .unwrap();
        let original = inventory(project);
        let root = worker.root().to_owned();
        for entry in &case.snapshot {
            if !entry.path.contains('/') {
                worker
                    .root
                    .remove_any_if_exists(Utf8Path::new(&entry.path))
                    .unwrap();
            }
        }
        populate(root.as_std_path(), &case.worker, &sentinel);
        assert_eq!(inventory(&root), expected_tree(&case.worker), "{}", case.id);

        observe_reset(&mut worker, &case.id, &case.first, case.fault.as_ref());
        assert_eq!(inventory(project), original, "{}", case.id);
        observe_reset(&mut worker, &case.id, &case.retry, None);
        assert_eq!(inventory(project), original, "{}", case.id);
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            b"outside worker",
            "{}",
            case.id
        );
    }
    assert_eq!(cases_checked, if cfg!(unix) { 60 } else { 59 });
}
