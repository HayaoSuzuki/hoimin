use super::super::{analyze_source_cancellable, payload::PayloadBudget, regex};
use super::{AnalyzeRequest, CandidateSelection, LineRange};
use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};

#[derive(serde::Deserialize)]
struct IntervalQuery {
    span: [u32; 2],
    symbol: Option<String>,
    include: bool,
}

#[derive(Debug, serde::Deserialize)]
struct Input {
    lines: Vec<[u32; 2]>,
    symbols: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RegexQuery {
    #[serde(flatten)]
    input: Input,
    inspects: bool,
    emits: bool,
}

#[derive(Debug, serde::Deserialize)]
struct Edit {
    start: usize,
    original: String,
    replacement: String,
    line: u32,
    symbol: String,
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Cases {
    Interval {
        #[serde(flatten)]
        input: Input,
        queries: Vec<IntervalQuery>,
    },
    Regex {
        source: String,
        edit: Edit,
        queries: Vec<RegexQuery>,
    },
}

#[derive(serde::Deserialize)]
struct Fixture {
    schema: u32,
    mode: String,
    id: String,
    #[serde(flatten)]
    cases: Cases,
}

impl Input {
    fn ranges(&self) -> Vec<LineRange> {
        self.lines
            .iter()
            .map(|&[start, end]| LineRange { start, end })
            .collect()
    }

    fn request<'a>(
        &'a self,
        lines: &'a [LineRange],
        operators: &'a MutationOperatorSelection,
    ) -> AnalyzeRequest<'a> {
        AnalyzeRequest {
            payload: PayloadBudget::default(),
            path: Utf8Path::new("pkg/subject.py"),
            lines,
            symbols: &self.symbols,
            operators,
            profile: MutationProfile::Full,
            max_candidates: 4,
        }
    }
}

fn check_regex(
    source: &str,
    edit: &Edit,
    query: &RegexQuery,
    operators: &MutationOperatorSelection,
    id: &str,
) {
    let lines = query.input.ranges();
    let request = query.input.request(&lines, operators);
    regex::clear_work();
    let actual = analyze_source_cancellable(&request, source, || false).unwrap();
    let inspected = regex::pattern_work().0 != 0;
    assert_eq!(inspected, query.inspects, "{id}: {query:?}");
    assert!(
        actual.diagnostics.is_empty(),
        "{id}: {:?}",
        actual.diagnostics
    );
    assert!(!actual.truncated, "{id}");
    assert_eq!(
        actual.candidates.len(),
        usize::from(query.emits),
        "{id}: {query:?}"
    );
    if let [candidate] = actual.candidates.as_slice() {
        assert_eq!(candidate.operator, MutationOperator::RegexAsciiMode, "{id}");
        assert_eq!(
            usize::try_from(candidate.span.start).unwrap(),
            edit.start,
            "{id}"
        );
        assert_eq!(
            usize::try_from(candidate.span.length).unwrap(),
            edit.original.len(),
            "{id}"
        );
        assert_eq!(candidate.original, edit.original, "{id}");
        assert_eq!(candidate.replacement, edit.replacement, "{id}");
        assert_eq!(candidate.line, edit.line, "{id}");
        assert_eq!(
            candidate.symbol.as_deref(),
            Some(edit.symbol.as_str()),
            "{id}"
        );
    }
}

#[test]
fn interval_admission_and_regex_anchors_match_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/candidate-selection.jsonl"
    ));
    let operators = serde_json::from_value(serde_json::json!([
        MutationOperator::RegexAsciiMode.as_str()
    ]))
    .unwrap();
    let mut ids = std::collections::BTreeSet::new();
    let mut interval_queries = 0;
    let mut regex_queries = 0;
    for row in corpus.lines() {
        let fixture: Fixture = serde_json::from_str(row).unwrap();
        assert_eq!(fixture.schema, 1, "{}", fixture.id);
        assert_eq!(fixture.mode, "internal-fixture", "{}", fixture.id);
        match fixture.cases {
            Cases::Interval { input, queries } => {
                let lines = input.ranges();
                let request = input.request(&lines, &operators);
                let admission = CandidateSelection::new(&request, &|| false).unwrap();
                interval_queries += queries.len();
                for query in queries {
                    let [first, last] = query.span;
                    assert_eq!(
                        admission.intersects(first, last, query.symbol.as_deref()),
                        query.include,
                        "{}: {first}..={last}, {:?}",
                        fixture.id,
                        query.symbol,
                    );
                }
            }
            Cases::Regex {
                source,
                edit,
                queries,
            } => {
                regex_queries += queries.len();
                for query in &queries {
                    check_regex(&source, &edit, query, &operators, &fixture.id);
                }
            }
        }
        assert!(ids.insert(fixture.id));
    }
    assert_eq!(
        (ids.len(), interval_queries, regex_queries),
        (72, 6732, 324)
    );
}
