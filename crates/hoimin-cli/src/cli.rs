use std::ffi::OsString;
use std::fmt;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use camino::Utf8PathBuf;
use clap::{ArgGroup, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use hoimin_core::{
    CommandArg, ConfigError, LineRange, LineSelection, MutationOperatorSelection, MutationProfile,
    OutputConfig, RawRunConfig, RawRunLimits, RunConfig, SessionConfig, SymbolSelection,
};

#[derive(Debug, Parser)]
#[command(
    name = "hoimin",
    version,
    about = "Bounded mutation testing for focused Python changes",
    after_help = "Run contract:\n  hoimin run [TARGETS] [SAFETY/OUTPUT/SESSION OPTIONS] -- <TEST_ARGV>...\n\nTarget selectors:\n  --root --source --file --line --symbol --changed --changed-context --diff-base\nCopy options:\n  --include --exclude\nMutation options:\n  --operators --exclude-operators --profile\nSafety options:\n  --jobs --max-mutants --max-candidates --analyzer-timeout\n  --baseline-timeout --mutant-timeout --total-timeout --max-memory\n  --max-output --max-copy-size --max-workspace-size --min-free-space\n  --max-processes --allow-best-effort-memory\nOutput/session options:\n  --format <json|jsonl|human> --metrics <PATH> --session --resume"
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
    Run(RawRunArgs),
    /// Discover mutation candidates without executing tests.
    Plan(RawPlanArgs),
    /// Execute one or more candidates using settings from a previously generated plan.
    Verify(RawVerifyArgs),
    /// Compare chronologically ordered mutation run reports.
    Progress(RawProgressArgs),
    /// Generate shell completion scripts.
    Completions(CompletionsArgs),
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: Shell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Json,
    Jsonl,
    Human,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ProgressOutputFormat {
    Human,
    Json,
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

    /// Exclude a path while copying; may be repeated and wins over include.
    #[arg(long, value_name = "GLOB")]
    exclude: Vec<String>,

    /// Include only named mutation operators; may be repeated or comma-delimited.
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

    /// Per-analyzer-process timeout.
    #[arg(long, default_value = "30s", value_name = "DURATION")]
    analyzer_timeout: String,

    /// Baseline test timeout.
    #[arg(long, default_value = "60s", value_name = "DURATION")]
    baseline_timeout: String,

    /// Per-mutant timeout or `auto`.
    #[arg(long, default_value = "auto", value_name = "DURATION|auto")]
    mutant_timeout: String,

    /// Wall-clock timeout for the complete run.
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
struct RawRunArgs {
    #[command(flatten)]
    mutation: RawMutationArgs,

    /// Machine-readable output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,

    /// Write performance metrics to PATH.
    #[arg(long, value_name = "PATH")]
    metrics: Option<PathBuf>,

    /// `SQLite` session path; no database is created unless specified.
    #[arg(long, value_name = "PATH")]
    session: Option<PathBuf>,

    /// Resume the newest compatible incomplete run in the session database.
    #[arg(long)]
    resume: bool,

    /// Test executable and arguments, passed directly without a shell.
    #[arg(last = true, num_args = 1.., value_name = "TEST_ARGV")]
    test_argv: Vec<OsString>,
}

#[derive(Debug, Args)]
struct RawPlanArgs {
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
            .args(["candidate_ids", "top"])
    ),
    after_help = "Execution and resource settings come from PLAN and cannot be overridden. Disk safety limits --max-workspace-size and --min-free-space are inherited from PLAN. Create a new plan to change them."
)]
struct RawVerifyArgs {
    /// Path to a version-4 plan manifest.
    #[arg(value_name = "PLAN")]
    manifest: PathBuf,

    /// Candidate ID to execute; repeat for multiple planned candidates.
    #[arg(long = "candidate", value_name = "ID")]
    candidate_ids: Vec<String>,

    /// Execute the N highest-ranked candidates retained in the plan; strict order is the default.
    #[arg(long, value_name = "N")]
    top: Option<NonZeroUsize>,

