use super::{DelimiterDepth, emit_edits};
use hoimin_core::MutationOperator;
use ruff_python_parser::parse_expression;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Value,
    Option,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct Edit {
    kind: Kind,
    start: usize,
    end: usize,
    replacement: String,
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: String,
    edits32: Option<Vec<Edit>>,
    edits: Vec<Edit>,
}

#[test]
fn fstring_format_edits_match_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/format-spec.jsonl"
    ));
    let operators = serde_json::from_value(serde_json::json!([
        MutationOperator::FStringFormatValueNeighbor.as_str(),
        MutationOperator::FStringFormatOptionSwap.as_str(),
    ]))
    .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        let source = &case.source;
        let parsed = parse_expression(source)
            .unwrap_or_else(|error| panic!("{}: {error} in {source}", case.id));
        let mut actual = Vec::new();
        emit_edits(
            parsed.syntax().body.as_f_string_expr().unwrap(),
            parsed.tokens(),
            source,
            &DelimiterDepth::default(),
            &operators,
            &|| false,
            |range, replacement, operator| {
                let kind = match operator {
                    MutationOperator::FStringFormatValueNeighbor => Kind::Value,
                    MutationOperator::FStringFormatOptionSwap => Kind::Option,
                    _ => panic!("{}: unexpected operator {operator:?}", case.id),
                };
                actual.push(Edit {
                    kind,
                    start: usize::from(range.start()),
                    end: usize::from(range.end()),
                    replacement,
                });
                true
            },
        );
        let expected = match isize::BITS {
            32 => case.edits32.unwrap_or(case.edits),
            64 => case.edits,
            bits => panic!("unsupported pointer width {bits}"),
        };
        assert_eq!(actual, expected, "{}: {source}", case.id);
        for edit in actual {
            let mut edited = source.clone();
            edited.replace_range(edit.start..edit.end, &edit.replacement);
            parse_expression(&edited)
                .unwrap_or_else(|error| panic!("{}: {error} in {edited}", case.id));
        }
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 471);
}
