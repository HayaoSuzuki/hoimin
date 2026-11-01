use std::collections::BTreeSet;

use hoimin_core::{
    BudgetLedger, EffectId, PreflightCompleted, RunBudgets, WorkspaceCopyGrant,
    reserve_workspace_copy,
};
use proptest::prelude::*;

use super::{PlanState, WorkspaceError};

fn ledger() -> BudgetLedger {
    BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: u64::MAX,
        processes: 1,
    })
}

fn grant(ledger: &mut BudgetLedger, preflight: u64, bytes: u64) -> WorkspaceCopyGrant {
    reserve_workspace_copy(
        ledger,
        &PreflightCompleted {
            id: EffectId(preflight),
            per_worker_logical_bytes: bytes,
            requested_workers: 4,
            aggregate_logical_bytes: bytes * 4,
            fingerprint: None,
        },
    )
    .unwrap()
}

#[rstest::rstest]
#[case::input_0_1(0)]
#[case::input_1_2(1)]
#[case::input_64_3(64)]
fn accepted_requests_bind_once_including_zero_allowance(#[case] bytes: u64) {
    let mut ledger = ledger();
    let grant = grant(&mut ledger, 7, bytes);
    let first = grant.create_worker(EffectId(10), 0).unwrap();
    let second = grant.create_worker(EffectId(11), 1).unwrap();
    let mut state = PlanState::default();

    let first_result = state.accept_grant(&first, EffectId(7), 2, bytes * 4);
    let second_result = state.accept_grant(&second, EffectId(7), 2, bytes * 4);

    assert_eq!(first_result, Ok(true));
    assert_eq!(second_result, Ok(false));
    assert_eq!(state.reservation, Some(grant.reservation_id()));
    assert_eq!(state.workers, BTreeSet::from([0, 1]));
}

#[rstest::rstest]
#[case::wrong_preflight_create_worker_effectid_20_3_unwrap_1(0)]
#[case::wrong_allowance_create_worker_effectid_21_3_unwrap_2(1)]
#[case::wrong_allowance_create_worker_effectid_22_0_unwrap_3(2)]
#[case::foreign_create_worker_effectid_23_0_unwrap_4(3)]
#[case::bound_create_worker_effectid_24_0_unwrap_5(4)]
fn rejection_precedence_preserves_the_bound_reservation_and_slots(#[case] scenario_index: usize) {
    let mut ledger = ledger();
    let bound = grant(&mut ledger, 7, 2);
    let wrong_preflight = grant(&mut ledger, 8, 3);
    let wrong_allowance = grant(&mut ledger, 7, 3);
    let foreign = grant(&mut ledger, 7, 2);
    let cases = [
        (
            wrong_preflight.create_worker(EffectId(20), 3).unwrap(),
            WorkspaceError::PreflightMismatch {
                expected: EffectId(7),
                received: EffectId(8),
            },
        ),
        (
            wrong_allowance.create_worker(EffectId(21), 3).unwrap(),
            WorkspaceError::WorkerOutOfRange {
                worker: 3,
                requested_workers: 2,
            },
        ),
        (
            wrong_allowance.create_worker(EffectId(22), 0).unwrap(),
            WorkspaceError::AllowanceMismatch {
                expected: 8,
                received: 12,
            },
        ),
        (
            foreign.create_worker(EffectId(23), 0).unwrap(),
            WorkspaceError::ReservationMismatch {
                expected: bound.reservation_id(),
                received: foreign.reservation_id(),
            },
        ),
        (
            bound.create_worker(EffectId(24), 0).unwrap(),
            WorkspaceError::WorkerAlreadyExists { worker: 0 },
        ),
    ];

    let (request, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let mut state = PlanState::default();
    let initial = bound.create_worker(EffectId(10), 0).unwrap();

    assert_eq!(state.accept_grant(&initial, EffectId(7), 2, 8), Ok(true));
    let reservation = state.reservation;
    let workers = state.workers.clone();

    let result = state.accept_grant(&request, EffectId(7), 2, 8);

    assert_eq!(result, Err(expected));
    assert_eq!(state.reservation, reservation);
    assert_eq!(state.workers, workers);
}

// A log of accepted raw inputs supplies expectations without consulting PlanState.
fn accept_input(history: &mut Vec<(usize, u32)>, grant: usize, worker: u32) -> Option<bool> {
    let admissible = grant < 2
        && worker < 2
        && history.first().is_none_or(|(bound, _)| *bound == grant)
        && history.iter().all(|(_, used)| *used != worker);
    if !admissible {
        return None;
    }
    let first = history.is_empty();
    history.push((grant, worker));
    Some(first)
}

proptest! {
    #[test]
    fn request_sequences_preserve_the_first_valid_binding(
        bytes in 0_u64..=64,
        inputs in prop::collection::vec((0_usize..4, 0_u32..4), 0..32),
    ) {
        let mut ledger = ledger();
        let grants = [
            grant(&mut ledger, 7, bytes),
            grant(&mut ledger, 7, bytes),
            grant(&mut ledger, 7, bytes + 1),
            grant(&mut ledger, 8, bytes),
        ];
        let mut state = PlanState::default();
        let mut accepted = Vec::new();
        for (kind, worker) in inputs {
            let request = grants[kind].create_worker(EffectId(10), worker).unwrap();
            let expected = accept_input(&mut accepted, kind, worker);
            let expected_reservation = accepted.first().map(|(kind, _)| grants[*kind].reservation_id());
            let expected_workers = accepted.iter().map(|(_, worker)| *worker).collect::<BTreeSet<_>>();

            let result = state.accept_grant(&request, EffectId(7), 2, bytes * 4);

            prop_assert_eq!(result.ok(), expected);
            prop_assert_eq!(state.reservation, expected_reservation);
            prop_assert_eq!(&state.workers, &expected_workers);
        }
    }
}
