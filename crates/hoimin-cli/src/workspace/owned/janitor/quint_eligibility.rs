//! Compare eligibility and typed diagnostics against real workspace markers.
use super::super::{
    ACTIVE_PREFIX, CLEANUP_READY_FILE, CleanupReadyMarker, CoordinatorLockGuard, HEARTBEAT_FILE,
    LEASE_FILE, ManagedRootCoordinator, ManagedRunRoot, OwnerKind, STALE_AFTER,
    open_owned_directory, open_regular_file_nofollow,
};
use super::STALE_THRESHOLD_TICK;
use crate::io_faults;
use crate::quint_itf::states;
use camino::Utf8Path;
use hoimin_core::{JanitorReason, JanitorSummary};
use serde_json::Value;
use std::time::{Duration, Instant, SystemTime};

fn tag(value: &Value) -> &str {
    value["tag"].as_str().unwrap()
}

fn reason(value: &Value) -> JanitorReason {
    match tag(value) {
        "LeaseInvalid" => JanitorReason::LeaseInvalid,
        "ActiveLease" => JanitorReason::ActiveLease,
        "LeaseLockFailed" => JanitorReason::LeaseLockFailed,
        "RetainMarker" => JanitorReason::RetainMarker,
        "CandidateInspectionFailed" => JanitorReason::CandidateInspectionFailed,
        "MarkerInvalid" => JanitorReason::MarkerInvalid,
        "IdentityInspectionFailed" => JanitorReason::IdentityInspectionFailed,
        "HeartbeatInvalid" => JanitorReason::HeartbeatInvalid,
        "Young" => JanitorReason::Young,
        "InvalidCandidate" => JanitorReason::InvalidCandidate,
        "CoordinatorBusy" => JanitorReason::CoordinatorBusy,
        "MutationBarrierActive" => JanitorReason::MutationBarrierActive,
        "MutationBarrierFailed" => JanitorReason::MutationBarrierFailed,
        other => panic!("unexpected eligibility reason {other}"),
    }
}

fn configure_lease(path: &Utf8Path, evidence: &str) {
    let marker = path.join(LEASE_FILE);
    match evidence {
        "Available" | "Locked" | "LeaseLockError" => {}
        "NoLease" => std::fs::remove_file(marker).unwrap(),
        "NonFileLease" => {
            std::fs::remove_file(&marker).unwrap();
            std::fs::create_dir(marker).unwrap();
        }
        "BrokenLease" => std::fs::write(marker, b"invalid lease").unwrap(),
        other => panic!("unexpected lease evidence {other}"),
    }
}

fn configure(root: &ManagedRunRoot, input: &Value) -> SystemTime {
    match tag(&input["retention"]) {
        "NoRetain" => {}
        "Retained" | "RetainReadError" => root.retain().unwrap(),
        other => panic!("unexpected retention evidence {other}"),
    }
    match tag(&input["ready"]) {
        "NoReady" => {}
        "ReadyMatches" | "ReadyIdentityError" => root.mark_cleanup_ready().unwrap(),
        "ReadyMismatch" => {
            root.mark_cleanup_ready().unwrap();
            let mut marker: CleanupReadyMarker =
                serde_json::from_slice(&root.dir.read(CLEANUP_READY_FILE).unwrap()).unwrap();
            marker.lease_inode ^= 1;
            root.dir
                .write(CLEANUP_READY_FILE, serde_json::to_vec(&marker).unwrap())
                .unwrap();
        }
        other => panic!("unexpected ready evidence {other}"),
    }
    let heartbeat = tag(&input["heartbeat"]);
    if heartbeat == "HeartbeatMismatch" {
        root.dir
            .write(HEARTBEAT_FILE, b"invalid heartbeat")
            .unwrap();
    }
    let threshold = std::fs::metadata(root.path().join(HEARTBEAT_FILE))
        .unwrap()
        .modified()
        .unwrap()
        + STALE_AFTER;
    match heartbeat {
        "BeforeStale" => threshold - STALE_THRESHOLD_TICK,
        "AtStale" | "HeartbeatMismatch" => threshold,
        "AfterStale" => threshold + STALE_THRESHOLD_TICK,
        other => panic!("unexpected heartbeat evidence {other}"),
    }
}

fn assert_decision(report: &JanitorSummary, expected: &Value, path: &Utf8Path) {
    assert_eq!(report.inspected_roots, 1);
    assert_eq!(report.absent_roots, 0);
    let outcome = &expected["outcome"];
    let counts = (
        report.reclaimed_roots,
        report.preserved_roots,
        report.unresolved_roots,
    );
    match tag(outcome) {
        "Reclaimed" => {
            assert_eq!(counts, (1, 0, 0));
            assert!(!path.exists());
        }
        "Held" => {
            assert_eq!(report.removed_entries, 0);
            let cause = reason(&outcome["value"]);
            if cause.is_normal_hold() {
                assert_eq!(counts, (0, 1, 0));
                assert_eq!(report.preserved_by_reason.get(&cause), Some(&1));
            } else {
                assert_eq!(counts, (0, 0, 1));
                assert_eq!(report.unresolved_by_reason.get(&cause), Some(&1));
            }
            assert_eq!(
                std::fs::read(path.join("payload")).unwrap(),
                b"keep until admitted"
            );
        }
        other => panic!("unexpected eligibility outcome {other}"),
    }
}

