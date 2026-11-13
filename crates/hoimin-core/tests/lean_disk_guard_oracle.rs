#![cfg_attr(coverage_nightly, feature(coverage_attribute, stmt_expr_attributes))]
use std::collections::BTreeSet;
use std::num::NonZeroU64;
use std::time::Duration;

use hoimin_core::disk::{
    DiskDecision, DiskLifecycle, DiskLifecycleEvent, DiskObservation, DiskPolicy, DiskSecondary,
    DiskStopReason,
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
    Observe {
        owned: u64,
        max_owned: u64,
        free: u64,
        min_free: u64,
    },
    MeterFailed,
    Dispatch,
    #[serde(other)]
    Unsupported,
}

#[test]
fn every_rust_policy_case_matches_the_lean_generated_expectation_once() {
    let cases = parse_corpus();
    let expected_ids = cases
        .iter()
        .filter(|case| {
            case.layer == "policy" && case.implementation_targets.iter().any(|v| v == "rust")
        })
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut executed = BTreeSet::new();

    for case in cases.iter().filter(|case| {
        case.layer == "policy"
            && case
                .implementation_targets
                .iter()
                .any(|value| value == "rust")
    }) {
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        assert!(
            executed.insert(case.id.as_str()),
            "duplicate case {}",
            case.id
        );
        assert_policy_case(case);
    }

    assert_eq!(executed, expected_ids);
    assert_eq!(executed.len(), 10);
}

#[test]
fn corpus_contract_rejects_unknown_or_duplicate_metadata() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl"
    ));
    let first = source.lines().next().unwrap();
    let unknown = first.replacen("\"schema\":1", "\"schema\":1,\"extra\":true", 1);
    assert!(serde_json::from_str::<CorpusCase>(&unknown).is_err());

    let mut case: CorpusCase = serde_json::from_str(first).unwrap();
    case.implementation_targets.push("rust".into());
    assert!(validate_contract(&case).is_err());
    case.implementation_targets.clear();
    assert!(validate_contract(&case).is_err());
    case.implementation_targets.push("python".into());
    assert!(validate_contract(&case).is_err());
}

fn parse_corpus() -> Vec<CorpusCase> {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl"
    ));
    source
        .lines()
        .map(|line| serde_json::from_str::<CorpusCase>(line).unwrap())
        .inspect(|case| validate_contract(case).unwrap())
        .collect()
}

fn validate_contract(case: &CorpusCase) -> Result<(), String> {
    if case.schema != 1 || case.id.is_empty() {
        return Err("invalid schema or id".into());
    }
    if !matches!(case.layer.as_str(), "policy" | "runtime") {
        return Err("unknown layer".into());
    }
    if !matches!(
        case.mode.as_str(),
        "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
    ) {
        return Err("unknown mode".into());
    }
    let targets = case.implementation_targets.iter().collect::<BTreeSet<_>>();
    if targets.is_empty() || targets.len() != case.implementation_targets.len() {
        return Err("empty or duplicate targets".into());
    }
    if targets.iter().any(|value| value.as_str() != "rust") {
        return Err("unknown target".into());
    }
    Ok(())
}

fn assert_policy_case(case: &CorpusCase) {
    assert_initial_policy_state(&case.initial, &case.id);
    let mut lifecycle = DiskLifecycle::new([]).unwrap();
    let mut accepted = true;
    let mut rejected_at = None;
    for (index, event) in case.events.iter().enumerate() {
        let event = match event {
            Event::Observe {
                owned,
                max_owned,
                free,
                min_free,
            } => DiskLifecycleEvent::Observation {
                policy: DiskPolicy {
                    max_owned_bytes: NonZeroU64::new(*max_owned).unwrap(),
                    min_free_bytes: NonZeroU64::new(*min_free).unwrap(),
                },
                value: DiskObservation {
                    owned_bytes: *owned,
                    available_bytes: *free,
                    measured_in: Duration::ZERO,
                },
            },
            Event::MeterFailed => DiskLifecycleEvent::MeasurementFailed {
                message: "oracle fixture".into(),
            },
            Event::Dispatch => DiskLifecycleEvent::DispatchRequested,
            Event::Unsupported => panic!("unsupported policy event in {}", case.id),
        };
        if !lifecycle.apply(event) {
            accepted = false;
            rejected_at = Some(index);
            break;
        }
    }

    let snapshot = lifecycle.snapshot();
    let actual_stop = snapshot
        .stop
        .as_ref()
        .map(|failure| reason_name(failure.reason.kind()));
    let secondary = snapshot
        .stop
        .as_ref()
        .map(|failure| {
            failure
                .secondary
                .iter()
                .fold(Vec::new(), |mut reasons, value| {
                    let reason = match value {
                        DiskSecondary::Observation { reason, .. } => reason_name(*reason),
                        DiskSecondary::Error { .. } => "measurement_failed",
                    };
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                    reasons
                })
        })
        .unwrap_or_default();
    assert_eq!(
        actual_stop,
        case.expected.state.stop.as_deref(),
        "{} stop",
        case.id
    );
    assert_eq!(
        secondary, case.expected.state.secondary_stops,
        "{} secondary",
        case.id
    );
    assert_eq!(
        snapshot.active, case.expected.state.active,
        "{} active",
        case.id
    );
    assert_eq!(
        snapshot.dispatched, case.expected.state.dispatched,
        "{} dispatched",
        case.id
    );
    assert_eq!(accepted, case.expected.accepted, "{} accepted", case.id);
    assert_eq!(
        rejected_at, case.expected.rejected_at,
        "{} rejected_at",
        case.id
    );
}

