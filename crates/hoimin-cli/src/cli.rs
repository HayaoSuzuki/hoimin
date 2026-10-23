use std::ffi::OsString;
use std::fmt;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use camino::Utf8PathBuf;
use clap::{ArgGroup, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use hoimin_core::{
    CommandArg, ConfigError, LineRange, LineSelection, MutationOperatorSelection, MutationProfile,
    OutputConfig, RawRunConfig, RawRunLimits, RunConfig, SessionConfig, SymbolSelection,
};

#[derive(Debug, Parser)]
#[command(
    name = "hoimin",
    version,
    about = "Bounded mutation testing for focused Python changes",
    after_help = "Run contract:\n  hoimin run [TARGETS] [SAFETY/OUTPUT/SESSION OPTIONS] -- <TEST_ARGV>...\n\nTarget selectors:\n  --root --source --file --line --symbol --changed --changed-context --diff-base\nCopy options:\n  --include --exclude\nMutation options:\n  --operators --exclude-operators --profile\nSafety options:\n  --jobs --max-mutants --max-candidates\n  --baseline-timeout --mutant-timeout --total-timeout --max-memory\n  --max-output --max-copy-size --max-workspace-size --min-free-space\n  --max-processes --allow-best-effort-memory\nOutput/session options:\n  --metrics <PATH> --session --resume"
)]
struct RootCli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
#[allow(
    clippy::large_enum_variant,
    reason = "command parsing immediately consumes the enum and keeps both argument values direct"
)]
enum Command {
    /// Run mutation tests for an explicitly selected target.
    Run(RunArgs),
    /// Discover mutation candidates without executing tests.
    Plan(RawPlanArgs),
    /// Execute one or more candidates using settings from a previously generated plan.
    Verify(RawVerifyArgs),
    /// Compare chronologically ordered mutation run reports.
    Progress(RawProgressArgs),
    /// Record and inspect human assessments independently of test outcomes.
    Review(ReviewCommand),
}

#[derive(Debug, Args)]
struct ReviewCommand {
    #[command(subcommand)]
    command: ReviewArgs,
}

#[derive(Debug, Subcommand)]
pub enum ReviewArgs {
    /// List current candidates and their assessments.
    List(ReviewListArgs),
    /// Show one candidate and its review history.
    Show(ReviewShowArgs),
    /// Append a human assessment using an expected revision.
    Record(ReviewRecordArgs),
}

#[derive(Debug, Args)]
pub struct ReviewCommonArgs {
    pub plan: PathBuf,
    #[arg(long)]
    pub store: PathBuf,
    #[arg(long)]
    pub report: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ReviewListArgs {
    #[command(flatten)]
    pub common: ReviewCommonArgs,
    /// Include stored candidates absent from the current plan in a separate list.
    #[arg(long)]
    pub stale: bool,
}

#[derive(Debug, Args)]
pub struct ReviewShowArgs {
    #[command(flatten)]
    pub common: ReviewCommonArgs,
    #[arg(long)]
    pub candidate: String,
}

#[derive(Debug, Args)]
pub struct ReviewRecordArgs {
    #[command(flatten)]
    pub common: ReviewCommonArgs,
    #[arg(long)]
    pub candidate: String,
    #[arg(long, value_enum)]
    pub assessment: crate::review::model::Assessment,
    #[arg(long)]
    pub reason: String,
    #[arg(long)]
    pub evidence: Vec<String>,
    #[arg(long)]
    pub expected_revision: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum ProfileArg {
    #[default]
    Full,
    Focused,
}

impl From<ProfileArg> for MutationProfile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::Full => Self::Full,
            ProfileArg::Focused => Self::Focused,
        }
    }
}

#[derive(Debug, Args)]
struct RawMutationArgs {
    /// Project root used to resolve relative paths.
    #[arg(long, default_value = ".", value_name = "DIR")]
    root: PathBuf,

