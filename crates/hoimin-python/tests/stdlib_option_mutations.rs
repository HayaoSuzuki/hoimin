use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};

#[rstest::rstest]
#[case::map("map_strict_toggle", "map(f, xs, ys)")]
#[case::batched(
    "batched_strict_toggle",
    "from itertools import batched\nbatched(xs, 2)"
)]
#[case::quote("url_quote_swap", "import urllib.parse as p\np.quote(s)")]
#[case::query("url_query_policy", "from urllib.parse import parse_qs\nparse_qs(s)")]
#[case::base64("base64_validate_toggle", "import base64\nbase64.b64decode(s)")]
#[case::json("json_allow_nan_toggle", "import json\njson.dumps(x)")]
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
#[case::map_single("map_strict_toggle", "map(str,x)")]
#[case::map_star("map_strict_toggle", "map(f,*x,y)")]
#[case::map_shadow("map_strict_toggle", "def f(map): return map(str,x,y)")]
#[case::map_literal("map_strict_toggle", "map(f,x,y,strict=False)")]
#[case::batched_literal(
    "batched_strict_toggle",
    "from itertools import batched\nbatched(xs,2,strict=True)"
)]
#[case::json_custom("json_allow_nan_toggle", "import json\njson.dump(x,fp,cls=encoder)")]
#[case::json_literal("json_allow_nan_toggle", "import json\njson.dumps(x,allow_nan=False)")]
#[case::json_unknown("json_allow_nan_toggle", "import json\njson.dumps(x,unknown=value)")]
#[case::base64_literal(
    "base64_validate_toggle",
    "import base64\nbase64.b64decode(x,None,True)"
)]
#[case::query_literal("url_query_policy", "import urllib.parse as p\np.urlencode(x,True)")]
#[case::quote_invalid_keyword("url_quote_swap", "import urllib.parse as p\np.unquote(x,safe='/')")]
fn rejects_call(#[case] id: &str, #[case] source: &str) {
    // Arrange
    let cap = 10;
    // Act
    let out = analyze(source, id, &[], cap);
    // Assert
    assert!(out.candidates.is_empty());
}

proptest::proptest! {
    #[test]
    fn duplicate_policy_is_excluded(group in 0..6usize, policy in "policy_[a-z]{1,10}") {
        // Arrange
        let source=format!("from urllib.parse import urlencode\nurlencode(xs, {}{policy}{}, doseq={policy})","(".repeat(group),")".repeat(group));
        // Act
        let out=analyze(&source,"url_query_policy",&[],10);
        // Assert
        proptest::prop_assert!(out.candidates.is_empty());
    }

    #[test]
    fn finite_cap_retains_prefix(n in 1..16usize,cap in 1..20usize) {
        // Arrange
        let source=format!("import json\n{}", "json.dumps(x)\njson.dump(x,fp)\n".repeat(n));
        let uncapped=analyze(&source,"json_allow_nan_toggle",&[],100);
        // Act
        let out=analyze(&source,"json_allow_nan_toggle",&[],cap);
        // Assert
        proptest::prop_assert_eq!(uncapped.candidates.len(),2*n);
        proptest::prop_assert_eq!(&out.candidates[..],&uncapped.candidates[..cap.min(2*n)]);
    }

    #[test]
    fn rendered_call_preserves_grammar(group in 0..8usize, scenario in 0..3usize) {
        // Arrange
        let args=match scenario {0=>"item for item in xs",1=>"xs, # comment\r\n",_=>"xs,allow_nan=(\n flag := value\n)"};
        let source=format!("import json\r\n{}json.dumps{}({args})","(".repeat(group),")".repeat(group));
        // Act
        let out=analyze(&source,"json_allow_nan_toggle",&[],10);
        // Assert
        proptest::prop_assert_eq!(out.candidates.len(),1);
        let c=&out.candidates[0];
        let start=usize::try_from(c.span.start).unwrap();
        let mut mutated=source;
        mutated.replace_range(start..start+usize::try_from(c.span.length).unwrap(),&c.replacement);
        proptest::prop_assert!(ruff_python_parser::parse_module(&mutated).is_ok(),"{}",mutated);
    }
}
#[rstest::rstest]
#[case::callee(2, 1)]
#[case::delimiter(3, 0)]
fn omitted_option_uses_call_line(#[case] line: u32, #[case] count: usize) {
    // Arrange
    let source = "import json\njson.dumps \\\n    (x)";
    let lines = [hoimin_core::LineRange {
        start: line,
        end: line,
    }];
    // Act
    let out = analyze(source, "json_allow_nan_toggle", &lines, 10);
    // Assert
    assert_eq!(out.candidates.len(), count);
    for candidate in out.candidates {
        let start = usize::try_from(candidate.span.start).unwrap();
        let length = usize::try_from(candidate.span.length).unwrap();
        assert_eq!(&source[start..start + length], "(x)");
    }
}

#[rstest::rstest]
#[case::original("urllib.parse.quote = mine")]
#[case::destination("urllib.parse.quote_plus = mine")]
#[case::root_escape("expose(urllib)")]
#[case::submodule_escape("expose(urllib.parse)")]
fn sibling_package_hazard(#[case] hazard: &str) {
    // Arrange
    let source =
        format!("import urllib.response\nfrom urllib.parse import quote as q\n{hazard}\nq('a b')");
    // Act
    let out = analyze(&source, "url_quote_swap", &[], 10);
    // Assert
    assert!(out.candidates.is_empty());
}
