use super::*;
use rstest::rstest;

fn edits(source: &str, op: &str) -> Vec<(String, String)> {
    let ops = MutationOperatorSelection::default();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &ops,
        profile: MutationProfile::Full,
        max_candidates: 256,
    };
    analyze_source(&request, source)
        .candidates
        .into_iter()
        .filter(|c| c.operator.as_str() == op)
        .map(|c| (c.original.clone(), c.replacement.clone()))
        .collect()
}

#[rstest]
#[case::compiled_local(
    "import re\ndef run(s):\n p=re.compile('a')\n return p.search(s,1)",
    "regex_match_boundary",
    "search",
    "match"
)]
#[case::aliased(
    "from re import fullmatch as f\nx=f(p,s)",
    "regex_match_boundary",
    "f",
    "__import__('re').match"
)]
#[case::terminal(
    r"import re; r=re.search(r'x$', s)",
    "regex_match_boundary",
    "$",
    r"\Z"
)]
#[case::decimal(
    r"import re; r=re.fullmatch(r'\d+', s)",
    "regex_character_set",
    r"\d",
    "[0-9]"
)]
#[case::negative_word(
    r"import re; r=re.search(r'\W', s, re.I)",
    "regex_character_set",
    r"\W",
    "(?a:[^A-Za-z0-9_])"
)]
#[case::ascii_boundary(
    r"from re import search, ASCII as A; r=search(r'\b', s, A)",
    "regex_character_set",
    r"\b",
    r"(?u:\b)"
)]
#[case::flag(r"import re; r=re.compile(r'(?u)\s\w')", "regex_ascii_mode", "u", "a")]
fn expected_regex_edit(
    #[case] source: &str,
    #[case] operator: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    let expected = (before.to_owned(), after.to_owned());

    let found = edits(source, operator);

    assert!(found.contains(&expected), "{found:?}");
}
#[rstest]
#[case::unclosed(r"\d(")]
#[case::unknown_escape(r"\d\q")]
#[case::orphan_repeat(r"\d|+")]
#[case::double_repeat(r"\d**")]
#[case::backward_bounds(r"\d{3,2}")]
#[case::overflow_repeat(r"\d{4294967295}")]
#[case::unknown_reference(r"\d\1")]
#[case::open_reference(r"(\d\1)")]
#[case::missing_named_reference(r"\d(?P=absent)")]
#[case::duplicate_group(r"(?P<x>\d)(?P<x>b)")]
#[case::same_lookbehind_group(r"(?<=(\d)\1)")]
#[case::variable_lookbehind(r"(?<=a|\d\d)")]
#[case::three_conditional_arms(r"(\d)(?(1)x|y|z)")]
#[case::missing_forward_group(r"\d(?(1)x|y)")]
#[case::bad_class_range(r"\d[\w-a]")]
#[case::descending_class(r"\d[z-a]")]
#[case::set_intersection(r"\d[a&&b]")]
#[case::misplaced_global(r"\d(?x)")]
#[case::contradictory_flags(r"(?i-i:\d)")]
#[case::incompatible_global(r"(?u)(?a)\d")]
#[case::unknown_named_character(r"\d\N{NOT A UNICODE NAME}")]
#[case::unicode17_name(r"\d\N{TROMBONE}")]
#[case::unused_leading_zero_in_cjk(r"\d\N{CJK UNIFIED IDEOGRAPH-04E00}")]
#[case::empty_global(r"(?)\d")]
#[case::future_difference(r"\d[!--]")]
fn malformed_suffix_releases_no_atoms(#[case] pattern: &str) {
    let source = format!("import re\np=re.compile(r'{pattern}')");

    let actual = edits(&source, "regex_character_set");

    assert!(actual.is_empty(), "{actual:?}");
}

