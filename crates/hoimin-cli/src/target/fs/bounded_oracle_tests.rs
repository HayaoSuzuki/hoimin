use super::{FsTargetError, discover_python_bounded};
use camino::Utf8PathBuf;
use hoimin_core::Selection;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Ok { paths: Vec<Utf8PathBuf> },
    UnsupportedPath { path: String },
    Walk,
    AnalysisLimit,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Observation {
    polls: usize,
    #[serde(flatten)]
    outcome: Outcome,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    unix_only: bool,
    root_available: bool,
    files: Vec<Utf8PathBuf>,
    ignored: Vec<String>,
    includes: Vec<String>,
    limit: usize,
    cancel_at: Option<usize>,
    expected: Observation,
}

#[test]
fn bounded_file_discovery_matches_lean() {
    let corpus =
        include_str!("../../../../../formal/HoiminOracle/corpus/bounded-file-discovery.jsonl");
    let mut ids = BTreeSet::new();
    let mut skipped = 0;
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        if case.unix_only && !cfg!(unix) {
            skipped += 1;
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let mut root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        if !case.root_available {
            root.push("absent");
        }
        for path in case.files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"value = 1\n").unwrap();
        }
        if !case.ignored.is_empty() {
            fs::write(root.join(".gitignore"), case.ignored.join("\n") + "\n").unwrap();
        }
        let selection = Selection {
            root,
            includes: case.includes,
            ..Selection::default()
        };
        let polls = Cell::new(0);
        let cancelled = || {
            let poll = polls.get() + 1;
            polls.set(poll);
            case.cancel_at.is_some_and(|stop| poll >= stop)
        };
        let outcome = match discover_python_bounded(&selection, case.limit, &cancelled) {
            Ok(files) => {
                assert!(files.iter().all(|file| file.is_python), "{}", case.id);
                Outcome::Ok {
                    paths: files.into_iter().map(|file| file.path).collect(),
                }
            }
            Err(FsTargetError::UnsupportedPath(path)) => Outcome::UnsupportedPath { path },
            Err(FsTargetError::Walk(_)) => Outcome::Walk,
            Err(FsTargetError::AnalysisLimit) => Outcome::AnalysisLimit,
            Err(error) => panic!("{}: unexpected discovery error: {error}", case.id),
        };
        assert_eq!(
            Observation {
                polls: polls.get(),
                outcome,
            },
            case.expected,
            "{}",
            case.id,
        );
    }
    assert_eq!(ids.len(), 240);
    assert_eq!(skipped, if cfg!(unix) { 0 } else { 48 });
}
