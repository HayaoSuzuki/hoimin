use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    BudgetLedger, EffectId, EnvironmentSnapshot, RunBudgets, RunConfig, reserve_workspace_copy,
};
use serde::Deserialize;

use crate::cli::parse_config_from;
use crate::workspace::{
    CopyOptions, LiteralExclusion, WorkspaceDiagnostic, WorkspaceError, WorkspacePlan,
};

use super::super::prepare_session_artifacts;

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/session-artifacts.jsonl");

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    platform: String,
    database: Option<Utf8PathBuf>,
    initial: Vec<Entry>,
    after_preparation: Vec<Entry>,
    include_all: bool,
    expected: Expected,
}

#[derive(Deserialize)]
struct Entry {
    path: Utf8PathBuf,
    #[serde(flatten)]
    kind: Kind,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Kind {
    Directory,
    File { content: String },
    Link { target: Utf8PathBuf },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Exclusion {
    File { path: Utf8PathBuf },
    Tree { path: Utf8PathBuf },
}

impl From<Exclusion> for LiteralExclusion {
    fn from(exclusion: Exclusion) -> Self {
        match exclusion {
            Exclusion::File { path } => Self::File(path),
            Exclusion::Tree { path } => Self::Tree(path),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Expected {
    SessionPathError,
    Prepared {
        database: Option<Utf8PathBuf>,
        exclusions: Vec<Exclusion>,
        copy: CopyResult,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CopyResult {
    InvalidWorkspacePath,
    Copied {
        directories: BTreeSet<Utf8PathBuf>,
        files: BTreeMap<Utf8PathBuf, String>,
        symlinks: BTreeSet<Utf8PathBuf>,
    },
}

fn create_entries(base: &Utf8Path, entries: &[Entry]) {
    for entry in entries {
        let path = base.join(&entry.path);
        match &entry.kind {
            Kind::Directory => fs::create_dir(path).unwrap(),
            Kind::File { content } => fs::write(path, content).unwrap(),
            Kind::Link { target } => {
                let target = base.join(target);
                #[cfg(unix)]
                std::os::unix::fs::symlink(target, path).unwrap();
                #[cfg(not(unix))]
                panic!("Unix symlink fixture: {path} to {target}");
            }
        }
    }
}

fn read_tree(root: &Utf8Path) -> BTreeMap<Utf8PathBuf, Kind> {
    let mut tree = BTreeMap::new();
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
            let metadata = entry.file_type().unwrap();
            let kind = if metadata.is_dir() {
                directories.push(path.clone());
                Kind::Directory
            } else if metadata.is_symlink() {
                Kind::Link {
                    target: Utf8PathBuf::from_path_buf(fs::read_link(&path).unwrap()).unwrap(),
                }
            } else {
                assert!(metadata.is_file(), "unexpected entry at {path}");
                Kind::File {
                    content: fs::read_to_string(&path).unwrap(),
                }
            };
            tree.insert(path.strip_prefix(root).unwrap().to_owned(), kind);
        }
    }
    tree
}

fn parse_config(root: &Utf8Path, database: Option<&Utf8Path>, include_all: bool) -> RunConfig {
    let mut args = [
        "hoimin",
        "run",
        "--root",
        root.as_str(),
        "--file",
        "calc.py",
    ]
    .map(OsString::from)
    .to_vec();
    if let Some(database) = database {
        args.extend(["--session", database.as_str()].map(OsString::from));
    }
    if include_all {
        args.extend(["--include", "**"].map(OsString::from));
    }
    args.extend(["--", "unused-test-command"].map(OsString::from));
    parse_config_from(args)
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap()
}

fn check_copy(
    plan: &WorkspacePlan,
    directories: BTreeSet<Utf8PathBuf>,
    files: BTreeMap<Utf8PathBuf, String>,
    symlinks: &BTreeSet<Utf8PathBuf>,
    id: &str,
) {
    let manifest = plan.manifest();
    assert_eq!(
        manifest.entries().len(),
        files.len(),
        "{id} manifest length"
    );
    for (path, content) in &files {
        let entry = manifest
            .entry(path)
            .unwrap_or_else(|| panic!("{id} missing from manifest: {path}"));
        assert_eq!(
            entry.blake3,
            blake3::hash(content.as_bytes()),
            "{id} {path}"
        );
    }
    let diagnostic_paths = plan
        .diagnostics()
        .iter()
        .map(|diagnostic| match diagnostic {
            WorkspaceDiagnostic::SymlinkSkipped { path } => path.clone(),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(&diagnostic_paths, symlinks, "{id} skipped links");
    assert_eq!(plan.diagnostics().len(), symlinks.len(), "{id} diagnostics");
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: files.values().map(|file| file.len() as u64).sum(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    let worker = plan
        .create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
        .unwrap();
    let expected = directories
        .into_iter()
        .map(|path| (path, Kind::Directory))
        .chain(
            files
                .into_iter()
                .map(|(path, content)| (path, Kind::File { content })),
        )
        .collect::<BTreeMap<_, _>>();
    assert_eq!(read_tree(worker.root()), expected, "{id} worker tree");
}

fn check_case(case: Case) {
    let temp = tempfile::tempdir().unwrap();
    let base = Utf8Path::from_path(temp.path()).unwrap();
    let root = base.join("project");
    create_entries(base, &case.initial);
    let user_database = case.database.map(|path| base.join(path));
    let config = parse_config(&root, user_database.as_deref(), case.include_all);
    let before = read_tree(base);
    let prepared = prepare_session_artifacts(&config);
    assert_eq!(read_tree(base), before, "{} preparation writes", case.id);
    let Expected::Prepared {
        database,
        exclusions,
        copy,
    } = case.expected
    else {
        assert!(prepared.is_err(), "{}: {prepared:?}", case.id);
        return;
    };
    let (actual_exclusions, actual_database) =
        prepared.unwrap_or_else(|error| panic!("{}: {error}", case.id));
    let canonical_base = Utf8PathBuf::from_path_buf(fs::canonicalize(base).unwrap()).unwrap();
    assert_eq!(
        actual_database,
        database.map(|path| canonical_base.join(path)),
        "{} database",
        case.id
    );
    let expected_exclusions = exclusions
        .into_iter()
        .map(LiteralExclusion::from)
        .collect::<Vec<_>>();
    assert_eq!(
        actual_exclusions, expected_exclusions,
        "{} exclusions",
        case.id
    );
    create_entries(base, &case.after_preparation);
    let result = WorkspacePlan::preflight(
        &root,
        EffectId(1),
        1,
        CopyOptions {
            includes: config.selection.includes.clone(),
            excludes: config.selection.excludes.clone(),
            literal_exclusions: actual_exclusions,
        },
    );
    match copy {
        CopyResult::InvalidWorkspacePath => {
            assert!(
                matches!(result, Err(WorkspaceError::InvalidPath { ref path }) if path.as_str().is_empty()),
                "{} expected rejection of the project root: {:?}",
                case.id,
                result.err()
            );
        }
        CopyResult::Copied {
            directories,
            files,
            symlinks,
        } => {
            let plan = result.unwrap_or_else(|error| panic!("{}: {error}", case.id));
            check_copy(&plan, directories, files, &symlinks, &case.id);
        }
    }
}

#[test]
fn session_artifacts_match_lean_oracle() {
    let platform = if cfg!(windows) { "windows" } else { "unix" };
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        if case.platform == platform {
            assert!(ids.insert(case.id.clone()), "{} duplicate case", case.id);
            check_case(case);
        }
    }
    assert!(!ids.is_empty(), "{platform} corpus is empty");
}
