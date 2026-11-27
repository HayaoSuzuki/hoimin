use super::{DiscoveryScope, DiscoveryStats, FsTargetError, discover_explicit_with_stats};
use camino::Utf8PathBuf;
use hoimin_core::{DiscoveredFile, LineRange, LineSelection, Selection, SymbolSelection};
use std::collections::BTreeSet;
use std::fs;
use std::sync::atomic::Ordering;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Ok {
        files: Vec<DiscoveredFile>,
        visited: usize,
    },
    UnsupportedPath {
        path: String,
    },
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Observation {
    scoped: bool,
    #[serde(flatten)]
    outcome: Outcome,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    unix_only: bool,
    sources: Vec<Utf8PathBuf>,
    files: Vec<Utf8PathBuf>,
    lines: Vec<Utf8PathBuf>,
    has_symbol: bool,
    paths: Vec<Utf8PathBuf>,
    expected: Observation,
}

#[test]
fn discovery_scope_matches_lean() {
    let corpus = include_str!("../../../../../formal/HoiminOracle/corpus/discovery-scope.jsonl");
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
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        for path in case.paths {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"value = 1\n").unwrap();
        }
        let selection = Selection {
            root,
            sources: case.sources,
            files: case.files,
            lines: case
                .lines
                .into_iter()
                .map(|path| LineSelection {
                    path,
                    range: LineRange { start: 1, end: 1 },
                })
                .collect(),
            symbols: case
                .has_symbol
                .then(|| SymbolSelection {
                    module: "app".into(),
                    qualname: "compute".into(),
                })
                .into_iter()
                .collect(),
            ..Selection::default()
        };
        let stats = DiscoveryStats::default();
        let scoped = DiscoveryScope::new(&selection).is_some();
        let outcome = match discover_explicit_with_stats(&selection, &stats) {
            Ok(files) => Outcome::Ok {
                files,
                visited: stats.visited_entries.load(Ordering::Relaxed),
            },
            Err(FsTargetError::UnsupportedPath(path)) => Outcome::UnsupportedPath { path },
            Err(error) => panic!("{}: unexpected discovery error: {error}", case.id),
        };
        assert_eq!(
            Observation { scoped, outcome },
            case.expected,
            "{}",
            case.id
        );
    }
    assert_eq!(ids.len(), 81);
    assert_eq!(skipped, if cfg!(unix) { 0 } else { 5 });
}
