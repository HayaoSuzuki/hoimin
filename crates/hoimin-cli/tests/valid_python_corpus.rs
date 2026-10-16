#![allow(dead_code)]

#[path = "../src/analyzer/types.rs"]
mod types;
pub use types::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};
mod analyzer {
    pub use crate::types::AnalyzerDiagnosticCode;
}
#[path = "../src/analyzer/rust.rs"]
mod rust;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use camino::Utf8Path;
use hoimin_core::{
    CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, LineRange, MutationOperatorSelection,
    MutationProfile, validate_candidate,
};
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/valid-python.jsonl");
const REGISTRY: &str = include_str!("fixtures/valid-python-operators.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Site {
    anchor: String,
    original: String,
    replacement: String,
    eligible: bool,
    broken: Option<bool>,
    fault: String,
    expected_bytes: Vec<u8>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u32,
    seed: u32,
    mode: String,
    id: String,
    producer: String,
    position: String,
    binding: String,
    source: String,
    operator: String,
    sites: Vec<Site>,
    variants: bool,
    harness: String,
    baseline: String,
    mutant: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    operator: String,
    status: String,
    reason: String,
}

fn cases() -> Vec<Case> {
    CORPUS
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                "../../.venv/Scripts/python.exe"
            } else {
                "../../.venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

// CPython receives actual bytes, preserving BOM and physical newline spelling.
// Infrastructure failures and compile/runtime mismatches are separate results.
async fn python_observe(
    sources: &[Vec<u8>],
    harness: &str,
) -> Result<Vec<serde_json::Value>, String> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let input = serde_json::json!({"sources": sources, "harness": harness});
    let script = r"
import contextlib, io, json, sys
assert sys.version_info[:2] == (3, 14), sys.version
request = json.loads(sys.argv[1])
results = []
for source in request['sources']:
    observation = {'compiled': False, 'stdout': '', 'exception': None}
    try:
        code = compile(bytes(source), '<valid-python-corpus>', 'exec')
        observation['compiled'] = True
        if request['harness']:
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                namespace = {}
                exec(code, namespace)
                exec(request['harness'], namespace)
            observation['stdout'] = out.getvalue()
    except Exception as error:
        observation['exception'] = type(error).__name__
    results.append(observation)
print(json.dumps(results))
";
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args(["-B", "-c", script, &input.to_string()])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "infrastructure-error: CPython deadline".to_owned())?
    .map_err(|e| format!("infrastructure-error: CPython launch: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "infrastructure-error: CPython version/process: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("infrastructure-error: CPython response: {e}"))
}

fn layout(source: &str, variant: &str) -> String {
    match variant {
        "lf" => source.to_owned(),
        "crlf" => source.replace('\n', "\r\n"),
        "cr" => source.replace('\n', "\r"),
        "mixed" => {
            source
                .split_inclusive('\n')
                .enumerate()
                .fold(String::new(), |mut output, (i, line)| {
                    output.push_str(line.trim_end_matches('\n'));
                    output.push_str(["\r", "\r\n", "\n"][i % 3]);
                    output
                })
        }
        "bom" => format!("\u{feff}{source}"),
        "no-final-newline" => source.trim_end_matches('\n').to_owned(),
        _ => panic!("unregistered layout {variant}"),
    }
}