    /// Skip K candidates in the complete selected-policy ordering before taking --top N.
    #[arg(
        long,
        requires = "top",
        conflicts_with = "candidate_ids",
        value_name = "K"
    )]
    offset: Option<usize>,

    /// Select strict saved-rank order or equal-score file diversity for --top.
    ///
    /// Diverse round-robins files only within equal-score tiers.
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

    /// Machine-readable output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
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

    /// Render the comparison as human-readable text or JSON.
    #[arg(long, value_enum, default_value_t = ProgressOutputFormat::Human)]
    format: ProgressOutputFormat,

    /// Ordered run report files to compare.
    #[arg(required = true, num_args = 2.., value_name = "REPORT")]
    reports: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct RunArgs {
    pub root: PathBuf,
    pub source: Vec<PathBuf>,
    pub import_root: Vec<PathBuf>,
    pub file: Vec<PathBuf>,
    pub line: Vec<String>,
    pub symbol: Vec<String>,
    pub changed: bool,
    pub changed_context: u32,
    pub diff_base: Option<String>,
    pub include: Vec<String>,
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_files: Vec<String>,
    pub exclude: Vec<String>,
    pub operators: Vec<String>,
    profile: ProfileArg,
    pub exclude_operators: Vec<String>,
    pub jobs: usize,
    pub max_mutants: usize,
    pub max_candidates: usize,
    pub analyzer_timeout: String,
    pub baseline_timeout: String,
    pub mutant_timeout: String,
    pub total_timeout: String,
    pub max_memory: String,
    pub max_output: String,
    pub max_copy_size: String,
    pub max_workspace_size: String,
    pub min_free_space: String,
    pub max_processes: usize,
    pub allow_best_effort_memory: bool,
    pub format: OutputFormat,
    pub metrics: Option<PathBuf>,
    pub session: Option<PathBuf>,
    pub resume: bool,
    pub test_argv: Vec<OsString>,
}

#[derive(Debug)]
pub struct ProgressArgs {
    pub reports: Vec<PathBuf>,
    pub patience: NonZeroUsize,
    pub format: ProgressOutputFormat,
}

#[derive(Debug)]
pub struct PlanArgs {
    run_args: RunArgs,
}

