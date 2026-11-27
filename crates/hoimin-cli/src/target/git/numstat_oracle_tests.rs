use super::{GitChangedRanges, GitPathScope, parse_binary_numstat};
use camino::Utf8PathBuf;
use hoimin_core::{LineRange, TargetError, TargetSlice};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Ok {
        excluded: BTreeSet<Utf8PathBuf>,
        surviving: BTreeMap<Utf8PathBuf, Vec<LineRange>>,
    },
    Error {
        message: String,
    },
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    scope: Option<Vec<Utf8PathBuf>>,
    input: Vec<u8>,
    seed: BTreeSet<Utf8PathBuf>,
    changed: BTreeMap<Utf8PathBuf, Vec<LineRange>>,
    expected: Observation,
}

#[test]
fn git_numstat_exclusions_match_lean() {
    let mut ids = BTreeSet::new();
    let corpus = include_str!("../../../../../formal/HoiminOracle/corpus/git-numstat.jsonl");
    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);

        let targets = fixture.scope.map(|paths| {
            paths
                .into_iter()
                .map(|path| TargetSlice {
                    path,
                    lines: Vec::new(),
                    symbols: Vec::new(),
                })
                .collect::<Vec<_>>()
        });
        let scope = GitPathScope::new(targets.as_deref());
        let mut excluded = fixture.seed;
        let actual = match parse_binary_numstat(&fixture.input, &mut excluded, &scope) {
            Ok(()) => {
                let GitChangedRanges(surviving) =
                    GitChangedRanges::from_patch(fixture.changed, &excluded);
                Observation::Ok {
                    excluded,
                    surviving,
                }
            }
            Err(TargetError::GitFailed(message)) => Observation::Error { message },
            Err(error) => panic!("{}: unexpected error: {error}", fixture.id),
        };
        assert_eq!(actual, fixture.expected, "{}", fixture.id);
        assert!(ids.insert(fixture.id));
    }
    assert_eq!(ids.len(), 419);
}