    /// Source root containing mutation targets; may be repeated.
    #[arg(long, value_name = "DIR")]
    source: Vec<PathBuf>,

    /// Worker import directory relative to --root; repeat in precedence order without selecting targets.
    #[arg(long, value_name = "DIR")]
    import_root: Vec<PathBuf>,

    /// Select a complete Python file; may be repeated.
    #[arg(long, value_name = "PATH")]
    file: Vec<PathBuf>,

    /// Select PATH:START-END (an inclusive line range); may be repeated.
    #[arg(long, value_name = "PATH:START-END")]
    line: Vec<String>,

    /// Select MODULE:QUALNAME; may be repeated.
    #[arg(long, value_name = "MODULE:QUALNAME")]
    symbol: Vec<String>,

    /// Restrict targets to changed Git lines.
    #[arg(long)]
    changed: bool,

    /// Include N neighboring lines around Git changes (0..=1073741823).
    #[arg(long, value_name = "N", default_value_t = 0, requires = "changed", value_parser = clap::value_parser!(u32).range(0..=i64::from(hoimin_core::MAX_CHANGED_CONTEXT)))]
    changed_context: u32,

    /// Compare changed lines with the merge-base of REV and HEAD.
    #[arg(long, value_name = "REV")]
    diff_base: Option<String>,

    /// Include a normally ignored path while copying; may be repeated.
    #[arg(long, value_name = "GLOB")]
    include: Vec<String>,

    /// Add a root-relative file glob to the session fingerprint; may be repeated.
    #[arg(long, value_name = "GLOB")]
    fingerprint_include: Vec<String>,

    /// Add one exact root-relative file to the session fingerprint; may be repeated.
    #[arg(long, value_name = "PATH")]
    fingerprint_file: Vec<String>,

    /// Track inherited NAME before worker rewriting; repeatable ASCII [A-Za-z_][A-Za-z0-9_]* (Unix case-sensitive, Windows uppercase).
    #[arg(long, value_name = "NAME")]
    fingerprint_env: Vec<String>,

    /// Exclude a path while copying; may be repeated and wins over include.
    #[arg(long, value_name = "GLOB")]
    exclude: Vec<String>,

    /// Include only named mutation operators; omit for the default runtime set. Repeatable or comma-delimited.
    #[arg(long, value_delimiter = ',')]
    operators: Vec<String>,

    /// Candidate-selection profile.
    #[arg(long, value_enum, default_value_t = ProfileArg::Full)]
    profile: ProfileArg,

    /// Exclude named mutation operators; may be repeated or comma-delimited.
    #[arg(long, value_delimiter = ',')]
    exclude_operators: Vec<String>,

    /// Maximum concurrently active workers.
    #[arg(long, default_value_t = 1)]
    jobs: usize,

    /// Maximum mutants to execute.
    #[arg(long, default_value_t = 100)]
    max_mutants: usize,

    /// Maximum candidates to discover before mutation starts.
    #[arg(long, default_value_t = 10_000)]
    max_candidates: usize,

    /// Baseline test timeout.
    #[arg(long, default_value = "60s", value_name = "DURATION")]
    baseline_timeout: String,

    /// Per-mutant timeout or `auto`.
    #[arg(long, default_value = "auto", value_name = "DURATION|auto")]
    mutant_timeout: String,

    /// Run time budget, including environment capture, fingerprint reads, and execution setup.
    ///
    /// Verify starts this budget after saved-plan validation and reanalysis. Preparation
    /// consumes execution time; shutdown uses one additional two-second grace. An OS
    /// operation already in progress may finish later while retaining its resource owners.
    #[arg(long, default_value = "5m", value_name = "DURATION")]
    total_timeout: String,

    /// Memory limit (Windows: per root process tree, committed memory; jobs multiplies total allowance).
    #[arg(long, default_value = "1GiB", value_name = "BYTES")]
    max_memory: String,

