use std::collections::BTreeSet;

use camino::Utf8PathBuf;
use hoimin_core::{
    DiscoveredFile, Selection, SymbolSelection, TargetError, TargetSlice, resolve_explicit,
};

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Ok { targets: Vec<TargetSlice> },
    MissingModule { module: String },
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    sources: Vec<Utf8PathBuf>,
    files: Vec<DiscoveredFile>,
    selectors: Vec<SymbolSelection>,
    expected: Observation,
}

#[test]
fn symbol_module_resolution_matches_lean() {
    let corpus = include_str!("../../../../formal/HoiminOracle/corpus/symbol-resolution.jsonl");
    let mut ids = BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        let selection = Selection {
            root: "project".into(),
            sources: case.sources,
            symbols: case.selectors,
            ..Selection::default()
        };
        let observed = match resolve_explicit(&selection, &case.files) {
            Ok(targets) => Observation::Ok { targets },
            Err(TargetError::SymbolModuleNotFound(module)) => Observation::MissingModule { module },
            Err(error) => panic!("{}: unexpected symbol target error: {error}", case.id),
        };
        assert_eq!(observed, case.expected, "{}", case.id);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 172);
}
