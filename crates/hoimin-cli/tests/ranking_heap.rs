//! Measure the actual private ranking module; public plan creation and all
//! candidate conversion happen before the allocator window.
use std::ffi::OsString;
use std::fmt::Write as _;
use std::sync::Mutex;

use hoimin_core::{PlanConfig, TargetSlice};

use hoimin_cli::cli::{ParsedCommand, parse_from};
use hoimin_cli::{plan, target::TargetHandler};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[path = "../src/plan/ranking.rs"]
mod ranking;

#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

static OBSERVATION: Mutex<()> = Mutex::new(());

struct Fixture {
    _project: tempfile::TempDir,
    config: PlanConfig,
    targets: Vec<TargetSlice>,
    candidates: Vec<ranking::RankedPlanCandidate>,
}

fn fixture(payload_bytes: usize) -> Fixture {
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
    let (config, discovery) = command.into_configs().unwrap();
    let output = runtime.block_on(plan::create(config, discovery)).unwrap();
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
    Fixture {
        _project: project,
        config: manifest.normalized_config,
        targets,
        candidates,
    }
}

fn assert_bounded_validation_heap(payload_bytes: usize) {
    // Arrange: isolate the global allocator across all fixture work and measurements.
    let _observation = OBSERVATION.lock().unwrap();
    let fixture = fixture(payload_bytes);

    for repeat in 0..3 {
        // Act: measure only production ranking validation, after public plan creation.
        heap_tracking::begin();
        let result = ranking::validate_ranking_against(
            &fixture.config.selection,
            &fixture.targets,
            &fixture.candidates,
        );
        let peak = heap_tracking::finish();

        // Assert: candidate body size must not cause proportional retained allocation.
        result.unwrap();
        eprintln!("ranking-peak payload={payload_bytes} repeat={repeat} bytes={peak}");
        assert!(peak < 64 * 1024, "body-size-dependent ranking heap: {peak}");
    }
}

#[test]
fn ranking_validation_heap_is_bounded_for_small_bodies() {
    assert_bounded_validation_heap(32);
}

#[test]
fn ranking_validation_heap_is_bounded_for_medium_bodies() {
    assert_bounded_validation_heap(32 * 1024);
}

#[test]
fn ranking_validation_heap_is_bounded_for_large_bodies() {
    assert_bounded_validation_heap(64 * 1024);
}

#[test]
fn ranking_heap_control_detects_cloned_candidate_bodies() {
    // Arrange: stay within discovery's retained-payload limit before measuring.
    let _observation = OBSERVATION.lock().unwrap();
    let fixture = fixture(64 * 1024);

    // Act: deliberately clone the same bodies through the actual ranking path.
    heap_tracking::begin();
    let cloned = ranking::rank_candidates(
        &fixture.config.selection,
        &fixture.targets,
        fixture
            .candidates
            .iter()
            .map(|row| row.candidate.clone())
            .collect(),
    );
    let peak = heap_tracking::finish();

    // Assert: the control exceeds the validation ceiling by more than 128 times.
    assert_eq!(cloned, fixture.candidates);
    assert!(peak > 8 * 1024 * 1024, "insensitive clone control: {peak}");
}
