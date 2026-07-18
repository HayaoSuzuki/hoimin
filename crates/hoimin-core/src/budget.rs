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
    next_id: u64,
}

impl BudgetLedger {
    pub fn new(limits: RunBudgets) -> Self {
        Self {
            limits,
            reservations: BTreeMap::new(),
            released: BTreeSet::new(),
            next_id: 0,
        }
    }

    pub fn limits(&self) -> RunBudgets {
        self.limits
    }

    pub fn reserve(
        &mut self,
        kind: BudgetKind,
        amount: u64,
    ) -> Result<ReservationId, LimitReached> {
        let reserved = self.reserved(kind);
        let available = self.limit(kind).saturating_sub(reserved);
        if amount > available {
            self.check_invariant();
            return Err(LimitReached {
                kind,
                requested: amount,
                available,
            });
        }
        let id = ReservationId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.reservations.insert(id, Reservation { kind, amount });
        self.check_invariant();
        Ok(id)
    }

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

    pub fn reserved(&self, kind: BudgetKind) -> u64 {
        self.reservations
            .values()
            .filter(|reservation| reservation.kind == kind)
            .map(|reservation| reservation.amount)
            .sum()
    }

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

    fn check_invariant(&self) {
        crate::contract_ensure!("budget.total.invariant", self.invariant(), self.limits);
    }
}

impl ContractInvariant for BudgetLedger {
    fn invariant(&self) -> bool {
        [BudgetKind::Memory, BudgetKind::Copy, BudgetKind::Processes]
            .into_iter()
            .all(|kind| self.reserved(kind) <= self.limit(kind))
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
    LimitReached(#[from] LimitReached),
}

impl WorkspaceBudgetError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AggregateOverflow => "workspace.preflight.aggregate_overflow",
            Self::AggregateMismatch { .. } => "workspace.preflight.aggregate_mismatch",
            Self::WorkerOutOfRange { .. } => "workspace.worker.out_of_range",
            Self::LimitReached(_) => "workspace.copy.limit",
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
    pub fn preflight_id(self) -> EffectId {
        self.preflight_id
    }

    pub fn reservation_id(self) -> ReservationId {
        self.reservation_id
    }

    pub fn granted_allowance(self) -> u64 {
        self.granted_allowance
    }

    pub fn per_worker_logical_bytes(self) -> u64 {
        self.per_worker_logical_bytes
    }

    pub fn requested_workers(self) -> u32 {
        self.requested_workers
    }

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

    pub fn cleanup(self, id: EffectId) -> Cleanup {
        Cleanup {
            id,
            reservations: vec![self.reservation_id],
        }
    }
}

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

pub fn release_workspace_copy(
    ledger: &mut BudgetLedger,
    cleanup: &CleanupFinished,
) -> Result<(), BudgetError> {
    for reservation in &cleanup.released_reservations {
        ledger.release(*reservation)?;
    }
    Ok(())
}
