//! Public-API probe: fixtures and digest construction are outside measurement.
mod requested_heap;

use camino::Utf8Path;
use hoimin_core::{
    CANDIDATE_SCHEMA_VERSION, CandidateIdentity, LineRange, MutationOperator,
    MutationOperatorSelection, MutationProfile, stable_mutant_id,
};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
use std::cell::Cell;
use std::time::Instant;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let keywords = args[1].parse::<usize>().unwrap();
    let mode = &args[2];
    let limit = args[3].parse::<usize>().unwrap();
    let payload_bytes = args[4].parse::<usize>().unwrap();
    let iterations = args[5].parse::<usize>().unwrap();
    assert!(iterations > 0);

    let arguments = (0..keywords)
        .map(|index| format!("k{index}=0"))
        .collect::<Vec<_>>()
        .join(",");
    let source = match mode.as_str() {
        "optional-child" => "def f(a=0, b=0): pass\nf(\n a=f(a=3),\n b=4,\n)\n".to_owned(),
        "boolean-child" => "def f(a=False): pass\nf(\n a=True,\n)\n".to_owned(),
        "symbol" => {
            "def f(a=0, b=0): pass\nf(a=9, b=8)\ndef chosen():\n return f(\n  b=2, a=1,\n )\n"
                .to_owned()
        }
        _ => format!("def f({arguments}): pass\nf({arguments})\nchosen = True\n"),
    };
    let lines = match mode.as_str() {
        "excluded" | "optional-child" | "boolean-child" => vec![LineRange { start: 3, end: 3 }],
        "selected" => vec![LineRange { start: 2, end: 2 }],
        "symbol" => vec![LineRange { start: 4, end: 4 }],
        "reversed" => vec![LineRange { start: 3, end: 2 }],
        "all" => Vec::new(),
        _ => panic!("unknown mode"),
    };
    let symbols = if mode == "symbol" {
        vec!["pkg.subject:chosen".to_owned()]
    } else {
        Vec::new()
    };
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        for operator in MutationOperatorSelection::parse_selector(&name).unwrap() {
            operators.exclude(operator);
        }
    }
    operators.include(MutationOperator::OptionalKeywordDelete);
    if mode == "boolean-child" {
        operators.include(MutationOperator::BooleanLiteral);
    }
    let request = AnalyzeRequest {
        payload: PayloadBudget::new(payload_bytes),
        path: Utf8Path::new("pkg/subject.py"),
        lines: &lines,
        symbols: &symbols,
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: limit,
    };
    requested_heap::self_check();
    drop(analyze_source_cancellable(&request, &source, || false));
    let checks = Cell::new(0usize);

    let baseline = requested_heap::begin();
    let start = Instant::now();
    let mut result = None;
    for _ in 0..iterations {
        drop(result.take());
        result = Some(analyze_source_cancellable(
            std::hint::black_box(&request),
            std::hint::black_box(&source),
            || {
                checks.set(checks.get() + 1);
                false
            },
        ));
    }
    let elapsed_ns = start.elapsed().as_nanos();
    let heap = requested_heap::finish(baseline);

    let (identity, count, diagnostic_count, truncated, error, output_bytes) = match result.unwrap()
    {
        Ok(output) => {
            let ids = output
                .candidates
                .iter()
                .map(|candidate| {
                    stable_mutant_id(&CandidateIdentity {
                        schema_version: CANDIDATE_SCHEMA_VERSION,
                        file_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
                        path: candidate.path.clone(),
                        span: candidate.span,
                        operator: candidate.operator.as_str().to_owned(),
                        replacement: candidate.replacement.clone(),
                    })
                })
                .collect::<Vec<_>>();
            let identity = format!(
                "{:?}|{:?}|{}|{ids:?}",
                output
                    .candidates
                    .iter()
                    .map(|candidate| &**candidate)
                    .collect::<Vec<_>>(),
                output.diagnostics,
                output.truncated,
            );
            let bytes = output
                .candidates
                .iter()
                .map(|candidate| {
                    candidate.path.as_str().len()
                        + candidate.original.len()
                        + candidate.replacement.len()
                        + candidate.symbol.as_ref().map_or(0, String::len)
                })
                .sum::<usize>();
            (
                identity,
                output.candidates.len(),
                output.diagnostics.len(),
                output.truncated,
                None,
                bytes,
            )
        }
        Err(error) => {
            let error = format!("{error:?}");
            (error.clone(), 0, 0, false, Some(error), 0)
        }
    };
    println!(
        "{}",
        serde_json::json!({
            "keywords": keywords,
            "mode": mode,
            "max_candidates": limit,
            "payload_bytes": payload_bytes,
            "iterations": iterations,
            "source_bytes": source.len(),
            "source_blake3": blake3::hash(source.as_bytes()).to_hex().as_str(),
            "elapsed_ns": elapsed_ns,
            "cumulative_requested_bytes": heap.cumulative_requested_bytes,
            "allocation_calls": heap.allocation_calls,
            "live_before": heap.live_before,
            "live_after": heap.live_after,
            "retained_delta": heap.retained_delta,
            "peak_delta": heap.peak_delta,
            "cancellation_checks": checks.get(),
            "candidates": count,
            "diagnostics": diagnostic_count,
            "truncated": truncated,
            "error": error,
            "output_string_bytes": output_bytes,
            "identity_blake3": blake3::hash(identity.as_bytes()).to_hex().as_str(),
        })
    );
}
