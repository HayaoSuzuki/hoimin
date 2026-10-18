//! Shared logical byte limit for retained candidate payload, not process memory.
//!
//! Leases follow owned strings across producers, conversion and spool encoding.
//! Source/parser state, collection capacity and one candidate under construction
//! are excluded. Callers reserve before making any additional retained copies.
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use super::{AnalysisError, AnalyzerCandidate};

pub const DEFAULT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct PayloadBudget(Arc<Mutex<State>>);

#[derive(Debug)]
struct State {
    limit: usize,
    used: usize,
    #[cfg(any(test, feature = "test-support"))]
    peak: usize,
    #[cfg(any(test, feature = "test-support"))]
    reservations: usize,
    #[cfg(any(test, feature = "test-support"))]
    releases: usize,
    #[cfg(any(test, feature = "test-support"))]
    admissions: usize,
    #[cfg(any(test, feature = "test-support"))]
    generated: usize,
}

impl Default for PayloadBudget {
    fn default() -> Self {
        Self::new(DEFAULT_LIMIT)
    }
}

impl PayloadBudget {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self(Arc::new(Mutex::new(State {
            limit,
            used: 0,
            #[cfg(any(test, feature = "test-support"))]
            peak: 0,
            #[cfg(any(test, feature = "test-support"))]
            reservations: 0,
            #[cfg(any(test, feature = "test-support"))]
            releases: 0,
            #[cfg(any(test, feature = "test-support"))]
            admissions: 0,
            #[cfg(any(test, feature = "test-support"))]
            generated: 0,
        })))
    }

    /// Returns the limit error for caller-side checked arithmetic.
    ///
    /// # Panics
    /// Panics if another holder poisoned the shared accounting mutex.
    #[must_use]
    pub fn exceeded(&self) -> AnalysisError {
        AnalysisError::PayloadLimit {
            limit: self.0.lock().unwrap().limit,
        }
    }

    /// Reserves a checked sum atomically. Failure leaves existing leases intact.
    ///
    /// # Errors
    /// Returns `PayloadLimit` on sum overflow or insufficient remaining bytes.
    ///
    /// # Panics
    /// Panics if another holder poisoned the shared accounting mutex.
    pub fn reserve(
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
        #[cfg(any(test, feature = "test-support"))]
        {
            state.peak = state.peak.max(used);
            state.reservations += 1;
        }
        Ok(PayloadLease {
            budget: self.clone(),
            bytes,
        })
    }

    /// Charges owned candidate strings before retaining the candidate.
    ///
    /// # Errors
    /// Returns `PayloadLimit` if the candidate cannot fit in the shared budget.
    pub fn retain(
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

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn generated(&self) {
        self.0.lock().unwrap().generated += 1;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn admitted(&self) {
        self.0.lock().unwrap().admissions += 1;
    }

    #[cfg(any(test, feature = "test-support"))]
    /// Observes logical accounting for development tests and probes.
    ///
    /// # Panics
    /// Panics if another holder poisoned the shared accounting mutex.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
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

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub used: usize,
    pub peak: usize,
    pub reservations: usize,
    pub releases: usize,
    pub admissions: usize,
    pub generated: usize,
}

/// Non-Clone ownership of a reservation. Move it with the strings it covers.
#[derive(Debug)]
pub struct PayloadLease {
    budget: PayloadBudget,
    bytes: usize,
}

impl PayloadLease {
    #[must_use]
    pub fn budget(&self) -> &PayloadBudget {
        &self.budget
    }

    /// Reserves additional strings while retaining this lease.
    ///
    /// # Errors
    /// Returns `PayloadLimit` on overflow or insufficient shared capacity.
    pub fn grow(&mut self, lengths: impl IntoIterator<Item = usize>) -> Result<(), AnalysisError> {
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
        #[cfg(any(test, feature = "test-support"))]
        {
            state.releases += 1;
        }
    }
}

/// Fields drop in declaration order: payload must die before its reservation.
#[derive(Debug)]
pub struct Charged<T> {
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
    pub fn into_untracked(self) -> T {
        self.value
    }
}
