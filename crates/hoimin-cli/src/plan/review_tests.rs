use std::cell::RefCell;
use std::path::Path;

use hoimin_core::{ByteSpan, CandidateDescriptor, FingerprintInputFile, MutationCandidate};
use proptest::prelude::*;

use super::{
    PlanError, PlanManifest, RankedPlanCandidate, RankingReason, RankingReasonCode, read_for_review,
};

#[derive(Default, Debug)]
pub(super) struct SourceWork {
    pub source_visits: usize,
    pub membership_queries: usize,
    pub index_builds: usize,
}

thread_local! {
    pub(super) static SOURCE_WORK: RefCell<SourceWork> = RefCell::default();
}

fn manifest(
    project: &super::tests::Project,
    candidate_count: usize,
    unused_sources: usize,
) -> PlanManifest {
    let config = project
        .config()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap()
        .into_plan_config(
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
        );
    let hash = "1".repeat(64);
    let candidates = (0..candidate_count)
        .map(|index| {
            let mut candidate = MutationCandidate {
                id: String::new(),
                // Sequence differs from display order, which is determined by rank/line.
                sequence: u64::try_from(candidate_count - index).unwrap(),
                path: "src/z.py".into(),
                span: ByteSpan {
                    start: u64::try_from(index * 10 + 6).unwrap(),
                    length: 1,
                },
                original: "+".into(),
                replacement: "-".into(),
                operator: "binary_add_sub".into(),
                line: u32::try_from(index + 1).unwrap(),
                column: 6,
                symbol: None,
                file_hash: hash.clone(),
            };
            candidate.id = hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(
                &CandidateDescriptor::from(&candidate),
            ))
            .to_string();
            RankedPlanCandidate {
                candidate,
                rank: index + 1,
                score: 70,
                ranking_reasons: vec![RankingReason {
                    code: RankingReasonCode::Arithmetic,
                    score: 70,
                }],
            }
        })
        .collect();
    let mut sources: Vec<_> = (0..unused_sources)
        .map(|index| FingerprintInputFile {
            path: format!("src/a{index:04}.py").into(),
            hash: "0".repeat(64),
        })
        .collect();
    sources.push(FingerprintInputFile {
        path: "src/z.py".into(),
        hash,
    });
    PlanManifest {
        schema_version: super::PLAN_SCHEMA_VERSION,
        kind: "plan".into(),
        ranking_rule_version: super::RANKING_RULE_VERSION,
        normalized_config: config,
        sources,
        candidates,
        truncated: false,
        diagnostics: vec![],
    }
}

fn save(project: &super::tests::Project, manifest: &PlanManifest) -> std::path::PathBuf {
    let path = project.root.join("plan.json").into_std_path_buf();
    std::fs::write(&path, serde_json::to_vec(manifest).unwrap()).unwrap();
    path
}

fn error(path: &Path) -> String {
    match read_for_review(path) {
        Err(PlanError::ManifestInvalid(message)) => message,
        Err(other) => panic!("unexpected error category: {other}"),
        Ok(_) => panic!("invalid review plan was accepted"),
    }
}

#[test]
fn review_reader_visits_sources_once_for_many_candidates_sharing_a_late_source() {
    let project = super::tests::Project::new();
    let manifest = manifest(&project, 256, 256);
    let path = save(&project, &manifest);
    SOURCE_WORK.with_borrow_mut(|work| *work = SourceWork::default());

    let result = read_for_review(&path).unwrap();

    assert_eq!(result.manifest.candidates, manifest.candidates);
    SOURCE_WORK.with_borrow(|work| {
        assert_eq!(
            work.source_visits, 257,
            "source membership must not rescan per candidate"
        );
        assert_eq!(work.membership_queries, 256);
        assert_eq!(work.index_builds, 1);
    });
}

#[test]
fn review_reader_keeps_candidate_values_and_order_with_matching_source_first() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 8, 16);
    manifest.sources.rotate_right(1);
    let path = save(&project, &manifest);

    let result = read_for_review(&path).unwrap();

    assert_eq!(result.manifest.candidates, manifest.candidates);
    assert_eq!(result.manifest.candidates[0].sequence, 8);
    assert_eq!(result.manifest.candidates[7].sequence, 1);
}

#[test]
fn review_reader_rejects_missing_candidate_source_record() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 1, 1);
    manifest.sources.pop();
    let path = save(&project, &manifest);

    let message = error(&path);

    assert_eq!(
        message,
        "candidate source record is missing or inconsistent"
    );
}

#[test]
fn review_reader_rejects_mismatched_source_hash() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 1, 0);
    manifest.sources[0].hash = "2".repeat(64);
    let path = save(&project, &manifest);

    let message = error(&path);

    assert_eq!(
        message,
        "candidate source record is missing or inconsistent"
    );
}

#[test]
fn review_reader_rejects_duplicate_source_paths_even_with_a_different_hash() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 1, 0);
    manifest.sources.push(FingerprintInputFile {
        path: "src/z.py".into(),
        hash: "2".repeat(64),
    });
    let path = save(&project, &manifest);

    let message = error(&path);

    assert_eq!(message, "sources contains duplicate path: src/z.py");
}

#[test]
fn review_reader_rejects_candidate_id_mismatch() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 1, 0);
    manifest.candidates[0].candidate.id = format!("m1_{}", "0".repeat(64));
    let path = save(&project, &manifest);

    let message = error(&path);

    assert_eq!(message, "candidate id does not match its descriptor");
}

#[test]
fn review_reader_reports_id_before_missing_source_for_the_same_candidate() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 1, 0);
    manifest.candidates[0].candidate.id = format!("m1_{}", "0".repeat(64));
    manifest.sources.clear();
    let path = save(&project, &manifest);

    let message = error(&path);

    assert_eq!(message, "candidate id does not match its descriptor");
}

#[test]
fn review_reader_reports_earlier_source_error_before_later_id_error() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 2, 0);
    manifest.sources.clear();
    manifest.candidates[1].candidate.id = format!("m1_{}", "0".repeat(64));
    let path = save(&project, &manifest);

    let message = error(&path);

    assert_eq!(
        message,
        "candidate source record is missing or inconsistent"
    );
}

#[test]
fn review_reader_canonicalizes_project_and_plan_without_requiring_unreferenced_source_files() {
    let project = super::tests::Project::new();
    let mut manifest = manifest(&project, 1, 3);
    manifest.normalized_config.selection.root = project.root.join("src/..");
    let path = save(&project, &manifest);

    let result = read_for_review(&path).unwrap();

    assert_eq!(
        result.project.as_std_path(),
        std::fs::canonicalize(&project.root).unwrap()
    );
    assert_eq!(result.canonical_path, std::fs::canonicalize(&path).unwrap());
    assert_eq!(result.manifest.candidates, manifest.candidates);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(32))]

    #[test]
    fn review_reader_source_permutations_preserve_candidate_values_and_order(
        priorities in proptest::collection::vec(any::<u32>(), 2..33),
    ) {
        let project = super::tests::Project::new();
        let mut manifest = manifest(&project, 8, priorities.len() - 1);
        let mut permuted: Vec<_> = priorities.into_iter().zip(manifest.sources).collect();
        permuted.sort_by_key(|(priority, _)| *priority);
        manifest.sources = permuted.into_iter().map(|(_, source)| source).collect();
        let path = save(&project, &manifest);

        let result = read_for_review(&path).unwrap();

        prop_assert_eq!(result.manifest.candidates, manifest.candidates);
        prop_assert_eq!(result.manifest.sources, manifest.sources);
    }
}
