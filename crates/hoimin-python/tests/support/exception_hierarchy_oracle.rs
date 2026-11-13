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
    parse_cases(CORPUS)
}

fn parse_cases(input: &str) -> Vec<Case> {
    let mut rows: Vec<Case> = input
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
                assert_eq!(case.expected.pairs, Vec::<(String, String)>::new());
                assert!(case.expected.error.is_none() && case.expected.truncated.is_none());
                assert!(
                    case.expected.load.is_none() && case.expected.fingerprint_matches.is_none()
                );
            }
            "internal-fixture" => {
                assert!(case.expected_alias_facts.is_none());
                assert_eq!(case.kind, "snapshot");
                assert_ne!(case.changed_source, "");
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

// Candidate ranking is a separate contract: compare the complete multiset without
// dropping duplicates. This also supports a minimal single-case reproduction.
pub fn selected(mode: &str) -> Vec<Case> {
    selected_kind(mode, None)
}

pub fn selected_kind(mode: &str, kind: Option<&str>) -> Vec<Case> {
    let filter = std::env::var("HOIMIN_ORACLE_CASE").ok();
    filtered_cases(mode, kind, filter.as_deref())
}

fn filtered_cases(mode: &str, kind: Option<&str>, filter: Option<&str>) -> Vec<Case> {
    let selected: Vec<_> = cases()
        .into_iter()
        .filter(|case| case.mode == mode && filter.is_none_or(|id| id == case.id))
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

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn comparison_sorts_observations_and_preserves_duplicate_pairs() {
        let mut case = cases().remove(0);
        case.expected = Observation {
            pairs: vec![
                ("A".into(), "B".into()),
                ("A".into(), "B".into()),
                ("C".into(), "D".into()),
            ],
            error: Some(false),
            truncated: Some(false),
            load: None,
            fingerprint_matches: None,
        };
        let actual = Observation {
            pairs: vec![
                ("C".into(), "D".into()),
                ("A".into(), "B".into()),
                ("A".into(), "B".into()),
            ],
            error: Some(false),
            truncated: Some(false),
            load: None,
            fingerprint_matches: None,
        };
        let mut mismatches = Vec::new();
        compare(&case, actual, &mut mismatches);
        assert_eq!(mismatches, Vec::<String>::new());
    }

    #[test]
    fn comparison_reports_missing_duplicate_pairs() {
        let mut case = cases().remove(0);
        case.expected = Observation {
            pairs: vec![
                ("A".into(), "B".into()),
                ("A".into(), "B".into()),
                ("C".into(), "D".into()),
            ],
            error: Some(false),
            truncated: Some(false),
            load: None,
            fingerprint_matches: None,
        };
        let mut mismatches = Vec::new();
        let actual = Observation {
            pairs: vec![("A".into(), "B".into()), ("C".into(), "D".into())],
            error: Some(false),
            truncated: Some(false),
            load: None,
            fingerprint_matches: None,
        };
        compare(&case, actual, &mut mismatches);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].contains(&format!("mode={} case={}", case.mode, case.id)));
        assert!(mismatches[0].contains("expected="));
        assert!(mismatches[0].contains("actual="));
    }

    #[test]
    fn source_writer_materializes_nested_valid_python_fixtures() {
        let mut case = cases().remove(0);
        case.files = vec![
            (
                "service.py".into(),
                "from package.errors import Failure\n".into(),
            ),
            (
                "package/errors.py".into(),
                "class Failure(Exception): pass\n".into(),
            ),
        ];
        let root = tempfile::tempdir().unwrap();
        write_sources(&case, root.path());
        assert_eq!(
            case.files
                .iter()
                .map(|(path, _)| std::fs::read_to_string(root.path().join(path)).unwrap())
                .collect::<Vec<_>>(),
            case.files
                .iter()
                .map(|(_, source)| source.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    #[should_panic(expected = "invalid corpus Python")]
    fn source_writer_rejects_invalid_python_before_writing_it() {
        let mut case = cases().remove(0);
        case.files = vec![("service.py".into(), "class (".into())];
        let root = tempfile::tempdir().unwrap();
        write_sources(&case, root.path());
    }
    #[test]
    fn selected_wrapper_preserves_the_requested_mode() {
        let rows = selected("strict");
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|case| case.mode == "strict"));
        assert!(rows.iter().all(|case| case.expected_alias_facts.is_none()));
    }
    #[test]
    fn single_case_filter_selects_the_requested_identity() {
        let id = cases()
            .into_iter()
            .find(|case| case.mode == "strict")
            .unwrap()
            .id;
        let selected = filtered_cases("strict", None, Some(&id));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, id);
    }

    #[test]
    #[should_panic(expected = "no selected strict cases")]
    fn single_case_filter_rejects_unknown_identity() {
        filtered_cases("strict", None, Some("unknown-fixture"));
    }
    #[test]
    #[should_panic(expected = "unsupported correspondence mode")]
    fn corpus_validator_rejects_unknown_correspondence_mode() {
        let mut rows = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
        let mut row: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
        row["mode"] = serde_json::json!("unsupported");
        rows[0] = row.to_string();
        parse_cases(&rows.join("\n"));
    }
}
