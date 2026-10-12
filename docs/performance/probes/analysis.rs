//! Copy to crates/hoimin-cli/examples/profile_analysis.rs.
use std::time::Instant;

use camino::Utf8PathBuf;
use hoimin_cli::analyzer::discover_targets;
use hoimin_core::{MutationOperatorSelection, MutationProfile, TargetSlice};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    let root = Utf8PathBuf::from_path_buf(std::fs::canonicalize(&args[1]).unwrap()).unwrap();
    let target = TargetSlice {
        path: "source.py".into(),
        lines: vec![],
        symbols: vec![],
    };
    let mut operators = MutationOperatorSelection::default();
    if args[2] != "default" {
        for name in operators.names() {
            for operator in MutationOperatorSelection::parse_selector(&name).unwrap() {
                operators.exclude(operator);
            }
        }
        for operator in MutationOperatorSelection::parse_selector(&args[2]).unwrap() {
            operators.include(operator);
        }
    }
    let profile = match args[3].as_str() {
        "full" => MutationProfile::Full,
        "focused" => MutationProfile::Focused,
        _ => panic!("unknown profile"),
    };
    for repetition in 0..7 {
        let start = Instant::now();
        let output = discover_targets(
            &root,
            std::slice::from_ref(&target),
            &operators,
            profile,
            10_000,
        )
        .await
        .unwrap();
        let elapsed = start.elapsed();
        let encoded = serde_json::to_vec(&output.candidates).unwrap();
        println!(
            "{}",
            serde_json::json!({
                "repetition": repetition, "seconds": elapsed.as_secs_f64(),
                "candidates": output.candidates.len(), "truncated": output.truncated,
                "diagnostics": format!("{:?}", output.diagnostics),
                "candidate_hash": blake3::hash(&encoded).to_hex().to_string(),
            })
        );
    }
}
