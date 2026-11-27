//! Replay startup recovery against real staging and deleting directories.
use super::super::{
    DELETING_PREFIX, HEARTBEAT_FILE, JANITOR_CLEANUP_BUDGET, LEASE_FILE, LEASE_SCHEMA, LeaseMarker,
    ManagedRootCoordinator, ManagedRunRoot, OwnerKind, STAGING_PREFIX, STALE_AFTER,
    create_owned_directory,
};
use crate::quint_itf::number;
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
        "ClaimFailed" => JanitorReason::ClaimFailed,
        "CoordinatorFailed" => JanitorReason::CoordinatorFailed,
        "DeleteFailed" => JanitorReason::DeleteFailed,
        "CleanupIncomplete" => JanitorReason::CleanupIncomplete,
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
    layout: &str,
    payload: bool,
) {
    assert_eq!(actual.inspected_roots, 1);
    assert_eq!(actual.absent_roots, 0);
    assert_eq!(actual.removed_entries, number(&expected["removed"]));
    let counts = (
        actual.reclaimed_roots,
        actual.preserved_roots,
        actual.unresolved_roots,
    );
    let outcome = &expected["outcome"];
    match tag(outcome) {
        "Reclaimed" => assert_eq!(counts, (1, 0, 0)),
        "Held" => {
            let cause = reason(&outcome["value"]);
            if cause.is_normal_hold() {
                assert_eq!(counts, (0, 1, 0));
                assert_eq!(actual.preserved_by_reason.get(&cause), Some(&1));
            } else {
                assert_eq!(counts, (0, 0, 1));
                assert_eq!(actual.unresolved_by_reason.get(&cause), Some(&1));
            }
        }
        other => panic!("unexpected namespace recovery outcome {other}"),
    }
    let remaining = match tag(&expected["placement"]) {
        "RemovedName" => {
            assert!(!original.exists());
            assert!(!deleting.exists());
            return;
        }
        "OriginalName" => {
            assert_eq!(deleting.exists(), deleting == original);
            original
        }
        "DeletingName" => {
            assert!(!original.exists());
            deleting
        }
        other => panic!("unexpected surviving namespace {other}"),
    };
    assert!(remaining.is_dir());
    assert_eq!(
        remaining.join(LEASE_FILE).is_file(),
        !matches!(layout, "Empty" | "UnmarkedPayload"),
    );
    assert_eq!(
        remaining.join(HEARTBEAT_FILE).is_file(),
        matches!(layout, "LeaseWithHeartbeat" | "MarkersWithPayload"),
    );
    let file = remaining.join("payload");
    if payload && actual.removed_entries == 0 {
        assert_eq!(std::fs::read(file).unwrap(), PAYLOAD);
    } else {
        assert!(!file.exists());
    }
}

fn reclaim(
    coordinator: &ManagedRootCoordinator,
    name: &str,
    now: SystemTime,
    elapsed: &dyn Fn(Instant) -> Duration,
) -> JanitorSummary {
    ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        coordinator,
        now,
        Instant::now(),
        elapsed,
        &|_| Ok((vec![name.to_owned()], None)),
    )
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
            Some((
                match layout {
                    "Empty" => "janitor.staging_age",
                    "LeaseWithHeartbeat" | "MarkersWithPayload" => "janitor.heartbeat_metadata",
                    other => panic!("no timestamp read for layout {other}"),
                },
                JanitorReason::AgeUnverifiable,
            )),
        ),
        other => panic!("unexpected age evidence {other}"),
    };
    let interruption = tag(&input["interruption"]);
    let injection = match interruption {
        "Uninterrupted" | "PayloadDeadline" => injection,
        "RenameError" => Some(("claim.rename", JanitorReason::ClaimFailed)),
        "UnlockError" => Some(("coordinator.unlock", JanitorReason::CoordinatorFailed)),
        "PayloadUnlinkError" => Some(("cleanup.unlink_file", JanitorReason::DeleteFailed)),
        other => panic!("unexpected recovery interruption {other}"),
    };
    let fault = injection.map(|(point, reason)| (io_faults::arm(point), reason));
    let report = reclaim(&coordinator, &name, now, &|_| {
        if interruption == "PayloadDeadline" && !deleting.join("payload").exists() {
            JANITOR_CLEANUP_BUDGET
        } else {
            Duration::ZERO
        }
    });

    let details: Vec<_> = expected["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(reason)
        .collect();
    if let Some((fault, reason)) = fault {
        assert_eq!(fault.triggered(), details.contains(&reason));
    }
    assert_eq!(
        report
            .details
            .iter()
            .map(|detail| detail.reason)
            .collect::<Vec<_>>(),
        details,
    );
    assert_outcome(&report, expected, &path, &deleting, layout, payload);
    if tag(&expected["placement"]) == "DeletingName"
        || details.contains(&JanitorReason::CleanupIncomplete)
    {
        drop(coordinator);
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let restarted = reclaim(&coordinator, deleting.file_name().unwrap(), now, &|_| {
            Duration::ZERO
        });
        assert_eq!(restarted.reclaimed_roots, 1);
        // The interrupted attempt left only the root, lease and heartbeat.
        assert_eq!(restarted.removed_entries, 3);
        assert!(!deleting.exists());
    }
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
#[case::claim_failure(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_claimFailureStep_0.itf.json"))]
#[case::slice_failure(include_str!("../../../../../../formal/quint/traces/janitor_namespace_recovery_sliceFailureStep_0.itf.json"))]
fn quint_history_matches_janitor_namespace_recovery(#[case] trace: &str) {
    let history = states(trace);
    assert_eq!(history.len(), 5);
    for row in history.iter().skip(1) {
        let last = &row["s"]["last"];
        assert_eq!(tag(last), "Evaluated");
        replay(&last["value"]);
    }
}
