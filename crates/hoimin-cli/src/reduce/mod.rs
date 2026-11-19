//! Runner-independent, bounded search for whole Python definition removals.
//! Session owns immutable inputs and speculative evidence; only policy-certified
//! evidence can become an applicable diff.
mod artifacts;
mod input;
mod runner;
mod session;
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ReduceArgs {
    pub plan: PathBuf,
    #[arg(long, required = true)]
    pub candidate: Vec<String>,
    /// Select root-relative `PATH:QUALNAME_GLOB`; repeat for additional definitions.
    #[arg(long, required = true)]
    pub test: Vec<String>,
    /// Keep definitions using the same `PATH:QUALNAME_GLOB` syntax; keep wins.
    #[arg(long)]
    pub keep: Vec<String>,
    /// Absolute search budget, including original evaluation and final confirmation.
    #[arg(long, required = true)]
    pub total_timeout: String,
    #[arg(long, default_value = "5")]
    pub min_reduction: String,
    /// New artifact directory outside the project.
    #[arg(long, required = true)]
    pub output: PathBuf,
}

pub(crate) async fn run(args: ReduceArgs) -> Result<i32, String> {
    session::run(args).await
}
