//! Measure the actual private ranking module; public plan creation and all
//! candidate conversion happen before the allocator window.
use std::ffi::OsString;
use std::fmt::Write as _;

use hoimin_cli::cli::{ParsedCommand, parse_from};
use hoimin_cli::{plan, target::TargetHandler};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[path = "../src/plan/ranking.rs"]
mod ranking;

#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

#[test]
fn ranking_validation_heap_does_not_scale_with_valid_candidate_bodies() {
    const LIMIT: usize = 64 * 1024;
    let mut measurements = Vec::new();
    for payload_bytes in [32, 32 * 1024, 256 * 1024] {
        let project = tempfile::tempdir().unwrap();
        let payload = "x".repeat(payload_bytes);
        let mut source = String::new();
        for index in 0..64 {
            writeln!(source, "record_{index} = ['{payload}']").unwrap();
        }
        std::fs::write(project.path().join("sample.py"), source).unwrap();
        let args = vec![
            OsString::from("hoimin"),
            "plan".into(),
            "--root".into(),
            project.path().as_os_str().to_owned(),
            "--file".into(),
            "sample.py".into(),
            "--operators".into(),
            "collection_list_tuple".into(),
            "--allow-best-effort-memory".into(),
            "--min-free-space".into(),
            "1B".into(),
            "--".into(),
            "never-executed".into(),
        ];
        let ParsedCommand::Plan(command) = parse_from(args).unwrap() else {
            panic!("expected plan");
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let output = runtime
            .block_on(plan::create(command.into_run_config().unwrap()))
            .unwrap();
        assert_eq!(output.exit_code, 0);
        let manifest = output.manifest;
        assert_eq!(manifest.ranking_rule_version, ranking::RANKING_RULE_VERSION);
        assert_eq!(manifest.candidates.len(), 64);
        let targets = runtime
            .block_on(TargetHandler::resolve(
                &manifest.normalized_config.selection,
            ))
            .unwrap();
        drop(runtime);
        let candidates: Vec<ranking::RankedPlanCandidate> =
            serde_json::from_value(serde_json::to_value(manifest.candidates).unwrap()).unwrap();
        ranking::validate_ranking(&candidates).unwrap();
        for repeat in 0..3 {
            heap_tracking::begin();
            let result = ranking::validate_ranking_against(
                &manifest.normalized_config.selection,
                &targets,
                &candidates,
            );
            let peak = heap_tracking::finish();
            result.unwrap();
            eprintln!("ranking-peak payload={payload_bytes} repeat={repeat} bytes={peak}");
            measurements.push((payload_bytes, peak));
        }
        if payload_bytes == 256 * 1024 {
            heap_tracking::begin();
            let cloned = ranking::rank_candidates(
                &manifest.normalized_config.selection,
                &targets,
                candidates.iter().map(|row| row.candidate.clone()).collect(),
            );
            let peak = heap_tracking::finish();
            assert_eq!(cloned, candidates);
            assert!(peak > 32 * 1024 * 1024, "insensitive clone control: {peak}");
        }
    }
    for (payload_bytes, peak) in measurements {
        assert!(
            peak < LIMIT,
            "body-size-dependent ranking heap: payload={payload_bytes} peak={peak}"
        );
    }
}