    /// Retained stdout and stderr per process.
    #[arg(long, default_value = "1MiB", value_name = "BYTES")]
    max_output: String,

    /// Run-wide logical copy-size limit.
    #[arg(long, default_value = "1GiB", value_name = "BYTES")]
    max_copy_size: String,

    /// Run-wide logical size of Hoimin-owned workspaces, including generated files.
    #[arg(long, default_value = "8GiB", value_name = "BYTES")]
    max_workspace_size: String,

    /// Mandatory minimum available bytes preserved on every owned-workspace filesystem.
    #[arg(long, default_value = "10GiB", value_name = "BYTES")]
    min_free_space: String,

    /// Process limit (Windows: per root process tree, including the root; jobs multiplies total allowance).
    #[arg(long, default_value_t = 64)]
    max_processes: usize,

    /// Permit best-effort memory enforcement when hard limits are unavailable.
    #[arg(long)]
    allow_best_effort_memory: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[command(flatten)]
    mutation: RawMutationArgs,

    /// Write performance metrics to PATH.
    #[arg(long, value_name = "PATH")]
    pub metrics: Option<PathBuf>,

    /// `SQLite` session path; no database is created unless specified.
    #[arg(long, value_name = "PATH")]
    pub session: Option<PathBuf>,

    /// Resume the newest compatible incomplete run in the session database.
    #[arg(long)]
    pub resume: bool,

    /// Test executable and arguments, passed directly without a shell.
    #[arg(last = true, num_args = 1.., value_name = "TEST_ARGV")]
    pub test_argv: Vec<OsString>,
}

#[derive(Debug, Args)]
struct RawPlanArgs {
    /// Cooperative deadline for discovery across all selected targets, inherited by verify.
    #[arg(long, default_value = "30s", value_name = "DURATION")]
    analyzer_timeout: String,

    #[command(flatten)]
    mutation: RawMutationArgs,

    /// Test executable and arguments, passed directly without a shell.
    #[arg(last = true, num_args = 1.., value_name = "TEST_ARGV")]
    test_argv: Vec<OsString>,
}

#[derive(Debug, Args)]
#[command(
    group(
        ArgGroup::new("selection")
            .required(true)
            .multiple(false)
            .args(["candidate_ids", "top", "sample"])
    ),
    after_help = "Execution and resource settings come from PLAN and cannot be overridden. Disk safety limits --max-workspace-size and --min-free-space are inherited from PLAN. Create a new plan to change them."
)]
struct RawVerifyArgs {
    /// Path to a version-5 plan manifest.
    #[arg(value_name = "PLAN")]
    manifest: PathBuf,

    /// Candidate ID to execute; repeat for multiple planned candidates.
    #[arg(long = "candidate", value_name = "ID")]
    candidate_ids: Vec<String>,

    /// Execute the N highest-ranked candidates retained in the plan; strict order is the default.
    #[arg(long, value_name = "N")]
    top: Option<NonZeroUsize>,

    /// Sample N candidates without replacement from a complete plan.
    #[arg(long, value_name = "N", requires = "seed", conflicts_with_all = ["offset", "selection_policy"])]
    sample: Option<NonZeroUsize>,

    /// Explicit reproducible sampling seed (unsigned 64-bit integer).
    #[arg(long, value_name = "SEED", requires = "sample", conflicts_with_all = ["top", "candidate_ids"])]
    seed: Option<u64>,

