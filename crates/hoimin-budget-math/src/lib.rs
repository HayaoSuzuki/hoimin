#![forbid(unsafe_code)]

mod duration;
mod projection;

pub use duration::{MutantTimeout, NonZeroDuration, auto_mutant_timeout};
pub use projection::{TopBudgetProjection, project_top_budget};
