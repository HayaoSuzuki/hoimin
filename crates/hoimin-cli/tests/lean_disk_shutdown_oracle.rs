use std::collections::BTreeSet;

use hoimin_cli::shell::ShellDiskLifecycle;
use hoimin_core::disk::{
    DiskCleanupOutcome, DiskCleanupState, DiskComponentState, DiskLifecycleEvent,
    DiskLifecycleSnapshot, DiskRootEntry, DiskRootId, DiskSecondary, DiskStopReason,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusCase {
    schema: u8,
    id: String,
    mode: String,
    layer: String,
    implementation_targets: Vec<String>,
    initial: State,
    events: Vec<Event>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    stop: Option<String>,
    secondary_stops: Vec<String>,
    active: u64,
    dispatched: u64,
    owned_roots: Vec<String>,
    delivery_roots: Vec<String>,
    cleanup_requested: Vec<String>,
    cleanup_clean: Vec<String>,
    cleanup_failed: Vec<String>,
    cleanup_deferred: Vec<String>,
    cleanup_retained: Vec<String>,
    process_drain: String,
    output_drain: String,
    monitor_join: String,
    report: String,
    finished: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    #[serde(flatten)]
    state: State,
    accepted: bool,
    rejected_at: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Dispatch,
    ProcessDrainSucceeded,
    ProcessDrainFailed,
    OutputDrained,
    OutputDrainFailed,
    MonitorJoined,
    MonitorJoinFailed,
    CleanupRequested {
        root: String,
    },
    CleanupSucceeded {
        root: String,
    },
    CleanupFailed {
        root: String,
    },
    CleanupDeferred {
        root: String,
    },
    CleanupRetained {
        root: String,
    },
    ReportSucceeded,
    ReportFailed,
    Finish,
    #[serde(other)]
    Unsupported,
}

#[test]
fn every_rust_runtime_oracle_case_executes_and_matches_the_complete_observation() {
    let cases = parse_corpus();
    let expected = cases
        .iter()
        .filter(|case| is_rust_runtime(case))
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut executed = BTreeSet::new();

    for case in cases.iter().filter(|case| is_rust_runtime(case)) {
        assert!(
            executed.insert(case.id.as_str()),
            "duplicate execution for {}",
            case.id
        );
        assert_runtime_case(case);
    }

    assert_eq!(executed, expected);
    assert_eq!(executed.len(), 12);
}

#[test]
fn runtime_oracle_rejects_unknown_duplicate_and_empty_targets() {
    let source = corpus_source();
    let runtime = source
        .lines()
        .map(|line| serde_json::from_str::<CorpusCase>(line).unwrap())
        .find(is_rust_runtime)
        .expect("Rust runtime case");

    let mut duplicate = runtime.implementation_targets.clone();
    duplicate.push("rust".to_owned());
    assert!(validate_targets(&duplicate).is_err());
    assert!(validate_targets(&[]).is_err());
    assert!(validate_targets(&["native".to_owned()]).is_err());
    assert!(validate_targets(&["python".to_owned()]).is_err());

    let first = source.lines().next().unwrap();
    let unknown = first.replacen("\"schema\":1", "\"schema\":1,\"extra\":true", 1);
    assert!(serde_json::from_str::<CorpusCase>(&unknown).is_err());
}

fn assert_runtime_case(case: &CorpusCase) {
    assert_initial_fixture(&case.initial, &case.id);
    let owned_roots = parse_roots(&case.initial.owned_roots, &case.id);
    let mut lifecycle = ShellDiskLifecycle::new(owned_roots).unwrap();

    // The internal two-active fixture begins after two workers have become active but
    // before mutation dispatch accounting. Seed the production lifecycle with two
    // dispatches, then subtract only that fixture setup from the final dispatch count.
    for _ in 0..case.initial.active {
        assert!(
            lifecycle.apply(DiskLifecycleEvent::DispatchRequested),
            "{} fixture dispatch",
            case.id
        );
    }
    let seeded_dispatches = case.initial.active;
    let mut accepted = true;
    let mut rejected_at = None;
    for (index, event) in case.events.iter().enumerate() {
        if !lifecycle.apply(runtime_event(event, &case.id)) {
            accepted = false;
            rejected_at = Some(index);
            break;
        }
    }

    assert_terminal_observation(
        &lifecycle.snapshot(),
        &case.expected,
        seeded_dispatches,
        accepted,
        rejected_at,
        &case.id,
    );
}

fn assert_terminal_observation(
    snapshot: &DiskLifecycleSnapshot,
    expected: &Expected,
    seeded_dispatches: u64,
    accepted: bool,
    rejected_at: Option<usize>,
    id: &str,
) {
    let actual_stop = snapshot
        .stop
        .as_ref()
        .map(|failure| stop_name(failure.reason.kind()).to_owned());
    let secondary_stops = snapshot
        .secondary
        .iter()
        .filter_map(|secondary| match secondary {
            DiskSecondary::Observation { reason, .. } => Some(stop_name(*reason).to_owned()),
            DiskSecondary::Error { code, .. } if code == hoimin_core::PROCESS_LIFECYCLE_FAILED => {
                Some("process_failed".to_owned())
            }
            DiskSecondary::Error { .. } => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(accepted, expected.accepted, "{id} accepted");
    assert_eq!(rejected_at, expected.rejected_at, "{id} rejected_at");
    assert_eq!(actual_stop, expected.state.stop, "{id} stop");
    assert_eq!(
        secondary_stops, expected.state.secondary_stops,
        "{id} secondary_stops"
    );
    assert_eq!(snapshot.active, expected.state.active, "{id} active");
    assert_eq!(
        snapshot.dispatched - seeded_dispatches,
        expected.state.dispatched,
        "{id} dispatched"
    );
    assert_root_observation(snapshot, &expected.state, id);
    assert_component_observation(snapshot, &expected.state, id);
}

fn assert_root_observation(snapshot: &DiskLifecycleSnapshot, expected: &State, id: &str) {
    // The Lean corpus uses abstract membership lists. Validate that abstraction
    // before building expected typed entries, rather than reintroducing old
    // production snapshot views. Diagnostic order is compared independently.
    for members in [
        &expected.cleanup_requested,
        &expected.cleanup_clean,
        &expected.cleanup_failed,
        &expected.cleanup_deferred,
        &expected.cleanup_retained,
    ] {
        let unique = members.iter().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), members.len(), "{id} duplicate membership");
        assert!(
            members
                .iter()
                .all(|root| expected.owned_roots.contains(root)),
            "{id} unowned membership"
        );
    }
    let delivery = expected
        .owned_roots
        .iter()
        .filter(|root| root.as_str() == "delivery")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(delivery, expected.delivery_roots, "{id} delivery identity");
    let entries = expected
        .owned_roots
        .iter()
        .map(|name| {
            let outcomes = [
                (
                    expected.cleanup_clean.contains(name),
                    DiskCleanupOutcome::Clean,
                ),
                (
                    expected.cleanup_failed.contains(name),
                    DiskCleanupOutcome::Failed("oracle fixture".into()),
                ),
                (
                    expected.cleanup_deferred.contains(name),
                    DiskCleanupOutcome::Deferred("oracle fixture".into()),
                ),
                (
                    expected.cleanup_retained.contains(name),
                    DiskCleanupOutcome::Retained,
                ),
            ];
            let mut completed = outcomes
                .into_iter()
                .filter_map(|(present, outcome)| present.then_some(outcome))
                .collect::<Vec<_>>();
            assert!(completed.len() <= 1, "{id} contradictory outcomes");
            let requested = expected.cleanup_requested.contains(name);
            let cleanup = if let Some(outcome) = completed.pop() {
                assert!(requested, "{id} unrequested outcome");
                DiskCleanupState::Completed(outcome)
            } else if requested {
                DiskCleanupState::Requested
            } else {
                DiskCleanupState::NotRequested
            };
            DiskRootEntry {
                root: parse_root(name, id),
                cleanup,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshot.roots, entries, "{id} registered root states");
}

fn assert_component_observation(snapshot: &DiskLifecycleSnapshot, expected: &State, id: &str) {
    assert_eq!(
        component_name(snapshot.process_drain),
        expected.process_drain,
        "{id} process_drain"
    );
    assert_eq!(
        component_name(snapshot.output_drain),
        expected.output_drain,
        "{id} output_drain"
    );
    assert_eq!(
        component_name(snapshot.monitor_join),
        expected.monitor_join,
        "{id} monitor_join"
    );
    assert_eq!(
        component_name(snapshot.report),
        expected.report,
        "{id} report"
    );
    assert_eq!(snapshot.finished, expected.finished, "{id} finished");
}

fn runtime_event(event: &Event, id: &str) -> DiskLifecycleEvent {
    match event {
        Event::Dispatch => DiskLifecycleEvent::DispatchRequested,
        Event::ProcessDrainSucceeded => DiskLifecycleEvent::ProcessDrainSucceeded,
        Event::ProcessDrainFailed => DiskLifecycleEvent::ProcessDrainFailed,
        Event::OutputDrained => DiskLifecycleEvent::OutputDrainSucceeded,
        Event::OutputDrainFailed => DiskLifecycleEvent::OutputDrainFailed,
        Event::MonitorJoined => DiskLifecycleEvent::MonitorJoinSucceeded,
        Event::MonitorJoinFailed => DiskLifecycleEvent::MonitorJoinFailed,
        Event::CleanupRequested { root } => DiskLifecycleEvent::CleanupRequested {
            root: parse_root(root, id),
        },
        Event::CleanupSucceeded { root } => DiskLifecycleEvent::CleanupCompleted {
            root: parse_root(root, id),
            outcome: DiskCleanupOutcome::Clean,
        },
        Event::CleanupFailed { root } => DiskLifecycleEvent::CleanupCompleted {
            root: parse_root(root, id),
            outcome: DiskCleanupOutcome::Failed("oracle fixture".to_owned()),
        },
        Event::CleanupDeferred { root } => DiskLifecycleEvent::CleanupCompleted {
            root: parse_root(root, id),
            outcome: DiskCleanupOutcome::Deferred("oracle fixture".to_owned()),
        },
        Event::CleanupRetained { root } => DiskLifecycleEvent::CleanupCompleted {
            root: parse_root(root, id),
            outcome: DiskCleanupOutcome::Retained,
        },
        Event::ReportSucceeded => DiskLifecycleEvent::ReportSucceeded,
        Event::ReportFailed => DiskLifecycleEvent::ReportFailed,
        Event::Finish => DiskLifecycleEvent::FinishRequested,
        Event::Unsupported => panic!("unsupported runtime event in {id}"),
    }
}

fn assert_initial_fixture(state: &State, id: &str) {
    assert_eq!(state.stop, None, "{id} initial stop");
    assert!(state.secondary_stops.is_empty(), "{id} initial secondary");
    assert_eq!(state.dispatched, 0, "{id} initial dispatched");
    assert!(state.cleanup_requested.is_empty(), "{id} initial request");
    assert!(state.cleanup_clean.is_empty(), "{id} initial clean");
    assert!(state.cleanup_failed.is_empty(), "{id} initial failed");
    assert!(state.cleanup_deferred.is_empty(), "{id} initial deferred");
    assert!(state.cleanup_retained.is_empty(), "{id} initial retained");
    assert_eq!(state.process_drain, "pending", "{id} initial process");
    assert_eq!(state.output_drain, "pending", "{id} initial output");
    assert_eq!(state.monitor_join, "pending", "{id} initial monitor");
    assert_eq!(state.report, "pending", "{id} initial report");
    assert!(!state.finished, "{id} initial finished");
    let derived_delivery = state
        .owned_roots
        .iter()
        .filter(|root| root.as_str() == "delivery")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        derived_delivery, state.delivery_roots,
        "{id} delivery roots"
    );
}

fn parse_corpus() -> Vec<CorpusCase> {
    let cases = corpus_source()
        .lines()
        .map(|line| serde_json::from_str::<CorpusCase>(line).unwrap())
        .collect::<Vec<_>>();
    let mut ids = BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1, "{} schema", case.id);
        assert_ne!(case.id, "");
        assert!(ids.insert(case.id.as_str()), "duplicate case {}", case.id);
        assert!(matches!(case.layer.as_str(), "policy" | "runtime"));
        assert!(matches!(
            case.mode.as_str(),
            "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
        ));
        validate_targets(&case.implementation_targets).unwrap();
    }
    cases
}

