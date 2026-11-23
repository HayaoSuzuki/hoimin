use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};

#[rstest::rstest]
#[case::digit_swap("unicode_digit_predicate_swap", "x.isdigit()\n")]
#[case::digit_ascii("unicode_digit_ascii", "x.isnumeric()\n")]
#[case::normalization_form(
    "unicode_normalization_form",
    "import unicodedata as u\nu.is_normalized('NFC', x)\n"
)]
#[case::normalization_remove(
    "unicode_normalization_remove",
    "from unicodedata import normalize as n\nn('NFC', x)\n"
)]
#[case::casefold("string_casefold_lower", "x.casefold()\n")]
#[case::affix("string_affix_strip", "x.removeprefix('ab')\n")]
#[case::encoding_replace("encoding_errors_replace", "x.decode()\n")]
#[case::encoding_strict("encoding_errors_strict", "x.encode(errors='replace')\n")]
#[case::ignored_omitted("encoding_errors_ignore", "x.decode()\n")]
#[case::ignored_positional("encoding_errors_ignore", "x.decode('ascii', 'strict')\n")]
#[case::ignored_dynamic("encoding_errors_ignore", "x.decode(errors=policy())\n")]
fn proposed_operator_supports_default_and_opt_out(
    #[case] id: &str,
    #[case] source: &str,
    #[values(true, false)] enabled: bool,
) {
    // Arrange
    let mut operators = MutationOperatorSelection::default();
    if !enabled {
        operators.exclude(MutationOperator::from_name(id).unwrap());
    }
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };

    // Act
    let output = analyze_source_cancellable(&request, source, || false).unwrap();

    // Assert
    assert_eq!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.operator.as_str() == id),
        enabled,
        "{id}"
    );
}

fn analyze(
    source: &str,
    id: &str,
    lines: &[hoimin_core::LineRange],
    cap: usize,
) -> hoimin_python::AnalyzerOutput {
    let operators = serde_json::from_value(serde_json::json!([id])).unwrap();
    let request = AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines,
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: cap,
        payload: PayloadBudget::default(),
    };
    analyze_source_cancellable(&request, source, || false).unwrap()
}

#[rstest::rstest]
#[case::unicode_rebound(
    "unicode_normalization_form",
    "import unicodedata as u\nu = custom\nu.normalize('NFC',s)"
)]
#[case::direct_shadow(
    "unicode_normalization_remove",
    "from unicodedata import normalize as n\ndef f(n): return n('NFC',s)"
)]
#[case::module_escape(
    "unicode_normalization_remove",
    "import unicodedata as u\nleak(u)\nu.normalize('NFC',s)"
)]
#[case::no_remove_predicate(
    "unicode_normalization_remove",
    "import unicodedata as u\nu.is_normalized('NFC',s)"
)]
#[case::unknown_form(
    "unicode_normalization_form",
    "import unicodedata as u\nu.normalize(form,s)"
)]
#[case::positional_only(
    "unicode_normalization_remove",
    "import unicodedata as u\nu.normalize('NFC',unistr=s)"
)]
#[case::codec_missing_object(
    "encoding_errors_ignore",
    "import codecs as c\nc.decode(encoding='utf8')"
)]
#[case::codec_duplicate_policy(
    "encoding_errors_replace",
    "import codecs as c\nc.decode(b,'utf8','strict',errors=policy)"
)]
#[case::codec_unknown_keyword("encoding_errors_replace", "s.decode(codec='utf8')")]
#[case::codec_module_escape("encoding_errors_replace", "import codecs as c\nf(c)\nc.decode(b)")]
#[case::unchanged_destination("encoding_errors_ignore", "s.decode(errors='ig' 'nore')")]
#[case::unchanged_default("encoding_errors_strict", "s.decode()")]
#[case::affix_keyword("string_affix_strip", "s.removeprefix(prefix='abc')")]
#[case::strip_omitted("string_affix_strip", "s.rstrip()")]
#[case::predicate_supplied("unicode_digit_ascii", "s.isdigit(1)")]
fn uncertain_or_incompatible_call_is_skipped(#[case] id: &str, #[case] source: &str) {
    // Arrange
    let lines = [];
    // Act
    let output = analyze(source, id, &lines, 10);
    // Assert
    assert_eq!(
        output.candidates.len(),
        0,
        "{id}: {source}\n{:?}",
        output.candidates
    );
}

