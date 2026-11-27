use super::collect_current_worktree_paths;
use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{LineRange, TargetError, TargetSlice};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

type Changes = BTreeMap<Utf8PathBuf, Vec<LineRange>>;

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Entry {
    File { bytes: Vec<u8> },
    Directory,
    BlockedParent,
}

#[derive(serde::Deserialize)]
struct FileFixture {
    path: Utf8PathBuf,
    #[serde(flatten)]
    entry: Entry,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Diagnostic {
    InvalidUtf8,
    LiteralBackslash { path: String },
    RootUnavailable,
    ReadParent { path: String },
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Observation {
    changed: Changes,
    failure: Option<Diagnostic>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    unix_only: bool,
    root_available: bool,
    inventory: Vec<u8>,
    scope: Option<Vec<TargetSlice>>,
    files: Vec<FileFixture>,
    initial: Changes,
    expected: Observation,
}

fn prepare(root: &Utf8Path, files: Vec<FileFixture>) {
    for file in files {
        let path = root.join(file.path);
        match file.entry {
            Entry::File { bytes } => {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            Entry::Directory => fs::create_dir_all(path).unwrap(),
            Entry::BlockedParent => {
                let parent = path.parent().unwrap();
                fs::create_dir_all(parent.parent().unwrap()).unwrap();
                fs::write(parent, b"regular file").unwrap();
            }
        }
    }
}

fn diagnostic(error: TargetError, root: &Utf8Path, id: &str) -> Diagnostic {
    let TargetError::GitFailed(message) = error else {
        panic!("{id}: unexpected error: {error}");
    };
    if message == "Git path is not valid UTF-8" {
        Diagnostic::InvalidUtf8
    } else if let Some(path) =
        message.strip_prefix("path contains an unsupported literal backslash: ")
    {
        Diagnostic::LiteralBackslash { path: path.into() }
    } else if message.starts_with(&format!("open worker root failed for {root}: ")) {
        Diagnostic::RootUnavailable
    } else if let Some((path, _)) = message
        .strip_prefix("open worker parent failed for ")
        .and_then(|suffix| suffix.split_once(": "))
    {
        Diagnostic::ReadParent { path: path.into() }
    } else {
        panic!("{id}: unexpected Git diagnostic: {message}");
    }
}

#[tokio::test]
async fn git_worktree_collection_matches_lean() {
    let corpus = include_str!("../../../../../formal/HoiminOracle/corpus/git-worktree.jsonl");
    let mut ids = BTreeSet::new();
    let mut skipped = 0;
    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        if fixture.unix_only && !cfg!(unix) {
            skipped += 1;
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let directory = Utf8Path::from_path(temp.path()).unwrap();
        let root = if fixture.root_available {
            prepare(directory, fixture.files);
            directory.to_owned()
        } else {
            directory.join("absent")
        };
        let mut changed = fixture.initial;
        let failure = collect_current_worktree_paths(
            &root,
            &fixture.inventory,
            &mut changed,
            fixture.scope.as_deref(),
        )
        .await
        .err()
        .map(|error| diagnostic(error, &root, &fixture.id));
        assert_eq!(
            Observation { changed, failure },
            fixture.expected,
            "{}",
            fixture.id
        );
    }
    assert_eq!(ids.len(), 134);
    assert_eq!(skipped, if cfg!(unix) { 0 } else { 3 });
}
