use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ContractInvariant;
use crate::{Cleanup, CleanupFinished, CreateWorker, EffectId, PreflightCompleted};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum BudgetKind {
    Memory,
    Copy,
    Processes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunBudgets {
    pub memory: u64,
    pub copy: u64,
    pub processes: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ReservationId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Reservation {
    pub kind: BudgetKind,
    pub amount: u64,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{kind:?} budget exhausted: requested {requested}, only {available} remains")]
pub struct LimitReached {
    pub kind: BudgetKind,
    pub requested: u64,
    pub available: u64,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ReserveError {
    #[error(transparent)]
    LimitReached(#[from] LimitReached),
    #[error("reservation identifier space is exhausted")]
    ReservationIdsExhausted,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BudgetError {
    #[error("reservation {0:?} was already released")]
    AlreadyReleased(ReservationId),
    #[error("reservation {0:?} is unknown")]
    UnknownReservation(ReservationId),
}

#[derive(Clone, Debug)]
pub struct BudgetLedger {
    limits: RunBudgets,
    reservations: BTreeMap<ReservationId, Reservation>,
    released: BTreeSet<ReservationId>,
    next_id: Option<u64>,
}

impl BudgetLedger {
    #[must_use]
    pub fn new(limits: RunBudgets) -> Self {
        Self {
            limits,
            reservations: BTreeMap::new(),
            released: BTreeSet::new(),
            next_id: Some(0),
        }
    }

    #[must_use]
    pub fn limits(&self) -> RunBudgets {
        self.limits
    }

    /// # Errors
    ///
    /// Returns [`ReserveError::LimitReached`] when the requested amount exceeds the remaining
    /// budget for `kind`, or [`ReserveError::ReservationIdsExhausted`] when no reservation
    /// identifier remains.
    ///
    /// # Panics
    ///
    /// Panics if the allocator's next identifier has already been used.
    ///
    pub fn reserve(
        &mut self,
        kind: BudgetKind,
        amount: u64,
    ) -> Result<ReservationId, ReserveError> {
        let reserved = self.reserved(kind);
        let available = self.limit(kind).saturating_sub(reserved);
        if amount > available {
            self.check_invariant();
            return Err(LimitReached {
                kind,
                requested: amount,
                available,
            }
            .into());
        }
        let id = match self.allocate_reservation_id() {
            Ok(id) => id,
            Err(error) => {
                self.check_invariant();
                return Err(error);
            }
        };
        let replaced = self.reservations.insert(id, Reservation { kind, amount });
        assert!(
            replaced.is_none(),
            "allocated reservation identifier must be unique"
        );
        self.check_invariant();
        Ok(id)
    }

    fn allocate_reservation_id(&mut self) -> Result<ReservationId, ReserveError> {
        let value = self.next_id.ok_or(ReserveError::ReservationIdsExhausted)?;
        let id = ReservationId(value);
        assert!(
            !self.reservations.contains_key(&id) && !self.released.contains(&id),
            "next reservation identifier must be globally unused"
        );
        self.next_id = value.checked_add(1);
        Ok(id)
    }

    /// # Errors
    ///
    /// Returns [`BudgetError::AlreadyReleased`] for a reservation released before, or [`BudgetError::UnknownReservation`] for an unknown ID.
    ///
    pub fn release(&mut self, id: ReservationId) -> Result<(), BudgetError> {
        if self.reservations.remove(&id).is_some() {
            self.released.insert(id);
            self.check_invariant();
            return Ok(());
        }
        self.check_invariant();
        if self.released.contains(&id) {
            Err(BudgetError::AlreadyReleased(id))
        } else {
            Err(BudgetError::UnknownReservation(id))
        }
    }

    #[must_use]
    pub fn reserved(&self, kind: BudgetKind) -> u64 {
        self.reservations
            .values()
            .filter(|reservation| reservation.kind == kind)
            .map(|reservation| reservation.amount)
            .sum()
    }

    #[must_use]
    pub fn reservation(&self, id: ReservationId) -> Option<&Reservation> {
        self.reservations.get(&id)
    }

    fn limit(&self, kind: BudgetKind) -> u64 {
        match kind {
            BudgetKind::Memory => self.limits.memory,
            BudgetKind::Copy => self.limits.copy,
            BudgetKind::Processes => self.limits.processes,
        }
    }

    // The contract macro may compile out while this helper retains its uniform invariant API.
    #[allow(clippy::unused_self)]
    fn check_invariant(&self) {
        crate::contract_ensure!("budget.total.invariant", self.invariant(), self.limits);
    }
}

impl ContractInvariant for BudgetLedger {
    fn invariant(&self) -> bool {
        let totals_fit = [BudgetKind::Memory, BudgetKind::Copy, BudgetKind::Processes]
            .into_iter()
            .all(|kind| self.reserved(kind) <= self.limit(kind));
        let active_and_released_are_disjoint = self
            .reservations
            .keys()
            .all(|id| !self.released.contains(id));
        let ids_precede_frontier = self.next_id.is_none_or(|next| {
            self.reservations
                .keys()
                .chain(self.released.iter())
                .all(|id| id.0 < next)
        });
        totals_fit && active_and_released_are_disjoint && ids_precede_frontier
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BudgetKind, BudgetLedger, PreflightCompleted, ReservationId, ReserveError, RunBudgets,
        WorkspaceBudgetError, reserve_workspace_copy,
    };
    use crate::EffectId;

    fn ledger() -> BudgetLedger {
        BudgetLedger::new(RunBudgets {
            memory: 8,
            copy: 8,
            processes: 8,
        })
    }

    #[test]
    fn allocator_exhaustion_preserves_the_last_active_reservation() {
        let mut ledger = ledger();
        ledger.next_id = Some(u64::MAX);

        let last = ledger.reserve(BudgetKind::Copy, 1).unwrap();
        assert_eq!(last, ReservationId(u64::MAX));
        assert_eq!(ledger.reserved(BudgetKind::Copy), 1);

        let error = ledger.reserve(BudgetKind::Copy, 1).unwrap_err();
        assert_eq!(error, ReserveError::ReservationIdsExhausted);
        assert_eq!(ledger.reserved(BudgetKind::Copy), 1);
        assert_eq!(ledger.reservation(last).unwrap().amount, 1);

        ledger.release(last).unwrap();
        assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
        assert_eq!(
            ledger.reserve(BudgetKind::Copy, 1),
            Err(ReserveError::ReservationIdsExhausted)
        );
    }

    #[test]
    fn limit_error_precedes_identifier_exhaustion_without_mutating_the_ledger() {
        let mut ledger = ledger();
        let existing = ledger.reserve(BudgetKind::Copy, 1).unwrap();
        ledger.next_id = None;

        let error = ledger.reserve(BudgetKind::Copy, 8).unwrap_err();

        assert_eq!(
            error,
            ReserveError::LimitReached(super::LimitReached {
                kind: BudgetKind::Copy,
                requested: 8,
                available: 7,
            })
        );
        assert_eq!(ledger.reserved(BudgetKind::Copy), 1);
        assert_eq!(ledger.reservation(existing).unwrap().amount, 1);
        assert_eq!(ledger.next_id, None);
    }

    #[test]
    fn workspace_reservation_reports_identifier_exhaustion_without_accounting() {
        let mut ledger = ledger();
        ledger.next_id = None;
        let preflight = PreflightCompleted {
            id: EffectId(90),
            per_worker_logical_bytes: 1,
            requested_workers: 1,
            aggregate_logical_bytes: 1,
            fingerprint: None,
        };

        let error = reserve_workspace_copy(&mut ledger, &preflight).unwrap_err();

        assert_eq!(
            error,
            WorkspaceBudgetError::Reserve(ReserveError::ReservationIdsExhausted)
        );
        assert_eq!(error.code(), "workspace.reservation_id.exhausted");
        assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
    }

    #[test]
    fn workspace_limit_code_is_preserved_after_reserve_error_wrapping() {
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 8,
            copy: 0,
            processes: 8,
        });
        let preflight = PreflightCompleted {
            id: EffectId(91),
            per_worker_logical_bytes: 1,
            requested_workers: 1,
            aggregate_logical_bytes: 1,
            fingerprint: None,
        };

        let error = reserve_workspace_copy(&mut ledger, &preflight).unwrap_err();
        assert!(matches!(
            error,
            WorkspaceBudgetError::Reserve(ReserveError::LimitReached(_))
        ));
        assert_eq!(error.code(), "workspace.copy.limit");
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WorkspaceBudgetError {
    #[error("workspace preflight aggregate byte count overflowed")]
    AggregateOverflow,
    #[error("workspace preflight aggregate mismatch: {per_worker} x {workers} != {reported}")]
    AggregateMismatch {
        per_worker: u64,
        workers: u32,
        reported: u64,
    },
    #[error("worker {worker} is outside requested worker count {requested_workers}")]
    WorkerOutOfRange { worker: u32, requested_workers: u32 },
    #[error(transparent)]
    Reserve(#[from] ReserveError),
}

impl WorkspaceBudgetError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::AggregateOverflow => "workspace.preflight.aggregate_overflow",
            Self::AggregateMismatch { .. } => "workspace.preflight.aggregate_mismatch",
            Self::WorkerOutOfRange { .. } => "workspace.worker.out_of_range",
            Self::Reserve(ReserveError::LimitReached(_)) => "workspace.copy.limit",
            Self::Reserve(ReserveError::ReservationIdsExhausted) => {
                "workspace.reservation_id.exhausted"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceCopyGrant {
    preflight_id: EffectId,
    reservation_id: ReservationId,
    granted_allowance: u64,
    per_worker_logical_bytes: u64,
    requested_workers: u32,
}

impl WorkspaceCopyGrant {
    #[must_use]
    pub fn preflight_id(self) -> EffectId {
        self.preflight_id
    }

    #[must_use]
    pub fn reservation_id(self) -> ReservationId {
        self.reservation_id
    }
    #[must_use]
    pub fn granted_allowance(self) -> u64 {
        self.granted_allowance
    }

    #[must_use]
    pub fn per_worker_logical_bytes(self) -> u64 {
        self.per_worker_logical_bytes
    }

    #[must_use]
    pub fn requested_workers(self) -> u32 {
        self.requested_workers
    }

    /// # Errors
    ///
    /// Returns [`WorkspaceBudgetError::WorkerOutOfRange`] when `worker` was not requested during preflight.
    ///
    pub fn create_worker(
        self,
        id: EffectId,
        worker: u32,
    ) -> Result<CreateWorker, WorkspaceBudgetError> {
        if worker >= self.requested_workers {
            return Err(WorkspaceBudgetError::WorkerOutOfRange {
                worker,
                requested_workers: self.requested_workers,
            });
        }
        Ok(CreateWorker::from_workspace_grant(
            id,
            self.preflight_id,
            self.reservation_id,
            self.granted_allowance,
            worker,
        ))
    }

    #[must_use]
    pub fn cleanup(self, id: EffectId) -> Cleanup {
        Cleanup {
            id,
            reservations: vec![self.reservation_id],
        }
    }
}

/// # Errors
///
/// Returns [`WorkspaceBudgetError::AggregateOverflow`] or
/// [`WorkspaceBudgetError::AggregateMismatch`] for invalid preflight bytes, or
/// [`WorkspaceBudgetError::Reserve`] when the copy budget or reservation identifiers are
/// exhausted.
///
pub fn reserve_workspace_copy(
    ledger: &mut BudgetLedger,
    preflight: &PreflightCompleted,
) -> Result<WorkspaceCopyGrant, WorkspaceBudgetError> {
    let expected = preflight
        .per_worker_logical_bytes
        .checked_mul(u64::from(preflight.requested_workers))
        .ok_or(WorkspaceBudgetError::AggregateOverflow)?;
    if expected != preflight.aggregate_logical_bytes {
        return Err(WorkspaceBudgetError::AggregateMismatch {
            per_worker: preflight.per_worker_logical_bytes,
            workers: preflight.requested_workers,
            reported: preflight.aggregate_logical_bytes,
        });
    }
    let reservation_id = ledger.reserve(BudgetKind::Copy, expected)?;
    Ok(WorkspaceCopyGrant {
        preflight_id: preflight.id,
        reservation_id,
        granted_allowance: expected,
        per_worker_logical_bytes: preflight.per_worker_logical_bytes,
        requested_workers: preflight.requested_workers,
    })
}

/// # Errors
///
/// Returns [`BudgetError::AlreadyReleased`] or [`BudgetError::UnknownReservation`] for a released cleanup reservation.
///
pub fn release_workspace_copy(
    ledger: &mut BudgetLedger,
    cleanup: &CleanupFinished,
) -> Result<(), BudgetError> {
    for reservation in &cleanup.released_reservations {
        ledger.release(*reservation)?;
    }
    Ok(())
}
