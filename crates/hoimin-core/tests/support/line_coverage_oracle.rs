use super::*;
use camino::Utf8Path;
use hoimin_core::{LineSelectionIndex, normalize_changed};
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct QueryObservation {
    covers: bool,
    contains_start: bool,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Observation {
    queries: Vec<QueryObservation>,
    normalized_changes: Vec<LineSelection>,
    covered_paths: Vec<String>,
}

#[derive(serde::Deserialize)]
struct Target {
    path: Utf8PathBuf,
    lines: Vec<LineRange>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    selected: Vec<LineSelection>,
    queries: Vec<LineSelection>,
    targets: Vec<Target>,
    expected: Observation,
}

fn replay(fixture: Fixture) {
    let index = LineSelectionIndex::new(Utf8Path::new("project"), &fixture.selected);
    let queries = fixture
        .queries
        .iter()
        .map(|query| QueryObservation {
            covers: index.covers(&query.path, query.range),
            contains_start: index.contains(&query.path, query.range.start),
        })
        .collect();
    let targets: Vec<_> = fixture
        .targets
        .into_iter()
        .map(|target| TargetSlice {
            path: target.path,
            lines: target.lines,
            symbols: Vec::new(),
        })
        .collect();
    let mut covered_paths: Vec<_> = index.covered_target_paths(&targets).into_iter().collect();
    covered_paths.sort_unstable();

    let mut ranges: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for selected in fixture.selected {
        ranges
            .entry(selected.path)
            .or_default()
            .push(selected.range);
    }
    let normalized_changes = normalize_changed(ranges)
        .into_iter()
        .flat_map(|(path, ranges)| {
            ranges.into_iter().map(move |range| LineSelection {
                path: path.clone(),
                range,
            })
        })
        .collect();

    let actual = Observation {
        queries,
        normalized_changes,
        covered_paths,
    };
    assert_eq!(actual, fixture.expected, "{}", fixture.id);
}

#[test]
fn line_selection_coverage_matches_lean() {
    let mut ids = BTreeSet::new();
    for line in include_str!("../../../../formal/HoiminOracle/corpus/line-coverage.jsonl").lines() {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "strict", "{}", fixture.id);
        assert!(ids.insert(fixture.id.clone()));
        replay(fixture);
    }
    assert_eq!(ids.len(), 268);
}
