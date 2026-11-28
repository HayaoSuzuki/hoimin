use std::{collections::BTreeSet, fs, num::NonZeroUsize, time::Duration};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    DiscoveryConfig, LineRange, LineSelection, MutationCandidate, MutationProfile, OutputConfig,
    RunConfig,
};

use super::{
    PlanManifest, PlanOutput, TargetHandler, create, discover_plan_targets,
    requested_discovery_targets, tests::Project, verify_oracle_tests::preview_candidate_ids,
};

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    id: String,
    counts: Vec<u32>,
    limit: NonZeroUsize,
    requested: Vec<Position>,
    expected: Expected,
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct Position {
    path: Utf8PathBuf,
    line: u32,
}

#[derive(serde::Deserialize)]
struct Expected {
    plan: Discovery,
    exit_code: i32,
    rediscovery: Option<Discovery>,
    confirmed: Vec<Position>,
}

#[derive(serde::Deserialize)]
struct Discovery {
    positions: Vec<Position>,
    truncated: bool,
    overflow_path: Option<Utf8PathBuf>,
}

async fn create_fixture_plan(project: &Project, case: &Case) -> PlanOutput {
    let mut data = project.config().into_data();
    data.selection.files.clear();
    data.operators = serde_json::from_str(r#"["binary_add_sub"]"#).unwrap();
    data.profile = MutationProfile::Full;
    data.limits.max_candidates = case.limit;
    for (index, count) in case.counts.iter().copied().enumerate() {
        let path = Utf8PathBuf::from(format!("src/file-{index}.py"));
        let selected = if count == 0 {
            "pass\n".to_owned()
        } else {
            "x = 1 + 2\n".repeat(usize::try_from(count).unwrap())
        };
        // This final mutation is outside the saved line slice.
        fs::write(project.root.join(&path), selected + "outside = 1 + 2\n").unwrap();
        data.selection.lines.push(LineSelection {
            path,
            range: LineRange {
                start: 1,
                end: count.max(1),
            },
        });
    }
    create(
        RunConfig::try_from(data).unwrap(),
        DiscoveryConfig::new(Duration::from_secs(30)).unwrap(),
    )
    .await
    .expect(&case.id)
}

fn assert_candidates(candidates: &[&MutationCandidate], expected: &Discovery, case: &str) {
    assert_eq!(candidates.len(), expected.positions.len(), "{case}");
    for ((candidate, position), sequence) in candidates.iter().zip(&expected.positions).zip(1..) {
        assert_eq!(candidate.path, position.path, "{case}");
        assert_eq!(candidate.line, position.line, "{case}");
        assert_eq!(candidate.sequence, sequence, "{case}");
        assert_eq!(candidate.operator, "binary_add_sub", "{case}");
        assert_eq!(candidate.original, "+", "{case}");
        assert_eq!(candidate.replacement, "-", "{case}");
    }
}

fn expected_diagnostics(expected: &Discovery) -> Vec<(&str, Option<&str>)> {
    expected
        .overflow_path
        .iter()
        .map(|path| ("candidate_limit", Some(path.as_str())))
        .collect()
}

fn assert_plan(output: &PlanOutput, case: &Case) {
    let expected = &case.expected.plan;
    assert_eq!(output.exit_code, case.expected.exit_code, "{}", case.id);
    assert_eq!(output.manifest.truncated, expected.truncated, "{}", case.id);
    let mut by_sequence: Vec<_> = output
        .manifest
        .candidates
        .iter()
        .map(|row| &row.candidate)
        .collect();
    by_sequence.sort_by_key(|candidate| candidate.sequence);
    assert_candidates(&by_sequence, expected, &case.id);
    let diagnostics: Vec<_> = output
        .manifest
        .diagnostics
        .iter()
        .map(|row| (row.code.as_str(), row.path.as_deref().map(Utf8Path::as_str)))
        .collect();
    assert_eq!(diagnostics, expected_diagnostics(expected), "{}", case.id);
}

fn stable_ids(manifest: &PlanManifest, positions: &[Position], case: &str) -> Vec<String> {
    positions
        .iter()
        .map(|position| {
            manifest
                .candidates
                .iter()
                .find(|candidate| {
                    candidate.path == position.path && candidate.line == position.line
                })
                .unwrap_or_else(|| {
                    panic!(
                        "{case}: no planned candidate at {}:{}",
                        position.path, position.line
                    )
                })
                .id
                .clone()
        })
        .collect()
}

async fn assert_rediscovery(manifest: &PlanManifest, case: &Case, expected: &Discovery) {
    let config = manifest
        .normalized_config
        .clone()
        .into_run_config(OutputConfig::default())
        .unwrap();
    let all = TargetHandler::resolve(&config.selection).await.unwrap();
    let selected: BTreeSet<_> = case.requested.iter().map(|row| row.path.clone()).collect();
    let filtered = requested_discovery_targets(&all, &selected);
    let actual = discover_plan_targets(
        &config,
        &manifest.normalized_config.discovery,
        &filtered,
        None,
    )
    .await
    .unwrap();
    assert_eq!(actual.truncated, expected.truncated, "{}", case.id);
    let candidates: Vec<_> = actual.candidates.iter().collect();
    assert_candidates(&candidates, expected, &case.id);
    let diagnostics: Vec<_> = actual
        .diagnostics
        .iter()
        .map(|row| {
            (
                row.code.canonical_name(),
                row.path.as_deref().map(Utf8Path::as_str),
            )
        })
        .collect();
    assert_eq!(diagnostics, expected_diagnostics(expected), "{}", case.id);
}

#[tokio::test]
async fn bounded_plan_rediscovery_matches_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/plan-discovery-budget.jsonl"
    ));
    let project = Project::new();
    let artifact = tempfile::NamedTempFile::new().unwrap();
    let mut ids = BTreeSet::new();
    let mut previews = 0;
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        let output = create_fixture_plan(&project, &case).await;
        assert_plan(&output, &case);
        if let Some(rediscovery) = &case.expected.rediscovery {
            assert_rediscovery(&output.manifest, &case, rediscovery).await;
            let mut requested = stable_ids(&output.manifest, &case.requested, &case.id);
            requested.reverse();
            let confirmed = preview_candidate_ids(&output.manifest, requested, &artifact)
                .await
                .unwrap_or_else(|error| panic!("{}: {error}", case.id));
            assert_eq!(
                confirmed,
                stable_ids(&output.manifest, &case.expected.confirmed, &case.id),
                "{}",
                case.id
            );
            previews += 1;
        } else {
            assert_eq!(case.requested, [], "{}", case.id);
            assert_eq!(case.expected.confirmed, [], "{}", case.id);
        }
    }
    assert_eq!(ids.len(), 143);
    assert_eq!(previews, 142);
}
