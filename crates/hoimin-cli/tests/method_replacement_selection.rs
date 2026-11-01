use camino::Utf8Path;
use hoimin_cli::analyzer::discover_targets;
use hoimin_core::{
    MutationCandidate, MutationOperator, MutationOperatorSelection, MutationProfile, TargetSlice,
};

async fn assert_method_prefix(limit: usize) {
    // The baseline uses LF bytes for spans, file hashes, and candidate IDs.
    // Undo checkout CRLF conversion before writing the source under test.
    let source = include_str!("fixtures/method-replacements/subject.py").replace("\r\n", "\n");
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
    assert_ne!(baseline, Vec::new());

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

#[rstest::rstest]
#[case::limit_0(0)]
#[case::limit_1(1)]
#[case::limit_2(2)]
#[case::limit_3(3)]
#[case::limit_4(4)]
#[case::limit_5(5)]
#[case::limit_6(6)]
#[case::limit_7(7)]
#[case::limit_8(8)]
#[case::limit_9(9)]
#[case::limit_10(10)]
#[case::limit_11(11)]
#[case::limit_12(12)]
#[case::limit_13(13)]
#[case::limit_14(14)]
#[case::limit_15(15)]
#[tokio::test]
async fn method_descriptors_ids_order_and_truncation_match_baseline(#[case] limit: usize) {
    assert_method_prefix(limit).await;
}

proptest::proptest! {
    #[test]
    fn generated_limits_preserve_method_descriptor_prefixes(limit in 0usize..60) {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        runtime.block_on(assert_method_prefix(limit));
    }
}