// Independent physical-line scan: do not reuse the production location helper.
fn location(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 1;
    let mut column = 0;
    let mut previous_cr = false;
    for (index, ch) in source[..offset].char_indices() {
        match ch {
            '\r' => {
                line += 1;
                column = 0;
                previous_cr = true;
            }
            '\n' => {
                if !previous_cr {
                    line += 1;
                }
                column = 0;
                previous_cr = false;
            }
            '\u{feff}' if index == 0 => {
                previous_cr = false;
            }
            _ => {
                column += 1;
                previous_cr = false;
            }
        }
    }
    (line, column)
}
fn site_start(source: &str, site: &Site) -> usize {
    assert_eq!(
        source.matches(&site.anchor).count(),
        1,
        "nonunique/missing anchor {}",
        site.anchor
    );
    assert_eq!(
        site.anchor.matches(&site.original).count(),
        1,
        "ambiguous original"
    );
    source.find(&site.anchor).unwrap() + site.anchor.find(&site.original).unwrap()
}
fn descriptor(source: &str, c: &AnalyzerCandidate) -> CandidateDescriptor {
    CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: c.path.clone(),
        span: c.span,
        original: c.original.clone(),
        replacement: c.replacement.clone(),
        operator: c.operator.as_str().to_owned(),
        line: c.line,
        column: c.column,
        symbol: c.symbol.clone(),
        file_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
    }
}
type Observation = (usize, String, String);
fn exact_candidates(actual: &[AnalyzerCandidate], expected: &[Observation]) -> Result<(), String> {
    let mut observed: Vec<_> = actual
        .iter()
        .map(|c| {
            (
                usize::try_from(c.span.start).unwrap(),
                c.original.clone(),
                c.replacement.clone(),
            )
        })
        .collect();
    observed.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    if observed == expected {
        Ok(())
    } else {
        Err(format!(
            "mismatch: expected {expected:?}, observed {observed:?}"
        ))
    }
}

async fn planned_candidates(
    case: &Case,
    source: &str,
    selection: &str,
    cap: usize,
    lines: &[LineRange],
    symbols: &[String],
    truncated: bool,
) -> BTreeMap<String, CandidateDescriptor> {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source.as_bytes()).unwrap();
    let mut args: Vec<OsString> = vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        root.path().as_os_str().to_owned(),
        "--source".into(),
        ".".into(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        if selection == "excluded" {
            format!("{},break_continue", case.operator).into()
        } else {
            case.operator.clone().into()
        },
        "--max-candidates".into(),
        cap.to_string().into(),
        "--profile".into(),
        if selection == "focused" {
            "focused".into()
        } else {
            "full".into()
        },
        "--allow-best-effort-memory".into(),
    ];
    if selection == "excluded" {
        args.extend(["--exclude-operators".into(), case.operator.clone().into()]);
    }
    for range in lines {
        args.extend([
            "--line".into(),
            format!("subject.py:{}-{}", range.start, range.end).into(),
        ]);
    }
    for symbol in symbols {
        args.extend(["--symbol".into(), format!("subject:{symbol}").into()]);
    }
    args.extend([
        "--".into(),
        python().into_os_string(),
        "-c".into(),
        "pass".into(),
    ]);
    let process = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(args.iter().skip(1))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("infrastructure-error: plan deadline")
    .expect("infrastructure-error: plan launch");
    assert_eq!(
        process.status.code(),
        Some(i32::from(truncated) * 4),
        "plan {} {selection}: {}",
        case.id,
        String::from_utf8_lossy(&process.stderr)
    );
    let plan: hoimin_cli::plan::PlanManifest = serde_json::from_slice(&process.stdout).unwrap();
    assert_eq!(plan.truncated, truncated);
    let plan_ids: BTreeMap<_, _> = plan
        .candidates
        .iter()
        .map(|ranked| {
            let c = &ranked.candidate;
            let desc = CandidateDescriptor {
                schema_version: CANDIDATE_SCHEMA_VERSION,
                path: c.path.clone(),
                span: c.span,
                original: c.original.clone(),
                replacement: c.replacement.clone(),
                operator: c.operator.as_str().to_owned(),
                line: c.line,
                column: c.column,
                symbol: c.symbol.clone(),
                file_hash: c.file_hash.clone(),
            };
            let id = validate_candidate(source.as_bytes(), &desc).unwrap();
            assert_eq!(id.as_str(), c.id);
            (id.as_str().to_owned(), desc)
        })
        .collect();
    assert_eq!(
        plan_ids.len(),
        plan.candidates.len(),
        "duplicate plan candidates"
    );
    plan_ids
}