fn corpus_source() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl"
    ))
}

fn is_rust_runtime(case: &CorpusCase) -> bool {
    case.layer == "runtime"
        && case
            .implementation_targets
            .iter()
            .any(|target| target == "rust")
}

fn validate_targets(targets: &[String]) -> Result<(), &'static str> {
    let unique = targets.iter().collect::<BTreeSet<_>>();
    if unique.is_empty() || unique.len() != targets.len() {
        return Err("empty or duplicate implementation targets");
    }
    if unique.iter().any(|target| target.as_str() != "rust") {
        return Err("unknown implementation target");
    }
    Ok(())
}

fn parse_roots(roots: &[String], id: &str) -> Vec<DiskRootId> {
    roots.iter().map(|root| parse_root(root, id)).collect()
}

fn parse_root(root: &str, id: &str) -> DiskRootId {
    match root {
        "execution" => DiskRootId::Execution,
        "delivery" => DiskRootId::Delivery,
        value => panic!("unknown root {value} in {id}"),
    }
}

fn component_name(component: DiskComponentState) -> &'static str {
    match component {
        DiskComponentState::Pending => "pending",
        DiskComponentState::Succeeded => "succeeded",
        DiskComponentState::Failed => "failed",
    }
}

fn stop_name(reason: DiskStopReason) -> &'static str {
    match reason {
        DiskStopReason::WorkspaceSizeExceeded => "workspace_size_exceeded",
        DiskStopReason::FilesystemReserveReached => "filesystem_reserve_reached",
        DiskStopReason::MeasurementFailed => "measurement_failed",
        DiskStopReason::ProcessFailed => "process_failed",
    }
}