    /// Skip K candidates in the complete selected-policy ordering before taking --top N.
    #[arg(
        long,
        requires = "top",
        conflicts_with = "candidate_ids",
        value_name = "K"
    )]
    offset: Option<usize>,

    /// Select saved-rank order or equal-score file/line diversity for --top.
    ///
    /// Diverse round-robins files; line-diverse round-robins (file, start-line) groups.
    /// Both keep higher-score tiers first.
    #[arg(
        long,
        value_enum,
        requires = "top",
        conflicts_with = "candidate_ids",
        value_name = "POLICY"
    )]
    selection_policy: Option<TopSelectionPolicy>,

    /// Validate and preview selected candidates without executing tests.
    #[arg(long, conflicts_with = "metrics")]
    dry_run: bool,

    /// Write execution metrics to PATH, relative to the invocation directory.
    #[arg(long, value_name = "PATH")]
    metrics: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct RawProgressArgs {
    /// Consecutive unchanged comparisons before the history is saturated.
    #[arg(
        long,
        default_value_t = NonZeroUsize::new(3).unwrap(),
        value_parser = clap::value_parser!(NonZeroUsize)
    )]
    patience: NonZeroUsize,

    /// Show identified changes in the final adjacent input pair.
    #[arg(long)]
    details: bool,

    /// Maximum transition details to show; zero reports only omission counts.
    #[arg(long, default_value_t = 100, requires = "details")]
    details_limit: usize,

    /// Ordered run report files to compare.
    #[arg(required = true, num_args = 2.., value_name = "REPORT")]
    reports: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct ProgressArgs {
    pub details: bool,
    pub details_limit: usize,
    pub reports: Vec<PathBuf>,
    pub patience: NonZeroUsize,
}

#[derive(Debug)]
pub struct PlanArgs {
    analyzer_timeout: String,
    run_args: RunArgs,
}

impl PlanArgs {
    /// Normalizes the execution template and the separately owned discovery settings.
    ///
    /// Planning always uses JSON output and does not retain a session or resume state.
    ///
    /// # Errors
    ///
    /// Returns an error when the shared mutation arguments or discovery timeout are invalid.
    pub fn into_configs(
        self,
    ) -> Result<
        (
            RunConfig<hoimin_core::EnvironmentRequest>,
            hoimin_core::DiscoveryConfig,
        ),
        CliError,
    > {
        let discovery = hoimin_core::DiscoveryConfig::new(parse_duration(
            &self.analyzer_timeout,
            "--analyzer-timeout",
        )?)?;
        Ok((run_config_from_args(self.run_args)?, discovery))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum TopSelectionPolicy {
    Strict,
    Diverse,
    LineDiverse,
}

#[derive(Debug, Eq, PartialEq)]
pub enum VerifySelection {
    CandidateIds(Vec<String>),
    Sample {
        count: NonZeroUsize,
        seed: u64,
    },
    Top {
        count: NonZeroUsize,
        policy: TopSelectionPolicy,
        offset: usize,
    },
}

#[derive(Debug)]
pub struct VerifyArgs {
    pub dry_run: bool,
    pub metrics: Option<Utf8PathBuf>,
    pub manifest: PathBuf,
    pub selection: VerifySelection,
}

#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "the public parser API deliberately exposes direct RunArgs and ProgressArgs values"
)]
pub enum ParsedCommand {
    Run(RunArgs),
    Plan(PlanArgs),
    Verify(VerifyArgs),
    Progress(ProgressArgs),
    Review(ReviewArgs),
}