#[expect(
    clippy::too_many_lines,
    reason = "one bounded fixture cross-product preserves the sequence of independent compiler, analyzer, validator and plan observations"
)]
async fn corpus_correspondence(
    mode: &str,
    check_eligibility: fn(&Case, &[AnalyzerCandidate], &[Observation]),
) {
    let mut seen_pairs = BTreeSet::new();
    let mut axis_values: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut checks = 0;
    let mut validated_candidates = 0;
    let mut runtime_candidates = 0;
    for case in cases().into_iter().filter(|case| case.mode == mode) {
        assert_eq!((case.schema, case.seed), (1, 489));
        assert!(matches!(case.mode.as_str(), "strict" | "model-only"));
        assert!(
            !case.sites.is_empty(),
            "zero-output must declare ineligible sites"
        );
        let layouts: &[&str] = [
            &["lf"][..],
            &["lf", "crlf", "cr", "mixed", "bom", "no-final-newline"][..],
        ][usize::from(case.variants)];
        for layout_name in layouts {
            let source = layout(&case.source, layout_name);
            let original = python_observe(&[source.as_bytes().to_vec()], &case.harness)
                .await
                .unwrap();
            assert_eq!(
                original[0]["compiled"], true,
                "invalid fixture {} {layout_name}: {original:?}",
                case.id
            );
            assert!(
                original[0]["exception"].is_null(),
                "runtime fixture {}: {original:?}",
                case.id
            );
            assert_eq!(original[0]["stdout"], case.baseline);
            let first_line = location(&source, site_start(&source, &case.sites[0])).0;
            let chosen = MutationOperatorSelection::parse_selector(&case.operator).unwrap()[0];
            let selections = [
                ("full", MutationProfile::Full, chosen, vec![], vec![], 100),
                (
                    "focused",
                    MutationProfile::Focused,
                    chosen,
                    vec![],
                    vec![],
                    100,
                ),
                (
                    "excluded",
                    MutationProfile::Full,
                    hoimin_core::MutationOperator::BreakContinue,
                    vec![],
                    vec![],
                    100,
                ),
                (
                    "line-hit",
                    MutationProfile::Full,
                    chosen,
                    vec![LineRange {
                        start: first_line,
                        end: first_line,
                    }],
                    vec![],
                    100,
                ),
                (
                    "line-miss",
                    MutationProfile::Full,
                    chosen,
                    vec![LineRange {
                        start: 999,
                        end: 999,
                    }],
                    vec![],
                    100,
                ),
                (
                    "symbol-hit",
                    MutationProfile::Full,
                    chosen,
                    vec![],
                    vec!["subject".to_owned()],
                    100,
                ),
                (
                    "symbol-miss",
                    MutationProfile::Full,
                    chosen,
                    vec![],
                    vec!["empty".to_owned()],
                    100,
                ),
                ("cap-1", MutationProfile::Full, chosen, vec![], vec![], 1),
                ("cap-2", MutationProfile::Full, chosen, vec![], vec![], 2),
                ("cap-3", MutationProfile::Full, chosen, vec![], vec![], 3),
            ];
            for (selection, profile, included, lines, symbols, cap) in selections
                .into_iter()
                .take([1, 10][usize::from(case.variants)])
            {
                let mut operators = MutationOperatorSelection::default();
                for name in MutationOperatorSelection::valid_names() {
                    for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
                        operators.exclude(operator);
                    }
                }
                operators.include(included);
                let mut expected: Vec<_> = case
                    .sites
                    .iter()
                    .filter(|s| s.eligible)
                    .map(|site| {
                        (
                            site_start(&source, site),
                            site.original.clone(),
                            site.replacement.clone(),
                        )
                    })
                    .filter(|(start, _, _)| {
                        !matches!(selection, "excluded" | "line-miss" | "symbol-miss")
                            && (selection != "line-hit"
                                || location(&source, *start).0 == first_line)
                    })
                    .collect();
                expected.sort();
                let truncated = expected.len() > cap;
                expected.truncate(cap);
                let output = rust::analyze_source(
                    &rust::AnalyzeRequest {
                        path: Utf8Path::new("subject.py"),
                        lines: &lines,
                        symbols: &symbols,
                        operators: &operators,
                        profile,
                        max_candidates: cap,
                    },
                    &source,
                );
                check_eligibility(&case, &output.candidates, &expected);
                assert_eq!(output.truncated, truncated);

                let mut ids = BTreeMap::new();
                let mut mutants = Vec::new();
                for c in &output.candidates {
                    assert_eq!(c.operator.as_str(), case.operator);
                    let start = usize::try_from(c.span.start).unwrap();
                    let end = start + usize::try_from(c.span.length).unwrap();
                    assert_eq!((c.line, c.column), location(&source, start));
                    assert_eq!(&source[start..end], c.original);
                    let desc = descriptor(&source, c);
                    let id = validate_candidate(source.as_bytes(), &desc).unwrap();
                    assert!(
                        ids.insert(id.as_str().to_owned(), desc).is_none(),
                        "duplicate raw candidate"
                    );
                    let mut bytes = source.as_bytes().to_vec();
                    bytes.splice(start..end, c.replacement.bytes());
                    mutants.push(bytes);
                }
                let observations = python_observe(&mutants, &case.harness).await.unwrap();
                for observation in observations {
                    assert_eq!(
                        observation["compiled"], true,
                        "mutant compile {}: {observation}",
                        case.id
                    );
                    assert!(
                        observation["exception"].is_null(),
                        "operator runtime contract {}: {observation}",
                        case.id
                    );
                    assert_eq!(observation["stdout"], case.mutant, "{}", case.id);
                }
                let plan_ids =
                    planned_candidates(&case, &source, selection, cap, &lines, &symbols, truncated)
                        .await;
                assert_eq!(plan_ids.len(), output.candidates.len());
                assert_eq!(ids, plan_ids, "raw analyzer / plan mismatch");
                let axes = [
                    ("producer", case.producer.as_str()),
                    ("position", case.position.as_str()),
                    ("binding", case.binding.as_str()),
                    ("layout", *layout_name),
                    ("selection", selection),
                ];
                for (i, (a, av)) in axes.iter().enumerate() {
                    axis_values.entry(a).or_default().insert((*av).into());
                    for (b, bv) in axes.iter().skip(i + 1) {
                        seen_pairs.insert(format!("{a}={av}|{b}={bv}"));
                    }
                }
                validated_candidates += output.candidates.len();
                runtime_candidates +=
                    usize::from(!case.harness.is_empty()) * output.candidates.len();
                checks += 1;
            }
        }
        eprintln!(
            "seed=489 fixture={} producer={} position={} binding={} operator={} mode={} sites={} eligible={}",
            case.id,
            case.producer,
            case.position,
            case.binding,
            case.operator,
            case.mode,
            case.sites.len(),
            case.sites.iter().filter(|s| s.eligible).count()
        );
    }
    let axes = ["producer", "position", "binding", "layout", "selection"];
    let mut uncovered = 0;
    for (i, a) in axes.iter().enumerate() {
        for b in axes.iter().skip(i + 1) {
            for av in &axis_values[a] {
                for bv in &axis_values[b] {
                    let pair = format!("{a}={av}|{b}={bv}");
                    uncovered += usize::from(!seen_pairs.contains(&pair));
                    for missing in std::iter::once(&pair).filter(|pair| !seen_pairs.contains(*pair))
                    {
                        eprintln!(
                            "uncovered-pair {missing}: not exercised by bounded seed489; no support or correctness claim"
                        );
                    }
                }
            }
        }
    }
    eprintln!(
        "seed=489 checks={checks} validated_and_compiled_candidates={validated_candidates} runtime_candidates={runtime_candidates} observed_pairs={} uncovered_pairs={uncovered}",
        seen_pairs.len()
    );
}