#[rstest::rstest]
#[case::insert_on_call("s.decode(\n encoding='utf8',\n)", 1, 1)]
#[case::insert_outside_selection("s.decode(\n encoding='utf8',\n)", 2, 0)]
#[case::explicit_on_option("s.decode(\n errors=policy,\n)", 2, 1)]
#[case::explicit_outside_selection("s.decode(\n errors=policy,\n)", 1, 0)]
fn encoding_selects_the_policy_site(
    #[case] source: &str,
    #[case] line: u32,
    #[case] expected: usize,
) {
    // Arrange
    let selected = [hoimin_core::LineRange {
        start: line,
        end: line,
    }];
    // Act
    let output = analyze(source, "encoding_errors_ignore", &selected, 10);
    // Assert
    assert_eq!(output.candidates.len(), expected);
}

proptest::proptest! {
    #[test]
    fn admitted_text_mutant_parses(
        scenario in 0..6usize, receiver_grouping in 0..6usize, callee_grouping in 0..6usize,
    ) {
        // Arrange
        let receiver = format!("{}r{}", "(".repeat(receiver_grouping), ")".repeat(receiver_grouping));
        let (id, callee, args) = match scenario {
            0 => ("unicode_digit_ascii", format!("{receiver}.isdigit"), ""),
            1 => ("unicode_normalization_remove", "u.normalize".into(), "'NFC', (x := value)"),
            2 => ("encoding_errors_ignore", format!("{receiver}.decode"), "encoding='ascii', # note\r\n"),
            3 => ("encoding_errors_replace", format!("{receiver}.encode"), "'ascii', (p := policy())"),
            4 => ("unicode_normalization_form", "u.normalize".into(), "('NF'\n'C'), value"),
            _ => ("encoding_errors_replace", format!("{receiver}.decode"), "item for item in values"),
        };
        let source = format!("import unicodedata as u\r\nout = {}{callee}{}({args})\r\n", "(".repeat(callee_grouping), ")".repeat(callee_grouping));

        // Act
        let output = analyze(&source,id,&[],10);

        // Assert
        proptest::prop_assert!(!output.candidates.is_empty(), "{}", source);
        for candidate in &output.candidates {
            let mut script = source.clone();
            let start = usize::try_from(candidate.span.start).unwrap();
            script.replace_range(start..start + usize::try_from(candidate.span.length).unwrap(), &candidate.replacement);
            proptest::prop_assert!(ruff_python_parser::parse_module(&script).is_ok(), "{}", script);
        }
    }

    #[test]
    fn capped_digit_swaps_preserve_the_uncapped_prefix(lines in 1..16usize, cap in 1..24usize) {
        // Arrange
        let source = "s.isdecimal()\ns.isnumeric()\n".repeat(lines);
        let uncapped = analyze(&source, "unicode_digit_predicate_swap", &[], 100);

        // Act
        let capped = analyze(&source, "unicode_digit_predicate_swap", &[], cap);

        // Assert
        proptest::prop_assert_eq!(&capped.candidates[..], &uncapped.candidates[..cap.min(uncapped.candidates.len())]);
    }
}

#[test]
fn normalization_form_keeps_comments_between_literal_parts() {
    // Arrange
    let source = "import unicodedata as u\r\nu.normalize(('NF' # chosen form\r\n 'C'), s)\r\n";

    // Act
    let output = analyze(source, "unicode_normalization_form", &[], 10);

    // Assert
    assert_eq!(output.candidates.len(), 2);
    assert!(
        output
            .candidates
            .iter()
            .all(|c| c.replacement.contains("# chosen form\r\n"))
    );
}

#[rstest::rstest]
#[case::decoded_form(
    "unicode_normalization_form",
    r"import unicodedata as u
u.normalize('N' '' '\x46' 'KC', text)",
    2
)]
#[case::decoded_policy("encoding_errors_ignore", r"s.decode(errors='ig' '' '\x6e' 'ore')", 0)]
#[case::form_prefix_only(
    "unicode_normalization_form",
    "import unicodedata as u\nu.normalize('NF' '', text)",
    0
)]
#[case::unknown_policy_suffix("encoding_errors_ignore", "s.decode(errors='ig' 'nored')", 1)]
fn multipart_matching_uses_the_entire_decoded_value(
    #[case] id: &str,
    #[case] source: &str,
    #[case] expected: usize,
) {
    // Arrange
    let lines = [];

    // Act
    let output = analyze(source, id, &lines, 10);

    // Assert
    assert_eq!(output.candidates.len(), expected);
}
