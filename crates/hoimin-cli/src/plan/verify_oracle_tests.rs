use std::{collections::BTreeSet, fs, io::ErrorKind, time::Duration};

use hoimin_core::{
    DiscoveryConfig, LineRange, MutationProfile, OutputConfig, RunConfig, TargetSlice,
};

use crate::cli::VerifySelection;

use super::{
    PlanError, PlanManifest, RankedPlanCandidate, ValidationStats, create,
    prepare_verify_preview_selection, tests::Project, validate_requested_candidates,
};

const PATHS: [&str; 3] = ["src/calc.py", "src/other.py", "src/unread.py"];

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    id: String,
    live: bool,
    entries: Vec<Entry>,
    targets: Vec<Target>,
    requested: Vec<usize>,
    readable: Vec<usize>,
    expected: Observation,
}

#[derive(serde::Deserialize)]
struct Entry {
    id: usize,
    stored: Stored,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stored {
    Missing,
    Valid,
    Hash,
    Original,
    Stable,
    Symbol,
    Sequence,
}

#[derive(serde::Deserialize)]
struct Target {
    path: usize,
    discovery: Vec<usize>,
}

#[derive(serde::Deserialize)]
struct Observation {
    contexts: usize,
    result: Expected,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Expected {
    Confirmed { confirmed: Vec<usize> },
    Failure { id: usize, reason: Reason },
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reason {
    Unknown,
    Unselected,
    Unreadable,
    Hash,
    Original,
    Stable,
    Undiscovered,
    Descriptor,
}

impl Expected {
    fn with_ids(&self, ids: &[&str; 4]) -> Result<Vec<String>, String> {
        match self {
            Self::Confirmed { confirmed } => {
                Ok(confirmed.iter().map(|slot| ids[*slot].to_owned()).collect())
            }
            Self::Failure { id, reason } => {
                let candidate_id = ids[*id];
                let path = PATHS[usize::from(*id == 1)];
                let error = match reason {
                    Reason::Unknown => format!("candidate id is not in the plan: {candidate_id}"),
                    Reason::Unselected => format!("candidate target is not selected: {path}"),
                    Reason::Unreadable => format!("{path}: root-relative file was not found"),
                    Reason::Hash => "candidate file hash does not match the source".into(),
                    Reason::Original => {
                        "candidate original text does not match the source span".into()
                    }
                    Reason::Stable => format!("candidate stable id differs for {candidate_id}"),
                    Reason::Undiscovered => format!(
                        "candidate is not discoverable under the planned configuration: {candidate_id}"
                    ),
                    Reason::Descriptor => {
                        format!("candidate descriptor differs for {candidate_id}")
                    }
                };
                Err(PlanError::CandidateInvalid(error).to_string())
            }
        }
    }
}

fn set_sources(project: &Project, source: &str, readable: &[usize]) {
    for (slot, path) in PATHS[..2].iter().enumerate() {
        let file = project.root.join(path);
        if readable.contains(&slot) {
            fs::write(file, source).unwrap();
        } else if let Err(error) = fs::remove_file(file) {
            assert_eq!(error.kind(), ErrorKind::NotFound);
        }
    }
}

async fn seed_plan(project: &Project) -> PlanManifest {
    let mut data = project.config().into_data();
    data.selection.files.push(PATHS[1].into());
    data.operators = serde_json::from_str(r#"["binary_add_sub"]"#).unwrap();
    data.profile = MutationProfile::Full;
    create(
        RunConfig::try_from(data).unwrap(),
        DiscoveryConfig::new(Duration::from_secs(30)).unwrap(),
    )
    .await
    .unwrap()
    .manifest
}

fn interleaved_candidates(manifest: &PlanManifest) -> [&RankedPlanCandidate; 3] {
    let first_file: Vec<_> = manifest
        .candidates
        .iter()
        .filter(|row| row.path.as_str() == PATHS[0])
        .collect();
    let second_file: Vec<_> = manifest
        .candidates
        .iter()
        .filter(|row| row.path.as_str() == PATHS[1])
        .collect();
    for first in &first_file {
        for last in &first_file {
            if last.line <= first.line || last.id <= first.id {
                continue;
            }
            if let Some(middle) = second_file
                .iter()
                .find(|middle| first.id < middle.id && middle.id < last.id)
            {
                return [*first, *middle, *last];
            }
        }
    }
    panic!("the two source files must supply candidate IDs in alternating file order");
}

fn apply_entries(manifest: &mut PlanManifest, entries: &[Entry], ids: &[&str; 4]) {
    manifest.candidates.retain_mut(|row| {
        let Some(edit) = entries.iter().find(|entry| row.id == ids[entry.id]) else {
            return true;
        };
        match edit.stored {
            Stored::Missing => return false,
            Stored::Valid => {}
            Stored::Hash => row.candidate.file_hash = "0".repeat(64),
            Stored::Original => row.candidate.original = "?".into(),
            Stored::Stable => row.candidate.replacement = "*".into(),
            Stored::Symbol => row.candidate.symbol = Some("different_saved_symbol".into()),
            Stored::Sequence => row.candidate.sequence += 1000,
        }
        true
    });
}

fn target_slices(targets: &[Target], candidates: [&RankedPlanCandidate; 3]) -> Vec<TargetSlice> {
    targets
        .iter()
        .map(|target| {
            let lines = if target.discovery.is_empty() {
                vec![LineRange {
                    start: 999,
                    end: 999,
                }]
            } else {
                target
                    .discovery
                    .iter()
                    .map(|slot| {
                        let line = candidates[*slot].line;
                        LineRange {
                            start: line,
                            end: line,
                        }
                    })
                    .collect()
            };
            TargetSlice {
                path: PATHS[target.path].into(),
                lines,
                symbols: Vec::new(),
            }
        })
        .collect()
}

async fn public_result(
    manifest: &PlanManifest,
    requested: Vec<String>,
    artifact: &tempfile::NamedTempFile,
) -> Result<Vec<String>, PlanError> {
    fs::write(artifact.path(), serde_json::to_vec(manifest).unwrap()).unwrap();
    let preview = prepare_verify_preview_selection(
        artifact.path(),
        &VerifySelection::CandidateIds(requested),
    )
    .await?;
    Ok(serde_json::to_value(preview).unwrap()["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["id"].as_str().unwrap().to_owned())
        .collect())
}

#[tokio::test]
async fn planned_candidate_verification_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/plan-verify.jsonl"
    ));
    let project = Project::new();
    let source = "x = 1 + 2\n".repeat(16);
    set_sources(&project, &source, &[0, 1]);
    let base = seed_plan(&project).await;
    let candidates = interleaved_candidates(&base);
    let missing = format!("m1_{}", "f".repeat(64));
    let ids = [
        candidates[0].id.as_str(),
        candidates[1].id.as_str(),
        candidates[2].id.as_str(),
        &missing,
    ];
    let config = base
        .normalized_config
        .clone()
        .into_run_config(OutputConfig::default())
        .unwrap();
    let artifact = tempfile::NamedTempFile::new().unwrap();
    let mut cases = BTreeSet::new();
    let mut public = 0;

    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert!(cases.insert(case.id.clone()), "{}", case.id);
        set_sources(&project, &source, &case.readable);
        let mut manifest = base.clone();
        apply_entries(&mut manifest, &case.entries, &ids);
        let requested = case.requested.iter().map(|slot| ids[*slot].to_owned());

        let actual = if case.live {
            public += 1;
            public_result(&manifest, requested.collect(), &artifact).await
        } else {
            let mut stats = ValidationStats::default();
            let actual = validate_requested_candidates(
                &manifest,
                &requested.collect(),
                &config,
                &target_slices(&case.targets, candidates),
                None,
                Some(&mut stats),
            )
            .await;
            assert_eq!(stats.contexts, case.expected.contexts, "{}", case.id);
            actual
        };
        assert_eq!(
            actual.map_err(|error| error.to_string()),
            case.expected.result.with_ids(&ids),
            "{}",
            case.id
        );
    }
    assert_eq!(cases.len(), 55);
    assert_eq!(public, 5);
}