#[test]
fn typed_root_projection_rejects_unowned_membership() {
    // Arrange
    let mut expected = parse_corpus()
        .into_iter()
        .find(is_rust_runtime)
        .unwrap()
        .initial;
    expected.owned_roots = vec!["execution".into()];
    expected.delivery_roots.clear();
    expected.cleanup_clean = vec!["delivery".into()];
    let snapshot = ShellDiskLifecycle::new([DiskRootId::Execution])
        .unwrap()
        .snapshot();

    // Act
    let result = std::panic::catch_unwind(|| {
        assert_root_observation(&snapshot, &expected, "unowned fixture");
    });

    // Assert
    assert!(result.is_err());
}

#[test]
fn typed_root_projection_rejects_contradictory_outcomes() {
    // Arrange
    let mut expected = parse_corpus()
        .into_iter()
        .find(is_rust_runtime)
        .unwrap()
        .initial;
    expected.owned_roots = vec!["execution".into()];
    expected.delivery_roots.clear();
    expected.cleanup_requested = vec!["execution".into()];
    expected.cleanup_clean = vec!["execution".into()];
    expected.cleanup_retained = vec!["execution".into()];
    let mut snapshot = ShellDiskLifecycle::new([DiskRootId::Execution])
        .unwrap()
        .snapshot();
    snapshot.roots[0].cleanup = DiskCleanupState::Completed(DiskCleanupOutcome::Retained);

    // Act
    let result = std::panic::catch_unwind(|| {
        assert_root_observation(&snapshot, &expected, "contradictory fixture");
    });

    // Assert
    assert!(result.is_err());
}

#[test]
fn typed_root_projection_rejects_unrequested_outcome() {
    // Arrange
    let mut expected = parse_corpus()
        .into_iter()
        .find(is_rust_runtime)
        .unwrap()
        .initial;
    expected.owned_roots = vec!["execution".into()];
    expected.delivery_roots.clear();
    expected.cleanup_clean = vec!["execution".into()];
    let mut snapshot = ShellDiskLifecycle::new([DiskRootId::Execution])
        .unwrap()
        .snapshot();
    snapshot.roots[0].cleanup = DiskCleanupState::Completed(DiskCleanupOutcome::Clean);

    // Act
    let result = std::panic::catch_unwind(|| {
        assert_root_observation(&snapshot, &expected, "unrequested fixture");
    });

    // Assert
    assert!(result.is_err());
}
