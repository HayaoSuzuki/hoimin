use camino::Utf8Path;
use hoimin_cli::analyzer::discover_targets;
use hoimin_core::{
    MutationCandidate, MutationOperator, MutationOperatorSelection, MutationProfile, TargetSlice,
};

#[tokio::test]
async fn method_descriptors_ids_order_and_truncation_match_baseline() {
    let source = include_str!("fixtures/method-replacements/subject.py");
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("subject.py"), source).unwrap();
    let root = Utf8Path::from_path(directory.path()).unwrap();
    let targets = [TargetSlice {
        path: "subject.py".into(),
        lines: vec![],
        symbols: vec![],
    }];
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(operator);
        }
    }
    for operator in [
        MutationOperator::CollectionAppendInsert,
        MutationOperator::StructureAppendExtend,
        MutationOperator::StructureMappingGetSubscript,
        MutationOperator::StructureSortReverse,
        MutationOperator::BinaryAddSub,
    ] {
        operators.include(operator);
    }
    // Captured at base 65865ea before selection guards; includes every public field.
    let baseline: Vec<MutationCandidate> =
        serde_json::from_str(include_str!("fixtures/method-replacements/candidates.json")).unwrap();
    assert!(!baseline.is_empty());
    for limit in 0..=baseline.len() + 1 {
        let output = discover_targets(root, &targets, &operators, MutationProfile::Full, limit)
            .await
            .unwrap();
        assert_eq!(
            output.candidates,
            baseline[..limit.min(baseline.len())],
            "limit {limit}"
        );
        assert_eq!(output.truncated, limit < baseline.len(), "limit {limit}");
    }
}
