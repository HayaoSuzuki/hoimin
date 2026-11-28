use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};
use serde::Deserialize;

use super::{
    SourceContentMetrics, inventory_logical_bytes, reset_source_content_metrics,
    selected_copy_membership, source_content_metrics,
};
use crate::workspace::{
    CopyOptions, LiteralExclusion, WorkspaceDiagnostic, WorkspaceError, WorkspacePlan,
};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/workspace-selection.jsonl");

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    platform: String,
    source: Vec<Entry>,
    includes: Vec<String>,
    excludes: Vec<String>,
    literals: Vec<Literal>,
    expected: Expected,
}

#[derive(Deserialize)]
struct Entry {
    path: Utf8PathBuf,
    #[serde(flatten)]
    kind: Kind,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Kind {
    Directory,
    File { content: String },
    Link { target: LinkTarget },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum LinkTarget {
    File,
    Directory,
    Missing,
}

impl LinkTarget {
    const fn fixture_name(&self) -> &'static str {
        match self {
            Self::File => "external-file",
            Self::Directory => "external-directory",
            Self::Missing => "missing-external-file",
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Literal {
    File { path: Utf8PathBuf },
    Tree { path: Utf8PathBuf },
}

impl From<Literal> for LiteralExclusion {
    fn from(literal: Literal) -> Self {
        match literal {
            Literal::File { path } => Self::File(path),
            Literal::Tree { path } => Self::Tree(path),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Expected {
    InvalidLiteral,
    InvalidGlob,
    Manifest(Manifest),
}

#[derive(Deserialize)]
struct Manifest {
    files: BTreeMap<Utf8PathBuf, String>,
    directories: BTreeSet<Utf8PathBuf>,
    symlinks: BTreeSet<Utf8PathBuf>,
    logical_bytes: u64,
}

fn setup(base: &Utf8Path, root: &Utf8Path, source: &[Entry]) {
    fs::create_dir(root).unwrap();
    fs::write(base.join("external-file"), b"outside content").unwrap();
    fs::create_dir(base.join("external-directory")).unwrap();
    fs::write(
        base.join("external-directory/never-copy.py"),
        b"outside directory",
    )
    .unwrap();
    for entry in source {
        let path = root.join(&entry.path);
        match &entry.kind {
            Kind::Directory => fs::create_dir(path).unwrap(),
            Kind::File { content } => fs::write(path, content).unwrap(),
            Kind::Link { target } => {
                let destination = base.join(target.fixture_name());
                #[cfg(unix)]
                std::os::unix::fs::symlink(destination, path).unwrap();
                #[cfg(not(unix))]
                panic!("Unix symlink fixture {path} to {destination}");
            }
        }
    }
}

fn check_metadata_paths(
    root: &Utf8Path,
    options: &CopyOptions,
    source: &[Entry],
    expected: &Manifest,
    case: &str,
) {
    reset_source_content_metrics();
    assert_eq!(
        inventory_logical_bytes(root, options).unwrap(),
        expected.logical_bytes,
        "{case} inventory"
    );
    for subset in [false, true] {
        let requested = source
            .iter()
            .enumerate()
            .filter(|(index, _)| !subset || index % 3 == 0)
            .map(|(_, entry)| entry.path.as_path())
            .chain([Utf8Path::new("missing-oracle-file")])
            .collect::<BTreeSet<_>>();
        let matching = expected
            .files
            .keys()
            .filter(|path| requested.contains(path.as_path()))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            selected_copy_membership(root, options, requested).unwrap(),
            matching,
            "{case} membership subset={subset}"
        );
    }
    assert_eq!(
        source_content_metrics(),
        SourceContentMetrics {
            reads: 0,
            read_bytes: 0,
            hashes: 0,
            hash_bytes: 0
        },
        "{case} metadata-only selection"
    );
}

fn check_materialized(plan: &WorkspacePlan, expected: &Manifest, case: &str) {
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    let mut worker = plan
        .create_worker(&grant.create_worker(EffectId(2), 1).unwrap())
        .unwrap();
    let root = worker.root().to_owned();
    let mut folders = vec![root.clone()];
    let mut directories = BTreeSet::new();
    let mut files = BTreeMap::new();
    while let Some(directory) = folders.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
            let relative = path.strip_prefix(&root).unwrap().to_owned();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                directories.insert(relative);
                folders.push(path);
            } else {
                assert!(kind.is_file(), "{case}: copied non-file {path}");
                files.insert(relative, fs::read_to_string(path).unwrap());
            }
        }
    }
    assert_eq!(files, expected.files, "{case} materialized files");
    assert_eq!(
        directories, expected.directories,
        "{case} materialized directories"
    );
    assert_eq!(
        plan.observed_copy_bytes(),
        expected.logical_bytes,
        "{case} copied bytes"
    );
    worker.try_cleanup().unwrap();
    drop(worker);
    assert_eq!(plan.observed_copy_bytes(), 0, "{case} cleanup");
}

fn check_manifest(plan: &WorkspacePlan, expected: &Manifest, case: &str) {
    assert_eq!(
        plan.manifest().logical_bytes(),
        expected.logical_bytes,
        "{case} manifest bytes"
    );
    assert_eq!(
        plan.aggregate_bytes(),
        2 * expected.logical_bytes,
        "{case} aggregate bytes"
    );
    assert_eq!(
        plan.manifest().entries().len(),
        expected.files.len(),
        "{case} manifest size"
    );
    for item in plan.manifest().entries() {
        let expected = &expected.files[&item.path];
        assert_eq!(item.size, expected.len() as u64, "{case}: {}", item.path);
        assert_eq!(
            item.blake3,
            blake3::hash(expected.as_bytes()),
            "{case}: {}",
            item.path
        );
    }
    assert_eq!(
        plan.manifest()
            .directories()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        expected.directories,
        "{case} manifest directories"
    );
    let symlinks = plan
        .diagnostics()
        .iter()
        .map(|diagnostic| match diagnostic {
            WorkspaceDiagnostic::SymlinkSkipped { path } => path.clone(),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(symlinks, expected.symlinks, "{case} symlinks");
    assert_eq!(
        plan.diagnostics().len(),
        symlinks.len(),
        "{case} duplicate diagnostic"
    );
}

#[test]
fn workspace_selection_matches_lean_oracle() {
    let mut ids = HashSet::new();
    let mut checked = 0;
    let platform = if cfg!(windows) { "windows" } else { "unix" };
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert!(ids.insert(case.id.clone()));
        if case.platform != platform {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let base = Utf8Path::from_path(temp.path()).unwrap();
        let root = base.join("project");
        setup(base, &root, &case.source);
        let options = CopyOptions {
            includes: case.includes,
            excludes: case.excludes,
            literal_exclusions: case
                .literals
                .into_iter()
                .map(LiteralExclusion::from)
                .collect(),
        };
        let plan = WorkspacePlan::preflight(&root, EffectId(1), 2, options.clone());
        match case.expected {
            Expected::InvalidLiteral => assert!(
                matches!(plan, Err(WorkspaceError::InvalidPath { path }) if path == "../outside"),
                "{}",
                case.id
            ),
            Expected::InvalidGlob => assert!(
                matches!(plan, Err(WorkspaceError::InvalidGlob(_))),
                "{}",
                case.id
            ),
            Expected::Manifest(expected) => {
                let plan = plan.unwrap_or_else(|error| panic!("{}: {error}", case.id));
                check_metadata_paths(&root, &options, &case.source, &expected, &case.id);
                check_manifest(&plan, &expected, &case.id);
                check_materialized(&plan, &expected, &case.id);
                assert_eq!(
                    fs::read(base.join("external-file")).unwrap(),
                    b"outside content",
                    "{}",
                    case.id
                );
            }
        }
        checked += 1;
    }
    assert_eq!(ids.len(), 237);
    assert_eq!(checked, if cfg!(windows) { 116 } else { 121 });
}