fn strict_eligibility(case: &Case, candidates: &[AnalyzerCandidate], expected: &[Observation]) {
    exact_candidates(candidates, expected)
        .unwrap_or_else(|error| panic!("seed=489 {}: {error}", case.id));
}

fn model_eligibility(case: &Case, candidates: &[AnalyzerCandidate], expected: &[Observation]) {
    let result = exact_candidates(candidates, expected);
    eprintln!(
        "model-only fixture={} eligibility_observation={result:?}; no strict source-binding correspondence claim",
        case.id
    );
    assert!(
        matches!(
            case.id.as_str(),
            "annotation_builtin_source_boundary" | "annotation_generic_source_boundary"
        ),
        "classify new model-only boundaries explicitly"
    );
}

#[tokio::test]
async fn valid_python_strict_corpus_correspondence() {
    corpus_correspondence("strict", strict_eligibility).await;
}

#[tokio::test]
async fn valid_python_model_boundaries_are_explicit() {
    corpus_correspondence("model-only", model_eligibility).await;
}

fn full_lf_candidates(case: &Case) -> Vec<AnalyzerCandidate> {
    let mut operators = MutationOperatorSelection::default();
    for name in MutationOperatorSelection::valid_names() {
        for operator in MutationOperatorSelection::parse_selector(name).unwrap() {
            operators.exclude(operator);
        }
    }
    operators.include(MutationOperatorSelection::parse_selector(&case.operator).unwrap()[0]);
    rust::analyze_source(
        &rust::AnalyzeRequest {
            path: Utf8Path::new("subject.py"),
            lines: &[],
            symbols: &[],
            operators: &operators,
            profile: MutationProfile::Full,
            max_candidates: 100,
        },
        &case.source,
    )
    .candidates
}

