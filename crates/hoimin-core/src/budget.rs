use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ContractInvariant;

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
