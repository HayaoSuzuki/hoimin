use super::replay_runtime_candidates;
use camino::Utf8PathBuf;
use hoimin_core::{
    ByteSpan, CandidateDescriptor, CandidateValidationContext, CandidateValidationError,
    MutationOperator, MutationOperatorSelection, MutationProfile, TargetSlice,
    validate_candidate_with_context,
};

#[derive(serde::Deserialize)]
struct Expected {
    raw_start: u64,
    decoded_start: usize,
    length: u64,
    line: u32,
    column: u32,
    original: String,
    replacement: String,
    symbol: Option<String>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    raw: Vec<u8>,
    candidates: Vec<Expected>,
}

#[tokio::test]
async fn recorded_candidate_locations_match_lean_across_source_encodings() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/candidate-locations.jsonl"
    ));
    let operator = MutationOperator::BooleanLiteral.as_str();
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([operator])).unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        assert_eq!(case.candidates.len(), 2, "{}", case.id);
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();
        let path = "locations.py";
        std::fs::write(root.join(path), &case.raw).unwrap();
        let target = TargetSlice {
            path: path.into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        };
        let candidates =
            replay_runtime_candidates(root, vec![target], operators.clone(), MutationProfile::Full)
                .await;
        assert_eq!(candidates.len(), case.candidates.len(), "{}", case.id);
        let context = CandidateValidationContext::new(&case.raw).unwrap();
        let decoded = context.decoded_source().unwrap();
        for (candidate, expected) in candidates.iter().zip(case.candidates) {
            assert_eq!(candidate.operator, operator, "{}", case.id);
            assert_eq!(
                candidate.span,
                ByteSpan {
                    start: expected.raw_start,
                    length: expected.length
                },
                "{}",
                case.id
            );
            assert_eq!(
                decoded.raw_to_utf8(usize::try_from(candidate.span.start).unwrap()),
                Some(expected.decoded_start),
                "{}",
                case.id
            );
            assert_eq!(
                (candidate.line, candidate.column),
                (expected.line, expected.column),
                "{}",
                case.id
            );
            assert_eq!(candidate.original, expected.original, "{}", case.id);
            assert_eq!(candidate.replacement, expected.replacement, "{}", case.id);
            assert_eq!(candidate.symbol, expected.symbol, "{}", case.id);
            let id = validate_candidate_with_context(&context, candidate).unwrap();
            assert_eq!(id.as_str(), candidate.id, "{}", case.id);
            let mut wrong_column = CandidateDescriptor::from(candidate);
            wrong_column.column += 1;
            assert_eq!(
                validate_candidate_with_context(&context, &wrong_column),
                Err(CandidateValidationError::LocationMismatch),
                "{}",
                case.id,
            );
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 18);
}