#[test]
fn valid_python_lf_candidates_compose_the_lean_bytes() {
    for case in cases() {
        for candidate in full_lf_candidates(&case) {
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            let site = case
                .sites
                .iter()
                .find(|site| {
                    site_start(&case.source, site) == start
                        && site.replacement == candidate.replacement
                })
                .unwrap();
            let mut bytes = case.source.as_bytes().to_vec();
            bytes.splice(start..end, candidate.replacement.bytes());

            assert_eq!(bytes, site.expected_bytes, "Lean CandidateSpan composition");
        }
    }
}

#[test]
fn valid_python_strict_analyzer_detects_each_declared_broken_site() {
    for case in cases().into_iter().filter(|case| case.mode == "strict") {
        let candidates = full_lf_candidates(&case);
        for (site, broken) in case
            .sites
            .iter()
            .filter_map(|site| site.broken.map(|broken| (site, broken)))
        {
            let present = candidates.iter().any(|candidate| {
                candidate.span.start == u64::try_from(site_start(&case.source, site)).unwrap()
                    && candidate.replacement == site.replacement
            });

            assert_ne!(
                present, broken,
                "broken {} was not detected via actual analyzer/plan case {}",
                site.fault, case.id
            );
        }
    }
}

#[test]
fn valid_python_operator_inventory_requires_registration_and_positive_producers() {
    let registry: Vec<Registration> = serde_json::from_str(REGISTRY).unwrap();
    let registered: BTreeSet<_> = registry.iter().map(|r| r.operator.as_str()).collect();
    assert_eq!(registered.len(), registry.len());
    let canonical: BTreeSet<_> = MutationOperatorSelection::valid_names()
        .into_iter()
        .filter(|name| hoimin_core::MutationOperator::from_name(name).is_some())
        .collect();
    assert_eq!(
        registered, canonical,
        "register each new operator, including explicit deferred reason"
    );
    let cases = cases();
    let positions: BTreeSet<_> = cases.iter().map(|c| c.position.as_str()).collect();
    for position in [
        "expression",
        "default",
        "annotation",
        "match-key",
        "match-value",
        "match-guard",
        "except",
        "except-star",
        "subscript",
        "first-iterable",
        "comprehension-body",
    ] {
        assert!(
            positions.contains(position),
            "missing required syntax position {position}"
        );
    }
    let triples: BTreeSet<_> = cases
        .iter()
        .map(|c| (c.producer.as_str(), c.position.as_str(), c.binding.as_str()))
        .collect();
    for triple in [
        ("token", "match-key", "unshadowed"),
        ("ast", "expression", "generic-destination"),
        ("ast", "subscript", "unshadowed"),
        ("annotation", "annotation", "generic-source"),
        ("operator-import", "expression", "generic-source"),
        ("ast", "first-iterable", "walrus"),
        ("ast", "comprehension-body", "iteration-source"),
        ("ast", "expression", "global"),
        ("ast", "expression", "nonlocal"),
    ] {
        assert!(
            triples.contains(&triple),
            "missing known-risk triple {triple:?}"
        );
        eprintln!("known-risk-triple={triple:?}");
    }
    let ids: BTreeSet<_> = cases.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), cases.len());
    for r in registry {
        assert_ne!(r.reason, "");
        assert!(matches!(r.status.as_str(), "covered" | "deferred"));
        assert_eq!(
            r.status == "covered",
            cases.iter().any(|c| c.operator == r.operator)
        );
        eprintln!(
            "operator={} status={} reason={}",
            r.operator, r.status, r.reason
        );
    }
    for producer in ["token", "ast", "annotation", "operator-import"] {
        assert!(
            cases
                .iter()
                .any(|c| c.producer == producer && c.sites.iter().any(|s| s.eligible)),
            "vacuous {producer}"
        );
        assert!(
            cases
                .iter()
                .any(|c| c.producer == producer && c.sites.iter().any(|s| !s.eligible)),
            "missing negative control {producer}"
        );
    }
}

