use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

const CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/exception-hierarchy.jsonl");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub schema: u64,
    pub id: String,
    pub mode: String,
    pub kind: String,
    pub files: Vec<(String, String)>,
    pub roots: Vec<String>,
    pub actions: Vec<String>,
    pub changed_source: String,
    pub max_candidates: usize,
    pub expected: Observation,
    pub expected_alias_facts: Option<AliasFacts>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AliasFacts {
    pub imports: Vec<((usize, String), String, u32)>,
    pub assignments: Vec<((usize, String), (usize, String))>,
    pub writes: Vec<(usize, String)>,
    pub affected: Vec<(usize, String)>,
}

impl AliasFacts {
    pub fn sort(&mut self) {
        self.imports.sort();
        self.assignments.sort();
        self.writes.sort();
        self.affected.sort();
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub pairs: Vec<(String, String)>,
    #[serde(deserialize_with = "required_option")]
    pub error: Option<bool>,
    #[serde(deserialize_with = "required_option")]
    pub truncated: Option<bool>,
    #[serde(deserialize_with = "required_option")]
    pub load: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub fingerprint_matches: Option<bool>,
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

pub fn cases() -> Vec<Case> {
    let mut rows: Vec<Case> = CORPUS
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut ids = BTreeSet::new();
    for case in &mut rows {
        assert_eq!(case.schema, 2);
        assert!(ids.insert(&case.id) && !case.id.is_empty());
        match case.mode.as_str() {
            "strict" => {
                assert!(case.expected_alias_facts.is_none());
                assert!(matches!(case.kind.as_str(), "candidate" | "resource"));
                assert!(case.actions.is_empty() && case.changed_source.is_empty());
                assert!(case.expected.error.is_some());
                assert!(
                    case.expected.load.is_none() && case.expected.fingerprint_matches.is_none()
                );
            }
            "internal-fixture" if case.kind == "alias-extraction" => {
                let facts = case.expected_alias_facts.as_mut().expect("alias facts");
                facts.sort();
                assert!(case.actions.is_empty() && case.changed_source.is_empty());
                assert!(case.expected.pairs.is_empty());
                assert!(case.expected.error.is_none() && case.expected.truncated.is_none());
                assert!(
                    case.expected.load.is_none() && case.expected.fingerprint_matches.is_none()
                );
            }
            "internal-fixture" => {
                assert!(case.expected_alias_facts.is_none());
                assert_eq!(case.kind, "snapshot");
                assert!(!case.changed_source.is_empty());
                assert!(case.expected.error.is_none() && case.expected.truncated.is_none());
                assert!(matches!(
                    case.expected.load.as_deref(),
                    Some("never" | "ok" | "error")
                ));
                assert!(case.expected.fingerprint_matches.is_some());
            }
            mode => panic!("unsupported correspondence mode: {mode}"),
        }
        assert!(
            case.actions
                .iter()
                .all(|a| matches!(a.as_str(), "change" | "delete" | "restore" | "build"))
        );
        assert!(case.max_candidates > 0);
        let mut paths = BTreeSet::new();
        for (path, _) in &case.files {
            assert!(paths.insert(path));
            assert!(
                Path::new(path)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
            );
        }
        assert!(case.files.iter().any(|(path, _)| path == "service.py"));
    }
    assert_eq!(rows.iter().filter(|r| r.mode == "strict").count(), 160);
    assert_eq!(
        rows.iter().filter(|r| r.mode == "internal-fixture").count(),
        215
    );
    rows
}

pub fn write_sources(case: &Case, root: &Path) {
    for (path, source) in &case.files {
        ruff_python_parser::parse_module(source)
            .unwrap_or_else(|error| panic!("invalid corpus Python {} {path}: {error}", case.id));
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}

pub fn config<E: std::fmt::Debug>(
    case: &Case,
    root: &Path,
    parse_config: impl FnOnce(Vec<String>) -> Result<hoimin_core::RunConfig, E>,
) -> hoimin_core::RunConfig {
    let limit = case.max_candidates.to_string();
    let mut args = vec![
        "hoimin",
        "run",
        "--root",
        root.to_str().unwrap(),
        "--file",
        "service.py",
        "--operators",
        "exception_hierarchy",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
        "--max-candidates",
        &limit,
    ];
    for import_root in &case.roots {
        args.extend(["--import-root", import_root]);
    }
    args.extend(["--", "unused-test-command"]);
    // Both adapters use the real CLI configuration parser.
    parse_config(args.into_iter().map(str::to_owned).collect()).unwrap()
}

// Candidate ranking is a separate contract: compare the complete multiset without
// dropping duplicates. This also supports a minimal single-case reproduction.
pub fn selected(mode: &str) -> Vec<Case> {
    selected_kind(mode, None)
}

pub fn selected_kind(mode: &str, kind: Option<&str>) -> Vec<Case> {
    let filter = std::env::var("HOIMIN_ORACLE_CASE").ok();
    let selected: Vec<_> = cases()
        .into_iter()
        .filter(|case| case.mode == mode && filter.as_ref().is_none_or(|id| id == &case.id))
        .filter(|case| kind.is_none_or(|kind| case.kind == kind))
        .collect();
    assert!(!selected.is_empty(), "no selected {mode} cases: {filter:?}");
    selected
}

pub fn compare(case: &Case, mut actual: Observation, mismatches: &mut Vec<String>) {
    actual.pairs.sort();
    if actual != case.expected {
        mismatches.push(format!(
            "mode={} case={} expected={:?} actual={actual:?}",
            case.mode, case.id, case.expected
        ));
    }
}