fn fault_site(input: &Value) -> Option<(&'static str, JanitorReason)> {
    match tag(&input["candidate"]) {
        "OpenError" => Some((
            "janitor.candidate_open",
            JanitorReason::CandidateInspectionFailed,
        )),
        "IdentityError" => Some((
            "janitor.candidate_identity",
            JanitorReason::IdentityInspectionFailed,
        )),
        "BarrierError" => Some((
            "janitor.mutation_barrier",
            JanitorReason::MutationBarrierFailed,
        )),
        _ if tag(&input["lease"]) == "LeaseLockError" => {
            Some(("janitor.lease_lock", JanitorReason::LeaseLockFailed))
        }
        _ if tag(&input["retention"]) == "RetainReadError" => Some((
            "janitor.retain_stat",
            JanitorReason::CandidateInspectionFailed,
        )),
        _ if tag(&input["ready"]) == "ReadyIdentityError" => Some((
            "janitor.ready_identity",
            JanitorReason::IdentityInspectionFailed,
        )),
        _ => None,
    }
}

fn replay(value: &Value) {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let path = root.path().to_owned();
    let name = path.file_name().unwrap().to_owned();
    let input = &value["observation"];
    let expected = &value["report"];
    let now = configure(&root, input);
    root.dir.write("payload", b"keep until admitted").unwrap();
    let lease = tag(&input["lease"]);
    drop(root);
    configure_lease(&path, lease);
    // Acquire the file lock independently of a live root's Windows mutation event.
    let lease_holder = (lease == "Locked").then(|| {
        let dir = open_owned_directory(&coordinator.dir, &name).unwrap();
        let file = open_regular_file_nofollow(&dir, LEASE_FILE).unwrap();
        file.try_lock().unwrap();
        file
    });
    let candidate = tag(&input["candidate"]);
    let chosen = if candidate == "InvalidName" {
        format!("{ACTIVE_PREFIX}invalid-uuid")
    } else {
        name.clone()
    };
    let coordinator_guard = (candidate == "CoordinatorLocked")
        .then(|| CoordinatorLockGuard::acquire(&coordinator).unwrap());
    #[cfg(windows)]
    let barrier = (candidate == "BarrierPresent").then(|| {
        let run_id = name.strip_prefix(ACTIVE_PREFIX).unwrap();
        super::super::windows::RootMutationBarrier::create(run_id).unwrap()
    });
    let injection = fault_site(input);
    let fault = injection.map(|(point, _)| io_faults::arm(point));
    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        now,
        Instant::now(),
        &|_| Duration::ZERO,
        &|_| Ok((vec![chosen.clone()], None)),
    );
    if let Some(guard) = coordinator_guard {
        guard.unlock(&coordinator.path).unwrap();
    }
    #[cfg(windows)]
    drop(barrier);
    drop(lease_holder);

    let details: Vec<_> = expected["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(reason)
        .collect();
    if let (Some(fault), Some((point, cause))) = (fault, injection) {
        assert_eq!(fault.triggered(), details.contains(&cause), "{point}");
    }
    assert_eq!(
        report
            .details
            .iter()
            .map(|detail| detail.reason)
            .collect::<Vec<_>>(),
        details,
    );
    assert_decision(&report, expected, &path);
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_3.itf.json"))]
#[case::priority(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_priorityStep_0.itf.json"))]
#[case::fallback(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_fallbackStep_0.itf.json"))]
#[case::lease(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_leaseEvidenceStep_0.itf.json"))]
#[case::candidate(include_str!("../../../../../../formal/quint/traces/janitor_eligibility_candidateStep_0.itf.json"))]
fn quint_history_matches_janitor_eligibility(#[case] trace: &str) {
    replay_history(trace);
}

fn replay_history(trace: &str) {
    let history = states(trace);
    assert_eq!(history.len(), 5);
    for row in history.iter().skip(1) {
        let last = &row["s"]["last"];
        assert_eq!(tag(last), "Evaluated");
        replay(&last["value"]);
    }
}

#[cfg(windows)]
#[test]
fn quint_windows_mutation_barrier_precedes_candidate_inspection() {
    replay_history(include_str!(
        "../../../../../../formal/quint/traces/janitor_eligibility_windowsCandidateStep_0.itf.json"
    ));
}