fn assert_initial_policy_state(state: &State, id: &str) {
    assert_eq!(state.stop, None, "{id}");
    assert!(state.secondary_stops.is_empty(), "{id}");
    assert_eq!(state.active, 0, "{id}");
    assert_eq!(state.dispatched, 0, "{id}");
    assert!(state.owned_roots.is_empty(), "{id}");
    assert!(state.delivery_roots.is_empty(), "{id}");
    assert!(state.cleanup_requested.is_empty(), "{id}");
    assert!(state.cleanup_clean.is_empty(), "{id}");
    assert!(state.cleanup_failed.is_empty(), "{id}");
    assert!(state.cleanup_deferred.is_empty(), "{id}");
    assert!(state.cleanup_retained.is_empty(), "{id}");
    assert_eq!(state.process_drain, "pending", "{id}");
    assert_eq!(state.output_drain, "pending", "{id}");
    assert_eq!(state.monitor_join, "pending", "{id}");
    assert_eq!(state.report, "pending", "{id}");
    assert!(!state.finished, "{id}");
}

fn reason_name(reason: DiskStopReason) -> &'static str {
    match reason {
        DiskStopReason::WorkspaceSizeExceeded => "workspace_size_exceeded",
        DiskStopReason::FilesystemReserveReached => "filesystem_reserve_reached",
        DiskStopReason::MeasurementFailed => "measurement_failed",
        DiskStopReason::ProcessFailed => "process_failed",
    }
}

#[test]
fn direct_policy_api_agrees_with_the_lifecycle_entry_point() {
    let policy = DiskPolicy {
        max_owned_bytes: NonZeroU64::new(10).unwrap(),
        min_free_bytes: NonZeroU64::new(10).unwrap(),
    };
    assert!(matches!(
        policy.evaluate(DiskObservation {
            owned_bytes: 10,
            available_bytes: 11,
            measured_in: Duration::ZERO,
        }),
        DiskDecision::Stop(_)
    ));
}

#[rstest::rstest]
#[case::brokenfamily_exclusivesize(BrokenFamily::ExclusiveSize)]
#[case::brokenfamily_reversedreserve(BrokenFamily::ReversedReserve)]
#[case::brokenfamily_dropsimultaneoussecondary(BrokenFamily::DropSimultaneousSecondary)]
#[case::brokenfamily_overwriteprimary(BrokenFamily::OverwritePrimary)]
#[case::brokenfamily_allowpoststopdispatch(BrokenFamily::AllowPostStopDispatch)]
fn corpus_detects_each_reviewed_broken_policy_family_scenario_1(#[case] family: BrokenFamily) {
    let cases = parse_corpus();

    let mismatches = cases
        .iter()
        .filter(|case| {
            case.layer == "policy"
                && case
                    .implementation_targets
                    .iter()
                    .any(|value| value == "rust")
        })
        .filter(|case| broken_policy_mismatches(case, family))
        .count();
    assert!(mismatches > 0, "{family:?} escaped the committed corpus");
}

#[derive(Clone, Copy, Debug)]
enum BrokenFamily {
    ExclusiveSize,
    ReversedReserve,
    DropSimultaneousSecondary,
    OverwritePrimary,
    AllowPostStopDispatch,
}

