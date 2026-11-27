//! Replay startup recovery against real staging and deleting directories.
use super::super::{
    DELETING_PREFIX, HEARTBEAT_FILE, LEASE_FILE, LEASE_SCHEMA, LeaseMarker, ManagedRootCoordinator,
    ManagedRunRoot, OwnerKind, STAGING_PREFIX, STALE_AFTER, create_owned_directory,
};
use crate::{io_faults, quint_itf::states};
use camino::Utf8Path;
use hoimin_core::{JanitorReason, JanitorSummary};
use serde_json::Value;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CREATED_SECONDS: u64 = 5;
const PAYLOAD: &[u8] = b"payload at recovery";

fn tag(value: &Value) -> &str {
    value["tag"].as_str().unwrap()
}

fn reason(value: &Value) -> JanitorReason {
    match tag(value) {
        "Young" => JanitorReason::Young,
        "LeaseInvalid" => JanitorReason::LeaseInvalid,
        "UnsafeStaging" => JanitorReason::UnsafeStaging,
        "HeartbeatInvalid" => JanitorReason::HeartbeatInvalid,
        "AgeUnverifiable" => JanitorReason::AgeUnverifiable,
        other => panic!("unexpected namespace recovery reason {other}"),
    }
}

fn populate(candidate: &cap_std::fs::Dir, run_id: String, layout: &str) -> (SystemTime, bool) {
    let (lease, heartbeat, payload) = match layout {
        "Empty" => (false, false, false),
        "UnmarkedPayload" => (false, false, true),
        "LeaseOnly" => (true, false, false),
        "LeaseWithHeartbeat" => (true, true, false),
        "LeaseWithPayload" => (true, false, true),
        "MarkersWithPayload" => (true, true, true),
        other => panic!("unexpected namespace layout {other}"),
    };
    if lease {
        let marker = serde_json::to_vec(&LeaseMarker {
            schema: LEASE_SCHEMA,
            run_id,
            created_unix_seconds: CREATED_SECONDS,
            owner: OwnerKind::PublicExecution,
        })
        .unwrap();
        candidate.write(LEASE_FILE, &marker).unwrap();
        if heartbeat {
            candidate.write(HEARTBEAT_FILE, &marker).unwrap();
        }
    }
    if payload {
        candidate.write("payload", PAYLOAD).unwrap();
    }
    let timestamp = if heartbeat {
        candidate
            .open(HEARTBEAT_FILE)
            .unwrap()
            .into_std()
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
    } else if lease {
        UNIX_EPOCH + Duration::from_secs(CREATED_SECONDS)
    } else {
        candidate
            .metadata(".")
            .unwrap()
            .modified()
            .unwrap()
            .into_std()
    };
    (timestamp + STALE_AFTER, payload)
}

fn assert_outcome(
    actual: &JanitorSummary,
    expected: &Value,
    original: &Utf8Path,
    deleting: &Utf8Path,
    payload: bool,
) {
    assert_eq!(actual.inspected_roots, 1);
    assert_eq!(actual.absent_roots, 0);
    let counts = (
        actual.reclaimed_roots,
        actual.preserved_roots,
        actual.unresolved_roots,
    );
    let outcome = &expected["outcome"];
    match tag(outcome) {
        "Reclaimed" => {
            assert_eq!(counts, (1, 0, 0));
            assert!(!original.exists());
            assert!(!deleting.exists());
        }
        "Held" => {
            let cause = reason(&outcome["value"]);
            if cause.is_normal_hold() {
                assert_eq!(counts, (0, 1, 0));
                assert_eq!(actual.preserved_by_reason.get(&cause), Some(&1));
            } else {
                assert_eq!(counts, (0, 0, 1));
                assert_eq!(actual.unresolved_by_reason.get(&cause), Some(&1));
            }
            assert!(original.is_dir());
            assert_eq!(deleting.exists(), deleting == original);
            if payload {
                assert_eq!(std::fs::read(original.join("payload")).unwrap(), PAYLOAD);
            }
        }
        other => panic!("unexpected namespace recovery outcome {other}"),
    }
}

fn replay(value: &Value) {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let input = &value["observation"];
    let expected = &value["report"];
    let prefix = match tag(&input["namespace"]) {
        "Staging" => STAGING_PREFIX,
        "Deleting" => DELETING_PREFIX,
        other => panic!("unexpected recovery namespace {other}"),
    };
    let run_id = uuid::Uuid::new_v4().to_string();
    let name = format!("{prefix}{run_id}");
    let path = coordinator.path.join(&name);
    let deleting = coordinator.path.join(format!("{DELETING_PREFIX}{run_id}"));
    let layout = tag(&input["layout"]);
    let candidate = create_owned_directory(&coordinator.dir, &name).unwrap();
    let (threshold, payload) = populate(&candidate, run_id, layout);
    drop(candidate);
    let (now, injection) = match tag(&input["age"]) {
        "BeforeThreshold" => (threshold - Duration::from_nanos(1), None),
        "AtThreshold" => (threshold, None),
        "AgeReadError" => (
            threshold,
            Some(match layout {
                "Empty" => "janitor.staging_age",
                "LeaseWithHeartbeat" | "MarkersWithPayload" => "janitor.heartbeat_metadata",
                other => panic!("no timestamp read for layout {other}"),
            }),
        ),
        other => panic!("unexpected age evidence {other}"),
    };
    let fault = injection.map(io_faults::arm);
    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        now,
        Instant::now(),
        &|_| Duration::ZERO,
        &|_| Ok((vec![name.clone()], None)),
    );

    let details: Vec<_> = expected["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(reason)
        .collect();
    if let Some(fault) = fault {
        assert_eq!(
            fault.triggered(),
            details.contains(&JanitorReason::AgeUnverifiable)
        );
    }
    assert_eq!(
        report
            .details
            .iter()
            .map(|detail| detail.reason)
            .collect::<Vec<_>>(),
        details,
    );
    assert_outcome(&report, expected, &path, &deleting, payload);
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_3.itf.json"))]
#[case::unmarked(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_unmarkedStep_0.itf.json"))]
#[case::markers(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_markersStep_0.itf.json"))]
#[case::payload(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_payloadStep_0.itf.json"))]
#[case::fallback(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_fallbackStep_0.itf.json"))]
fn quint_history_matches_janitor_namespace_recovery(#[case] trace: &str) {
    let history = states(trace);
    assert_eq!(history.len(), 5);
    for row in history.iter().skip(1) {
        let last = &row["s"]["last"];
        assert_eq!(tag(last), "Evaluated");
        replay(&last["value"]);
    }
}