#[derive(Debug)]
pub enum CliError {
    Clap(clap::Error),
    ProgressCommand,
    MissingTargetSelector,
    MissingTestArgv,
    InvalidValue { name: &'static str, value: String },
    NonUtf8Value(&'static str),
    Config(ConfigError),
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clap(error) => error.fmt(formatter),
            Self::ProgressCommand => {
                formatter.write_str("the `progress` command does not produce a run configuration")
            }
            Self::MissingTargetSelector => {
                formatter.write_str("at least one target selector is required")
            }
            Self::MissingTestArgv => {
                formatter.write_str("at least one test argv element is required after `--`")
            }
            Self::InvalidValue { name, value } => match *name {
                "--max-memory"
                | "--max-output"
                | "--max-copy-size"
                | "--max-workspace-size"
                | "--min-free-space" => write!(
                    formatter,
                    "invalid {name}: {value}; expected bytes with one of: B, KB, MB, GB, KiB, MiB, GiB"
                ),
                "--analyzer-timeout" | "--baseline-timeout" | "--mutant-timeout"
                | "--total-timeout" => write!(
                    formatter,
                    "invalid {name}: {value}; expected a duration such as 90s or 5m"
                ),
                "--line" => write!(
                    formatter,
                    "invalid {name}: {value}; expected PATH:START-END with 1-based START <= END"
                ),
                _ => write!(formatter, "invalid {name}: {value}"),
            },
            Self::NonUtf8Value(name) => write!(formatter, "{name} must be valid UTF-8"),
            Self::Config(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CliError {}

impl From<clap::Error> for CliError {
    fn from(error: clap::Error) -> Self {
        Self::Clap(error)
    }
}

impl From<ConfigError> for CliError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl CliError {
    #[must_use]
    pub fn config_error(&self) -> Option<&ConfigError> {
        match self {
            Self::Config(error) => Some(error),
            _ => None,
        }
    }
}

impl TryFrom<Command> for ParsedCommand {
    type Error = CliError;

    fn try_from(command: Command) -> Result<Self, Self::Error> {
        match command {
            Command::Run(args) => {
                validate_mutation_inputs(&args.mutation, &args.test_argv)?;
                Ok(Self::Run(args))
            }
            Command::Plan(raw) => {
                validate_mutation_inputs(&raw.mutation, &raw.test_argv)?;
                Ok(Self::Plan(PlanArgs {
                    analyzer_timeout: raw.analyzer_timeout,
                    run_args: RunArgs {
                        mutation: raw.mutation,
                        test_argv: raw.test_argv,
                        metrics: None,
                        session: None,
                        resume: false,
                    },
                }))
            }
            Command::Verify(mut raw) => {
                let mut seen = std::collections::BTreeSet::new();
                raw.candidate_ids.retain(|id| seen.insert(id.clone()));
                let selection = match raw.top {
                    Some(count) => VerifySelection::Top {
                        count,
                        policy: raw.selection_policy.unwrap_or(TopSelectionPolicy::Strict),
                        offset: raw.offset.unwrap_or_default(),
                    },
                    None => match raw.sample {
                        Some(count) => VerifySelection::Sample {
                            count,
                            seed: raw.seed.expect("clap requires --seed with --sample"),
                        },
                        None => VerifySelection::CandidateIds(raw.candidate_ids),
                    },
                };
                Ok(Self::Verify(VerifyArgs {
                    dry_run: raw.dry_run,
                    metrics: raw.metrics.map(metrics_path).transpose()?,
                    manifest: raw.manifest,
                    selection,
                }))
            }
            Command::Progress(raw) => Ok(Self::Progress(ProgressArgs {
                details: raw.details,
                details_limit: raw.details_limit,
                reports: raw.reports,
                patience: raw.patience,
            })),
            Command::Review(args) => Ok(Self::Review(args.command)),
        }
    }
}

/// Builds the shared command definition used by parsing and documentation.
/// Usage/help use the trusted product name even when the executable is renamed.
#[must_use]
pub fn root_command() -> clap::Command {
    let operator_roster = operator_help();
    RootCli::command()
        .bin_name("hoimin")
        .mut_subcommand("run", |command| command.after_help(operator_roster.clone()))
        .mut_subcommand("plan", |command| command.after_help(operator_roster))
}

fn operator_help() -> String {
    let names = MutationOperatorSelection::valid_names();
    let rows = names
        .chunks(4)
        .map(|row| row.join(", "))
        .collect::<Vec<_>>()
        .join("\n  ");
    format!("Mutation operator IDs and selector families:\n  {rows}")
}

fn validate_mutation_inputs(raw: &RawMutationArgs, test_argv: &[OsString]) -> Result<(), CliError> {
    let has_selector = !raw.source.is_empty()
        || !raw.file.is_empty()
        || !raw.line.is_empty()
        || !raw.symbol.is_empty()
        || raw.changed;
    if !has_selector {
        return Err(CliError::MissingTargetSelector);
    }
    if test_argv.is_empty() {
        return Err(CliError::MissingTestArgv);
    }

    Ok(())
}

/// Parses command-line arguments into a command and its executable arguments.
///
/// # Errors
///
/// Returns an error when Clap rejects the arguments or required inputs are absent.
pub fn parse_from<I, T>(args: I) -> Result<ParsedCommand, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let matches = root_command().try_get_matches_from(args)?;
    let root = RootCli::from_arg_matches(&matches)?;
    ParsedCommand::try_from(root.command)
}

/// Parses command-line arguments and validates a run configuration.
///
/// # Errors
///
/// Returns an error when an argument or configuration value is invalid.
pub fn parse_config_from<I, T>(
    args: I,
) -> Result<RunConfig<hoimin_core::EnvironmentRequest>, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let ParsedCommand::Run(args) = parse_from(args)? else {
        return Err(CliError::ProgressCommand);
    };
    run_config_from_args(args)
}

pub(crate) fn run_config_from_args(
    args: RunArgs,
) -> Result<RunConfig<hoimin_core::EnvironmentRequest>, CliError> {
    RunConfig::try_from(raw_config(args)?).map_err(CliError::from)
}

fn raw_config(args: RunArgs) -> Result<RawRunConfig, CliError> {
    let RunArgs {
        mutation: args,
        metrics,
        session,
        resume,
        test_argv,
    } = args;
    let root = utf8_path(args.root, "--root")?;
    let sources = args
        .source
        .into_iter()
        .map(|path| utf8_path(path, "--source"))
        .collect::<Result<Vec<_>, _>>()?;
    let files = args
        .file
        .into_iter()
        .map(|path| utf8_path(path, "--file"))
        .collect::<Result<Vec<_>, _>>()?;
    let lines = args
        .line
        .iter()
        .map(|value| parse_line_selection(value))
        .collect::<Result<Vec<_>, _>>()?;
    let symbols = args
        .symbol
        .iter()
        .map(|value| parse_symbol_selection(value))
        .collect::<Result<Vec<_>, _>>()?;
    let session = session
        .map(|path| utf8_path(path, "--session").map(|path| SessionConfig { path }))
        .transpose()?;
    let metrics = metrics.map(metrics_path).transpose()?;
    let limits = RawRunLimits {
        jobs: args.jobs,
        max_mutants: args.max_mutants,
        max_candidates: args.max_candidates,
        baseline_timeout: parse_duration(&args.baseline_timeout, "--baseline-timeout")?,
        mutant_timeout: if args.mutant_timeout == "auto" {
            None
        } else {
            Some(parse_duration(&args.mutant_timeout, "--mutant-timeout")?)
        },
        total_timeout: parse_duration(&args.total_timeout, "--total-timeout")?,
        max_memory: parse_bytes(&args.max_memory, "--max-memory")?,
        max_output: parse_bytes(&args.max_output, "--max-output")?,
        max_copy_size: parse_bytes(&args.max_copy_size, "--max-copy-size")?,
        max_workspace_size: parse_bytes(&args.max_workspace_size, "--max-workspace-size")?,
        min_free_space: parse_bytes(&args.min_free_space, "--min-free-space")?,
        max_processes: args.max_processes,
    };
    Ok(RawRunConfig {
        root,
        sources,
        import_roots: args
            .import_root
            .into_iter()
            .map(|path| utf8_path(path, "--import-root"))
            .collect::<Result<Vec<_>, _>>()?,
        files,
        lines,
        symbols,
        changed: args.changed,
        changed_context: args.changed_context,
        diff_base: args.diff_base,
        includes: args.include,
        excludes: args.exclude,
        fingerprint_includes: args.fingerprint_include,
        fingerprint_files: args.fingerprint_file,
        fingerprint_env: args.fingerprint_env,
        operators: args.operators,
        exclude_operators: args.exclude_operators,
        allow_best_effort_memory: args.allow_best_effort_memory,
        profile: args.profile.into(),
        limits,
        test_argv: test_argv.iter().map(command_arg).collect(),
        output: OutputConfig { metrics },
        session,
        resume,
    })
}

fn metrics_path(path: PathBuf) -> Result<Utf8PathBuf, CliError> {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|error| CliError::InvalidValue {
                name: "--metrics",
                value: error.to_string(),
            })?
            .join(path)
    };
    utf8_path(path, "--metrics")
}

