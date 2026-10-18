use super::{
    PLAN_SCHEMA_VERSION, PlanManifest, RankedPlanCandidate, RankingReason, RankingReasonCode,
};
use hoimin_core::{
    ByteSpan, CandidateDescriptor, CommandArg, EnvironmentSnapshot, FingerprintInputFile,
    MutationCandidate, RawRunConfig, RunConfig,
};

pub fn manifest(candidate_count: usize, payload_bytes: usize) -> PlanManifest {
    let normalized_config = RunConfig::try_from(RawRunConfig {
        files: vec!["sample.py".into()],
        test_argv: vec![CommandArg::Unix(b"test".to_vec())],
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(EnvironmentSnapshot::Untracked)
    .unwrap()
    .into_plan_config(
        hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
    );
    let original = format!("['{}']", "x".repeat(payload_bytes));
    let replacement = format!("('{}',)", "x".repeat(payload_bytes));
    let source = format!("{original}\n").repeat(candidate_count);
    let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
    let candidates = (0..candidate_count)
        .map(|index| {
            let mut candidate = MutationCandidate {
                id: String::new(),
                sequence: u64::try_from(index + 1).unwrap(),
                path: "sample.py".into(),
                span: ByteSpan {
                    start: u64::try_from(index * (original.len() + 1)).unwrap(),
                    length: u64::try_from(original.len()).unwrap(),
                },
                original: original.clone(),
                replacement: replacement.clone(),
                operator: "collection_list_tuple".into(),
                line: u32::try_from(index + 1).unwrap(),
                column: 0,
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
                score: 80,
                ranking_reasons: vec![RankingReason {
                    code: RankingReasonCode::Behavioral,
                    score: 80,
                }],
            }
        })
        .collect();
    PlanManifest {
        schema_version: PLAN_SCHEMA_VERSION,
        kind: "plan".into(),
        ranking_rule_version: 4,
        normalized_config,
        sources: vec![FingerprintInputFile {
            path: "sample.py".into(),
            hash,
        }],
        candidates,
        truncated: false,
        diagnostics: vec![],
    }
}