#[rstest]
#[case::external_ascii(
    r"import re; p=re.compile(r'\W',re.A)",
    "regex_character_set",
    r"\W",
    r"(?u:\W)"
)]
#[case::explicit_ascii(
    r"import re; p=re.compile(r'[^0-9]',256)",
    "regex_character_set",
    "[^0-9]",
    r"(?u:\D)"
)]
#[case::external_or(
    r"from re import A as ASCII, I, compile; p=compile(r'\w',flags=I|ASCII)",
    "regex_ascii_mode",
    "ASCII",
    "32"
)]
#[case::strict_multiline(
    r"import re; p=re.compile(r'(?m:\Ax)')",
    "regex_match_boundary",
    r"\A",
    "^"
)]
#[case::named_group(
    r"import re; p=re.compile(r'(?P<value>\d)(?P=value)')",
    "regex_character_set",
    r"\d",
    "[0-9]"
)]
#[case::conditional(
    r"import re; p=re.compile(r'(\d)?(?(1)x|y)')",
    "regex_character_set",
    r"\d",
    "[0-9]"
)]
#[case::lookbehind(
    r"import re; p=re.compile(r'(?<=\d|x)y')",
    "regex_character_set",
    r"\d",
    "[0-9]"
)]
#[case::unicode_name(
    r"import re; p=re.compile(r'[\N{DIGIT ZERO}-\N{DIGIT NINE}]')",
    "regex_character_set",
    r"[\N{DIGIT ZERO}-\N{DIGIT NINE}]",
    r"\d"
)]
#[case::unicode_group(
    r"import re; p=re.compile(r'(?P<数>\d)(?P=数)')",
    "regex_character_set",
    r"\d",
    "[0-9]"
)]
#[case::possessive(
    r"import re; p=re.compile(r'(?>\d{2,5}+)')",
    "regex_character_set",
    r"\d",
    "[0-9]"
)]
#[case::implicit_adjacent(
    "import re; p=re.compile('[0-''9]''x')",
    "regex_character_set",
    "[0-''9]'",
    "\\\\d''' "
)]
#[case::bytes_end(
    r"import re; p=re.compile(rb'\xff\z')",
    "regex_match_boundary",
    r"\z",
    "$"
)]
#[case::effective_folded_word(
    r"import re; re.compile(r'[a-z0-9_]',re.I|re.A)",
    "regex_character_set",
    "[a-z0-9_]",
    r"(?u:\w)"
)]
fn supported_regex_shapes(
    #[case] source: &str,
    #[case] op: &str,
    #[case] old: &str,
    #[case] new: &str,
) {
    let expected = (old.to_owned(), new.to_owned());

    let actual = edits(source, op);

    assert!(actual.contains(&expected), "{actual:?}");
}

#[rstest]
#[case::shadow("import re\ndef f(re): return re.match(x,s)", "regex_match_boundary")]
#[case::module_write("import re\nre.match=fn\nre.search(x,s)", "regex_match_boundary")]
#[case::module_escape("import re\npublish(re)\nre.match(x,s)", "regex_match_boundary")]
#[case::unknown_receiver("def f(p): return p.search(s)", "regex_match_boundary")]
#[case::local_rebound(
    "import re\ndef f():\n p=re.compile('x')\n p=custom\n return p.match(s)",
    "regex_match_boundary"
)]
#[case::conditional_binding(
    "import re\ndef f(ok):\n if ok: p=re.compile('x')\n return p.match(s)",
    "regex_match_boundary"
)]
#[case::dynamic_expansion("import re; re.match(*args)", "regex_match_boundary")]
#[case::duplicate_parameter("import re; re.match('x',s,pattern='y')", "regex_match_boundary")]
#[case::wrong_type(r"import re; re.match(r'\d',b'2')", "regex_match_boundary")]
#[case::unknown_flag(r"import re; re.compile(r'\d',unknown)", "regex_character_set")]
#[case::fully_scoped(r"import re; re.compile(r'(?a:\d)(?u:\w)')", "regex_ascii_mode")]
#[case::external_and_inline(r"import re; re.compile(r'(?a)\d',re.A)", "regex_ascii_mode")]
#[case::ascii_literal(r"import re; re.compile(r'(?i)x')", "regex_ascii_mode")]
#[case::ascii_reference(r"import re; re.compile(r'(?i)(x)\1')", "regex_ascii_mode")]
#[case::class_backspace(r"import re; re.compile(r'[\b]')", "regex_character_set")]
#[case::escaped_slash(r"import re; re.compile(r'\\d')", "regex_character_set")]
#[case::only_comments("import re; re.compile(r'(?x)(?#\\d)#\\w')", "regex_character_set")]
#[case::redundant_anchor(r"import re; re.fullmatch(r'^x$',s)", "regex_match_boundary_anchor")]
#[case::locale(r"import re; re.compile(rb'\w',re.L)", "regex_character_set")]
#[case::compiled_type_mismatch(
    r"import re; pat=re.compile(b'x'); re.match(pat,'x')",
    "regex_match_boundary"
)]
#[case::verbose_comment_ending_in_backslash(
    "import re\nre.compile('(?x)\\\\d #comment\\\\')",
    "regex_character_set"
)]
#[case::uncased_ordinal(r"import re; re.compile(r'(?i)ª')", "regex_ascii_mode")]
fn excludes_unproven_or_equivalent(#[case] source: &str, #[case] op: &str) {
    let anchor = op == "regex_match_boundary_anchor";

    let mut actual = edits(source, if anchor { "regex_match_boundary" } else { op });
    if anchor {
        actual.retain(|(s, _)| s != "fullmatch")
    }

    assert!(actual.is_empty(), "{actual:?}");
}