impl PlanArgs {
    /// Converts planning arguments into the normalized configuration used to create a manifest.
    ///
    /// Planning always uses JSON output and does not retain a session or resume state.
    ///
    /// # Errors
    ///
    /// Returns an error when the shared mutation arguments are invalid.
    pub fn into_run_config(self) -> Result<RunConfig, CliError> {
        run_config_from_args(self.run_args)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum TopSelectionPolicy {
    Strict,
    Diverse,
}

#[derive(Debug, Eq, PartialEq)]
pub enum VerifySelection {
    CandidateIds(Vec<String>),
    Top {
        count: NonZeroUsize,
        policy: TopSelectionPolicy,
    },
    TopRange {
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
    pub format: OutputFormat,
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
    Completions(CompletionsArgs),
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
            Command::Run(raw) => Ok(Self::Run(run_args_from_mutation(
                raw.mutation,
                raw.test_argv,
                raw.format,
                raw.metrics,
                raw.session,
                raw.resume,
            )?)),
            Command::Plan(raw) => Ok(Self::Plan(PlanArgs {
                run_args: run_args_from_mutation(
                    raw.mutation,
                    raw.test_argv,
                    OutputFormat::Json,
                    None,
                    None,
                    false,
                )?,
            })),
            Command::Verify(mut raw) => {
                let mut seen = std::collections::BTreeSet::new();
                raw.candidate_ids.retain(|id| seen.insert(id.clone()));
                let selection = match raw.top {
                    Some(count) if raw.offset.is_some() => VerifySelection::TopRange {
                        count,
                        policy: raw.selection_policy.unwrap_or(TopSelectionPolicy::Strict),
                        offset: raw.offset.unwrap_or_default(),
                    },
                    Some(count) => VerifySelection::Top {
                        count,
                        policy: raw.selection_policy.unwrap_or(TopSelectionPolicy::Strict),
                    },
                    None => VerifySelection::CandidateIds(raw.candidate_ids),
                };
                Ok(Self::Verify(VerifyArgs {
                    dry_run: raw.dry_run,
                    metrics: raw.metrics.map(metrics_path).transpose()?,
                    manifest: raw.manifest,
                    selection,
                    format: raw.format,
                }))
            }
            Command::Progress(raw) => Ok(Self::Progress(ProgressArgs {
                reports: raw.reports,
                patience: raw.patience,
                format: raw.format,
            })),
            Command::Completions(args) => Ok(Self::Completions(args)),
        }
    }
}

pub fn write_completions(shell: Shell, writer: &mut impl std::io::Write) {
    clap_complete::generate(shell, &mut root_command(), "hoimin", writer);
}

fn root_command() -> clap::Command {
    let operator_roster = operator_help();
    RootCli::command()
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

fn run_args_from_mutation(
    raw: RawMutationArgs,
    test_argv: Vec<OsString>,
    format: OutputFormat,
    metrics: Option<PathBuf>,
    session: Option<PathBuf>,
    resume: bool,
) -> Result<RunArgs, CliError> {
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

    Ok(RunArgs {
        root: raw.root,
        source: raw.source,
        import_root: raw.import_root,
        file: raw.file,
        line: raw.line,
        symbol: raw.symbol,
        changed: raw.changed,
        changed_context: raw.changed_context,
        diff_base: raw.diff_base,
        include: raw.include,
        fingerprint_includes: raw.fingerprint_include,
        fingerprint_files: raw.fingerprint_file,
        exclude: raw.exclude,
        operators: raw.operators,
        profile: raw.profile,
        exclude_operators: raw.exclude_operators,
        jobs: raw.jobs,
        max_mutants: raw.max_mutants,
        max_candidates: raw.max_candidates,
        analyzer_timeout: raw.analyzer_timeout,
        baseline_timeout: raw.baseline_timeout,
        mutant_timeout: raw.mutant_timeout,
        total_timeout: raw.total_timeout,
        max_memory: raw.max_memory,
        max_output: raw.max_output,
        max_copy_size: raw.max_copy_size,
        max_workspace_size: raw.max_workspace_size,
        min_free_space: raw.min_free_space,
        max_processes: raw.max_processes,
        allow_best_effort_memory: raw.allow_best_effort_memory,
        format,
        metrics,
        session,
        resume,
        test_argv,
    })
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
pub fn parse_config_from<I, T>(args: I) -> Result<RunConfig, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let ParsedCommand::Run(args) = parse_from(args)? else {
        return Err(CliError::ProgressCommand);
    };
    run_config_from_args(args)
}

pub(crate) fn run_config_from_args(args: RunArgs) -> Result<RunConfig, CliError> {
    RunConfig::try_from(raw_config(args)?).map_err(CliError::from)
}

fn raw_config(args: RunArgs) -> Result<RawRunConfig, CliError> {
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
    let session = args
        .session
        .map(|path| utf8_path(path, "--session").map(|path| SessionConfig { path }))
        .transpose()?;
    let metrics = args.metrics.map(metrics_path).transpose()?;
    let limits = RawRunLimits {
        jobs: args.jobs,
        max_mutants: args.max_mutants,
        max_candidates: args.max_candidates,
        analyzer_timeout: parse_duration(&args.analyzer_timeout, "--analyzer-timeout")?,
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
        fingerprint_includes: args.fingerprint_includes,
        fingerprint_files: args.fingerprint_files,
        operators: args.operators,
        exclude_operators: args.exclude_operators,
        allow_best_effort_memory: args.allow_best_effort_memory,
        profile: args.profile.into(),
        limits,
        test_argv: args.test_argv.iter().map(command_arg).collect(),
        output: OutputConfig {
            format: match args.format {
                OutputFormat::Json => hoimin_core::OutputFormat::Json,
                OutputFormat::Jsonl => hoimin_core::OutputFormat::Jsonl,
                OutputFormat::Human => hoimin_core::OutputFormat::Human,
            },
            metrics,
        },
        session,
        resume: args.resume,
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
