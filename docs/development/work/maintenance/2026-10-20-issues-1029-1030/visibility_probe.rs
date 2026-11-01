//! Link this unchanged against each release revision. Setup and API-result
//! characterization are outside the timed/allocator-metered index construction.
mod requested_heap;

use std::cell::Cell;
use std::fmt::Write as _;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    CANDIDATE_SCHEMA_VERSION, CandidateIdentityRef, MutationOperatorSelection, MutationProfile,
};
use hoimin_python::{
    AnalyzeRequest, ExceptionIndex, analyze_source_with_exceptions, payload::PayloadBudget,
};
use serde_json::{Value, json};

fn output(index: &ExceptionIndex, sources: &[(Utf8PathBuf, String)], limit: usize) -> Value {
    let (_, source) = sources
        .iter()
        .find(|(path, _)| path == "service.py")
        .unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        for operator in MutationOperatorSelection::parse_selector(&name).unwrap() {
            operators.exclude(operator);
        }
    }
    for operator in MutationOperatorSelection::parse_selector("exception_hierarchy").unwrap() {
        operators.include(operator);
    }
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("service.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: limit,
    };
    let result = analyze_source_with_exceptions(&request, source, &|| false, Some(index)).unwrap();
    let file_hash = blake3::hash(source.as_bytes()).to_hex().to_string();
    let candidates = result
        .candidates
        .iter()
        .map(|candidate| {
            let id = CandidateIdentityRef {
                schema_version: CANDIDATE_SCHEMA_VERSION,
                file_hash: &file_hash,
                path: &candidate.path,
                span: candidate.span,
                operator: candidate.operator.as_str(),
                replacement: &candidate.replacement,
            }
            .stable_mutant_id();
            json!({
                "id": id.as_str(), "path": candidate.path, "span": candidate.span,
                "original": candidate.original, "replacement": candidate.replacement,
                "operator": candidate.operator.as_str(), "line": candidate.line,
                "column": candidate.column, "symbol": candidate.symbol,
            })
        })
        .collect::<Vec<_>>();
    let diagnostics = result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            json!({
                "code": diagnostic.code.canonical_name(), "path": diagnostic.path,
                "line": diagnostic.line, "column": diagnostic.column, "message": diagnostic.message,
            })
        })
        .collect::<Vec<_>>();
    json!({"candidates": candidates, "diagnostics": diagnostics, "truncated": result.truncated})
}

fn sources(ordinary: usize, aliases: usize, classes: usize) -> Vec<(Utf8PathBuf, String)> {
    let mut errors = String::new();
    for member in 0..ordinary {
        writeln!(errors, "ordinary{member:05} = 0").unwrap();
    }
    for class in 0..classes {
        let base = if class == 0 {
            "Exception"
        } else {
            "Error00000"
        };
        writeln!(errors, "class Error{class:05}({base}): pass").unwrap();
    }
    let mut service = String::new();
    for alias in 0..aliases {
        writeln!(service, "import errors as alias{alias:05}").unwrap();
    }
    if classes > 0 && aliases > 0 {
        service.push_str("def target():\n    raise alias00000.Error00000()\n");
    }
    vec![("errors.py".into(), errors), ("service.py".into(), service)]
}

fn contracts() {
    let mut results = Vec::new();
    for scenario in [
        "loaded",
        "unloaded",
        "shadowed_alias",
        "shadowed_class",
        "reexport",
        "ambiguous",
        "diagnostic",
    ] {
        let mut errors =
            "from pkg import Root\nclass Child(Root): pass\nclass Other(Root): pass\n".to_owned();
        if scenario == "shadowed_class" {
            errors.push_str("Child = 0\n");
        }
        let mut service = "import pkg as p\nfrom pkg import Root\n".to_owned();
        if !matches!(scenario, "unloaded" | "reexport") {
            service.push_str("import pkg.errors as zz\n");
        }
        if scenario == "shadowed_alias" {
            service.push_str("p = 0\n");
        }
        if scenario == "reexport" {
            service.push_str("import facade as f\n");
        }
        if scenario == "ambiguous" {
            service.push_str("import errors as ambiguous\n");
        }
        service.push_str("def target():\n    raise Root()\n");
        if scenario == "diagnostic" {
            service.push_str("    raise missing()\n");
        }
        let sources = vec![
            (
                "pkg/__init__.py".into(),
                "class Root(Exception): pass\n".into(),
            ),
            ("pkg/errors.py".into(), errors),
            ("facade.py".into(), "from pkg.errors import Child\n".into()),
            ("service.py".into(), service),
        ];
        let roots = if scenario == "ambiguous" {
            vec![".".into(), "pkg".into()]
        } else {
            vec![".".into()]
        };
        let index =
            ExceptionIndex::from_sources_with_inputs(&sources, &roots, &[], &|| false).unwrap();
        for limit in [0, 1, 2, 100] {
            results.push(json!({"scenario": scenario, "limit": limit, "output": output(&index, &sources, limit)}));
        }
    }
    println!("{}", json!({"contracts": results}));
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    requested_heap::self_check();
    if args[1] == "contracts" {
        contracts();
        return;
    }
    let ordinary: usize = args[1].parse().unwrap();
    let aliases: usize = args[2].parse().unwrap();
    let classes: usize = args[3].parse().unwrap();
    let sources = sources(ordinary, aliases, classes);
    let roots = vec![".".into()];
    // One index warmup per process; it is dropped before the measurement.
    drop(ExceptionIndex::from_sources_with_inputs(&sources, &roots, &[], &|| false).unwrap());
    let polls = Cell::new(0usize);
    let baseline = requested_heap::begin();
    let start = Instant::now();
    let index = ExceptionIndex::from_sources_with_inputs(&sources, &roots, &[], &|| {
        polls.set(polls.get() + 1);
        false
    })
    .unwrap();
    let elapsed_ns = start.elapsed().as_nanos();
    let heap = requested_heap::finish(baseline);
    let output = output(&index, &sources, 100);
    let output_digest = blake3::hash(&serde_json::to_vec(&output).unwrap())
        .to_hex()
        .to_string();
    let source_digest = blake3::hash(&serde_json::to_vec(&sources).unwrap())
        .to_hex()
        .to_string();
    println!(
        "{}",
        json!({
            "ordinary": ordinary, "aliases": aliases, "classes": classes,
            "elapsed_ns": elapsed_ns, "cancellation_polls": polls.get(),
            "cumulative_requested_bytes": heap.cumulative_requested_bytes,
            "allocation_calls": heap.allocation_calls,
            "live_before": heap.live_before, "live_after": heap.live_after,
            "retained_requested_bytes": heap.retained_delta, "peak_requested_bytes": heap.peak_delta,
            "source_digest": source_digest, "output_digest": output_digest, "output": output,
        })
    );
    std::hint::black_box(index);
}