fn analyze_regex(
    source: &str,
    ids: &[&str],
    max: usize,
    lines: &[LineRange],
    symbols: &[String],
) -> AnalyzerOutput {
    let operators = serde_json::from_value(serde_json::json!(ids)).unwrap();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        operators: &operators,
        lines,
        symbols,
        profile: MutationProfile::Full,
        max_candidates: max,
    };
    analyze_source(&request, source)
}
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(24))]
    #[test]
    fn referenced_variants_obey_unlimited_ranked_prefix(
        categories in proptest::collection::vec(proptest::sample::select(vec!['d','W','s','B']),12..48),
        budget in 0usize..6,
    ){
        let pattern=categories.into_iter().map(|c|format!("\\{c}")).collect::<String>();
        let source=format!("import re\np=r'{pattern}'\ndef f(s): return re.search(p,s)\n");
        let ids=["regex_match_boundary","regex_character_set","regex_ascii_mode"];
        let all=analyze_regex(&source,&ids,usize::MAX,&[],&[]);
        let expected:Vec<_>=all.candidates.iter().take(budget).map(|c|(c.span,c.operator,c.replacement.clone())).collect();

        super::regex::reset_materialized();
        let limited=analyze_regex(&source,&ids,budget,&[],&[]);
        let generated=super::regex::materialized();

        let actual:Vec<_>=limited.candidates.iter().map(|c|(c.span,c.operator,c.replacement.clone())).collect();
        proptest::prop_assert_eq!(actual,expected);
        proptest::prop_assert!(limited.truncated);
        proptest::prop_assert!(generated<=(budget+2)*(pattern.len()+34));
    }
}
#[rstest]
#[case::inside(4, "chosen", 1)]
#[case::other_line(3, "chosen", 0)]
#[case::other_symbol(4, "other", 0)]
fn edits_respect_token_line_and_symbol(
    #[case] line: u32,
    #[case] symbol: &str,
    #[case] count: usize,
) {
    let source = "import re\ndef chosen():\n return re.compile(r'a'\n                   r'\\d')\n";
    let ids = ["regex_character_set"];

    let output = analyze_regex(
        source,
        &ids,
        32,
        &[LineRange {
            start: line,
            end: line,
        }],
        &[symbol.to_owned()],
    );

    assert_eq!(output.candidates.len(), count);
}

#[test]
fn rejected_sink_stops_reference_materialization() {
    let source = format!(
        "import re\npat=r'{}'\nm=re.compile(pat)\n",
        r"\d".repeat(2048)
    );
    let selected = serde_json::from_value(serde_json::json!(["regex_character_set"])).unwrap();
    let request = AnalyzeRequest {
        payload: super::super::payload::PayloadBudget::new(0),
        path: Utf8Path::new("x.py"),
        lines: &[],
        symbols: &[],
        operators: &selected,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    super::regex::reset_materialized();

    let outcome = analyze_source_cancellable(&request, &source, || false);

    assert!(matches!(outcome, Err(AnalysisError::PayloadLimit { .. })));
    assert_eq!(super::regex::materialized(), 4102);
}