fn utf8_path(path: PathBuf, name: &'static str) -> Result<Utf8PathBuf, CliError> {
    Utf8PathBuf::from_path_buf(path).map_err(|_| CliError::NonUtf8Value(name))
}

fn parse_line_selection(value: &str) -> Result<LineSelection, CliError> {
    let (path, range) = value
        .rsplit_once(':')
        .ok_or_else(|| invalid_line_selection(value))?;
    if path.is_empty() {
        return Err(invalid_line_selection(value));
    }
    let (start, end) = range.split_once('-').unwrap_or((range, range));
    let start = start
        .parse::<u32>()
        .map_err(|_| invalid_line_selection(value))?;
    let end = end
        .parse::<u32>()
        .map_err(|_| invalid_line_selection(value))?;
    if start == 0 || start > end {
        return Err(invalid_line_selection(value));
    }
    let path =
        crate::portable_path::from_native(path).map_err(|_| invalid_line_selection(value))?;
    Ok(LineSelection {
        path: Utf8PathBuf::from(path.into_owned()),
        range: LineRange { start, end },
    })
}

fn invalid_line_selection(value: &str) -> CliError {
    CliError::InvalidValue {
        name: "--line",
        value: value.to_owned(),
    }
}

fn parse_symbol_selection(value: &str) -> Result<SymbolSelection, CliError> {
    let (module, qualname) = value
        .split_once(':')
        .ok_or_else(|| CliError::InvalidValue {
            name: "--symbol",
            value: value.to_owned(),
        })?;
    if module.is_empty() || qualname.is_empty() {
        return Err(CliError::InvalidValue {
            name: "--symbol",
            value: value.to_owned(),
        });
    }
    Ok(SymbolSelection {
        module: module.to_owned(),
        qualname: qualname.to_owned(),
    })
}

