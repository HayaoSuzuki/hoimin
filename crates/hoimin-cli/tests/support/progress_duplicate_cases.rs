use std::{collections::BTreeSet, path::PathBuf};

#[derive(serde::Deserialize)]
pub struct Case {
    schema: u32,
    pub id: String,
    pub mode: Mode,
    pub initial: Vec<Option<u8>>,
    pub actions: Vec<Action>,
    pub expected: Vec<Option<Duplicate>>,
    pub history: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Strict,
    InternalFixture,
}

#[derive(serde::Deserialize)]
pub struct Action {
    pub path: usize,
    pub fingerprint: u8,
    pub edit: Option<Edit>,
}

#[derive(serde::Deserialize)]
pub struct Edit {
    pub path: usize,
    pub bytes: Option<u8>,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
pub struct Duplicate {
    pub previous: usize,
    pub reason: Reason,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    SamePath,
    IdenticalBytes,
}

pub fn read_cases() -> Vec<Case> {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/progress-duplicates.jsonl"
    ));
    let mut ids = BTreeSet::new();
    corpus
        .lines()
        .map(|row| {
            let case: Case = serde_json::from_str(row).unwrap();
            let id = &case.id;
            assert_eq!(case.schema, 1, "{id}");
            assert!(ids.insert(case.id.clone()), "{id}");
            assert_eq!(case.actions.len(), case.expected.len(), "{id}");
            assert_eq!(case.history.is_some(), case.mode == Mode::Strict, "{id}");
            for action in &case.actions {
                assert!(action.path < case.initial.len(), "{id}");
                if let Some(edit) = &action.edit {
                    assert_eq!(case.mode, Mode::InternalFixture, "{id}");
                    assert!(edit.path < case.initial.len(), "{id}");
                    assert!(edit.bytes.is_none_or(|label| label <= 2), "{id}");
                }
                if case.mode == Mode::Strict {
                    assert_eq!(case.initial[action.path], Some(action.fingerprint), "{id}");
                }
            }
            case
        })
        .collect()
}

pub fn paths(directory: &tempfile::TempDir, case: &Case) -> Vec<PathBuf> {
    (0..case.initial.len())
        .map(|index| directory.path().join(format!("report-{index}.json")))
        .collect()
}
