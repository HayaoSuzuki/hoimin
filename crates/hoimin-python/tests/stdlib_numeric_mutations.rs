use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};

#[rstest::rstest]
#[case::isclose("math_isclose_tolerance", "import math\nmath.isclose(a,b)")]
#[case::precision("math_precision_loss", "import math\nmath.expm1(x)")]
#[case::statistics(
    "statistics_sample_population",
    "import statistics\nstatistics.variance(xs)"
)]
#[case::heap("heap_pushpop_replace", "import heapq\nheapq.heappushpop(h,x)")]
fn default_and_individual_exclusion(
    #[case] id: &str,
    #[case] source: &str,
    #[values(true, false)] enabled: bool,
) {
    // Arrange
    let mut operators = MutationOperatorSelection::default();
    if !enabled {
        let op = MutationOperator::from_name(id).expect("registered operator");
        operators.exclude(op);
    }
    let request = AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
        payload: PayloadBudget::default(),
    };

    // Act
    let output = analyze_source_cancellable(&request, source, || false).unwrap();

    // Assert
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|c| c.operator.as_str() == id)
            .count(),
        usize::from(enabled)
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
#[case::named_operands(
    "math_isclose_tolerance",
    "from math import isclose as c\nc(b=b, a=a)",
    1
)]
#[case::two_explicit(
    "math_isclose_tolerance",
    "import math\nmath.isclose(a,b,rel_tol=r,abs_tol=s)",
    2
)]
#[case::omitted_relative(
    "math_isclose_tolerance",
    "import math\nmath.isclose(a,b,abs_tol=s)",
    2
)]
#[case::signed_zeros(
    "math_isclose_tolerance",
    "import math\nmath.isclose(a,b,rel_tol=-0.,abs_tol=+0x0)",
    0
)]
#[case::false_zero(
    "math_isclose_tolerance",
    "import math\nmath.isclose(a,b,rel_tol=False)",
    0
)]
#[case::positional_tolerance("math_isclose_tolerance", "import math\nmath.isclose(a,b,1)", 0)]
#[case::duplicate_operand("math_isclose_tolerance", "import math\nmath.isclose(a,b,a=c)", 0)]
#[case::unknown_policy(
    "math_isclose_tolerance",
    "import math\nmath.isclose(a,b,absolute=s)",
    0
)]
#[case::precision_alias("math_precision_loss", "from math import log1p as l\nl(x)", 1)]
#[case::precision_keyword("math_precision_loss", "import math\nmath.expm1(x=x)", 0)]
#[case::precision_unrelated("math_precision_loss", "import math\nmath.log(x)", 0)]
#[case::precision_rebound(
    "math_precision_loss",
    "from math import log1p as l\ndef f(l): return l(x)",
    0
)]
#[case::precision_escape("math_precision_loss", "from math import log1p as l\nsend(l)\nl(x)", 0)]
#[case::source_write("math_precision_loss", "import math\nmath.expm1=f\nmath.expm1(x)", 0)]
#[case::destination_write("math_precision_loss", "import math as m\nm.exp=f\nm.expm1(x)", 0)]
#[case::fallback_shadow(
    "math_precision_loss",
    "from math import expm1\n__import__=f\nexpm1(x)",
    0
)]
#[case::stable_sibling(
    "math_precision_loss",
    "from math import expm1, exp as e\n__import__=f\nexpm1(x)",
    1
)]
#[case::sample_center(
    "statistics_sample_population",
    "from statistics import stdev as s\ns(xbar=y, data=x)",
    1
)]
#[case::population_center(
    "statistics_sample_population",
    "import statistics as s\ns.pvariance(data=x,mu=c)",
    1
)]
#[case::wrong_center(
    "statistics_sample_population",
    "import statistics as s\ns.pstdev(x,xbar=c)",
    0
)]
#[case::duplicate_center(
    "statistics_sample_population",
    "import statistics as s\ns.variance(x,c,xbar=c)",
    0
)]
#[case::kwargs(
    "statistics_sample_population",
    "import statistics as s\ns.variance(x,**kw)",
    0
)]
#[case::stats_escaped_destination(
    "statistics_sample_population",
    "import statistics as s\nexpose(s.variance)\ns.pvariance(x)",
    0
)]
#[case::heap_alias(
    "heap_pushpop_replace",
    "from heapq import heapreplace as r\nr(h,i)",
    1
)]
#[case::heap_missing("heap_pushpop_replace", "import heapq\nheapq.heapreplace(h)", 0)]
#[case::heap_keyword(
    "heap_pushpop_replace",
    "import heapq\nheapq.heappushpop(heap=h,item=i)",
    0
)]
#[case::heap_star("heap_pushpop_replace", "import heapq\nheapq.heapreplace(*pair)", 0)]
#[case::heap_escape(
    "heap_pushpop_replace",
    "import heapq as h\nsave(h)\nh.heapreplace(a,b)",
    0
)]
fn eligible_calls(#[case] id: &str, #[case] source: &str, #[case] count: usize) {
    // Arrange
    let cap = 10;
    // Act
    let output = analyze(source, id, &[], cap);
    // Assert
    assert_eq!(output.candidates.len(), count);
}

