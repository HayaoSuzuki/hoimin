use hoimin_core::{BudgetError, BudgetKind, BudgetLedger, ContractInvariant, RunBudgets};

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
    assert_eq!(error.kind, BudgetKind::Copy);
    assert_eq!(error.requested, 5);
    assert_eq!(error.available, 4);

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