fn parse_duration(value: &str, name: &'static str) -> Result<std::time::Duration, CliError> {
    humantime::parse_duration(value).map_err(|_| CliError::InvalidValue {
        name,
        value: value.to_owned(),
    })
}

fn parse_bytes(value: &str, name: &'static str) -> Result<u64, CliError> {
    const SUFFIXES: [(&str, u64); 7] = [
        ("GiB", 1024 * 1024 * 1024),
        ("MiB", 1024 * 1024),
        ("KiB", 1024),
        ("GB", 1_000_000_000),
        ("MB", 1_000_000),
        ("KB", 1_000),
        ("B", 1),
    ];
    for (suffix, multiplier) in SUFFIXES {
        if let Some(number) = value.strip_suffix(suffix) {
            return number
                .parse::<u64>()
                .ok()
                .and_then(|number| number.checked_mul(multiplier))
                .ok_or_else(|| CliError::InvalidValue {
                    name,
                    value: value.to_owned(),
                });
        }
    }
    value.parse::<u64>().map_err(|_| CliError::InvalidValue {
        name,
        value: value.to_owned(),
    })
}

#[cfg(unix)]
fn command_arg(value: &OsString) -> CommandArg {
    use std::os::unix::ffi::OsStrExt;
    CommandArg::Unix(value.as_bytes().to_vec())
}

#[cfg(windows)]
fn command_arg(value: &OsString) -> CommandArg {
    use std::os::windows::ffi::OsStrExt;
    CommandArg::Windows(value.encode_wide().collect())
}
