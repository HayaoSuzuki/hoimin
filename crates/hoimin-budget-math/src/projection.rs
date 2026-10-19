use std::num::NonZeroUsize;
use std::time::Duration;

#[cfg(creusot)]
use crate::duration::{auto_timeout_nanos, duration_max_nanos};
#[cfg(creusot)]
use creusot_std::prelude::{DeepModel, ensures, requires};

use crate::{MutantTimeout, auto_mutant_timeout};

const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Clone, Copy, Debug, Eq)]
#[cfg_attr(creusot, derive(creusot_std::prelude::DeepModel))]
pub struct TopBudgetProjection {
    pub selected: usize,
    pub jobs: usize,
    pub planned_total_timeout: Duration,
    pub baseline: Duration,
    pub effective_mutant_timeout: Duration,
    pub remaining: Duration,
    pub waves: usize,
    pub projected_capacity: Duration,
}

impl PartialEq for TopBudgetProjection {
    #[cfg_attr(creusot, ensures(result == (self.deep_model() == other.deep_model())))]
    fn eq(&self, other: &Self) -> bool {
        self.selected == other.selected
            && self.jobs == other.jobs
            && self.planned_total_timeout == other.planned_total_timeout
            && self.baseline == other.baseline
            && self.effective_mutant_timeout == other.effective_mutant_timeout
            && self.remaining == other.remaining
            && self.waves == other.waves
            && self.projected_capacity == other.projected_capacity
    }
}

impl TopBudgetProjection {
    #[must_use]
    #[cfg_attr(creusot, ensures(result == (self.projected_capacity@ > self.remaining@)))]
    pub fn is_shortfall(&self) -> bool {
        self.projected_capacity > self.remaining
    }
}

#[must_use]
#[cfg_attr(creusot, ensures(result.selected == selected))]
#[cfg_attr(creusot, ensures(result.jobs@ == jobs@))]
#[cfg_attr(creusot, ensures(result.planned_total_timeout@ == planned_total_timeout@))]
#[cfg_attr(creusot, ensures(result.baseline@ == baseline@))]
#[cfg_attr(creusot, ensures(result.remaining@ == remaining@))]
#[cfg_attr(creusot, ensures(result.waves@ == (selected@ + jobs@ - 1) / jobs@))]
#[cfg_attr(creusot, ensures(match mutant_timeout {
    MutantTimeout::Auto => result.effective_mutant_timeout@ == auto_timeout_nanos(baseline@),
    MutantTimeout::Fixed(value) => result.effective_mutant_timeout@ == value@,
}))]
#[cfg_attr(creusot, ensures(result.projected_capacity@ == (result.effective_mutant_timeout@ * result.waves@).min(duration_max_nanos())))]
pub fn project_top_budget(
    selected: usize,
    jobs: NonZeroUsize,
    planned_total_timeout: Duration,
    baseline: Duration,
    mutant_timeout: MutantTimeout,
    remaining: Duration,
) -> TopBudgetProjection {
    let jobs = jobs.get();
    let waves = selected / jobs + usize::from(!selected.is_multiple_of(jobs));
    let effective_mutant_timeout = match mutant_timeout {
        MutantTimeout::Auto => auto_mutant_timeout(baseline),
        MutantTimeout::Fixed(value) => value.get(),
    };
    let projected_capacity = saturating_duration_mul(effective_mutant_timeout, waves);

    TopBudgetProjection {
        selected,
        jobs,
        planned_total_timeout,
        baseline,
        effective_mutant_timeout,
        remaining,
        waves,
        projected_capacity,
    }
}

#[cfg_attr(creusot, ensures(result@ == (duration@ * factor@).min(duration_max_nanos())))]
fn saturating_duration_mul(duration: Duration, factor: usize) -> Duration {
    let Some(nanos) = duration.as_nanos().checked_mul(factor as u128) else {
        return Duration::MAX;
    };
    if nanos > Duration::MAX.as_nanos() {
        return Duration::MAX;
    }
    duration_from_nanos(nanos)
}

// The caller has already bounded the total; both casts preserve its exact value.
#[cfg_attr(creusot, requires(nanos@ <= duration_max_nanos()))]
#[cfg_attr(creusot, ensures(result@ == nanos@))]
fn duration_from_nanos(nanos: u128) -> Duration {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "total nanoseconds are at most Duration::MAX"
    )]
    let seconds = (nanos / NANOS_PER_SECOND) as u64;
    let subsec_nanos = (nanos % NANOS_PER_SECOND) as u32;
    Duration::new(seconds, subsec_nanos)
}
