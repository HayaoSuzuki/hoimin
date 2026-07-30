use std::num::NonZeroUsize;
use std::time::Duration;

use crate::{MutantTimeout, auto_mutant_timeout};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

impl TopBudgetProjection {
    #[must_use]
    pub fn is_shortfall(&self) -> bool {
        self.projected_capacity > self.remaining
    }
}

#[must_use]
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
    let projected_capacity = match u32::try_from(waves) {
        Ok(waves) => effective_mutant_timeout
            .checked_mul(waves)
            .unwrap_or(Duration::MAX),
        Err(_) => Duration::MAX,
    };

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