#[tokio::test]
async fn valid_python_adapter_rejects_missing_excess_and_corrupt_observations() {
    let case = cases()
        .into_iter()
        .find(|c| c.id == "token_expression")
        .unwrap();
    let site = &case.sites[0];
    let start = site_start(&case.source, site);
    let expected = vec![(start, site.original.clone(), site.replacement.clone())];
    assert!(exact_candidates(&[], &expected).is_err());
    let c = AnalyzerCandidate {
        path: "subject.py".into(),
        span: hoimin_core::ByteSpan {
            start: u64::try_from(start).unwrap(),
            length: 1,
        },
        original: site.original.clone(),
        replacement: site.replacement.clone(),
        operator: hoimin_core::MutationOperator::from_name(&case.operator).unwrap(),
        line: 2,
        column: 11,
        symbol: Some("subject".into()),
    };
    assert!(exact_candidates(&[c.clone(), c.clone()], &expected).is_err());
    let mut desc = descriptor(&case.source, &c);
    let loc = location(&case.source, start);
    desc.line = loc.0;
    desc.column = loc.1;
    assert!(validate_candidate(case.source.as_bytes(), &desc).is_ok());
    desc.file_hash = "0".repeat(64);
    assert!(validate_candidate(case.source.as_bytes(), &desc).is_err());
    desc = descriptor(&case.source, &c);
    desc.line = loc.0;
    desc.column = loc.1;
    desc.span.start += 1;
    assert!(validate_candidate(case.source.as_bytes(), &desc).is_err());
    let observation = python_observe(&[b"def invalid(:\n".to_vec()], "")
        .await
        .unwrap();
    assert_eq!(observation[0]["compiled"], false);
    assert_eq!(observation[0]["exception"], "SyntaxError");
}

#[tokio::test]
async fn slice_tuple_import_only_run_does_not_count_syntax_error_as_killed() {
    // With no ordinary tuple literals, every old candidate is a syntax-error false kill.
    let root = tempfile::tempdir().unwrap();
    let source = "def subject(x):\n    a = x[:,]\n    b = x[1:2, 3]\n    return x[:, :]\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--root"])
            .arg(root.path())
            .args([
                "--file",
                "subject.py",
                "--operators",
                "collection_list_tuple",
                "--format",
                "json",
                "--min-free-space",
                "1B",
                "--allow-best-effort-memory",
                "--",
            ])
            .arg(python())
            .args(["-B", "-c", "import subject"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("infrastructure-error: run deadline")
    .expect("infrastructure-error: run launch");
    assert!(
        result.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0, "{report}");
    assert_eq!(report["summary"]["counts"]["killed"], 0, "{report}");
    assert!(report["mutants"].as_array().unwrap().is_empty(), "{report}");
    assert_eq!(report["summary"]["complete"], true, "{report}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
        source
    );
}
