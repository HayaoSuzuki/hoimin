//! Shared logical byte limit for retained candidate payload, not process memory.
//!
//! Leases follow owned strings across producers, conversion and spool encoding.
//! Source/parser state, collection capacity and one candidate under construction
//! are excluded. Callers reserve before making any additional retained copies.
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use super::{AnalysisError, AnalyzerCandidate};

pub(crate) const DEFAULT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct PayloadBudget(Arc<Mutex<State>>);

#[derive(Debug)]
struct State {
    limit: usize,
    used: usize,
    #[cfg(test)]
    peak: usize,
    #[cfg(test)]
    reservations: usize,
    #[cfg(test)]
    releases: usize,
    #[cfg(test)]
    admissions: usize,
    #[cfg(test)]
    generated: usize,
}

impl Default for PayloadBudget {
    fn default() -> Self {
        Self::new(DEFAULT_LIMIT)
    }
}

impl PayloadBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self(Arc::new(Mutex::new(State {
            limit,
            used: 0,
            #[cfg(test)]
            peak: 0,
            #[cfg(test)]
            reservations: 0,
            #[cfg(test)]
            releases: 0,
            #[cfg(test)]
            admissions: 0,
            #[cfg(test)]
            generated: 0,
        })))
    }

    pub(crate) fn exceeded(&self) -> AnalysisError {
        AnalysisError::PayloadLimit {
            limit: self.0.lock().unwrap().limit,
        }
    }

    /// Reserves a checked sum atomically. Failure leaves existing leases intact.
    pub(crate) fn reserve(
        &self,
        lengths: impl IntoIterator<Item = usize>,
    ) -> Result<PayloadLease, AnalysisError> {
        let bytes = lengths
            .into_iter()
            .try_fold(0usize, usize::checked_add)
            .ok_or_else(|| self.exceeded())?;
        let mut state = self.0.lock().unwrap();
        let used = state
            .used
            .checked_add(bytes)
            .filter(|used| *used <= state.limit)
            .ok_or(AnalysisError::PayloadLimit { limit: state.limit })?;
        state.used = used;
        #[cfg(test)]
        {
            state.peak = state.peak.max(used);
            state.reservations += 1;
        }
        Ok(PayloadLease {
            budget: self.clone(),
            bytes,
        })
    }

    pub(crate) fn retain(
        &self,
        value: AnalyzerCandidate,
    ) -> Result<Charged<AnalyzerCandidate>, AnalysisError> {
        let lease = self.reserve([
            value.path.as_str().len(),
            value.original.len(),
            value.replacement.len(),
            value.symbol.as_ref().map_or(0, String::len),
        ])?;
        Ok(Charged { value, lease })
    }

    #[cfg(test)]
    pub(crate) fn generated(&self) {
        self.0.lock().unwrap().generated += 1;
    }

    #[cfg(test)]
    pub(crate) fn admitted(&self) {
        self.0.lock().unwrap().admissions += 1;
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Snapshot {
        let state = self.0.lock().unwrap();
        Snapshot {
            used: state.used,
            peak: state.peak,
            reservations: state.reservations,
            releases: state.releases,
            admissions: state.admissions,
            generated: state.generated,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct Snapshot {
    pub used: usize,
    pub peak: usize,
    pub reservations: usize,
    pub releases: usize,
    pub admissions: usize,
    pub generated: usize,
}

/// Non-Clone ownership of a reservation. Move it with the strings it covers.
#[derive(Debug)]
pub(crate) struct PayloadLease {
    budget: PayloadBudget,
    bytes: usize,
}

impl PayloadLease {
    pub(crate) fn budget(&self) -> &PayloadBudget {
        &self.budget
    }

    pub(crate) fn grow(
        &mut self,
        lengths: impl IntoIterator<Item = usize>,
    ) -> Result<(), AnalysisError> {
        let mut extra = self.budget.reserve(lengths)?;
        // Both reservations coexist within one checked limit, so their sum fits.
        self.bytes += extra.bytes;
        extra.bytes = 0;
        Ok(())
    }
}

impl Drop for PayloadLease {
    fn drop(&mut self) {
        let mut state = self.budget.0.lock().unwrap();
        state.used -= self.bytes;
        #[cfg(test)]
        {
            state.releases += 1;
        }
    }
}

/// Fields drop in declaration order: payload must die before its reservation.
#[derive(Debug)]
pub(crate) struct Charged<T> {
    pub value: T,
    pub lease: PayloadLease,
}

impl<T> Deref for Charged<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: PartialEq> PartialEq for Charged<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Charged<T> {}

impl<T> Charged<T> {
    /// Terminal handoff only: no later producer may reuse this accounting owner.
    pub(crate) fn into_untracked(self) -> T {
        self.value
    }
}