#[rstest::rstest]
#[case::omitted_call(2, 1)]
#[case::operand_line(3, 0)]
#[case::explicit_value(4, 1)]
fn line_selection(#[case] line: u32, #[case] count: usize) {
    // Arrange
    let source = "from math import isclose\nisclose \\\n (a,b,\n abs_tol=margin)";
    let lines = [hoimin_core::LineRange {
        start: line,
        end: line,
    }];
    // Act
    let output = analyze(source, "math_isclose_tolerance", &lines, 10);
    // Assert
    assert_eq!(output.candidates.len(), count);
    if let Some(c) = output.candidates.first() {
        assert_eq!(c.replacement.contains("rel_tol="), line == 2);
    }
}

proptest::proptest! {
    #[test]
    fn cap_retains_ranked_prefix(n in 1..12usize, cap in 1..18usize) {
        // Arrange
        let source = format!("from math import isclose\n{}", "isclose(a,b,abs_tol=x)\n".repeat(n));
        let full = analyze(&source, "math_isclose_tolerance", &[], 100);
        // Act
        let capped = analyze(&source, "math_isclose_tolerance", &[], cap);
        // Assert
        proptest::prop_assert_eq!(full.candidates.len(), 2*n);
        proptest::prop_assert_eq!(capped.candidates, &full.candidates[..cap.min(2*n)]);
    }
}

#[rstest::rstest]
#[case::exp("math_precision_loss", "math.expm1(x)", 199, 1)]
#[case::log("math_precision_loss", "math.log1p(x)", 199, 1)]
#[case::tolerance("math_isclose_tolerance", "math.isclose(a,b,rel_tol=x)", 198, 2)]
fn parser_limit_excludes_deeper_edit(
    #[case] id: &str,
    #[case] expression: &str,
    #[case] grouping: usize,
    #[case] extra: usize,
) {
    // Arrange
    let source = format!(
        "import math\n{}{expression}{}",
        "(".repeat(grouping),
        ")".repeat(grouping)
    );
    assert!(grouping + extra + 1 > 200);
    // Act
    let output = analyze(&source, id, &[], 10);
    // Assert
    assert_eq!(output.candidates.len(), 0, "{id}: {expression}");
}

#[rstest::rstest]
#[case::signed_tolerance(
    "math_isclose_tolerance",
    "import math\nmath.isclose(a,b, rel_tol=+(\n # preserve\n 0.1))"
)]
#[case::implicit_generator(
    "math_precision_loss",
    "import math as m\nm.log1p(x for x in xs # preserve\n)"
)]
#[case::grouped_callee(
    "math_precision_loss",
    "from math import expm1 as e\n((e) # preserve\n)(x,)"
)]
#[case::keyword_center(
    "statistics_sample_population",
    "import statistics as s\ns.variance(\n # preserve\n xbar=2,data=xs)"
)]
#[case::omitted_tolerance(
    "math_isclose_tolerance",
    "import math\nmath.isclose(a,b, # preserve\n)"
)]
fn edits_preserve_comments_and_multiline_syntax(#[case] id: &str, #[case] source: &str) {
    // Arrange
    let source = source.replace('\n', "\r\n");
    let out = analyze(&source, id, &[], 10);
    assert_eq!(out.candidates.len(), 1);
    let mut mutated = source.clone();
    let c = &out.candidates[0];
    let start = usize::try_from(c.span.start).unwrap();
    // Act
    mutated.replace_range(
        start..start + usize::try_from(c.span.length).unwrap(),
        &c.replacement,
    );
    // Assert
    assert!(
        ruff_python_parser::parse_module(&mutated).is_ok(),
        "{mutated}"
    );
    assert_eq!(mutated.matches("# preserve\r\n").count(), 1, "{mutated}");
}

#[rstest::rstest]
#[case::expm1("from math import expm1 as e", "e(x)", "math_precision_loss", 198)]
#[case::log1p("from math import log1p as l", "l(x)", "math_precision_loss", 198)]
#[case::explicit_tolerance(
    "from math import isclose as c",
    "c(a,b,rel_tol=x)",
    "math_isclose_tolerance",
    197
)]
fn parser_limit_admits_exact_fit(
    #[case] imports: &str,
    #[case] expression: &str,
    #[case] id: &str,
    #[case] grouping: usize,
) {
    // Arrange
    let source = format!(
        "{imports}\n{}{expression}{}",
        "(".repeat(grouping),
        ")".repeat(grouping)
    );
    // Act
    let output = analyze(&source, id, &[], 10);
    // Assert
    assert_eq!(output.candidates.len(), 1);
}

#[rstest::rstest]
#[case::member("from math import exp as ｉｆ, expm1 as tiny")]
#[case::module("import math as ａｓｙｎｃ\nfrom math import expm1 as tiny")]
fn unsafe_normalized_destination_without_fallback(#[case] imports: &str) {
    // Arrange
    let source = format!("{imports}\n__import__=None\nout=tiny(x)");
    // Act
    let out = analyze(&source, "math_precision_loss", &[], 10);
    // Assert
    assert_eq!(out.candidates.len(), 0, "{imports}");
}
