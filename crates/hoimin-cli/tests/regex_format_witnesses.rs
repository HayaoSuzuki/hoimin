use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
use std::path::{Path, PathBuf};
use std::time::Duration;

async fn run_python(script: &str) {
    let python = std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap();
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        },
        PathBuf::from,
    );
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python)
            .args(["-B", "-c", script])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython execution deadline")
    .expect("set HOIMIN_OPERATOR_TEST_PYTHON to a CPython executable");
    assert!(
        output.status.success(),
        "{script}\n{}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[rstest::rstest]
#[case::lower_bound(
    MutationOperator::RegexQuantifierNeighbor,
    "import re\nout = re.fullmatch(r'a{2,4}', 'a')\n",
    "{2,4}",
    "{1,4}",
    "assert out is None",
    "assert out.group() == 'a'"
)]
#[case::fixed_lookbehind(
    MutationOperator::RegexQuantifierNeighbor,
    "import re\nout = re.search(r'(?<=a{2})x', 'ax')\n",
    "{2}",
    "{1}",
    "assert out is None",
    "assert out.span() == (1, 2)"
)]
#[case::lookbehind_shared_capture(
    MutationOperator::RegexQuantifierNeighbor,
    r"import re; out = re.search(r'(a{2})(?<=(?:\1a|\1b))x', 'aax')",
    "{2}",
    "{1}",
    "assert out is None",
    "assert out.span() == (1, 3) and out[1] == 'a'"
)]
#[case::possessive_count(
    MutationOperator::RegexQuantifierNeighbor,
    "import re\nout = re.fullmatch(rb'a{2}+a', b'aaa')\n",
    "{2}",
    "{3}",
    "assert out.group() == b'aaa'",
    "assert out is None"
)]
#[case::greedy_unicode(
    MutationOperator::RegexGreedinessToggle,
    "import re\nout = re.search(r'(?i:é+)', 'éÉé')\n",
    "+",
    "+?",
    "assert out.group() == 'éÉé'",
    "assert out.group() == 'é'"
)]
#[case::lazy_bytes(
    MutationOperator::RegexGreedinessToggle,
    r"import re; out = re.search(rb'\w+?', b'abc')",
    "?",
    "",
    "assert out.group() == b'a'",
    "assert out.group() == b'abc'"
)]
#[case::nested_alternative(
    MutationOperator::RegexAlternativeDelete,
    "import re\nout = re.fullmatch(r'(?:ant|bird|cat)s', 'birds')\n",
    "bird|",
    "",
    "assert out.group() == 'birds'",
    "assert out is None"
)]
#[case::named_reference(
    MutationOperator::RegexAlternativeDelete,
    "import re\nout = [re.fullmatch(r'(?:(a)|b|(?P<target>c))(?P=target)', s) for s in ['ac', 'bc', 'cc']]\n",
    "(a)|",
    "",
    "assert [m is not None for m in out] == [False, False, True] and out[-1].groupdict() == {'target': 'c'}",
    "assert [m is not None for m in out] == [False, False, True] and out[-1].groups() == ('c',)"
)]
#[case::dynamic_alignment(
    MutationOperator::FStringFormatOptionSwap,
    "events = []\ndef read(name, value):\n    events.append(name)\n    return value\nout = f'{read(\"value\", 5):{read(\"fill\", \"_\")}>{read(\"width\", 4)}}'\n",
    ">",
    "<",
    "assert (out, events) == ('___5', ['value', 'fill', 'width'])",
    "assert (out, events) == ('5___', ['value', 'fill', 'width'])"
)]
#[case::sign(
    MutationOperator::FStringFormatOptionSwap,
    "out = f'{5:+5d}'\n",
    "+",
    " ",
    "assert out == '   +5'",
    "assert out == '    5'"
)]
#[case::grouping(
    MutationOperator::FStringFormatOptionSwap,
    "out = f'{123456:,d}'\n",
    ",",
    "_",
    "assert out == '123,456'",
    "assert out == '123_456'"
)]
#[case::fractional_grouping(
    MutationOperator::FStringFormatOptionSwap,
    "out = f'{1.234567:.6,f}'\n",
    ",",
    "_",
    "assert out == '1.234,567'",
    "assert out == '1.234_567'"
)]
#[case::presentation_with_two_fields(
    MutationOperator::FStringFormatOptionSwap,
    "events = []\ndef read(name, value):\n    events.append(name)\n    return value\nout = f'{read(\"value\", 0.25):{read(\"width\", 8)}.{read(\"precision\", 1)}f}'\n",
    "f",
    "%",
    "assert (out, events) == ('     0.2', ['value', 'width', 'precision'])",
    "assert (out, events) == ('   25.0%', ['value', 'width', 'precision'])"
)]
#[case::complex_presentation(
    MutationOperator::FStringFormatOptionSwap,
    "out = f'{1j:f}'\n",
    "f",
    "e",
    "assert out == '0.000000+1.000000j'",
    "assert out == '0.000000e+00+1.000000e+00j'"
)]
#[case::signed_complex_presentation(
    MutationOperator::FStringFormatOptionSwap,
    "out = f'{-1j:g}'\n",
    "g",
    "e",
    "assert out == '-0-1j'",
    "assert out == '-0.000000e+00-1.000000e+00j'"
)]
#[tokio::test]
async fn discovered_mutant_changes_cpython_behavior(
    #[case] operator: MutationOperator,
    #[case] source: &str,
    #[case] original: &str,
    #[case] replacement: &str,
    #[case] original_assertion: &str,
    #[case] mutant_assertion: &str,
) {
    // Arrange
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([operator.as_str()])).unwrap();
    let request = AnalyzeRequest {
        path: Utf8Path::new("regex_format_witness.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
        payload: PayloadBudget::default(),
    };
    run_python(&format!("{source}\n{original_assertion}")).await;

    // Act
    let analyzed = analyze_source_cancellable(&request, source, || false).unwrap();
    let candidate = analyzed
        .candidates
        .iter()
        .find(|candidate| candidate.original == original && candidate.replacement == replacement)
        .expect("matching analyzer candidate");
    let mut mutant = source.to_owned();
    let start = usize::try_from(candidate.span.start).unwrap();
    let length = usize::try_from(candidate.span.length).unwrap();
    mutant.replace_range(start..start + length, &candidate.replacement);

    // Assert
    assert_eq!(candidate.operator, operator);
    run_python(&format!("{mutant}\n{mutant_assertion}")).await;
}