fn broken_policy_mismatches(case: &CorpusCase, family: BrokenFamily) -> bool {
    let mut stop: Option<&'static str> = None;
    let mut secondary = Vec::new();
    let mut active = 0;
    let mut dispatched = 0;
    let mut accepted = true;
    let mut rejected_at = None;
    for (index, event) in case.events.iter().enumerate() {
        match event {
            Event::Observe {
                owned,
                max_owned,
                free,
                min_free,
            } => {
                let size = match family {
                    BrokenFamily::ExclusiveSize => owned > max_owned,
                    _ => owned >= max_owned,
                };
                let reserve = match family {
                    BrokenFamily::ReversedReserve => free >= min_free,
                    _ => free <= min_free,
                };
                let mut reasons = Vec::new();
                if reserve {
                    reasons.push("filesystem_reserve_reached");
                }
                if size && !(reserve && matches!(family, BrokenFamily::DropSimultaneousSecondary)) {
                    reasons.push("workspace_size_exceeded");
                }
                for reason in reasons {
                    if matches!(family, BrokenFamily::OverwritePrimary) {
                        stop = Some(reason);
                        secondary.clear();
                    } else if stop.is_none() {
                        stop = Some(reason);
                    } else if stop != Some(reason) && !secondary.contains(&reason) {
                        secondary.push(reason);
                    }
                }
            }
            Event::MeterFailed => {
                let reason = "measurement_failed";
                if stop.is_none() || matches!(family, BrokenFamily::OverwritePrimary) {
                    stop = Some(reason);
                    if matches!(family, BrokenFamily::OverwritePrimary) {
                        secondary.clear();
                    }
                } else if stop != Some(reason) && !secondary.contains(&reason) {
                    secondary.push(reason);
                }
            }
            Event::Dispatch => {
                if stop.is_some() && !matches!(family, BrokenFamily::AllowPostStopDispatch) {
                    accepted = false;
                    rejected_at = Some(index);
                    break;
                }
                active += 1;
                dispatched += 1;
            }
            Event::Unsupported => return false,
        }
    }
    stop != case.expected.state.stop.as_deref()
        || secondary != case.expected.state.secondary_stops
        || active != case.expected.state.active
        || dispatched != case.expected.state.dispatched
        || accepted != case.expected.accepted
        || rejected_at != case.expected.rejected_at
}

#[rstest::rstest]
#[case(|case: &mut CorpusCase| case.schema = 2, "invalid schema or id")]
#[case(|case: &mut CorpusCase| case.id.clear(), "invalid schema or id")]
#[case(|case: &mut CorpusCase| case.layer = "unknown".into(), "unknown layer")]
#[case(|case: &mut CorpusCase| case.mode = "unknown".into(), "unknown mode")]
fn corpus_contract_rejects_invalid_identity(
    #[case] mutate: fn(&mut CorpusCase),
    #[case] expected: &str,
) {
    let mut case = parse_corpus().remove(0);
    mutate(&mut case);
    assert_eq!(validate_contract(&case).unwrap_err(), expected);
}

#[test]
fn policy_adapter_rejects_unsupported_events() {
    let mut case = parse_corpus()
        .into_iter()
        .find(|case| case.layer == "policy")
        .unwrap();
    case.events = vec![Event::Unsupported];
    let panic = std::panic::catch_unwind(|| assert_policy_case(&case)).unwrap_err();
    assert_eq!(
        panic.downcast_ref::<String>().unwrap(),
        &format!("unsupported policy event in {}", case.id)
    );
}

#[test]
fn process_failure_has_the_process_stop_name() {
    assert_eq!(reason_name(DiskStopReason::ProcessFailed), "process_failed");
}

fn sequential_failure_case() -> CorpusCase {
    let mut case = parse_corpus()
        .into_iter()
        .find(|case| case.layer == "policy")
        .unwrap();
    case.id = "size_then_measurement_failure".into();
    case.events = vec![
        Event::Observe {
            owned: 1,
            max_owned: 1,
            free: 2,
            min_free: 1,
        },
        Event::MeterFailed,
    ];
    case.expected.state.stop = Some("workspace_size_exceeded".into());
    case.expected.state.secondary_stops = vec!["measurement_failed".into()];
    case.expected.state.active = 0;
    case.expected.state.dispatched = 0;
    case.expected.accepted = true;
    case.expected.rejected_at = None;
    case
}

#[test]
fn policy_adapter_retains_measurement_failure_secondary_to_size_limit() {
    assert_policy_case(&sequential_failure_case());
}

#[test]
fn simultaneous_secondary_mutant_agrees_on_sequential_failures() {
    assert!(!broken_policy_mismatches(
        &sequential_failure_case(),
        BrokenFamily::DropSimultaneousSecondary
    ));
}

#[test]
fn broken_policy_comparison_ignores_unsupported_events() {
    let mut case = sequential_failure_case();
    case.events = vec![Event::Unsupported];
    assert!(!broken_policy_mismatches(
        &case,
        BrokenFamily::ExclusiveSize
    ));
}
