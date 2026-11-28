use super::{Change, Literal};
use ruff_python_parser::parse_expression;

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    source: String,
    decoded: String,
    ranges: Vec<RangeQuery>,
    edits: Vec<EditQuery>,
    protected: Vec<String>,
}

#[derive(serde::Deserialize)]
struct RangeQuery {
    span: [usize; 2],
    expected: Option<[usize; 2]>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum EditKind {
    Replace,
    Surround,
}

#[derive(serde::Deserialize)]
struct EditQuery {
    span: [usize; 2],
    kind: EditKind,
    before: String,
    after: String,
    expected: String,
}

fn read_values(source: &str, id: &str) -> String {
    let parsed =
        parse_expression(source).unwrap_or_else(|error| panic!("{id}: {error} in {source:?}"));
    Literal::read(&parsed.syntax().body, source, &|| false)
        .unwrap_or_else(|| panic!("{id}: did not map {source:?}"))
        .chars
        .into_iter()
        .collect()
}

#[test]
fn regex_literal_spans_and_edits_match_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/regex-literal-span.jsonl"
    ));
    let mut ids = std::collections::BTreeSet::new();
    let mut range_count = 0;
    let mut edit_count = 0;
    for row in corpus.lines() {
        let case: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        let parsed =
            parse_expression(&case.source).unwrap_or_else(|error| panic!("{}: {error}", case.id));
        let literal = Literal::read(&parsed.syntax().body, &case.source, &|| false)
            .unwrap_or_else(|| panic!("{}: did not map the literal", case.id));
        assert_eq!(
            literal.chars.iter().collect::<String>(),
            case.decoded,
            "{}",
            case.id
        );
        for query in &case.ranges {
            let [start, end] = query.span;
            let actual = literal
                .range_for(&(start..end))
                .map(|range| [usize::from(range.start()), usize::from(range.end())]);
            assert_eq!(actual, query.expected, "{} span {start}..{end}", case.id);
        }
        for query in &case.edits {
            let [start, end] = query.span;
            let change = match query.kind {
                EditKind::Replace => Change::Replace(&query.before),
                EditKind::Surround => Change::Surround {
                    prefix: &query.before,
                    suffix: &query.after,
                },
            };
            let (range, replacement) = literal
                .edit(start..end, change)
                .unwrap_or_else(|| panic!("{}: no edit at {start}..{end}", case.id));
            let mut edited = case.source.clone();
            edited.replace_range(
                usize::from(range.start())..usize::from(range.end()),
                &replacement,
            );
            assert_eq!(
                read_values(&edited, &case.id),
                query.expected,
                "{} span {start}..{end}",
                case.id
            );
            let reference = format!("({})", literal.at_reference(range, &replacement));
            assert_eq!(
                read_values(&reference, &case.id),
                query.expected,
                "{} reference {start}..{end}",
                case.id
            );
            for protected in &case.protected {
                let occurrences = case.source.matches(protected).count();
                assert!(occurrences > 0, "{}", case.id);
                assert_eq!(
                    edited.matches(protected).count(),
                    occurrences,
                    "{} lost {protected:?}",
                    case.id
                );
            }
        }
        range_count += case.ranges.len();
        edit_count += case.edits.len();
        assert!(ids.insert(case.id));
    }
    assert_eq!((ids.len(), range_count, edit_count), (12, 261, 16));
}
