use hoimin_core::{
    BudgetError, BudgetKind, BudgetLedger, CleanupFinished, ContractInvariant, EffectId,
    PreflightCompleted, ReserveError, RunBudgets, release_workspace_copy, reserve_workspace_copy,
};

fn budgets(copy: u64) -> RunBudgets {
    RunBudgets {
        memory: 1_024,
        copy,
        processes: 4,
    }
}

#[test]
fn reservations_share_one_run_wide_allowance() {
    let mut ledger = BudgetLedger::new(budgets(10));
    let first = ledger.reserve(BudgetKind::Copy, 6).unwrap();
    assert_eq!(ledger.reserved(BudgetKind::Copy), 6);

    let error = ledger.reserve(BudgetKind::Copy, 5).unwrap_err();
    assert_eq!(
        error,
        ReserveError::LimitReached(hoimin_core::LimitReached {
            kind: BudgetKind::Copy,
            requested: 5,
            available: 4,
        })
    );

    ledger.release(first).unwrap();
    assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
    assert!(ledger.invariant());
}

#[test]
fn aggregate_worker_copy_is_reserved_as_one_amount() {
    let mut ledger = BudgetLedger::new(budgets(12));
    let reservation = ledger.reserve(BudgetKind::Copy, 3 * 4).unwrap();
    assert_eq!(ledger.reservation(reservation).unwrap().amount, 12);
    assert!(ledger.reserve(BudgetKind::Copy, 1).is_err());
}

#[test]
fn release_rejects_a_second_release_without_subtracting_twice() {
    let mut ledger = BudgetLedger::new(budgets(10));
    let reservation = ledger.reserve(BudgetKind::Copy, 7).unwrap();
    ledger.release(reservation).unwrap();

    assert_eq!(
        ledger.release(reservation),
        Err(BudgetError::AlreadyReleased(reservation))
    );
    assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
    assert!(ledger.invariant());
}

#[test]
fn budget_kinds_have_independent_run_wide_limits() {
    let mut ledger = BudgetLedger::new(budgets(10));
    ledger.reserve(BudgetKind::Copy, 10).unwrap();
    ledger.reserve(BudgetKind::Memory, 1_024).unwrap();
    ledger.reserve(BudgetKind::Processes, 4).unwrap();
    assert!(ledger.invariant());
}

#[test]
fn preflight_metadata_is_reserved_by_core_and_carried_to_each_worker_effect() {
    let mut ledger = BudgetLedger::new(budgets(30));
    let preflight = PreflightCompleted {
        id: EffectId(10),
        per_worker_logical_bytes: 10,
        requested_workers: 3,
        aggregate_logical_bytes: 30,
        fingerprint: None,
    };

    let grant = reserve_workspace_copy(&mut ledger, &preflight).unwrap();
    let first = grant.create_worker(EffectId(11), 0).unwrap();
    let second = grant.create_worker(EffectId(12), 1).unwrap();

    assert_eq!(ledger.reserved(BudgetKind::Copy), 30);
    assert_eq!(grant.preflight_id(), EffectId(10));
    assert_eq!(grant.granted_allowance(), 30);
    assert_eq!(grant.per_worker_logical_bytes(), 10);
    assert_eq!(grant.requested_workers(), 3);
    assert_eq!(first.id(), EffectId(11));
    assert_eq!(first.preflight_id(), EffectId(10));
    assert_eq!(first.reservation_id(), grant.reservation_id());
    assert_eq!(first.granted_allowance(), 30);
    assert_eq!(first.worker(), 0);
    assert_eq!(second.reservation_id(), first.reservation_id());
    assert_eq!(second.worker(), 1);
}

#[test]
fn inconsistent_preflight_metadata_is_rejected_without_reserving() {
    let mut ledger = BudgetLedger::new(budgets(100));
    let preflight = PreflightCompleted {
        id: EffectId(20),
        per_worker_logical_bytes: 10,
        requested_workers: 3,
        aggregate_logical_bytes: 29,
        fingerprint: None,
    };

    let error = reserve_workspace_copy(&mut ledger, &preflight).unwrap_err();

    assert_eq!(error.code(), "workspace.preflight.aggregate_mismatch");
    assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
}

#[test]
fn cleanup_completion_releases_the_core_copy_reservation_once() {
    let mut ledger = BudgetLedger::new(budgets(30));
    let grant = reserve_workspace_copy(
        &mut ledger,
        &PreflightCompleted {
            id: EffectId(30),
            per_worker_logical_bytes: 10,
            requested_workers: 3,
            aggregate_logical_bytes: 30,
            fingerprint: None,
        },
    )
    .unwrap();
    let cleanup = CleanupFinished {
        id: EffectId(31),
        released_reservations: vec![grant.reservation_id()],
    };

    release_workspace_copy(&mut ledger, &cleanup).unwrap();

    assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
    assert_eq!(
        release_workspace_copy(&mut ledger, &cleanup),
        Err(BudgetError::AlreadyReleased(grant.reservation_id()))
    );
}

#[test]
fn preflight_aggregate_overflow_is_typed_and_does_not_reserve() {
    let mut ledger = BudgetLedger::new(budgets(u64::MAX));
    let preflight = PreflightCompleted {
        id: EffectId(40),
        per_worker_logical_bytes: u64::MAX,
        requested_workers: 2,
        aggregate_logical_bytes: 0,
        fingerprint: None,
    };

    let error = reserve_workspace_copy(&mut ledger, &preflight).unwrap_err();

    assert_eq!(error.code(), "workspace.preflight.aggregate_overflow");
    assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
}

#[test]
fn grant_rejects_worker_index_outside_preflight_count() {
    let mut ledger = BudgetLedger::new(budgets(10));
    let grant = reserve_workspace_copy(
        &mut ledger,
        &PreflightCompleted {
            id: EffectId(50),
            per_worker_logical_bytes: 5,
            requested_workers: 2,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        },
    )
    .unwrap();

    let error = grant.create_worker(EffectId(51), 2).unwrap_err();

    assert_eq!(error.code(), "workspace.worker.out_of_range");
}

#[test]
fn grant_can_release_a_reserved_preflight_even_before_any_worker_is_created() {
    let mut ledger = BudgetLedger::new(budgets(10));
    let grant = reserve_workspace_copy(
        &mut ledger,
        &PreflightCompleted {
            id: EffectId(60),
            per_worker_logical_bytes: 10,
            requested_workers: 1,
            aggregate_logical_bytes: 10,
            fingerprint: None,
        },
    )
    .unwrap();

    let cleanup = grant.cleanup(EffectId(61));
    assert_eq!(cleanup.reservations, vec![grant.reservation_id()]);
    let completed = CleanupFinished {
        id: cleanup.id,
        released_reservations: cleanup.reservations,
    };
    release_workspace_copy(&mut ledger, &completed).unwrap();
    assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
}
