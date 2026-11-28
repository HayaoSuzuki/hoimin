use super::analyze;
use hoimin_core::MutationOperator;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Edit {
    start: usize,
    original: String,
    replacement: String,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: String,
    edits: Vec<Edit>,
}

#[test]
fn format_removal_matches_lean_through_public_analysis() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/format-removal.jsonl"
    ));
    let operator = MutationOperator::FStringFormatSpecRemove;
    let selection = serde_json::from_value(serde_json::json!([operator.as_str()])).unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        let output = analyze(&case.source, &selection, 100);
        assert!(!output.truncated, "{}", case.id);
        assert!(
            output.diagnostics.is_empty(),
            "{}: {:?}",
            case.id,
            output.diagnostics
        );
        let actual: Vec<_> = output
            .candidates
            .iter()
            .map(|candidate| {
                assert_eq!(candidate.operator, operator, "{}", case.id);
                let start = usize::try_from(candidate.span.start).unwrap();
                let length = usize::try_from(candidate.span.length).unwrap();
                let stop = start + length;
                assert_eq!(
                    case.source.get(start..stop),
                    Some(candidate.original.as_str()),
                    "{}",
                    case.id
                );
                let mut mutant = case.source.clone();
                mutant.replace_range(start..stop, &candidate.replacement);
                ruff_python_parser::parse_module(&mutant)
                    .unwrap_or_else(|error| panic!("{}: {error} in {mutant}", case.id));
                Edit {
                    start,
                    original: candidate.original.clone(),
                    replacement: candidate.replacement.clone(),
                }
            })
            .collect();
        assert_eq!(actual, case.edits, "{}: {}", case.id, case.source);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 145);
}
